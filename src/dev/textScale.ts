// ?textscale=121 (dev previews only, never the real app): the Windows "Text size" setting
// (Settings > Accessibility > Text size, stored as a percentage in TextScaleFactor).
//
// WebView2 applies that setting as a page zoom on top of the display scale, the same as the
// browser's own zoom: boxes and text grow together. Measured in the HUD window at 150 % display
// scale and 121 % text size: devicePixelRatio 1.815 (1.5 x 1.21), and the 340 px window (510
// physical px) lays out 281 CSS px wide, so the panel's 300 px column gets about 250 px.
// Zooming the whole page by the same factor in a 340 px wide preview gives the same layout
// width, which is what decides whether a readout fits.
//
// Windows offers 100 to 225 % in 1 % steps, so any whole number in that range is accepted;
// anything else leaves the page at 100 %.
export const TEXT_SCALE_MIN = 100;
export const TEXT_SCALE_MAX = 225;

/** The zoom factor for a `?textscale=` value, or null for none or one Windows doesn't offer. */
export function textScaleZoom(flag: string | null): number | null {
  if (flag === null || !/^\d+$/.test(flag)) return null;
  const pct = Number(flag);
  if (pct < TEXT_SCALE_MIN || pct > TEXT_SCALE_MAX) return null;
  return pct / 100;
}

// In the HUD preview, size the browser window to the real window's width (340 px).
export function applyTextScale() {
  const zoom = textScaleZoom(new URLSearchParams(location.search).get("textscale"));
  if (zoom !== null && zoom !== 1) document.documentElement.style.zoom = String(zoom);
}
