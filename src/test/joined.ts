/**
 * A Testing Library matcher for a line drawn by `Joined` (src/dashboard/ui.tsx), whose parts are
 * separate elements: it matches the whole line's text, with no-break spaces read as spaces.
 */
export function joinedLine(text: string) {
  return (_content: string, el: Element | null) =>
    !!el?.hasAttribute("data-joined") && (el.textContent ?? "").replace(/\u00a0/g, " ") === text;
}
