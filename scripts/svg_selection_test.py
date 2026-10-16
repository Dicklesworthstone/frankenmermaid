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
                page.evaluate("() => { " + script + " }")
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
        browser.close()
        if page_errors:
            failures.append("uncaught-browser-errors")
            print(json.dumps({"passed": False, "errors": page_errors}))
    print(json.dumps({"checks": checks, "failed": failures,
        "entry_sha256": hashlib.sha256((ROOT / "web/mermaid.mjs").read_bytes()).hexdigest()}))
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
