import { useState, type ReactNode } from "react";
import { stasisClock } from "../../format";
import { dispatch, type AppSnapshot, type DeckSnapshot } from "../../ipc";
import { linkView } from "../../state/linkStatus";
import { snapToDeck, snapToLevel } from "../../state/deckSpeeds";
import { useAppSnapshot } from "../../state/useAppState";
import { STAT_WIDGETS, statWidget } from "../../widgets/registry";
import { Sparkline } from "../../widgets/Sparkline";
import { Axis } from "../live/Axis";
import { PinToggle } from "../live/PinToggle";
import { Transport } from "../live/Transport";
import { TargetReadout } from "../live/TargetReadout";
import { StepMap } from "../live/StepMap";
import { WalkReadout } from "../live/WalkReadout";
import { ZoneReadout } from "../live/ZoneReadout";
import { ActiveProgramCard } from "../programs/ActiveProgramCard";
import { readPref, writePref } from "../localPrefs";
import { StatusPill } from "../StatusPill";
import { MaintenanceBanner } from "../system/MaintenanceBanner";
import { CalibrateOffer } from "../system/calibration/CalibrateOffer";
import { Codename, Label, Page, Panel, PlainLine, Waiting } from "../ui";
import { cx, TONE_TEXT } from "../util";

const SPEED_PRESETS = [1.5, 2.5, 3, 4, 5, 6];

const LIVE_STATS = STAT_WIDGETS.filter((w) => w.id !== "VELOCITY" && w.id !== "INCLINE");

/**
 * A tile's codename shrinks with the tile (12 px down to 11 px) so a single long word such as
 * DISPLACEMENT fits a narrow tile at a large Windows text size; the break inside a word stays only
 * as a last resort below that floor.
 */
export const CODENAME_FIT =
  "min-w-0 text-[length:clamp(11px,7.5cqw,12px)] tracking-[0.12em] [overflow-wrap:anywhere]";

/** Stat tiles the Live page fits under the controls: three rows of three. */
const LIVE_STAT_CELLS = 9;

/**
 * The Live page's stats. PULSE_RATE takes two tiles while it has a chart.
 * DESK TIME WALKED joins only when a tile is free, so the page stays three rows
 * high (760x600 with a program card open has no room for a fourth); the HUD
 * page still offers it.
 */
function liveStats(s: AppSnapshot) {
  const shown = LIVE_STATS.filter((w) => w.id !== "NEURAL" || s.neural.enabled);
  const cells = shown.reduce((n, w) => n + (w.spark?.(s) ? 2 : 1), 0);
  return cells > LIVE_STAT_CELLS ? shown.filter((w) => w.id !== "PC_WALK_SHARE") : shown;
}

/**
 * Tilt presets from this treadmill's own range: its lowest step, a third,
 * two thirds and the top, so none falls outside what it can do.
 */
function tiltPresets(deck: DeckSnapshot): number[] {
  const top = deck.max_incline_level;
  // A level the deck skips is left out of the presets.
  const levels = [
    ...new Set([0, Math.round(top / 3), Math.round((2 * top) / 3), top].map((l) => snapToLevel(l, deck))),
  ];
  return levels.map((l) => deck.incline_offset_pct + l * deck.incline_pct_per_level);
}

/** The incline level nearest to a grade in percent. */
function levelForPct(deck: DeckSnapshot, pct: number): number {
  const per = deck.incline_pct_per_level;
  const raw = Math.abs(per) < 1e-6 ? 0 : Math.round((pct - deck.incline_offset_pct) / per);
  return snapToLevel(Math.max(0, Math.min(deck.max_incline_level, raw)), deck);
}

