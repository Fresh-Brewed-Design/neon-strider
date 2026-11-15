import type { ReactNode } from "react";

/** The HUD's small text button on cards, toasts and the program row. */
export function SmallButton({
  onClick,
  children,
  className,
}: {
  onClick: () => void;
  children: ReactNode;
  /** Extra classes, e.g. `hud-btn-go` for the emphasised GO. */
  className?: string;
}) {
  return (
    <button type="button" className={`hud-btn hud-btn-sm${className ? ` ${className}` : ""}`} onClick={onClick}>
      {children}
    </button>
  );
}
