import { useCallback, useEffect, useRef, useState } from "react";
import { ChevronLeft, ChevronRight, Play, Square } from "lucide-react";
import { GhostButton, Label, NeonButton } from "../ui";
import { cx } from "../util";
import { ActionStatus } from "../../components/ActionStatus";
import { INPUT_CLASS } from "./fields";
import { DEFAULT_TEMPLATE, fillTemplate, frameBody, hex, parseTemplate } from "./rawFrame";

/** The shortest step the cycler allows, so a typo cannot flood the treadmill. */
const MIN_DELAY_MS = 200;

/**
 * Protocol discovery tool: fills the `XX` slot of a body template with each
 * byte in a range, one step at a time or on a timer, so you can watch what the
 * treadmill does. Rust frames and sends it; the readout shows the full frame.
 *
 * `onSend` resolves true once the body went out (after any confirm), false if
 * nothing was sent; auto-step stops then.
 */
export function CommandCycler({ onSend }: { onSend: (body: number[]) => Promise<boolean> }) {
  const [templateText, setTemplateText] = useState(DEFAULT_TEMPLATE);
  const [startByte, setStartByte] = useState(0x00);
  const [endByte, setEndByte] = useState(0x20);
  const [currentByte, setCurrentByte] = useState(0x00);
  const [delay, setDelay] = useState(2000);
  const [delayText, setDelayText] = useState<string | null>(null);
  const [isRunning, setIsRunning] = useState(false);

  const parsed = parseTemplate(templateText);
  const template = "template" in parsed ? parsed.template : null;

  // The auto-step loop reads the latest values without restarting.
  const latest = useRef({ onSend, template, currentByte, startByte, endByte, delay });
  useEffect(() => {
    latest.current = { onSend, template, currentByte, startByte, endByte, delay };
  });

  const stepNext = useCallback(() => {
    setCurrentByte((prev) => (prev >= endByte ? startByte : prev + 1));
  }, [endByte, startByte]);

  const stepPrev = useCallback(() => {
    setCurrentByte((prev) => (prev <= startByte ? endByte : prev - 1));
  }, [endByte, startByte]);

  // Send the current byte, wait one full delay, step, repeat. The wait starts
  // only once the send is done, so a confirm dialog doesn't let bytes pile up.
  useEffect(() => {
    if (!isRunning) return;
    let stopped = false;
    let timer: number | undefined;
    const tick = async (byte: number) => {
      const { onSend: sendNow, template: t } = latest.current;
      const sent = t !== null && (await sendNow(fillTemplate(t, byte)));
      if (stopped) return;
      if (!sent) return setIsRunning(false);
      timer = window.setTimeout(() => {
        const { startByte: first, endByte: last } = latest.current;
        const next = byte >= last ? first : byte + 1;
        setCurrentByte(next);
        void tick(next);
      }, latest.current.delay);
    };
    void tick(latest.current.currentByte);
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [isRunning]);

  const frame = template && frameBody(fillTemplate(template, currentByte));
  const slot = template ? template.indexOf(null) + 1 : -1;

  return (
    <div className="flex flex-col gap-3">
      <label className="block">
        <Label>Body (hex, XX cycles)</Label>
        <input
          type="text"
          value={templateText}
          spellCheck={false}
          aria-invalid={!template}
          onChange={(e) => {
            setIsRunning(false);
            setTemplateText(e.target.value);
          }}
          className={cx(INPUT_CLASS, "mt-1 block w-full uppercase")}
        />
        <ActionStatus error={"error" in parsed ? parsed.error : null} className="mt-1" />
      </label>

      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0 font-digital text-sm tracking-[0.12em] text-neon-magenta/60" aria-label="Frame to send">
          {frame
            ? frame.map((b, i) => (
                <span key={i} className={cx("mr-1.5 inline-block", i === slot && "text-neon-magenta")}>
                  {hex(b)}
                </span>
              ))
            : "--"}
        </div>
        <div className="flex shrink-0 items-center gap-1">
          <GhostButton aria-label="Previous byte" onClick={() => { setIsRunning(false); stepPrev(); }}>
            <ChevronLeft size={16} />
          </GhostButton>
          <NeonButton
            tone="magenta"
            className="px-3 py-1.5"
            disabled={!template}
            onClick={() => {
              setIsRunning(false);
              if (template) void onSend(fillTemplate(template, currentByte));
            }}
          >
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
            min={MIN_DELAY_MS}
            value={delayText ?? delay}
            onChange={(e) => {
              setDelayText(e.target.value);
              const n = parseInt(e.target.value);
              if (n >= MIN_DELAY_MS) setDelay(n);
            }}
            onBlur={() => {
              const n = parseInt(delayText ?? "");
              if (delayText !== null) setDelay(Math.max(MIN_DELAY_MS, Number.isFinite(n) ? n : 1000));
              setDelayText(null);
            }}
            className={cx(INPUT_CLASS, "mt-1 block w-24")}
          />
        </label>
        <NeonButton
          tone="magenta"
          disabled={!template && !isRunning}
          onClick={() => setIsRunning(!isRunning)}
          className={cx("ml-auto flex items-center gap-2 px-4 py-2", isRunning && "motion-safe:animate-pulse")}
        >
          {isRunning ? <Square size={14} fill="currentColor" /> : <Play size={14} fill="currentColor" />}
          {isRunning ? "Stop stepping" : "Auto-step"}
        </NeonButton>
      </div>
    </div>
  );
}
