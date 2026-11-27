"""Checks that the HUD keeps its controls in place across states at the Windows text sizes.

Needs Python Playwright (pip install playwright; playwright install chromium) and a Vite
server: npx vite --port 5192 --strictPort. Then: python scripts/hud-layout-check.py
Optional arguments: a base URL (default http://localhost:5192) and the text sizes to check
(default 100,110,125,135,150,175,200,225). Exits 1 when a check fails.

Each state is opened in the 340 px wide HUD preview with ?textscale=<n> (src/dev/textScale.ts):
- the header has one height in every link and belt state, in the whole panel, the strip and
  the running strip (#420);
- ENGAGE has one place in the strip whatever its stat slot shows: TODAY, a program's rest, a
  folded ATROPHY_WARNING or the STASIS clock (#436);
- after a refused press on the strip, its note stays under the header and beside ENGAGE, and
  ENGAGE stays where it was (#435).
- the strip and the running strip never scroll sideways, every button stays inside the panel
  and keeps its words inside itself, and the program line hides whole lines, never half of
  one (#443).
It is not a project dependency, like scripts/screenshots.py.
"""
import sys
from playwright.sync_api import sync_playwright

BASE = sys.argv[1] if len(sys.argv) > 1 else "http://localhost:5192"
SCALES = [int(v) for v in sys.argv[2].split(",")] if len(sys.argv) > 2 else [100, 110, 125, 135, 150, 175, 200, 225]

# Every word the header's chip takes in the preview.
HEADER_STATES = {
    "RUNNING": "",
    "SPIN-UP": "status=countdown",
    "SPIN-DOWN": "status=stopping",
    "STASIS": "program=paused",
    "GO wait": "program=ready",
    "STANDBY": "idle",
    "DECK OFF": "status=standby",
    "STATE ?9": "status=9",
    "RELINK": "link=reconnecting",
    "LINKING": "link=connecting",
    "NO_CARRIER": "link=failed",
    "NO SIGNAL": "link=nodata",
    "SCANNING": "link=scanning",
    "BT OFF": "link=bt-off",
}
VIEWS = {"panel": "", "strip": "&stasis=strip", "running strip": "&minified"}

STRIP_STATES = {
    "TODAY": "stasis=strip&idle",
    "TODAY 88.88": "stasis=strip&idle&daily=88.88&dailytime=659",
    "rest": "stasis=strip&program=resting",
    "ATROPHY_WARNING": "stasis=strip&idle&seated=folded",
    "STASIS clock": "stasis=strip&deck=paused&stasisclock=3909",
}

FIT_STATES = {
    "strip": "stasis=strip&idle",
    "strip rest": "stasis=strip&program=resting",
    "strip STASIS clock": "stasis=strip&deck=paused&stasisclock=3909",
    "running strip": "minified",
    "running strip program": "minified&program=running",
    "running strip paused": "minified&program=paused",
}
# Sideways scroll, a button past the panel or its words past the button, a text line cut across the
# program box's foot. Returns the problems found.
FIT = """() => {
  const bad = [];
  const body = document.querySelector('.hud-body');
  if (body.scrollWidth > body.clientWidth + 0.5) bad.push('body scrolls sideways ' + body.scrollWidth + ' > ' + body.clientWidth);
  const panel = document.querySelector('.hud-panel').getBoundingClientRect();
  for (const b of document.querySelectorAll('.hud-strip .hud-btn')) {
    const r = b.getBoundingClientRect();
    const name = b.textContent.trim().slice(0, 14);
    if (r.left < panel.left - 0.5 || r.right > panel.right + 0.5) bad.push('button ' + name + ' leaves the panel');
    for (const t of b.querySelectorAll('.is-shown')) {
      const q = t.getBoundingClientRect();
      if (q.left < r.left - 0.5 || q.right > r.right + 0.5 || t.scrollWidth > t.clientWidth + 1) bad.push('button ' + name + ' clips its words');
    }
  }
  const prog = document.querySelector('.hud-run-program');
  if (prog) {
    const box = prog.getBoundingClientRect();
    for (const el of prog.querySelectorAll('.hud-label, .hud-run-seg, .hud-run-clock')) {
      const q = el.getBoundingClientRect();
      if (q.height > 0 && q.top < box.bottom - 1 && q.bottom > box.bottom + 1) bad.push('program line cut: ' + el.textContent.trim().slice(0, 12));
    }
  }
  return bad;
}"""

