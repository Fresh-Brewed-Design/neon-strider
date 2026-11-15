import { ActionStatus } from "../../../components/ActionStatus";
import { useAction, useBackendAction } from "../../../components/useAction";
import { dispatch } from "../../../ipc";
import { useAppSnapshot } from "../../../state/useAppState";
import { useTreadmillStore } from "../../../store/treadmillStore";
import { GhostButton, Hint, Label, NeonButton } from "../../ui";
import { Section } from "../fields";
import { Markdown } from "./Markdown";
import { formatPublished, installConfirmText, installProgress, updateBusy } from "./updates";

function formatChecked(ms: number | null): string {
  if (ms === null) return "not yet";
  return new Date(ms).toLocaleString(undefined, { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

/**
 * System -> Updates: the running version, Check now, and any update on offer
 * with its notes, Install and Not now. Background check failures show here
 * only; they never interrupt.
 */
export function UpdatesSection() {
  const s = useAppSnapshot();
  const u = s?.update;
  const status = u?.status.state;
  const check = useBackendAction(u?.last_result ?? null, status === "checking");
  const install = useBackendAction(
    u?.last_result ?? null,
    status === "downloading" || status === "installing",
  );
  const notNow = useAction();
  if (!s || !u) return null;

  const busy = updateBusy(u);
  const offer = u.offer;
  const progress = installProgress(u);
  const published = formatPublished(offer?.published ?? null);
  // A background check's failure: shown here, quietly, when no button owns it.
  const backgroundFailure = u.status.state === "failed" && !check.error && !install.error ? u.status.message : null;

  const startInstall = () => {
    if (!offer) return;
    useTreadmillStore.getState().confirm({
      title: `Install v${offer.version}`,
      message: installConfirmText(s, offer.version),
      onConfirm: () => void install.run(() => dispatch({ type: "update_install" })),
    });
  };

  return (
    <Section
      id="sys-updates"
      title="Updates"
      description="Neon Strider looks for a new version on GitHub when it starts and once a day. Updates are signed, and installing one restarts the app."
      status={
        <span className="shrink-0 font-mono text-[11px] font-bold uppercase tracking-[0.18em] text-white/50">
          {offer && u.status.state !== "up_to_date" ? (
            <span className="text-neon-magenta">UPDATE READY // v{offer.version}</span>
          ) : (
            <>BUILD // v{u.current_version}</>
          )}
        </span>
      }
    >
      <div className="flex flex-col gap-1.5">
        <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
          <NeonButton
            className="px-4 py-2"
            disabled={busy || check.pending}
            onClick={() => void check.run(() => dispatch({ type: "update_check" }))}
          >
            Check now
          </NeonButton>
          <Hint>
            Installed v{u.current_version} · last checked {formatChecked(u.last_checked_ms)}
          </Hint>
        </div>
        <ActionStatus pending={check.pending && "Checking GitHub…"} error={check.error} done={check.done} />
        {backgroundFailure && <ActionStatus ambient error={`Last automatic check: ${backgroundFailure}`} />}
      </div>

      {offer && (
        <div className="flex flex-col gap-2 rounded border border-neon-magenta/30 bg-neon-magenta/5 p-3">
          <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
            <span className="font-digital text-xs font-bold tracking-[0.15em] text-neon-magenta">
              v{offer.version} // AVAILABLE
            </span>
            {published && <Label className="tracking-[0.12em]">Released {published}</Label>}
          </div>
          {u.notes_warning && <Hint>{u.notes_warning}</Hint>}
          <div className="max-h-56 overflow-y-auto pr-1" aria-label={`Release notes for v${offer.version}`}>
            <Markdown text={offer.notes} />
          </div>
          <div className="flex flex-wrap items-center gap-2 pt-1">
            <NeonButton tone="magenta" className="px-4 py-2" disabled={busy} onClick={startInstall}>
              Install
            </NeonButton>
            {!u.dismissed && (
              <GhostButton disabled={busy} onClick={() => void notNow.run(() => dispatch({ type: "update_dismiss" }))}>
                Not now
              </GhostButton>
            )}
            {u.dismissed && <Hint>Put off until tomorrow&apos;s check.</Hint>}
          </div>
          <ActionStatus pending={progress ?? (install.pending && "Starting…")} error={install.error ?? notNow.error} />
        </div>
      )}
    </Section>
  );
}
