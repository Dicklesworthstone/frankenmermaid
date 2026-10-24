#!/usr/bin/env python3
"""Real Chromium contract tests for bindSvgSelection; no WASM/toolchain required.

Run: python scripts/svg_selection_test.py
Requires Python Playwright and Chromium (CHROMIUM may override its executable).
The entry module is loaded from a Blob for an offline test. Only its unrelated
lazy worker imports/import.meta URL are substituted; the selection implementation,
SVG geometry, transforms, focus, event propagation and MutationObserver are real.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
FIXTURE = """<svg id="diagram" xmlns="http://www.w3.org/2000/svg" width="640" height="420"
 viewBox="0 0 640 420" aria-label="Selection test diagram">
 <g transform="translate(20 10)">
  <g class="fm-node" data-id="A" id="scoped-fm-node-a-0" role="graphics-symbol" aria-label="Alpha">
   <rect x="30" y="40" width="100" height="60" fill="#ddd"/><text x="40" y="75">Alpha</text>
  </g>
  <a href="#activated"><g class="fm-node" data-id="B" id="scoped-fm-node-b-1" tabindex="0">
   <rect x="210" y="40" width="100" height="60" fill="#ddd"/>
  </g></a>
  <g class="fm-node" data-id="C"><rect x="30" y="180" width="100" height="60" fill="#ddd"/></g>
  <g class="fm-node" data-id="__proto__" transform="rotate(35 260 240)">
   <rect x="210" y="210" width="100" height="60" fill="#ddd"/>
  </g>
  <g style="display:none"><g class="fm-node" data-id="hidden"><rect width="100" height="60"/></g></g>
 </g>
