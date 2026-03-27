# FrankenMermaid Pattern Inventory

Status: active planning artifact

Primary bead:
- `bd-2u0.5.10.1` Create structured pattern inventory from frankentui and frankentui_website

Related artifacts:
- `evidence/demo_strategy.md`
- `evidence/capability_scenario_matrix.json`
- `frankenmermaid_demo_showcase.html`

## Purpose

This inventory captures reusable product, architecture, and demo-delivery patterns mined from `frankentui` and `frankentui_website`, then translates them into concrete guidance for FrankenMermaid.

The goal is not parity theater. The goal is to identify patterns that make the FrankenMermaid demo:
- easier to understand quickly
- more honest about current capability depth
- easier to validate and keep correct over time

## Source Basis

Primary upstream references:
- `/dp/frankentui/docs/spec/mermaid-showcase.md`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_mega_showcase.rs`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/screen_snapshots.rs`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/terminal_web_parity.rs`
- `/dp/frankentui/demo.yaml`
- `/dp/frankentui/scripts/demo_showcase_screen_sweep_e2e.sh`
- `/dp/frankentui/crates/ftui-showcase-wasm/src/runner_core.rs`
- `/dp/frankentui_website/app/page.tsx`
- `/dp/frankentui_website/app/showcase/page.tsx`
- `/dp/frankentui_website/app/getting-started/page.tsx`
- `/dp/frankentui_website/app/web_react/page.tsx`
- `/dp/frankentui_website/app/architecture/page.tsx`
- `/dp/frankentui_website/app/how-it-was-built/page.tsx`
- `/dp/frankentui_website/components/section-shell.tsx`
- `/dp/frankentui_website/lib/content.ts`
- `/dp/frankentui_website/lib/wasm-loader.ts`
- `/dp/frankentui_website/scripts/sync-showcase.sh`
- `/dp/frankentui_website/scripts/update-web-demo.sh`
- `/dp/frankentui_website/tests/web-demo.spec.ts`

Current FrankenMermaid artifacts used for applicability checks:
- `frankenmermaid_demo_showcase.html`
- `evidence/demo_strategy.md`
- `evidence/capability_scenario_matrix.json`

## Pattern Inventory

### P1. Intent-segmented landing flow

Pattern:
- Split users early by intent instead of forcing one generic hero path.

Why it matters:
- FrankenMermaid serves different evaluation modes: try a live demo, inspect architecture, embed via WASM, or adopt through CLI/docs workflows.

Applicability to FrankenMermaid:
- The current standalone showcase already has the right raw ingredients: sticky section navigation, a featured spotlight, gallery browsing, and a live playground in `frankenmermaid_demo_showcase.html`.
- The next refinement should make the top of the experience explicitly route to the highest-value journeys from `evidence/demo_strategy.md`: messy-input recovery, multi-surface parity, CI determinism, and embeddable runtime.

Recommended use:
- Add explicit entry points for `Try Recovery Story`, `Compare Surfaces`, `Inspect Browser Runtime`, and `See CI/Determinism Evidence`.

Risk:
- Too many top-level CTAs can create noise if they are not tied to the ranked journey order.

Source refs:
- `/dp/frankentui_website/app/page.tsx:94`
- `/dp/frankentui_website/app/page.tsx:96`
- `/dp/frankentui_website/app/page.tsx:107`
- `/dp/frankentui_website/app/page.tsx:117`
- `/dp/frankentui_website/app/page.tsx:127`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:600`

### P2. Sticky section shell for deep technical storytelling

Pattern:
- Use a stable long-form section shell with persistent context and a content rail, rather than a flat landing page blob.

Why it matters:
- FrankenMermaid’s value proposition is technical. The audience needs architecture, feature honesty, and proof artifacts without feeling like they were dumped into raw docs.

Applicability to FrankenMermaid:
- The current showcase already uses a sticky left-nav structure and sectioned experience. This should become the canonical shell for future docs/showcase evolution, not a temporary HTML one-off.

Recommended use:
- Keep the left rail anchored to the ranked journeys and proof artifacts.
- Reuse the shell across `overview`, `showcase`, `architecture`, and `evidence` views if the demo becomes a multi-page site.

Risk:
- A strong shell can still fail if the sections themselves are not ordered by decision value.

