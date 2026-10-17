#!/usr/bin/env python3
"""Run the production SVG adapter in Chromium: actor menus and callback coexistence.

Requires Python Playwright and Chromium; CHROMIUM may override its executable.
The native SVG metadata is a boundary fixture, not a Mermaid parser or WASM mock.
FM_SVG_MODULE can name a baseline module for an explicit negative-control run.
No production source is rewritten. All DOM, URL, focus, popover and input APIs are real.
"""
from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import shutil

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]


def main() -> int:
    failures = []
    checks = 0
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch(
            executable_path=os.environ.get("CHROMIUM") or shutil.which("chromium"),
            headless=True, args=["--no-sandbox", "--disable-dev-shm-usage"])
        page = browser.new_page(viewport={"width": 900, "height": 700})
        page_errors = []
        page.on("pageerror", lambda error: page_errors.append(str(error)))
        source = Path(os.environ.get("FM_SVG_MODULE", ROOT / "web/mermaid-svg.mjs")).read_text()
        page.set_content('<!doctype html><html><head><base href="https://host.example/docs/"/></head><body></body></html>')
        page.evaluate("""async source => {
          const url = URL.createObjectURL(new Blob([source], {type:'text/javascript'}));
          try { window.adapter = await import(url); } finally { URL.revokeObjectURL(url); }
          window.expect = (value, why = 'assertion failed') => { if (!value) throw Error(why); };
          window.equal = (left, right) => expect(JSON.stringify(left) === JSON.stringify(right),
            JSON.stringify(left) + ' != ' + JSON.stringify(right));
          window.throws = fn => { let error; try { fn(); } catch (caught) { error = caught; }
            expect(error, 'expected rejection'); return error; };
          window.frame = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
          window.links = [{label:'Dashboard',url:'https://service.example/health'},
            {label:'Docs café 😀',url:'../reference?q=1&lang=en'}, {label:'Wiki',url:'https://wiki.example/'}];
          const xml = text => text.replaceAll('&','&amp;').replaceAll('"','&quot;').replaceAll('<','&lt;');
          window.svgSource = ({menus = links, raw, callback = false, wrapped = false, extra = ''} = {}) => {
            const data = xml(raw ?? JSON.stringify(menus));
            const actor = `<g id="fm-node-alice-0" class="fm-node fm-node-has-menu-links" data-id="Alice" data-menu-links="${data}" ${callback ? 'data-callback="inspect"' : ''}>
              <title>Alice service</title><rect x="20" y="20" width="170" height="70" fill="lightgray"/><text x="40" y="60">Alice</text></g>`;
            return `<svg xmlns="http://www.w3.org/2000/svg" role="img" width="600" height="300" viewBox="0 0 600 300">
              ${wrapped ? '<a href="https://primary.example/">'+actor+'</a>' : actor}
              <g id="fm-node-bob-1" class="fm-node" data-id="Bob" data-callback="inspect"><rect x="280" y="20" width="170" height="70" fill="lightgray"/></g>${extra}</svg>`;
          };
          window.cleanups = [];
          window.fresh = (options = {}) => {
            for (const cleanup of cleanups.splice(0)) cleanup();
            document.body.removeAttribute('style');
            document.body.innerHTML = '<button id="before">Before</button><div id="mount"></div><button id="after">After</button>';
            document.querySelector('base').href = 'https://host.example/docs/';
            window.calls = []; window.errors = []; window.live = true;
            const prepared = adapter.prepareSvg(svgSource(options), 'diagram', document);
            window.prepared = prepared;
            window.svg = prepared.element.cloneNode(true);
            document.querySelector('#mount').append(svg);
            window.actor = svg.querySelector('[data-id="Alice"]');
            window.bob = svg.querySelector('[data-id="Bob"]');
            // Callback identity must remain stable for the existing callback binder.
            const handler = id => calls.push(id);
            window.bind = adapter.createSvgBinder(prepared.element, {resolveCallback: () => handler,
              isLive: () => live, reportError: error => errors.push(String(error)), ...options.binder});
            window.cleanup = bind(document.querySelector('#mount')); cleanups.push(cleanup);
          };
          window.menu = () => document.querySelector('[role="menu"]');
          window.items = () => [...menu().querySelectorAll('[role="menuitem"]')];
          window.press = (node, key, extra = {}) => node.dispatchEvent(new KeyboardEvent('keydown',
            {key,bubbles:true,cancelable:true,...extra}));
          window.click = node => node.dispatchEvent(new MouseEvent('click',{button:0,bubbles:true,cancelable:true}));
          fresh();
        }""", source)

        def check(name, script):
            nonlocal checks
            checks += 1
            try:
                script() if callable(script) else page.evaluate("async () => { " + script + " }")
                print(json.dumps({"scenario_id": name, "surface": "svg", "passed": True}))
            except Exception as error:
                failures.append(name)
                print(json.dumps({"scenario_id": name, "surface": "svg", "passed": False, "error": str(error)}))

        check("menu-metadata-is-admitted-without-a-popup-or-network", """
          fresh(); expect(!menu()); equal(JSON.parse(actor.getAttribute('data-menu-links')), links);
          expect(actor.getAttribute('aria-haspopup') === 'menu'); expect(actor.getAttribute('aria-expanded') === 'false');
          expect(svg.getAttribute('role') === 'group'); expect(actor.getAttribute('tabindex') === '0');
          expect(actor.getAttribute('aria-label') === 'Alice service');
        """)
        check("click-opens-native-links-with-protected-targets", """
          fresh(); click(actor); expect(menu()); equal(items().map(el => el.textContent), links.map(link => link.label));
          equal(items().map(el => el.href), ['https://service.example/health','https://host.example/reference?q=1&lang=en','https://wiki.example/']);
          expect(items().every(el => el.target === '_blank' && el.rel === 'noopener noreferrer'));
          expect(actor.getAttribute('aria-controls') === menu().id); expect(document.activeElement === items()[0]); equal(calls,[]);
        """)
        check("keyboard-open-first-and-last", """
          for (const key of ['Enter',' ','ArrowDown']) { fresh(); actor.focus(); press(actor,key); expect(document.activeElement === items()[0],key); }
          fresh(); actor.focus(); press(actor,'ArrowUp'); expect(document.activeElement === items().at(-1));
        """)
        check("menu-keyboard-wrap-home-end-typeahead", """
          fresh(); press(actor,'ArrowDown'); press(document.activeElement,'ArrowUp'); expect(document.activeElement === items()[2]);
          press(document.activeElement,'ArrowDown'); expect(document.activeElement === items()[0]);
          press(document.activeElement,'End'); expect(document.activeElement === items()[2]);
          press(document.activeElement,'Home'); expect(document.activeElement === items()[0]);
          press(document.activeElement,'d'); expect(document.activeElement === items()[1]);
          press(document.activeElement,'w'); expect(document.activeElement === items()[2]);
        """)
        check("escape-returns-focus-and-clears-controls", """
          fresh(); click(actor); press(document.activeElement,'Escape'); expect(!menu());
          expect(document.activeElement === actor); expect(actor.getAttribute('aria-expanded') === 'false'); expect(!actor.hasAttribute('aria-controls'));
        """)
        check("callback-primary-action-is-not-hijacked", """
          fresh({callback:true}); click(actor); equal(calls,['Alice']); expect(!menu());
          press(actor,'Enter'); equal(calls,['Alice','Alice']); expect(!menu());
          press(actor,'ArrowDown'); expect(menu()); equal(calls,['Alice','Alice']);
        """)
        check("context-menu-keeps-both-actions-reachable", """
          fresh({callback:true}); const event = new MouseEvent('contextmenu',{bubbles:true,cancelable:true});
          actor.dispatchEvent(event); expect(event.defaultPrevented); expect(menu()); equal(calls,[]);
          press(document.activeElement,'Escape'); press(actor,'F10',{shiftKey:true}); expect(menu());
        """)
        check("ordinary-callback-remains-functional", """
          fresh(); click(bob); equal(calls,['Bob']); expect(!menu());
          press(bob,' '); bob.dispatchEvent(new KeyboardEvent('keyup',{key:' ',bubbles:true,cancelable:true})); equal(calls,['Bob','Bob']);
        """)
        check("callback-registry-replacement-stays-inert", """
          fresh({binder:{resolveCallback:()=>()=>calls.push('should-not-run')}}); click(bob); equal(calls,[]);
        """)
        check("primary-hyperlink-is-not-converted-to-menu-click", """
          fresh({wrapped:true}); const event = new MouseEvent('click',{button:0,bubbles:true,cancelable:true});
          // Prevent actual navigation only after the production root listener has run.
          svg.addEventListener('click', e => { window.primaryPrevented = e.defaultPrevented; e.preventDefault(); }, {once:true});
          actor.dispatchEvent(event); expect(!primaryPrevented); expect(!menu()); press(actor,'ArrowDown'); expect(menu());
        """)
        check("literal-labels-never-become-markup", """
          fresh({menus:[{label:'<img src=x onerror=alert(1)> & café 😀',url:'https://safe.example/'}]}); click(actor);
          expect(items()[0].textContent === '<img src=x onerror=alert(1)> & café 😀'); expect(!menu().querySelector('img,script'));
        """)
        check("menu-fragments-follow-svg-id-scoping", """
          fresh({menus:[{label:'Bob',url:'#fm-node-bob-1'}]});
          const raw = JSON.parse(actor.getAttribute('data-menu-links')); expect(raw[0].url === '#'+bob.id);
          click(actor); expect(items()[0].hash === '#'+bob.id); expect(items()[0].target === '_self');
        """)
        check("relative-destinations-do-not-follow-late-base-edits", """
          fresh(); document.querySelector('base').href = 'https://other.example/elsewhere/'; click(actor);
          expect(items()[1].href === 'https://host.example/reference?q=1&lang=en');
        """)
        check("empty-menu-leaves-node-inert", """
          fresh({menus:[]}); expect(!actor.hasAttribute('aria-haspopup')); click(actor); expect(!menu());
        """)
        for name, url in [("javascript", "javascript:alert(1)"), ("mixed-case-script", "JaVaScRiPt:alert(1)"),
                          ("control-obfuscation", "java\nscript:alert(1)"), ("data", "data:text/html,bad"),
                          ("file", "file:///tmp/x"), ("vbscript", "vbscript:bad")]:
            check("reject-unsafe-menu-url-" + name, lambda url=url: page.evaluate("""url => {
              const error = throws(() => adapter.prepareSvg(svgSource({menus:[{label:'Bad',url}]}),'bad',document));
              expect(error.message.includes('hyperlink'));
            }""", url))
        for name, value in [("object", '{}'), ("null", 'null'), ("invalid-json", '['),
                            ("missing-url", '[{"label":"Docs"}]'), ("empty-label", '[{"label":" ","url":"https://safe.example/"}]'),
                            ("extra-fields", '[{"label":"Docs","url":"https://safe.example/","target":"_top"}]')]:
            check("reject-malformed-menu-" + name, lambda value=value: page.evaluate("""raw => {
              throws(() => adapter.prepareSvg(svgSource({raw}),'bad',document));
            }""", value))
        check("menu-and-label-limits", """
          throws(() => adapter.prepareSvg(svgSource({menus:Array(129).fill(links[0])}),'bad',document));
          throws(() => adapter.prepareSvg(svgSource({menus:[{label:'a'.repeat(2049),url:'https://safe.example/'}]}),'bad',document));
          throws(() => adapter.prepareSvg(svgSource({menus:[{label:'Bad',url:'https://safe.example/'+ 'a'.repeat(8192)}]}),'bad',document));
          throws(() => adapter.prepareSvg(svgSource({raw:' '.repeat(128*1024+1)}),'bad',document));
          throws(() => adapter.prepareSvg(svgSource({raw:'[{"label":"\\ud800","url":"https://safe.example/"}]'}),'bad',document));
        """)
        check("data-attribute-case-confusion-is-rejected", """
          throws(() => adapter.prepareSvg(svgSource().replace('data-menu-links=','DATA-MENU-LINKS='),'bad',document));
        """)
        check("diagram-wide-link-budget-is-enforced", """
          const parsed = new DOMParser().parseFromString(svgSource({menus:Array(128).fill(links[0])}),'image/svg+xml');
          const first=parsed.querySelector('[data-id="Alice"]');
          for (let i=0;i<32;i++) { const copy=first.cloneNode(true); copy.id='node'+i; parsed.documentElement.append(copy); }
          throws(() => adapter.prepareSvg(new XMLSerializer().serializeToString(parsed),'bad',document));
        """)
        check("metadata-change-revokes-an-open-menu", """
          fresh(); click(actor); actor.setAttribute('data-menu-links','[]'); await frame(); expect(!menu());
          click(actor); expect(!menu());
        """)
        check("detached-root-clears-open-menu", """
          fresh(); click(actor); svg.remove(); await frame(); expect(!menu());
        """)
        check("source-node-replacement-does-not-retain-stale-handlers", """
          fresh(); click(actor); const copy=actor.cloneNode(true); actor.replaceWith(copy); await frame(); expect(!menu());
          click(copy); expect(!menu());
        """)
        check("inactive-owner-cannot-open-or-activate-menu", """
          fresh(); live=false; click(actor); expect(!menu());
          fresh(); click(actor); live=false;
          const event=new MouseEvent('click',{button:0,bubbles:true,cancelable:true}); items()[0].dispatchEvent(event);
          expect(event.defaultPrevented); expect(!menu());
        """)
        check("menu-links-cannot-be-replaced-after-admission", """
          fresh(); click(actor); const link=items()[0]; link.href='javascript:alert(1)';
          const event=new MouseEvent('click',{button:0,bubbles:true,cancelable:true}); link.dispatchEvent(event);
          expect(event.defaultPrevented); expect(!menu());
        """)
        check("hidden-inert-disabled-or-zero-scale-nodes-do-not-open", """
          for(const attribute of ['inert','hidden','aria-hidden','aria-disabled']) {
            fresh(); actor.setAttribute(attribute,'true'); click(actor); expect(!menu(),attribute);
          }
          fresh(); actor.style.display='none'; click(actor); expect(!menu());
          fresh(); actor.setAttribute('transform','scale(0)'); click(actor); expect(!menu());
        """)
        check("outside-pointer-and-focus-dismiss-without-stealing-focus", """
          fresh(); click(actor); document.querySelector('#after').dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,composed:true})); expect(!menu());
          click(actor); document.querySelector('#after').focus(); expect(!menu()); expect(document.activeElement.id==='after');
        """)
        check("same-root-binding-is-idempotent-and-cleanup-restores-template", """
          fresh(); expect(bind(svg)===cleanup); click(actor); expect(document.querySelectorAll('[role="menu"]').length===1);
          cleanup(); cleanup(); expect(!menu()); expect(svg.isEqualNode(prepared.element),'all owned attributes must restore');
          click(actor); expect(!menu()); cleanups.push(bind(svg)); click(actor); expect(menu());
        """)
        check("cleanup-preserves-later-host-attribute-edits", """
          fresh(); click(actor); actor.setAttribute('role','link'); actor.setAttribute('aria-expanded','mixed'); cleanup();
          expect(actor.getAttribute('role')==='link'); expect(actor.getAttribute('aria-expanded')==='mixed'); expect(!menu());
        """)
        check("binding-failure-restores-partially-installed-callbacks", """
          fresh(); cleanup(); const original=svg.cloneNode(true); throws(() => adapter.createSvgBinder(svg,{onBind:()=>{throw Error('host')}})(svg));
          expect(svg.isEqualNode(original)); expect(!menu());
        """)
        check("menu-repositions-after-transform-and-resize", """
          fresh(); click(actor); const x=menu().getBoundingClientRect().x;
          actor.setAttribute('transform','translate(100,40)'); await frame(); expect(menu().getBoundingClientRect().x>x+90);
          expect(menu().getBoundingClientRect().bottom<=innerHeight); expect(menu().getBoundingClientRect().right<=innerWidth);
        """)
        check("top-layer-menu-escapes-clipped-transformed-container", """
          fresh(); const mount=document.querySelector('#mount'); mount.style.cssText='height:40px;width:100px;overflow:hidden;transform:translate(20px,30px)';
          press(actor,'ArrowDown'); expect(menu().matches(':popover-open')); const r=items()[0].getBoundingClientRect();
          expect(document.elementFromPoint(r.x+r.width/2,r.y+r.height/2)===items()[0]);
        """)
        check("shadow-root-keeps-menu-and-aria-controls-in-one-tree", """
          fresh(); cleanup(); svg.remove(); const host=document.createElement('div'); document.body.append(host);
          const shadow=host.attachShadow({mode:'open'}); shadow.append(svg); cleanups.push(adapter.createSvgBinder(svg)(svg));
          press(actor,'ArrowDown'); const popup=shadow.querySelector('[role="menu"]'); expect(popup);
          expect(actor.getAttribute('aria-controls')===popup.id); expect(shadow.activeElement===popup.querySelector('a'));
          press(shadow.activeElement,'Escape'); expect(!shadow.querySelector('[role="menu"]')); expect(shadow.activeElement===actor);
        """)
        check("multiple-diagrams-remain-independent", """
          fresh(); click(actor); const first=menu(); const other=adapter.prepareSvg(svgSource(),'other',document);
          const root=other.element.cloneNode(true); document.body.append(root); cleanups.push(adapter.createSvgBinder(other.element)(root));
          press(root.querySelector('[data-id="Alice"]'),'ArrowDown'); expect(!first.isConnected); expect(menu());
          expect(menu().id.startsWith('other--')); cleanup(); expect(menu());
        """)

        def real_input():
            page.evaluate("fresh()")
            page.locator('[data-id="Alice"]').click()
            page.evaluate("expect(menu()); expect(document.activeElement===items()[0])")
            page.keyboard.press("ArrowDown")
            page.evaluate("expect(document.activeElement===items()[1])")
            page.keyboard.press("Escape")
            page.evaluate("expect(!menu()); expect(document.activeElement===actor)")
            page.keyboard.press("Space")
            page.evaluate("expect(menu()); expect(document.activeElement===items()[0])")
            page.keyboard.press("Tab")
            page.evaluate("expect(!menu()); expect(document.activeElement===bob)")
        check("real-pointer-keyboard-space-tab", real_input)
        check("no-uncaught-browser-errors", lambda: page.evaluate("errors => expect(errors.length===0,JSON.stringify(errors))", page_errors))
        browser.close()
    print(json.dumps({"checks": checks, "failures": failures,
        "implementation_sha256": hashlib.sha256(source.encode()).hexdigest()}))
    return int(bool(failures))


if __name__ == "__main__":
    raise SystemExit(main())
