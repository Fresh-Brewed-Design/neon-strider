// ECHO: the History button that saves a walk as a program. The mock builds a
// three-step program from the walk's average speed and adds it to Programs,
// with a suffix when the name is taken. ?echo=short: every walk is refused as
// too short, with Rust's message.
import type { EchoProgram, ProgramStep } from "../../ipc";
import { params, publish, snapshot } from "../mockState";
import { sessions } from "./history";
import type { Scenario } from "./types";

const refuse = params.get("echo") === "short";

/** Like Rust's `echo_name`: the walk's date, and (2), (3)… when taken. */
function echoName(startMs: number): string {
  const d = new Date(startMs);
  const pad = (n: number) => String(n).padStart(2, "0");
  const base = `ECHO // ${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
  const taken = new Set(snapshot.programs.map((p) => p.name.toLowerCase()));
  if (!taken.has(base.toLowerCase())) return base;
  for (let n = 2; ; n += 1) {
    const name = `${base} (${n})`;
    if (!taken.has(name.toLowerCase())) return name;
  }
}

async function programFromSession(args: Record<string, unknown>): Promise<EchoProgram> {
  // 400 ms: long enough to see the button's spinner.
  await new Promise((r) => setTimeout(r, 400));
  if (refuse) throw "ECHO needs a walk of at least 3 minutes.";
  const ids = args.sessionIds as number[];
  const walk = sessions.find((s) => s.id === ids[0]);
  if (!walk) throw "The walk is no longer in History.";
  const minutes = Math.max(3, Math.round(walk.duration_seconds / 60));
  const third = Math.max(1, Math.round(minutes / 3));
  const speed = Math.round(walk.avg_speed * 10) / 10;
  const steps: ProgramStep[] = [
    { duration_minutes: third, speed_kmh: speed, incline_pct: 0 },
    { duration_minutes: minutes - 2 * third, speed_kmh: Math.round((speed + 1) * 10) / 10, incline_pct: walk.avg_incline },
    { duration_minutes: third, speed_kmh: speed, incline_pct: 0 },
  ];
  const id = Math.max(0, ...snapshot.programs.map((p) => p.id)) + 1;
  const name = echoName(walk.start_time);
  snapshot.programs.push({ id, name, steps, repeat_every_minutes: null });
  publish();
  return { id, name, step_count: steps.length };
}

export const echo: Scenario = {
  queries: { program_from_session: programFromSession },
};
