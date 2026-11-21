import { ArrowDown, ArrowUp, Plus, X } from "lucide-react";
import {
  dispatch,
  type HudAnchor,
  type HudIdleMode,
  type HudMinifyHotkey,
  type HudMonitor,
  type HudWidget,
} from "../../ipc";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useAppSnapshot } from "../../state/useAppState";
import { MINIFY_KEYS, minifyKeyText } from "../../hud/minifyHotkey";
import { MODULE_WIDGETS, STAT_WIDGETS, widgetName } from "../../widgets/registry";
import { Toggle } from "../system/fields";
import { GhostButton, Hint, Label, Page, Panel, Waiting } from "../ui";
import { cx, send } from "../util";

const CORNERS: { anchor: HudAnchor; label: string; className: string }[] = [
  { anchor: "top_left", label: "Top left", className: "left-2 top-2" },
  { anchor: "top_right", label: "Top right", className: "right-2 top-2" },
  { anchor: "bottom_left", label: "Bottom left", className: "bottom-2 left-2" },
  { anchor: "bottom_right", label: "Bottom right", className: "bottom-2 right-2" },
];

const ALL: { id: HudWidget; description: string }[] = [
  ...MODULE_WIDGETS.map((m) => ({ id: m.id, description: m.description })),
  ...STAT_WIDGETS.map((w) => ({ id: w.id, description: `Shown as ${w.label}` })),
];

function CornerPicker({ anchor, custom }: { anchor: HudAnchor; custom: boolean }) {
  return (
    <div className="relative mx-auto aspect-[16/10] w-full max-w-64 rounded-md border border-white/15 bg-white/[0.02]">
      <div className="absolute inset-x-0 bottom-0 h-3 rounded-b-md border-t border-white/10 bg-white/5" />
      {CORNERS.map((c) => {
        const active = !custom && c.anchor === anchor;
        return (
          <button
            key={c.anchor}
            type="button"
            aria-pressed={active}
            title={c.label}
            onClick={() => send({ type: "hud_set_anchor", anchor: c.anchor })}
            className={cx(
              "absolute h-10 w-12 rounded-sm border transition-colors",
              c.className,
              c.anchor.startsWith("bottom") && "bottom-5",
              active
                ? "border-neon-cyan bg-neon-cyan/20 shadow-[0_0_10px_rgba(0,243,255,0.4)]"
                : "border-white/15 bg-black/40 hover:border-neon-cyan/50",
            )}
          >
            <span className="sr-only">{c.label}</span>
          </button>
        );
      })}
    </div>
  );
}

/**
 * Where the HUD is: a corner, or where its logo was dragged to, with Reset
 * (the same as double-clicking the logo) to go back to the corner.
 */
function PlacementRow({ anchor, custom }: { anchor: HudAnchor; custom: boolean }) {
  const reset = useAction();
  const corner = CORNERS.find((c) => c.anchor === anchor)?.label ?? "Bottom right";
  return (
    <div className="flex flex-col gap-1">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <span className="font-mono text-[11px] text-white/70" aria-live="polite">
          {custom ? "Custom position" : `${corner} corner`}
        </span>
        {custom && (
          <GhostButton
            disabled={reset.pending}
            onClick={() => void reset.run(() => dispatch({ type: "hud_reset_position" }))}
          >
            Reset
          </GhostButton>
        )}
      </div>
      <ActionStatus pending={reset.pending} error={reset.error} />
      <Hint>
        Drag the NEON STRIDER logo to move the HUD to any display. Near a corner, the HUD snaps into place.
      </Hint>
    </div>
  );
}