Source refs:
- `/dp/frankentui_website/components/section-shell.tsx:71`
- `/dp/frankentui_website/components/section-shell.tsx:90`
- `/dp/frankentui_website/components/section-shell.tsx:101`
- `/dp/frankentui_website/components/section-shell.tsx:150`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:600`

### P3. Progressive showcase ladder

Pattern:
- Move from low-friction inspection to higher-commitment interaction: look first, then try, then integrate.

Why it matters:
- This prevents the demo from depending entirely on successful runtime boot while still preserving a powerful live moment.

Applicability to FrankenMermaid:
- The current showcase already follows this pattern with spotlight, gallery, runtime status, and playground sections. The inventory confirms this is the right default structure.

Recommended use:
- Keep the sequence `featured proof -> scenario gallery -> live playground -> capability evidence -> integration path`.
- Treat static SVG examples as first-class, not as fallback embarrassment.

Risk:
- If the live playground appears before enough trust is built, runtime failures dominate user perception.

Source refs:
- `/dp/frankentui_website/app/showcase/page.tsx:127`
- `/dp/frankentui_website/app/showcase/page.tsx:144`
- `/dp/frankentui_website/app/showcase/page.tsx:160`
- `/dp/frankentui_website/app/showcase/page.tsx:163`
- `/dp/frankentui_website/app/showcase/page.tsx:192`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:722`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:808`

### P4. Sample registry with stable IDs and metadata

Pattern:
- Treat demo scenarios as structured inventory with stable identifiers, metadata, and coverage tags.

Why it matters:
- Stable IDs make filtering, URL deep-linking, source-span mapping, capability accounting, and E2E checks much easier.

Applicability to FrankenMermaid:
- The capability-to-scenario matrix already uses stable scenario IDs. The showcase should keep leaning into that instead of adding anonymous ad hoc samples.

Recommended use:
- Every featured demo, gallery item, and proof artifact should resolve to a scenario ID already present in `evidence/capability_scenario_matrix.json`.
- Keep scenario metadata aligned with journey IDs and maturity/confidence claims.

Risk:
- The registry becomes misleading if UI copy invents categories that the machine-readable matrix does not encode.

Source refs:
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:315`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:338`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:371`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:2455`
- `/data/projects/frankenmermaid/evidence/capability_scenario_matrix.json:1`

### P5. Stateful controls as product instrumentation

Pattern:
- Expose layout, rendering, diagnostics, and viewport toggles as intentional product surface instead of hidden debug state.

Why it matters:
- FrankenMermaid differentiates on inspectability and deterministic behavior. A good demo should make internal state legible without feeling like a dev-only sandbox.

Applicability to FrankenMermaid:
- The existing playground already compares multiple surfaces and runtime/bootstrap state. It should continue evolving toward deliberate instrumentation rather than generic input/output widgets.

Recommended use:
- Surface render target, accessibility summary, source-map presence, diagnostics count, and scenario maturity.
- Keep the “what changed and why” metadata visible near the output.

Risk:
- Too many knobs can turn the core story into a debugging interface for maintainers only.

Source refs:
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:1419`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:1511`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:808`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:1992`

### P6. In-demo status log and evidence trail

Pattern:
- Keep recent actions, warnings, and status transitions visible in stable order inside the demo.

Why it matters:
- This converts hidden runtime behavior into observable proof and helps reviewers understand whether the system degraded gracefully or silently failed.

Applicability to FrankenMermaid:
- The runtime bootstrap/status panel is already a strong start. It should evolve into a durable evidence rail that records fetch mode, runtime availability, render result, and artifact validation outcomes.

Recommended use:
- Preserve an append-only event log with stable labels such as `matrix-loaded`, `runtime-ready`, `render-warning`, `fallback-svg-used`, and `source-map-exported`.

Risk:
- A noisy log without prioritization can bury the key outcome.

Source refs:
- `/dp/frankentui/docs/spec/mermaid-showcase.md:125`
- `/dp/frankentui/docs/spec/mermaid-showcase.md:153`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:1380`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:4267`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:640`

### P7. Architecture as a visual pipeline, not prose dump

Pattern:
- Explain the technical system as a staged, visual render cycle.

Why it matters:
- FrankenMermaid’s core advantage is the shared pipeline: parse -> IR -> layout -> render -> multiple surfaces. That is easier to trust when shown as a pipeline with evidence anchors.

