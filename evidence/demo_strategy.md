# FrankenMermaid Demo Strategy and Journey Contract

Status: active planning artifact

Primary beads:
- `bd-2u0.5.1` Define demo strategy, narrative, and measurable success criteria
- `bd-2u0.5.1.1` Define audience personas and high-value demo journeys

Planned extension point:
- `bd-2u0.5.1.2` KPI rubric and release-quality acceptance scorecard

## Evidence Basis

This strategy is grounded in the current repo state rather than aspirational parity claims alone.

Primary sources:
- `README.md`
- `frankenmermaid_demo_showcase.html`
- `evidence/pattern_inventory.md`
- `evidence/capability_matrix.json`
- `evidence/capability_scenario_matrix.json`
- `evidence/demo_resilience_fixture_suite.json`
- `crates/fm-cli/src/main.rs`
- `crates/fm-parser/src/lib.rs`
- `crates/fm-layout/src/lib.rs`
- `crates/fm-render-svg/src/lib.rs`
- `crates/fm-render-term/src/lib.rs`
- `crates/fm-wasm/src/lib.rs`

Current product truth that the demo must respect:
- The strongest implemented story today is shared-pipeline rendering from one parse/layout flow into CLI, SVG, terminal, and WASM/browser surfaces.
- The most differentiated product claims are resilience to malformed input, deterministic output, cycle-aware layout handling, terminal rendering, and inspectable diagnostics.
- Capability breadth is large, but many diagram families are still `partial` in `evidence/capability_matrix.json`; the demo must highlight breadth without implying uniform depth.
- Recovery and stress examples should now be anchored in `evidence/demo_resilience_fixture_suite.json` rather than ad hoc fixture mentions.
- Upstream showcase mining in `evidence/pattern_inventory.md` should guide information architecture, sample registry design, and validation seams without creating fake parity claims.

## Demo Thesis

The demo should convince a skeptical technical buyer that FrankenMermaid is already valuable because it:
- salvages real-world Mermaid-like input instead of failing hard
- produces deterministic, inspectable output across multiple surfaces
- offers capabilities Mermaid users usually do not get together: terminal output, diagnostics, and a Rust/WASM integration path

The demo should not optimize for "look how many diagram types exist" first. It should optimize for "why would a serious engineer switch or adopt this now?"

## Primary Decision the Demo Must Enable

After one guided session, the viewer should be able to say one of the following with confidence:
- "This is the right rendering engine for documentation and CI pipelines."
- "This is viable as an embeddable browser/WASM diagram runtime."
- "This is materially more resilient and inspectable than the Mermaid stack I use today."

## Persona Matrix

| Persona | Core decision | Current pain | FrankenMermaid proof point | Must-see surface |
|---|---|---|---|---|
| Documentation engineer | Can I render docs-as-code reliably in CI and local tooling? | Mermaid failures on malformed or evolving diagrams break docs pipelines | Best-effort parse, diagnostics, deterministic output, CLI render/validate/detect | CLI + SVG + validation JSON |
| Platform / developer tools engineer | Can I standardize diagram generation across backends and environments? | Separate pipelines for browser, server, and terminal create drift | Shared IR and layout path feeding SVG, terminal, canvas, and WASM | CLI + WASM + capability matrix |
| Frontend product engineer | Can I embed this in a web app without shipping a brittle JS-only stack? | Browser integration often means opaque behavior and weak control | `fm-wasm` runtime, source-span payloads, canvas/SVG path, configurable init | Browser showcase + WASM API |
| Architecture / systems lead | Will it handle larger, messier dependency graphs better than stock Mermaid? | Cycles, dense graphs, and layout instability reduce trust | Cycle strategies, layout guardrails, algorithm families, structured observability | SVG showcase + layout diagnostics |
| Operator / incident responder | Can I inspect diagrams in terminals, logs, and constrained environments? | Mermaid offers no native terminal-native visibility | Terminal rendering, diff mode, compact preview path | Terminal renderer + diff |

## Persona Priority

Primary personas for launch:
- Documentation engineer
- Platform / developer tools engineer
- Frontend product engineer

Secondary personas for launch:
- Architecture / systems lead
- Operator / incident responder

Rationale:
- The first three map directly to the repo's strongest credible differentiators today.
- The latter two are compelling, but their ideal story depends more heavily on unfinished parity and deeper performance evidence.

## High-Value Demo Journeys

