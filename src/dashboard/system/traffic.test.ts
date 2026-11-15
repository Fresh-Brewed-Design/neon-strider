import { describe, expect, it } from "vitest";
import { clock, fileSize, trafficSummary } from "./traffic";

describe("traffic recorder text", () => {
  it("formats elapsed time like the rest of the app", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(42)).toBe("0:42");
    expect(clock(3599)).toBe("59:59");
    expect(clock(3600 + 65)).toBe("1:01:05");
    expect(clock(-5)).toBe("0:00");
  });

  it("formats file sizes", () => {
    expect(fileSize(820)).toBe("820 B");
    expect(fileSize(14 * 1024)).toBe("14 KB");
    expect(fileSize(2.4 * 1024 * 1024)).toBe("2.4 MB");
  });

  it("joins packets, time and size", () => {
    expect(trafficSummary({ packets: 1234, elapsed_s: 42, bytes: 14 * 1024 })).toBe("1,234 packets · 0:42 · 14 KB");
    expect(trafficSummary({ packets: 1, elapsed_s: 1, bytes: 90 })).toBe("1 packet · 0:01 · 90 B");
  });
});
