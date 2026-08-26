#!/usr/bin/env python3
"""Headless behavioral suite for crates/fm-cli/src/deck_runtime.js (bd-lnca7).

Drives crates/fm-cli/tests/fixtures/deck_runtime_fixture.html in headless Chromium and
asserts the full runtime contract: mount + SVG pinning, camera transform, dim/half/hidden
focus classes, staggered step reveals, slide navigation, overview tour rect (presence +
animation), click-a-dimmed-node travel, stage-scoped keyboard, tooltip toggle, freecam +
Escape, and leak-free destroy(). 24 checks; exit 0 = all pass.

OPT-IN tooling, not a default gate: requires a Python with playwright + a downloaded
chromium (any venv works: /path/to/venv/bin/python scripts/deck_runtime_e2e.py). The
default quality gates stay browser-free by design; scripts/verify_deck_runtime.mjs carries
the dependency-free guards.
"""
try:
    from playwright.sync_api import sync_playwright  # noqa: F401
except ImportError:
    import sys
    print("SKIP: playwright not available in this Python; see module docstring")
    sys.exit(0)

import threading, functools, sys
from http.server import ThreadingHTTPServer, SimpleHTTPRequestHandler
from playwright.sync_api import sync_playwright

ROOT = "/data/projects/frankenmermaid"
Handler = functools.partial(SimpleHTTPRequestHandler, directory=ROOT)
server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
port = server.server_address[1]
threading.Thread(target=server.serve_forever, daemon=True).start()

checks = []
def check(name, ok, detail=""):
    checks.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + ((" — " + str(detail)) if detail and not ok else ""))

with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={"width": 1200, "height": 800})
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.goto(f"http://127.0.0.1:{port}/crates/fm-cli/tests/fixtures/deck_runtime_fixture.html")
    page.wait_for_timeout(400)

    check("no page errors on mount", not errors, errors)
    check("viewport mounted", page.locator(".fm-deck-viewport").count() == 1)
    svg_w = page.eval_on_selector(".fm-deck-viewport svg", "el => el.style.width")
    check("svg pinned to viewBox px", svg_w == "900px", svg_w)
    tf = page.eval_on_selector(".fm-deck-viewport", "el => el.style.transform")
    check("camera transform applied", "translate3d" in tf and "scale" in tf, tf)

    dim_d = page.eval_on_selector("#fm-node-d-3", "el => el.classList.contains('fm-deck-dim')")
    hid_b = page.eval_on_selector("#fm-node-b-1", "el => el.classList.contains('fm-deck-hidden')")
    check("scene 0: off-slide d dimmed", dim_d)
    check("scene 0: step-1 b hidden", hid_b)
    check("counter shows steps", page.inner_text("#deck-num").strip() == "01 / 03 · 0/1", page.inner_text("#deck-num"))

    page.click("#next"); page.wait_for_timeout(600)
    hid_b = page.eval_on_selector("#fm-node-b-1", "el => el.classList.contains('fm-deck-hidden')")
    check("advance reveals b (staggered)", not hid_b)
    check("counter step advanced", "1/1" in page.inner_text("#deck-num"))

    page.click("#next"); page.wait_for_timeout(700)
    check("second advance changes slide", page.inner_text("#deck-title").strip() == "The core cluster")
    half_e0 = page.eval_on_selector("#fm-edge-0", "el => el.classList.contains('fm-deck-half')")
    dim_a = page.eval_on_selector("#fm-node-a-0", "el => el.classList.contains('fm-deck-dim')")
    half_cl = page.eval_on_selector("#fm-cluster-0", "el => el.classList.contains('fm-deck-half')")
    check("touching edge half-dimmed", half_e0)
    check("off-slide a dimmed", dim_a)
    check("cameraContained cluster NOT half", not half_cl)

    page.click("#ov"); page.wait_for_timeout(1900)
    check("overview title", page.inner_text("#deck-title").strip() == "One graph")
    tour = page.locator(".fm-deck-tour")
    check("tour rect present", tour.count() == 1)
    w = page.eval_on_selector(".fm-deck-tour", "el => parseFloat(el.getAttribute('width') || '0')")
    check("tour rect animating over windows", w > 100, w)

    # Travel: back to slide 0, click the DIMMED d node -> should go to 'core'
    page.click("#deck-dots i >> nth=0"); page.wait_for_timeout(400)
    page.eval_on_selector("#fm-node-d-3", "el => el.dispatchEvent(new PointerEvent('pointerdown', {bubbles: true, pointerId: 7}))")
    page.eval_on_selector("#deck-stage", "el => el.dispatchEvent(new PointerEvent('pointerup', {bubbles: true, pointerId: 7, clientX: 0, clientY: 0}))")
    page.wait_for_timeout(300)
    # travel is wired through stage pointer handlers; simulate a plain click path instead
    title = page.inner_text("#deck-title").strip()
    if title != "The core cluster":
        page.locator("#fm-node-d-3").click(force=True)
        page.wait_for_timeout(400)
        title = page.inner_text("#deck-title").strip()
    check("click dimmed node travels to its slide", title == "The core cluster", title)

    # Keyboard on the stage
    page.click("#deck-dots i >> nth=0"); page.wait_for_timeout(300)
    page.focus("#deck-stage"); page.keyboard.press("ArrowRight"); page.wait_for_timeout(500)
    check("ArrowRight advances step", "1/1" in page.inner_text("#deck-num"), page.inner_text("#deck-num"))
    page.keyboard.press("ArrowLeft"); page.wait_for_timeout(300)
    check("ArrowLeft un-reveals", "0/1" in page.inner_text("#deck-num"), page.inner_text("#deck-num"))

    # Tooltip on active node a
    page.locator("#fm-node-a-0").click(force=True); page.wait_for_timeout(300)
    tip_shown = page.eval_on_selector(".fm-deck-tip", "el => el.classList.contains('fm-deck-tip-show')")
    tip_text = page.inner_text(".fm-deck-tip")
    check("tooltip toggles on active node", tip_shown and tip_text == "the entry point", tip_text)

    # Freecam wheel + Escape
    page.hover("#deck-stage"); page.mouse.wheel(0, -400); page.wait_for_timeout(200)
    page.keyboard.press("Escape"); page.wait_for_timeout(600)
    check("no page errors after freecam/escape", not errors, errors)

    # destroy() teardown
    page.click("#kill"); page.wait_for_timeout(200)
    check("destroy removes viewport", page.locator(".fm-deck-viewport").count() == 0)
    check("destroy removes tooltip el", page.locator(".fm-deck-tip").count() == 0)
    check("no page errors at teardown", not errors, errors)

    browser.close()
server.shutdown()
failed = [c for c in checks if not c[1]]
print(f"\n{len(checks) - len(failed)}/{len(checks)} checks passed")
sys.exit(1 if failed else 0)