### J1. Messy Input to Useful Output

Target personas:
- Documentation engineer
- Platform / developer tools engineer

Narrative:
- Start with malformed or typo-ridden Mermaid-like input.
- Show `detect`, `validate`, and `parse` surfacing warnings instead of a dead-end failure.
- Finish with successful SVG or terminal output plus diagnostics.

Why this is high value:
- It demonstrates the "never waste user intent" thesis immediately.
- It is differentiated, believable, and already supported by current parser/diagnostic code paths.

Required proof:
- detection confidence
- warnings / structured diagnostics
- output artifact still produced

### J2. One Diagram, Many Surfaces

Target personas:
- Platform / developer tools engineer
- Frontend product engineer

Narrative:
- Use one representative diagram.
- Show the same input rendering through CLI SVG, terminal, and WASM/browser paths.
- Emphasize shared contracts rather than separate ad hoc renderers.

Why this is high value:
- This is the clearest "Rust-first shared engine" story in the repo today.
- It turns architecture into user-facing leverage.

Required proof:
- shared parse/layout flow
- browser render result
- terminal render result
- source spans / observability payload where useful

### J3. Deterministic and Reviewable for CI

Target personas:
- Documentation engineer
- Architecture / systems lead

Narrative:
- Render the same input repeatedly.
- Show stable output expectations, golden coverage, or capability evidence already in-repo.
- Frame this as "safe for snapshots, PR review, and release pipelines."

Why this is high value:
- Determinism is a headline promise in both README and architecture.
- CI trust is a practical buying trigger.

Required proof:
- repeated output stability
- golden/capability evidence references
- explicit artifact hashes or consistent outputs in validation evidence

### J4. Terminal-First Incident or Remote Workflow

Target personas:
- Operator / incident responder
- Documentation engineer

Narrative:
- Show render-to-terminal and diff in a constrained environment story.
- Position this as a capability Mermaid users generally do not have natively.

Why this is high value:
- It is novel and memorable.
- It expands the audience beyond browser-only diagram consumers.

Required proof:
- readable terminal render
- structural diff output
- graceful fallback to ASCII-compatible modes where needed

### J5. Embeddable Browser Runtime with Inspectable Metadata

Target personas:
- Frontend product engineer
- Platform / developer tools engineer

Narrative:
- Show the web showcase loading the runtime and rendering diagrams in-browser.
- Surface WASM payload details such as detected type, guard metadata, or source spans.

Why this is high value:
- This is the adoption bridge from repo technology to real product integration.
- It turns WASM from a checkbox into a practical API story.

Required proof:
- browser runtime boot
- render output
- metadata payload that proves the runtime is not opaque

## Ranked Journey Order

| Rank | Journey | Demo criticality | Decision impact | Why it ranks here |
|---|---|---:|---:|---|
| 1 | J1 Messy Input to Useful Output | 5 | 5 | Best expression of the project's resilience thesis |
| 2 | J2 One Diagram, Many Surfaces | 5 | 5 | Best expression of the shared-engine architecture |
| 3 | J3 Deterministic and Reviewable for CI | 4 | 5 | Converts architecture quality into operational trust |
| 4 | J5 Embeddable Browser Runtime with Inspectable Metadata | 4 | 4 | Critical for modern product integration story |
| 5 | J4 Terminal-First Incident or Remote Workflow | 3 | 4 | Highly memorable differentiator, but secondary for broad adoption |

## Showcase Information Architecture Contract

Primary bead:
- `bd-2u0.5.3.1`

Upstream design inputs:
- `evidence/pattern_inventory.md`
- `frankenmermaid_demo_showcase.html`

Design intent:
- The showcase must behave like a decision tool, not a feature dump.
- Section order must follow the ranked journey order closely enough that a first-time visitor gets the resilience story first, the shared-engine story second, and the proof/evidence story before deeper exploration.
- The current standalone HTML is directionally correct, but the sections should be interpreted as a formal contract for future `/web` and `/web_react` surfaces rather than as a one-off page.

### Top-Level Sections and Order

1. Runtime
   Purpose:
   Prove whether the browser-facing runtime is actually live, what source it loaded from, and whether the surface is honest about failures.

2. Spotlight
   Purpose:
   Show one large, realistic scenario that demonstrates why the engine is worth caring about before the visitor sees the full atlas.

3. Gallery
   Purpose:
   Provide broad scenario discoverability without displacing the curated narrative path.

