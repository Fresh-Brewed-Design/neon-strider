/** One whole part of a message and the separator after it (" //" or " ·"), if any. */
export interface Segment {
  text: string;
  sep: string;
}

/** "CALL DETECTED // QUIET MODE" as whole parts, each keeping the separator that follows it. */
export function splitSegments(message: string): Segment[] {
  const parts = message.split(/ (\/\/|·) /);
  const out: Segment[] = [];
  for (let i = 0; i < parts.length; i += 2) out.push({ text: parts[i], sep: parts[i + 1] ?? "" });
  return out;
}
