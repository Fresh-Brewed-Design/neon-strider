import { act, fireEvent, render, screen } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SETTING_LIMITS, type AppSnapshot } from "../../ipc";

let current: AppSnapshot | null = null;
vi.mock("../../state/useAppState", () => ({
  useAppSnapshot: () => current,
  useAppState: <T,>(select: (s: AppSnapshot) => T) => (current ? select(current) : null),
}));

const { snapshot: mock } = await import("../../dev/mockBackend");
const { DailyQuotaSetting } = await import("./DailyQuotaSetting");

const LABEL = "DAILY_QUOTA: Daily km target";
const dispatched = () => vi.mocked(invoke).mock.calls.filter(([cmd]) => cmd === "dispatch").map(([, a]) => a);

beforeEach(() => {
  vi.mocked(invoke).mockClear();
  current = structuredClone(mock);
  current.daily_quota = { target_km: null, fraction: 0, left_km: 0, message: "" };
});

describe("System, DAILY_QUOTA", () => {
  it("is off by default, with no target field", () => {
    render(<DailyQuotaSetting />);
    expect(screen.getByRole("switch", { name: LABEL })).toHaveAttribute("aria-checked", "false");
    expect(screen.queryByRole("spinbutton")).toBeNull();
  });

  it("switches on at Rust's default target", async () => {
    render(<DailyQuotaSetting />);
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: LABEL })));
    expect(dispatched()).toEqual([{ command: { type: "daily_quota_set", km: SETTING_LIMITS.default_daily_quota_km } }]);
  });

  it("shows the target and switches off with none", async () => {
    current!.daily_quota = { target_km: 7.5, fraction: 0.5, left_km: 3.75, message: "" };
    render(<DailyQuotaSetting />);
    expect(screen.getByRole("spinbutton")).toHaveValue(7.5);
    expect(screen.getByText(`${SETTING_LIMITS.daily_quota_km.min} to ${SETTING_LIMITS.daily_quota_km.max} km. A new day starts the count at midnight.`)).toBeInTheDocument();
    await act(async () => fireEvent.click(screen.getByRole("switch", { name: LABEL })));
    expect(dispatched()).toEqual([{ command: { type: "daily_quota_set", km: null } }]);
  });

  it("saves a typed target to a tenth of a km", async () => {
    current!.daily_quota = { target_km: 5, fraction: 0, left_km: 5, message: "" };
    render(<DailyQuotaSetting />);
    const field = screen.getByRole("spinbutton");
    fireEvent.change(field, { target: { value: "7.25" } });
    await act(async () => fireEvent.blur(field));
    expect(dispatched()).toEqual([{ command: { type: "daily_quota_set", km: 7.3 } }]);
  });
});
