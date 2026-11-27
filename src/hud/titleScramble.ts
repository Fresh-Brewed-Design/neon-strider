export const TITLE = "NEON STRIDER";
/** Frames before the first letter settles; each later letter settles one frame after the one before. */
export const SCRAMBLE_LEAD = 4;

/**
 * Advance widths in px at the logo's 11 px bold Orbitron (canvas measureText at
 * 110 px, divided by 10, in the mocked HUD preview), for the title's letters and
 * every glyph the scramble may draw. Letter spacing adds the same amount to each
 * glyph, so these compare as they are at either spacing (0.28em, or 0.14em while
 * the link is down).
 */
export const ADVANCE: Readonly<Record<string, number>> = {
  N: 9.152, E: 8.426, O: 9.108, S: 9.086, T: 8.349, R: 9.075, I: 2.354, D: 9.174,
  A: 9.196, B: 9.152, C: 9.042, F: 7.953, G: 9.13, H: 9.361, J: 8.58, K: 8.767, L: 8.569,
  M: 10.208, P: 8.701, Q: 9.724, U: 9.108, V: 11.033, W: 12.969, X: 8.932, Y: 8.866, Z: 9.031,
  "0": 9.174, "1": 4.301, "2": 9.13, "3": 9.086, "4": 8.03, "5": 9.13, "6": 9.02, "7": 7.26,
  "8": 9.174, "9": 9.108, "#": 8.767, "/": 5.731, "<": 5.203, ">": 5.225, _: 9.108, "|": 2.354,
};
/**
 * Each noise glyph is clipped to the cell of the letter it covers (hud.css), so it must be no
 * wider than that letter. Per title letter: every glyph in ADVANCE no
 * wider than it. Wide ones (V, W, M, %, &) are left out; the I (2.354 px) can only be
 * covered by I or |.
 */
const NOISE_FOR: Readonly<Record<string, string>> = Object.fromEntries(
  Array.from(new Set(TITLE.replace(" ", "")), (letter) => [
    letter,
    Object.keys(ADVANCE).filter((g) => ADVANCE[g] <= ADVANCE[letter]).join(""),
  ]),
);
/** The title at `frame`: settled letters left of the edge, noise to its right; spaces stay spaces. */
export function scrambled(frame: number): string {
  return Array.from(TITLE, (ch, i) => {
    if (ch === " " || frame >= SCRAMBLE_LEAD + i) return ch;
    const pool = NOISE_FOR[ch];
    return pool[Math.floor(Math.random() * pool.length)];
  }).join("");
}