Applicability to FrankenMermaid:
- This should become a first-class section or page that directly mirrors the actual crate graph and current implementation boundaries.

Recommended use:
- Present `fm-parser -> fm-core::MermaidDiagramIr -> fm-layout -> fm-render-* -> fm-cli/fm-wasm`.
- Attach proof links to representative files, tests, or golden artifacts.

Risk:
- Oversimplification can hide current partial status by diagram family unless the architecture view links back to maturity evidence.

Source refs:
- `/dp/frankentui_website/app/architecture/page.tsx:31`
- `/dp/frankentui_website/app/architecture/page.tsx:161`
- `/dp/frankentui_website/app/architecture/page.tsx:206`
- `/dp/frankentui_website/app/architecture/page.tsx:228`
- `/data/projects/frankenmermaid/evidence/demo_strategy.md:1`

### P8. Try-before-install onboarding

Pattern:
- Offer a browser or static demo before asking for installation or code integration.

Why it matters:
- This lowers the evaluation cost for engineers who only want to answer “is this real?” before they spend time adopting it.

Applicability to FrankenMermaid:
- The standalone showcase already fills this role. Future onboarding should explicitly treat it as the front door, then hand off to CLI or embedding instructions.

Recommended use:
- Put a runnable sample next to a minimal CLI command and a WASM embedding snippet.
- Sequence this after trust-building evidence, not before.

Risk:
- If the browser path is the only polished surface, users may infer the CLI is secondary when the repo strength is actually shared-engine fidelity.

Source refs:
- `/dp/frankentui_website/app/getting-started/page.tsx:80`
- `/dp/frankentui_website/app/getting-started/page.tsx:85`
- `/dp/frankentui_website/app/getting-started/page.tsx:93`
- `/dp/frankentui_website/app/getting-started/page.tsx:96`
- `/dp/frankentui_website/app/getting-started/page.tsx:111`

### P9. Integrator-specific surface

Pattern:
- Maintain a dedicated path for embedders and app developers instead of making them reverse-engineer the main demo.

Why it matters:
- Browser/WASM adoption has different questions than “can the engine render a nice diagram?” Integrators care about boot, sizing, API shape, payloads, and constraints.

Applicability to FrankenMermaid:
- The current showcase already contains runtime/bootstrap and capability validation logic. That should be split into an explicit integrator track if the project grows beyond a single demo page.

Recommended use:
- Provide a focused embed page or panel with runtime contract, source-map output, accessibility payloads, and example initialization flow.

Risk:
- If this path is added before the core story is stable, the project can look more platform-complete than it actually is.

Source refs:
- `/dp/frankentui_website/app/web_react/page.tsx:213`
- `/dp/frankentui_website/app/web_react/page.tsx:220`
- `/dp/frankentui_website/app/web_react/page.tsx:299`
- `/dp/frankentui_website/app/web_react/page.tsx:325`
- `/dp/frankentui_website/app/web_react/page.tsx:337`
- `/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html:1992`

### P10. Coverage lab beside polished showcase

Pattern:
- Separate the “best first impression” showcase from the broader coverage and stress-validation lab.

Why it matters:
- A launch-quality narrative needs curation, but engineering confidence needs breadth and failure-mode coverage.

Applicability to FrankenMermaid:
- The existing capability matrix and gallery already hint at this separation. The next step is to keep a curated featured track while using the matrix-backed gallery or evidence pages as the broader coverage lab.

Recommended use:
- Preserve a small featured set for journeys J1-J5.
- Route broader family coverage, partial support, and stress cases into matrix-backed browsing and evidence reports.

Risk:
- If the split is unclear, users may mistake partial coverage inventory for polished end-user experience.

Source refs:
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_mega_showcase.rs:1`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_mega_showcase.rs:37`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_mega_showcase.rs:88`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_mega_showcase.rs:144`
- `/data/projects/frankenmermaid/evidence/capability_scenario_matrix.json:1`

### P11. Machine-readable demo manifest and validation contract

Pattern:
- Treat demo assets and claims as machine-readable artifacts with explicit assertions.

Why it matters:
- This turns a showcase from “marketing page” into a release-checkable proof surface.

Applicability to FrankenMermaid:
- The current capability matrix is already close to this pattern. The next increment is to formalize expected proof for each journey: scenario IDs, output surfaces, runtime requirements, and acceptance checks.

