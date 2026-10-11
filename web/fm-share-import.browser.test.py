#!/usr/bin/env python3
"""Real Chromium coverage for local shared-link import controls.

Uses production fm-share.js and browser streams/DOM/events. The document workspace
is an explicit transaction-boundary fixture; this is not a full WASM/editor test.
Requires Playwright and Chromium; no network access or downloaded browser is needed.
"""
import base64
import json
import os
from pathlib import Path
import shutil
import zlib

from playwright.sync_api import sync_playwright


def live_link(code, mode="pako"):
    state = json.dumps({"code": code, "mermaid": {"securityLevel": "loose"}}, ensure_ascii=False).encode()
    payload = zlib.compress(state) if mode == "pako" else state
    return "https://mermaid.live/edit#" + mode + ":" + base64.urlsafe_b64encode(payload).decode().rstrip("=")


def main():
    executable = os.environ.get("CHROMIUM_PATH") or shutil.which("chromium") or shutil.which("chromium-browser")
    if not executable:
        raise RuntimeError("Chromium is required; set CHROMIUM_PATH to its executable.")
    source = Path(__file__).with_name("fm-share.js").read_text()
    checked = []
    with sync_playwright() as p:
        browser = p.chromium.launch(executable_path=executable, headless=True, args=["--no-sandbox"])
        page = browser.new_page()
        requests, errors = [], []
        page.on("request", lambda request: requests.append(request.url))
        page.on("pageerror", lambda error: errors.append(str(error)))
        page.route("**/*", lambda route: route.abort())
        page.set_content('<textarea id="source" aria-label="Current source">local edits</textarea><div id="panel"></div>')
        page.evaluate("""async source => {
            const module = await import('data:text/javascript;base64,' + btoa(unescape(encodeURIComponent(source))));
            const editor = document.getElementById('source');
            let revision = 0;
            window.approve = true;
            window.commits = 0;
            editor.addEventListener('input', () => revision++);
            window.controls = module.mountShareControls({ panelEl: document.getElementById('panel'), workspace: {
                sourceSnapshot() { return { source: editor.value, name: 'local.mmd', revision }; },
                async openSharedSource(read, { isCurrent }) {
                    const captured = revision;
                    const incoming = await read();
                    if (window.pauseImport) {
                        window.waiting = true;
                        await new Promise(resolve => { window.releaseImport = resolve; });
                        window.waiting = false;
                    }
                    if (!isCurrent() || captured !== revision || !window.approve) return false;
                    editor.value = incoming.source;
                    revision++;
                    window.commits++;
                    return true;
                }
            }});
            await window.controls.ready;
        }""", source)
        field = page.get_by_label("Import a FrankenMermaid or Mermaid Live link")
        button = page.get_by_role("button", name="Import linked source")
        assert button.is_disabled()

        text = '\ufeff%% 世界 😀\r\nflowchart LR\r\nA-->B\n'
        field.fill(live_link(text))
        field.press("Enter")
        page.wait_for_function("window.commits === 1")
        # HTML textarea values normalize CRLF to LF; the codec's exact preservation
        # is separately asserted below without a textarea boundary.
        assert page.locator("#source").input_value() == text.replace("\r\n", "\n")
        assert "settings were not applied" in page.locator("#share-import-status").inner_text()
        assert page.url == "about:blank"
        checked.append("keyboard import, real DEFLATE, Unicode, source-only notice and unchanged history")

        for mode in ("pako", "base64"):
            result = page.evaluate("""async ({ source, link }) => {
                const module = await import('data:text/javascript;base64,' + btoa(unescape(encodeURIComponent(source))));
                return module.decodeShareLink(link);
            }""", {"source": source, "link": live_link(text, mode)})
            assert result == {"source": text, "name": "diagram.mmd"}
        checked.append("both codecs preserve source exactly before the textarea boundary")

        page.evaluate("window.approve = false")
        field.fill(live_link("declined"))
        assert page.evaluate("window.controls.importLink()") is False
        assert "declined" not in page.locator("#source").input_value()
        assert "retained" in page.locator("#share-import-status").inner_text()
        page.evaluate("window.approve = true")
        field.fill("javascript:alert(1)#base64:e30")
        assert page.evaluate("window.controls.importLink()") is False
        assert "HTTP" in page.locator("#share-import-status").inner_text()
        checked.append("declined replacements and unsafe links retain the current source")

        page.evaluate("window.pauseImport = true")
        field.fill(live_link("stale"))
        button.click()
        page.wait_for_function("window.waiting")
        field.fill(live_link("replacement"))
        page.evaluate("window.pauseImport = false; window.releaseImport()")
        page.wait_for_function("!window.waiting")
        assert "stale" not in page.locator("#source").input_value()
        assert "Link changed" in page.locator("#share-import-status").inner_text()
        assert page.evaluate("window.controls.importLink()") is True
        assert page.locator("#source").input_value() == "replacement"
        checked.append("editing a pending link cancels stale publication and permits retry")

        page.evaluate("window.pauseImport = true")
        field.fill(live_link("must not overwrite typing"))
        button.click()
        page.wait_for_function("window.waiting")
        page.locator("#source").fill("new typing")
        page.evaluate("window.pauseImport = false; window.releaseImport()")
        page.wait_for_function("!window.waiting")
        assert page.locator("#source").input_value() == "new typing"
        checked.append("workspace revision guard retains edits made during import")

        hostile = '</textarea><img src=x onerror="window.executed=true"><script>window.executed=true</script>'
        field.fill(live_link(hostile, "base64"))
        assert page.evaluate("window.controls.importLink()") is True
        assert page.locator("#source").input_value() == hostile
        assert page.evaluate("window.executed === undefined")
        assert page.locator("img, script").count() == 0
        checked.append("hostile source stays inert text")

        page.evaluate("window.controls.dispose()")
        assert button.is_disabled() and field.is_disabled()
        assert page.evaluate("window.controls.importLink()") is False
        assert requests == [], requests
        assert errors == [], errors
        checked.append("disposal disables controls; no network requests or page errors")
        browser.close()
    print(json.dumps({"passed": len(checked), "checks": checked}, indent=2))


if __name__ == "__main__":
    main()
