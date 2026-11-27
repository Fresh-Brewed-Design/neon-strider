/** Width of a 12 px monospace character, in px, to size a month name. */
const MONTH_CHAR_PX = 7.5;
/** The least gap kept between two month names, so neighbours never touch. */
const MONTH_GAP_PX = 4;

/** The month names that fit with a gap between them, given the width of one week's column. */
export function fittingMonths<T extends { name: string; col: number }>(months: T[], colPx: number): T[] {
  if (colPx <= 0) return months;
  let freeFrom = -Infinity;
  return months.filter((m) => {
    if (m.col * colPx < freeFrom) return false;
    freeFrom = m.col * colPx + m.name.length * MONTH_CHAR_PX + MONTH_GAP_PX;
    return true;
  });
}
