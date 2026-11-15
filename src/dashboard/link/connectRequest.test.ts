import { describe, expect, it } from "vitest";
import { isNewConnectRequest } from "./connectRequest";

describe("isNewConnectRequest", () => {
  it("follows each request once", () => {
    expect(isNewConnectRequest(null, null)).toBe(false);
    expect(isNewConnectRequest(1_000, null)).toBe(true);
    expect(isNewConnectRequest(1_000, "1000")).toBe(false);
    expect(isNewConnectRequest(1_001, "1000")).toBe(true);
    expect(isNewConnectRequest(null, "1000")).toBe(false);
  });
});
