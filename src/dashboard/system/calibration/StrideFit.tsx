import { useState } from "react";
import { dispatch } from "../../../ipc";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import { useAppState } from "../../../state/useAppState";
import { PLAIN } from "../../../names";
import { GhostButton, Hint, Label, NeonButton, PlainLine } from "../../ui";
import { cx } from "../../util";
import { INPUT_CLASS } from "../fields";

/** Below this the belt is still, so there is no current speed to take. Rust's `STILL_KMH`. */
const STILL_KMH = 0.1;

/**
 * STRIDE_FIT: the fastest speed (calibrated km/h) the user's walking stride
 * still fits the deck, set by feel and kept by Rust for each treadmill
 * (`snapshot.deck.stride_fit_kmh`). No sourced rule ties deck length to
 * stride (#223), so nothing here works a speed out from the belt loop.
 * The program editor flags steps above it.
 */
export function StrideFit() {
  const fit = useAppState((s) => s.deck.stride_fit_kmh) ?? null;
  const save = useAction();
  // Keyed by the saved speed, so the field shows Rust's value (held to the range) after a save.
  return (
    <div className="hacker-panel space-y-2 rounded-lg border-neon-cyan/20 bg-white/[0.02] p-3">
      <div>
        <Label className="text-neon-yellow">STRIDE_FIT</Label>
        <PlainLine text={PLAIN.STRIDE_FIT} />
      </div>
      <Hint>Fastest speed your walking stride still fits the deck, set by feel.</Hint>
      <Hint>Program steps faster than STRIDE_FIT get a flag in the editor.</Hint>
      <StrideFitForm key={fit ?? "off"} fit={fit} save={save} />
      <ActionStatus pending={save.pending} error={save.error} done={save.done} />
    </div>
  );
}

function StrideFitForm({ fit, save }: { fit: number | null; save: ReturnType<typeof useAction> }) {
  const [value, setValue] = useState(fit === null ? "" : fit.toFixed(1));
  const speed = useAppState((s) => s.deck.speed_kmh) ?? 0;
  const moving = speed >= STILL_KMH;
  const submit = () => {
    if (value.trim() === "" || save.pending) return;
    void save.run(() => dispatch({ type: "stride_fit_set", kmh: Number(value) }), { done: "STRIDE_FIT saved." });
  };
  return (
    <div className="flex flex-wrap items-end gap-2">
      <label className="block">
        <Label>VELOCITY</Label>
        <input
          type="number"
          min={0}
          step={0.1}
          value={value}
          placeholder="off"
          onChange={(e) => setValue(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && submit()}
          className={cx(INPUT_CLASS, "mt-1 block w-24")}
          aria-label="STRIDE_FIT speed in km/h"
        />
      </label>
      <span className="pb-2 font-mono text-xs text-white/60">km/h</span>
      <NeonButton className="px-3 py-1.5" disabled={save.pending || value.trim() === ""} onClick={submit}>
        SAVE
      </NeonButton>
      {moving && (
        <GhostButton title="Fill in the belt's speed now" onClick={() => setValue(speed.toFixed(1))}>
          USE CURRENT
        </GhostButton>
      )}
      {fit !== null && (
        <GhostButton
          disabled={save.pending}
          onClick={() =>
            void save.run(() => dispatch({ type: "stride_fit_set", kmh: null }), { done: "STRIDE_FIT cleared." })
          }
        >
          CLEAR
        </GhostButton>
      )}
    </div>
  );
}