/** Which display the HUD sits on. Hidden when there is only one. */
function MonitorPicker({ monitors, selected }: { monitors: HudMonitor[]; selected: string | null }) {
  if (monitors.length < 2) return null;
  // Rust says which display the HUD is on: the saved one, found by name or by
  // position, or the primary when it is gone.
  const current = monitors.find((m) => m.active);
  const active = current?.name ?? monitors.find((m) => m.is_primary)?.name ?? monitors[0].name;
  const missing = selected !== null && (!current || current.is_primary);
  return (
    <div className="flex flex-col gap-1.5">
      <Label>Display</Label>
      <ul className="flex flex-col gap-1" aria-label="Display">
        {monitors.map((m) => (
          <li key={m.name}>
            <button
              type="button"
              aria-pressed={m.name === active}
              onClick={() => send({ type: "hud_set_monitor", monitor: m.is_primary ? null : m.name })}
              className={cx(
                "w-full rounded border px-2 py-1.5 text-left font-mono text-[11px] transition-colors",
                m.name === active
                  ? "border-neon-cyan bg-neon-cyan/15 text-neon-cyan"
                  : "border-white/15 text-white/70 hover:border-neon-cyan/50",
              )}
            >
              {m.label}
              {m.is_primary && <span className="text-white/45"> (primary)</span>}
            </button>
          </li>
        ))}
      </ul>
      {missing && (
        <p className="font-mono text-[11px] leading-relaxed text-neon-yellow/80">
          The chosen display is not connected, so the HUD sits on the primary display.
        </p>
      )}
    </div>
  );
}

const IDLE_MODES: { mode: HudIdleMode; label: string }[] = [
  { mode: "full", label: "Show everything" },
  { mode: "strip", label: "Shrink to a strip" },
  { mode: "hidden", label: "Hide" },
];

/** "When the belt is still": the whole panel, a strip, or nothing until the belt moves. */
function IdleModePicker({ mode }: { mode: HudIdleMode }) {
  const set = useAction();
  return (
    <div className="flex flex-col gap-1.5">
      <Label>When the belt is still</Label>
      <div className="flex flex-col gap-1" role="radiogroup" aria-label="When the belt is still">
        {IDLE_MODES.map((m) => (
          <button
            key={m.mode}
            type="button"
            role="radio"
            aria-checked={m.mode === mode}
            disabled={set.pending}
            onClick={() => void set.run(() => dispatch({ type: "hud_set_idle_mode", mode: m.mode }))}
            className={cx(
              "w-full rounded border px-2 py-1.5 text-left font-mono text-[11px] transition-colors",
              m.mode === mode
                ? "border-neon-cyan bg-neon-cyan/15 text-neon-cyan"
                : "border-white/15 text-white/70 hover:border-neon-cyan/50",
            )}
          >
            {m.label}
          </button>
        ))}
      </div>
      <ActionStatus pending={set.pending} error={set.error} />
      <Hint>Cards that need an answer still show, and a hidden HUD comes back when the belt moves.</Hint>
    </div>
  );
}

/** The Alt hotkey that minifies the HUD: on or off, and which key. Off, the key is kept for next time. */
function MinifyHotkeyPicker({ hotkey }: { hotkey: HudMinifyHotkey }) {
  const set = useAction();
  const save = (next: HudMinifyHotkey) => void set.run(() => dispatch({ type: "hud_set_minify_hotkey", hotkey: next }));
  const keyText = minifyKeyText(hotkey.key);
  return (
    <div className="flex flex-col gap-1.5">
      <Toggle
        label="Minify hotkey"
        checked={hotkey.enabled}
        onChange={(enabled) => save({ ...hotkey, enabled })}
        description={
          hotkey.enabled
            ? `${keyText} shrinks the HUD to a strip and brings it back.`
            : `Other apps get ${keyText}.`
        }
      />
      {hotkey.enabled && (
        <div className="flex flex-col gap-1" role="radiogroup" aria-label="Minify hotkey">
          {MINIFY_KEYS.map((k) => (
            <button
              key={k.key}
              type="button"
              role="radio"
              aria-checked={k.key === hotkey.key}
              disabled={set.pending}
              onClick={() => save({ ...hotkey, key: k.key })}
              className={cx(
                "w-full rounded border px-2 py-1.5 text-left font-mono text-[11px] transition-colors",
                k.key === hotkey.key
                  ? "border-neon-cyan bg-neon-cyan/15 text-neon-cyan"
                  : "border-white/15 text-white/70 hover:border-neon-cyan/50",
              )}
            >
              {minifyKeyText(k.key)}
            </button>
          ))}
        </div>
      )}
      <ActionStatus pending={set.pending} error={set.error} />
    </div>
  );
}

