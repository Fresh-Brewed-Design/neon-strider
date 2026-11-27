/**
 * The pink "update waiting" dot. The same dot sits on the System nav and on
 * System's Updates tab; the tray icon carries one too. Rust decides when it
 * shows (`snapshot.update.dot`).
 */
export function UpdateDot({ className = "" }: { className?: string }) {
  return (
    <span
      className={`h-2 w-2 shrink-0 rounded-full bg-neon-cyan shadow-[0_0_6px_#00f3ff] ${className}`}
      role="img"
      aria-label="Update available"
      title="Update available"
    />
  );
}
