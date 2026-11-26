//! What each `Command` does. `snapshot::dispatch` is the IPC entry point.

use super::{
    calls, desk, eyebreak, hud, hud_drag, lights_out, maintenance, presenting, seated, stasis,
    walkaway, weekly, Command, HudConfig, HudWidget,
};
use crate::bluetooth::{self, ORCHESTRATOR};
use crate::calibration;
use crate::engine::OrchestratorState;
use crate::programs;
use crate::strava;
use crate::util::blocking;
use std::time::Instant;
use tauri::AppHandle;

/// Below this the engine treats a speed as "pause" on a still belt.
const PAUSE_BELOW_KMH: f32 = crate::engine::STILL_KMH;

/// Manual speed, held inside [calibrated min, calibrated max]. Below the
/// engine's pause threshold on a still belt means "stay still" (0). On a
/// moving belt it is held at the floor: a speed control never takes the belt
/// to 0, only Pause or Stop do (#177). The engine would bump a small positive
/// value up to the floor anyway; doing it here keeps the stored intent honest.
pub(crate) fn clamp_manual_speed(state: &OrchestratorState, kmh: f32) -> Result<f32, String> {
    if !kmh.is_finite() {
        return Err("Speed must be a finite number".to_string());
    }
    if kmh < PAUSE_BELOW_KMH {
        if !state.last_status.is_moving() {
            return Ok(0.0);
        }
        log::info!(
            "Speed {kmh:.2} km/h from set_speed (Live page slider or preset) on a moving belt: held at the floor {:.2} km/h, only Pause or Stop take the belt to 0",
            state.min_physical_speed()
        );
    }
    let max = state.to_physical_speed(state.top_hw_speed());
    // Snapped to a speed the deck runs (DECK_PROFILE), then kept in bounds.
    Ok(state
        .snap_speed(kmh.min(max).max(state.min_physical_speed()))
        .min(max))
}

/// `Command::SetSpeed`: a speed set by hand, clamped (`clamp_manual_speed`).
/// The sim's scenarios set speeds through here too.
pub(crate) fn set_speed(state: &mut OrchestratorState, kmh: f32) -> Result<(), String> {
    let speed = clamp_manual_speed(state, kmh)?;
    log::info!("Speed set by hand: {speed:.2} km/h (asked {kmh:.2})");
    bluetooth::apply_manual_intent(state, Some(speed), None);
    Ok(())
}

pub(super) fn clamp_incline_level(state: &OrchestratorState, level: u8) -> u8 {
    // Snapped to a level the deck runs (DECK_PROFILE), then kept in bounds.
    state.snap_incline_level(level.min(state.max_incline))
}

/// A button press, handled by the same code as the hotkeys.
async fn press(action: &str) -> Result<(), String> {
    bluetooth::trigger_action(action.to_string()).await
}

/// Save the program list after `change` edits it.
pub(crate) async fn change_programs(
    app: &AppHandle,
    change: impl FnOnce(&mut OrchestratorState) -> Result<(), String>,
) -> Result<(), String> {
    let (list, next_id) = {
        let mut state = ORCHESTRATOR.lock().await;
        change(&mut state)?;
        (state.programs.clone(), state.program_next_id)
    };
    programs::persist(app, &list, next_id)
}

/// Change the HUD config (see `hud::mutate`); it cannot fail.
fn set_hud(app: &AppHandle, f: impl FnOnce(&mut HudConfig)) -> Result<(), String> {
    hud::mutate(app, f);
    Ok(())
}

fn toggle_widget(widgets: &mut Vec<HudWidget>, widget: HudWidget) {
    if let Some(i) = widgets.iter().position(|w| *w == widget) {
        widgets.remove(i);
    } else {
        widgets.push(widget);
    }
}

