import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { ActionStatus } from "../../../components/ActionStatus";
import { useAction } from "../../../components/useAction";
import type { StravaSnapshot } from "../../../ipc";
import { GhostButton, Hint, Label } from "../../ui";
import { cx, post } from "../../util";
import { INPUT_CLASS } from "../fields";

const PLACEHOLDERS = [
  "{distance_km}",
  "{duration}",
  "{avg_speed}",
  "{avg_incline}",
  "{calories}",
  "{steps}",
  "{walks}",
  "{date}",
  "{time}",
];

type Preview = { title: string; rollup_title: string; description: string };

/** The upload title and description templates, with a sample rendered by Rust. */
export function TemplateEditor({ strava }: { strava: StravaSnapshot }) {
  const [draft, setDraft] = useState<{ title?: string; rollup?: string; description?: string }>({});
  const title = draft.title ?? strava.title_template;
  const rollup = draft.rollup ?? strava.rollup_title_template;
  const description = draft.description ?? strava.description_template;
  const [preview, setPreview] = useState<Preview | null>(null);
  const saving = useAction();

  // Rust renders the preview so it can never drift from what is uploaded.
  useEffect(() => {
    let stale = false;
    const timer = setTimeout(() => {
      invoke<Preview>("strava_template_preview", { title, rollupTitle: rollup, description })
        .then((p) => !stale && setPreview(p))
        .catch(() => !stale && setPreview(null));
    }, 150);
    return () => {
      stale = true;
      clearTimeout(timer);
    };
  }, [title, rollup, description]);

  const commit = () => {
    if (draft.title === undefined && draft.rollup === undefined && draft.description === undefined) return;
    void saving.run(post({ type: "strava_set_templates", title, rollup_title: rollup, description }), {
      done: "Saved.",
    });
    setDraft({});
  };

  const reset = () => {
    void saving.run(post({ type: "strava_set_templates", title: "", rollup_title: "", description: "" }), {
      done: "Reset to the default templates.",
    });
    setDraft({});
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex items-center justify-between gap-3">
        <Label>Title and description</Label>
        <GhostButton onClick={reset} disabled={saving.pending}>
          {saving.pending ? "Saving…" : "Reset to default"}
        </GhostButton>
      </div>
      <ActionStatus pending={saving.pending && "Saving…"} error={saving.error} done={saving.done} />

      <label className="block">
        <span className="font-mono text-[11px] text-white/60">Title</span>
        <input
          type="text"
          maxLength={300}
          value={title}
          onChange={(e) => setDraft((d) => ({ ...d, title: e.target.value }))}
          onBlur={commit}
          className={cx(INPUT_CLASS, "mt-1 w-full")}
        />
      </label>
      {strava.daily_rollup && (
        <label className="block">
          <span className="font-mono text-[11px] text-white/60">Daily rollup title</span>
          <input
            type="text"
            maxLength={300}
            value={rollup}
            onChange={(e) => setDraft((d) => ({ ...d, rollup: e.target.value }))}
            onBlur={commit}
            className={cx(INPUT_CLASS, "mt-1 w-full")}
          />
        </label>
      )}
      <label className="block">
        <span className="font-mono text-[11px] text-white/60">Description</span>
        <textarea
          rows={3}
          maxLength={2000}
          value={description}
          onChange={(e) => setDraft((d) => ({ ...d, description: e.target.value }))}
          onBlur={commit}
          className={cx(INPUT_CLASS, "mt-1 w-full resize-none")}
        />
      </label>

      <Hint>
        Placeholders: {PLACEHOLDERS.join(" ")}. Anything else in braces is left as typed. Titles stop at 100
        characters.
      </Hint>

      <div
        aria-label="Preview from a sample walk"
        className="select-text rounded border border-white/10 bg-black/30 p-3 font-mono text-[11px] leading-relaxed"
      >
        <div className="text-white/40">Preview · sample walk</div>
        {preview ? (
          <>
            <div className="mt-1 break-words font-bold text-white/85">{preview.title}</div>
            {strava.daily_rollup && <div className="break-words text-white/60">{preview.rollup_title}</div>}
            <div className="mt-1 whitespace-pre-wrap break-words text-white/60">{preview.description}</div>
          </>
        ) : (
          <div className="mt-1 text-white/35">…</div>
        )}
      </div>
    </div>
  );
}
