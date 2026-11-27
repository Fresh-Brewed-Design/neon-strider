// The window side of the IPC contract: the generated types, the snapshot
// query and event, and `dispatch` for every command.

import { invoke } from "@tauri-apps/api/core";
import { listen, type EventTarget, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import type { AppSnapshot } from "./bindings/AppSnapshot";
import type { COLD_KEYS } from "./bindings/SNAPSHOT_PARTS";
import type { Command } from "./bindings/Command";

// One line per generated type, sorted by name: add a new one in its place.
export type { ActiveProgram } from "./bindings/ActiveProgram";
export type { AppConfig } from "./bindings/AppConfig";
export type { AppSnapshot } from "./bindings/AppSnapshot";
export type { BlinkStep } from "./bindings/BlinkStep";
export type { CalibrationGuideView } from "./bindings/CalibrationGuideView";
export type { CalibrationLimits } from "./bindings/CalibrationLimits";
export type { CalibrationPhase } from "./bindings/CalibrationPhase";
export type { CalibrationPoint } from "./bindings/CalibrationPoint";
export type { CalibrationResult } from "./bindings/CalibrationResult";
export type { CalibrationRunView } from "./bindings/CalibrationRunView";
export type { CalibrationSnapshot } from "./bindings/CalibrationSnapshot";
export type { CalibrationSweepView } from "./bindings/CalibrationSweepView";
export type { CallsSnapshot } from "./bindings/CallsSnapshot";
export type { CheckVerdict } from "./bindings/CheckVerdict";
export type { Change } from "./bindings/Change";
export type { Command } from "./bindings/Command";
export type { ConnectionSnapshot } from "./bindings/ConnectionSnapshot";
export type { DeckProfileSnapshot } from "./bindings/DeckProfileSnapshot";
export type { DeckSnapshot } from "./bindings/DeckSnapshot";
export type { DeckSpeedStep } from "./bindings/DeckSpeedStep";
export type { DeskDay } from "./bindings/DeskDay";
export type { DeskSync } from "./bindings/DeskSync";
export type { EchoProgram } from "./bindings/EchoProgram";
export type { WallClock } from "./bindings/WallClock";
export type { EyeBreakSnapshot } from "./bindings/EyeBreakSnapshot";
export type { FitToDeckOffer } from "./bindings/FitToDeckOffer";
export type { FitToTimeOption } from "./bindings/FitToTimeOption";
export type { GuideCheck } from "./bindings/GuideCheck";
export type { GuideMark } from "./bindings/GuideMark";
export type { GuideSpeed } from "./bindings/GuideSpeed";
export type { GuidedTask } from "./bindings/GuidedTask";
export type { GuidedTaskSnapshot } from "./bindings/GuidedTaskSnapshot";
export type { DayMeters } from "./bindings/DayMeters";
export type { HistoryStats } from "./bindings/HistoryStats";
export type { HistoryTotals } from "./bindings/HistoryTotals";
export type { HeartZone } from "./bindings/HeartZone";
export type { HudAnchor } from "./bindings/HudAnchor";
export type { HudCard } from "./bindings/HudCard";
export type { HudConfig } from "./bindings/HudConfig";
export type { HudCustom } from "./bindings/HudCustom";
export type { HudIdleMode } from "./bindings/HudIdleMode";
export type { HudMonitor } from "./bindings/HudMonitor";
export type { HudView } from "./bindings/HudView";
export type { HudWidget } from "./bindings/HudWidget";
export type { KeyCombo } from "./bindings/KeyCombo";
export type { LastError } from "./bindings/LastError";
export type { LinkError } from "./bindings/LinkError";
export type { LinkErrorKind } from "./bindings/LinkErrorKind";
export type { LinkState } from "./bindings/LinkState";
export type { MachineStatus } from "./bindings/MachineStatus";
export type { MaintenanceBasis } from "./bindings/MaintenanceBasis";
export type { MaintenanceSnapshot } from "./bindings/MaintenanceSnapshot";
export type { MuteMode } from "./bindings/MuteMode";
export type { NeuralSensitivity } from "./bindings/NeuralSensitivity";
export type { NeuralSnapshot } from "./bindings/NeuralSnapshot";
export type { OdometerSnapshot } from "./bindings/OdometerSnapshot";
export type { Os } from "./bindings/Os";
export type { Permission } from "./bindings/Permission";
export type { PermissionKind } from "./bindings/PermissionKind";
export type { PermissionState } from "./bindings/PermissionState";
export type { PlatformSnapshot } from "./bindings/PlatformSnapshot";
export type { Program } from "./bindings/Program";
export type { ProgramCue } from "./bindings/ProgramCue";
export type { ProgramPhase } from "./bindings/ProgramPhase";
export type { ProgramStep } from "./bindings/ProgramStep";
export type { ProgramTonesSnapshot } from "./bindings/ProgramTonesSnapshot";
export type { ProgramWait } from "./bindings/ProgramWait";
export type { PulseEnd } from "./bindings/PulseEnd";
export type { PulseTarget } from "./bindings/PulseTarget";
export type { SavedFile } from "./bindings/SavedFile";
export type { SeatedSnapshot } from "./bindings/SeatedSnapshot";
export type { SessionSnapshot } from "./bindings/SessionSnapshot";
export type { SettingLimits } from "./bindings/SettingLimits";
export type { SettingRange } from "./bindings/SettingRange";
export type { Shortcut } from "./bindings/Shortcut";
export type { ShortcutKey } from "./bindings/ShortcutKey";
export type { ShortcutsSnapshot } from "./bindings/ShortcutsSnapshot";
export type { ShortcutView } from "./bindings/ShortcutView";
export type { SpeedDriftSnapshot } from "./bindings/SpeedDriftSnapshot";
export type { SpeedDriftState } from "./bindings/SpeedDriftState";
export type { StartPress } from "./bindings/StartPress";
export type { StartResult } from "./bindings/StartResult";
export type { StepKind } from "./bindings/StepKind";
export type { StepKindSpot } from "./bindings/StepKindSpot";
export type { StepRunView } from "./bindings/StepRunView";
export type { StravaActivity } from "./bindings/StravaActivity";
export type { StravaGear } from "./bindings/StravaGear";
export type { StravaHint } from "./bindings/StravaHint";
export type { StravaResult } from "./bindings/StravaResult";
export type { StravaSnapshot } from "./bindings/StravaSnapshot";
export type { StravaToday } from "./bindings/StravaToday";
export type { StravaStep } from "./bindings/StravaStep";
export type { SweepMark } from "./bindings/SweepMark";
export type { TemplateField } from "./bindings/TemplateField";
export type { TimelineMark } from "./bindings/TimelineMark";
export type { TimelineRun } from "./bindings/TimelineRun";
export type { TodayTimeline } from "./bindings/TodayTimeline";
export type { TrafficSnapshot } from "./bindings/TrafficSnapshot";
export type { UpdateOffer } from "./bindings/UpdateOffer";
export type { UpdateResult } from "./bindings/UpdateResult";
export type { UpdateSnapshot } from "./bindings/UpdateSnapshot";
export type { UpdateStatus } from "./bindings/UpdateStatus";
export type { WalkAwayCard } from "./bindings/WalkAwayCard";
export type { WalkAwayReason } from "./bindings/WalkAwayReason";
export type { WalkAwaySnapshot } from "./bindings/WalkAwaySnapshot";
export type { WeeklySummary } from "./bindings/WeeklySummary";
export type { WeekStats } from "./bindings/WeekStats";
export type { WhatsNew } from "./bindings/WhatsNew";
export type { ZoneBand } from "./bindings/ZoneBand";
export type { ZonePosition } from "./bindings/ZonePosition";
export type { ZoneTrim } from "./bindings/ZoneTrim";
export type { UnlistenFn };

/** Every settings range and cap, written from Rust's constants by `cargo test`. */
export { SETTING_LIMITS } from "./bindings/SETTING_LIMITS";
/** `AppConfig::default()`, written by `cargo test`: the settings shown until Rust's saved ones load. */
export { DEFAULT_APP_CONFIG } from "./bindings/DEFAULT_APP_CONFIG";
/** Every Strava template field with what it shows and its sample text, written by `cargo test`. */
export { TEMPLATE_FIELDS } from "./bindings/TEMPLATE_FIELDS";
/** The snapshot fields an `app-state` frame carries only when `full`, written by `cargo test`. */
export { COLD_KEYS } from "./bindings/SNAPSHOT_PARTS";

/** A snapshot field that changes rarely (programs, Strava, HUD config, ...). */
export type ColdKey = (typeof COLD_KEYS)[number];

/**
 * One `app-state` event (`snapshot/frame.rs`): the whole snapshot when
 * `full`, otherwise every field but the cold ones, which the window keeps.
 */
export type AppStateFrame = ({ full: true } & AppSnapshot) | ({ full: false } & Omit<AppSnapshot, ColdKey>);

export function getSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_snapshot");
}

export function dispatch(command: Command): Promise<void> {
  return invoke<void>("dispatch", { command });
}

/**
 * This window as an event target: Rust sends each window its own frames and
 * none to a hidden one. A page with no Tauri window (the dev previews) hears
 * every frame.
 */
function thisWindow(): EventTarget | undefined {
  try {
    return { kind: "WebviewWindow", label: getCurrentWindow().label };
  } catch {
    return undefined;
  }
}

export function onAppState(cb: (frame: AppStateFrame) => void): Promise<UnlistenFn> {
  const target = thisWindow();
  return listen<AppStateFrame>("app-state", (event) => cb(event.payload), target ? { target } : undefined);
}
