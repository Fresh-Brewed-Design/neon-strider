// STRIDE_FIT (#223). ?stridefit=4.5: the fastest speed the stride still fits
// the deck set to 4.5 km/h in System, Calibration, so the program editor flags
// Brisk's 5.0 km/h step and Interval walk's 6.0 km/h steps (Post-meal's 4.0
// stays unflagged). The belt runs at 5.0 km/h, so the Live page's speed
// slider shows the FIT mark below the thumb and the HUD shows its
// ABOVE STRIDE_FIT line (#226). Brisk, VO2 max and Interval walk offer
// FIT_TO_DECK on the Programs page (#225), worked out as Rust does.
import type { FitToDeckOffer, Program, ProgramStep } from "../../ipc";
import { params, snapshot } from "../mockState";
import type { Scenario } from "./types";

/** Rust's `stride_fit::in_deck_range`: held to the speed range, in tenths. */
function inDeckRange(kmh: number): number {
  const { min_speed_kmh: min, max_speed_kmh: max } = snapshot.deck;
  return Math.round(Math.min(Math.max(kmh, min), max) * 10) / 10;
}

/** Rust's `stride_fit::above`: running, and the lower of reading and target above the fit, in tenths. */
function above(): boolean {
  const { status, speed_kmh, target_speed_kmh, stride_fit_kmh: fit } = snapshot.deck;
  if (fit === null || status !== "Running") return false;
  return Math.round(Math.min(speed_kmh, target_speed_kmh) * 10) > Math.round(fit * 10);
}

/** Rust's `fit_to_deck::walking_vo2`: the ACSM walking equation, km/h and %. */
function vo2(kmh: number, pct: number): number {
  const s = (kmh * 1000) / 60;
  return 0.1 * s + 1.8 * s * (pct / 100) + 3.5;
}

/** The seeded programs as the mock starts, to tell an unedited one (Rust compares with `seed_programs`). */
const SEEDED: Program[] = structuredClone(snapshot.programs);

const tenths = (v: number) => Math.round(v * 10);
const sameSteps = (a: ProgramStep[], b: ProgramStep[]) => JSON.stringify(a) === JSON.stringify(b);

/** Rust's `fit_to_deck::fit_steps`: steps above the fit at it, with the grade that keeps their effort. */
function fitSteps(steps: ProgramStep[], fit: number, maxPct: number) {
  const changes: { from: ProgramStep; toPct: number; kept: number }[] = [];
  const fitted = steps.map((st) => {
    if (tenths(st.speed_kmh) <= tenths(fit)) return st;
    const sf = (fit * 1000) / 60;
    const want = ((vo2(st.speed_kmh, st.incline_pct) - 3.5 - 0.1 * sf) / (1.8 * sf)) * 100;
    const toPct = Math.round(Math.max(Math.min(want, maxPct), st.incline_pct) * 10) / 10;
    changes.push({ from: st, toPct, kept: vo2(fit, toPct) / vo2(st.speed_kmh, st.incline_pct) });
    return { ...st, speed_kmh: fit, incline_pct: toPct };
  });
  return { fitted, changes };
}

const plural = (n: number) => (n === 1 ? "1 step" : `${n} steps`);

/** Rust's `fit_to_deck::offers`, with its words. */
function fitOffers(): FitToDeckOffer[] {
  const fit = snapshot.deck.stride_fit_kmh;
  if (fit === null) return [];
  const maxPct = snapshot.deck.max_incline_pct;
  const name = `STRIDE_FIT ${fit.toFixed(1)} km/h`;
  return snapshot.programs.flatMap((p) => {
    const seeded = p.note && SEEDED.some((s) => sameSteps(s.steps, p.steps) && s.repeat_every_minutes === p.repeat_every_minutes);
    if (!seeded) return [];
    const { changes } = fitSteps(p.steps, fit, maxPct);
    if (changes.length === 0) return [];
    const distinct = changes.filter(
      (c, i) => changes.findIndex((d) => d.from.speed_kmh === c.from.speed_kmh && d.from.incline_pct === c.from.incline_pct) === i,
    );
    const short = distinct.filter((d) => d.kept < 0.995);
    const hint = short.length
      ? short.map((d) => `The deck tops out at ${maxPct.toFixed(1)}%, so the ${d.from.speed_kmh.toFixed(1)} km/h steps keep ${Math.floor(d.kept * 100)}% of their effort.`)
      : ["The new incline keeps the same effort by the ACSM walking equation."];
    return [
      {
        program_id: p.id,
        summary: changes.length === 1 ? `1 step runs above ${name}.` : `${plural(changes.length)} run above ${name}.`,
        message: [
          `FIT_TO_DECK sets ${plural(changes.length)} of ${p.name} to ${name}.`,
          ...distinct.map((d) => `${d.from.speed_kmh.toFixed(1)} km/h at ${d.from.incline_pct.toFixed(1)}% becomes ${fit.toFixed(1)} km/h at ${d.toPct.toFixed(1)}%.`),
        ].join("\n"),
        hint: [...hint, "FIT_TO_DECK never starts the program."].join("\n"),
        done: `${p.name} now runs at ${name} with more incline.`,
      },
    ];
  });
}

/** The belt speed the scenario runs at, faster than its 4.5 km/h fit. */
const BELT_KMH = 5;

export const strideFit: Scenario = {
  apply() {
    const flag = params.get("stridefit");
    if (flag === null) return;
    const kmh = Number(flag || "4.5");
    if (Number.isFinite(kmh) && kmh > 0) snapshot.deck.stride_fit_kmh = inDeckRange(kmh);
    if (snapshot.deck.status === "Running") {
      snapshot.deck.speed_kmh = BELT_KMH;
      snapshot.deck.target_speed_kmh = BELT_KMH;
    }
    snapshot.deck.above_stride_fit = above();
    snapshot.fit_to_deck = fitOffers();
  },
  tick() {
    const next = above();
    const offers = fitOffers();
    if (next === snapshot.deck.above_stride_fit && JSON.stringify(offers) === JSON.stringify(snapshot.fit_to_deck)) return false;
    snapshot.deck.above_stride_fit = next;
    snapshot.fit_to_deck = offers;
    return true;
  },
  commands: {
    stride_fit_set: (c) => {
      if (c.kmh === null) {
        snapshot.deck.stride_fit_kmh = null;
        return;
      }
      if (!Number.isFinite(c.kmh) || c.kmh <= 0) throw "STRIDE_FIT must be a speed above 0 km/h.";
      snapshot.deck.stride_fit_kmh = inDeckRange(c.kmh);
    },
    program_fit_to_deck: (c) => {
      const p = snapshot.programs.find((x) => x.id === c.id);
      const fit = snapshot.deck.stride_fit_kmh;
      if (!p || fit === null || !fitOffers().some((o) => o.program_id === c.id)) {
        throw "FIT_TO_DECK only fits a seeded program you haven't edited.";
      }
      p.steps = fitSteps(p.steps, fit, snapshot.deck.max_incline_pct).fitted;
      snapshot.fit_to_deck = fitOffers();
    },
  },
};
