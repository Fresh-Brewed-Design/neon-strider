import { parseBlocks, parseInline, type HeadingLevel } from "./parse";

function InlineText({ text }: { text: string }) {
  return (
    <>
      {parseInline(text).map((r, i) =>
        r.kind === "bold" ? (
          <strong key={i} className="font-bold text-white">
            {r.text}
          </strong>
        ) : r.kind === "code" ? (
          <code key={i} className="rounded bg-white/10 px-1 text-neon-cyan">
            {r.text}
          </code>
        ) : (
          <span key={i}>{r.text}</span>
        ),
      )}
    </>
  );
}

const HEADING: Record<HeadingLevel, string> = {
  1: "font-digital text-sm font-bold tracking-[0.12em] text-neon-cyan",
  2: "font-digital text-xs font-bold tracking-[0.15em] text-neon-cyan",
  3: "font-mono text-[11px] font-bold uppercase tracking-[0.2em] text-neon-magenta",
  4: "font-mono text-[11px] font-bold uppercase tracking-[0.15em] text-white/70",
};

/** Release notes rendered as headings, bullets and paragraphs (see `parse.ts`). */
export function Markdown({ text, className = "" }: { text: string; className?: string }) {
  return (
    <div
      className={`flex min-w-0 flex-col gap-2 break-words font-mono text-[11px] leading-relaxed text-white/75 ${className}`}
    >
      {parseBlocks(text).map((b, i) => {
        switch (b.kind) {
          case "heading":
            return (
              <div
                key={i}
                role="heading"
                aria-level={Math.min(6, b.level + 2)}
                className={`${HEADING[b.level]} ${i > 0 ? "mt-1" : ""}`}
              >
                <InlineText text={b.text} />
              </div>
            );
          case "list":
            return (
              <ul key={i} className="flex list-disc flex-col gap-1 pl-4 marker:text-neon-cyan/60">
                {b.items.map((item, j) => (
                  <li key={j}>
                    <InlineText text={item} />
                  </li>
                ))}
              </ul>
            );
          case "rule":
            return <hr key={i} className="border-white/10" />;
          default:
            return (
              <p key={i}>
                <InlineText text={b.text} />
              </p>
            );
        }
      })}
    </div>
  );
}