HEADER = "() => Math.round(document.querySelector('.hud-header').getBoundingClientRect().height)"
ENGAGE = """() => {
  // A press focuses ENGAGE, which may scroll the body sideways where the row is too narrow for it.
  document.querySelector('.hud-body').scrollLeft = 0;
  const p = document.querySelector('.hud-panel').getBoundingClientRect();
  const e = document.querySelector('.hud-strip .hud-btn-main').getBoundingClientRect();
  return [Math.round(e.left - p.left), Math.round(e.top - p.top)];
}"""
NOTE = """() => {
  const h = document.querySelector('.hud-header').getBoundingClientRect();
  const n = document.querySelector('.hud-note-pop');
  const e = document.querySelector('.hud-strip .hud-btn-main').getBoundingClientRect();
  if (!n) return null;
  const r = n.getBoundingClientRect();
  return { underHeader: r.top >= h.bottom - 0.5, besideEngage: r.right <= e.left + 0.5 || r.bottom <= e.top + 0.5 };
}"""


def open_view(page, scale, query):
    page.goto(f"{BASE}/hud-preview.html?textscale={scale}&{query}")
    page.wait_for_selector(".hud-header")
    page.evaluate("document.fonts.ready")
    page.wait_for_timeout(300)


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    failed = 0
    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": 340, "height": 900})
        for scale in SCALES:
            for view, extra in VIEWS.items():
                heights = {}
                for name, query in HEADER_STATES.items():
                    open_view(page, scale, query + extra)
                    heights[name] = page.evaluate(HEADER)
                ok = len(set(heights.values())) == 1
                failed += 0 if ok else 1
                print(f"{scale:>3} % header ({view}): {'ok' if ok else 'FAIL'} {sorted(set(heights.values()))}")
                if not ok:
                    print(f"        {heights}")

            places = {}
            for name, query in STRIP_STATES.items():
                open_view(page, scale, query)
                places[name] = tuple(page.evaluate(ENGAGE))
            ok = len(set(places.values())) == 1
            failed += 0 if ok else 1
            print(f"{scale:>3} % strip ENGAGE: {'ok' if ok else 'FAIL'} {sorted(set(places.values()))}")
            if not ok:
                print(f"        {places}")

            for name, query in FIT_STATES.items():
                open_view(page, scale, query)
                bad = page.evaluate(FIT)
                failed += 0 if not bad else 1
                print(f"{scale:>3} % fit ({name}): {'ok' if not bad else 'FAIL'} {bad or ''}")

            open_view(page, scale, STRIP_STATES["TODAY"] + "&reject")
            before = page.evaluate(ENGAGE)
            page.locator(".hud-strip .hud-btn-main").first.click()
            page.wait_for_timeout(400)
            # The longest note a refusal gives (useHudSend's MAX_CHARS).
            page.evaluate("() => { const t = document.querySelector('.hud-note-text'); if (t) t.textContent = 'SYS // ' + 'WAKE THE DECK FIRST AT THE TREADMILL OR REMOTE TO GO ON'; }")
            page.wait_for_timeout(100)
            note = page.evaluate(NOTE)
            ok = note is not None and note["underHeader"] and note["besideEngage"] and page.evaluate(ENGAGE) == before
            failed += 0 if ok else 1
            print(f"{scale:>3} % strip note: {'ok' if ok else 'FAIL'} {note}")
        browser.close()
    print(f"{failed} check(s) failed")
    sys.exit(1 if failed else 0)


if __name__ == "__main__":
    main()