4. Playground
   Purpose:
   Let the user inspect real API surfaces, compare outputs, and validate the shared-engine/browser story interactively.

5. Support Evidence
   Purpose:
   Surface capability coverage, support honesty, and future architecture/evidence contracts in a reviewable form.

Ordering rationale:
- Runtime first because J5 credibility and browser honesty are prerequisites for trusting the rest of the page.
- Spotlight second because the first large visual should sell J1/J2 immediately.
- Gallery third because breadth should follow, not precede, the flagship proof.
- Playground fourth because hands-on inspection matters more after the visitor has context.
- Support evidence last because it is proof and due diligence, not the emotional hook.

### Section-to-Journey Mapping

| Section | Primary journeys served | Secondary journeys served |
|---|---|---|
| Runtime | J5 | J3 |
| Spotlight | J1, J2 | J4 |
| Gallery | J2 | J3, J4 |
| Playground | J2, J5 | J1, J3 |
| Support Evidence | J3 | J1, J5 |

### Panel Responsibilities and Interaction Contracts

Runtime section:
- Left panel owns live bootstrap truth: source URL, mode, retry action, endpoint diagnostics, and failure transparency.
- Right panel owns the "why this matters" framing and the short differentiation summary.
- The runtime panel must never imply publication success when the runtime is unavailable; it must degrade to explicit evidence of absence.

Spotlight section:
- Main panel owns the featured scenario render at serious size, plus the "why this sample matters" explanation and source text.
- Side rail owns category switching and proof-callout cards.
- Clicking a feature-strip item updates the spotlight without losing scenario identity.
- Opening the spotlight in the editor must preserve scenario identity so later deep-link/state beads can make the handoff reproducible.

Gallery section:
- Filter row owns category chips and search.
- Grid owns broad capability discoverability and navigation back to the spotlight/editor.
- Gallery cards should default to visual output first and source second.
- Gallery state must stay subordinate to canonical scenario IDs from `evidence/capability_scenario_matrix.json`.

Playground section:
- Left column owns authoring and raw API inspection: source editor, `detectType()`, `sourceSpans()`, and `parse()`.
- Right column owns rendered outputs and matrix-backed evidence: FrankenMermaid SVG, Mermaid baseline, canvas surface, capability summary, and capability table.
- Render actions must be explicit and reproducible; any debounced or cancellable pipeline later added must preserve the current "what output corresponds to what input" clarity.
- The editor surface should expose authoring affordances directly in the page: syntax lens, structural hints, sample-cycling shortcuts, and URL-safe source overrides that survive deep-link restoration.
- The live authoring loop should be latest-edit-wins: stale async runs must be discarded, and visible stage timing/error state should explain what actually rendered.
- The section should function as the home for future telemetry, determinism, diagnostics, and export labs rather than scattering those surfaces across the page.

Support Evidence section:
- Support grid owns concise proof cards summarizing what the project can honestly claim now.
- Coverage summary/table owns matrix-backed support truth and should remain machine-readable in spirit even when visually styled.
- The architecture/pipeline explainer should live here or immediately adjacent to this section, because it is evidence and interpretation, not hero copy.

### Route and Surface Contract

Standalone static route:
- `frankenmermaid_demo_showcase.html` remains the initial contract reference for `file://` and static-hosted usage.

Future hosted/static route:
- `/web` should preserve the same section order and evidence obligations unless there is a deliberate divergence recorded in `bd-2u0.5.10.3`.

Future React route:
- `/web_react` may improve interaction mechanics, but it should preserve the same decision order, scenario identity model, and proof surfaces.

Shared cross-surface invariants:
- The same scenario ID should mean the same narrative example across all surfaces.
- Runtime truth, capability truth, and diagnostics truth must not vary semantically by host.
- Any surface-specific enhancement must be clearly additive, not a hidden semantic fork.

### Breakpoint and Layout Rules

Desktop:
- Maintain a sticky top navigation plus two-column section layouts where one side carries proof/context and the other side carries the primary artifact.
- Spotlight and playground should preserve a dominant visual/output viewport.

Tablet:
- Preserve section order and most panel pairings, but allow side rails to collapse beneath the primary artifact.
- Filters and action rows may wrap, but core CTA order must remain stable.

Mobile:
- Collapse each section to a single vertical stack.
- Keep navigation compact and scroll-oriented rather than trying to preserve split panes.
- Within each section, order content as: claim -> primary artifact -> controls -> supporting evidence -> raw source/details.

