import { deckLimitOffer, limitsText, snapToDeck, snapToLevel } from "../../state/deckSpeeds";
import { useAppState } from "../../state/useAppState";
import { useTreadmillStore } from "../../store/treadmillStore";
import { GhostButton, Hint } from "../ui";
import { NumberField, Section, SliderField } from "./fields";

/** Speed and incline limits, incline scaling and body weight. Saved by Rust as part of its config. */
export function TreadmillSection() {
  const minSpeed = useTreadmillStore((s) => s.minSpeed);
  const maxSpeed = useTreadmillStore((s) => s.maxSpeed);
  const deck = useAppState((s) => s.deck);
  const maxIncline = useTreadmillStore((s) => s.maxIncline);
  const tiltMultiplier = useTreadmillStore((s) => s.tiltMultiplier);
  const inclineOffset = useTreadmillStore((s) => s.inclineOffset);
  const bodyMassKg = useTreadmillStore((s) => s.bodyMassKg);
  const { updateSettings } = useTreadmillStore.getState();
  const offer = deck ? deckLimitOffer(deck.profile, minSpeed, maxSpeed) : null;

  return (
    <Section
      id="sys-treadmill"
      title="Treadmill"
      description="Limits and scaling for the treadmill, applied as soon as they change."
    >
      {deck && offer && (
        <div className="flex flex-wrap items-center gap-3">
          <Hint>DECK_PROFILE saw the treadmill run {limitsText(deck.profile)}, inside your speed limits.</Hint>
          <GhostButton onClick={() => updateSettings(offer)}>Use deck limits</GhostButton>
        </div>
      )}
      <div className="grid grid-cols-2 gap-4">
        {/* 0.1 km/h is one deck speed step, the slowest the deck can be asked
            for; 25 km/h matches Top speed's range. */}
        <NumberField
          label="Slowest speed"
          unit="km/h"
          min={0.1}
          max={25}
          step={0.1}
          value={minSpeed}
          hint="The lowest running speed the controls will request, raised to the deck's own floor once DECK_PROFILE has seen it."
          onCommit={(v) => updateSettings({ minSpeed: Math.min(v, maxSpeed) })}
        />
        <NumberField
          label="Top speed"
          unit="km/h"
          min={1}
          max={25}
          step={0.5}
          value={maxSpeed}
          hint="The highest speed the controls will request, kept under the deck's own top speed once DECK_PROFILE has seen it."
          snap={(v) => snapToDeck(v, deck, "hw_kmh")}
          onCommit={(v) => updateSettings({ maxSpeed: v })}
        />
        <NumberField
          label="Highest incline level"
          unit="levels"
          min={0}
          max={20}
          value={maxIncline}
          hint="The treadmill's top incline step, kept at or under the deck's own top level once DECK_PROFILE has seen it."
          snap={(v) => snapToLevel(v, deck)}
          onCommit={(v) => updateSettings({ maxIncline: v })}
        />
        <NumberField
          label="Body weight"
          unit="kg"
          min={30}
          max={250}
          step={0.5}
          value={bodyMassKg}
          hint="Used to estimate calories burned. Stays on this PC."
          onCommit={(v) => updateSettings({ bodyMassKg: v })}
        />
      </div>
      <SliderField
        label="Incline per level"
        tone="magenta"
        min={0.1}
        max={5}
        step={0.1}
        value={tiltMultiplier}
        format={(v) => `1 level = ${v.toFixed(1)}%`}
        hint="Percent of grade for each whole incline step the treadmill reports."
        onCommit={(v) => updateSettings({ tiltMultiplier: v })}
      />
      <SliderField
        label="Incline offset"
        tone="magenta"
        min={-5}
        max={10}
        step={0.5}
        value={inclineOffset}
        format={(v) => `${v > 0 ? "+" : ""}${v.toFixed(1)}%`}
        hint="Corrects incline readings for adjustable feet or a sloping floor without moving the motor's zero point."
        onCommit={(v) => updateSettings({ inclineOffset: v })}
      />
    </Section>
  );
}
