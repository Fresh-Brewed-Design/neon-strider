import { describe, expect, it } from "vitest";
import { hudErrorLine } from "./useHudSend";

describe("hudErrorLine", () => {
  it("turns a rejection into one short themed line", () => {
    expect(hudErrorLine("Treadmill not connected.")).toBe("SYS // TREADMILL NOT CONNECTED");
    expect(hudErrorLine({ kind: "timeout", message: "No  answer\nfrom the deck" })).toBe("SYS // NO ANSWER FROM THE DECK");
  });

  it("cuts long messages so they fit the panel", () => {
    expect(hudErrorLine("x".repeat(80))).toBe(`SYS // ${"X".repeat(55)}…`);
  });
});
