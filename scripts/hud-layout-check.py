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