Non-negotiable responsive rules:
- No breakpoint may hide the runtime truth panel entirely.
- No breakpoint may place broad gallery browsing ahead of the flagship spotlight.
- Playground raw inspection panels may collapse, but render outputs and key evidence summaries must stay visible without requiring source expansion first.

### Architecture Explainer Placement Contract

- The shared-pipeline explainer must be visibly reachable from the main showcase flow.
- It should sit after the visitor has seen at least one large visual proof and at least one inspectable evidence surface.
- The explainer should show `parse -> IR -> layout -> render -> surfaces`, then connect those stages to concrete files and current capability claims.
- It must not read like generic architecture documentation; it should explain why the pipeline produces trust, inspectability, and multi-surface leverage.

### Immediate Downstream Implications

- `bd-2u0.5.3.2` should implement the shell against this section order and the responsive degradation rules above.
- `bd-2u0.5.4.1` should treat the playground as the canonical editor/instrumentation home, not create a parallel authoring surface elsewhere.
- `bd-2u0.5.5.4` should keep gallery breadth subordinate to spotlight-first narrative sequencing.
- `bd-2u0.5.6.1` and `bd-2u0.5.4.3` should extend the playground/support-evidence model instead of inventing detached diagnostics panels.
- `bd-2u0.5.8.1` should carry these contracts into shared core surface boundaries for `/web` and `/web_react`.

## Launch Scope Boundary

Must show at launch:
- honest support messaging using the capability matrix rather than overstated parity
- at least one malformed-input recovery story
- at least one multi-surface story spanning CLI and browser or CLI and terminal
- deterministic / reviewable evidence story
- at least one terminal-native moment

Should show if time allows:
- cycle-heavy graph story with guardrails or layout rationale
- a polished browser playground moment tied to real runtime metadata

Stretch only:
- broad "all diagram types are fully ready" messaging
- gantt, C4, or xyChart as headline journeys unless the implementation status materially improves

## Journey-to-Repo Mapping

| Journey | Core files / artifacts to lean on |
|---|---|
| J1 Messy Input to Useful Output | `crates/fm-parser/src/lib.rs`, `crates/fm-cli/src/main.rs` |
| J2 One Diagram, Many Surfaces | `crates/fm-cli/src/main.rs`, `crates/fm-render-svg/src/lib.rs`, `crates/fm-render-term/src/lib.rs`, `crates/fm-wasm/src/lib.rs` |
| J3 Deterministic and Reviewable for CI | `crates/fm-cli/tests/golden_svg_test.rs`, `crates/fm-cli/tests/golden/*`, `evidence/capability_matrix.json` |
| J4 Terminal-First Incident or Remote Workflow | `crates/fm-render-term/src/lib.rs`, CLI `diff` and render flows |
| J5 Embeddable Browser Runtime with Inspectable Metadata | `crates/fm-wasm/src/lib.rs`, `frankenmermaid_demo_showcase.html` |

## Capability-to-Scenario Matrix Contract

Primary bead:
- `bd-2u0.5.2.1`

Source-of-truth artifact:
- `evidence/capability_scenario_matrix.json`

Contract:
- Every demo-relevant feature family must map to one or more concrete scenario IDs.
- Scenario IDs must resolve to executable examples already present in the repo: showcase samples, golden fixtures, or equivalent deterministic artifacts.
- Each feature family must carry explicit `maturity` and `confidence` so the demo can stay honest about what is strong, partial, or still experimental.
- The standalone showcase should consume this artifact directly when possible, and degrade to a synced fallback only when local `file://` execution prevents fetch-based loading.

Current family coverage in the matrix:
- `parser`
- `layout`
- `rendering`
- `export`
- `diagnostics`
- `performance`
- `accessibility`

Current scenario anchors in the matrix:
- golden fixtures: `flowchart_simple`, `flowchart_cycle`, `sequence_basic`, `state_basic`, `gantt_basic`, `pie_basic`, `malformed_recovery`
- showcase featured/gallery scenarios: `flowchart-1-incident-response-escalation`, `sequence-1-checkout-risk-review`, `state-1-security-exception-lifecycle`, `gantt-1-q2-reliability-hardening`, `journey-1-new-customer-launch-journey`, `gitGraph-1-release-train-alpha`

