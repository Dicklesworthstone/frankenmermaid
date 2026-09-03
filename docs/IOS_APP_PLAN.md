# FrankenMermaid for Apple Platforms

Status: implementation plan, reviewed before application code is added

## 1. Product promise

FrankenMermaid is a private, offline diagram studio for iPhone, iPad, and Mac. It turns Mermaid source into deterministic, accessible diagrams through the existing Rust engine and makes the engine’s best differentiators—source-aware diagnostics, many diagram families, themes/layouts, graph lenses, and Graph Deck presentations—feel native on Apple hardware.

SwiftUI owns the product experience. A tightly sandboxed local `WKWebView` hosts the exact tracked WebAssembly renderer for SVG/canvas/deck output; it is a rendering surface, not the app UI. There is no account, server render, analytics, CDN, external font, or remote JavaScript.

The visual direction is an underwater galvanic cartography laboratory: premium dark glass, emerald/cyan bioluminescence, restrained amber brass, a friendly suite monster, and graph energy that visibly follows the user’s real nodes and edges. It should be spectacular without looking like a game wrapped around an editor.

## 2. Non-negotiable constraints

- Entirely on device; no source or rendered diagram is uploaded.
- One diagram engine. Swift never reimplements parsing, layout, source spans, or SVG.
- All diagram animation derives from the user’s actual parsed/rendered structure. No random matrix letters or decorative fake nodes.
- Link/click directives are non-navigating in preview by default. External navigation requires a visible user gesture and destination confirmation.
- Source limits, layout limits, export limits, and the engine’s existing security posture apply at the native boundary.
- Render feedback reports actual phases and timings available from the worker/bridge. Unknown internal progress is not represented as a percentage.
- iPhone, iPad, and Mac have purpose-built layouts.
- Accessibility, Reduce Motion, high contrast, keyboard use, pointer use, and VoiceOver are launch requirements.

## 3. Engine and bridge

### 3.1 Bundled renderer

The app bundles the repository’s tracked package:

- `pkg/frankenmermaid.js`;
- `pkg/frankenmermaid_bg.wasm`;
- `pkg/frankenmermaid.d.ts`; and
- `crates/fm-cli/src/deck_runtime.js` after byte-identity verification against deployed copies.

`ios/sync-renderer.sh` copies these into `ios/Renderer/`, writes a SHA-256 manifest, and has a `--check` mode used by release verification. The app ships the applicable license text and version/source hash in About.

### 3.2 Offline scheme

`FrankenResourceSchemeHandler` serves a strict allow-list at `frankenmermaid-resource://bundle/` with correct JavaScript/WebAssembly MIME types. The internal resource scheme is intentionally distinct from the public `frankenmermaid://` deep-link scheme. It rejects traversal and does not forward unknown URLs. Navigation delegates reject HTTP/HTTPS and any non-allow-listed custom URL.

The renderer page exposes a versioned native protocol for:

- engine initialization and capability report;
- type detection and strict config/directive validation;
- SVG rendering;
- diagram description and accessibility summary;
- parse/diagram lenses and source edits;
- graph-deck render plus manifest;
- bounded SVG/PNG export chunks;
- click/source-span events; and
- cancellation/supersession.

Commands carry request IDs. Live edits use a short debounce and the worker path when available so stale render requests can be cancelled without freezing native controls.

### 3.3 Structured results

Every result returns structured metadata rather than scraping SVG:

- diagram family and detected type;
- warnings/errors with source spans where available;
- SVG viewBox and element count;
- real parse/layout/render/total timings when the worker response exposes them, otherwise a single measured Rust-core duration;
- accessible description;
- lens snapshot/source element mapping; and
- deck manifest summary when present.

Large SVG/PNG outputs use the same ordered 64 KiB chunk protocol as FrankenMarkdown. Swift validates lengths, bounds, and request identity before publishing a temporary export URL.

## 4. Native document model