Recommended use:
- Add a dedicated artifact that maps journey IDs to scenario IDs, proof assets, and validation hooks.
- Keep it simple and deterministic enough for CI and local `file://` showcase validation.

Risk:
- Over-designing the schema too early can create process weight before the demo flows are stable.

Source refs:
- `/dp/frankentui/demo.yaml:1`
- `/dp/frankentui/demo.yaml:6`
- `/dp/frankentui/demo.yaml:148`
- `/dp/frankentui/demo.yaml:174`
- `/data/projects/frankenmermaid/evidence/capability_scenario_matrix.json:1`

### P12. Determinism-friendly test hooks and matrix validation

Pattern:
- Expose stable selectors, deterministic modes, and explicit cross-surface parity checks.

Why it matters:
- Demo credibility collapses if the artifact is visually impressive but impossible to test repeatably.

Applicability to FrankenMermaid:
- Stable scenario IDs, accessibility summaries, source-map element IDs, and capability matrix loading already create good hooks for deterministic checks.

Recommended use:
- Verify the same scenario across SVG, terminal, and browser-facing outputs where practical.
- Prefer checks keyed by scenario ID, output metadata, and evidence presence instead of brittle positional assumptions.

Risk:
- Snapshot-heavy validation can create maintenance drag if every cosmetic change becomes a broad golden churn.

Source refs:
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:2445`
- `/dp/frankentui/crates/ftui-demo-showcase/src/screens/mermaid_showcase.rs:2455`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/screen_snapshots.rs:3`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/screen_snapshots.rs:58`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/screen_snapshots.rs:121`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/terminal_web_parity.rs:47`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/terminal_web_parity.rs:120`
- `/dp/frankentui/crates/ftui-demo-showcase/tests/terminal_web_parity.rs:171`

### P13. Static-first deployment for expensive demo artifacts

Pattern:
- Ship the interactive experience as static assets when possible, then harden asset syncing, caching, and integrity checks.

Why it matters:
- FrankenMermaid’s strongest demo surfaces can be delivered without backend complexity if artifact syncing and cache semantics are reliable.

Applicability to FrankenMermaid:
- The current standalone HTML showcase already benefits from static-first deployment assumptions. This pattern supports evolving it into a hosted artifact bundle without turning the project into a web backend.

Recommended use:
- Keep prebuilt examples, capability JSON, and browser assets versioned and testable.
- Add integrity checks for hosted demo bundles before broad public rollout.

Risk:
- Static shipping only works if asset versioning is disciplined; otherwise stale runtime/data mismatches become the dominant failure mode.

Source refs:
- `/dp/frankentui_website/README.md:26`
- `/dp/frankentui_website/README.md:90`
- `/dp/frankentui_website/public/web/index.html:1`
- `/dp/frankentui_website/lib/wasm-loader.ts:91`
- `/dp/frankentui_website/lib/wasm-loader.ts:164`
- `/dp/frankentui_website/scripts/sync-showcase.sh:68`
- `/dp/frankentui_website/scripts/sync-showcase.sh:87`
- `/dp/frankentui_website/scripts/update-web-demo.sh:121`
- `/dp/frankentui_website/public/web/_headers:1`
- `/dp/frankentui_website/tests/web-demo.spec.ts:129`
- `/dp/frankentui_website/tests/web-demo.spec.ts:221`

## Recommended Near-Term Adoption Order

1. Preserve and sharpen the existing progressive showcase ladder around journeys J1-J5.
2. Make scenario IDs and matrix metadata the single source of truth for demo sample inventory.
3. Upgrade the runtime status area into a clearer evidence trail with stable event labels.
4. Add an explicit architecture/pipeline explainer tied to real crate boundaries and proof artifacts.
5. Add a machine-readable demo validation contract once the curated journey set stabilizes.

## Pattern-to-Bead Mapping

