import { render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { TEMPLATE_FIELDS, type StravaSnapshot } from "../../../ipc";
import { TemplateEditor } from "./TemplateEditor";

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
