import type { ReactNode } from "react";
import { Swap } from "./Swap";

/** The HUD's small text button on cards, toasts and the program row. */
export function SmallButton({
  onClick,
  children,
  className,
  sub,
  subs,
  title,
  disabled = false,
}: {
  onClick: () => void;
  children: ReactNode;
  /** Extra classes, e.g. `hud-btn-go` for the emphasised GO. */
  className?: string;
  /** A plain lowercase line under the name, for a button whose codename needs explaining. */
  sub?: string;
  /** Every line `sub` takes in turn: they share one cell, so the button keeps one size. */
  subs?: readonly string[];
  title?: string;
  disabled?: boolean;
}) {
  const cls = `hud-btn hud-btn-sm${sub ? " hud-btn-stack" : ""}${className ? ` ${className}` : ""}`;
  return (
    <button type="button" className={cls} title={title} disabled={disabled} onClick={onClick}>
      {children}
      {sub && " "}
      {sub && (subs ? <Swap className="hud-btn-sub" shown={sub} all={subs} /> : <span className="hud-btn-sub">{sub}</span>)}
    </button>
  );
}
