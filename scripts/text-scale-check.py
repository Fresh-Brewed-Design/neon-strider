"""Checks every dashboard page at the Windows text sizes for sideways overflow and clipped text.

Needs Python Playwright (pip install playwright; playwright install chromium) and a Vite
server: npx vite --port 5192 --strictPort. Then: python scripts/text-scale-check.py
Optional arguments: a base URL (default http://localhost:5192), a folder to save
760x600 screenshots of every view into (or "-" for none), and the text sizes to check
(default 100,150,200,225). Exits 1 when any view overflows.

Each view is opened in the 760x600 dashboard preview with ?textscale=<n>, the page zoom
WebView2 applies for that Windows text size (src/dev/textScale.ts). A view passes when the
document doesn't scroll sideways, no text runs past the window or out of a box that hides
its overflow, no word wraps one letter per line, and no two elements' text overlap.
It is not a project dependency, like scripts/screenshots.py.
"""
import sys
from pathlib import Path
from playwright.sync_api import sync_playwright

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:5192"
SHOTS = Path(sys.argv[2]) if len(sys.argv) > 2 and sys.argv[2] != "-" else None
SCALES = [int(v) for v in sys.argv[3].split(",")] if len(sys.argv) > 3 else [100, 150, 200, 225]

# (name, nav button, preview flags, then an optional press on the page: a button's name,
# "expand" for the first expandable row, or "tab:<name>" for one of System's group tabs)
VIEWS = [
    ("live", "Live", "", None),
    ("live-map", "Live", "program=interval", None),
    ("live-offline", "Live", "offline", None),
    ("live-guided", "Live", "guided&noprogram", None),
    ("programs", "Programs", "stridefit=4.5", None),
    ("programs-lastrun", "Programs", "noprogram", None),
    ("programs-editor", "Programs", "", "New"),
    ("history", "History", "", None),
    ("history-walk", "History", "heart=on", "expand"),
    ("hud", "HUD", "", None),
    ("system", "System", "", "tab:DECK"),
    ("system-links", "System", "", "tab:LINKS"),
    ("system-routine", "System", "", "tab:AUTOMATION"),
    ("system-core", "System", "update=available", "tab:CORE"),
    ("system-calibration", "System", "calwizard=guidedone", "tab:DECK"),
]