`MermaidDocument` is a normal UTF-8 text document supporting `.mmd`, `.mermaid`, and plain-text imports. App presentation state—theme, layout/config, camera, inspector selection—is stored outside the source unless the user explicitly chooses to write directives.

Inputs:

- Files/document browser (bounded UTF-8 `.mmd`, `.mermaid`, and plain-text import is shipped;
  in-place document editing remains future work);
- Share extension text or one compatible file;
- clipboard paste through an explicit user action;
- drag/drop on iPad and Mac;
- starter templates for every supported diagram family; and
- deep link/App Intent requests to create a chosen diagram type.

Outputs:

- SVG with namespace, accessible metadata, embedded theme styling, and deterministic source output;
- bounded transparent PNG from the exact SVG stage at up to 2x scale (shipped; explicit scale/background controls remain future work);
- PDF through WebKit’s local PDF generation over the exact SVG stage (shipped);
- original Mermaid source; and
- Graph Deck presentation HTML package when a deck manifest is present.

Export uses Quick Look and ShareLink. Clipboard actions distinguish Copy Source, Copy SVG, and Copy Image.

## 5. Information architecture

- **Studio**: source, live diagram canvas, and core render controls.
- **Inspector**: diagram type, theme, supported configuration/layout controls, camera, accessibility, and export appearance.
- **Diagnostics**: strict config/directive errors, parser/layout warnings, and source-linked issues.
- **Lens**: rendered node/edge/cluster selection, exact source-binding inspection, and bounded source edits through the engine lens are shipped.
- **Deck**: create/edit deck metadata, rehearse, present, and export.
- **Gallery**: searchable built-in samples for all supported families plus recent documents.

The app opens directly into a useful starter diagram. The user can type immediately; no “bring the engine alive” button exists.

## 6. Platform-specific composition

### 6.1 iPhone

- Native segmented tabs switch **Code**, **Diagram**, and **Inspect** so neither editor nor canvas is squeezed.
- A compact status bar shows detected family, diagnostic count, measured render time, and friendly monster state.
- Diagram mode is a full-screen pan/zoom stage with Fit, 100%, Focus Selection, and Present controls that avoid obscuring content.
- Selecting an element opens a native bottom sheet with its source-linked lens editor and actions.
- Theme and export options use concise sheets; the keyboard dismisses naturally when the user taps or drags outside the editor.
- Deck presentation becomes a true landscape/full-screen theater with tap/swipe/keyboard navigation and minimal chrome.

### 6.2 iPad

- Regular width uses an adjustable editor/canvas split with a trailing inspector.
- The canvas remains large and independently pannable while the source selection and lens inspector stay synchronized.
- Stage Manager/narrow widths collapse the inspector first, then fall back to tabbed mode.
- Pencil/Scribble works in the editor; pointer hover highlights real diagram elements; drag/drop imports source and exports SVG/PNG.
- Deck mode treats iPad as both presentation display and presenter console, with scene list, current slide, next-slide summary, and external-display readiness where platform APIs permit.
- Hardware keyboard commands cover render, fit, zoom, theme cycling, diagnostics, search, and deck navigation.

### 6.3 Mac

- Native-design Catalyst uses a three-pane `NavigationSplitView`: gallery/documents, editor/canvas workspace, inspector/diagnostics.
- Editor and canvas divider positions persist per window; the canvas can detach into a second window for a clean presentation or review surface.
- Standard File/Edit/View/Diagram/Present/Window menus provide New, Open, Save, Save As, Find, Replace, Render (`⌘R`), Fit (`⌘0`), Actual Size (`⌘1`), Zoom, Export (`⇧⌘E`), Present (`⇧⌘P`), and inspector toggles.
- Multiwindow documents, right-click element actions, drag/drop, menu validation, pointer hover, titlebar toolbar items, full screen, and state restoration are first-class.
- Touch-sized controls and phone sheets do not get stretched into desktop chrome.

## 7. Spectacular real-data visualization

`GraphReactorView` visualizes the actual render pipeline and graph:

`SOURCE → PARSE → IR → LAYOUT → SVG/CANVAS`

When parse/lens data is available, the reactor’s particles correspond to real node IDs and traverse real edge relationships. When only a combined core duration is observable, the middle pipeline remains energized as one measured core phase. The UI never invents characters, nodes, text, confidence, or progress.

The main stage adds three polished effects derived from real output:

1. **Birth animation**: new/changed nodes materialize from their true final positions, with edges drawing between actual endpoints.
2. **Current flow**: a restrained bioluminescent pulse can travel along rendered edge paths while idle; it automatically stops under Reduce Motion, Low Power Mode, or thermal pressure.
3. **Lens focus**: tapping a real element bends surrounding light toward it and connects it to the matching source range, making the source/diagram relationship educational.

During a render, the native overlay reports the actual active phase, measured milliseconds, graph family, node/edge counts when known, warnings, and cancellation. Completion has a subtle galvanic lock-in and haptic, never a blocking victory screen.

Graph Deck uses the repository’s canonical deck runtime. Scene camera moves, reveal steps, overview tours, tooltips, and morphing use the engine manifest—not duplicated Swift heuristics.

## 8. Shared FrankenSuite design system

The app shares the suite’s semantic component vocabulary:

- `Lab` tokens with emerald primary, cyan live/selection, amber presentation/emphasis, and red only for errors;
- adaptive `LaboratoryBackground`, `LabPanel`, `LabLabel`, button styles, status lines, progress visuals, and high-contrast fallbacks;
- `MonsterStatusMark` with a graph-core instrument;
- native materials and SF Symbols for platform controls;
- coherent motion/haptic grammar across all FrankenSuite apps; and
- typography/spacing tuned separately for touch and pointer environments.

Its app icon uses the same friendly monster identity as FrankenMarkdown, holding a glowing glass graph heart in an underwater laboratory with a subtle tail/fin silhouette. It has no text or baked-in mask. The existing website illustration contributes underwater palette and optional gallery/onboarding art but is not used as the app-family character reference.

## 9. Apple integration

- **Files/document browser**: edit standard Mermaid text in place.
- **Share extension**: accept source text or one compatible file into the App Group inbox and open Studio.
- **Widgets**: small New Diagram launcher; medium recent-diagram widget showing title, family, and an opt-in sanitized thumbnail. Source text is never exposed by default.
- **App Intents / Shortcuts**: New Flowchart, New Sequence Diagram, New Diagram by Type, Open Diagram Studio, and Present Recent Deck.
- **Spotlight**: opt-in title/family/date indexing; source text and labels remain private by default.
- **Handoff**: continue document URL, selected element ID, and camera state when the chosen document provider makes the file available.
- **Quick Look and ShareLink** for SVG/PNG/PDF/deck artifacts.
- **Printing** from the generated PDF.
- **External display** for deck presentation where Catalyst/iPad scene APIs support it.
- **Dynamic Island / Live Activity** is omitted for normal fast renders. Deck presentation controls belong in the app/lock-screen media-style affordance only if a future supported API and real user value justify it; the app does not misuse Live Activities as decoration.

## 10. Privacy and security

- Privacy manifest: no collected data, no tracking, no tracking domains.
- App Store privacy: Data Not Collected.
- Review notes: all source/rendering remains on device; no third-party AI service or remote diagram service exists.
- Content Security Policy permits only the custom resource scheme and inline data needed by the local renderer; no remote connect/image/font/script origins.
- Scheme handler canonicalizes paths and serves an allow-list only.
- External diagram links are disabled by default; a user gesture and destination confirmation are required to leave the app.
- Deck text reaches the DOM through `textContent`; untrusted text is never interpolated as HTML.
- Renderer-produced SVG is the only markup inserted into the stage and is governed by the engine’s sanitizer/security mode.
- Bridge schema, request IDs, source lengths, config sizes, node/edge budgets, export bytes, and chunk counts are bounded.
- Unknown message types and stale request IDs are rejected.

