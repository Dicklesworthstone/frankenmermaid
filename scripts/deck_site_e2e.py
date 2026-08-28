#!/usr/bin/env python3
"""Headless site-level drive of the showcase #deck section and the fm-cli talk.html page.

Complements scripts/deck_runtime_e2e.py (fixture-scoped runtime contract): this suite
drives the REAL pages end to end — index.html's lazy-mounted deck through the shipped
pkg/ WASM build, the same section at a phone viewport with touch, and the committed
flowchart_subgraphs.talk.html golden as a living page. Checks: mount, morph arming,
live-edge tracking, push-out exile vs at-home members, mobile tap navigation and the
top-anchored slide card, and talk.html animation + keyboard. 23 checks; exit 0 = all pass.

By default it serves the repo root locally. Set DECK_E2E_URL to drive a deployed site
instead (e.g. DECK_E2E_URL=https://frankenmermaid.pages.dev); the talk.html section is
skipped for remote targets, which do not ship the golden.

OPT-IN tooling, not a default gate: requires a Python with playwright + a downloaded
chromium (any venv works: /path/to/venv/bin/python scripts/deck_site_e2e.py). The default
quality gates stay browser-free by design; scripts/verify_deck_runtime.mjs carries the
dependency-free guards.
"""
try:
    from playwright.sync_api import sync_playwright  # noqa: F401
except ImportError:
    import sys
    print("SKIP: playwright not available in this Python; see module docstring")
    sys.exit(0)

import functools
import os
import sys
import threading
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

from playwright.sync_api import sync_playwright

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

base_url = os.environ.get("DECK_E2E_URL", "").rstrip("/")
server = None
if not base_url:
    Handler = functools.partial(SimpleHTTPRequestHandler, directory=ROOT)
    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    base_url = f"http://127.0.0.1:{server.server_address[1]}"

checks = []


def check(name, ok, detail=""):
    checks.append((name, ok))
    print(("PASS " if ok else "FAIL ") + name + ((" — " + str(detail)) if detail and not ok else ""))


def push_mag(transform):
    try:
        parts = transform.replace("translate(", "").replace(")", "").split()
        return abs(float(parts[0])) + abs(float(parts[1]))
    except Exception:
        return -1.0


def open_deck(page):
    page.goto(base_url + "/index.html" if server else base_url + "/")
    page.wait_for_timeout(1500)
    page.eval_on_selector("#deck", "el => el.scrollIntoView()")
    page.wait_for_timeout(5000)  # lazy IntersectionObserver mount + WASM render + push easing


