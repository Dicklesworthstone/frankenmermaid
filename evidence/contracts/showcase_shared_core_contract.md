# Showcase Shared Core Contract

## Purpose

This contract defines the shared core that both future showcase hosts must consume:

- static host: `/web`
- React host: `/web_react`

The current canonical behavior reference is the standalone [frankenmermaid_demo_showcase.html](/data/projects/frankenmermaid/frankenmermaid_demo_showcase.html). Future hosts may change presentation mechanics, but they must not fork the meaning of state, diagnostics, capability truth, or fallback behavior.

## Source Of Truth Inputs

Both hosts must consume the same logical inputs:

1. Scenario corpus and canonical IDs from `frankenmermaid_demo_showcase.html` and `evidence/capability_scenario_matrix.json`.
2. Section order, panel responsibilities, route invariants, and playground obligations from `evidence/demo_strategy.md`.
3. Browser runtime contract exposed by `pkg/frankenmermaid.js` / `pkg/frankenmermaid_bg.wasm`.
4. Mermaid baseline renderer as a comparison surface, not as a substitute source of truth.

## Shared State Snapshot

The cross-host state model is the URL-restorable showcase snapshot:

```text
sample: canonical scenario id for the editor/playground source
spotlight: canonical scenario id for the hero artifact
galleryFilter: category filter or "all"
galleryQuery: free-text gallery query
compare: ordered list of up to two canonical scenario ids staged for gallery compare mode
layoutLabFocus: active inspection lens for the layout/cycle lab
studio: runtime-backed style studio state
  preset
  fontSize
  padding
  radius
  shadows
  embedCss
source: optional source override when the current editor text differs from the canonical sample
shells: per-section shell state
  showcase.balance / showcase.rail
  playground.balance / playground.rail
  support.balance / support.rail
```

Rules:

- The same state snapshot must resolve to the same scenario meaning on every host.
- `compare`, `layoutLabFocus`, and `studio` are portable shared state, not host-local decoration.
- `source` is an override layer, never a new canonical scenario identity.
- Hosts may add ephemeral UI state locally, but they must not serialize non-portable host-only state into the shared URL contract.

## Shared Core Modules

The shared core must be decomposed into these logical modules, even if the static host initially implements them inline:

### 1. Scenario Catalog

Responsibilities:

- own canonical scenario IDs
- resolve spotlight/editor/gallery samples
- provide category metadata and narrative annotations
- provide compare-mode eligibility and canonical compare ordering
- guarantee that later hosts reuse the same sample identity map

Inputs:

- checked-in sample definitions
- capability matrix scenario metadata

Outputs:

- `findSampleById(id)`
- featured sample list
- gallery sample list
- category index

### 2. URL State Codec

Responsibilities:

- collect state from current showcase state
- build and restore URL snapshots
- enforce canonical fallback behavior when invalid IDs or malformed shell state appear
- validate compare-mode selection cardinality and de-duplicate compare scenario ids

Required guarantees:

- latest valid URL state always wins on restore
- malformed URL fields degrade to canonical defaults, never to undefined behavior

### 3. Shell State Controller

Responsibilities:

- normalize split-shell balance and rail state
- expose per-section shell state transitions
- keep button state and section dataset state semantically aligned
- leave compare-mode, lab-focus, and style-studio state to the URL codec / shared state layer rather than embedding it in host-only shell state

The React host may implement this as hooks/components; the static host may keep DOM dataset wiring. Semantics must remain identical.

### 4. Render Pipeline Controller

Responsibilities:

- schedule debounced playground runs
- enforce latest-edit-wins semantics
- suppress stale async commits
- collect per-stage timing
- commit render snapshots atomically

Required stages:

- Mermaid baseline render
- FrankenMermaid SVG render
- detectType
- parse
- sourceSpans
- capabilityMatrix
- canvas render

### 5. Diagnostics Projector

Responsibilities:

- turn runtime, detection, parse, warning, and render failures into one structured diagnostics view
- map diagnostics to line-linked markers
- surface remediation hints and confidence context
- preserve cross-host severity semantics

Severity contract:

- `error`: output degraded or trust materially compromised
- `warning`: recovered or partial quality risk
- `info`: notable but non-blocking context
- `hint`: authoring suggestion or low-confidence guidance

### 6. Safe Fallback Preview Controller

Responsibilities:

- preserve the last healthy preview snapshot
- distinguish healthy, degraded, and unavailable states
- explain why fallback is active instead of silently hiding failure

Required guarantee:

- degraded current state must not destroy the user’s last trustworthy preview without explanation

### 7. Capability Artifact Loader

Responsibilities:

- load `evidence/capability_scenario_matrix.json` when host fetching is available
- degrade to the inline fallback only when local file execution prevents artifact loading
- preserve semantic parity between fetched and fallback artifacts

### 8. Host Services Adapter

This is the only host-specific boundary.

Services:

- DOM binding / component rendering
- history API interaction
- clipboard access
- scroll/focus orchestration
- runtime module loading
- intersection observation / animation hooks

Everything above this boundary is shared-core logic. Everything below it is host integration.

## Adapter Boundaries

### Static Host Adapter (`/web`)

Owns:

- direct DOM element lookup
- event listener registration
- dataset mutation for shell state
- HTML string commits for SVG/diagnostics/fallback panels

Must not own:

- canonical sample resolution rules
- URL state semantics
- diagnostics severity rules
- fallback policy

### React Host Adapter (`/web_react`)

Owns:

- component tree composition
- hook-based state propagation
- memoized derived view models if needed
- React-friendly lifecycle around runtime loading and async pipeline state

Must not own:

- alternate scenario ID mapping
- alternate degradation semantics
- alternate timing/diagnostic meaning
- alternate fallback criteria

## Non-Negotiable Cross-Host Invariants

1. Scenario identity:
   The same scenario ID always means the same sample, narrative, and capability anchor.

2. Runtime honesty:
   Runtime unavailable means runtime unavailable on every host. No host may claim “live” success while another reports artifact absence for the same artifact state.

3. Diagnostics truth:
   Severity, remediation, and fallback activation rules must not fork by host.

4. Latest-edit-wins:
   Stale async runs are never allowed to overwrite newer source state.

5. Fallback preservation:
   Degraded states keep the last healthy preview with explicit explanation.

6. Capability truth:
   Both hosts must consume the same capability artifact semantics and scenario mapping.

## Shared-Core Readiness Checklist

- Canonical scenario catalog extracted from standalone page logic
- Shared URL codec defined and independently testable
- Portable compare-mode, layout-lab, and style-studio state captured in the shared snapshot
- Shared render pipeline controller defined with snapshot commit semantics
- Shared diagnostics projector defined with stable severity schema
- Shared fallback controller defined with last-healthy preservation policy
- Host-services interface documented and narrow enough that static and React adapters can implement it independently

## Downstream Consumers

This contract is the direct prerequisite for:

- `bd-2u0.5.8.2`
- `bd-2u0.5.8.2.1`
- `bd-2u0.5.8.3`
- `bd-2u0.5.8.3.1`
- `bd-2u0.5.11.2`
- `bd-2u0.5.9.1`

Those beads should extend this contract, not redefine it.
