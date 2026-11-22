import type { ReactNode } from "react";

/** The HUD's small text button on cards, toasts and the program row. */
export function SmallButton({
  onClick,
  children,
  className,
  sub,
  title,
}: {
  onClick: () => void;
  children: ReactNode;
  /** Extra classes, e.g. `hud-btn-go` for the emphasised GO. */
  className?: string;
  /** A plain lowercase line under the name, for a button whose codename needs explaining. */
  sub?: string;
  title?: string;
}) {
  const cls = `hud-btn hud-btn-sm${sub ? " hud-btn-stack" : ""}${className ? ` ${className}` : ""}`;
  return (
    <button type="button" className={cls} title={title} onClick={onClick}>
      {children}
      {sub && " "}
      {sub && <span className="hud-btn-sub">{sub}</span>}
    </button>
  );
}
