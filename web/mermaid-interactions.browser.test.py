#!/usr/bin/env python3
"""Interaction lifecycle tests using real Chromium DOM/events and the production SVG adapter.

Run: python web/mermaid-interactions.browser.test.py
The SVG is an explicit native-output boundary fixture, not an executed WASM engine.
No network is used. Playwright and Chromium (or CHROMIUM_PATH) are required.
"""
import base64
import os
from pathlib import Path
import shutil
import unittest

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parent


class BrowserFixture:
    @classmethod
    def setUpClass(cls):
        cls.playwright = sync_playwright().start()
        executable = os.environ.get("CHROMIUM_PATH") or shutil.which("chromium") or shutil.which("chromium-browser")
        cls.browser = cls.playwright.chromium.launch(
            headless=True, args=["--no-sandbox"], **({"executable_path": executable} if executable else {})
        )
        cls.module_url = "data:text/javascript;base64," + base64.b64encode((ROOT / "mermaid-svg.mjs").read_bytes()).decode()

    @classmethod
    def tearDownClass(cls):
        cls.browser.close()
        cls.playwright.stop()

    def setUp(self):
        self.context = self.browser.new_context()
        self.page = self.context.new_page()
        self.page.set_default_timeout(5000)
        self.page.set_content('<button id="before">Before</button><div id="stage"></div><div id="other"></div><button id="after">After</button>')
        self.page.evaluate("async url => { window.adapter = await import(url); }", self.module_url)
        self.page.evaluate('''() => {
          window.calls = []; window.errors = []; window.live = true;
          window.callbacks = new Map([['app.visit', id => calls.push(id)]]);
          window.raw = '<svg xmlns="http://www.w3.org/2000/svg" id="engine" viewBox="0 0 200 80" role="img" aria-label="A graph">' +
            '<g id="fm-node-A-0" data-id="A雪" data-callback="app.visit" role="graphics-symbol" tabindex="-1" title="Open details">' +
            '<title>Node A</title><rect x="5" y="5" width="70" height="50"/><text x="10" y="30">A</text></g>' +
            '<g id="fm-node-B-1" data-id="B"><rect x="100" y="5" width="70" height="50"/></g></svg>';
          window.mount = (source = raw, id = 'diagram', host = document.querySelector('#stage')) => {
            window.output = adapter.prepareSvg(source, id, document);
            window.bind = adapter.createSvgBinder(output.element, {
              resolveCallback: name => callbacks.get(name), isLive: () => live,
              reportError: error => errors.push(error.message),
            });
            host.innerHTML = output.svg;
            window.cleanup = bind(host);
          };
        }''')

    def tearDown(self):
        self.context.close()

    def mount(self):
        self.page.evaluate('mount()')


