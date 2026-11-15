import { describe, expect, it } from "vitest";
import { snapshot } from "../dev/mockBackend";
import type { StravaActivity, StravaSnapshot } from "../ipc";
import { activityText, stravaPill } from "./stravaStatus";

const strava = (activity: StravaActivity, patch: Partial<StravaSnapshot> = {}): StravaSnapshot => ({
  ...snapshot.strava,
  activity,
  ...patch,
});

describe("activityText", () => {
  it("describes each step", () => {
    expect(activityText({ state: "idle" })).toBeNull();
    expect(activityText({ state: "authorizing", expires_in_s: 252 })).toBe(
      "Waiting for you to approve in the browser (4:12 left)",
    );
    expect(activityText({ state: "uploading", label: "2.40 km", step: "sending" })).toBe("Sending to Strava… 2.40 km");
    expect(activityText({ state: "uploading", label: "", step: "processing" })).toBe("Strava is processing…");
    expect(activityText({ state: "backing_off", retry_in_s: 600, reason: "No internet." })).toContain("10:00");
    expect(activityText({ state: "queued", count: 3 })).toBe("3 uploads waiting…");
  });
});

describe("stravaPill", () => {
  it("ranks activity over the stored result", () => {
    expect(stravaPill(strava({ state: "uploading", label: "", step: "sending" })).tone).toBe("working");
    expect(stravaPill(strava({ state: "paused", reason: "x" })).label).toBe("PAUSED");
    expect(stravaPill(strava({ state: "idle" })).label).toBe("LINKED");
    const failed = strava({ state: "idle" }, { last_result: { ok: false, message: "No.", at_ms: 1, hint: "none" } });
    expect(stravaPill(failed)).toMatchObject({ tone: "problem", label: "FAILED", title: "No." });
    expect(stravaPill(strava({ state: "idle" }, { connected: false })).tone).toBe("idle");
  });
});
