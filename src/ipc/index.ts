// The window side of the IPC contract: the generated types, the snapshot
// query and event, and `dispatch` for every command.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSnapshot } from "./bindings/AppSnapshot";
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
export type { EyeBreakSnapshot } from "./bindings/EyeBreakSnapshot";
export type { GuideCheck } from "./bindings/GuideCheck";
export type { GuideMark } from "./bindings/GuideMark";
export type { GuideSpeed } from "./bindings/GuideSpeed";
export type { GuidedTask } from "./bindings/GuidedTask";
export type { GuidedTaskSnapshot } from "./bindings/GuidedTaskSnapshot";
export type { HudAnchor } from "./bindings/HudAnchor";
export type { HudConfig } from "./bindings/HudConfig";
export type { HudCustom } from "./bindings/HudCustom";
export type { HudIdleMode } from "./bindings/HudIdleMode";
export type { HudMinifyHotkey } from "./bindings/HudMinifyHotkey";
export type { HudMinifyKey } from "./bindings/HudMinifyKey";
export type { HudMonitor } from "./bindings/HudMonitor";
export type { HudView } from "./bindings/HudView";
export type { HudWidget } from "./bindings/HudWidget";
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
export type { Program } from "./bindings/Program";
export type { ProgramPhase } from "./bindings/ProgramPhase";
export type { ProgramStep } from "./bindings/ProgramStep";
export type { ProgramWait } from "./bindings/ProgramWait";
export type { SavedFile } from "./bindings/SavedFile";
export type { SeatedSnapshot } from "./bindings/SeatedSnapshot";
export type { SessionSnapshot } from "./bindings/SessionSnapshot";
export type { SettingLimits } from "./bindings/SettingLimits";
export type { SettingRange } from "./bindings/SettingRange";
export type { StartResult } from "./bindings/StartResult";
export type { StepRunView } from "./bindings/StepRunView";
export type { StravaActivity } from "./bindings/StravaActivity";
export type { StravaGear } from "./bindings/StravaGear";
export type { StravaHint } from "./bindings/StravaHint";
export type { StravaResult } from "./bindings/StravaResult";
export type { StravaSnapshot } from "./bindings/StravaSnapshot";
export type { StravaStep } from "./bindings/StravaStep";
export type { SweepMark } from "./bindings/SweepMark";
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

export function getSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_snapshot");
}

export function dispatch(command: Command): Promise<void> {
  return invoke<void>("dispatch", { command });
}

export function onAppState(cb: (snapshot: AppSnapshot) => void): Promise<UnlistenFn> {
  return listen<AppSnapshot>("app-state", (event) => cb(event.payload));
}
