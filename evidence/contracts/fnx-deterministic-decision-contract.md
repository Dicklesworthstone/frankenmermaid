# Decision Contract: FNX Deterministic Decision Contract

## Scope

This contract defines when future `fnx` analysis is allowed to influence frankenmermaid behavior, how ties are broken deterministically, and what diagnostics must say when `fnx` advice is used, skipped, unavailable, or degraded.

The contract is intentionally Phase 1 only:

- `fnx` may provide **advisory structural intelligence**
- existing frankenmermaid parser/layout heuristics remain **authoritative**
- no directed-only `fnx` result may silently override current deterministic engine behavior

This contract exists to unblock:

- `bd-ml2r.2` `[FNX] Implement Mermaid IR -> fnx graph adapter layer`
- `bd-ml2r.3` `[FNX] Add parser and CLI structural diagnostics via fnx`
- `bd-ml2r.9.1` `[FNX] UX subtask: config + CLI controls for fnx modes`
- `bd-ml2r.10.3` `[FNX] Runtime subtask: fallback ladder and strict-mode behavior`

## Dependency Model

- Canonical dependency is a pinned Git checkout of `franken_networkx` with explicit commit hashes in `Cargo.toml` workspace dependencies.
- FNX crates are optional and only compiled when `fnx-integration` is enabled.
- When FNX is unavailable (offline or missing git access), keep FNX features disabled; the default build must succeed without FNX.
- Local co-development uses a developer-only Cargo `[patch]` override (e.g., `.cargo/config.toml`) and must not be committed.

## Feature-Flag Topology

- Source-of-truth feature flags live in `crates/fm-layout/Cargo.toml`.
- `crates/fm-cli/Cargo.toml` and `crates/fm-wasm/Cargo.toml` re-export those flags for surface-level toggles.
- `fnx-experimental-directed` depends on `fnx-integration`; both default to off.
- CI runs FNX on/off paths (`.github/workflows/ci.yml`) to keep the optional dependency honest.

## Feature-Flag Boundary

### Default build

- `fnx-integration` disabled
- no `fnx` code path required for parse/layout/render success
- CLI/WASM behavior must remain byte-stable relative to the non-FNX baseline

### `fnx-integration`

- enables **undirected/advisory** structural analysis only
- may emit witness-like metadata, diagnostics, and supplemental scoring
- must not silently replace parser detection, layout dispatch, or guardrail fallback decisions

### `fnx-experimental-directed`

- extends `fnx-integration`
- reserved for explicit experiments only
- any directed-only result remains non-default and must identify itself as experimental in diagnostics

## Decision Table

| Situation | FNX role | Engine authority | Required behavior |
| --- | --- | --- | --- |
| Feature disabled | unavailable | native engine | skip FNX entirely; emit no false FNX claims |
| Feature enabled, FNX succeeds | advisory | native engine | attach FNX advice, preserve native decision unless an explicit future contract says otherwise |
| Feature enabled, FNX times out/errors | degraded | native engine | continue with native decision and record deterministic fallback reason |
| Feature enabled, FNX output conflicts with native heuristic | advisory conflict | native engine | prefer native result, record conflict note, keep ordering stable |
| Strict future mode asks for FNX and FNX unavailable | gate failure | caller policy | return a deterministic error or warning per explicit mode, never guess |

## Deterministic Precedence Rules

1. Parse validity and existing support contracts are always decided by frankenmermaid first.
2. Layout algorithm dispatch remains governed by current `fm-layout` logic unless a later contract explicitly promotes a specific FNX signal to authoritative.
3. FNX scores may refine diagnostics, ranking, or explanation text, but not mutate the selected algorithm in Phase 1.
4. If multiple FNX-derived candidates tie, stable sorting must use:
   - canonical diagram order
   - stable node/edge identifiers
   - lexical comparison of algorithm names or rule ids
5. If native heuristics tie, FNX may be consulted only as a deterministic secondary explainer, not as a hidden tiebreaker.
6. Any fallback path must emit the same outcome for the same input, config, and pressure tier.

## Advisory vs Authoritative Contract

### Advisory signals allowed in Phase 1

- graph density summaries
- component counts
- undirected connectivity observations
- witness hashes or structural evidence identifiers
- “why this was suggested” explanations for diagnostics

### Authoritative decisions reserved to frankenmermaid in Phase 1

- detected diagram type
- parse success/failure semantics
- selected layout algorithm
- guardrail fallback selection
- render degradation behavior
- CLI exit status

If a future bead wants an FNX result to become authoritative, it must update this contract first and specify:

- exact input surface
- exact precedence over native logic
- deterministic tie-breakers
- failure semantics when FNX is unavailable

## Fallback Ladder

When `fnx-integration` is enabled, the fallback ladder is:

1. Attempt FNX analysis with deterministic inputs derived from the current IR snapshot.
2. If FNX is unavailable, disabled, times out, or errors:
   - keep the native parser/layout decision unchanged
   - mark FNX state as `skipped`, `unavailable`, or `degraded`
   - include a machine-readable reason string
3. If FNX returns partial output:
   - keep only fields that pass validation
   - discard invalid/unstable fields
   - record partial-degradation notes
4. Never synthesize a fake FNX result to fill gaps.

## Diagnostics Contract

Future CLI/WASM evidence payloads must surface FNX participation explicitly using stable fields equivalent to:

```json
{
  "fnx_mode": "off|advisory|experimental_directed|strict",
  "fnx_status": "unused|used|skipped|unavailable|degraded|error",
  "projection_mode": "native_only|native_plus_fnx_advisory",
  "decision_mode": "native_authoritative|fnx_authoritative_experimental",
  "fallback_reason": "feature_disabled|timeout|analysis_error|invalid_projection|native_precedence"
}
```

Minimum semantic requirements:

- `used`: FNX ran and contributed advisory metadata
- `skipped`: FNX intentionally not consulted
- `unavailable`: feature requested but runtime/dependency absent
- `degraded`: FNX path attempted but only partial/failed output was usable
- `error`: reserved for explicit strict-mode failures

Diagnostics must never imply that FNX changed a decision when native logic remained authoritative.

## Executable Verification Obligations

The contract is only satisfied when executable checks verify:

- seeded ledger/report generation includes this contract
- deterministic report output remains stable
- the contract remains referenced by a checked-in ledger entry
- any future FNX diagnostic payload tests use the field names and state lattice defined above

## Adoption Criteria

- [ ] checked-in contract and ledger entry exist
- [ ] evidence tooling can seed the FNX contract entry deterministically
- [ ] ledger reporting includes this contract without manual post-processing
- [ ] downstream FNX implementation beads reference this contract instead of inventing new precedence rules
- [ ] future diagnostic tests use the same `fnx_mode` / `fnx_status` / `decision_mode` semantics

## Rejection Criteria

- [ ] FNX path can silently change parser or layout decisions without a contract update
- [ ] tie-break behavior depends on map iteration order, hash order, or runtime timing
- [ ] diagnostics cannot distinguish skipped vs unavailable vs degraded FNX states
- [ ] strict-mode behavior is unspecified or inconsistent across CLI/WASM

## Notes

- This contract intentionally matches the current repository state: feature flags and CI support exist, but runtime FNX behavior is not yet implemented.
- The contract therefore constrains future work before adapter/runtime code lands.
