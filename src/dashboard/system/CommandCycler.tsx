import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Play, Square } from "lucide-react";
import { GhostButton, Label, NeonButton } from "../ui";
import { cx } from "../util";
import { INPUT_CLASS } from "./fields";

const hex = (n: number) => n.toString(16).padStart(2, "0").toUpperCase();
const command = (byte: number) => [0x02, 0x53, 0x0c, byte];

/**
 * Protocol discovery tool: sends `02 53 0C XX` for a range of XX, one step at a
 * time or on a timer, so you can watch what the treadmill does. Sends real
 * bytes to the connected machine.
 */
export function CommandCycler({ onSend }: { onSend: (cmd: number[]) => void }) {
  const [startByte, setStartByte] = useState(0x00);
  const [endByte, setEndByte] = useState(0x20);
  const [currentByte, setCurrentByte] = useState(0x00);
  const [delay, setDelay] = useState(2000);
  const [isRunning, setIsRunning] = useState(false);

  // The auto-step effect below must not re-run (and re-send) when the parent re-renders.
  const send = useRef(onSend);
  useEffect(() => {
    send.current = onSend;
  });

  const stepNext = useCallback(() => {
    setCurrentByte((prev) => (prev >= endByte ? startByte : prev + 1));
  }, [endByte, startByte]);

  const stepPrev = useCallback(() => {
    setCurrentByte((prev) => (prev <= startByte ? endByte : prev - 1));
  }, [endByte, startByte]);

  useEffect(() => {
    if (!isRunning) return;
    const timer = window.setInterval(stepNext, delay);
    return () => clearInterval(timer);
  }, [isRunning, delay, stepNext]);

  // While auto-stepping, every new byte (and the moment it starts) is sent.
  useEffect(() => {
    if (isRunning) send.current(command(currentByte));
  }, [currentByte, isRunning]);

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <div className="font-digital text-2xl tracking-[0.2em] text-neon-magenta">02 53 0C {hex(currentByte)}</div>
        <div className="flex items-center gap-1">
          <GhostButton aria-label="Previous byte" onClick={() => { setIsRunning(false); stepPrev(); }}>
            <ChevronLeft size={16} />
          </GhostButton>
          <NeonButton tone="magenta" className="px-3 py-1.5" onClick={() => { setIsRunning(false); onSend(command(currentByte)); }}>
            Send once
          </NeonButton>
          <GhostButton aria-label="Next byte" onClick={() => { setIsRunning(false); stepNext(); }}>
            <ChevronRight size={16} />
          </GhostButton>
        </div>
      </div>

      <div className="flex flex-wrap items-end gap-4">
        <label className="block">
          <Label>First byte (hex)</Label>
          <input
            type="text"
            value={hex(startByte)}
            onChange={(e) => setStartByte(parseInt(e.target.value, 16) || 0)}
            className={cx(INPUT_CLASS, "mt-1 block w-20")}
          />
        </label>
        <label className="block">
          <Label>Last byte (hex)</Label>
          <input
            type="text"
            value={hex(endByte)}
            onChange={(e) => setEndByte(parseInt(e.target.value, 16) || 0)}
            className={cx(INPUT_CLASS, "mt-1 block w-20")}
          />
        </label>
        <label className="block">
          <Label>Step every (ms)</Label>
          <input
            type="number"
            value={delay}
            onChange={(e) => setDelay(parseInt(e.target.value) || 1000)}
            className={cx(INPUT_CLASS, "mt-1 block w-24")}
          />
        </label>
        <NeonButton
          tone="magenta"
          onClick={() => setIsRunning(!isRunning)}
          className={cx("ml-auto flex items-center gap-2 px-4 py-2", isRunning && "animate-pulse")}
        >
          {isRunning ? <Square size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
          {isRunning ? "Stop stepping" : "Auto-step"}
        </NeonButton>
      </div>
    </div>
  );
}
