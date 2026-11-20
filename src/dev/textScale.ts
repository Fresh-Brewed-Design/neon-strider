// ?textscale=110|125 (dev previews only, never the real app): approximates the Windows text
// size setting by zooming the page's content. In the HUD preview, size the browser window to the
// real window's width (340 px) to see how the content copes.
export function applyTextScale() {
  const scale = Number(new URLSearchParams(location.search).get("textscale"));
  if (scale === 110 || scale === 125) document.documentElement.style.zoom = String(scale / 100);
}