/** A warning in the page header, where it costs no height: this page is sized to fit 760x600. */
function headerNotice(s: AppSnapshot): ReactNode {
  let notice: { label: string; title: string; text: string };
  if (s.connection.link?.state === "no_data") {
    const view = linkView(s.connection, s.platform.settings);
    notice = { label: view.label, title: view.title, text: view.headline };
  } else if (s.connection.connected && s.deck.unknown_status_code !== null) {
    notice = {
      label: "STATE ?",
      title: "The log records each unknown treadmill status code.",
      text: `Treadmill state unknown (code ${s.deck.unknown_status_code})`,
    };
  } else if (s.connection.connected && s.deck.standby) {
    notice = { label: "DECK OFF", title: "Neon Strider can't wake a treadmill that is off or asleep.", text: "Treadmill is off or asleep" };
  } else {
    return null;
  }
  return (
    <div role="status" className="flex max-w-80 items-center gap-2 font-mono text-[11px] leading-snug text-neon-yellow">
      <StatusPill bare tone="working" label={notice.label} title={notice.title} className="shrink-0" />
      <span className="min-w-0">{notice.text}</span>
    </div>
  );
}

/** The quiet speed QUIET eases to: the quiet setting, never under the deck's slowest (as on the HUD). */
function quietKmh(s: AppSnapshot): number {
  return Math.max(s.calls.quiet_kmh, s.deck.min_speed_kmh);
}

/**
 * QUIET // DAMPED while Quiet now (the HUD's QUIET or the tray's) holds the belt at the quiet speed,
 * with the speed a second press puts back: the HUD's QUIET line, in the header so it costs no height.
 */
function quietNotice(s: AppSnapshot): ReactNode {
  const { quiet_now, quiet_now_back_kmh: back } = s.calls;
  if (!quiet_now || back === null) return null;
  return (
    // One line down to 150 % Windows text, so the program card's buttons stay above the fold.
    <div
      role="status"
      title="QUIET is on the HUD and in the tray menu."
      className="flex flex-wrap items-baseline justify-end gap-x-2 font-mono text-[11px] leading-snug text-neon-yellow"
    >
      <span className="whitespace-nowrap font-bold tracking-[0.08em]">QUIET // DAMPED {quietKmh(s).toFixed(1)} km/h</span>
      <span>QUIET again puts {back.toFixed(1)} km/h back.</span>
    </div>
  );
}

/** The header's right side: the maintenance banner, then QUIET's hold and a warning under it. */
function headerActions(s: AppSnapshot): ReactNode {
  const quiet = quietNotice(s);
  const notice = headerNotice(s);
  if (!s.maintenance?.due && !(quiet && notice)) return quiet ?? notice;
  return (
    <div className="flex flex-col items-end gap-0.5">
      {s.maintenance?.due && <MaintenanceBanner />}
      {quiet}
      {notice}
    </div>
  );
}

/**
 * The STASIS clock while the belt is paused: how long the pause has lasted (Rust's
 * `deck.paused_for_s`, the HUD's STASIS line). It sits in Velocity's title row beside the pin, a
 * row the pin already makes tall enough, so nothing moves and Re-jack keeps its plain line. Where
 * the row is short of room (150 % Windows text) STASIS wraps onto a hidden line and the time stays.
 */
function stasisClockStatus(s: AppSnapshot): ReactNode {
  const p = s.deck.paused_for_s;
  if (!s.connection.connected || s.deck.status !== "Paused" || p === null) return null;
  return (
    <span
      role="status"
      data-testid="live-stasis-clock"
      title="Time since the belt paused."
      aria-label={`STASIS ${stasisClock(p)}`}
      className="flex h-4 min-w-0 flex-row-reverse flex-wrap items-center gap-x-1.5 overflow-hidden whitespace-nowrap font-mono text-[11px] font-bold leading-4 text-neon-cyan"
    >
      <span className="tabular-nums tracking-[0.04em]">{stasisClock(p)}</span>
      <span className="tracking-[0.15em]">STASIS</span>
    </span>
  );
}

/** STRIDE_FIT's mark on the speed slider, or none while it is unset. Only a mark: the slider still goes to the top. */
function strideFitMark(deck: DeckSnapshot) {
  const fit = deck.stride_fit_kmh;
  if (fit === null || fit === undefined) return null;
  return {
    value: fit,
    label: `FIT ${fit.toFixed(1)}`,
    title: `STRIDE_FIT: your stride fits the deck up to ${fit.toFixed(1)} km/h.`,
  };
}

