// The window side of the IPC contract: the generated types, the snapshot
// query and event, and `dispatch` for every command.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AppSnapshot } from "./bindings/AppSnapshot";
import type { Command } from "./bindings/Command";

export type { AppSnapshot } from "./bindings/AppSnapshot";
export type { ActiveProgram } from "./bindings/ActiveProgram";
export type { Command } from "./bindings/Command";
export type { CallsSnapshot } from "./bindings/CallsSnapshot";
export type { Change } from "./bindings/Change";
export type { ConnectionSnapshot } from "./bindings/ConnectionSnapshot";
export type { DeckSnapshot } from "./bindings/DeckSnapshot";
export type { HudAnchor } from "./bindings/HudAnchor";
export type { HudConfig } from "./bindings/HudConfig";
export type { HudCustom } from "./bindings/HudCustom";
export type { HudMonitor } from "./bindings/HudMonitor";
export type { HudWidget } from "./bindings/HudWidget";
export type { LastError } from "./bindings/LastError";
export type { LinkError } from "./bindings/LinkError";
export type { LinkErrorKind } from "./bindings/LinkErrorKind";
export type { LinkState } from "./bindings/LinkState";
export type { MachineStatus } from "./bindings/MachineStatus";
export type { MuteMode } from "./bindings/MuteMode";
export type { MaintenanceBasis } from "./bindings/MaintenanceBasis";
export type { MaintenanceSnapshot } from "./bindings/MaintenanceSnapshot";
export type { NeuralSnapshot } from "./bindings/NeuralSnapshot";
export type { NeuralSensitivity } from "./bindings/NeuralSensitivity";
export type { OdometerSnapshot } from "./bindings/OdometerSnapshot";
export type { Program } from "./bindings/Program";
export type { ProgramPhase } from "./bindings/ProgramPhase";
export type { ProgramWait } from "./bindings/ProgramWait";
export type { ProgramStep } from "./bindings/ProgramStep";
export type { SeatedSnapshot } from "./bindings/SeatedSnapshot";
export type { SessionSnapshot } from "./bindings/SessionSnapshot";
export type { SavedFile } from "./bindings/SavedFile";
export type { StravaActivity } from "./bindings/StravaActivity";
export type { StravaGear } from "./bindings/StravaGear";
export type { StravaHint } from "./bindings/StravaHint";
export type { StravaResult } from "./bindings/StravaResult";
export type { StartResult } from "./bindings/StartResult";
export type { StravaStep } from "./bindings/StravaStep";
export type { StravaSnapshot } from "./bindings/StravaSnapshot";
export type { WalkAwayCard } from "./bindings/WalkAwayCard";
export type { WalkAwayReason } from "./bindings/WalkAwayReason";
export type { WalkAwaySnapshot } from "./bindings/WalkAwaySnapshot";
export type { WeeklySummary } from "./bindings/WeeklySummary";
export type { WeekStats } from "./bindings/WeekStats";
export type { UpdateOffer } from "./bindings/UpdateOffer";
export type { UpdateResult } from "./bindings/UpdateResult";
export type { UpdateSnapshot } from "./bindings/UpdateSnapshot";
export type { UpdateStatus } from "./bindings/UpdateStatus";
export type { WhatsNew } from "./bindings/WhatsNew";
export type { UnlistenFn };

export function getSnapshot(): Promise<AppSnapshot> {
  return invoke<AppSnapshot>("get_snapshot");
}

export function dispatch(command: Command): Promise<void> {
  return invoke<void>("dispatch", { command });
}

export function onAppState(cb: (snapshot: AppSnapshot) => void): Promise<UnlistenFn> {
  return listen<AppSnapshot>("app-state", (event) => cb(event.payload));
}