Why this artifact matters:
- It stops the demo from drifting into vague claims by forcing each headline capability to point at a real, replayable scenario.
- It bridges product narrative and repo truth: ranked journeys stay legible to humans, while the matrix stays machine-readable for the showcase and future validation tooling.
- It gives future demo beads one stable place to extend instead of duplicating ad hoc sample lists across docs and UI code.

## Recovery and Stress Fixture Contract

Primary bead:
- `bd-2u0.5.2.3`

Source-of-truth artifact:
- `evidence/demo_resilience_fixture_suite.json`

Contract:
- Recovery and stress scenarios must be backed by checked-in `.mmd` inputs and `.svg` golden outputs.
- Each fixture entry must define expected warning volume, degradation tier, and minimum recovered graph size so the suite proves useful salvage rather than mere non-crashing behavior.
- The golden harness should enforce this manifest directly so resilience proof stays executable and deterministic.

Current resilience anchors:
- `malformed_recovery`
- `fuzzy_keyword_recovery`
- `dense_flowchart_stress`

## Presenter Guidance

Lead with:
- recovery
- shared pipeline
- deterministic evidence

Avoid leading with:
- raw diagram-count marketing
- unfinished parity areas as if they are fully closed
- algorithm jargon before the viewer understands the user problem

Preferred framing:
- "FrankenMermaid is the Mermaid engine for messy reality, CI trust, and multi-surface rendering."

## Planning-Only Verification Checklist

- [x] Personas are tied to concrete adoption decisions, not generic demographics.
- [x] Each journey maps to a repo capability that exists today.
- [x] Ranking reflects current implementation truth, not wishlist scope.
- [x] Launch-vs-stretch boundary is explicit.
- [x] The artifact provides a stable extension point for `bd-2u0.5.1.2`.
- [x] Evidence sources are listed so later beads can validate and extend this document without re-deriving assumptions.

## Follow-On Contract for `bd-2u0.5.1.2`

The KPI bead should extend this file rather than replace it. It should add:
- measurable pass/fail criteria per ranked journey
- ownership for each metric
- rehearsal and release thresholds
- structured evidence expectations for demo validation runs
- explicit references back to `evidence/capability_scenario_matrix.json` so KPI scoring always attaches to concrete scenario IDs

## KPI Rubric and Release-Quality Scorecard

Primary bead:
- `bd-2u0.5.1.2`

Scoring intent:
- convert the ranked journeys into explicit go / no-go criteria
- reward proof and clarity over breadth theater
- make it impossible to "pass" the demo while hiding unsupported or flaky areas

### Scorecard Dimensions

| Dimension | What it measures | Weight |
|---|---|---:|
| Narrative clarity | How quickly the audience understands why FrankenMermaid matters | 20 |
| Runtime credibility | Whether the showcased behaviors are grounded in current repo truth | 25 |
| Multi-surface proof | Whether the shared-engine story is demonstrated rather than asserted | 20 |
| Reliability evidence | Whether recovery, determinism, and diagnostics are visibly proven | 25 |
| Presentation discipline | Whether scope stays honest and avoids unsupported overreach | 10 |

Total possible score: 100

### Journey-Level Pass Criteria

| Journey | Minimum pass condition | Stretch condition | Owner role |
|---|---|---|---|
| J1 Messy Input to Useful Output | Demo shows malformed or typo-ridden input, emits warnings/diagnostics, and still produces output | Show both SVG and terminal salvage paths from the same bad input | Parser + CLI owner |
| J2 One Diagram, Many Surfaces | One diagram is rendered on at least 3 surfaces with the same underlying story | All 4 surfaces are shown: CLI SVG, terminal, browser SVG, browser canvas | Runtime integration owner |
| J3 Deterministic and Reviewable for CI | One repeated-run scenario proves stable output or stable artifact hashes across 5 runs | Goldens, capability evidence, and validation output are linked live in the demo | QA / evidence owner |
| J4 Terminal-First Incident or Remote Workflow | Terminal render or diff is legible and materially useful in a non-browser framing | Includes ASCII or degraded-mode fallback explanation without losing usefulness | Terminal UX owner |
| J5 Embeddable Browser Runtime with Inspectable Metadata | Browser showcase renders and exposes at least detected type plus one metadata payload | Browser story also shows source spans or guard metadata in a useful UI panel | WASM / web owner |

### Release Gate Metrics

