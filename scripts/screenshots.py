"""Regenerates docs/img/*.png from the mocked dashboard and HUD previews.

Needs Python Playwright (pip install playwright; playwright install chromium) and a Vite
server: npx vite --port 5192 --strictPort. Then: python scripts/screenshots.py
It is not a project dependency; the images are committed.
"""
import sys
from pathlib import Path
from playwright.sync_api import sync_playwright

BASE = "http://localhost:5192"
OUT = Path(__file__).resolve().parent.parent / "docs" / "img"
OUT.mkdir(parents=True, exist_ok=True)


def settle(page, ms=1500):
    page.evaluate("document.fonts.ready")
    page.wait_for_timeout(ms)


def dashboard(browser, name, nav=None, scroll_to=None, tab=None, query=""):
    ctx = browser.new_context(viewport={"width": 760, "height": 600}, device_scale_factor=2)
    page = ctx.new_page()
    page.goto(f"{BASE}/dashboard-preview.html{query}")
    page.wait_for_selector("nav[aria-label='Sections']")
    if nav:
        page.locator("nav[aria-label='Sections'] button", has_text=nav).click()
    if tab:
        page.get_by_role('button', name=tab).or_(page.get_by_role('tab', name=tab)).first.click()
    settle(page)
    if tab:
        # Tabs scroll their section flush under the header; back off a little so the card's top edge shows.
        page.evaluate("()=>{const e=document.querySelector('div.overflow-y-auto');e.scrollTop-=24}")
        settle(page, 500)
    if scroll_to:
        page.get_by_text(scroll_to, exact=False).first.scroll_into_view_if_needed()
        page.evaluate(
            "(t)=>{const e=[...document.querySelectorAll('h1,h2,h3,h4,summary,button,div,span')]"
            ".find(x=>x.children.length===0&&x.textContent.trim().toUpperCase().includes(t.toUpperCase()));"
            "if(e){let p=e;while(p&&p.scrollHeight<=p.clientHeight)p=p.parentElement;"
            "if(p)p.scrollTop=e.getBoundingClientRect().top-p.getBoundingClientRect().top+p.scrollTop-16;}}",
            scroll_to,
        )
        settle(page, 600)
    page.screenshot(path=str(OUT / f"{name}.png"))
    ctx.close()


def hud(browser, name, query=""):
    ctx = browser.new_context(viewport={"width": 700, "height": 1000}, device_scale_factor=2)
    page = ctx.new_page()
    page.goto(f"{BASE}/hud-preview.html{query}")
    page.wait_for_timeout(500)
    # Dark neutral backdrop instead of the preview's gradient.
    page.add_style_tag(content="body{background:#14161a!important} body>div:first-child{background:#14161a!important}")
    settle(page, 2500)
    box = page.evaluate(
        "()=>{const els=[...document.querySelectorAll('#root *')].filter(e=>{const r=e.getBoundingClientRect();"
        "return r.width>50&&r.height>20&&r.width<420});let x0=1e9,y0=1e9,x1=0,y1=0;"
        "for(const e of els){const r=e.getBoundingClientRect();x0=Math.min(x0,r.left);y0=Math.min(y0,r.top);"
        "x1=Math.max(x1,r.right);y1=Math.max(y1,r.bottom)}return {x:x0,y:y0,w:x1-x0,h:y1-y0}}"
    )
    pad = 12
    clip = {"x": max(box["x"] - pad, 0), "y": max(box["y"] - pad, 0), "width": box["w"] + 2 * pad, "height": box["h"] + 2 * pad}
    page.screenshot(path=str(OUT / f"{name}.png"), clip=clip)
    ctx.close()


with sync_playwright() as p:
    browser = p.chromium.launch()
    only = set(sys.argv[1:])
    jobs = {
        "dashboard-live": lambda: dashboard(browser, "dashboard-live"),
        "dashboard-programs": lambda: dashboard(browser, "dashboard-programs", nav="Programs"),
        "dashboard-history": lambda: dashboard(browser, "dashboard-history", nav="History"),
        "dashboard-system": lambda: dashboard(browser, "dashboard-system", nav="System", tab="Strava"),
        "hud-program": lambda: hud(browser, "hud-program"),
        "hud-walkaway": lambda: hud(browser, "hud-walkaway", "?walkaway=warning"),
        "hud-neural": lambda: hud(browser, "hud-neural", "?neural=active"),
    }
    for k, f in jobs.items():
        if not only or k in only:
            f()
            print("captured", k)
    browser.close()
