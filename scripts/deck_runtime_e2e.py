#!/usr/bin/env python3
"""Headless behavioral suite for crates/fm-cli/src/deck_runtime.js (bd-lnca7).

Drives crates/fm-cli/tests/fixtures/deck_runtime_fixture.html in headless Chromium and
asserts the full runtime contract: mount + SVG pinning, camera transform, dim/half/hidden
focus classes, staggered step reveals, slide navigation, overview tour rect (presence +
animation), click-a-dimmed-node travel, stage-scoped keyboard, tooltip toggle, freecam +
Escape, leak-free destroy(), and MORPH MODE (bd-tm1q7): the morphing class, live edge
layer, push-out exile of off-slide nodes, float-only members, parked engine paths, and
drag spring-back, self-loop edges gluing to their node, and variant followers. 36 checks; exit 0 = all pass.

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

    # Morph mode (manifest 1.1.0): the graph itself rearranges — graphcon parity (bd-tm1q7)
    page.wait_for_timeout(900)  # let the 6%/frame push easing act
    check("morphing class armed", page.eval_on_selector(
        ".fm-deck-viewport", "el => el.classList.contains('fm-deck-morphing')"))
    check("live edge layer with 4 paths", page.locator(".fm-deck-live path").count() == 4)

    def push_mag(tf):
        try:
            parts = tf.replace("translate(", "").replace(")", "").split()
            return abs(float(parts[0])) + abs(float(parts[1]))
        except Exception:
            return -1.0
    d_tf = page.eval_on_selector("#fm-node-d-3", "el => el.getAttribute('transform') || ''")
    # With visible-member camera fit, the intro window is tight around node a and d is
    # ALREADY outside it — the push-out ray short-circuits (graphcon's m >= 1 branch) and
    # d stays home, floating only. The positive push assertion lives on slide 2 below.
    check("outside-the-window d floats in place", 0 <= push_mag(d_tf) < 30, d_tf)
    a_tf = page.eval_on_selector("#fm-node-a-0", "el => el.getAttribute('transform') || ''")
    check("member a stays near home (float only)", 0 <= push_mag(a_tf) < 30, a_tf)
    live_d = page.eval_on_selector(".fm-deck-live path", "el => el.getAttribute('d') || ''")
    check("live edges carry path data", live_d.startswith("M "), live_d)
    parked = page.eval_on_selector(
        "#fm-edge-0",
        "el => (el.tagName === 'path' ? el : el.querySelector('path'))"
        ".classList.contains('fm-deck-parked')")
    check("engine edge path parked", parked)
    # Self-loop (fm-edge-4 on node c): no degenerate live segment; the engine loop path
    # stays visible and the whole edge group rides the node's displacement instead.
    loop_parked = page.eval_on_selector(
        "#fm-edge-4 path", "el => el.classList.contains('fm-deck-parked')")
    check("self-loop engine path NOT parked", not loop_parked)
    # Read companions in ONE evaluate so the sampled transforms come from the same frame
    # (separate round-trips race the animation loop).
    companions = page.evaluate(
        "() => { const g = s => document.querySelector(s);"
        " return { c: g('#fm-node-c-2').getAttribute('transform') || '',"
        "   loop: g('#fm-edge-4').getAttribute('transform') || '',"
        "   mirror: g('#fm-node-c-2-mirror-header').getAttribute('transform') || '',"
        "   mirrorDim: g('#fm-node-c-2-mirror-header').classList.contains('fm-deck-dim') }; }")
    check("self-loop group rides its node (both displaced together)",
          push_mag(companions["c"]) > 30 and companions["loop"] == companions["c"],
          companions)
    # Variant follower (fm-node-c-2-mirror-header, a sequence-style bottom mirror): not a
    # manifest member, but it must mirror c's focus classes and ride c's exact transform.
    check("variant follower mirrors dim + exact transform of its base",
          companions["mirrorDim"] and companions["mirror"] == companions["c"],
          companions)
    # Drag member a and release: it follows the pointer, then springs back home.
    box = page.eval_on_selector("#fm-node-a-0", "el => { const r = el.getBoundingClientRect(); return {x: r.x + r.width / 2, y: r.y + r.height / 2}; }")
    page.mouse.move(box["x"], box["y"]); page.mouse.down()
    page.mouse.move(box["x"] + 120, box["y"] + 60, steps=5); page.wait_for_timeout(120)
    dragged = push_mag(page.eval_on_selector("#fm-node-a-0", "el => el.getAttribute('transform') || ''"))
    page.mouse.up(); page.wait_for_timeout(900)
    settled = push_mag(page.eval_on_selector("#fm-node-a-0", "el => el.getAttribute('transform') || ''"))
    check("drag moves member node", dragged > 60, dragged)
    check("release springs it back home", 0 <= settled < 30, settled)

    page.click("#next"); page.wait_for_timeout(600)
    hid_b = page.eval_on_selector("#fm-node-b-1", "el => el.classList.contains('fm-deck-hidden')")
    check("advance reveals b (staggered)", not hid_b)
    check("counter step advanced", "1/1" in page.inner_text("#deck-num"))

    page.click("#next"); page.wait_for_timeout(1000)
    check("second advance changes slide", page.inner_text("#deck-title").strip() == "The core cluster")
    # On slide 2 (b, c, d home team), off-slide a sits INSIDE the fitted window's reach
    # and must be ray-pushed out past it (full-ease exile ~100 units for this geometry).
    a_push = push_mag(page.eval_on_selector("#fm-node-a-0", "el => el.getAttribute('transform') || ''"))
    check("off-slide a pushed out of slide 2's window", a_push > 50, a_push)
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
