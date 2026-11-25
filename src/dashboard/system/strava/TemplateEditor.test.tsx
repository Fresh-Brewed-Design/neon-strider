import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { TEMPLATE_FIELDS, type StravaSnapshot } from "../../../ipc";
import { useTreadmillStore } from "../../../store/treadmillStore";
import { TemplateEditor } from "./TemplateEditor";

const templateSaves = () =>
  vi
    .mocked(invoke)
    .mock.calls.filter(([cmd, a]) => cmd === "dispatch" && (a as { command: { type: string } }).command.type === "strava_set_templates");

const strava = {
  title_template: "NEON STRIDER // {distance_km} KM UPLINK",
  rollup_title_template: "",
  description_template: "",
  daily_rollup: false,
} as StravaSnapshot;

describe("TemplateEditor help", () => {
  it("lists every template field Rust renders, with what it shows", () => {
    const { container } = render(<TemplateEditor strava={strava} />);
    const list = container.querySelector<HTMLElement>('dl[aria-label="Template fields"]');
    if (!list) throw new Error("no field list");
    const terms = within(list).getAllByRole("term").map((t) => t.textContent);
    expect(terms).toEqual(TEMPLATE_FIELDS.map((f) => `{${f.name}}`));
    for (const f of TEMPLATE_FIELDS) expect(within(list).getByText(f.about)).toBeInTheDocument();
    expect(screen.getByText(/no value for a walk is left out/)).toBeInTheDocument();
  });

  it("covers the stats a walk saves", () => {
    const names = TEMPLATE_FIELDS.map((f) => f.name);
    for (const name of [
      "distance_km",
      "duration",
      "avg_speed",
      "max_speed",
      "avg_incline",
      "max_incline",
      "climb_m",
      "calories",
      "steps",
      "avg_heart_rate",
      "max_heart_rate",
      "program",
      "pc_walked",
      "walks",
      "date",
      "time",
      "end_time",
    ]) {
      expect(names).toContain(name);
    }
  });
});

describe("TemplateEditor reset", () => {
  it("asks before putting the default templates back", async () => {
    vi.mocked(invoke).mockClear();
    render(<TemplateEditor strava={strava} />);
    fireEvent.click(screen.getByRole("button", { name: "Reset to default" }));
    expect(templateSaves()).toHaveLength(0);
    const ask = useTreadmillStore.getState().pendingConfirmation;
    expect(ask).toMatchObject({
      message: "Your own title and description templates are replaced by the defaults.",
      confirmLabel: "Reset",
      tone: "danger",
    });
    await act(async () => ask?.onConfirm());
    expect(templateSaves()).toHaveLength(1);
  });
});