/** Which of STEP_MAP or the stats shows while a program runs; STEP_MAP until switched. */
const STEP_MAP_PREF = "liveStepMap";

/** The Live page control a target came from. */
type NoteFrom = "speed" | "tilt";

/**
 * The Live page. `onCalibrate` opens System, Calibration from the first-connect offer, which
 * shows here in the stats' place, under the controls, so its arrival and SKIP never move them.
 */
export function LivePage({ onCalibrate }: { onCalibrate?: () => void } = {}) {
  const s = useAppSnapshot();
  // Which control sent last, and the deck's note showing at that press: that
  // note is about an older target (a program step, the HUD), so it stays off
  // the control. A press on the control that already shows a note keeps it.
  const [sent, setSent] = useState<{ from: NoteFrom; before: string | null } | null>(null);
  const [mapOpen, setMapOpen] = useState(() => readPref(STEP_MAP_PREF) !== "closed");
  const toggleMap = () => {
    writePref(STEP_MAP_PREF, mapOpen ? "closed" : "open");
    setMapOpen(!mapOpen);
  };
  if (s && !s.deck.target_note && sent) setSent(null);
  const sentFrom = (from: NoteFrom) => {
    if (sent?.from !== from) setSent({ from, before: s?.deck.target_note ?? null });
  };
  const noteFor = (from: NoteFrom) =>
    sent?.from === from && s?.deck.target_note !== sent.before ? (s?.deck.target_note ?? null) : null;
  if (!s) return <Waiting />;
  const { deck } = s;
  const offline = !s.connection.connected;
  const stats = liveStats(s);
  const pinned = new Set(s.hud.widgets);

  return (
    // `fitHeader`: in a short window (a large Windows text size) the title gives its height to the
    // controls; the rail already marks Live, and a warning in the header still shows.
    <Page codename="LOCOMOTION_DECK" title="Live" actions={headerActions(s)} fitHeader>
      {/* The controls fit the 760x600 popup without scrolling up to 150 % Windows text, even with
          the maintenance banner and a running program both showing: both banners sit in the
          header, and the calibration offer and the stats come after the controls. */}
      <div className="flex min-h-full flex-col gap-1.5">
        {/* Two columns while each gets 12rem, which keeps them side by side up to 150 % Windows
            text (each panel lays itself out narrower under 14rem); above that they stack. */}
        <div className="grid grid-cols-[repeat(auto-fit,minmax(12rem,1fr))] gap-3">
          <Axis
            title="Velocity"
            wheelHint="Scroll to change speed."
            tone="cyan"
            unit="km/h"
            actual={deck.speed_kmh}
            target={deck.target_speed_kmh}
            // The lowest setting is the belt's floor: only Pause or Stop take it to 0.
            min={deck.min_speed_kmh}
            max={Math.max(deck.max_speed_kmh, deck.min_speed_kmh + 0.1)}
            step={0.1}
            presets={SPEED_PRESETS}
            formatPreset={(v) => v.toFixed(1)}
            disabled={offline}
            onSet={(kmh) => {
              sentFrom("speed");
              return dispatch({ type: "set_speed", kmh });
            }}
            onNudge={(up) => {
              sentFrom("speed");
              return dispatch({ type: "nudge_speed", up });
            }}
            note={noteFor("speed")}
            snap={(kmh) => snapToDeck(kmh, deck)}
            mark={strideFitMark(deck)}
            pin={{ widget: "VELOCITY", pinned: pinned.has("VELOCITY") }}
            status={stasisClockStatus(s)}
            spark={statWidget("VELOCITY")?.spark?.(s) ?? null}
          />
          <Axis
            title="Tilt"
            wheelHint="Scroll to change incline."
            tone="magenta"
            unit="%"
            actual={deck.incline_pct}
            target={deck.target_incline_pct}
            min={deck.incline_offset_pct}
            max={Math.max(deck.max_incline_pct, deck.incline_offset_pct + 0.1)}
            step={Math.max(Math.abs(deck.incline_pct_per_level), 0.1)}
            presets={tiltPresets(deck)}
            formatPreset={(v) => `${Math.round(v * 10) / 10}%`}
            disabled={offline}
            onSet={(pct) => {
              sentFrom("tilt");
              return dispatch({ type: "set_incline_level", level: levelForPct(deck, pct) });
            }}
            onNudge={(up) => {
              sentFrom("tilt");
              return dispatch({ type: "nudge_incline", up });
            }}
            note={noteFor("tilt")}
            pin={{ widget: "INCLINE", pinned: pinned.has("INCLINE") }}
          />
        </div>

        <Transport s={s} />

        <ActiveProgramCard stepMap={{ open: mapOpen, onToggle: toggleMap }} />

        {/* The first-connect offer stands in the stats' place (compact, no taller than them), so its
            arrival and SKIP move nothing above it and the page keeps its height; the stats or
            STEP_MAP come back at SKIP. */}
        {onCalibrate && s.calibration.offer ? (
          <CalibrateOffer compact onCalibrate={onCalibrate} />
        ) : s.active_program && mapOpen ? (
          <StepMap a={s.active_program} readout={<WalkReadout s={s} />} />
        ) : (
          // At most three tiles a row (each a third of the row, less the two 0.375rem gaps), as at
          // 100 %; under 8.5rem a tile (DISPLACEMENT at 11 px and the pin) would not fit, so a large Windows
          // text size gives fewer, wider tiles. The tight spacing and the -mb-2 (12 px under the grid
          // in place of the page's 20) keep three rows under a program card at 760x600.
          <div className="-mb-2 grid grid-cols-[repeat(auto-fit,minmax(max(8.5rem,calc((100%_-_0.75rem)/3)),1fr))] gap-1.5">
            {stats.map((w) => {
              const { value, unit } = w.read(s);
              const spark = w.spark?.(s);
              const zone = w.zone?.(s);
              const target = w.target?.(s);
              const detail = w.detail?.(s);
              // A detail with no space to break at (today's walking time) drops under the value when the tile is too narrow.
              const unbreakable = detail != null && !detail.includes(" ");
              // PULSE_RATE with a sensor takes two tiles, so its chart sits
              // beside the bpm and the zone beside the name, and the page keeps
              // the height it has without one. It is the last stat, so its row
              // always has a free tile to take.
              return (
                <Panel key={w.id} className={cx("@container flex flex-col px-3 py-[3px]", spark && "col-span-2")}>
                  {/* With no readout beside the name, the pin sits snug so LIFETIME_ODOMETER keeps one line. */}
                  <div className={cx("flex items-start justify-between", (zone || target) && "gap-1")}>
                    <div className="min-w-0">
                      <Label className={cx("block leading-tight", CODENAME_FIT)}>
                        <Codename text={w.codename} />
                      </Label>
                      {w.plain && <PlainLine text={w.plain} />}
                    </div>
                    {zone && <ZoneReadout zone={zone} />}
                    {target && <TargetReadout target={target} />}
                    <div className="-mr-1 shrink-0">
                      <PinToggle widget={w.id} pinned={pinned.has(w.id)} />
                    </div>
                  </div>
                  {/* Offline, the reading is the treadmill's last one, dimmed as on the HUD. */}
                  <div className={cx("flex items-center gap-x-3", unbreakable && "flex-wrap", offline && "opacity-45")}>
                    <span className={cx("shrink-0 font-digital text-base leading-5 tracking-wider", TONE_TEXT[w.accent])}>
                      {value}
                      {unit && <span className="ml-1 font-mono text-xs text-white/60">{unit}</span>}
                    </span>
                    {spark && <Sparkline data={spark} className={cx("block h-5 min-w-0 flex-1", TONE_TEXT[w.accent])} />}
                    {detail && <span className="min-w-0 font-mono text-xs leading-none text-white/60">{detail}</span>}
                  </div>
                </Panel>
              );
            })}
          </div>
        )}
      </div>
    </Page>
  );
}