with sync_playwright() as p:
    browser = p.chromium.launch()

    # ── Desktop showcase deck ────────────────────────────────────────
    page = browser.new_page(viewport={"width": 1400, "height": 900})
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    open_deck(page)

    check("desktop: no page errors", not errors, errors[:2])
    check("desktop: deck mounted", page.locator("#deck-stage .fm-deck-viewport").count() == 1)
    check("desktop: morphing armed", page.eval_on_selector(
        "#deck-stage .fm-deck-viewport", "el => el.classList.contains('fm-deck-morphing')"))
    live = page.locator("#deck-stage .fm-deck-live path").count()
    check("desktop: live edge layer populated", live > 0, live)

    tf1 = page.eval_on_selector(
        "#deck-stage .fm-deck-viewport svg g[id^='fm-node-']",
        "el => el.getAttribute('transform') || ''")
    page.wait_for_timeout(700)
    tf2 = page.eval_on_selector(
        "#deck-stage .fm-deck-viewport svg g[id^='fm-node-']",
        "el => el.getAttribute('transform') || ''")
    check("desktop: nodes animating between frames", tf1 != "" and tf1 != tf2, (tf1, tf2))

    d1 = page.eval_on_selector("#deck-stage .fm-deck-live path", "el => el.getAttribute('d')")
    page.wait_for_timeout(700)
    d2 = page.eval_on_selector("#deck-stage .fm-deck-live path", "el => el.getAttribute('d')")
    check("desktop: live edges tracking endpoints", bool(d1) and d1 != d2, (d1, d2))

    mags = page.eval_on_selector_all(
        "#deck-stage .fm-deck-viewport svg g[id^='fm-node-']",
        "els => els.map(el => { const t = el.getAttribute('transform') || 'translate(0 0)';"
        " const m = t.match(/translate\\(([-\\d.]+) ([-\\d.]+)\\)/);"
        " return m ? Math.abs(parseFloat(m[1])) + Math.abs(parseFloat(m[2])) : 0; })")
    check("desktop: some nodes pushed far out (exiles)", max(mags) > 150, max(mags))
    check("desktop: some nodes near home (members)", min(mags) < 30, min(mags))
    check("desktop: no page errors at end", not errors, errors[:2])
    page.close()

    # ── Mobile showcase deck (phone viewport, touch) ─────────────────
    page = browser.new_page(
        viewport={"width": 390, "height": 844},
        device_scale_factor=3,
        is_mobile=True,
        has_touch=True,
    )
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    open_deck(page)

    check("mobile: no page errors", not errors, errors[:2])
    check("mobile: deck mounted", page.locator("#deck-stage .fm-deck-viewport").count() == 1)
    check("mobile: morphing armed", page.eval_on_selector(
        "#deck-stage .fm-deck-viewport", "el => el.classList.contains('fm-deck-morphing')"))
    scale = page.eval_on_selector(
        "#deck-stage .fm-deck-viewport",
        "el => { const m = el.style.transform.match(/scale\\(([\\d.]+)\\)/);"
        " return m ? parseFloat(m[1]) : 0; }")
    check("mobile: camera scale sane (not microscopic)", 0.15 < scale <= 1.6, scale)
    page.locator("#deck-next").tap()
    page.wait_for_timeout(700)
    page.locator("#deck-next").tap()
    page.wait_for_timeout(1200)
    num = page.inner_text("#deck-num").strip()
    check("mobile: tap-Next advances", num.startswith("02") or "1/" in num, num)
    # The fixed bottom dock covers the stage's lower edge on phones; the slide card is
    # top-anchored there so title and caption stay readable.
    card_top = page.eval_on_selector(
        "#deck-card",
        "el => el.getBoundingClientRect().top - "
        "document.querySelector('#deck-stage').getBoundingClientRect().top")
    check("mobile: slide card anchored at stage top", 0 <= card_top < 80, card_top)
    card_width = page.eval_on_selector("#deck-card", "el => el.getBoundingClientRect().width")
    check("mobile: slide card fits viewport", 0 < card_width <= 390, card_width)
    check("mobile: no page errors at end", not errors, errors[:2])
    page.close()

    # ── Standalone talk.html (fm-cli deck output, committed golden) ──
    if server:
        page = browser.new_page(viewport={"width": 1280, "height": 800})
        errors = []
        page.on("pageerror", lambda e: errors.append(str(e)))
        page.goto(base_url + "/crates/fm-cli/tests/golden/deck/flowchart_subgraphs.talk.html")
        page.wait_for_timeout(2500)
        check("talk.html: no page errors", not errors, errors[:2])
        check("talk.html: deck mounted", page.locator(".fm-deck-viewport").count() == 1)
        check("talk.html: morphing armed (1.1 manifest embedded)", page.eval_on_selector(
            ".fm-deck-viewport", "el => el.classList.contains('fm-deck-morphing')"))
        check("talk.html: live edges present", page.locator(".fm-deck-live path").count() > 0)
        tf1 = page.eval_on_selector(
            ".fm-deck-viewport svg g[id^='fm-node-']", "el => el.getAttribute('transform') || ''")
        page.wait_for_timeout(600)
        tf2 = page.eval_on_selector(
            ".fm-deck-viewport svg g[id^='fm-node-']", "el => el.getAttribute('transform') || ''")
        check("talk.html: nodes animating", tf1 != "" and tf1 != tf2, (tf1, tf2))
        page.keyboard.press("ArrowRight")
        page.wait_for_timeout(600)
        check("talk.html: keyboard advances without errors", not errors, errors[:2])
        page.close()
    else:
        print("SKIP: talk.html section (remote target does not ship the golden)")

    browser.close()
if server:
    server.shutdown()
failed = [c for c in checks if not c[1]]
print(f"\n{len(checks) - len(failed)}/{len(checks)} deck site checks passed")
sys.exit(1 if failed else 0)
