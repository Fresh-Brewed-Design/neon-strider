import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ChevronDown, ChevronRight } from "lucide-react";
import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { useTreadmillStore } from "../../store/treadmillStore";
import { readPref, writePref } from "../localPrefs";
import { Label, NeonButton } from "../ui";
import { CommandCycler } from "./CommandCycler";
import { DeckProfile } from "./DeckProfile";
import { Section, Toggle } from "./fields";
import { TrafficRecorder } from "./TrafficRecorder";

const DEV_TOOLS_KEY = "system.devTools";
/** One confirm arms raw sending for this long, so auto-step isn't a dialog per byte. */
const RAW_ARMED_MS = 5 * 60 * 1000;

/**
 * The log folder, the traffic recorder, and behind an opt-in, a tool that
 * sends raw protocol bytes.
 */
export function DiagnosticsSection() {
  const [open, setOpen] = useState(false);
  // Raw commands can move the belt, so they sit behind an explicit opt-in that
  // is remembered per machine, plus a confirm before sending.
  const [devTools, setDevTools] = useState(() => readPref(DEV_TOOLS_KEY) === "on");
  const armedUntil = useRef(0);
  const logs = useAction();
  const raw = useAction();

  const toggleDevTools = (on: boolean) => {
    setDevTools(on);
    writePref(DEV_TOOLS_KEY, on ? "on" : "off");
  };

  const runRaw = raw.run;
  const sendRaw = useCallback(
    (body: number[]): Promise<boolean> => {
      const fire = () => {
        void runRaw(() => invoke("send_bluetooth_command", { body }));
        return true;
      };
      if (Date.now() <= armedUntil.current) return Promise.resolve(fire());
      // Only one confirm at a time; a send asked for while it is open is dropped.
      const store = useTreadmillStore.getState();
      if (store.pendingConfirmation) return Promise.resolve(false);
      return new Promise((resolve) =>
        store.confirm({
          title: "Send raw commands",
          message: "Raw commands can start the belt. Step off first. Armed for 5 minutes.",
          onConfirm: () => {
            armedUntil.current = Date.now() + RAW_ARMED_MS;
            resolve(fire());
          },
          onCancel: () => resolve(false),
        }),
      );
    },
    [runRaw],
  );

  return (
    <Section
      id="sys-diagnostics"
      title="Diagnostics"
      description="Neon Strider's log for troubleshooting, plus protocol tools that are rarely needed."
    >
      <div>
        <NeonButton onClick={() => void logs.run(() => invoke("open_log_folder"))} className="px-3 py-1.5">
          Open log folder
        </NeonButton>
        <ActionStatus error={logs.error && `Could not open the log folder: ${logs.error}`} className="mt-1" />
      </div>
      <DeckProfile />
      <button
        type="button"
        aria-expanded={open}
        onClick={() => setOpen(!open)}
        className="flex w-fit items-center gap-2 rounded px-1 py-1 font-mono text-xs font-bold uppercase tracking-[0.12em] text-white/60 transition-colors hover:text-neon-cyan"
      >
        {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        {open ? "Hide diagnostics" : "Show diagnostics"}
      </button>
      {open && (
        <>
          <TrafficRecorder />
          <Toggle
            label="Developer tools"
            description="Show tools that send raw commands to the treadmill."
            checked={devTools}
            onChange={toggleDevTools}
          />
          {devTools && (
            <div>
              <Label className="mb-1 block">Command cycler</Label>
              <p className="mb-3 font-mono text-[11px] leading-relaxed text-white/50">
                Raw commands can start the belt, so step off the treadmill first.
              </p>
              <CommandCycler onSend={sendRaw} />
              <ActionStatus error={raw.error && `Could not send: ${raw.error}`} className="mt-2" />
            </div>
          )}
        </>
      )}
    </Section>
  );
}