## 11. Accessibility and localization

- The engine’s `describeDiagram` output backs an accessible diagram summary and element list.
- Users can navigate nodes/edges through a native accessibility rotor/list rather than depending on spatial SVG exploration.
- Source-linked diagnostics include severity, line/column, and focused selection.
- Graph colors are paired with shapes/labels and tested for contrast.
- Reduce Motion replaces birth/current/lens travel with opacity and direct state changes; deck camera transitions become immediate.
- Decorative underwater bubbles/particles are hidden from accessibility.
- All native strings are localizable; Mermaid syntax and identifiers remain verbatim.

## 12. Targets and identifiers

Planned XcodeGen source of truth: `ios/project.yml`.

- App: `com.frankenmermaid.FrankenMermaid`
- Widget: `com.frankenmermaid.FrankenMermaid.Widgets`
- Share extension: `com.frankenmermaid.FrankenMermaid.Share`
- App Group: `group.com.frankenmermaid.FrankenMermaid`
- URL scheme: `frankenmermaid://`
- Deployment: iOS/iPadOS 17+, Mac Catalyst 14+ through the iOS 17 target
- Device families: iPhone and iPad; Mac Catalyst native design
- Initial version: `1.0`, build `1`
- Category: Developer Tools, with Productivity as secondary if App Store Connect permits
- Price: Free, worldwide where App Store Connect permits

## 13. Verification gates

1. Build/verify the tracked WASM package and record source hash, raw/gzip size, and smoke renders for representative families.
2. Verify the canonical deck runtime byte-identically matches every shipped copy.
3. Run `ios/sync-renderer.sh --check` and validate renderer SHA-256 manifest.
4. Generate with XcodeGen; review source-of-truth project changes.
5. Unit-test scheme canonicalization, MIME types, CSP/navigation rejection, bridge schema, cancellation, chunk ordering, bounds, diagnostics/source span conversion, and camera-state persistence.
6. Golden-test SVG export and representative source/type/theme combinations against the browser package.
7. UI-test create/import/edit/undo/render/lens/diagnostics/theme/export/share/deck flows, malformed source, security directives, huge graphs, and state restoration.
8. Build Debug and Release for Apple Silicon iPhone simulator, iPad simulator, physical iPhone, physical iPad, and arm64 Mac Catalyst.
9. Test Dynamic Type XXXL, VoiceOver/rotor, Voice Control, Reduce Motion, Reduce Transparency, contrast, light/dark appearance, Stage Manager widths, keyboard, pointer, Pencil, memory pressure, Low Power Mode, and thermal adaptation.
10. Confirm full functionality with networking disabled.
11. Run the repository’s UBS scanner on every changed source file and complete the repository-required test/commit/push process, including `main:master` synchronization.
12. Archive/validate distribution signing, entitlements, privacy manifest, bundle contents, and absence of unexpected remote origins or SDKs.
13. Capture final iPhone, iPad, and Mac screenshots from the release candidate only.

## 14. Delivery order

1. Verify and freeze the WASM/deck artifacts.
2. Add the plan-reviewed XcodeGen skeleton, identifiers, privacy files, icon, and renderer resources.
3. Implement strict offline scheme and versioned bridge with tests.
4. Implement document model, editor, gallery, import, state restoration, and native commands.
5. Implement SVG stage, diagnostics, descriptions, themes/config, lens selection/editing, export, and sharing.
6. Implement Graph Deck theater/presenter experience from the canonical manifest/runtime.
7. Implement adaptive iPhone/iPad/Mac compositions.
8. Implement the real-data graph reactor, accessibility reductions, and suite polish.
9. Add Share extension, widget, App Intents, Spotlight opt-in, and Handoff.
10. Install/test on physical devices and Mac, then perform a two-pass fresh-eyes review.
11. Create the App Store Connect record only after identity, signing, privacy posture, icon, and builds are real.