class SvgInteractionTests(BrowserFixture, unittest.TestCase):
    def test_pointer_activation_receives_author_source_id_not_namespaced_id(self):
        self.mount()
        self.page.locator('[data-id="A雪"] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])
        self.assertNotEqual(self.page.locator('[data-id="A雪"]').get_attribute('id'), 'fm-node-A-0')
        self.page.locator('[data-id="B"] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])

    def test_keyboard_enter_and_space_and_tab_focus(self):
        self.mount()
        self.page.locator('#before').focus()
        self.page.keyboard.press('Tab')
        self.assertEqual(self.page.evaluate('document.activeElement.dataset.id'), 'A雪')
        self.page.keyboard.press('Enter')
        self.page.keyboard.down('Space')
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])
        self.page.keyboard.up('Space')
        self.assertEqual(self.page.evaluate('calls'), ['A雪', 'A雪'])
        self.assertEqual(self.page.locator('[data-id="A雪"]').get_attribute('role'), 'button')
        self.assertEqual(self.page.locator('#diagram').get_attribute('role'), 'group')
        self.assertEqual(self.page.locator('#diagram').get_attribute('aria-label'), 'A graph')

    def test_key_repeats_modifiers_composition_and_unarmed_space_are_ignored(self):
        self.mount()
        self.page.evaluate('''() => {
          const node = document.querySelector('[data-callback]');
          for (const options of [{repeat:true}, {ctrlKey:true}, {altKey:true}, {metaKey:true}, {shiftKey:true}, {isComposing:true}]) {
            node.dispatchEvent(new KeyboardEvent('keydown', {key:'Enter', bubbles:true, cancelable:true, ...options}));
          }
          node.dispatchEvent(new KeyboardEvent('keyup', {key:' ', bubbles:true, cancelable:true}));
          node.dispatchEvent(new MouseEvent('click', {button:1, bubbles:true, cancelable:true}));
          node.dispatchEvent(new MouseEvent('click', {ctrlKey:true, bubbles:true, cancelable:true}));
        }''')
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_space_is_cancelled_when_focus_leaves(self):
        self.mount()
        self.page.locator('[data-callback]').focus()
        self.page.keyboard.down('Space')
        self.page.locator('#after').focus()
        self.page.locator('[data-callback]').focus()
        self.page.keyboard.up('Space')
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_repeated_binding_is_idempotent_and_cleanup_allows_rebinding(self):
        self.mount()
        self.assertTrue(self.page.evaluate('cleanup === bind(document.querySelector("#stage"))'))
        self.page.locator('[data-callback] rect').click()
        self.page.evaluate('cleanup(); cleanup()')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('role'), 'graphics-symbol')
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('tabindex'), '-1')
        self.assertIsNone(self.page.locator('[data-callback]').get_attribute('aria-keyshortcuts'))
        self.assertEqual(self.page.locator('#diagram').get_attribute('role'), 'img')
        self.page.evaluate('() => { window.cleanup = bind(document.querySelector("#diagram")); }')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪', 'A雪'])

    def test_unregistered_handlers_never_fall_back_to_page_globals(self):
        self.page.evaluate('''() => {
          callbacks.clear();
          Object.defineProperty(window, 'app', {get() { calls.push('global lookup'); return {visit() { calls.push('global execution'); }}; }});
          mount();
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertIn("No callback registered for 'app.visit'", self.page.evaluate('errors[0]'))
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('tabindex'), '-1')

    def test_revocation_or_replacement_cannot_retarget_an_existing_binding(self):
        self.mount()
        self.page.evaluate("callbacks.set('app.visit', id => calls.push('replacement:' + id))")
        self.page.locator('[data-callback] rect').click()
        self.page.evaluate("callbacks.delete('app.visit')")
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_prebinding_tampering_is_rejected_without_partial_decoration(self):
        result = self.page.evaluate('''() => {
          const result = adapter.prepareSvg(raw, 'diagram', document);
          const binder = adapter.createSvgBinder(result.element, {resolveCallback: name => callbacks.get(name)});
          const host = document.querySelector('#stage'); host.innerHTML = result.svg;
          host.querySelector('[data-callback]').setAttribute('data-id', 'forged');
          try { binder(host); } catch (error) { return error.message; }
        }''')
        self.assertIn('changed before binding', result)
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('role'), 'graphics-symbol')

    def test_binding_is_scoped_and_rejects_duplicate_or_missing_render_roots(self):
        self.mount()
        self.page.evaluate("document.querySelector('#other').innerHTML = output.svg")
        self.page.locator('#other [data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertIn('exactly one', self.page.evaluate('''() => {
          try { bind(document.body); } catch (error) { return error.message; }
        }'''))
        self.page.evaluate("document.querySelector('#other').replaceChildren()")
        self.assertIn('exactly one', self.page.evaluate('''() => {
          try { bind(document.querySelector('#other')); } catch (error) { return error.message; }
        }'''))

    def test_mutated_or_cloned_callback_metadata_cannot_gain_authority(self):
        self.mount()
        self.page.evaluate('''() => {
          const node = document.querySelector('[data-callback]');
          node.setAttribute('data-id', 'forged');
          node.dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
          node.setAttribute('data-id', 'A雪');
          node.setAttribute('data-callback', 'different');
          node.dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
          node.setAttribute('data-callback', 'app.visit');
          const clone = node.cloneNode(true); node.parentElement.append(clone);
          clone.dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
        }''')
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_replaced_svg_and_disposed_owners_are_inert(self):
        self.mount()
        self.page.evaluate('''() => {
          const old = document.querySelector('[data-callback]');
          document.querySelector('#stage').innerHTML = output.svg;
          old.dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
          live = false;
        }''')
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertIn('disposed', self.page.evaluate('''() => {
          try { bind(document.querySelector('#stage')); } catch (error) { return error.message; }
        }'''))

    def test_disabled_hidden_and_prevented_events_do_not_activate(self):
        self.mount()
        self.page.evaluate('''() => {
          const node = document.querySelector('[data-callback]');
          for (const [name, value] of [['aria-disabled','true'], ['aria-hidden','true'], ['hidden',''], ['inert','']]) {
            node.setAttribute(name, value);
            node.dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
            node.removeAttribute(name);
          }
          node.addEventListener('click', event => event.preventDefault(), {once:true});
          node.dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
        }''')
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_cleanup_preserves_later_host_attribute_changes(self):
        self.mount()
        self.page.evaluate('''() => {
          document.querySelector('[data-callback]').setAttribute('role','link');
          document.querySelector('[data-callback]').setAttribute('tabindex','3');
          cleanup();
        }''')
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('role'), 'link')
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('tabindex'), '3')

    def test_callback_errors_and_rejected_promises_are_reported(self):
        self.page.evaluate("callbacks.set('app.visit', () => { throw new Error('synchronous failure'); }); mount()")
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('errors'), ['synchronous failure'])
        self.page.evaluate("() => { cleanup(); callbacks.set('app.visit', async () => { throw new Error('async failure'); }); bind(document.querySelector('#stage')); }")
        self.page.locator('[data-callback] rect').click()
        self.page.wait_for_function('errors.length === 2')
        self.assertEqual(self.page.evaluate('errors'), ['synchronous failure', 'async failure'])

    def test_nested_hyperlinks_keep_native_behavior(self):
        self.page.evaluate('''() => {
          const linked = raw.replace('<title>Node A</title>', '<title>Node A</title><a href="https://example.invalid/details" target="_blank"><text x="10" y="65">Link</text></a>');
          mount(linked);
          window.wasPrevented = null;
          document.addEventListener('click', event => { wasPrevented = event.defaultPrevented; event.preventDefault(); }, {once:true});
          document.querySelector('a').dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
        }''')
        self.assertFalse(self.page.evaluate('wasPrevented'))
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertEqual(self.page.locator('a').get_attribute('rel'), 'noopener noreferrer')

    def test_shadow_root_binding_and_native_tooltip_metadata_survive(self):
        self.page.evaluate('''() => {
          const host = document.querySelector('#other').attachShadow({mode:'open'});
          const stage = document.createElement('div'); host.append(stage);
          mount(raw, 'shadow-diagram', stage);
        }''')
        self.page.locator('#other [data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])
        self.assertEqual(self.page.locator('#other [data-callback]').get_attribute('title'), 'Open details')
        self.assertEqual(self.page.locator('#other [data-callback] title').text_content(), 'Node A')

    def test_rejected_callback_names_are_data_never_code(self):
        result = self.page.evaluate('''() => {
          const invalid = ['', 'app.visit()', 'app["visit"]', 'constructor', 'x.__proto__.call', 'x.prototype', 'a'.repeat(129)];
          return invalid.map(name => { try { adapter.checkCallbackName(name); return false; } catch { return true; } });
        }''')
        self.assertEqual(result, [True] * 7)
        self.page.evaluate("mount(raw.replace('app.visit', 'app.visit()'))")
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])


class MermaidApiInteractionTests(BrowserFixture, unittest.TestCase):
    def setUp(self):
        super().setUp()
        # Resolve only the relative module boundary; run the production implementation.
        code = (ROOT / 'mermaid-compat.mjs').read_text().replace('"./mermaid-svg.mjs"', repr(self.module_url))
        url = 'data:text/javascript;base64,' + base64.b64encode(code.encode()).decode()
        self.page.evaluate('async url => { window.compat = await import(url); }', url)
        self.page.evaluate('''() => {
          window.engineCalls = [];
          window.engine = {
            parse: source => ({ir:{diagram_type:'Flowchart', diagnostics:[]}, warnings:[]}),
            renderSvg: (source, config) => { engineCalls.push({source,config}); return raw; },
            validateConfig: () => JSON.stringify({schemaVersion:'1.0.0', errors:[]}),
          };
          window.makeWidget = (report = error => errors.push(error.message)) => compat.createMermaid({
            document, loadEngine: () => engine, reportError: report,
          });
          window.widget = makeWidget();
          widget.initialize({securityLevel:'loose', startOnLoad:false});
          window.unregister = widget.registerCallback('app.visit', id => calls.push(id));
          window.draw = async (instance = widget, id = 'diagram', host = document.querySelector('#stage')) => {
            const result = await instance.render(id, 'flowchart LR\\n A-->B', host);
            host.innerHTML = result.svg;
            window.result = result;
            window.unbind = result.bindFunctions(host);
          };
        }''')

    def test_render_returns_bind_function_and_never_populates_container(self):
        self.page.evaluate('''async () => {
          const host = document.querySelector('#stage'); host.textContent = 'Host content';
          window.result = await widget.render('diagram', 'flowchart LR\\n A-->B', host);
        }''')
        self.assertEqual(self.page.locator('#stage').text_content(), 'Host content')
        self.assertEqual(self.page.evaluate('typeof result.bindFunctions'), 'function')
        self.page.evaluate('''() => {
          const host = document.querySelector('#stage'); host.innerHTML = result.svg;
          window.unbind = result.bindFunctions(host);
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])

    def test_run_binds_before_post_render_callback_and_coalesces_overlapping_runs(self):
        self.page.evaluate('''async () => {
          const hosts = [document.querySelector('#stage'), document.querySelector('#other')];
          hosts.forEach(host => { host.className = 'mermaid'; host.textContent = 'flowchart LR\\n A-->B'; });
          window.afterRender = [];
          const options = {postRenderCallback(id) {
            afterRender.push(id);
            document.getElementById(id).querySelector('[data-callback]').dispatchEvent(new MouseEvent('click', {bubbles:true, cancelable:true}));
          }};
          await Promise.all([widget.run(options), widget.run(options)]);
          await widget.run(options);
        }''')
        self.assertEqual(self.page.evaluate('calls'), ['A雪', 'A雪'])
        self.assertEqual(self.page.evaluate('afterRender.length'), 2)
        self.assertEqual(self.page.evaluate('new Set(afterRender).size'), 2)
        self.assertEqual(self.page.evaluate('engineCalls.length'), 2)
        self.assertEqual(self.page.locator('[data-processed="true"]').count(), 2)

    def test_default_strict_and_source_loose_cannot_enable_callbacks(self):
        self.page.evaluate('''async () => {
          widget.initialize({theme:'dark'});
          window.result = await widget.render('diagram', '%%{init:{securityLevel:"loose"}}%%\\nflowchart LR\\nA-->B');
          const host = document.querySelector('#stage'); host.innerHTML = result.svg;
          result.bindFunctions(host);
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('role'), 'graphics-symbol')
        self.assertEqual(self.page.evaluate('errors'), [])

    def test_tightening_security_revokes_existing_and_previously_returned_results(self):
        self.page.evaluate('draw()')
        self.page.locator('[data-callback] rect').click()
        self.page.evaluate('''() => {
          widget.initialize({securityLevel:'strict'});
          widget.initialize({securityLevel:'loose'});
          result.bindFunctions(document.querySelector('#stage'));
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])
        self.assertEqual(self.page.locator('[data-callback]').get_attribute('role'), 'graphics-symbol')
        self.page.evaluate('draw()')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪', 'A雪'])

    def test_tightening_security_during_async_render_revokes_pending_authority(self):
        self.page.evaluate('''() => {
          engine.parse = () => new Promise(resolve => { window.finishParse = resolve; });
          window.pending = widget.render('diagram','flowchart LR\\n A-->B');
        }''')
        self.page.wait_for_function('typeof finishParse === "function"')
        self.page.evaluate('''async () => {
          widget.initialize({securityLevel:'strict'}); widget.initialize({securityLevel:'loose'});
          finishParse({ir:{diagram_type:'Flowchart', diagnostics:[]}, warnings:[]});
          const result = await pending;
          const host = document.querySelector('#stage'); host.innerHTML = result.svg; result.bindFunctions(host);
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_callback_registry_is_snapshotted_before_engine_load(self):
        self.page.evaluate('''() => {
          unregister();
          engine.parse = () => new Promise(resolve => { window.finishParse = resolve; });
          window.pending = widget.render('diagram', 'flowchart LR\\n A-->B');
        }''')
        self.page.wait_for_function('typeof finishParse === "function"')
        self.page.evaluate('''async () => {
          widget.registerCallback('app.visit', id => calls.push('late:' + id));
          finishParse({ir:{diagram_type:'Flowchart', diagnostics:[]}, warnings:[]});
          const result = await pending;
          const host = document.querySelector('#stage'); host.innerHTML = result.svg; result.bindFunctions(host);
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertIn('No callback registered', self.page.evaluate('errors[0]'))

    def test_old_unregister_cannot_remove_replacement_registration(self):
        self.page.evaluate('''async () => {
          widget.registerCallback('app.visit', id => calls.push('new:' + id));
          unregister(); unregister();
          await draw();
        }''')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['new:A雪'])

    def test_run_captures_one_registry_for_all_selected_diagrams(self):
        self.page.evaluate('''async () => {
          unregister();
          const hosts = [document.querySelector('#stage'), document.querySelector('#other')];
          hosts.forEach(host => { host.textContent = 'flowchart LR\\n A-->B'; });
          await widget.run({nodes:hosts, postRenderCallback() {
            widget.registerCallback('app.visit', id => calls.push('late:' + id));
          }});
          document.querySelectorAll('[data-callback]').forEach(node => {
            node.dispatchEvent(new MouseEvent('click', {bubbles:true,cancelable:true}));
          });
        }''')
        self.assertEqual(self.page.evaluate('calls'), [])
        self.assertEqual(self.page.locator('[data-processed="true"]').count(), 2)

    def test_competing_registry_cannot_coalesce_with_another_runs_authority(self):
        self.page.evaluate('''() => {
          const host = document.querySelector('#stage'); host.textContent = 'flowchart LR\\n A-->B';
          engine.parse = () => new Promise(resolve => { window.finishParse = resolve; });
          window.firstRun = widget.run({nodes:[host]});
        }''')
        self.page.wait_for_function('typeof finishParse === "function"')
        self.page.evaluate('''async () => {
          widget.registerCallback('app.visit', id => calls.push('changed:' + id));
          window.failure = await widget.run({nodes:[document.querySelector('#stage')]}).then(() => null, error => error.errors[0].code);
          finishParse({ir:{diagram_type:'Flowchart', diagnostics:[]}, warnings:[]});
          await firstRun;
        }''')
        self.assertEqual(self.page.evaluate('failure'), 'element-busy')
        self.page.locator('[data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_instance_disposal_unbinds_only_its_own_diagrams(self):
        self.page.evaluate('''async () => {
          await draw();
          window.otherWidget = makeWidget(); otherWidget.initialize({securityLevel:'LOOSE'});
          otherWidget.registerCallback('app.visit', id => calls.push('other:' + id));
          await draw(otherWidget, 'other-diagram', document.querySelector('#other'));
          widget.dispose(); widget.dispose();
        }''')
        self.page.locator('#stage [data-callback] rect').click()
        self.page.locator('#other [data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['other:A雪'])
        self.assertEqual(self.page.locator('#stage [data-callback]').get_attribute('role'), 'graphics-symbol')
        self.assertEqual(self.page.locator('#other [data-callback]').get_attribute('role'), 'button')
        self.assertIn('disposed', self.page.evaluate('''() => {
          try { widget.registerCallback('new', () => {}); } catch (error) { return error.message; }
        }'''))

    def test_reporter_reentrancy_cannot_publish_stale_source(self):
        self.page.evaluate('''async () => {
          const host = document.querySelector('#stage'); host.textContent = 'flowchart LR\\n A-->B';
          const instance = makeWidget(() => { host.textContent = 'New document'; });
          instance.initialize({securityLevel:'loose'});
          try { await instance.run({nodes:[host]}); } catch (error) { window.failure = error.errors[0].code; }
        }''')
        self.assertEqual(self.page.evaluate('failure'), 'stale-source')
        self.assertEqual(self.page.locator('#stage').text_content(), 'New document')
        self.assertIsNone(self.page.locator('#stage').get_attribute('data-processed'))

    def test_reporter_reentrancy_cannot_claim_a_conflicting_id(self):
        self.page.evaluate('''async () => {
          const host = document.querySelector('#stage'); host.textContent = 'flowchart LR\\n A-->B';
          // A fresh module instance has assigned no document IDs before this run.
          const instance = makeWidget(() => { document.querySelector('#other').id = 'fm-mermaid-1'; });
          instance.initialize({securityLevel:'loose'});
          try { await instance.run({nodes:[host]}); } catch (error) { window.failure = error.errors[0].code; }
        }''')
        self.assertEqual(self.page.evaluate('failure'), 'id-conflict')
        self.assertEqual(self.page.locator('#stage').text_content(), 'flowchart LR\n A-->B')
        self.assertIsNone(self.page.locator('#stage').get_attribute('data-processed'))

    def test_binding_failure_preserves_source_and_independent_siblings_continue(self):
        self.page.evaluate('''async () => {
          const first = document.querySelector('#stage'), second = document.querySelector('#other');
          first.textContent = 'bad callback metadata'; second.textContent = 'valid metadata';
          engine.renderSvg = source => source.startsWith('bad') ? raw.replace('data-id="A雪"', 'data-id=""') : raw;
          try { await widget.run({nodes:[first,second]}); } catch (error) { window.failure = error.errors[0].message; }
        }''')
        self.assertIn('no addressable source node', self.page.evaluate('failure'))
        self.assertEqual(self.page.locator('#stage').text_content(), 'bad callback metadata')
        self.assertIsNone(self.page.locator('#stage').get_attribute('data-processed'))
        self.assertEqual(self.page.locator('#other').get_attribute('data-processed'), 'true')
        self.page.locator('#other [data-callback] rect').click()
        self.assertEqual(self.page.evaluate('calls'), ['A雪'])

    def test_callback_failure_uses_reporter_without_reclassifying_as_parse_error(self):
        self.page.evaluate('''async () => {
          window.parseFailures = [];
          widget.parseError = error => parseFailures.push(error.message);
          widget.registerCallback('app.visit', async () => { throw new Error('application failure'); });
          await draw();
        }''')
        self.page.locator('[data-callback] rect').click()
        self.page.wait_for_function('errors.length === 1')
        self.assertEqual(self.page.evaluate('errors'), ['application failure'])
        self.assertEqual(self.page.evaluate('parseFailures'), [])

    def test_reporter_disposal_during_binding_never_leaves_installed_controls(self):
        self.page.evaluate('''async () => {
          const instance = makeWidget(() => instance.dispose());
          instance.initialize({securityLevel:'loose'});
          instance.registerCallback('app.visit', id => calls.push(id));
          engine.renderSvg = () => raw.replace('</svg>', '<g data-callback="invalid()"/></svg>');
          const result = await instance.render('diagram', 'flowchart LR\\nA-->B');
          const host = document.querySelector('#stage'); host.innerHTML = result.svg;
          try { result.bindFunctions(host); } catch (error) { window.failure = error.message; }
        }''')
        self.assertIn('disposed while preparing', self.page.evaluate('failure'))
        self.assertEqual(self.page.locator('[data-id="A雪"]').get_attribute('role'), 'graphics-symbol')
        self.page.locator('[data-id="A雪"] rect').click()
        self.assertEqual(self.page.evaluate('calls'), [])

    def test_registry_is_bounded_and_validated_without_loading_engine(self):
        result = self.page.evaluate('''() => {
          const instance = compat.createMermaid({loadEngine() { throw new Error('must stay lazy'); }});
          const errors = [];
          for (const [name, fn] of [['constructor', () => {}], ['x', 'not a function']]) {
            try { instance.registerCallback(name, fn); } catch (error) { errors.push(error.message); }
          }
          for (let index = 0; index < 1024; index++) instance.registerCallback('cb' + index, () => {});
          instance.registerCallback('cb0', () => {});
          try { instance.registerCallback('overflow', () => {}); } catch (error) { errors.push(error.message); }
          return errors;
        }''')
        self.assertEqual(len(result), 3)
        self.assertIn('safe identifier', result[0])
        self.assertIn('must be a function', result[1])
        self.assertIn('1024', result[2])

    def test_all_new_native_diagram_families_reach_parse_and_render(self):
        result = self.page.evaluate('''async () => {
          const results = [];
          for (const family of ['Venn', 'Wardley', 'EventModeling']) {
            engine.parse = () => ({ir:{diagram_type:family, diagnostics:[]}, warnings:[]});
            const parsed = await widget.parse('native family fixture');
            const rendered = await widget.render('family', 'native family fixture');
            results.push([parsed.diagramType, rendered.diagramType, rendered.svg.includes('<svg')]);
          }
          return results;
        }''')
        self.assertEqual(result, [['venn','venn',True], ['wardley','wardley',True], ['eventmodeling','eventmodeling',True]])


if __name__ == '__main__':
    unittest.main(verbosity=2)