pub(super) async fn run(app: AppHandle, command: Command) -> Result<(), String> {
    // Grouped by feature like the `Command` enum in `types.rs`.
    match command {
        // ---- Belt: start, stop, speed and incline ----
        Command::Start => press("start").await,
        Command::Pause => press("pause").await,
        Command::Stop => {
            calibration::on_stop();
            ORCHESTRATOR.lock().await.profile_sweep_stop();
            press("stop").await
        }
        Command::ToggleStart => press("toggle-start").await,
        Command::NudgeSpeed { up } => press(if up { "speed-up" } else { "speed-down" }).await,
        Command::NudgeIncline { up } => press(if up { "incline-up" } else { "incline-down" }).await,
        Command::SetSpeed { kmh } => set_speed(&mut *ORCHESTRATOR.lock().await, kmh),
        Command::SetInclineLevel { level } => {
            let mut state = ORCHESTRATOR.lock().await;
            let level = clamp_incline_level(&state, level);
            bluetooth::apply_manual_intent(&mut state, None, Some(level));
            Ok(())
        }

        // ---- Treadmill link ----
        Command::ConnectNow => crate::link::connect_now(&app),
        Command::OpenConnectDialog => {
            crate::link::open_connect_dialog(&app);
            Ok(())
        }

        // ---- Programs ----
        Command::ProgramSave { program } => {
            change_programs(&app, |state| state.save_program(program)).await
        }
        Command::ProgramDelete { id } => {
            change_programs(&app, |state| state.delete_program(id, Instant::now())).await?;
            programs::persist_last_run(&app, None)
        }
        Command::ProgramRun { id, minutes } => {
            ORCHESTRATOR
                .lock()
                .await
                .run_program_fitted(id, minutes, Instant::now())?;
            programs::persist_last_run(&app, Some(id))
        }
        Command::ProgramPause => ORCHESTRATOR.lock().await.pause_program(Instant::now()),
        Command::ProgramResume => ORCHESTRATOR.lock().await.resume_program(Instant::now()),
        Command::ProgramSkip => ORCHESTRATOR.lock().await.skip_program(Instant::now()),
        Command::ProgramEnd => {
            ORCHESTRATOR.lock().await.end_program(true, Instant::now());
            Ok(())
        }
        Command::ProgramFitToDeck { id } => {
            change_programs(&app, |state| {
                let fit = crate::calibration::stride_fit::kmh(
                    state.min_physical_speed(),
                    state.to_physical_speed(state.top_hw_speed()),
                );
                state.fit_program_to_deck(id, fit)
            })
            .await
        }
        Command::ProgramTonesSet { on } => {
            blocking(move || super::program_tones::set_enabled(&app, on)).await
        }

        // ---- HUD and dashboard windows ----
        Command::HudToggleWidget { widget } => {
            set_hud(&app, |cfg| toggle_widget(&mut cfg.widgets, widget))
        }
        Command::HudSetWidgets { widgets } => {
            set_hud(&app, |cfg| cfg.widgets = hud::dedup_widgets(widgets))
        }
        Command::HudSetAnchor { anchor } => set_hud(&app, |cfg| {
            cfg.anchor = anchor;
            cfg.custom = None;
        }),
        Command::HudDragStart { grab_x, grab_y } => hud_drag::start(&app, grab_x, grab_y),
        Command::HudResetPosition => set_hud(&app, |cfg| cfg.custom = None),
        Command::HudSetMonitor { monitor } => {
            let (monitor, rect) = hud::monitor_choice(monitor);
            set_hud(&app, |cfg| {
                cfg.monitor = monitor;
                cfg.monitor_rect = rect;
            })
        }
        Command::DashboardSetPinned { pinned } => {
            set_hud(&app, |cfg| cfg.dashboard_pinned = pinned)
        }
        Command::HudSetIdleMode { mode } => set_hud(&app, |cfg| cfg.idle_mode = mode),
        Command::HudExpand => {
            // Expand from either strip ends a minify too: back to the normal rules.
            hud::mutate(&app, |cfg| cfg.minified = false);
            stasis::expand();
            Ok(())
        }
        Command::HudSetMinified { minified } => set_hud(&app, |cfg| cfg.minified = minified),

        // ---- Strava ----
        Command::StravaSetCredentials {
            client_id,
            client_secret,
        } => strava::set_credentials(&app, client_id, client_secret),
        Command::StravaConnect => strava::connect(&app).await,
        Command::StravaDisconnect => strava::disconnect(&app),
        Command::StravaSetAutoSync { enabled } => strava::set_auto_sync(&app, enabled),
        Command::StravaSetDailyRollup { enabled, shift_end } => {
            strava::set_daily_rollup(&app, enabled, &shift_end)
        }
        Command::StravaUpload { session_ids } => strava::upload_manual(&app, session_ids),
        Command::StravaSyncNow => strava::sync_now(&app),
        Command::StravaRetryNow => strava::retry_now(&app),
        Command::StravaCancelConnect => strava::cancel_connect(),
        Command::StravaOpenApiSettings => strava::open_api_settings(&app),
        Command::StravaMarkUploaded { session_ids } => strava::mark_on_strava(&app, session_ids),
        Command::StravaOpenSaveFolder => strava::open_save_folder(&app),
        Command::StravaSetGear { gear_id } => strava::set_gear(&app, gear_id),
        Command::StravaRefreshGear => strava::refresh_gear_command(&app),
        Command::StravaSetMute {
            mode,
            under_minutes,
        } => strava::set_mute(&app, mode, under_minutes),
        Command::StravaSetTemplates {
            title,
            rollup_title,
            description,
        } => strava::set_templates(&app, title, rollup_title, description),

        // ---- Belt reminder (maintenance) ----
        Command::MaintenanceMarkServiced => {
            blocking(move || maintenance::mark_serviced(&app)).await
        }
        Command::MaintenanceSetLastService { at_ms } => {
            blocking(move || maintenance::set_last_service(&app, at_ms)).await
        }
        Command::MaintenanceSetIntervals { km, days } => {
            blocking(move || maintenance::set_intervals(&app, km, days)).await
        }
        Command::MaintenanceSetBasis { basis, hours } => {
            blocking(move || maintenance::set_basis(&app, basis, hours)).await
        }

        // ---- Weekly summary ----
        Command::WeeklySummaryAck => blocking(move || weekly::acknowledge(&app)).await,

        // ---- Idle nudge (ATROPHY_WARNING) ----
        Command::SeatedSetConfig {
            enabled,
            minutes,
            program_ids,
        } => blocking(move || seated::set_config(&app, enabled, minutes, program_ids)).await,
        Command::SeatedEngage => desk::press_engage().await,
        Command::SeatedStart { id } => desk::press_program(&app, id).await,
        Command::SeatedSnooze => {
            desk::snooze();
            Ok(())
        }

        // ---- Eye breaks (OCULAR_IMPLANT) ----
        Command::EyeBreakSetConfig {
            enabled,
            minutes,
            seconds,
            sound,
            pause_belt,
            fit_rests,
        } => {
            let cfg = eyebreak::Config {
                enabled,
                minutes,
                seconds,
                sound,
                pause_belt,
                fit_rests,
            };
            blocking(move || eyebreak::set_config(&app, cfg)).await
        }
        Command::EyeBreakSkip => {
            eyebreak::with(|rt| rt.machine.skip());
            Ok(())
        }
        Command::EyeBreakResume => desk::eye_break_resume().await,
        Command::EyeBreakStayStill => {
            desk::eye_break_stay_still();
            Ok(())
        }

        // ---- Speed drift ----
        Command::SpeedDriftSetConfig {
            enabled,
            low_kmh,
            high_kmh,
            every_minutes,
        } => {
            blocking(move || {
                crate::speed_drift::set_config(&app, enabled, low_kmh, high_kmh, every_minutes)
            })
            .await
        }

        // ---- Slow down on calls ----
        Command::CallsSetConfig { enabled, quiet_kmh } => {
            blocking(move || calls::set_config(&app, enabled, quiet_kmh)).await
        }
        Command::QuietNow => desk::press_quiet_now().await,

        // ---- Walk-away safety ----
        Command::WalkawaySetConfig {
            lock_enabled,
            idle_enabled,
            idle_minutes,
        } => {
            blocking(move || walkaway::set_config(&app, lock_enabled, idle_enabled, idle_minutes))
                .await
        }
        Command::WalkawayStay => {
            desk::walkaway_stay();
            Ok(())
        }
        Command::WalkawayResume => desk::walkaway_resume().await,
        Command::WalkawayEnd => desk::walkaway_end().await,

        // ---- LIGHTS_OUT ----
        Command::LightsOutSetConfig { enabled } => {
            blocking(move || lights_out::set_enabled(&app, enabled)).await
        }

        // ---- Hold cards while presenting ----
        Command::PresentingSetConfig { enabled } => {
            blocking(move || presenting::set_enabled(&app, enabled)).await
        }

        // ---- Auto-update ----
        Command::UpdateCheck => {
            crate::update::check(&app, true).await;
            Ok(())
        }
        Command::UpdateInstall => crate::update::install(&app),
        Command::UpdateDismiss => blocking(move || crate::update::dismiss(&app)).await,
        Command::UpdateHudCard { open } => {
            blocking(move || crate::update::hud_card(&app, open)).await
        }
        Command::UpdateWhatsNewAck => blocking(move || crate::update::whats_new_ack(&app)).await,
        Command::UpdateAutoInstall { on } => {
            blocking(move || crate::update::set_auto_install(&app, on)).await
        }

        // ---- Diagnostics: traffic recorder ----
        Command::TrafficRecord { on } => {
            blocking(move || crate::traffic::set_recording(&app, on)).await
        }
        Command::TrafficOpenFolder => crate::traffic::open_folder(&app),

        // ---- DECK_PROFILE and DECK_SWEEP ----
        Command::DeckProfileForget => {
            ORCHESTRATOR.lock().await.forget_deck_profile();
            Ok(())
        }
        Command::DeckProfileSweepStart => ORCHESTRATOR.lock().await.profile_sweep_start(),
        Command::DeckProfileSweepInclineStart => {
            ORCHESTRATOR.lock().await.profile_sweep_incline_start()
        }
        Command::DeckProfileSweepStop => {
            ORCHESTRATOR.lock().await.profile_sweep_stop();
            Ok(())
        }
        Command::DeckProfileSweepSet { on } => crate::profile_sweep::set_enabled(&app, on).await,

        // ---- Easy start ----
        Command::EasyStartSet { on } => crate::easy_start::set_enabled(&app, on).await,
        Command::EasyStopSet { on } => crate::easy_stop::set_enabled(&app, on).await,

        // ---- Guided calibration (GROUND_TRUTH) ----
        Command::CalibrationRunStart { target_kmh } => {
            if !super::emitter::read_connection().await.0 {
                return Err("Connect the treadmill first.".to_string());
            }
            calibration::run_start(&mut *ORCHESTRATOR.lock().await, target_kmh)
        }
        Command::CalibrationTap => {
            // The moment of the press, before waiting on the lock.
            let at = std::time::Instant::now();
            let saved = calibration::tap(&mut *ORCHESTRATOR.lock().await, at)?;
            if let Some(points) = saved {
                blocking(move || {
                    calibration::persist_points(&app, &points);
                    Ok(())
                })
                .await?;
            }
            Ok(())
        }
        Command::CalibrationTapReset => {
            let restored = calibration::retap(&mut *ORCHESTRATOR.lock().await)?;
            if let Some(points) = restored {
                blocking(move || {
                    calibration::persist_points(&app, &points);
                    Ok(())
                })
                .await?;
            }
            Ok(())
        }
        Command::CalibrationUndo => {
            let points = calibration::undo(&mut *ORCHESTRATOR.lock().await)?;
            blocking(move || {
                calibration::persist_points(&app, &points);
                Ok(())
            })
            .await
        }
        Command::CalibrationBeltSet { cm } => {
            blocking(move || calibration::set_belt(&app, cm)).await
        }
        Command::CalibrationRunClose => {
            calibration::close();
            Ok(())
        }
        Command::CalibrationTargetAdd { kmh } => {
            let (min, max) = {
                let state = ORCHESTRATOR.lock().await;
                (state.min_speed, state.max_speed)
            };
            blocking(move || calibration::add_target(&app, kmh, min, max)).await
        }
        Command::CalibrationTargetRemove { kmh } => {
            let points: Vec<f32> = ORCHESTRATOR
                .lock()
                .await
                .calibration_points
                .iter()
                .map(|p| p.speed)
                .collect();
            blocking(move || calibration::remove_target(&app, kmh, &points)).await
        }
        Command::CalibrationSweepStart => {
            if !super::emitter::read_connection().await.0 {
                return Err("Connect the treadmill first.".to_string());
            }
            calibration::sweep_start(&mut *ORCHESTRATOR.lock().await)
        }
        Command::CalibrationSweepRun => {
            if !super::emitter::read_connection().await.0 {
                return Err("Connect the treadmill first.".to_string());
            }
            calibration::sweep_run(&mut *ORCHESTRATOR.lock().await)
        }
        Command::CalibrationSweepNext { saved } => calibration::sweep_next(saved),
        Command::CalibrationSweepEnd => {
            calibration::close_all();
            Ok(())
        }
        Command::CalibrationStepsStart => {
            let restored = calibration::steps_start(&mut *ORCHESTRATOR.lock().await)?;
            if let Some(points) = restored {
                blocking(move || {
                    calibration::persist_points(&app, &points);
                    Ok(())
                })
                .await?;
            }
            Ok(())
        }
        Command::CalibrationStepsSkip => calibration::steps_skip(),
        Command::CalibrationOfferSkip => blocking(move || calibration::skip_offer(&app)).await,
        Command::CalibrationGuideStart { top_kmh } => {
            calibration::guide_start(&*ORCHESTRATOR.lock().await, top_kmh)
        }
        Command::CalibrationGuideRun => {
            if !super::emitter::read_connection().await.0 {
                return Err("Connect the treadmill first.".to_string());
            }
            calibration::guide_run(&mut *ORCHESTRATOR.lock().await)
        }
        Command::CalibrationGuideSkip => calibration::guide_skip(),
        Command::CalibrationGuideEnd => {
            calibration::close_all();
            Ok(())
        }
        Command::CalibrationStartFresh => {
            let save = calibration::start_fresh(&mut *ORCHESTRATOR.lock().await)?;
            blocking(move || calibration::persist_fresh(&app, &save)).await
        }
        Command::CalibrationRestore => {
            let save = calibration::restore(&mut *ORCHESTRATOR.lock().await)?;
            blocking(move || calibration::persist_fresh(&app, &save)).await
        }
        Command::CalibrationKeepNew => {
            let save = calibration::keep_new()?;
            blocking(move || calibration::persist_fresh(&app, &save)).await
        }
        Command::CalibrationStepsUndo => {
            let points = calibration::steps_undo(&mut *ORCHESTRATOR.lock().await)?;
            blocking(move || {
                calibration::persist_points(&app, &points);
                Ok(())
            })
            .await
        }
        Command::StrideFitSet { kmh } => {
            let (min, max) = {
                let state = ORCHESTRATOR.lock().await;
                (
                    state.min_physical_speed(),
                    state.to_physical_speed(state.top_hw_speed()),
                )
            };
            blocking(move || calibration::stride_fit::set(&app, kmh, min, max).map(|_| ())).await
        }

        // ---- GHOST_POD (Labs) ----
        Command::GhostPodSet { on } => {
            blocking(move || crate::ghost_pod::set_enabled(&app, on)).await
        }
        Command::GhostPodPowerSet { on } => {
            blocking(move || crate::ghost_pod::set_power(&app, on)).await
        }

        // ---- Typing by speed (Labs) ----
        Command::TypingSpeedSet { on } => {
            blocking(move || crate::typing_speed::set_enabled(&app, on)).await
        }

        // ---- HEART_LINK ----
        Command::HeartRateScan => crate::heart_rate::scan().await,
        Command::HeartRatePick { id, name } => {
            blocking(move || crate::heart_rate::pick_sensor(&app, id, name)).await
        }
        Command::HeartRateForget => blocking(move || crate::heart_rate::forget_sensor(&app)).await,
        Command::HeartRateMaxSet { bpm } => {
            blocking(move || crate::heart_rate::set_max_heart_rate(&app, bpm)).await
        }
        Command::HeartRateInclineTrimSet { on } => {
            blocking(move || crate::heart_rate::set_zone_incline_trim(&app, on)).await
        }

        // ---- Guided tasks ----
        Command::GuidedTaskStart { task } => {
            if crate::guided_task::start(task) {
                crate::dashboard::on_guided_pin_changed(&app);
            }
            Ok(())
        }
        Command::GuidedTaskEnd { task } => {
            if crate::guided_task::end(task) {
                crate::dashboard::on_guided_pin_changed(&app);
            }
            Ok(())
        }
        Command::GuidedTaskCancel => {
            if crate::guided_task::cancel() {
                crate::dashboard::on_guided_pin_changed(&app);
            }
            Ok(())
        }
        Command::ShortcutsSet { shortcut, keys } => {
            blocking(move || crate::shortcuts::set(&app, shortcut, keys)).await
        }
        Command::ShortcutsReset => blocking(move || crate::shortcuts::reset(&app)).await,
        Command::ShortcutsRecording { on } => {
            crate::shortcuts::set_recording(on);
            Ok(())
        }
        Command::OpenPermissionSettings { permission } => {
            crate::platform::open_permission_settings(permission)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_speed_and_incline_clamping() {
        // min 0.6, max 12.0, no calibration
        let state = OrchestratorState {
            max_incline: 12,
            ..OrchestratorState::default()
        };

        assert_eq!(clamp_manual_speed(&state, 0.0).unwrap(), 0.0);
        assert_eq!(clamp_manual_speed(&state, 0.04).unwrap(), 0.0);
        assert_eq!(clamp_manual_speed(&state, -3.0).unwrap(), 0.0);
        assert_eq!(clamp_manual_speed(&state, 0.3).unwrap(), 0.6);
        assert_eq!(clamp_manual_speed(&state, 4.5).unwrap(), 4.5);
        assert_eq!(clamp_manual_speed(&state, 99.0).unwrap(), 12.0);
        assert!(clamp_manual_speed(&state, f32::NAN).is_err());

        // On a moving belt a speed control's lowest setting is the floor.
        let moving = OrchestratorState {
            last_status: crate::protocol::MachineStatus::Running,
            ..OrchestratorState::default()
        };
        assert_eq!(clamp_manual_speed(&moving, 0.0).unwrap(), 0.6);
        assert_eq!(clamp_manual_speed(&moving, -3.0).unwrap(), 0.6);
        assert_eq!(clamp_manual_speed(&moving, 0.04).unwrap(), 0.6);
        assert_eq!(clamp_manual_speed(&moving, 4.5).unwrap(), 4.5);

        assert_eq!(clamp_incline_level(&state, 5), 5);
        assert_eq!(clamp_incline_level(&state, 200), 12);
    }

    /// `min_speed` is hardware units: with a calibration the clamp holds a
    /// speed at the calibrated floor, which the deck gets as its floor byte.
    #[test]
    fn manual_speed_clamps_to_the_calibrated_floor() {
        let mut state = OrchestratorState::default();
        state.sync_config(&crate::engine::slow_floor_config());
        let floor = state.min_physical_speed();
        assert!((floor - 1.2).abs() < 1e-4, "{floor}");
        let kmh = clamp_manual_speed(&state, 0.8).unwrap();
        assert_eq!(kmh, floor);
        let byte = crate::engine::calculate_hardware_bytes(
            kmh,
            0,
            &state.calibration_points,
            state.max_speed,
        )
        .0;
        assert_eq!(byte, 6, "the deck's floor byte (0.6 hardware km/h)");
        assert_eq!(clamp_manual_speed(&state, 3.0).unwrap(), 3.0);
    }

    #[test]
    fn toggling_a_widget_adds_then_removes_it() {
        let mut widgets = vec![HudWidget::Timer];
        toggle_widget(&mut widgets, HudWidget::Steps);
        assert_eq!(widgets, vec![HudWidget::Timer, HudWidget::Steps]);
        toggle_widget(&mut widgets, HudWidget::Timer);
        assert_eq!(widgets, vec![HudWidget::Steps]);
    }
}
