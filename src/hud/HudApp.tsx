import { useRef } from "react";
import { useAppSnapshot } from "../state/useAppState";
import { statWidget } from "../widgets/registry";
import { HudCardView } from "./HudCards";
import { useEyeBreakTones } from "./eyeBreakTones";
import { useProgramTones } from "./programTones";
import { HudHeader, type HeaderToggle } from "./HudHeader";
import type { PanelNote } from "./HudNote";
import { hotkeyTitle } from "./shortcutLabel";
import { Controls, ProgramsModule, StatCell, StatusRow } from "./HudPanel";
import { RunStripRow, StripRow } from "./HudStrip";
import { linkLabel } from "./status";
import { startFailure } from "../state/linkStatus";
import { waitHeadline } from "../dashboard/programs/format";
import { ToastCard } from "./ToastCard";
import { useHitRegion } from "./useHitRegion";
import { useLogoDrag, usePlacement } from "./useHudDrag";
import { useHudSend } from "./useHudSend";
import { useRelink } from "./useRelink";
import { useToasts } from "./useToasts";
import { useGrowsUp } from "./useGrowsUp";
import "./hud.css";

/**
 * The always-on-top overlay: the panel (header, pinned stats, modules), the
 * one card Rust picked (`hud_card`, ranked in `card_queue.rs`), then
 * transient toasts. Only this content takes the
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
  // A panel that grows upward keeps the programs module's tallest height.
  const growsUp = useGrowsUp(contentRef, placement.anchor, placement.custom !== null);
  const drag = useHudSend();
  const minify = useHudSend();
  // One sender for every control on the panel, so a refused press shows in the panel's one note.
  const panel = useHudSend();
  const logo = useLogoDrag(panelRef, drag.report);
  const relink = useRelink(snapshot?.connection ?? null);

  useEyeBreakTones(snapshot?.eye_break);
  useProgramTones(snapshot?.program_tones);

  const widgets = snapshot?.hud.widgets ?? [];
  const view = snapshot?.hud_view ?? "full";
  const full = view === "full";
  const minified = view === "minified";
  const wait = snapshot?.active_program?.wait ?? null;
  // The program card's headline is the one prompt while it shows.
  const promptOnProgramCard = full && wait !== null && widgets.includes("PROGRAMS");
  const hasPanel = snapshot !== null && widgets.length > 0 && view !== "hidden";
  // A toast is redundant while the panel already shows the same thing live
  // (the running strip shows speed; the still strip shows neither). The
  // header shows the belt's state and the link in every view of the panel.
  const toasts = allToasts.filter(
    (t) =>
      !(hasPanel && (t.type === "DECK" || t.type === "LINK")) &&
      !(full && t.type === "VELOCITY" && widgets.includes("VELOCITY")) &&
      !(minified && t.type === "VELOCITY") &&
      !(full && t.type === "ELEVATION" && widgets.includes("INCLINE")) &&
      !(promptOnProgramCard && t.type === "AUTOMATION" && t.message === waitHeadline(wait)),
  );
  const stats = widgets.filter((w) => statWidget(w) !== undefined);
  // One card at a time: Rust ranks the cards due, holds them while
  // presenting and names the one to show.
  const card = snapshot?.hud_card ?? "none";
  const hasContent = hasPanel || toasts.length > 0 || card !== "none";

  useHitRegion(contentRef, hasContent);

  if (!hasContent) return null;

  const custom = placement.custom;
  const offline = snapshot !== null && !snapshot.connection.connected;
  // The panel's one note: a refused press first, then the start that failed,
  // then the deck's reply to a target (with Rust's plain words) on the whole panel.
  const pressNote = panel.error ?? relink.error ?? drag.error ?? minify.error;
  // A start the sit-too-long card's ENGAGE made shows on that card while it is up, not here too.
  const cardOwnsStart = card === "seated" && snapshot?.connection.start_press === "seated_engage";
  const failed = snapshot && !cardOwnsStart ? startFailure(snapshot) : null;
  const deckNote = full && snapshot?.deck.target_note ? snapshot.deck : null;
  const note: PanelNote | null = pressNote
    ? { text: pressNote }
    : failed
      ? { text: failed }
      : deckNote?.target_note
        ? { text: deckNote.target_note, detail: deckNote.target_note_hint }
        : null;
  // The size button sits in the header's corner in every view. A program's
  // first 30 s waiting for GO keep the whole panel (Rust ignores minify then,
  // `hud_go_hold`), so the button stays in place but off until GO or the hold ends.
  const toggle: HeaderToggle | undefined =
    snapshot === null
      ? undefined
      : full
        ? {
            kind: "minify",
            disabled: snapshot.hud_go_hold,
            title: snapshot.hud_go_hold ? "The whole HUD stays up for 30 seconds while a program waits for GO." : hotkeyTitle("Shrink the HUD to a strip", snapshot.shortcuts),
            onClick: () => void minify.send({ type: "hud_set_minified", minified: true }),
          }
        : minified
          ? {
              kind: "expand",
              title: hotkeyTitle("Show the whole HUD", snapshot.shortcuts),
              onClick: () => void minify.send({ type: "hud_set_minified", minified: false }),
            }
          : view === "strip"
            ? {
                kind: "expand",
                title: "Show the whole HUD until the belt moves again",
                onClick: () => void minify.send({ type: "hud_expand" }),
              }
            : undefined;
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
            className={`hud-panel${full ? "" : " is-strip"}${resting ? " is-resting" : ""}${offline ? " is-offline" : ""}`}
          >
            <HudHeader
              link={link}
              relink={relink.view}
              onRelink={relink.relink}
              logo={logo}
              dragging={placement.dragging}
              toggle={toggle}
              simulated={snapshot.connection.simulated}
            />
            {/* Everything under the header: it scrolls if the panel can't fit the screen (a large text size), so the header stays on it. */}
            <div className="hud-body">
              {full ? (
                <>
                  {stats.length > 0 && (
                    <div className="hud-stats">
                      {stats.map((id) => (
                        <StatCell key={id} id={id} snapshot={snapshot} />
                      ))}
                    </div>
                  )}
                  <StatusRow snapshot={snapshot} relink={relink} note={note} />
                  {widgets.includes("CONTROLS") && <Controls snapshot={snapshot} send={panel.send} />}
                  {widgets.includes("PROGRAMS") && <ProgramsModule snapshot={snapshot} send={panel.send} reserve={growsUp} />}
                </>
              ) : minified ? (
                <RunStripRow snapshot={snapshot} send={panel.send} note={note} />
              ) : (
                <StripRow snapshot={snapshot} send={panel.send} note={note} />
              )}
            </div>
          </section>
        )}
        {snapshot && <HudCardView card={card} snapshot={snapshot} />}
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