| ID | Metric | Threshold | Evidence source | Owner role | Related journeys |
|---|---|---|---|---|---|
| KPI-1 | Primary narrative can be delivered end-to-end without dead links, broken panels, or manual code edits | Pass all top 3 journeys in one rehearsal run | demo rehearsal notes + showcase artifact | Demo owner | J1, J2, J3 |
| KPI-2 | Recovery story proves "useful output, not hard failure" | At least 1 malformed-input scenario ends with warnings plus rendered output | CLI `detect` / `validate` / `render` outputs | Parser + CLI owner | J1 |
| KPI-3 | Shared-engine story is visible across surfaces | Same source diagram shown successfully on at least 3 surfaces | CLI output, terminal render, browser runtime | Runtime integration owner | J2, J5 |
| KPI-4 | Determinism claim is demonstrated, not implied | Representative scenario repeated 5 times with stable output or hash | golden tests, artifact hashes, repeat-run log | QA / evidence owner | J3 |
| KPI-5 | Capability honesty is preserved | No showcased headline journey depends on a diagram family currently marked unsupported; any `partial` family is labeled as partial | `evidence/capability_matrix.json` + presenter notes | Docs / demo owner | J1-J5 |
| KPI-6 | Diagnostics are intelligible to a skeptical engineer | At least one diagnostics view includes source context plus actionable wording | validation output or browser metadata panel | CLI / WASM owner | J1, J5 |
| KPI-7 | Terminal value is memorable, not token | Terminal segment must either show a meaningful render or a useful structural diff | terminal output capture | Terminal UX owner | J4 |
| KPI-8 | Demo stays within disciplined launch scope | Live flow spends >= 70% of time on primary personas and top 3 journeys | presenter runbook timing | Demo owner | J1, J2, J3 |
| KPI-9 | Evidence trail is reviewable after the demo | All supporting artifacts are linked from one checklist or runbook | `evidence/demo_strategy.md` and follow-on runbook | QA / evidence owner | J1-J5 |

### Weighted Rubric

| Score band | Meaning | Release posture |
|---|---|---|
| 90-100 | Strong launch-ready demo with credible proof and disciplined scope | Ship as primary showcase |
| 75-89 | Good demo with minor evidence or polish gaps | Ship after one focused cleanup pass |
| 60-74 | Story is promising but still brittle or too aspirational | Do not position as flagship demo |
| <60 | Demo is misleading, fragile, or unsupported by evidence | Block release use |

### Hard Blockers

Any one of these is a release blocker regardless of weighted score:
- J1 is missing or ends in hard failure without a recovery story
- J2 cannot demonstrate the shared-engine story on at least 3 surfaces
- J3 lacks any repeatable determinism evidence
- the demo markets unsupported or partial areas as complete without explicit qualification
- the browser showcase fails to render during rehearsal

### Rehearsal Checklist

- [ ] Run top 3 journeys in ranked order without improvising new scope.
- [ ] Confirm every live claim is backed by either current runtime behavior or a linked evidence artifact.
- [ ] Confirm the malformed-input demo still recovers on the current branch.
- [ ] Confirm browser showcase still loads and renders.
- [ ] Confirm terminal segment is readable in a typical narrow viewport.
- [ ] Confirm all referenced artifacts exist at stable repo paths.

### Metric Ownership Notes

Owner roles are intentionally role-based rather than person-based so parallel agents can pick them up without rewriting the scorecard.

- Demo owner: narrative sequence, timing discipline, honesty of scope
- Parser + CLI owner: recovery path, detect/validate/render evidence
- Runtime integration owner: cross-surface consistency
- QA / evidence owner: determinism proof and artifact trail
- WASM / web owner: browser runtime and metadata panel
- Terminal UX owner: terminal legibility and diff usefulness

### Recommended Initial Passing Target

For the first serious launch-quality demo, the minimum acceptable bar should be:
- weighted score >= 75
- all hard blockers cleared
- J1, J2, and J3 fully passing
- J4 or J5 passing strongly enough to serve as the memorable differentiator segment

## Planning-Only Verification Checklist for `bd-2u0.5.1.2`

- [x] Every primary journey now has an explicit pass condition.
- [x] Release gates are tied to stable repo artifacts or runtime outputs.
- [x] Hard blockers prevent misleading demo readiness calls.
- [x] Ownership is role-based so other agents can adopt the work without identity churn.
- [x] The rubric favors current truth and evidence quality over breadth marketing.