export function HudPage() {
  const s = useAppSnapshot();
  if (!s) return <Waiting />;
  const { widgets, anchor, monitor } = s.hud;

  const move = (i: number, by: -1 | 1) => {
    const next = [...widgets];
    const j = i + by;
    if (j < 0 || j >= next.length) return;
    [next[i], next[j]] = [next[j], next[i]];
    send({ type: "hud_set_widgets", widgets: next });
  };
  const available = ALL.filter((w) => !widgets.includes(w.id));

  return (
    <Page codename="OVERLAY_CONFIG" title="HUD">
      <div className="grid grid-cols-[minmax(0,1fr)_14rem] gap-3">
        <div className="flex flex-col gap-3">
          <Panel className="p-3">
            <div className="mb-2 flex items-center justify-between">
              <Label>On the HUD</Label>
              <span className="font-mono text-[11px] text-white/40">Top to bottom</span>
            </div>
            {widgets.length === 0 ? (
              <p className="px-1 py-3 font-mono text-xs text-white/45">
                Nothing pinned, so the HUD is hidden. Add a widget from Available.
              </p>
            ) : (
              <ul className="flex flex-col gap-1">
                {widgets.map((id, i) => (
                  <li key={id} className="flex items-center gap-2 rounded border border-white/10 bg-black/30 px-3 py-1.5">
                    <span className="flex-1 font-mono text-xs font-bold uppercase tracking-[0.15em] text-neon-cyan">
                      {widgetName(id)}
                    </span>
                    <GhostButton aria-label="Move up" disabled={i === 0} onClick={() => move(i, -1)}>
                      <ArrowUp size={14} />
                    </GhostButton>
                    <GhostButton aria-label="Move down" disabled={i === widgets.length - 1} onClick={() => move(i, 1)}>
                      <ArrowDown size={14} />
                    </GhostButton>
                    <GhostButton aria-label="Remove" onClick={() => send({ type: "hud_toggle_widget", widget: id })}>
                      <X size={14} />
                    </GhostButton>
                  </li>
                ))}
              </ul>
            )}
          </Panel>

          {available.length > 0 && (
            <Panel className="p-3">
              <Label className="mb-2 block">Available</Label>
              <ul className="flex flex-col gap-1">
                {available.map((w) => (
                  <li key={w.id}>
                    <button
                      type="button"
                      onClick={() => send({ type: "hud_toggle_widget", widget: w.id })}
                      className="flex w-full items-center gap-2 rounded border border-white/10 px-3 py-1.5 text-left transition-colors hover:border-neon-cyan/50 hover:bg-neon-cyan/5"
                    >
                      <Plus size={14} className="shrink-0 text-white/50" />
                      <span className="min-w-0">
                        <span className="block font-mono text-xs font-bold uppercase tracking-[0.12em] text-white/80">
                          {widgetName(w.id)}
                        </span>
                        <span className="block truncate font-mono text-[11px] text-white/40">{w.description}</span>
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </Panel>
          )}
        </div>

        <Panel className="flex flex-col gap-3 p-3">
          <Label>Corner</Label>
          <CornerPicker anchor={anchor} custom={s.hud.custom !== null} />
          <PlacementRow anchor={anchor} custom={s.hud.custom !== null} />
          <MonitorPicker monitors={s.hud_monitors} selected={monitor} />
          <IdleModePicker mode={s.hud.idle_mode} />
          <MinifyHotkeyPicker hotkey={s.hud.minify_hotkey} />
          <Hint>
            Clicks pass through the HUD except on its panel. Speed and tilt pop-ups stay hidden while VEL or TILT is on
            the HUD.
          </Hint>
        </Panel>
      </div>
    </Page>
  );
}
