# FrankenMermaid Apple app progress

The repository Beads store is currently in unresolved sync-merge state. This file records implementation progress without mutating or bypassing that tracker conflict; migrate these items into the canonical tracker after the sync state is repaired.

## 2026-08-28 foundation

- Added an XcodeGen universal iPhone/iPad/Mac Catalyst project using bundle ID `com.frankenmermaid.FrankenMermaid`.
- Bundled the tracked `pkg/frankenmermaid.js`, `pkg/frankenmermaid_bg.wasm`, type declarations, and canonical Graph Deck runtime with a SHA-256 manifest.
- Added a strict allow-list `WKURLSchemeHandler`, nonpersistent WebKit data store, no remote navigation, explicit WASM `ArrayBuffer` initialization, and CSP permitting only the bundled scheme.
- Added native SwiftUI Code/Diagram/Inspect lanes for compact widths and a side-by-side iPad/Mac studio, generated icon, suite theme, privacy manifest, measured render metadata, device-neutral footer, and a deterministic screenshot launch hook.
- Verified green iPhone simulator, iPad simulator, and unsigned Catalyst builds. Live captures rendered the real starter graph; the first clipped iPhone SVG was fixed by constraining the local stage before acceptance.

## Still open

- In-place document persistence, autosave, state restoration, Quick Look, Graph Deck, widgets, intents, Spotlight/Handoff, menus, and multiwindow.
- Remaining 19 visual rounds per platform in `FRANKEN_DOCUMENT_APPS_QA.md`.
- Signed physical-device and Catalyst builds, distribution archive, App Store metadata, and worldwide availability.

## 2026-09-02 engine parity and native insight

- Added a build-time package gate that stages the tracked `pkg/` JavaScript/WASM/type artifacts and canonical deck runtime into the app bundle only when their SHA-256 values match the reviewed manifest. A fresh clone no longer depends on the ignored local WASM file under `ios/Renderer/`.
- Wired the existing Rust/WASM `parse` and `describeDiagram` exports into every native render. Studio now exposes the engine-authored semantic description and structured parser/recovery diagnostics in the persistent Inspect lane, including source locations and suggestions when the core supplies them.
- The bridge and Swift model retain request-ID rejection so an older parse/description cannot overwrite insight for newer source.
- Themes/config, sample gallery, and SVG/source/animated-HTML sharing had already landed after the original progress snapshot; they are no longer open parity items. At this snapshot, the remaining high-value gaps were document persistence/import, source-linked lens editing, PNG/PDF, and Graph Deck presentation.

## 2026-09-03 App Store build 2 preparation

- Added bounded Files import for UTF-8 `.mmd`, `.mermaid`, and plain-text source.
- Added PNG and PDF export beside the existing source, SVG, and self-contained animated HTML exports.
- Added source-lens inspection and engine-owned exact source editing with request, range, stale-source, and size validation.
- Bumped the iOS build number to 2 and added app-bound UI evidence for the private source studio, live diagram stage, export formats, Rust diagnostics/source lens, and 24-family sample gallery.
- Runtime UI evidence, release archive validation, replacement App Store screenshots, build upload, reviewer reply, and resubmission remain gated on a passing disk-safety preflight.

## 2026-09-04 compact source-lens ownership

- Made every source-bound SVG node, edge, and cluster a deterministic accessible button instead of an incorrectly announced toggle, while retaining the current-selection state separately.
- On compact iPhone layouts, tapping a real rendered element now opens its exact engine-authored source binding in a native Source Lens sheet instead of burying the editor below style controls and a long semantic description.
- Replaced the heading-only storefront check with a real iPhone interaction: select the rendered `Source` node, edit the bound UTF-8 range, require the bundled Rust `applyLensEdit` receipt, dismiss only after acceptance, and confirm that the native source editor changed.
- Extended `scripts/dsr-apple-quality.sh` with Swift parsing, privacy-plist validation, a concrete audio-fenced iPhone lane, dark/light terminate-and-relaunch persistence, and the exact source-lens edit path. DSR receipt `20260904T020732-88811` passes the generic iOS Simulator build, 13/13 Catalyst unit tests, and 2/2 iPhone UI tests.
- Retained DSR screenshots show the direct Source Lens sheet, Rust-updated source, and remembered light appearance. Physical-device interaction, iPad/Catalyst source-lens UI, document ownership, and Graph Deck remain open.
