// The STASIS clock (`deck.paused_for_s`): while the belt is paused it counts
// up once a second, as Rust's does, and clears on resume or stop. A paused
// preview starts it at 3:20 so the label shows a value at once;
// ?stasisclock=3909 starts it there instead (1:05:09, past an hour).
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

const DEFAULT_START_S = 200;

function startSeconds(): number {
  const n = Number(params.get("stasisclock"));
  return Number.isFinite(n) && n > 0 ? Math.floor(n) : DEFAULT_START_S;
}

/** When the mocked pause began, back-dated by the start value. */
let pausedAt: number | null = null;

function follow(): boolean {
  if (snapshot.deck.status !== "Paused") {
    if (pausedAt === null && snapshot.deck.paused_for_s === null) return false;
    pausedAt = null;
    snapshot.deck.paused_for_s = null;
    return true;
  }
  pausedAt ??= Date.now() - startSeconds() * 1000;
  const s = Math.floor((Date.now() - pausedAt) / 1000);
  if (s === snapshot.deck.paused_for_s) return false;
  snapshot.deck.paused_for_s = s;
  return true;
}

export const stasisClock: Scenario = {
  apply() {
    follow();
  },
  tick: follow,
};