| Pattern | Adoption | Primary owning beads | Dependency / sequencing impact | Notes |
|---|---|---|---|---|
| P1 Intent-segmented landing flow | Mandatory | `bd-2u0.5.3.1`, `bd-2u0.5.7.1` | IA must define journey-first entry points before presenter mode can layer storytelling on top. | This should route users into J1-J5 instead of generic browsing. |
| P2 Sticky section shell | Mandatory | `bd-2u0.5.3.1`, `bd-2u0.5.3.2` | Shell contract precedes responsive implementation. | The current standalone HTML already proves the shape; formalize it before visual polish work. |
| P3 Progressive showcase ladder | Mandatory | `bd-2u0.5.3.1`, `bd-2u0.5.5.4`, `bd-2u0.5.7.1` | Section ordering should be fixed before gallery and guided tour details expand. | Keep `featured -> gallery -> live -> evidence -> integration`. |
| P4 Stable sample registry and metadata | Mandatory | `bd-2u0.5.2.2`, `bd-2u0.5.5.4`, `bd-2u0.5.6.2` | Canonical scenarios must exist before compare mode and determinism checks can be trustworthy. | Use scenario IDs from `evidence/capability_scenario_matrix.json` as the source of truth. |
| P5 Stateful controls as instrumentation | Mandatory | `bd-2u0.5.5.1`, `bd-2u0.5.5.2`, `bd-2u0.5.5.3`, `bd-2u0.5.6.1` | Feature labs should expose meaningful controls, then telemetry consolidates their runtime state. | Avoid generic knobs with no evidence value. |
| P6 In-demo status log and evidence trail | Mandatory | `bd-2u0.5.4.3`, `bd-2u0.5.6.1`, `bd-2u0.5.7.2` | Inline diagnostics and telemetry should converge on one stable event model before release gating is defined. | This is the clearest adaptation of FrankenTUI's status-log pattern. |
| P7 Visual pipeline architecture explainer | Mandatory | `bd-2u0.5.3.1`, `bd-2u0.5.7.1`, `bd-2u0.5.10.3` | IA decides placement; presenter mode and decision log carry the narrative and rationale. | Tie directly to `fm-parser -> fm-core -> fm-layout -> fm-render-* -> fm-cli/fm-wasm`. |
| P8 Try-before-install onboarding | Optional for first implementation pass, mandatory for public launch | `bd-2u0.5.8.2.1`, `bd-2u0.5.8.3.1`, `bd-2u0.5.7.1` | Static and React entrypoint contracts should preserve a low-friction runnable path before broader docs polish. | Relevant once `/web` and `/web_react` become public surfaces. |
| P9 Integrator-specific surface | Optional | `bd-2u0.5.8.1`, `bd-2u0.5.8.3.1`, `bd-2u0.5.5.3` | Shared core contracts come first; integrator-facing API presentation should not outrun actual adapter boundaries. | Best framed as a focused embed contract, not a separate marketing story yet. |
| P10 Coverage lab beside curated showcase | Mandatory | `bd-2u0.5.2.2`, `bd-2u0.5.5.4`, `bd-2u0.5.7.2` | Canonical scenarios and gallery compare mode must exist before the coverage/release matrix can be credible. | Separate “best-first narrative” from “full capability discoverability.” |
| P11 Machine-readable demo manifest | Optional now, likely mandatory before release | `bd-2u0.5.7.2`, `bd-2u0.5.7.3`, `bd-2u0.5.10.3` | Release gate checklist should define the schema only after core showcase modules stabilize. | Prevents premature schema churn. |
| P12 Determinism-friendly hooks and parity checks | Mandatory | `bd-2u0.5.6.2`, `bd-2u0.5.8.4`, `bd-2u0.5.7.2` | Stable selectors and evidence surfaces must be in place before cross-surface parity harnesses are reliable. | Builds directly on stable IDs/source maps/accessibility summaries already landed in code. |
| P13 Static-first deployment and asset hardening | Optional during local iteration, mandatory before hosted rollout | `bd-2u0.5.8.2`, `bd-2u0.5.7.3`, `bd-2u0.5.6.4` | Static host integration should happen before deployment smoke checks and bundle hardening. | Best fit for the current standalone HTML showcase trajectory. |

## Adoption Summary

Mandatory in the near-term demo implementation sequence:
- P1, P2, P3 for information architecture and narrative shape
- P4, P10 for curated-scenario inventory and coverage separation
- P5, P6 for inspectable runtime behavior and evidence visibility
- P7 for architecture explanation
- P12 for determinism and parity validation

Optional until the showcase surfaces mature:
- P8 try-before-install onboarding
- P9 integrator-specific surface
- P11 machine-readable manifest formalization
- P13 hosted static deployment hardening

