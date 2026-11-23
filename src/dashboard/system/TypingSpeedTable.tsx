import type { TypingSpeedSnapshot } from "../../ipc/bindings/TypingSpeedSnapshot";
import { Hint } from "../ui";
import { bandLabel, barShares, typingSpeedLines } from "./typingSpeedText";

/**
 * Labs, typing by speed: the median key presses per minute in each speed band
 * over recent walks (`snapshot.typing_speed`, Rust's `typing_speed.rs`).
 */
export function TypingSpeedTable({ data }: { data: TypingSpeedSnapshot }) {
  const shares = barShares(data.bands);
  const lines = typingSpeedLines(data);
  return (
    <div className="mt-2">
      {data.bands.length > 0 && (
        <div
          role="table"
          aria-label="Key presses per minute by walking speed"
          className="grid grid-cols-[auto_auto_minmax(0,1fr)] items-center gap-x-4 gap-y-1 font-mono text-[11px]"
        >
          <div role="row" className="contents text-white/45 uppercase tracking-[0.12em]">
            <span role="columnheader">Speed</span>
            <span role="columnheader" className="text-right">
              Minutes walked
            </span>
            <span role="columnheader">Keys per minute</span>
          </div>
          {data.bands.map((b, i) => (
            <div role="row" key={b.from_kmh} className="contents text-white/80">
              <span role="cell" className="whitespace-nowrap">
                {bandLabel(b)}
              </span>
              <span role="cell" className="text-right tabular-nums">
                {b.minutes}
              </span>
              <span role="cell" className="flex min-w-0 items-center gap-2">
                <span className="w-8 shrink-0 text-right tabular-nums">{b.median_kpm}</span>
                <span className="h-1.5 min-w-0 flex-1 rounded-full bg-white/10">
                  <span
                    className="block h-full rounded-full bg-neon-cyan/70"
                    style={{ width: `${Math.round(shares[i] * 100)}%` }}
                  />
                </span>
              </span>
            </div>
          ))}
        </div>
      )}
      {lines.map((line) => (
        <Hint key={line} className="mt-1">
          {line}
        </Hint>
      ))}
    </div>
  );
}
