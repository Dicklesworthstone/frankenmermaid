# FrankenMermaid Apple app progress

The repository Beads store is currently in unresolved sync-merge state. This file records implementation progress without mutating or bypassing that tracker conflict; migrate these items into the canonical tracker after the sync state is repaired.

## 2026-08-28 foundation

- Added an XcodeGen universal iPhone/iPad/Mac Catalyst project using bundle ID `com.frankenmermaid.FrankenMermaid`.
- Bundled the tracked `pkg/frankenmermaid.js`, `pkg/frankenmermaid_bg.wasm`, type declarations, and canonical Graph Deck runtime with a SHA-256 manifest.
- Added a strict allow-list `WKURLSchemeHandler`, nonpersistent WebKit data store, no remote navigation, explicit WASM `ArrayBuffer` initialization, and CSP permitting only the bundled scheme.
- Added native SwiftUI Code/Diagram/Inspect lanes for compact widths and a side-by-side iPad/Mac studio, generated icon, suite theme, privacy manifest, measured render metadata, device-neutral footer, and a deterministic screenshot launch hook.
- Verified green iPhone simulator, iPad simulator, and unsigned Catalyst builds. Live captures rendered the real starter graph; the first clipped iPhone SVG was fixed by constraining the local stage before acceptance.

## Still open

- Package sync/check script and byte-identity gate.
- Document browser, imports, autosave, state restoration, diagnostics, source lens, themes/config, export/Quick Look/share, Graph Deck, widgets, intents, Spotlight/Handoff, menus, and multiwindow.
- Remaining 19 visual rounds per platform in `FRANKEN_DOCUMENT_APPS_QA.md`.
- Signed physical-device and Catalyst builds, distribution archive, App Store metadata, and worldwide availability.
