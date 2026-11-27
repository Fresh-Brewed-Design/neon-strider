import { dispatch, type Permission, type PermissionKind, type PermissionState } from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import type { StatusTone } from "../../state/linkStatus";
import { useAppSnapshot } from "../../state/useAppState";
import { StatusPill } from "../StatusPill";
import { GhostButton, Hint } from "../ui";
import { Section } from "./fields";

/** What each permission is for, as the OS's settings name it. */
const PERMISSION_TEXT: Record<PermissionKind, { name: string; why: string }> = {
  bluetooth: { name: "Bluetooth", why: "Connects to the treadmill and a heart rate sensor." },
  input_monitoring: {
    name: "Input Monitoring",
    why: "Counts key presses for Neural flow and typing by speed. Never which keys.",
  },
};

const STATE: Record<PermissionState, { label: string; tone: StatusTone; title: string }> = {
  allowed: { label: "ALLOWED", tone: "ok", title: "Allowed in System Settings." },
  denied: { label: "NOT ALLOWED", tone: "problem", title: "Turned off in System Settings." },
  not_asked: {
    label: "NOT ASKED YET",
    tone: "idle",
    title: "macOS asks the first time the app needs it.",
  },
};

/** OPEN SETTINGS for one permission, with its result under it. */
function OpenSettingsButton({ kind }: { kind: PermissionKind }) {
  const open = useAction();
  return (
    <div className="flex flex-col items-end gap-1">
      <GhostButton
        disabled={open.pending}
        onClick={() => void open.run(() => dispatch({ type: "open_permission_settings", permission: kind }))}
      >
        Open settings
      </GhostButton>
      <ActionStatus pending={open.pending} error={open.error} />
    </div>
  );
}

function PermissionRow({ permission }: { permission: Permission }) {
  const text = PERMISSION_TEXT[permission.kind];
  const state = STATE[permission.state];
  return (
    <div className="flex flex-wrap items-start justify-between gap-2">
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <span className="font-mono text-xs font-bold uppercase tracking-[0.15em] text-white">{text.name}</span>
          <StatusPill tone={state.tone} label={state.label} title={state.title} />
        </div>
        <Hint className="mt-0.5">{text.why}</Hint>
      </div>
      <OpenSettingsButton kind={permission.kind} />
    </div>
  );
}

/**
 * System -> Permissions (macOS): what the OS lets the app use, and a way to
 * the setting for each. Hidden where the OS asks for none (Windows).
 */
export function PermissionsSection() {
  const s = useAppSnapshot();
  const permissions = s?.platform.permissions ?? [];
  if (permissions.length === 0) return null;
  return (
    <Section
      id="sys-permissions"
      title="Permissions"
      description="macOS asks for each permission the first time Neon Strider needs it."
    >
      {permissions.map((p) => (
        <PermissionRow key={p.kind} permission={p} />
      ))}
    </Section>
  );
}

/**
 * Under a feature that needs Input Monitoring: what to do while macOS hasn't
 * allowed it. Nothing where it is allowed or the OS asks for none.
 */
export function InputMonitoringNote() {
  const permissions = useAppSnapshot()?.platform.permissions ?? [];
  const state = permissions.find((p) => p.kind === "input_monitoring")?.state;
  if (state === undefined || state === "allowed") return null;
  return (
    <div className="flex flex-wrap items-start justify-between gap-2 rounded border border-neon-yellow/30 bg-neon-yellow/5 px-3 py-2">
      <Hint className="min-w-0 flex-1 text-neon-yellow/90">
        {state === "denied"
          ? "macOS blocks key press counting until Neon Strider is allowed in Input Monitoring."
          : "Key presses are not counted until Input Monitoring is allowed in the macOS prompt."}
      </Hint>
      <OpenSettingsButton kind="input_monitoring" />
    </div>
  );
}
