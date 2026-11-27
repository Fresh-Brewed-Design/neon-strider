import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { WAKE_FIRST, WAKE_FIRST_SUB } from "./status";

describe("the deck-asleep wording", () => {
  it("is one sentence in the HUD and in Rust's refusal, naming no button", () => {
    const link = readFileSync("src-tauri/src/link.rs", "utf8");
    expect(link).toContain(`pub const START_STANDBY: &str = "${WAKE_FIRST}";`);
    expect(WAKE_FIRST.toLowerCase()).toContain(WAKE_FIRST_SUB.replace("deck", "the deck"));
    expect(WAKE_FIRST).not.toMatch(/engage|\bGO\b|press/i);
  });
});