Immediate unblock implications:
- `bd-2u0.5.3.1` should consume P1, P2, P3, and P7 directly.
- `bd-2u0.5.3.2` should inherit shell/layout constraints from P2 after the IA spec lands.
- `bd-2u0.5.2.2` should use P4 and P10 to avoid mixing flagship scenarios with coverage inventory.
- `bd-2u0.5.6.1` and `bd-2u0.5.4.3` should share one event/evidence model derived from P5 and P6.
- `bd-2u0.5.8.1` should treat P9 as a constraint, not a promise, while defining shared core contracts.

## Adoption and Divergence Decision Log

| Decision ID | Pattern(s) | Decision | Rationale | Execution beads |
|---|---|---|---|---|
| D1 | P1, P3 | Adopt now | FrankenMermaid needs a journey-first showcase flow immediately because the project’s strongest value is not raw parity breadth; it is resilience, shared-engine leverage, and inspectable proof. | `bd-2u0.5.3.1`, `bd-2u0.5.7.1` |
| D2 | P2 | Adopt now | The sticky shell pattern already matches the current standalone page and reduces future drift between `/web`, `/web_react`, and the evidence-driven narrative. | `bd-2u0.5.3.1`, `bd-2u0.5.3.2` |
| D3 | P4, P10 | Adopt now | Scenario IDs and curated-vs-coverage separation are necessary to keep the demo honest about partial support while still showing breadth. | `bd-2u0.5.2.2`, `bd-2u0.5.5.4`, `bd-2u0.5.6.2` |
| D4 | P5, P6 | Adopt now | FrankenMermaid’s differentiators are inspectability and explicit diagnostics, so status/evidence surfaces must be product features rather than hidden debug views. | `bd-2u0.5.4.3`, `bd-2u0.5.6.1`, `bd-2u0.5.7.2` |
| D5 | P7 | Adopt now | The architecture explanation is part of the product story because shared-pipeline design is a buyer-facing advantage here, not an internal implementation detail. | `bd-2u0.5.3.1`, `bd-2u0.5.10.3`, `bd-2u0.5.7.1` |
| D6 | P8, P13 | Adopt later | Try-before-install and hosted static deployment hardening matter for public launch, but they should not outrun the shell, scenario inventory, and validation model. | `bd-2u0.5.8.2.1`, `bd-2u0.5.7.3`, `bd-2u0.5.6.4` |
| D7 | P9 | Adopt later with constraints | An integrator-specific surface is valuable, but FrankenMermaid should not imply a mature embed platform before the shared showcase core contracts are stabilized. | `bd-2u0.5.8.1`, `bd-2u0.5.8.3.1`, `bd-2u0.5.5.3` |
| D8 | P11 | Delay schema formalization | A machine-readable manifest is desirable, but the team should first stabilize journey structure and proof surfaces to avoid premature schema churn. | `bd-2u0.5.7.2`, `bd-2u0.5.7.3`, `bd-2u0.5.10.3` |
| D9 | P3, P10 | Intentional divergence from FrankenTUI mega-showcase emphasis | FrankenTUI can lean harder into a broad “coverage lab” because it already has a richer showcase/testing stack. FrankenMermaid should lead with curated proof first and only then reveal breadth. | `bd-2u0.5.2.2`, `bd-2u0.5.5.4` |
| D10 | P8, P9 | Intentional divergence from website-style expansion | FrankenMermaid should not yet split into many polished marketing-style pages. The current priority is one evidence-heavy showcase surface that can later branch into `/web` and `/web_react`. | `bd-2u0.5.8.1`, `bd-2u0.5.8.2.1`, `bd-2u0.5.8.3.1` |

Decision principles:
- Adopt patterns that strengthen honesty, inspectability, and deterministic proof immediately.
- Delay patterns that imply product-surface maturity beyond current repo truth.
- Diverge on sequencing when FrankenMermaid’s strongest current assets differ from FrankenTUI or the website’s more polished publication posture.

## Validation Checklist

- `evidence/demo_strategy.md` cites this artifact in its evidence basis.
- `evidence/capability_scenario_matrix.json` includes this artifact in `sources`.
- Every recommended pattern maps back to at least one source reference and one FrankenMermaid applicability statement.
- No pattern claims full parity; each recommendation stays honest about current partial maturity.
- The top adoption order reinforces the ranked journeys in `evidence/demo_strategy.md`.