</svg>"""


def main() -> int:
    failures = []
    checks = 0
    with sync_playwright() as playwright:
        executable = os.environ.get("CHROMIUM") or shutil.which("chromium") or shutil.which("chromium-browser")
        browser = playwright.chromium.launch(executable_path=executable, headless=True,
            args=["--no-sandbox"])
        page = browser.new_page(viewport={"width": 1000, "height": 800})
        page_errors = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        page.set_content("<!doctype html><html><body></body></html>")
        source = (ROOT / "web/mermaid.mjs").read_text()
        page.evaluate("""async ({markup, source}) => {
          const compat = URL.createObjectURL(new Blob([`
            export function createWorkerMermaid() { return {}; }
            export function createMermaid() { return {}; }
            export class MermaidError extends Error {}
          `], {type: 'text/javascript'}));
          const rewritten = source.replaceAll('"./mermaid-compat.mjs"', JSON.stringify(compat))
            .replaceAll('import.meta.url', JSON.stringify('https://frankenmermaid.invalid/web/mermaid.mjs'));
          const entry = URL.createObjectURL(new Blob([rewritten], {type: 'text/javascript'}));
          try { window.bind = (await import(entry)).bindSvgSelection; }
          finally { URL.revokeObjectURL(entry); URL.revokeObjectURL(compat); }
          window.markup = markup;
          window.fresh = (options = {}) => {
            window.controller?.dispose();
            document.body.removeAttribute('style');
            document.body.innerHTML = markup;
            window.svg = document.querySelector('svg');
            window.errors = []; window.changes = []; window.activations = 0;
            for (const node of svg.querySelectorAll('.fm-node')) {
              node.addEventListener('click', () => activations++);
              node.addEventListener('keydown', event => { if (event.key === 'Enter' || event.key === ' ') activations++; });
            }
            window.controller = bind({svg, onSelectionChange: state => changes.push(state),
              onError: error => errors.push(String(error)), ...options});
          };
          window.ids = () => controller.selectionState().selectedNodeIds;
          window.node = id => [...svg.querySelectorAll('.fm-node')].find(el => el.dataset.id === id);
          window.center = id => { const r = node(id).getBoundingClientRect();
            return {x: r.x + r.width / 2, y: r.y + r.height / 2}; };
          window.expect = (value, message = 'assertion failed') => { if (!value) throw Error(message); };
          window.equal = (a, b) => expect(JSON.stringify(a) === JSON.stringify(b),
            JSON.stringify(a) + ' != ' + JSON.stringify(b));
          fresh();
        }""", {"markup": FIXTURE, "source": source})

        def check(name, script):
            nonlocal checks
            checks += 1
            try:
                script() if callable(script) else page.evaluate("() => { " + script + " }")
                print(json.dumps({"scenario_id": name, "surface": "svg", "passed": True}))
            except Exception as error:
                failures.append(name)
                print(json.dumps({"scenario_id": name, "surface": "svg", "passed": False, "error": str(error)}))

        check("authored-ids-and-immutable-snapshots", """
          fresh(); expect(controller.selectNode('A')); equal(ids(), ['A']);
          const state = controller.selectionState();
          expect(Object.isFrozen(state) && Object.isFrozen(state.selectedNodeIds));
          expect(state.primaryNodeId === 'A');
          expect(svg.querySelector('[data-fm-selection-overlay] polygon').dataset.nodeId === 'A');
          expect(node('A').children.length === 2, 'highlight must not change node geometry');
        """)
        check("selection-modes", """
          fresh(); controller.selectNode('A'); controller.selectNode('B', 'add'); equal(ids(), ['A','B']);
          controller.selectNode('A', 'toggle'); equal(ids(), ['B']);
          controller.selectNode('C', 'toggle'); equal(ids(), ['B','C']);
          controller.selectNode('B', 'remove'); equal(ids(), ['C']);
          controller.clear(); equal(ids(), []);
        """)
        check("unknown-id-is-transactional", """
          fresh(); controller.selectNode('A'); expect(!controller.selectNodes(['B', 'missing']));
          equal(ids(), ['A']); expect(!controller.selectNode('hidden')); equal(ids(), ['A']);
        """)
        check("hostile-author-id-is-not-a-selector", """
          fresh(); controller.selectNode('__proto__'); equal(ids(), ['__proto__']);
          node('__proto__').setAttribute('data-id', 'quote" ] :not(*)'); controller.refresh();
          expect(controller.selectNode('quote" ] :not(*)')); equal(ids(), ['quote" ] :not(*)']);
        """)
        check("finite-picking-and-empty-space", """
          fresh(); let p = center('A'); controller.selectAt(p.x, p.y); equal(ids(), ['A']);
          expect(!controller.selectAt(NaN, 0)); expect(!controller.selectAt(Infinity, 0)); equal(ids(), ['A']);
          controller.selectAt(600, 400); equal(ids(), []);
        """)
        check("topmost-overlap-wins", """
          fresh(); const copy = node('A').cloneNode(true); copy.dataset.id = 'top';
          node('A').parentNode.append(copy); controller.refresh();
          const p = center('A'); controller.selectAt(p.x, p.y); equal(ids(), ['top']);
        """)
        check("actual-rotated-box-not-screen-aabb", """
          fresh(); let p = center('__proto__'); controller.selectAt(p.x, p.y); equal(ids(), ['__proto__']);
          const r = node('__proto__').getBoundingClientRect(); controller.selectAt(r.left + 1, r.top + 1);
          equal(ids(), []);
        """)
        check("css-viewbox-and-nested-transform-picking", """
          fresh(); svg.style.transformOrigin = '0 0'; svg.style.transform = 'translate(80px, 20px) rotate(8deg) scale(.8,1.1)';
          svg.setAttribute('viewBox', '-30 -20 700 500');
          let p = center('B'); controller.selectAt(p.x, p.y); equal(ids(), ['B']);
          const overlay = svg.querySelector('[data-fm-selection-overlay] polygon').getBoundingClientRect();
          const real = node('B').getBoundingClientRect();
          for (const key of ['x','y','width','height']) expect(Math.abs(overlay[key]-real[key]) < .01, key);
        """)
        check("directional-keyboard-and-extension", """
          fresh(); controller.selectNode('A'); expect(controller.navigate('right')); equal(ids(), ['B']);
          expect(controller.navigate('down', true)); equal(ids(), ['B','__proto__']);
          expect(document.activeElement === node('__proto__'));
          controller.navigate('left'); equal(ids(), ['C']);
        """)
        check("source-replacement-retains-semantic-selection", """
          fresh(); controller.selectNode('B'); controller.beforeRender();
          expect(!controller.selectionState().frameAvailable); expect(!controller.selectNode('A'));
          svg.innerHTML = new DOMParser().parseFromString(markup, 'image/svg+xml').documentElement.innerHTML;
          node('B').id = 'different-generated-id'; node('B').parentNode.parentNode.prepend(node('B'));
          controller.refresh(); equal(ids(), ['B']);
          expect(svg.querySelector('[data-fm-selection-overlay] polygon').dataset.nodeId === 'B');
          node('B').remove(); controller.refresh(); equal(ids(), []);
        """)
        check("hidden-and-singular-geometry-is-not-selectable", """
          fresh(); controller.selectNode('A'); node('A').style.opacity = '0'; controller.refresh(); equal(ids(), []);
          node('B').setAttribute('transform','scale(0)'); expect(!controller.selectNode('B'));
        """)
        check("duplicate-identity-fails-closed", """
          fresh(); controller.dispose(); const duplicate = node('C'); duplicate.dataset.id = 'A';
          let rejected = false; try { bind({svg}); } catch { rejected = true; } expect(rejected);
          expect(!svg.hasAttribute('tabindex')); duplicate.dataset.id = 'C';
          controller = bind({svg}); expect(controller.selectNode('A'));
        """)
        check("invalid-options-and-inputs", """
          fresh(); let errors = 0;
          for (const operation of [() => bind({svg}), () => bind({svg: document.body}),
            () => controller.selectNode('A','invalid'), () => controller.selectNodes([3]),
            () => controller.navigate('north')]) { try { operation(); } catch { errors++; } }
          expect(errors === 5); equal(ids(), []);
        """)
        check("callbacks-cannot-corrupt-selection", """
          fresh({onSelectionChange: () => { throw Error('host failure'); }});
          controller.selectNode('A'); equal(ids(), ['A']); expect(errors.length === 1);
          controller.selectNode('B'); equal(ids(), ['B']); expect(errors.length === 2);
        """)
        check("callback-can-dispose-reentrantly", """
          fresh({onSelectionChange: () => controller.dispose()}); controller.selectNode('A');
          equal(ids(), []); expect(!svg.querySelector('[data-fm-selection-overlay]'));
          expect(!controller.selectNode('B'));
        """)
        check("independent-diagrams", """
          fresh(); controller.selectNode('A'); const other = svg.cloneNode(true); document.body.append(other);
          other.querySelector('[data-fm-selection-overlay]')?.remove();
          const second = bind({svg: other}); second.selectNode('B'); equal(ids(), ['A']);
          equal(second.selectionState().selectedNodeIds, ['B']); second.dispose(); other.remove();
        """)
        check("dispose-restores-only-owned-state", """
          fresh(); controller.selectNode('A'); controller.navigate('right'); controller.navigate('left');
          expect(node('A').getAttribute('tabindex') === '-1');
          node('B').setAttribute('tabindex', '7'); controller.dispose(); controller.dispose();
          expect(!svg.hasAttribute('tabindex')); expect(!node('A').hasAttribute('tabindex'));
          expect(node('B').getAttribute('tabindex') === '7'); expect(!svg.querySelector('[data-fm-selection-overlay]'));
          expect(!controller.selectNode('A')); controller = bind({svg}); expect(controller.selectNode('A'));
        """)
        check("empty-chart-surface", """
          fresh(); svg.replaceChildren(); controller.refresh(); expect(!controller.navigate('right'));
          equal(ids(), []); expect(controller.selectionState().frameAvailable);
        """)
        # Real browser input: not synthetic events or a mock geometry API.
        page.evaluate("fresh()")
        p = page.evaluate("center('B')")
        page.mouse.click(p["x"], p["y"])
        check("real-pointer-selection-never-activates-link", """
          equal(ids(), ['B']); expect(activations === 0); expect(location.hash === '');
          expect(document.activeElement === node('B'));
        """)
        page.keyboard.press("ArrowLeft")
        check("real-keyboard-navigation", "equal(ids(), ['A']); expect(activations === 0);")
        page.keyboard.press("Shift+ArrowDown")
        check("real-shift-keyboard-multiselect", "equal(ids(), ['A','C']);")
        page.keyboard.press("Control+a")
        check("real-select-all", "equal(ids(), ['A','B','C','__proto__']);")
        page.keyboard.press("Escape")
        check("real-escape-clears", "equal(ids(), []);")
        page.keyboard.press("Home")
        page.keyboard.press("Enter")
        page.keyboard.press("Space")
        check("keyboard-selection-is-not-activation", "equal(ids(), ['A']); expect(activations === 0);")
        # Wait for the actual observer + RAF, not manual refresh.
        page.evaluate("node('A').remove()")
        page.wait_for_function("ids().length === 0")
        check("automatic-source-reconciliation", "equal(ids(), []); expect(errors.length === 0);")
        page.evaluate("fresh(); controller.dispose()")
        p = page.evaluate("center('B')")
        page.mouse.click(p["x"], p["y"])
        check("dispose-restores-host-activation", "expect(activations === 1); expect(location.hash === '#activated');")
        # Pointer-capture, rollback and polygon-region tests exercise the same export.
        def frames():
            page.evaluate("() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))")

        def drag_case(name, options, initial, points, assertion, during=None, modifiers=()):
            def run():
                page.evaluate("fresh(" + options + "); " + initial + "; window.pointerId = null; "
                    "window.addEventListener('pointerdown', e => pointerId = e.pointerId, {once:true,capture:true});")
                for modifier in modifiers:
                    page.keyboard.down(modifier)
                try:
                    page.mouse.move(*points[0])
                    page.mouse.down()
                    for point in points[1:]:
                        page.mouse.move(*point)
                    frames()
                    if during:
                        during() if callable(during) else page.evaluate("() => { " + during + " }")
                    page.mouse.up()
                    frames()
                    page.evaluate("() => { " + assertion + " }")
                finally:
                    page.mouse.up()
                    for modifier in modifiers:
                        page.keyboard.up(modifier)
            check(name, run)

        rectangle = [(40, 40), (180, 140)]
        drag_case("box-preview-and-commit", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['A']); expect(controller.selectionState().activeGesture === null); "
            "expect(!svg.querySelector('[data-fm-selection-region]')); expect(activations === 0);",
            "equal(ids(), ['A']); expect(typeof controller.selectionState().activeGesture === 'string'); "
            "expect(svg.querySelector('[data-fm-selection-region]'));")
        drag_case("shift-drag-adds", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['A','B']);", modifiers=("Shift",))
        drag_case("alt-drag-removes", "{}", "controller.selectNodes(['A','B'])", rectangle,
            "equal(ids(), ['B']);", modifiers=("Alt",))

        def retrace():
            page.evaluate("equal(ids(), ['B'])")
            page.mouse.move(45, 45)
            frames()
            page.evaluate("equal(ids(), ['A','B'])")
            page.mouse.move(180, 140)
            frames()
            page.evaluate("equal(ids(), ['B'])")

        drag_case("toggle-preview-is-baseline-relative", "{}", "controller.selectNodes(['A','B'])", rectangle,
            "equal(ids(), ['B']); expect(activations === 0);", during=retrace, modifiers=("Control",))
        drag_case("escape-rolls-back-not-clears", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            during=lambda: page.keyboard.press("Escape"))
        drag_case("explicit-cancel-rolls-back", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(!controller.cancel());",
            "expect(controller.cancel()); equal(ids(), ['B']);")
        drag_case("pointercancel-rolls-back", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            "svg.dispatchEvent(new PointerEvent('pointercancel', {pointerId, bubbles:true}));")
        drag_case("lost-capture-rolls-back", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            "svg.releasePointerCapture(pointerId);")
        drag_case("source-replacement-revokes-gesture", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            "controller.beforeRender(); svg.innerHTML = new DOMParser().parseFromString(markup, 'image/svg+xml').documentElement.innerHTML; "
            "controller.refresh(); equal(ids(), ['B']);")
        drag_case("viewbox-change-rolls-back", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            "svg.setAttribute('viewBox', '0 0 1280 840');")
        drag_case("ancestor-transform-change-rolls-back", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            "document.body.style.transform = 'translate(5px, 10px)';")
        drag_case("blur-rolls-back", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']);", "window.dispatchEvent(new Event('blur'));")
        drag_case("second-touch-cancels-first-gesture", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(controller.selectionState().activeGesture === null);",
            "svg.dispatchEvent(new PointerEvent('pointerdown', {pointerId:pointerId + 1, pointerType:'touch', isPrimary:false, bubbles:true}));")
        drag_case("tool-change-cancels-gesture", "{}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']);", "controller.setTool('lasso');")
        drag_case("dispose-during-gesture-releases-capture", "{}", "controller.selectNode('B')", rectangle,
            "expect(!svg.hasPointerCapture(pointerId)); expect(!svg.querySelector('[data-fm-selection-overlay]')); "
            "expect(svg.style.touchAction === ''); equal(ids(), []);", "controller.dispose();")
        drag_case("partial-box-intersects", "{containment:'intersects'}", "", [(40, 40), (70, 70)],
            "equal(ids(), ['A']);")
        drag_case("partial-box-does-not-contain", "{containment:'contains'}", "", [(40, 40), (70, 70)],
            "equal(ids(), []);")
        drag_case("full-box-contains", "{containment:'contains'}", "", rectangle, "equal(ids(), ['A']);")
        drag_case("lasso-encloses-node", "{tool:'lasso',containment:'contains'}", "",
            [(40, 40), (180, 40), (180, 140), (40, 140), (40, 40)], "equal(ids(), ['A']);")
        notch = [(40, 40), (90, 40), (90, 90), (125, 90), (125, 40), (180, 40), (180, 140), (40, 140), (40, 40)]
        drag_case("concave-lasso-corners-are-not-enough", "{tool:'lasso',containment:'contains'}", "", notch,
            "equal(ids(), []);")
        drag_case("concave-lasso-still-intersects", "{tool:'lasso',containment:'intersects'}", "", notch,
            "equal(ids(), ['A']);")
        drag_case("edge-crossing-with-no-vertex-inside", "{tool:'lasso'}", "",
            [(100, 30), (105, 30), (105, 160), (100, 160), (100, 30)], "equal(ids(), ['A']);")
        drag_case("self-crossing-lasso-even-odd", "{tool:'lasso'}", "",
            [(40, 40), (180, 140), (40, 140), (180, 40), (40, 40)], "equal(ids(), ['A']);")
        drag_case("degenerate-lasso-restores-baseline", "{tool:'lasso'}", "controller.selectNode('B')",
            [(40, 40), (180, 40), (200, 40)], "equal(ids(), ['B']);")
        drag_case("final-pointer-up-position-is-flushed", "{}", "", [(40, 40), (80, 80)],
            "equal(ids(), ['A','B']); expect(controller.selectionState().activeGesture === null);",
            "svg.dispatchEvent(new PointerEvent('pointerup', {pointerId, clientX:360, clientY:140, bubbles:true, cancelable:true}));")
        drag_case("oversized-coalesced-batch-rolls-back", "{tool:'lasso'}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(errors.length === 1 && errors[0].includes('4096'));",
            "const event = new PointerEvent('pointermove', {pointerId,clientX:200,clientY:150,buttons:1,bubbles:true}); "
            "Object.defineProperty(event, 'getCoalescedEvents', {value: () => Array(4097).fill({clientX:200,clientY:150})}); "
            "svg.dispatchEvent(event);")
        drag_case("bounded-total-lasso-samples-roll-back", "{tool:'lasso'}", "controller.selectNode('B')", rectangle,
            "equal(ids(), ['B']); expect(errors.length === 1 && errors[0].includes('4096'));",
            "const event = new PointerEvent('pointermove', {pointerId,clientX:200,clientY:150,buttons:1,bubbles:true}); "
            "Object.defineProperty(event, 'getCoalescedEvents', {value: () => Array.from({length:4096}, (_,i) => ({clientX:200+i/100,clientY:150}))}); "
            "svg.dispatchEvent(event);")
        check("zero-threshold-still-allows-click", lambda: (
            page.evaluate("fresh({dragThreshold:0})"), page.mouse.click(100, 80),
            page.evaluate("equal(ids(), ['A'])")))
        check("gesture-options-validated-before-ownership", """
          fresh(); controller.dispose(); let count = 0;
          for (const options of [{tool:'invalid'}, {containment:'invalid'}, {dragThreshold:NaN}, {dragThreshold:65}]) {
            try { bind({svg,...options}); } catch { count++; }
          }
          expect(count === 4); expect(svg.style.touchAction === '');
          controller = bind({svg}); controller.selectNode('A'); equal(ids(), ['A']);
        """)
        drag_case("lasso-inner-hole-is-not-full-containment", "{tool:'lasso',containment:'contains'}", "",
            [(40, 40), (180, 40), (180, 140), (40, 140), (40, 40),
             (85, 75), (120, 75), (120, 100), (85, 100), (85, 75), (40, 40)], "equal(ids(), []);")
        check("touch-action-priority-and-host-edits-restored", """
          fresh(); controller.dispose(); svg.style.setProperty('touch-action','pan-y','important');
          controller = bind({svg}); expect(svg.style.touchAction === 'none'); controller.dispose();
          expect(svg.style.touchAction === 'pan-y' && svg.style.getPropertyPriority('touch-action') === 'important');
          controller = bind({svg}); svg.style.setProperty('touch-action','pan-x'); controller.dispose();
          expect(svg.style.touchAction === 'pan-x');
        """)
        check("geometry-errors-do-not-activate-authored-handlers", """
          fresh(); node('C').dataset.id = 'A';
          const event = new MouseEvent('click', {bubbles:true,cancelable:true});
          node('B').dispatchEvent(event); expect(event.defaultPrevented); expect(activations === 0);
          expect(errors.some(error => error.includes('Ambiguous')));
        """)
        browser.close()
        if page_errors:
            failures.append("uncaught-browser-errors")
            print(json.dumps({"passed": False, "errors": page_errors}))
    print(json.dumps({"checks": checks, "failed": failures,
        "entry_sha256": hashlib.sha256((ROOT / "web/mermaid.mjs").read_bytes()).hexdigest()}))
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
