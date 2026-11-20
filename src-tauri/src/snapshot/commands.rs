//! What each `Command` does. `snapshot::dispatch` is the IPC entry point.

use super::{
    calls, desk, eyebreak, hud, hud_drag, lights_out, maintenance, seated, stasis, walkaway,
    weekly, Command, HudConfig, HudWidget,
};
use crate::bluetooth::{self, ORCHESTRATOR};
use crate::calibration;
use crate::engine::OrchestratorState;
use crate::programs;
use crate::strava;
use crate::util::blocking;
use std::time::Instant;
use tauri::AppHandle;

/// Below this the engine treats a speed as "pause".
const PAUSE_BELOW_KMH: f32 = 0.05;

/// Manual speed: below the engine's pause threshold means "pause" (0),
/// anything else is held inside [calibrated min, calibrated max]. The engine
/// would bump a small positive value up to the floor anyway; doing it here
/// keeps the stored intent honest.
pub(super) fn clamp_manual_speed(state: &OrchestratorState, kmh: f32) -> Result<f32, String> {
    if !kmh.is_finite() {
        return Err("Speed must be a finite number".to_string());
    }
    if kmh < PAUSE_BELOW_KMH {
        return Ok(0.0);
    }
    let max = state.to_physical_speed(state.top_hw_speed());
    // Snapped to a speed the deck runs (DECK_PROFILE), then kept in bounds.
    Ok(state
        .snap_speed(kmh.min(max).max(state.min_physical_speed()))
        .min(max))
}

pub(super) fn clamp_incline_level(state: &OrchestratorState, level: u8) -> u8 {
    level.min(state.max_incline)
}

/// A button press, handled by the same code as the hotkeys.
async fn press(action: &str) -> Result<(), String> {
    bluetooth::trigger_action(action.to_string()).await
}

/// Save the program list after `change` edits it.
async fn change_programs(
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
    match command {
        Command::Start => press("start").await,
        Command::Pause => press("pause").await,
        Command::Stop => {
            calibration::on_stop();
            press("stop").await
        }
        Command::ToggleStart => press("toggle-start").await,
        Command::NudgeSpeed { up } => press(if up { "speed-up" } else { "speed-down" }).await,
        Command::NudgeIncline { up } => press(if up { "incline-up" } else { "incline-down" }).await,
        Command::SetSpeed { kmh } => {
            let mut state = ORCHESTRATOR.lock().await;
            let speed = clamp_manual_speed(&state, kmh)?;
            bluetooth::apply_manual_intent(&mut state, Some(speed), None);
            Ok(())
        }
        Command::SetInclineLevel { level } => {
            let mut state = ORCHESTRATOR.lock().await;
            let level = clamp_incline_level(&state, level);
            bluetooth::apply_manual_intent(&mut state, None, Some(level));
            Ok(())
        }

        Command::ProgramSave { program } => {
            change_programs(&app, |state| state.save_program(program)).await
        }
        Command::ProgramDelete { id } => {
            change_programs(&app, |state| state.delete_program(id, Instant::now())).await?;
            programs::persist_last_run(&app, None)
        }
        Command::ProgramRun { id } => {
            ORCHESTRATOR.lock().await.run_program(id, Instant::now())?;
            programs::persist_last_run(&app, Some(id))
        }
        Command::ProgramPause => ORCHESTRATOR.lock().await.pause_program(Instant::now()),
        Command::ProgramResume => ORCHESTRATOR.lock().await.resume_program(Instant::now()),
        Command::ProgramSkip => ORCHESTRATOR.lock().await.skip_program(Instant::now()),
        Command::ProgramEnd => {
            ORCHESTRATOR.lock().await.end_program(true, Instant::now());
            Ok(())
        }

        Command::HudToggleWidget { widget } => {
            set_hud(&app, |cfg| toggle_widget(&mut cfg.widgets, widget))
        }
        Command::HudSetWidgets { widgets } => {
            set_hud(&app, |cfg| cfg.widgets = hud::dedup_widgets(widgets))
        }
        Command::HudSetMonitor { monitor } => {
            let (monitor, rect) = hud::monitor_choice(monitor);
            set_hud(&app, |cfg| {
                cfg.monitor = monitor;
                cfg.monitor_rect = rect;
            })
        }
        Command::HudSetAnchor { anchor } => set_hud(&app, |cfg| {
            cfg.anchor = anchor;
            cfg.custom = None;
        }),
        Command::HudDragStart { grab_x, grab_y } => hud_drag::start(&app, grab_x, grab_y),
        Command::HudResetPosition => set_hud(&app, |cfg| cfg.custom = None),
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

        Command::ConnectNow => crate::link::connect_now(&app),
        Command::OpenConnectDialog => {
            crate::link::open_connect_dialog(&app);
            Ok(())
        }

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
        Command::WeeklySummaryAck => blocking(move || weekly::acknowledge(&app)).await,

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
        Command::CallsSetConfig { enabled, quiet_kmh } => {
            blocking(move || calls::set_config(&app, enabled, quiet_kmh)).await
        }

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
        Command::LightsOutSetConfig { enabled } => {
            blocking(move || lights_out::set_enabled(&app, enabled)).await
        }

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

        Command::TrafficRecord { on } => {
            blocking(move || crate::traffic::set_recording(&app, on)).await
        }
        Command::TrafficOpenFolder => crate::traffic::open_folder(&app),
        Command::DeckProfileForget => {
            ORCHESTRATOR.lock().await.forget_deck_profile();
            Ok(())
        }

        Command::CalibrationSetWizard { on } => {
            blocking(move || calibration::set_wizard(&app, on)).await
        }
        Command::CalibrationRunStart { target_kmh } => {
            if !super::emitter::read_connection().await.0 {
                return Err("Connect the treadmill first.".to_string());
            }
            calibration::run_start(&mut *ORCHESTRATOR.lock().await, target_kmh)
        }
        Command::CalibrationCountStart => {
            let raw = ORCHESTRATOR.lock().await.hardware_speed;
            calibration::count_start(raw)
        }
        Command::CalibrationCountStop => {
            let raw = ORCHESTRATOR.lock().await.hardware_speed;
            calibration::count_stop(raw)
        }
        Command::CalibrationRunCount { count } => {
            calibration::count(&*ORCHESTRATOR.lock().await, &count)
        }
        Command::CalibrationRunClose => {
            calibration::close();
            Ok(())
        }
        Command::CalibrationSweepStart { targets_kmh } => {
            if !super::emitter::read_connection().await.0 {
                return Err("Connect the treadmill first.".to_string());
            }
            calibration::sweep_start(&mut *ORCHESTRATOR.lock().await, &targets_kmh)
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
        Command::GhostPodSet { on } => {
            blocking(move || crate::ghost_pod::set_enabled(&app, on)).await
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
