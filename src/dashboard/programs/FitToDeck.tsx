import { ActionStatus } from "../../components/ActionStatus";
import { useAction } from "../../components/useAction";
import { dispatch, type FitToDeckOffer } from "../../ipc";
import { useTreadmillStore } from "../../store/treadmillStore";
import { GhostButton } from "../ui";

/**
 * FIT_TO_DECK on a seeded program's card (#225): Rust offers it for an
 * unedited seeded program with a step above STRIDE_FIT
 * (`snapshot.fit_to_deck`) and writes every line shown. The user always
 * confirms first; the save never starts the program. Mounted on every card,
 * so the saved line stays under the card once Rust drops the offer.
 */
export function FitToDeck({ offer }: { offer: FitToDeckOffer | null }) {
  const action = useAction();
  const confirm = useTreadmillStore((s) => s.confirm);
  if (!offer && !action.pending && !action.error && !action.done) return null;
  const ask = (o: FitToDeckOffer) =>
    confirm({
      title: "FIT_TO_DECK",
      message: o.message,
      hint: o.hint,
      confirmLabel: "Fit to deck",
      onConfirm: () => void action.run(() => dispatch({ type: "program_fit_to_deck", id: o.program_id }), { done: o.done }),
    });
  return (
    <div className="mt-1 space-y-1">
      {offer && (
        <div className="flex flex-wrap items-center gap-2">
          <GhostButton
            className="shrink-0 border border-neon-yellow/40 text-neon-yellow"
            disabled={action.pending}
            onClick={() => ask(offer)}
          >
            FIT_TO_DECK
          </GhostButton>
          <span className="min-w-0 font-mono text-[11px] text-neon-yellow/80">{offer.summary}</span>
        </div>
      )}
      <ActionStatus pending={action.pending} error={action.error} done={action.done} />
    </div>
  );
}