# Every text node: past the window's sides, cut by an ancestor that hides overflow, or
# wrapped one letter per line. Text inside a visually hidden accessible label is skipped.
PROBE = r"""() => {
  const doc = document.documentElement;
  const vw = doc.clientWidth;
  const out = { scrollWidth: doc.scrollWidth, clientWidth: vw, problems: [] };
  const walker = document.createTreeWalker(document.getElementById("root"), NodeFilter.SHOW_TEXT);
  const seen = new Set();
  const lines = [];
  for (let n = walker.nextNode(); n; n = walker.nextNode()) {
    const text = n.textContent.trim();
    if (!text) continue;
    const el = n.parentElement;
    const cs = getComputedStyle(el);
    if (cs.visibility === "hidden" || cs.display === "none") continue;
    // A visually hidden label (clip-path: inset(50%)) is read out, never seen.
    let hiddenLabel = false;
    for (let a = el; a && !hiddenLabel; a = a.parentElement) hiddenLabel = getComputedStyle(a).clipPath.startsWith("inset(50%");
    if (hiddenLabel) continue;
    // A separator that would open a wrapped line is clipped on purpose (Joined in src/dashboard/ui.tsx).
    if (el.hasAttribute("data-joined-sep")) continue;
    const range = document.createRange();
    range.selectNodeContents(n);
    const rects = [...range.getClientRects()].filter((r) => r.width > 0);
    if (!rects.length) continue;
    const left = Math.min(...rects.map((r) => r.left));
    const right = Math.max(...rects.map((r) => r.right));
    const label = text.slice(0, 40);
    // Text on a floating layer (a sticky header or a toast) sits over scrolled content on purpose.
    let floating = false;
    for (let a = el; a && a !== document.body; a = a.parentElement) {
      const s = getComputedStyle(a);
      if ((s.position === "fixed" || s.position === "sticky") && !a.classList.contains("dash-root")) floating = true;
    }
    // Compared in the coordinates of the scrolling box that holds the text, so a header that
    // stays put never counts as lying over content scrolled beneath it.
    let box = null;
    for (let a = el; a && a !== document.body && !box; a = a.parentElement) {
      const oy = getComputedStyle(a).overflowY;
      if ((oy === "auto" || oy === "scroll") && a.scrollHeight > a.clientHeight) box = a;
    }
    const dy = box ? box.scrollTop - box.getBoundingClientRect().top : 0;
    if (!floating) for (const r of rects) lines.push({ el, label, box, r: { left: r.left, right: r.right, top: r.top + dy, bottom: r.bottom + dy, height: r.height } });
    const report = (kind) => {
      const key = kind + label;
      if (!seen.has(key)) { seen.add(key); out.problems.push(kind + ": " + label); }
    };
    if (right > vw + 0.5 || left < -0.5) report("off the window");
    // Clipped: an ancestor that hides horizontal overflow and doesn't hold the text.
    for (let a = el; a && a !== document.body; a = a.parentElement) {
      const s = getComputedStyle(a);
      if (s.overflowX === "visible") continue;
      const r = a.getBoundingClientRect();
      if (right > r.right + 1 || left < r.left - 1) { report("clipped (" + s.overflowX + ")"); break; }
    }
    // One letter per line: a word of 4+ letters spread over as many lines as it has letters / 2.
    const longest = Math.max(...text.split(/\s+/).map((w) => w.length));
    const rows = new Set(rects.map((r) => Math.round(r.top))).size;
    if (longest >= 4 && rows >= 3 && rows * 2 >= longest + text.split(/\s+/).length) report("letter per line");
  }
  // Overlap: lines of text from two different elements that share more than a quarter of the
  // smaller line's height and more than 2 px of width (a History row's stats under its buttons).
  for (let i = 0; i < lines.length; i++) {
    for (let j = i + 1; j < lines.length; j++) {
      const a = lines[i], b = lines[j];
      if (a.box !== b.box || a.el === b.el || a.el.contains(b.el) || b.el.contains(a.el)) continue;
      const w = Math.min(a.r.right, b.r.right) - Math.max(a.r.left, b.r.left);
      const h = Math.min(a.r.bottom, b.r.bottom) - Math.max(a.r.top, b.r.top);
      if (w > 2 && h > 0.25 * Math.min(a.r.height, b.r.height)) {
        const key = "overlap" + a.label + b.label;
        if (!seen.has(key)) { seen.add(key); out.problems.push("overlaps: " + a.label + " / " + b.label); }
      }
    }
  }
  return out;
}"""


def open_view(page, scale, nav, flags, press):
    query = f"?textscale={scale}" + (f"&{flags}" if flags else "")
    page.goto(f"{BASE}/dashboard-preview.html{query}")
    page.wait_for_selector("nav[aria-label='Sections']")
    page.locator("nav[aria-label='Sections'] button", has_text=nav).first.click()
    if press and press.startswith("tab:"):
        page.get_by_role("radio", name=press[4:]).click()
    elif press == "expand":
        page.locator("button[aria-expanded]").first.click()
    elif press:
        page.get_by_role("button", name=press).first.click()
    page.evaluate("document.fonts.ready")
    page.wait_for_timeout(400)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    failed = 0
    with sync_playwright() as p:
        browser = p.chromium.launch()
        ctx = browser.new_context(viewport={"width": 760, "height": 600}, device_scale_factor=2)
        page = ctx.new_page()
        for scale in SCALES:
            for name, nav, flags, press in VIEWS:
                open_view(page, scale, nav, flags, press)
                res = page.evaluate(PROBE)
                sideways = res["scrollWidth"] > res["clientWidth"]
                ok = not sideways and not res["problems"]
                failed += 0 if ok else 1
                status = "ok" if ok else "FAIL"
                print(f"{scale:>3} % {name:<16} scrollWidth {res['scrollWidth']:>4} / {res['clientWidth']:>4}  {status}")
                for prob in res["problems"][:60]:
                    print(f"        {prob}")
                if SHOTS:
                    SHOTS.mkdir(parents=True, exist_ok=True)
                    page.screenshot(path=str(SHOTS / f"{name}-{scale}.png"))
        ctx.close()
        browser.close()
    print(f"{failed} view(s) failed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
