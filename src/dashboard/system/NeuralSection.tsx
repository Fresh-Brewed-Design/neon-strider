import { SETTING_LIMITS, type NeuralSensitivity } from "../../ipc";
import { useAppSnapshot } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { Hint, Label } from "../ui";
import { cx } from "../util";
import { ChoiceGroup, Section, SliderField, Toggle } from "./fields";
import { InputMonitoringNote } from "./PermissionsSection";

const SENSITIVITY_CHOICES: { value: NeuralSensitivity; label: string; hint: string }[] = [
  { value: "low", label: "Low", hint: "Eases off only during long, fast typing." },
  { value: "medium", label: "Medium", hint: "Eases off part-way at a steady typing pace, fully only when typing hard." },
  { value: "high", label: "High", hint: "Eases off sooner and further for the same amount of typing." },
];

/** What Neural flow is doing right now. Its own component so snapshot ticks do not re-render the section. */
function NeuralLive() {
  const s = useAppSnapshot();
  if (!s) return null;
  const { neural, deck } = s;
  const running = deck.status === "Running";
  const pct = Math.round(neural.factor * 100);
  const state = !neural.enabled
    ? "Off"
    : neural.active
      ? `${pct}% · ${neural.target_kmh.toFixed(1)} of ${neural.base_kmh.toFixed(1)} km/h`
      : running
        ? "Full speed"
        : "Waiting for a walk";
  return (
    <div>
      <div className="mb-1 flex items-baseline justify-between gap-3">
        <Label>Right now</Label>
        <span className={cx("font-digital text-sm", neural.active ? "text-emerald-400" : "text-white/60")}>{state}</span>
      </div>
      <div className="h-1.5 overflow-hidden rounded-full border border-white/10 bg-white/5">
        <div
          className="h-full bg-emerald-400 shadow-[0_0_8px_#34d399] transition-[width]"
          style={{ width: `${neural.enabled ? pct : 0}%` }}
        />
      </div>
      {neural.enabled && running && (
        <Hint className="mt-1">
          Typing pace: about {Math.round(neural.keys_per_min)} key presses a minute.
        </Hint>
      )}
    </div>
  );
}

function SensitivityChoice() {
  const value = useTreadmillStore((s) => s.neuralSensitivity);
  const hint = SENSITIVITY_CHOICES.find((c) => c.value === value)?.hint;
  return (
    <div className="flex flex-col gap-1.5">
      <Label>Sensitivity</Label>
      <ChoiceGroup
        label="Sensitivity"
        options={SENSITIVITY_CHOICES}
        value={value}
        onChange={(neuralSensitivity) => useTreadmillStore.getState().updateSettings({ neuralSensitivity })}
      />
      {hint && <Hint>{hint}</Hint>}
    </div>
  );
}

/** Typing-aware pace: on/off, how far it may slow the belt, and how readily. */
export function NeuralSection() {
  const enabled = useTreadmillStore((s) => s.focusStabilityEnabled);
  const floor = useTreadmillStore((s) => s.neuralFloorPct);
  const { updateSettings } = useTreadmillStore.getState();

  return (
    <Section
      id="sys-neural"
      title="Neural flow"
      description="Eases the belt off while you type a lot and brings it back once you stop."
    >
      <Toggle
        label="Neural flow"
        checked={enabled}
        onChange={(v) => updateSettings({ focusStabilityEnabled: v })}
        description="Works on any walk, set by hand or by a program."
        hint={[
          "Counts key presses only, never which keys.",
          "Easing starts after about 10 s of steady typing.",
          "Typing less brings the speed back within about 20 s and full speed returns a minute after typing stops.",
          "A speed you set yourself becomes the new starting point.",
        ]}
      />
      {enabled && (
        <>
          <InputMonitoringNote />
          <NeuralLive />
          <SliderField
            label="Slowest speed"
            tone="green"
            min={SETTING_LIMITS.neural_floor_pct.min}
            max={SETTING_LIMITS.neural_floor_pct.max}
            step={1}
            value={floor}
            format={(v) => `${v.toFixed(0)}% of your speed`}
            hint="How far Neural flow can ease the belt, reached only during intense typing."
            onCommit={(v) => updateSettings({ neuralFloorPct: v })}
          />
          <SensitivityChoice />
        </>
      )}
    </Section>
  );
}
