// The command cycler's body template. Rust frames what is sent
// (`protocol::raw_packet`); this mirrors it only to show the frame.

/** Same limit as Rust's `RAW_BODY_MAX`: framed, it still fits one 20-byte BLE write. */
export const RAW_BODY_MAX = 17;
export const DEFAULT_TEMPLATE = "53 XX 00 00 00 00 00 00 00 00 00 00";

/** A body with one cycling slot (`null`). */
export type Template = (number | null)[];

/** Reads `53 XX 00 ...`: hex pairs, exactly one `XX`. */
export function parseTemplate(text: string): { template: Template } | { error: string } {
  const parts = text.trim().split(/\s+/).filter(Boolean);
  if (parts.length === 0) return { error: "Enter hex bytes." };
  if (parts.length > RAW_BODY_MAX) return { error: `At most ${RAW_BODY_MAX} bytes.` };
  const bad = parts.find((p) => !/^([0-9a-f]{2}|xx)$/i.test(p));
  if (bad) return { error: `"${bad}" isn't a hex byte.` };
  const slots = parts.filter((p) => p.toLowerCase() === "xx").length;
  if (slots !== 1) return { error: "Mark exactly one byte as XX." };
  return { template: parts.map((p) => (p.toLowerCase() === "xx" ? null : parseInt(p, 16))) };
}

/** The body with the cycling byte filled in. */
export const fillTemplate = (template: Template, byte: number): number[] => template.map((b) => b ?? byte);

/** `02, body, XOR of the body, 03`: the bytes Rust will write. */
export const frameBody = (body: number[]): number[] => [0x02, ...body, body.reduce((a, b) => a ^ b, 0), 0x03];

export const hex = (n: number) => n.toString(16).padStart(2, "0").toUpperCase();
