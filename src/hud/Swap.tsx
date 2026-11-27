/**
 * One wording out of several in a single cell: every wording is laid out on
 * the same spot and only `shown` is visible, so the cell is as big as the
 * biggest wording in every state and nothing beside or under it moves when
 * the wording changes. The hidden wordings are hidden from screen readers too.
 */
export function Swap({
  shown,
  all,
  className,
  tight = [],
}: {
  shown: string;
  all: readonly string[];
  className?: string;
  /** Wordings set a size smaller (never under 11px), so a longer one fits the cell the others make. */
  tight?: readonly string[];
}) {
  const words = all.includes(shown) ? all : [...all, shown];
  return (
    <span className={className ? `hud-swap ${className}` : "hud-swap"}>
      {words.map((w) => (
        <span
          key={w}
          className={[w === shown ? "is-shown" : "", tight.includes(w) ? "is-tight" : ""].filter(Boolean).join(" ") || undefined}
          aria-hidden={w === shown ? undefined : true}
        >
          {w}
        </span>
      ))}
    </span>
  );
}
