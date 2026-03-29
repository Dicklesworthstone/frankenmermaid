# Showcase Test Taxonomy And Logging Schema

## Purpose

This contract defines the shared test taxonomy, structured logging schema, and artifact traceability rules for showcase-related work across:

- standalone showcase artifact
- future `/web` static host
- future `/web_react` host
- supporting CLI / WASM / renderer evidence paths

It exists so every downstream demo bead produces evidence in the same shape instead of inventing one-off logs or ad hoc test language.

## Test Taxonomy

Every demo-facing bead must classify coverage using these buckets.

### 1. Unit

Use when:

- validating pure state transforms
- validating URL codec behavior
- validating diagnostics projection
- validating fallback policy selection
- validating logging field population

Required expectations:

- edge cases included
- malformed input cases included when relevant
- deterministic outputs for identical input/config

### 2. Integration

Use when:

- multiple showcase modules interact
- runtime metadata feeds diagnostics or fallback UI
- static adapter and shared-core boundaries are exercised together
- artifact loader behavior changes

Required expectations:

- cross-module contracts explicitly asserted
- latest-edit-wins semantics asserted for async pipeline work
- capability artifact and scenario identity semantics preserved

### 3. End-To-End

Use when:

- a user-visible journey is exercised from scenario selection through render/evidence output
- runtime availability/unavailability behavior is part of the claim
- diagnostics or fallback flows are part of the user story

Required expectations:

- named scenario IDs
- explicit pass/fail reason
- artifact paths recorded
- evidence links stable enough for later review

### 4. Determinism Replay

Use when:

- stable output or stable artifact hash is a product claim
- layout, render, fallback, or telemetry behavior could regress nondeterministically

Required expectations:

- at least 5 repeat runs for representative scenarios when determinism is being claimed
- stable hash or stable normalized log summary
- explicit note when a surface is expected to vary

### 5. Evidence Completeness

Use for planning/research/documentation beads that do not change runtime code.

Required expectations:

- referenced artifact exists
- schema sections are present and non-empty
- downstream bead linkage is recorded
- traceability back to scenario IDs, KPIs, or route contracts is explicit

## Required Coverage Matrix By Bead Category

| Bead category | Unit | Integration | E2E | Determinism replay | Evidence completeness |
|---|---|---|---|---|---|
| Planning / research / contract | optional | optional | optional | optional | mandatory |
| Static showcase UI behavior | recommended | mandatory | mandatory | recommended | mandatory |
| Shared-core behavior | mandatory | mandatory | recommended | mandatory when timing/state claims exist | mandatory |
| Runtime / WASM integration | mandatory | mandatory | mandatory | mandatory | mandatory |
| Diagnostics / fallback / telemetry | mandatory | mandatory | mandatory | recommended | mandatory |
| Export / artifact lab | mandatory | mandatory | mandatory | mandatory when hashes are exposed | mandatory |

## Structured Log Schema

Every reproducible demo validation run must emit a machine-readable record with these required fields:

```json
{
  "schema_version": 1,
  "bead_id": "bd-...",
  "scenario_id": "flowchart-1-incident-response-escalation",
  "input_hash": "sha256:...",
  "surface": "standalone|web|web_react|cli|wasm|terminal",
  "renderer": "franken-svg|mermaid-baseline|canvas|term|cli",
  "theme": "corporate",
  "config_hash": "sha256:...",
  "parse_ms": 0,
  "layout_ms": 0,
  "render_ms": 0,
  "diagnostic_count": 0,
  "degradation_tier": "healthy|partial|fallback|unavailable",
  "output_artifact_hash": "sha256:...",
  "pass_fail_reason": "human-readable summary"
}
```

### Additional Required Context Fields

These fields are also required for showcase work even when the original bead text does not list them explicitly:

- `run_kind`
  Values: `unit`, `integration`, `e2e`, `determinism`, `evidence`
- `trace_id`
  Stable run-level traceability handle
- `revision`
  Git SHA, working tree label, or equivalent run identifier
- `host_kind`
  Values: `standalone`, `static-web`, `react-web`, `cli`, `test-harness`
- `fallback_active`
  Boolean
- `runtime_mode`
  Values: `live`, `artifact-missing`, `fallback-only`, `mock-forbidden`

Replay/profile-oriented suites may add host-specific fields such as `profile`, `run_index`, `window_size`, or other execution metadata, but these must be additive. They must not replace or redefine the required core fields above.

## Degradation Tier Semantics

- `healthy`: all claimed surfaces for the run behaved as intended
- `partial`: output exists, but warnings or known quality loss are active
- `fallback`: user-facing fallback path is active but still reviewable
- `unavailable`: claimed surface or runtime could not execute

No downstream bead may invent alternate degradation labels without updating this contract.

## Artifact Naming Convention

All stored evidence artifacts should use this pattern:

```text
evidence/runs/<surface>/<bead_id>/<scenario_id>/<timestamp>__<run_kind>__<artifact_kind>.<ext>
```

Examples:

- `evidence/runs/standalone/bd-2u0.5.4.2/flowchart-1-incident-response-escalation/2026-03-27T18-00-00Z__e2e__log.json`
- `evidence/runs/cli/bd-2u0.5.1.2/malformed_recovery/2026-03-27T18-05-00Z__determinism__stdout.txt`

Artifact kinds:

- `log`
- `summary`
- `svg`
- `png`
- `html`
- `json`
- `stdout`
- `screenshot`

## Retention Rules

- Keep the latest passing artifact set for each `scenario_id + surface + bead_id`.
- Keep the latest failing artifact set when it explains an active blocker or regression.
- Planning-only beads may store checklist or schema artifacts without screenshots/binaries.
- Large temporary artifacts may be omitted if their summary log preserves hashes and stable reproduction instructions.

## Traceability Rules

Every artifact set must be traceable to:

1. a bead ID
2. a scenario ID when applicable
3. a host/surface
4. a renderer
5. a reproducible configuration hash or explicit default config statement

Artifacts that cannot be traced back to those fields do not count as reviewable evidence.

## Downstream Adoption Checklist

- The reusable validation entrypoint now lives at `scripts/showcase_harness.py`; downstream demo beads should extend or call it instead of inventing bead-local schema/host validators.
- `bd-2u0.5.11.2` should implement helpers against this schema rather than inventing a parallel one.
- `bd-2u0.5.5.*` showcase feature modules should log with this schema for UI-side evidence runs.
- `bd-2u0.5.8.*` static/React host beads should keep host-specific fields additive only.
- `bd-2u0.5.1.2` KPI scoring should point at artifacts emitted under this naming convention.
