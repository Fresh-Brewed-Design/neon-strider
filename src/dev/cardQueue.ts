// Dev-only: Rust's `card_queue.rs` on the mock's snapshot, so the HUD preview
// shows one card at a time as the app does. Rust's tests pin the rules; this
// copy only has to look the same in the preview.
import type { AppSnapshot, HudCard } from "../ipc";

const GRACE_MS = 5_000;
const FOLD_MS = 60_000;
const START_ASK_MS = 120_000;

type Showing = { kind: "fresh" } | { kind: "shown"; since: number } | { kind: "folded" };

let shown: HudCard = "none";
let leftAt: number | null = null;
let wasHolding = false;
let holdEndedAt: number | null = null;
let startAskedAt: number | null = null;
let seated: Showing = { kind: "fresh" };

const within = (now: number, since: number | null, ms: number) => since !== null && now - since < ms;

/** A folded card from the start (`?seated=folded`). */
export function foldSeated() {
  seated = { kind: "folded" };
}

/** Pick the card for `s` now and write it in (`hud_card`, `seated.folded`). */
export function applyCardQueue(s: AppSnapshot, now = Date.now()) {
  const connected = s.connection.connected;
  const running = connected && s.deck.status === "Running";
  const starting = connected && (s.deck.status === "Starting" || s.connection.start_pending);
  const holding = s.presenting.holding;
  if (wasHolding && !holding) holdEndedAt = now;
  wasHolding = holding;
  if (starting) startAskedAt = now;
  if (s.connection.start_result) startAskedAt = Math.max(startAskedAt ?? 0, s.connection.start_result.at_ms);

  // Rust's machine lets the card go while the belt runs.
  const seatedDue = s.seated.show_card && !running;
  if (!seatedDue) seated = { kind: "fresh" };
  else if (seated.kind === "fresh" && (running || within(now, startAskedAt, START_ASK_MS))) seated = { kind: "folded" };

  const eye = s.eye_break.phase;
  const eyeDeferred = eye === "upcoming" && ((s.eye_break.at_rest && running) || starting);
  const ranked: [boolean, HudCard][] = [
    [s.walkaway.card !== "none", "walk_away"],
    [eye === "complete" || eye === "break", "eye_break"],
    [s.active_program?.wait != null, "none"],
    [eye === "upcoming" && !eyeDeferred && !holding, "eye_break"],
    [seatedDue && seated.kind !== "folded" && !holding, "seated"],
    [s.weekly.show_card && !holding, "weekly"],
    [s.maintenance.hud_card && !holding, "maintenance"],
    [s.update.hud_card && !holding, "update"],
  ];
  const top = ranked.find(([due]) => due)?.[1] ?? "none";
  if (shown !== "none" && top !== shown) leftAt = now;
  const waits = top !== "none" && top !== "walk_away" && (top !== "eye_break" || eye === "upcoming");
  const inGrace = within(now, leftAt, GRACE_MS) || within(now, holdEndedAt, GRACE_MS);
  let card: HudCard = top !== shown && waits && inGrace ? "none" : top;
  shown = card;
  if (card === "seated") {
    if (seated.kind === "fresh") seated = { kind: "shown", since: now };
    if (seated.kind === "shown" && now - seated.since >= FOLD_MS) {
      seated = { kind: "folded" };
      leftAt = now;
      shown = card = "none";
    }
  }
  s.hud_card = card;
  s.seated.folded = seatedDue && seated.kind === "folded" && !starting;
  return card;
}
