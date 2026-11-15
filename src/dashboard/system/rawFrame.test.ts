import { describe, expect, it } from "vitest";
import { DEFAULT_TEMPLATE, fillTemplate, frameBody, parseTemplate, type Template } from "./rawFrame";

const parsed = (text: string): Template => {
  const r = parseTemplate(text);
  if ("error" in r) throw new Error(r.error);
  return r.template;
};
const frame = (text: string, byte: number) => frameBody(fillTemplate(parsed(text), byte));
const zeros = Array<number>(10).fill(0);

describe("command cycler frames", () => {
  it("match the deck's known packets", () => {
    expect(frame(DEFAULT_TEMPLATE, 0x01)).toEqual([0x02, 0x53, 0x01, ...zeros, 0x52, 0x03]); // READY
    expect(frame(DEFAULT_TEMPLATE, 0x09)).toEqual([0x02, 0x53, 0x09, ...zeros, 0x5a, 0x03]); // START
    expect(frame(DEFAULT_TEMPLATE, 0x03)).toEqual([0x02, 0x53, 0x03, ...zeros, 0x50, 0x03]); // STOP
    expect(frame(DEFAULT_TEMPLATE, 0x0a)).toEqual([0x02, 0x53, 0x0a, ...zeros, 0x59, 0x03]); // PAUSE
    expect(frame("XX", 0x51)).toEqual([0x02, 0x51, 0x51, 0x03]); // poll
  });

  it("cycle any one position", () => {
    expect(frame("53 02 xx 00", 35)).toEqual([0x02, 0x53, 0x02, 35, 0x00, 0x53 ^ 0x02 ^ 35, 0x03]);
  });
});

describe("parseTemplate", () => {
  it("explains what is wrong", () => {
    expect(parseTemplate("  ")).toEqual({ error: "Enter hex bytes." });
    expect(parseTemplate("53 0C")).toEqual({ error: "Mark exactly one byte as XX." });
    expect(parseTemplate("XX XX")).toEqual({ error: "Mark exactly one byte as XX." });
    expect(parseTemplate("53 G1 XX")).toEqual({ error: `"G1" isn't a hex byte.` });
    expect(parseTemplate("530C XX")).toHaveProperty("error");
    expect(parseTemplate(Array(17).fill("00").concat("XX").join(" "))).toEqual({ error: "At most 17 bytes." });
  });

  it("accepts the longest body that fits one write", () => {
    expect(parsed(Array(16).fill("00").concat("XX").join(" "))).toHaveLength(17);
  });
});
