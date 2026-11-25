import { useRef } from "react";
import { useAppSnapshot } from "../state/useAppState";
import { statWidget } from "../widgets/registry";
import { EyeBreakCard, MaintenanceCard, SeatedCard, UpdateCard, WalkAwayCard, WeeklyCard } from "./HudCards";
import { useEyeBreakTones } from "./eyeBreakTones";
import { useProgramTones } from "./programTones";
import { HudHeader } from "./HudHeader";
import { HudNote } from "./HudNote";
import { shortcutLabel } from "./shortcutLabel";
import {
  Controls,
  EasyStopLine,
  NeuralLine,
  PresentingLine,
  QuietNowLine,
  ProgramsModule,
  SpeedDriftLine,
  StatCell,
  StrideFitLine,
} from "./HudPanel";
import { RunStripRow, StripRow } from "./HudStrip";
import { linkLabel } from "./status";
import { waitHeadline } from "../dashboard/programs/format";
import { ToastCard } from "./ToastCard";
import { useHitRegion } from "./useHitRegion";
import { useLogoDrag, usePlacement } from "./useHudDrag";
import { useHudSend } from "./useHudSend";
import { useRelink } from "./useRelink";
import { useToasts } from "./useToasts";
import "./hud.css";

/**
 * The always-on-top overlay: the panel (header, pinned stats, modules), the
 * persistent cards, then transient toasts. Only this content takes the
 * pointer; the rest of the window is click-through. While the belt is still,
 * the live stats dim, and Rust may ask for the panel as a strip or not at all,
 * and minified for the running strip (`hud_view`); the cards and toasts show
 * either way.
 */
export function HudApp() {
  const snapshot = useAppSnapshot();
  const allToasts = useToasts();
  const contentRef = useRef<HTMLDivElement>(null);
  const panelRef = useRef<HTMLElement>(null);
  const placement = usePlacement(snapshot?.hud);
  const drag = useHudSend();
  const minify = useHudSend();
  const logo = useLogoDrag(panelRef, drag.report);
  const relink = useRelink(snapshot?.connection ?? null);

  useEyeBreakTones(snapshot?.eye_break);
  useProgramTones(snapshot?.program_tones);

  const widgets = snapshot?.hud.widgets ?? [];
  const view = snapshot?.hud_view ?? "full";
  const full = view === "full";
  const minified = view === "minified";
  const wait = snapshot?.active_program?.wait ?? null;
  // A program waiting for GO keeps the whole panel (Rust ignores minify then).
  const canMinify = full && !wait;
  // The program card's headline is the one prompt while it shows.
  const promptOnProgramCard = full && wait !== null && widgets.includes("PROGRAMS");
  // A toast is redundant while the panel already shows the same thing live
  // (the running strip shows speed; the still strip shows neither).
  const toasts = allToasts.filter(
    (t) =>
      !(full && t.type === "VELOCITY" && widgets.includes("VELOCITY")) &&
      !(minified && t.type === "VELOCITY") &&
      !(full && t.type === "ELEVATION" && widgets.includes("INCLINE")) &&
      !(promptOnProgramCard && t.type === "AUTOMATION" && t.message === waitHeadline(wait)),
  );
  const stats = widgets.filter((w) => statWidget(w) !== undefined);
  const hasPanel = snapshot !== null && widgets.length > 0 && view !== "hidden";
  // Rust holds the cards that can wait while presenting (`presenting.holding`).
  const maintenanceDue = snapshot?.maintenance.hud_card ?? false;
  const weeklyCard = snapshot?.weekly.show_card ?? false;
  const seatedCard = snapshot?.seated.show_card ?? false;
  const eyeBreakCard = (snapshot?.eye_break.phase ?? "idle") !== "idle";
  const walkawayCard = (snapshot?.walkaway.card ?? "none") !== "none";
  const updateCard = snapshot?.update.hud_card ?? false;
  const hasContent =
    hasPanel || toasts.length > 0 || maintenanceDue || weeklyCard || seatedCard || eyeBreakCard || walkawayCard || updateCard;

  useHitRegion(contentRef, hasContent);

  if (!hasContent) return null;

  const custom = placement.custom;
  const base = snapshot ? linkLabel(snapshot) : null;
  // A relink click in flight reads as working (LINKING…) until the state moves.
  const link = base && relink.view?.text ? { text: relink.view.text, tone: "warn" as const } : base;
  // While the belt rests the live stats recede. Never while the link is down
  // or a program waits for GO; a note on the panel keeps them up too (hud.css).
  const resting = link?.tone === "idle" && wait === null;

  return (
    <div className={`hud-root anchor-${custom ? "custom" : placement.anchor}`}>
      {/* Custom position: the spacer holds the content's top offset, and
          shrinks first if the content grows past the bottom of the screen. */}
      {custom && <div className="hud-spacer" style={{ height: custom.top }} />}
      <div ref={contentRef} className="hud-content">
        {hasPanel && snapshot && link && (
          <section
            ref={panelRef}
            className={`hud-panel${full ? "" : " is-strip"}${resting ? " is-resting" : ""}`}
          >
            <HudHeader
              link={link}
              relink={relink.view}
              onRelink={relink.relink}
              logo={logo}
              dragging={placement.dragging}
              onMinify={canMinify ? () => void minify.send({ type: "hud_set_minified", minified: true }) : undefined}
              minifyKey={shortcutLabel(snapshot.shortcuts, "toggle_minified")}
              simulated={snapshot.connection.simulated}
            />
            <HudNote error={relink.error ?? drag.error ?? minify.error} />
            {full ? (
              <>
                {stats.length > 0 && (
                  <div className="hud-stats">
                    {stats.map((id) => (
                      <StatCell key={id} id={id} snapshot={snapshot} />
                    ))}
                  </div>
                )}
                <NeuralLine snapshot={snapshot} />
                <SpeedDriftLine snapshot={snapshot} />
                <EasyStopLine snapshot={snapshot} />
                <StrideFitLine snapshot={snapshot} />
                <QuietNowLine snapshot={snapshot} />
                <PresentingLine snapshot={snapshot} />
                {widgets.includes("CONTROLS") && <Controls snapshot={snapshot} />}
                {widgets.includes("PROGRAMS") && (
                  <ProgramsModule snapshot={snapshot} offlineShown={widgets.includes("CONTROLS")} />
                )}
              </>
            ) : minified ? (
              <RunStripRow snapshot={snapshot} />
            ) : (
              <StripRow snapshot={snapshot} />
            )}
          </section>
        )}
        {walkawayCard && snapshot && <WalkAwayCard snapshot={snapshot} />}
        {seatedCard && snapshot && <SeatedCard snapshot={snapshot} />}
        {eyeBreakCard && snapshot && <EyeBreakCard snapshot={snapshot} />}
        {maintenanceDue && snapshot && <MaintenanceCard snapshot={snapshot} />}
        {weeklyCard && snapshot && <WeeklyCard snapshot={snapshot} />}
        {updateCard && snapshot && <UpdateCard snapshot={snapshot} />}
        {toasts.length > 0 && (
          <div className="hud-toasts">
            {toasts.map((t) => (
              <ToastCard key={t.key} toast={t} snapshot={snapshot} />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
