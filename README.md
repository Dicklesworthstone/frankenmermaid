<div align="center">

<img src="frankenmermaid_illustration.webp" alt="frankenmermaid" width="320" />

# frankenmermaid

**A Rust-first, Mermaid-compatible diagram engine with intent-aware parsing, 15 layout algorithms, and SVG / terminal / Canvas2D / WASM rendering from a single intermediate representation.**

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Rust 2024](https://img.shields.io/badge/rust-2024_edition-orange.svg)](rust-toolchain.toml)
[![No unsafe](https://img.shields.io/badge/unsafe-forbid-success.svg)](#zero-unsafe-code)
[![Live Demo](https://img.shields.io/badge/demo-live-brightgreen.svg)](https://dicklesworthstone.github.io/frankenmermaid/)

**Live Demo:** <https://dicklesworthstone.github.io/frankenmermaid/>
*80+ interactive examples, live editor, presenter mode, style studio, diagnostics panel, and determinism checker.*

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh" | bash
```

</div>

---

<!-- BEGIN GENERATED: runtime-capability-metadata -->
| Surface | Status | Evidence |
|---------|--------|----------|
| CLI detect command | Implemented | 2 evidence refs |
| CLI parse command with IR JSON evidence | Implemented | 1 evidence refs |
| CLI SVG rendering | Implemented | 1 evidence refs |
| CLI terminal rendering | Implemented | 1 evidence refs |
| CLI validate command with structured diagnostics | Implemented | 1 evidence refs |
| CLI capability matrix command | Implemented | 2 evidence refs |
| WASM API renders SVG | Implemented | 1 evidence refs |
| WASM API exposes capability matrix metadata | Implemented | 1 evidence refs |
| Canvas rendering backend | Implemented | 1 evidence refs |
<!-- END GENERATED: runtime-capability-metadata -->

## TL;DR

**The Problem.** Mermaid syntax is wonderful for diagrams-as-code, but real-world inputs hit walls fast: cycles produce tangled hierarchical layouts, malformed syntax crashes the parser, large graphs grind through quadratic crossing-minimization, styling control is shallow, and there is no terminal output path at all. JavaScript-based renderers can't easily run in CI, embed in CLIs, or guarantee bit-identical output across runs.

**The Solution.** `frankenmermaid` is a ground-up Rust implementation built around one shared intermediate representation that feeds 15 layout algorithms and four render backends (SVG, terminal, Canvas2D, and WASM). It recovers from bad input instead of crashing, picks cycle-aware layout strategies automatically, optionally consults a graph-intelligence engine (`FNX`) for centrality and topology hints, runs an incremental layout pipeline that skips stages whose inputs have not changed, and produces deterministic output suitable for CI snapshot testing.

### Why use frankenmermaid?

| Capability | What it does |
|---|---|
| **24 diagram types** | Flowchart, sequence, class, state, ER, gantt, pie, gitGraph, journey, mindmap, timeline, sankey, quadrant, xyChart, block-beta, packet-beta, architecture-beta, 5 C4 variants, requirement, kanban |
| **Intent-aware parsing** | Best-effort recovery with structured diagnostics. Fuzzy keyword matching catches typos like `flowchar` or `seqeunceDiagram`; dangling edges auto-create placeholder nodes; never panics on malformed input |
| **15 layout algorithms** | Sugiyama, force-directed, tree, radial, sequence, timeline, gantt, xychart, sankey, kanban, grid, pie, quadrant, gitgraph, packet — auto-selected per diagram type |
| **4 cycle strategies** | Greedy, DFS back-edge, MFAS approximation, full cycle-aware with SCC detection and cluster collapse |
| **Incremental layout** | Adapton-style self-adjusting computation, a cache-oblivious vEB layout index, and an epoch-based concurrent IR handle skip unchanged subgraphs on re-render |
| **E-graph crossing minimization** | Egg-based equality saturation explores rank-order rewrites in parallel, with strict node-budget and timeout guards plus Sugiyama fallback |
| **Conformal Geometric Algebra (CGA)** | Rotor-based transform composition and intersection queries for obstacle-aware edge routing |
| **High-fidelity SVG** | Responsive `viewBox`, 23 node shapes, 30 arrow variants, gradients, drop shadows, glow effects, CSS animations, custom SVG icons, cluster backgrounds, accessible ARIA markup, 10 theme presets |
| **Terminal rendering** | Braille (2×4), block (2×2), half-block, and cell-only sub-pixel modes with Unicode box-drawing, ASCII fallback, diff engine, and minimap |
| **Web / WASM** | `@frankenmermaid/core` API surface with SVG + Canvas2D rendering backends, parse/layout/render round-trip, and bidirectional source-map artifacts |
| **Deterministic output** | Same input + same config → byte-identical SVG. Stable tie-breaking at every pipeline stage |
| **FNX graph intelligence** | Optional `franken_networkx` integration adds centrality-aware semantic styling, cycle scoring, hub detection, and structural diagnostics. Phase 1 (undirected, advisory) is live; Phase 2 (directed: SCC, WCC, reachability) is in canary rollout |
| **Zero unsafe code** | `#![forbid(unsafe_code)]` in every crate |
| **DOT bridge** | Parses Graphviz DOT and converts to the shared IR for rendering through the same pipeline |

## Quick example

```bash
# Detect diagram type with confidence score
echo 'flowchart LR; A-->B-->C' | fm-cli detect -
# → Flowchart (confidence: 1.0, method: ExactKeyword)

# Render to SVG
echo 'flowchart LR; A-->B-->C' | fm-cli render - --format svg --output demo.svg

# Render to terminal (great for CI logs and SSH sessions)
echo 'flowchart LR; A-->B-->C' | fm-cli render - --format term

# Validate with structured diagnostics and a CI-friendly fail gate
echo 'flowchrt LR; A-->B' | fm-cli validate - --fail-on warning
# → Warning: fuzzy match "flowchrt" → "flowchart" (confidence: 0.85)

# Parse to IR JSON for tooling integration
echo 'sequenceDiagram; Alice->>Bob: hello' | fm-cli parse - --pretty

# Emit the capability matrix as JSON
fm-cli capabilities --pretty

# File-based workflow with dark theme + source-span attributes
fm-cli render diagrams/process.mmd \
  --format svg --theme dark \
  --embed-source-spans \
  --source-map-out diagrams/process.map.json \
  --output out/process.svg

# Compare two revisions structurally
fm-cli diff before.mmd after.mmd --format terminal
```

## Design philosophy

1. **Never waste user intent.** Malformed input degrades into best-effort IR + actionable diagnostics, not dead-end errors. If the parser can figure out what you probably meant, it will.
2. **Determinism is a feature.** Every layout phase uses stable tie-breaking. Node ordering, rank assignment, coordinate computation, edge routing, and even FNX cache keys are deterministic. CI snapshot tests rely on this.
3. **Layout quality beats minimal correctness.** Four cycle-breaking strategies, barycenter + transpose + sift crossing minimization, optional egg-based equality saturation, Brandes-Köpf coordinate assignment, obstacle-aware orthogonal edge routing, and 15 specialized algorithms.
4. **One IR, many outputs.** A shared `MermaidDiagramIr` feeds SVG, terminal, Canvas2D, and WASM. Parse once, render everywhere. Layout statistics, decision ledgers, and diagnostics travel through the entire pipeline.
5. **Polish is core product surface.** Typography, spacing, theming, accessibility, gradients, drop shadows, CSS animations, custom icons, and responsive sizing are all part of correctness.
6. **Make the runtime auditable.** Every render carries a layout decision ledger, optional source-span attributes, an evidence bundle, and a witness block describing what was decided and why.

## How it compares

| Capability | frankenmermaid | mermaid-js | mermaid-cli (`mmdc`) |
|---|---|---|---|
| Language / runtime | Rust + WASM | JavaScript | Node.js wrapper around mermaid-js |
| Parser recovery on malformed input | Best-effort with diagnostics | Often strict failure | Upstream behavior |
| Fuzzy keyword detection | Levenshtein + heuristics | No | No |
| Cycle-aware layout strategies | 4 strategies + cluster collapse | Basic | Upstream |
| Specialized layout algorithms | 15 (auto-selected per type) | Varies | Upstream |
| Incremental re-layout on small edits | Adapton DCG + epoch IR + cache-oblivious vEB | No | No |
| E-graph crossing minimization | Yes (with budget + fallback) | No | No |
| Graph-intelligence integration (centrality, SCC) | FNX (optional) | No | No |
| Terminal rendering | Built-in (4 fidelity modes + minimap + diff) | No | No |
| Canvas2D web rendering | Built-in (with mock for tests) | No | No |
| DOT format bridge | Built-in | No | No |
| Deterministic output guarantee | Explicit design goal | Not guaranteed | Not guaranteed |
| Source-span attributes / source maps | Yes (`--embed-source-spans`, `--source-map-out`) | No | No |
| SVG accessibility (ARIA, accTitle/accDescr) | Built-in | Limited | Upstream |
| WASM JS API | `@frankenmermaid/core` | Yes | No |
| `unsafe` code | Forbidden (`#![forbid(unsafe_code)]`) | N/A (JS) | N/A |

### vs Graphviz and PlantUML

| Capability | frankenmermaid | Graphviz (`dot`) | PlantUML |
|---|---|---|---|
| Input format | Mermaid + DOT bridge | DOT (native) | PlantUML DSL |
| Runtime | Native Rust + WASM | Native C | JVM (Java) |
| Deterministic output | Explicit guarantee | Mostly deterministic; depends on version + options | Mostly deterministic |
| Web rendering | SVG + Canvas2D in-browser via WASM | Server-side rendering only (no WASM port) | Server-side rendering only |
| Terminal rendering | Built-in (braille / block / half-block / cell) | Third-party `dot2tex`-style hacks only | No |
| Error recovery | Best-effort with diagnostics | Reject on first parse error | Reject on first parse error |
| Layout algorithms | 15 specialized | `dot`, `neato`, `fdp`, `sfdp`, `circo`, `twopi`, `osage`, `patchwork` | Internal hierarchical / sequence |
| Theme system | 10 built-in presets + Mermaid `themeVariables` | Attribute-based (no preset themes) | Skin system |
| Sandbox-friendly | WASM (no FS / network access) | Native binary (sandbox externally) | JVM startup overhead |
| Embedded incremental layout | Yes (Adapton DCG) | No | No |
| First-party CLI / lib / WASM / Canvas surfaces | Yes (`frankenmermaid` umbrella) | CLI (`dot`), library bindings via third-party | CLI / web service |

frankenmermaid isn't a drop-in replacement for Graphviz or PlantUML — the trade-off is that you get a single deterministic pipeline that works the same way from a CLI, a Rust library, and in the browser via WASM, with a parser tuned for human-authored Mermaid diagrams rather than algorithmic graph generation.

## Supported diagram types

<!-- BEGIN GENERATED: supported-diagram-types -->
| Diagram Type | Runtime Status |
|--------------|----------------|
| `flowchart` | Implemented |
| `sequence` | Partial |
| `class` | Implemented |
| `state` | Implemented |
| `er` | Implemented |
| `C4Context` | Implemented |
| `C4Container` | Implemented |
| `C4Component` | Implemented |
| `C4Dynamic` | Implemented |
| `C4Deployment` | Implemented |
| `architecture-beta` | Implemented |
| `block-beta` | Implemented |
| `gantt` | Implemented |
| `timeline` | Implemented |
| `journey` | Implemented |
| `gitGraph` | Implemented |
| `sankey` | Implemented |
| `mindmap` | Implemented |
| `pie` | Implemented |
| `quadrantChart` | Implemented |
| `xyChart` | Implemented |
| `requirementDiagram` | Implemented |
| `packet-beta` | Implemented |
| `kanban` | Implemented |
<!-- END GENERATED: supported-diagram-types -->

The authoritative parity matrix against the FrankenTUI reference implementation lives in [`FEATURE_PARITY.md`](FEATURE_PARITY.md). Every diagram family has dedicated detection, parser, layout dispatch, and SVG/terminal/canvas rendering. The "Partial" rows in the generated table above reflect the conformance-fixture status; in practice, every diagram type renders end-to-end through `fm-cli render`.

## Installation

### Quick install (CLI)

```bash
curl -fsSL "https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh" | bash
```

The installer auto-installs a minimal Rust toolchain via `rustup` if `cargo` is absent, then builds `frankenmermaid-cli` from the GitHub repo with `cargo install --locked` and drops the binary into `~/.local/bin`. The binary is registered under two names: `frankenmermaid` (canonical) and `fm-cli` (legacy alias).

Environment overrides:

| Variable | Default | Effect |
|---|---|---|
| `FM_INSTALL_GIT_URL` | `https://github.com/Dicklesworthstone/frankenmermaid.git` | Source repo |
| `FM_INSTALL_GIT_REV` | *(unset)* | Pin to a specific commit SHA |
| `FM_INSTALL_GIT_TAG` | *(unset)* | Pin to a specific tag |
| `FM_INSTALL_GIT_BRANCH` | `main` | Branch to install from |
| `FM_INSTALL_ROOT` | `$HOME/.local` | Install root (binary goes to `$FM_INSTALL_ROOT/bin`) |
| `FM_INSTALL_PATH` | *(unset)* | Install from a local workspace path instead of git |

### From source

```bash
git clone https://github.com/Dicklesworthstone/frankenmermaid.git
cd frankenmermaid
cargo build --release --workspace
# Binary at target/release/frankenmermaid (alias: target/release/fm-cli)
```

**Requires Rust nightly** (pinned via `rust-toolchain.toml`). The project uses Rust 2024 edition features. Workspace `rust-version` is `1.95`.

### JavaScript / WASM

> **Note on npm distribution.** The `@frankenmermaid/core` package builds cleanly to `pkg/` via `./build-wasm.sh` and the `npm-publish` CI job in `.github/workflows/ci.yml` is wired up, but it is gated to `refs/tags/v*` pushes and the project has not yet cut a tagged release — so the package is not currently on npm. Until a version is published, use one of the two methods below.

**Option 1 — build from source (recommended):**

```bash
git clone https://github.com/Dicklesworthstone/frankenmermaid.git
cd frankenmermaid
./build-wasm.sh
# Copy pkg/ into your project
cp -r pkg/ your-project/frankenmermaid/
```

**Option 2 — use the GitHub Pages bundle.** The [live demo](https://dicklesworthstone.github.io/frankenmermaid/) ships a pre-built WASM bundle. Reference it directly, or download from the `gh-pages` branch.

### Optional feature flags

```toml
[features]
default = []
watch                    = ["dep:notify"]        # File watching for `fm-cli watch`
serve                    = ["dep:tiny_http"]     # Local preview server for `fm-cli serve`
png                      = ["dep:resvg", "dep:usvg"]  # PNG rasterization
fnx-integration          = ["fm-layout/fnx-integration"]               # Phase-1 FNX advisory
fnx-experimental-directed = ["fm-layout/fnx-experimental-directed"]    # Phase-2 directed FNX
```

Default builds remain FNX-free and crates.io-clean. As of 2026-04-21 the formerly git-pinned `franken-kernel` is now consumed from crates.io (currently pinned to `0.3.1`); only the FNX feature still requires a git-pinned dependency on `franken_networkx`.

### Crates.io status

The workspace is at version `0.1.0`. Per [`CRATES_IO_PUBLISHING.md`](CRATES_IO_PUBLISHING.md), the publish order is:

```
fm-core → fm-parser → fm-layout → fm-render-svg → fm-render-term →
fm-render-canvas → fm-wasm → frankenmermaid-cli
```

`fm-cli` is taken on crates.io, so the CLI crate publishes as `frankenmermaid-cli` while preserving the `fm-cli` binary alias.

## Quick start

1. **Create** a Mermaid file:

   ```bash
   cat > demo.mmd <<'EOF'
   flowchart LR
     A[Start] --> B{Decision}
     B -->|Yes| C[Action]
     B -->|No|  D[Skip]
     C --> E[End]
     D --> E
   EOF
   ```

2. **Detect** the diagram type:

   ```bash
   fm-cli detect demo.mmd
   ```

3. **Render** to SVG:

   ```bash
   fm-cli render demo.mmd --format svg --output demo.svg
   ```

4. **Preview** in the terminal:

   ```bash
   fm-cli render demo.mmd --format term
   ```

5. **Validate** for issues with structured diagnostics:

   ```bash
   fm-cli validate demo.mmd
   ```

6. **Edit interactively** with a live split-pane preview:

   ```bash
   fm-cli interactive demo.mmd
   ```

7. **Use from JavaScript** in the browser:

   ```ts
   import { init, renderSvg } from '@frankenmermaid/core';
   await init();
   const svg = renderSvg('flowchart LR\nA-->B');
   document.getElementById('diagram').innerHTML = svg;
   ```

## Command reference

The CLI's canonical entry point is `frankenmermaid`. `fm-cli` is the legacy alias and is interchangeable.

### Global flags

```
--config <path>        Config file (TOML). Auto-discovers ./frankenmermaid.toml
                       and ~/.config/frankenmermaid/config.toml
-v, --verbose          Structured debug logging (repeatable: -vv, -vvv)
-q, --quiet            Suppress everything except errors
```

### `fm-cli render`

Parse, lay out, and render a diagram.

```bash
# SVG output to a file
fm-cli render input.mmd --format svg --output diagram.svg

# PNG rasterization (requires --features png)
fm-cli render input.mmd --format png --output diagram.png

# Terminal preview
fm-cli render input.mmd --format term

# ASCII-only (no Unicode box-drawing)
fm-cli render input.mmd --format ascii

# With theme, layout override, and explicit dimensions
fm-cli render input.mmd --format svg --theme dark --layout-algorithm force \
  -W 1280 -H 800 --font-size 14

# From stdin
echo 'flowchart TD; A-->B' | fm-cli render - --format svg

# Embed source spans + emit a JSON source map artifact
fm-cli render input.mmd --format svg \
  --embed-source-spans \
  --source-map-out input.map.json \
  --output input.svg
```

Render-time flags include `--parse-mode {strict|compat|recover}`, `--layout-algorithm {auto|sugiyama|force|tree|radial|timeline|gantt|sankey|kanban|grid}` (the 10 most useful general-purpose algorithms; the 6 chart-style layouts — `sequence`, `xychart`, `pie`, `quadrant`, `gitgraph`, `packet` — are auto-dispatched per diagram type and can also be selected by name in `frankenmermaid.toml`), and FNX controls (`--fnx-mode {auto|enabled|disabled}`, `--fnx-projection {undirected|directed}`, `--fnx-fallback {graceful|strict}`).

### `fm-cli parse`

Emit the intermediate representation as JSON.

```bash
fm-cli parse input.mmd                # Summary
fm-cli parse input.mmd --full         # Full IR
fm-cli parse input.mmd --full --pretty
```

### `fm-cli detect`

Detect the diagram type with confidence and method.

```bash
fm-cli detect input.mmd
fm-cli detect input.mmd --json
# → { "type": "flowchart", "confidence": 1.0, "method": "ExactKeyword" }
```

### `fm-cli validate`

Check syntax and semantics, print diagnostics with source spans, optionally write a machine-readable JSON artifact.

```bash
fm-cli validate input.mmd
fm-cli validate input.mmd --format json --diagnostics-out diags.json
fm-cli validate input.mmd --fail-on warning
fm-cli validate input.mmd --fnx-mode enabled        # Include FNX structural diagnostics
```

`--fail-on` accepts `error` (default), `warning`, `hint`, or `none`.

### `fm-cli capabilities`

Emit the runtime capability claim matrix as JSON. This is the same matrix surfaced at the top of this README.

```bash
fm-cli capabilities --pretty
fm-cli capabilities --output capabilities.json
```

### `fm-cli diff`

Compare two diagrams and emit a structural diff.

```bash
fm-cli diff before.mmd after.mmd --format terminal     # Side-by-side with ANSI colors
fm-cli diff before.mmd after.mmd --format summary      # Aggregate counts
fm-cli diff before.mmd after.mmd --format json         # Machine-readable
fm-cli diff before.mmd after.mmd --format plain        # Color-stripped text
```

The diff engine classifies each node and edge as `Added`, `Removed`, `Changed` (with the specific change kind: `LabelChanged`, `ShapeChanged`, `ClassesChanged`, `MembersChanged`, `ArrowChanged`), or `Unchanged`.

### `fm-cli interactive`

Launch a split-pane terminal editor with a live diagram preview.

```bash
fm-cli interactive input.mmd
fm-cli interactive input.mmd --theme dark
```

**Ctrl+Q** to quit, **Ctrl+S** to save. Requires a real TTY (not available under CI or piped input).

### `fm-cli watch` *(requires `--features watch`)*

Watch a file and re-render on every change. Particularly effective with the incremental layout engine, which skips stages whose inputs have not changed.

```bash
fm-cli watch diagrams/process.mmd --format term --clear
```

### `fm-cli serve` *(requires `--features serve`)*

Start a local HTTP playground with live reload.

```bash
fm-cli serve --host 127.0.0.1 --port 4173 --open
```

### `evidence` binary

A separate `evidence` binary ships alongside `fm-cli` and is responsible for emitting structured evidence bundles consumed by CI release-signoff workflows. See "Evidence and release signoff" below.

## JavaScript / WASM API

```ts
import {
  init,
  renderSvg,
  detectType,
  parse,
  describeDiagram,
  diagramLens,
  applyLensEdit,
  parseLens,
  applyParseLensEdit,
  Diagram,
} from '@frankenmermaid/core';

// Initialize with defaults — sets theme and the default render config used
// when a per-call config isn't passed.
await init({ theme: 'corporate' });

// Render a complete SVG string
const svg = renderSvg('flowchart LR\nA-->B', { theme: 'dark' });

// Detect diagram type
const type = detectType('sequenceDiagram\nAlice->>Bob: hi');
// → { type: "sequence", confidence: 1.0, method: "ExactKeyword" }

// Parse to IR (returns the full ParseResult: ir, warnings, confidence,
// detection_method, format_complement)
const parsed = parse('classDiagram\nA <|-- B');

// Generate a human-readable description (for screen readers, alt text)
const description = describeDiagram('flowchart TD\nA-->B-->C');

// Bidirectional structural lens for editor integrations
const lens = diagramLens('flowchart LR\nA-->B');
const edited = applyLensEdit(lens, { /* ... */ });

// Parse-tree-level lens for token-grained edits
const ptLens = parseLens('flowchart LR\nA-->B');
const ptEdited = applyParseLensEdit(ptLens, { /* ... */ });

// Imperative Diagram class with Canvas2D (or WebGPU fallback) renderer
const diagram = new Diagram(
  document.getElementById('canvas-root')!,
  { renderer: 'canvas2d' },
);
diagram.render('flowchart TD\nStart-->End');
diagram.setTheme('dark');
diagram.on('rendered', () => console.log('done'));
diagram.destroy();
```

The wasm-bindgen surface intentionally stays narrow: nine free functions (`init`, `renderSvg`, `detectType`, `parse`, `describeDiagram`, `diagramLens`, `applyLensEdit`, `parseLens`, `applyParseLensEdit`) plus the `Diagram` class. The capability matrix and source-span artifacts are produced by the CLI / Rust library surfaces; for browser-side capability introspection use the auto-generated metadata block at the top of this README or load the JSON emitted by `fm-cli capabilities`.

The WASM build integrates the same `IncrementalLayoutEngine` used by the CLI, so successive renders of near-identical input skip stages whose dependency-graph inputs have not changed.

## The lens system — bidirectional editor integration

`diagramLens` / `applyLensEdit` and `parseLens` / `applyParseLensEdit` together form a bidirectional bridge between source text and structured edits. The motivating constraint: when an editor performs a structural action ("rename node `A` to `Start`", "swap participant order", "add a new task to section `Backend`"), the resulting source text should preserve everything else exactly — comments, whitespace, ordering of unrelated declarations, even quote style.

Naive parse → re-emit pipelines lose all of that. The lens system instead:

1. Parses the input into an IR plus a **trivia map** that records the exact original spelling of every token (whitespace, comments, quote style, delimiter choice).
2. Exposes structural edit operations that operate on the IR while preserving the trivia map for unchanged regions.
3. Emits the edited source by walking the trivia map and substituting only the changed tokens.

| Function | Use case |
|---|---|
| `diagramLens(source)` | Diagram-level structural lens — edits at the node/edge level |
| `applyLensEdit(lens, edit)` | Apply one structural edit, return the new source |
| `parseLens(source)` | Lower-level parse lens — gives access to parse-tree node-level edits |
| `applyParseLensEdit(lens, edit)` | Apply one parse-tree edit |

The showcase's structural-edit toolbar runs entirely on the lens system. The same lens bindings are exported from the WASM API so editor integrations (VS Code extensions, web playgrounds) can perform refactor-style edits without writing their own incremental parser.

---

## Configuration

`fm-cli` auto-discovers `./frankenmermaid.toml`, then `~/.config/frankenmermaid/config.toml`. CLI flags always override the file.

```toml
# Global behavior
[core]
max_input_bytes   = 5_000_000   # Hard input-size cap, enforced on stdin and files
fallback_on_error = true        # Best-effort parse on failure (default true; set false to error out instead of recovering)

# Parser settings
[parser]
intent_inference         = true   # Fuzzy keyword matching
fuzzy_keyword_distance   = 2      # Max Levenshtein distance
auto_close_delimiters    = true   # Auto-close unclosed brackets
create_placeholder_nodes = true   # Create nodes for dangling edges

# Layout defaults
[layout]
algorithm     = "auto"          # auto | sugiyama | force | tree | radial | timeline | gantt | sankey | kanban | grid | sequence | xychart | pie | quadrant | gitgraph | packet
cycle_strategy = "cycle-aware"  # greedy | dfs-back | mfas | cycle-aware
node_spacing  = 80              # Horizontal gap between rank-adjacent nodes
rank_spacing  = 120             # Vertical gap between ranks
edge_routing  = "orthogonal"    # orthogonal | spline

# Render defaults
[render]
default_format    = "svg"
show_back_edges   = true
reduced_motion    = "auto"      # auto | reduce | no-preference

# SVG visual system
[svg]
theme           = "corporate"
rounded_corners = 8
shadows         = true
gradients       = true
accessibility   = true          # ARIA labels, semantic markup, source-span attributes
enable_links    = false         # Whether `click` directives produce clickable elements
link_mode       = "off"         # off | inline | footnote

# Terminal renderer
[term]
tier    = "rich"                # compact | normal | rich
unicode = true                  # Unicode box-drawing vs ASCII
minimap = true                  # Scaled overview for large diagrams
```

The TOML config uses `deny_unknown_fields`, so a typo or an unrecognized key is a hard error rather than silently ignored. Per-section keys are exactly those listed above; anything else (e.g., `edge_bundling`, `max_nodes`) lives in `MermaidConfig` and is reachable through the WASM / Rust APIs but is not currently exposed in the file format.

Mermaid-style inline `%%{init}%%` directives are also honored when `parser.enable_init_directives = true`:

```mermaid
%%{init: {"theme":"dark","flowchart":{"curve":"basis"}} }%%
flowchart LR
A --> B
```

Resolution priority (highest first): CLI flags → inline `%%{init}%%` → config file → built-in defaults.

### Init directive deep dive

Init directives are JSON5-compatible (so trailing commas and unquoted keys are accepted) and must appear before the diagram body. The full grammar supports nested objects:

```mermaid
%%{init: {
  "theme": "dark",
  "themeVariables": {
    "primaryColor": "#3b82f6",
    "lineColor": "#94a3b8",
    "clusterBkg": "rgba(15, 23, 42, 0.6)"
  },
  "flowchart": { "rankDir": "LR", "curve": "basis" },
  "sequence": { "mirrorActors": false, "showSequenceNumbers": true },
  "securityLevel": "strict"
}}%%
```

| Variable | Type | Effect |
|---|---|---|
| `theme` | string | Selects a theme preset (one of the 10 named themes) |
| `themeVariables.primaryColor` | color | Overrides the primary node fill |
| `themeVariables.lineColor` | color | Overrides edge / line color |
| `themeVariables.clusterBkg` | color | Overrides cluster background |
| `flowchart.rankDir` / `flowchart.direction` | `LR`/`TB`/`RL`/`BT` | Sets graph direction |
| `flowchart.curve` | `basis`/`linear`/`step` | Edge curve interpolation style |
| `sequence.mirrorActors` | bool | Show actors at bottom as well |
| `sequence.showSequenceNumbers` | bool | Number each sequence message |
| `securityLevel` | `strict`/`loose` | Controls link / script sanitization |

Init directives are only honored when `parser.enable_init_directives = true` in the config (default `false` for security; enable explicitly when you trust the input source).

### MermaidConfig field reference

The complete runtime configuration surface, defined in `fm-core/src/lib.rs`:

| Field | Default | Purpose |
|---|---|---|
| `enabled` | `true` | Master switch — when `false`, render returns a noop placeholder |
| `glyph_mode` | `Unicode` | `Unicode` / `Ascii` — character set for terminal rendering |
| `render_mode` | `Auto` | `Auto` / `CellOnly` / `Braille` / `Block` / `HalfBlock` — sub-cell mode (`Auto` picks based on detected terminal capability) |
| `tier_override` | `Normal` | `Compact` / `Normal` / `Rich` / `Auto` — terminal detail tier |
| `max_nodes` | `200` | Soft cap; degradation warning when exceeded |
| `max_edges` | `400` | Soft cap; degradation warning when exceeded |
| `route_budget` | `4_000` | Routing-operation budget |
| `layout_iteration_budget` | `200` | Crossing-min iteration budget |
| `edge_bundling` | `false` | Merge parallel edges into bundles |
| `edge_bundle_min_count` | `3` | Minimum edge count before bundling |
| `max_label_chars` | `48` | Truncate labels beyond this length |
| `max_label_lines` | `3` | Cap multi-line labels |
| `wrap_mode` | `WordChar` | `WordChar` / `Word` / `Char` / `None` — text wrapping strategy |
| `enable_styles` | `true` | Whether `classDef`/`style` directives are applied |
| `enable_init_directives` | `false` | Whether `%%{init}%%` blocks are honored |
| `enable_links` | `false` | Whether `click` directives produce clickable elements |
| `link_mode` | `Off` | `Off` / `Inline` / `Footnote` — link rendering strategy |
| `sanitize_mode` | `Strict` | `Strict` / `Lenient` — URL scheme allow-list |
| `error_mode` | `Panel` | How errors are surfaced in render output |
| `log_path` | `None` | Optional path for structured tracing output |
| `cache_enabled` | `true` | Toggle the layout / FNX cache layers |
| `capability_profile` | `None` | Pinned capability profile (overrides auto-detection) |
| `debug_overlay` | `false` | Render crossing/bend/symmetry metrics on top of the diagram |
| `palette` | `Default` | One of the named palette presets |
| `theme` | `None` | Mermaid-style theme name |
| `theme_variables` | `{}` | Mermaid-style `themeVariables` overrides |
| `flowchart_direction` | `None` | Direction hint from init directives |
| `flowchart_curve` | `None` | Curve style from init directives |
| `sequence_mirror_actors` | `None` | Sequence mirror toggle from init directives |
| `sequence_show_sequence_numbers` | `None` | Sequence numbering toggle from init directives |

## Architecture

### Workspace layout

```
frankenmermaid/
├── Cargo.toml                 # Workspace root (version 0.1.0)
├── rust-toolchain.toml        # Nightly toolchain pin
├── crates/
│   ├── fm-core/               # Shared IR, config, errors, diagnostics, CGA primitives
│   ├── fm-parser/             # Detection + Mermaid/DOT parsing + recovery + IR builder
│   ├── fm-layout/             # 15 algorithms, cycle breaking, e-graph, FNX, incremental
│   ├── fm-render-svg/         # Zero-dep SVG document/element/path/text/defs + theme system
│   ├── fm-render-term/        # Terminal rendering (4 fidelity modes) + diff + minimap
│   ├── fm-render-canvas/      # Canvas2D rendering with mock context for tests
│   ├── fm-wasm/               # wasm-bindgen API + lens bindings + WebRenderer selection
│   ├── fm-cli/                # CLI surface (frankenmermaid + fm-cli binaries + evidence)
│   └── fm-regression-harness/ # Real-world corpus ingestion + HTML thumbnail report
├── docs/                      # FNX integration guides + migration + compatibility matrix
├── legacy_mermaid_code/       # Reference corpus (gitignored gitlink)
└── tests/                     # Cross-component integration tests
```

### Crate map

| Crate | Lines | Responsibility |
|---|---|---|
| `fm-core` | ~15,600 | Shared IR, 23 node shapes, 30 arrow types, config, diagnostics, CGA, evidence, canary state machine |
| `fm-parser` | ~18,000 | 24-type detection + parsing + error recovery + DOT bridge + IR builder + interning |
| `fm-layout` | ~34,100 | 15 algorithms, 4 cycle strategies, Brandes-Köpf coords, E-graph crossing, CGA routing, Adapton incremental, FNX adapter |
| `fm-render-svg` | ~13,800 | Accessible themeable SVG with gradients/shadows/glows/CSS animations/custom icons + 10 theme presets |
| `fm-render-term` | ~6,500 | Terminal renderer + diff engine + minimap + 4 fidelity modes |
| `fm-render-canvas` | ~3,500 | Canvas2D rendering with trait-based abstraction and mock context |
| `fm-wasm` | ~1,800 | wasm-bindgen API + TypeScript bindings + lens edits |
| `fm-cli` | ~19,900 | CLI surface, evidence binary, golden / conformance / benchmark harnesses |
| `fm-regression-harness` | ~1,000 | Real-world Mermaid corpus ingestion + HTML thumbnail report |
| **Total** | **~114,000** | |

### Pipeline

```
            Mermaid / DOT text
                    │
                    ▼
      ┌──────────────────────────────┐
      │ fm-parser                    │
      │  • type detection            │  24 diagram types
      │  • fuzzy matching            │  Levenshtein + heuristics
      │  • recovery + warnings       │  best-effort, never crashes
      │  • IR builder (interning)    │
      └──────────────────────────────┘
                    │ MermaidDiagramIr
                    ▼
      ┌──────────────────────────────┐
      │ fm-core                      │
      │  • IR types + 23 shapes      │
      │  • CGA primitives            │
      │  • diagnostic categories     │
      │  • epoch-based IR handle     │
      └──────────────────────────────┘
                    │
                    ▼
      ┌──────────────────────────────┐
      │ fm-layout                    │
      │  • capability-aware dispatch │  config-aware algorithm selection
      │  • cycle strategy            │  4 modes (greedy/dfs/mfas/cycle-aware)
      │  • Brandes-Köpf coords       │
      │  • E-graph crossing min      │  egg + budget + Sugiyama fallback
      │  • CGA orthogonal routing    │  obstacle-aware via intersection queries
      │  • incremental (Adapton)     │  dependency-graph cache, skip-on-clean
      │  • [optional] FNX advisory   │  centrality tiers, bridges, SCC/WCC
      │  • guardrails (time/iter/op) │  fallback ladder
      │  • decision ledger           │
      └──────────────────────────────┘
                    │ DiagramLayout + stats + ledger
                    ▼
      ┌──────────────────────────────┐
      │ Render-Scene IR              │
      │  Groups/Paths/Text + source  │
      │  tags, backend-agnostic      │
      └──────┬──────────┬────────────┘
             │          │            │
             ▼          ▼            ▼
       ┌─────────┐ ┌──────┐ ┌──────────┐
       │   SVG   │ │ Term │ │  Canvas  │
       └─────────┘ └──────┘ └──────────┘
              │                    │
              ▼                    ▼
         SVG / PNG          WASM + browser
```

### Release profile

```toml
[profile.release]
opt-level     = "z"     # Optimize for WASM binary size
lto           = true    # Link-time optimization
codegen-units = 1       # Single codegen unit
panic         = "abort" # No unwinding overhead
strip         = true    # Remove debug symbols

[profile.release.package.fm-layout]
opt-level = 3           # Layout is the computational bottleneck

[profile.dev.package.fm-layout]
opt-level = 3           # Keep dev iteration fast on layout work too
```

## How the parser works

The parser runs a **five-tier detection pipeline** to identify the diagram type, then dispatches to a type-specific parser that produces the shared IR.

### Type detection pipeline

```
Input text
    ▼
┌─────────────────────────────────────────────────┐
│ 1. DOT Format Detection            conf: 0.95   │
│    digraph/graph keyword + braces               │
├─────────────────────────────────────────────────┤
│ 2. Exact Keyword Match             conf: 1.0    │
│    flowchart, sequenceDiagram, classDiagram,    │
│    stateDiagram, erDiagram, gantt, pie, …       │
├─────────────────────────────────────────────────┤
│ 3. Fuzzy Keyword Match           conf: 0.70+    │
│    Levenshtein distance 1-2                     │
│    "flowchrt" → "flowchart"                     │
├─────────────────────────────────────────────────┤
│ 4. Content Heuristics            conf: 0.60+    │
│    Arrow patterns: -->, ->>, ||--o{             │
│    Keywords: participant, state, branch, commit │
├─────────────────────────────────────────────────┤
│ 5. Fallback                        conf: 0.30   │
│    Default to Flowchart + warning               │
└─────────────────────────────────────────────────┘
```

### Fuzzy matching

A two-row dynamic-programming Levenshtein computation (O(mn) time, O(n) space) against the 17 base keywords in `DIAGRAM_KEYWORDS` (one per diagram family, with `graph` aliased to `flowchart`). Only distances 1–2 are accepted:

| Distance | Confidence | Example |
|---|---|---|
| 0 | 1.0 (handled by tier 2) | `flowchart` |
| 1 | 0.85 | `flowchrt` → `flowchart` |
| 2 | 0.70 | `flwchart` → `flowchart` |
| ≥ 3 | Rejected | Too ambiguous |

### Error recovery strategies

The parser never panics on malformed input. The recovery toolbox includes:

1. **Dangling-edge recovery** — references to undeclared nodes auto-create an `implicit: true` placeholder node with a `recovery`-category diagnostic.
2. **Node deduplication** — repeated node IDs with different label/shape variants are merged, preserving the most specific information.
3. **Label normalization** — quotes, backticks, and surrounding whitespace are stripped.
4. **Graceful unknown syntax** — lines that don't match any known pattern produce a warning-level diagnostic but don't abort parsing.
5. **Auto-close delimiters** — when configured, unclosed brackets are silently closed at end-of-line.

The result is that even heavily malformed input produces a best-effort IR with diagnostics explaining what was recovered.

### IR builder

The parser doesn't construct `MermaidDiagramIr` directly. It uses an `IrBuilder` that provides:

- **Node interning** — `intern_node(id)` deduplicates by ID. If a later reference adds a label or shape the existing node lacks, it's merged in place. This is why `A --> B` followed by `A[Start]` correctly assigns the "Start" label to A.
- **Cluster/subgraph bidirectional consistency** — every update to `ir.clusters` is mirrored in `ir.graph.clusters`.
- **Label interning** — labels live in a shared `Vec<IrLabel>` referenced by `IrLabelId`, eliminating string duplication.
- **Semantic recovery** — `apply_semantic_recovery()` runs after parsing to emit diagnostics for auto-created placeholder nodes and other implicit decisions.

## How the layout engine works

`fm-layout` takes a parsed `MermaidDiagramIr` and produces a `DiagramLayout`: positioned node boxes, routed edge paths, and cluster boundaries. The output shape is identical regardless of which algorithm produced it.

### Algorithm auto-selection

When `algorithm = "auto"` (the default), a **capability-aware dispatcher** consults graph analysis (density, branching factor, cycle presence, leaf count) to map each diagram type to its best algorithm:

| Algorithm | Used for | Strategy |
|---|---|---|
| **Sugiyama** | Flowchart, class, state, ER, C4, requirement, architecture | Hierarchical layered layout with rank assignment, Brandes-Köpf coords, and crossing minimization |
| **Force** | Available for all graph types (fallback for dense cyclic graphs) | Fruchterman-Reingold spring-electrical with Barnes-Hut for n>100 |
| **Tree** | Available for all graph types | Reingold-Tilford tidy tree with Knuth-style spacing |
| **Radial** | Mindmap | Concentric rings with angle allocation proportional to subtree leaf count |
| **Sequence** | Sequence diagrams | Participant columns, message stacking, activation bars, notes, fragments |
| **Timeline** | Timeline | Horizontal periods with vertically stacked events |
| **Gantt** | Gantt | Time-axis bar layout with section swimlanes |
| **XyChart** | XY chart | Cartesian axis layout with category padding and series anchors |
| **Sankey** | Sankey | Flow-conserving column layout with iterative relaxation |
| **Kanban** | Journey, kanban | Fixed-column card stacking with metadata-aware coloring |
| **Grid** | Block-beta | CSS-grid-like positioning with column/row spans and nested groups |
| **Pie** | Pie | Radial slice geometry with perimeter label anchoring |
| **Quadrant** | Quadrant chart | Four-quadrant scatter on `[0,1]` axes |
| **GitGraph** | Git graph | Branch-lane layout with chronological commit stacking |
| **Packet** | Packet-beta | Grid-derived packet-lane layout |

### The Sugiyama algorithm

Sugiyama is the workhorse for most graph diagram types. Seven phases:

**Phase 1 — Cycle removal.** Cycles must be broken before layering. Four strategies are available (see "Cycle strategies" below). All use **Tarjan's strongly connected components** under the hood with on-stack flags to distinguish back-edges from cross-edges.

**Phase 2 — Rank assignment.** Each node is assigned an integer rank using a longest-path heuristic in topological order so every non-reversed edge goes from a lower rank to a higher one.

**Phase 3 — Crossing minimization (barycenter).** The order of nodes within each rank is optimized to minimize edge crossings. The algorithm performs 4 bidirectional sweeps: for each rank compute each node's barycenter (the weighted average position of its neighbors in the adjacent rank), sort by barycenter, tie-break by stable node index, sweep top↔bottom.

**Phase 4 — Crossing refinement (transpose + sift).** Two local-search passes further reduce crossings: **transpose** swaps adjacent pairs that reduce crossings (up to 10 passes, early-exit on zero crossings); **sifting** evaluates all positions for each node and moves it to the best one. The crossing count itself is computed via a merge-sort inversion-counting algorithm at O(m log m) per rank pair.

The layout stats record `crossing_count_before_refinement` and the final `crossing_count`.

**Phase 5 — Coordinate assignment (Brandes-Köpf).** Coordinates are computed using the Brandes-Köpf algorithm, which produces balanced, type-aware horizontal positioning. The original recursive implementation was rewritten as an iterative variant to eliminate stack-overflow risk on deep cyclic graphs.

**Phase 6 — Edge routing.** Edges are routed as orthogonal (Manhattan) paths. Special cases:

- **Self-loops** route as rectangular loops extending to the right and back.
- **Parallel edges** receive incremental lateral offsets so they remain visually distinguishable.
- **Reversed edges** are flagged `reversed: true` so renderers can mark them visually.
- **Obstacle-aware routing** uses CGA intersection queries to detect segments that pass through node bounding boxes and re-route around them.

**Phase 7 — Post-processing.** Cluster boundaries are computed to enclose their member nodes with configurable padding (default 52px), coordinates are normalized to non-negative values, and edge-length quality metrics (`total_edge_length`, `reversed_edge_total_length`) are recorded.

### Cycle strategies

| Strategy | How it works | When to use |
|---|---|---|
| **Greedy** | Repeatedly remove sinks (out-degree 0) and sources (in-degree 0). Order remaining nodes by `max(out_degree − in_degree)`. Reverse edges that violate the resulting order | Fast default. Good enough for most graphs |
| **DFS back-edge** | Standard DFS with three-color marking. Edges to nodes in the "visiting" state are back-edges and get reversed. Linear O(V+E) and reproducible | Predictable results, identical DFS order → identical reversed-edge set. Iterative implementation — no stack-overflow risk on deep graphs |
| **MFAS approximation** | Operates per SCC. Sorts nodes by `(out_degree − in_degree)` descending; reverses edges that violate the position order. Falls back to DFS if no improvement | Minimum reversed edges → better visual quality |
| **Cycle-aware** | Full SCC detection with optional cluster collapse. Records `cycle_count`, `cycle_node_count`, `max_cycle_size`, and `reversed_edge_total_length` | Best visual quality. Cycle clusters render as grouped boxes |

### E-graph equality saturation for crossing minimization

For graphs where greedy barycenter ordering leaves room on the table, `fm-layout` includes an optional **egg-based equality saturation** pass (`fm-layout/src/egraph_crossing.rs`). It explores rank-order rewrites in parallel under a node-budget and timeout, then extracts the lowest-crossing ordering. If the budget is exceeded, the engine falls back to Sugiyama's standard refinement automatically.

Hardening includes:

- Hard node-budget and timeout guards (fault-tested).
- Memory-explosion / budget-exhaustion fault tests.
- Criterion benchmarks comparing E-graph vs greedy.

### Conformal Geometric Algebra (CGA)

`fm-layout` and `fm-render-svg` share a CGA primitive layer (`fm-core/src/cga.rs`, `fm-layout/src/cga_routing.rs`, `fm-render-svg/src/cga_transform.rs`) used for two distinct purposes:

1. **Edge routing.** Geometric primitives (`CgaPoint`, `CgaLineSegment`, `CgaCircle`, `CgaRect`) and intersection queries let the orthogonal router detect when a candidate segment would pass through an obstacle and reroute around it.
2. **SVG transform composition.** A `TransformStack` composes rotors (rotation), translations, and scales through geometric-algebra products instead of stacking matrices. This keeps determinism intact and gives correct scale-extraction even after composed transforms.

### Incremental layout

For interactive editors, batch processing, and live previews, full re-layout is wasteful when only a few nodes changed. `fm-layout` includes a comprehensive incremental engine:

- **Adapton-style self-adjusting computation** (`fm-layout/src/adapton.rs`) — dependency-graph cache where each pipeline stage records its inputs. When inputs are unchanged on the next run, the stage's output is reused. The trace marks reuse explicitly so you can see what was skipped.
- **Epoch-based concurrent IR handle** (`fm-core/src/epoch.rs`) — multiple readers can observe a consistent IR snapshot while another agent prepares the next epoch.
- **Cache-oblivious vEB layout** (`fm-layout/src/cache_oblivious.rs`) — a `build_veb()` BFS-index tree traversal gives a cache-oblivious layout of nodes so that recursive access patterns make good use of every level of the memory hierarchy without tuning to a specific cache size. The rewrite replaced an older count-based recursion that had correctness and shift-overflow edge cases.
- **Boundary smoothing for incremental layout** — when only part of the graph is recomputed, edges that cross the boundary between dirty and clean subgraphs can develop visible kinks. `smooth_boundary_edges()` applies Laplacian smoothing along those edges so the visual seam disappears without retouching the clean side.
- **Layout recompute duration tracing** — every cached vs recomputed stage is timed in the trace.

The WASM `Diagram` class wires the same `IncrementalLayoutEngine`, so successive renders of near-identical inputs in the browser also benefit.

### Adapton typed DCG phases

The Adapton implementation in `fm-layout/src/adapton.rs` is intentionally simpler than the full Adapton paper. It's optimized for layout workloads:

- **Single-threaded** — layout is inherently sequential, so concurrent invalidation isn't needed.
- **Coarse-grained invalidation** — at whole-phase granularity, not per-node. This keeps bookkeeping cheap; finer-grained tracking (per-node, per-rank) is reserved for future work.
- **Known type set** — no dynamic type erasure; the DCG knows the four phase types statically.

The typed phases are:

| Phase | What gets cached |
|---|---|
| **Graph metrics** | Node/edge counts, density, branching factor, cycle presence (used by the dispatcher) |
| **Rank assignments** | Node-to-rank mapping after the longest-path heuristic |
| **Node orderings** | Within-rank ordering after barycenter + transpose + sift |
| **Final layout** | Complete positioned diagram including edge paths |

Each phase records its input fingerprint (typically an FNV-1a hash of the inputs). On the next run the DCG compares fingerprints and skips re-computation when they match. The fingerprint of the FNX cache key explicitly includes `fnx_enabled` so a cached layout produced FNX-on never leaks across to an FNX-off render.

### Brandes-Köpf coordinate assignment

After rank assignment and crossing minimization, nodes need actual x/y coordinates. The Brandes-Köpf algorithm computes them in four directional passes, then averages the result:

1. **Four alignments** — for each combination of `{top-down, bottom-up} × {leftmost, rightmost}`, the algorithm produces a candidate horizontal coordinate per node by aligning nodes against the "median" of their neighbors in the adjacent rank.
2. **Vertical compaction** — within each alignment, nodes in the same alignment-block share a coordinate; the algorithm packs blocks horizontally subject to the `node_spacing` constraint.
3. **Balancing** — for each node, the final coordinate is the average of the two median candidate coordinates (the smallest and largest of the four, dropped).
4. **Normalization** — coordinates are shifted so the layout origin is `(0, 0)`.

This produces straight edges through "long" chains, minimal horizontal travel, and balanced placement under most graph shapes. The original recursive implementation was rewritten to an iterative variant after a series of stack-overflow regressions on deep cyclic graphs.

### Force-directed deep dive (Fruchterman-Reingold)

For graphs where hierarchical layering isn't appropriate, the force-directed layout simulates a physical system where nodes repel each other and edges act as springs.

**Physics model.** Two forces per iteration:

- **Repulsive force** between all node pairs: `F = k² / distance` (inverse-distance, like electrical charge). Prevents node overlap.
- **Attractive force** along edges: `F = distance² / k` (Hooke's law). Pulls connected nodes together.

Where `k` is the ideal edge length, computed as `sqrt(area / node_count)`.

**Cooling schedule.** Linear cooling: `temperature = t₀ × (1.0 - progress)` where `t₀ = k × 10.0`. The temperature limits how far nodes can move per iteration, preventing oscillation as the system converges.

**Iteration budget.** `min(50 + n × 2, 500)`. A 10-node graph runs 70 iterations; a 200-node graph runs the maximum 500.

**Cluster cohesion.** For graphs with clusters, an additional cohesion force pulls nodes toward their cluster centroid with strength 0.3, keeping visually grouped nodes together without hard containment constraints.

**Barnes-Hut optimization.** For graphs with more than 100 nodes, the engine switches from O(n²) all-pairs force computation to a grid-based Barnes-Hut approximation. Grid size: `√n` cells per side. Opening-angle threshold: 1.5. Within-cell interactions are computed exactly; cross-cell interactions are approximated using the cell centroid. This reduces force computation from O(n²) to roughly O(n log n).

**Deterministic initial placement.** Initial positions are computed from FNV-1a hashes of node IDs (prime: `0x0100_0000_01b3`, offset: `0xcbf2_9ce4_8422_2325`), laid out in a `⌈√n⌉`-column grid with ±30% jitter derived from hash bits. This combined with IEEE 754 deterministic arithmetic guarantees identical final positions for identical inputs.

**NaN guards.** Force layout proactively guards against NaN positions (division by tiny distances after node collisions) by reflecting the offending node and re-seeding from its hash.

### Tree layout (Reingold-Tilford variant)

1. **Root selection** — all nodes with in-degree 0. If there are multiple, they're treated as siblings of a virtual root.
2. **Depth assignment** — BFS from roots assigns each node to a level.
3. **Subtree span computation** — bottom-up recursive calculation. Each node's span is `max(own_width, sum_of_children_spans)`.
4. **Coordinate assignment** — children are centered under their parent. Siblings are spaced by `node_spacing`.
5. **Direction support** — TB (default), LR, RL, BT. The depth axis and breadth axis swap roles depending on direction.

### Radial layout (leaf-weighted angle allocation)

For mindmaps and hierarchical structures benefiting from a radial arrangement:

1. **Leaf counting** — memoized bottom-up count of leaf descendants per subtree.
2. **Angle allocation** — each child receives an angular range proportional to its leaf count relative to its siblings' total leaf count.
3. **Ring radius** — each depth level gets its own radius, growing outward. The radius increment accounts for the widest node at that level plus `rank_spacing`.
4. **Positioning** — polar coordinates `(angle, radius)` are converted to Cartesian `(x, y)` for the final layout.
5. **Floating-point drift correction** — the last child's angle span is adjusted to exactly fill the remaining range, preventing gaps from accumulated rounding errors.

### Sankey, Grid, Pie, Quadrant, GitGraph, Packet

Each specialized layout is small but purpose-built. Key invariants:

- **Sankey** — column assignment by reachability from sources, height proportional to total flow (`30 + max(in_degree, out_degree) × 14.0` px), column spacing `rank_spacing + 136 px`, within-column ordering iteratively relaxed to minimize flow-band crossings.
- **Grid (block-beta)** — column count from `columns N` directive or `⌈√n⌉` default, cell sizing `max_node_width + node_spacing` × `max_node_height + rank_spacing × 0.6`, column spanning via `:N` suffix, `space[:N]` empty cells, nested `block:id … end` sub-grids.
- **Pie** — slice angles computed from values, perimeter label anchoring with collision avoidance, accent colors from the active theme.
- **Quadrant** — `[0,1]` axes with central cross at `(0.5, 0.5)`, point positions mapped from input coordinates directly.
- **GitGraph** — branch lanes assigned in deterministic order, commits stacked chronologically within each lane, merge edges drawn from feature-lane to main-lane.
- **Packet (packet-beta)** — grid-derived field layout where each packet field occupies one or more consecutive cells; field widths scale with bit-widths.

### Edge bundling

When `edge_bundling = true` and at least `edge_bundle_min_count` edges (default 3) share the same source-target pair, the layout engine replaces them with a single representative edge carrying a count label (`×N`). The representative edge inherits the most common arrow type among the bundled edges and shows the merged label set in a popover-style annotation.

### Layout guardrails

For very large diagrams, the engine enforces time, iteration, and routing-operation budgets:

| Budget | Default | When exceeded |
|---|---|---|
| **Time** | 250 ms | Fall back to a cheaper algorithm (Tree, then Grid) |
| **Iterations** | ~1,000 | Skip refinement phases (transpose/sifting) |
| **Route operations** | ~10,000 | Simplify edge routing |

A `LayoutGuardDecision` records `initial_algorithm`, `selected_algorithm`, `fallback_applied`, and `reason`. The fallback chain tries cheaper alternatives in preference order:

```
Sugiyama → Tree → Grid
Force    → Tree → Grid
Radial   → Tree → Sugiyama
```

### Layout decision explanation

`fm-cli` can emit a **layout decision ledger** describing why a particular algorithm, cycle strategy, and refinement plan were chosen for a given diagram. This is part of the broader pressure-adaptive runtime epic (`bd-3uz`) and includes a global budget broker that coordinates parse / layout / render so a single stage can't starve the others.

## FNX graph-intelligence integration

`FNX` is an optional graph-analysis layer (powered by [`franken_networkx`](https://github.com/Dicklesworthstone/franken_networkx)) that provides structural intelligence to improve layout quality and surface actionable diagnostics. **FNX is advisory only** — the native layout engine always has final authority.

The integration is gated by two feature flags:

```bash
cargo build -p fm-cli --features fnx-integration              # Phase 1 (undirected)
cargo build -p fm-cli --features fnx-experimental-directed    # Phase 1 + Phase 2 (directed)
```

Default builds remain FNX-free.

### Phase 1 — undirected advisory (live)

Phase 1 wraps fnx's undirected algorithms (degree centrality, connectivity, cycle detection) for advisory use within the Sugiyama layout. Capabilities now in production:

- **Stable-ID projection** of `MermaidDiagramIr` into an FNX graph and projection-policy controls (directed vs undirected).
- **Centrality-aware semantic styling** — hub nodes receive CSS classes (`fm-node-centrality-high`, etc.) for visual emphasis. Centrality tiers also act as tie-breakers inside barycenter ordering.
- **FNX edge criticality scoring** for cycle removal.
- **Structural diagnostics** integrated into `fm-cli validate`: hub detection, bridge detection, disconnected component warnings, cycle recommendations.
- **Deterministic FNX analysis cache** keyed on graph fingerprint (and `fnx_enabled`), with budget enforcement and a fallback ladder.
- **FNX witness metadata** in CLI JSON output and the WASM API, so every render carries the analysis used.
- Differential quality/performance reporting comparing FNX-on vs FNX-off baselines.

### Phase 2 — directed (canary rollout)

Phase 2 extends FNX to directed graph semantics. The algorithms are implemented natively in `fm-layout/src/fnx_directed.rs` so we don't have to wait for upstream fnx directed APIs:

- **Strongly Connected Components (SCC)** via Tarjan with deterministic ordering.
- **Weakly Connected Components (WCC)** via BFS.
- **Directed cycle detection** via DFS with back-edge tracking.
- **Reachability analysis** with source/sink identification.

A canary rollout state machine (`fm-core/src/canary.rs`) drives Phase-2 enablement through a `RolloutPhase` enum: `Disabled` → `Canary` (≈1% traffic) → `Partial` (10–50%) → `Full`, with a `RolledBack` sink state. The conceptual progression documented in [`docs/FNX_PHASE2_ROLLOUT.md`](docs/FNX_PHASE2_ROLLOUT.md) maps these traffic phases onto three policy modes — Shadow (results logged, not used), Advisory (used as hints), Full Integration — with go/no-go gates for determinism, correctness, performance budget (≤100 ms for 100-node graphs), and pipeline parity.

### CLI controls

| Flag | Values | Description |
|---|---|---|
| `--fnx-mode` | `auto` (default) / `enabled` / `disabled` | Auto-detects feature availability; `enabled` errors if FNX is unavailable; `disabled` forces native-only |
| `--fnx-projection` | `undirected` (default) / `directed` | Graph projection for analysis algorithms |
| `--fnx-fallback` | `graceful` (default) / `strict` | Continue with native engine on FNX failure, or fail the command |

### Documentation

- [`docs/FNX_INTEGRATION.md`](docs/FNX_INTEGRATION.md) — Authoritative architecture contract, dependency model, feature topology, success metrics
- [`docs/FNX_USER_GUIDE.md`](docs/FNX_USER_GUIDE.md) — When to use FNX, command-line examples, troubleshooting
- [`docs/FNX_MIGRATION.md`](docs/FNX_MIGRATION.md) — Risk-tiered adoption guide with validation checklists
- [`docs/FNX_COMPATIBILITY_MATRIX.md`](docs/FNX_COMPATIBILITY_MATRIX.md) — Phase-by-phase required capabilities and availability
- [`docs/FNX_PHASE2_ROLLOUT.md`](docs/FNX_PHASE2_ROLLOUT.md) — Directed rollout policy, gates, and rollback triggers

## How SVG rendering works

The SVG renderer turns a `DiagramLayout` into a complete SVG document with visual polish that goes well beyond rectangles and lines.

### Node shape library (23)

| Shape | Syntax | Visual |
|---|---|---|
| Rectangle | `A[text]` | Standard box |
| Rounded | `A(text)` | Rounded corners |
| Stadium | `A([text])` | Pill (fully rounded ends) |
| Subroutine | `A[[text]]` | Double-bordered box |
| Diamond | `A{text}` | Rotated square |
| Hexagon | `A{{text}}` | Six-sided polygon |
| Circle | `A((text))` | Circular |
| Filled Circle | | Solid disc (mindmap / state markers) |
| Double Circle | `A(((text)))` | Concentric circles |
| Asymmetric | `A>text]` | Flag shape |
| Cylinder | `A[(text)]` | Database icon |
| Trapezoid | `A[/text\]` | Wider top |
| Inverse Trapezoid | `A[\text/]` | Wider bottom |
| Parallelogram | `A[/text/]` | Slanted |
| Inverse Parallelogram | `A[\text\]` | Reverse slant |
| Triangle | | Three-sided |
| Pentagon | | Five-sided |
| Star | | Five-pointed star |
| Cloud | `)text(` | Mindmap cloud |
| Tag | | Bookmark shape |
| Crossed Circle | | Circle with X |
| Note | | Folded-corner rectangle |
| Horizontal Bar | | Divider / separator |

Node icons are extractable from `{ icon: "..." }` metadata and from `::icon(name)` directives on mindmaps; custom SVG icons can be supplied via `SvgRenderConfig::custom_icons: BTreeMap<String, CustomSvgIcon>` (keyed by icon name).

### Visual effects

- **Gradients** — three styles defined as reusable SVG `<defs>`: linear vertical (3 stops), linear horizontal (3 stops), and radial (center-weighted, 0.8 radius).
- **Drop shadows** — SVG `<filter>` with configurable offset (default 2px), blur radius (default 6px), opacity (default 0.15), and theme-aware color.
- **Glow effects** — colored blur behind highlighted elements (blur radius 6px, opacity 0.35).
- **Cluster backgrounds** — semi-transparent filled rectangles (default opacity 0.08) with a 10px rounded corner radius and the cluster title above.
- **CSS-only animations** — entrance, flow, pulse, and hover effects. No JavaScript required.

### Arrowheads

The renderer ships with a full library of SVG `<marker>` definitions matching the parsed arrow types. Coverage includes solid/dotted/thick variants, open/closed/half/stick arrowheads (top + bottom + reverse + dotted), cross terminators, dotted cross, and the special `Central` connection used by mindmap and architecture diagrams.

### Theme system (10 presets)

| Theme | Character |
|---|---|
| Default | Clean light background with blue accents |
| Dark | Dark background with bright node fills |
| Forest | Green-tinted organic palette |
| Neutral | Grayscale with minimal color |
| Corporate | Professional blue/gray tones |
| Neon | Dark background with vivid accent colors |
| Pastel | Soft muted colors |
| HighContrast | Maximum readability, WCAG compliant |
| Monochrome | Pure black and white |
| Blueprint | Technical drawing style on blue background |

Each theme exposes 15 base CSS custom properties (7 named — `--fm-bg`, `--fm-text-color`, `--fm-node-fill`, `--fm-node-stroke`, `--fm-edge-color`, `--fm-cluster-fill`, `--fm-cluster-stroke` — plus `--fm-accent-1` through `--fm-accent-8`). The renderer then derives ~half a dozen more (`--fm-edge-muted`, `--fm-cluster-label-color`, `--fm-edge-label-bg`, `--fm-node-accent`, `--fm-node-hover-accent`, animation knobs) via `var()` references, giving ~21 custom properties in the final `<style>` block. Mermaid-style `%%{init}%%` `themeVariables` (`primaryColor`, `lineColor`, `clusterBkg`, etc.) are mapped onto the base properties automatically.

### Accessibility

The SVG renderer includes built-in accessibility features:

- `<title>` and `<desc>` elements on the root `<svg>` for screen readers.
- `accTitle` and `accDescr` directives parsed from the source and propagated to the SVG envelope.
- ARIA labels on node and edge groups.
- `describe_diagram()`, `describe_node()`, and `describe_edge()` functions emit human-readable descriptions (also exposed in the WASM API as `describeDiagram`).
- Print-optimized CSS rules accessible via `accessibility_css()`.

### Source spans and source maps

When `--embed-source-spans` is passed (or `accessibility = true` in config), every SVG element is annotated with a `data-fm-source-span` attribute linking back to its source line and column. A separate JSON source-map artifact (`--source-map-out`) maps SVG element IDs to input spans for editor click-to-source tooling.

### `classDef` / `style` / `linkStyle`

`classDef`, `style`, and `linkStyle` directives are parsed into structured `IrStyle` references and applied during SVG rendering. Style values pass through a sanitizer that strips disallowed properties (e.g., everything except a whitelist of safe CSS), is case-insensitive to `javascript:` schemes, and handles comment-obfuscated payloads.

## How terminal rendering works

The terminal renderer produces diagrams as text using Unicode box-drawing and sub-cell pixel rendering. It's designed for CI logs, SSH sessions, and quick previews without leaving the terminal.

### Sub-cell rendering modes

| Mode | Resolution | Characters | Best for |
|---|---|---|---|
| **Auto** (default) | (varies) | (varies) | Pick the highest mode the detected terminal supports |
| **Braille** | 2×4 per cell | U+2800–U+28FF (256 patterns) | Highest resolution, smooth curves |
| **Block** | 2×2 per cell | Quarter blocks U+2596–U+259F | Balance of detail and compatibility |
| **HalfBlock** | 1×2 per cell | Half blocks ▀ ▄ █ | Wide terminal compatibility |
| **CellOnly** | 1×1 per cell | Full block █ or space | Maximum compatibility, lowest resolution |

`Auto` is the default — it inspects environment variables (`TERM`, `LC_ALL`, `LANG`) and a small probe of supported code points to pick the highest fidelity mode the terminal can actually render, and degrades to `CellOnly` for environments where even half-blocks are unsafe.

Braille mode encodes an 8-dot pattern per cell where each dot maps to a sub-pixel. The renderer draws into a boolean pixel buffer using Bresenham's line algorithm and the midpoint circle algorithm, then encodes 8-pixel blocks into single braille code points starting at U+2800.

### Rendering tiers

| Tier | Node style | Edge style | Labels |
|---|---|---|---|
| **Compact** | Single character or small box | Minimal line segments | Abbreviated |
| **Normal** | Box-drawn rectangles with labels | Box-drawing characters (─ │ ┌ ┐ └ ┘ ├ ┤) | Full text |
| **Rich** | Decorated boxes with shape hints | Styled edges with arrowheads (→ ← ↑ ↓) | Full text with wrapping |

### Diff engine

```bash
fm-cli diff before.mmd after.mmd --format terminal
```

The diff engine tracks element-level changes:

- **Nodes** — Added, Removed, Changed (label, shape, classes, members), Unchanged
- **Edges** — Added, Removed, Changed (arrow type, label), Unchanged

Output shows a side-by-side comparison with color-coded change markers plus aggregate counts (`3 added, 1 removed, 2 changed, 15 unchanged`). ANSI is automatically suppressed when writing to a file or when `--color never` is set.

### Minimap

For diagrams that exceed the terminal viewport, the renderer can produce a scaled minimap — a compressed overview showing the overall structure with a viewport indicator. Detail level is auto-selected based on density classification (sparse / medium / dense).

## Canvas2D web rendering

The Canvas2D renderer is an alternative to SVG for browser-based rendering, particularly suited for large diagrams and interactive use.

### Trait-based abstraction

`Canvas2dContext` is a trait with 40 methods covering path operations (`begin_path`, `move_to`, `line_to`, `quadratic_curve_to`, `bezier_curve_to`, `arc`, `arc_to`, `rect`, …), drawing (`fill`, `stroke`, `fill_rect`, `stroke_rect`, `clear_rect`), text (`fill_text`, `stroke_text`, `measure_text`, plus alignment / baseline setters), style (fill / stroke / line-width / line-cap / line-join / line-dash / global-alpha / font), transform (`save`, `restore`, `translate`, `scale`, `rotate`, `set_transform`, `reset_transform`, `clip`), and shadows.

In WASM builds, the trait is implemented against `web_sys::CanvasRenderingContext2d` with font-size-aware text metrics. For testing, a `MockCanvas2dContext` records all draw operations into a `Vec<DrawOperation>` so the full render pipeline can be tested in CI without a browser.

### Viewport transform

Automatic fit-to-container scaling: `scale = min(container_width / diagram_width, container_height / diagram_height)`, clamped to never zoom beyond 100% for small diagrams. The diagram is centered within the available space, and pan/zoom uses point-preserving zoom (zooming toward the cursor rather than the origin).

## The intermediate representation

`MermaidDiagramIr` is the central data structure that connects parsing → layout → rendering.

```rust
MermaidDiagramIr {
    diagram_type: DiagramType,          // 24 types + Unknown
    direction: GraphDirection,          // TB, LR, RL, BT
    nodes: Vec<IrNode>,                 // id, label, shape, icon, classes, href, callback,
                                        //   tooltip, span_primary/span_all, implicit,
                                        //   members (ER), class_meta, requirement_meta,
                                        //   c4_meta, inline_style
    edges: Vec<IrEdge>,                 // from/to: IrEndpoint, arrow, label, span,
                                        //   er_notation, source/target_cardinality,
                                        //   guard, action, inline_style
    ports: Vec<IrPort>,                 // ER entity attributes (PK/FK/UK)
    clusters: Vec<IrCluster>,           // Visual grouping containers
    graph: MermaidGraphIr,              // Indexed adjacency view, including IrGraphEdge
                                        //   entries that carry IrEdgeKind (Generic /
                                        //   Relationship / Message / Timeline /
                                        //   Dependency / Commit)
    labels: Vec<IrLabel>,               // Interned text (referenced by IrLabelId)
    subgraphs: Vec<IrSubgraph>,         // Hierarchical nesting
    constraints: Vec<IrConstraint>,     // Layout hints (same-rank, min-length)
    styles: Vec<IrStyle>,               // classDef/style/linkStyle (structured)
    meta: MermaidDiagramMeta,           // Config, parse mode, theme overrides, title
    diagnostics: Vec<Diagnostic>,       // Warnings/errors with source spans
}
```

### Key design decisions

- **Label interning** avoids string duplication and centralizes normalization/wrapping.
- **Span tracking** — every node, edge, label, and cluster carries a `Span` with byte offset, line, and column.
- **Implicit nodes** — `A --> B` is accepted without explicit declarations; auto-created nodes carry `implicit: true`.
- **Semantic edge kinds** — beyond just the arrow type, the indexed graph view (`MermaidGraphIr::edges` → `IrGraphEdge.kind: IrEdgeKind`) encodes diagram-specific semantics (`Generic`, `Relationship` for ER, `Message` for sequence, `Timeline`, `Dependency` for gantt, `Commit` for gitGraph). Renderers and layout dispatchers consult the graph view when they need that semantic distinction; the flat `edges: Vec<IrEdge>` list carries the surface-syntax info (arrow type, label, cardinality, guard, action, inline style).
- **Structured styles** — `classDef`/`style`/`linkStyle` are stored as typed `IrStyle` records with sanitized values.

### Diagnostics

```rust
Diagnostic {
    severity: Hint | Info | Warning | Error,
    category: Lexer | Parser | Semantic | Recovery | Inference | Compatibility,
    message: String,
    span: Option<Span>,
    suggestion: Option<String>,
    expected: Vec<String>,                   // For parse errors
    found: Option<String>,
    related: Vec<RelatedDiagnostic>,         // Multi-location context
}
```

A `StructuredDiagnostic` JSON form (with `error_code`, `rule_id`, `confidence`, `remediation_hint`) is emitted by `fm-cli validate --format json` for automation.

## DOT format bridge

The DOT parser (`fm-parser/src/dot_parser.rs`) enables Graphviz interop by converting DOT syntax to the shared Mermaid IR.

### Supported DOT features

| Feature | DOT syntax | IR mapping |
|---|---|---|
| Directed graph | `digraph G { ... }` | `DiagramType::Flowchart`, `ArrowType::Arrow` |
| Undirected graph | `graph G { ... }` | `DiagramType::Flowchart`, `ArrowType::Line` |
| Node declaration | `node_id [label="text"]` | `IrNode` with label |
| Edge declaration | `A -> B -> C` | Two `IrEdge` entries (chaining supported) |
| Subgraph | `subgraph cluster_X { ... }` | `IrSubgraph` + `IrCluster` |
| Anonymous subgraph | `{ A B }` | Cluster with auto-generated ID |
| Attribute lists | `[label="...", shape=box]` | Label extracted, attributes as classes |
| HTML labels | `[label=<b>bold</b>]` | HTML stripped, text preserved |
| Comments | `// line` and `/* block */` | Stripped during pre-processing |
| Escape sequences | `\n`, `\t`, `\"`, `\\` | Decoded in string values |
| Edge groups with quoted IDs | `"foo" -> "bar"` | Correctly handled (single + double quotes, brace-balanced) |

The parser is hardened against several adversarial patterns (case-insensitive header keywords, comment-obfuscated headers, brace adjacency, symbol-only identifiers).

## Diagram-family parser deep dives

### ER diagram — 14 cardinality operators

The ER parser recognizes 14 distinct cardinality operators, each encoding a specific relationship type:

| Operator | Meaning | Line style |
|---|---|---|
| `\|\|--o{` | One-to-many (optional many) | Solid |
| `\|\|--\|{` | One-to-many (required many) | Solid |
| `}\|--\|\|` | Many-to-one (required) | Solid |
| `}o--\|\|` | Many-to-one (optional many) | Solid |
| `\|o--o\|` | One-to-one (both optional) | Solid |
| `\|\|--\|\|` | One-to-one (both required) | Solid |
| `o\|--\|{` | Optional-one to many | Solid |
| `}\|--\|{` | Many-to-many | Solid |
| `o\|--\|\|` | Optional-one to one | Solid |
| `}\|..\|{` | Many-to-many | Dotted |
| `\|\|..\|\|` | One-to-one | Dotted |
| `o\|..\|{` | Optional-one to many | Dotted |
| `\|o..\|{` | One-optional to many | Dotted |
| `}o--o{` | Many-optional to many-optional | Solid |

Each entity attribute is parsed into an `IrPort` with `is_pk` / `is_fk` / `is_uk` flags and the relationship label becomes the edge label.

### GitGraph — stateful branch tracking

The gitGraph parser maintains a `GitGraphState` struct that tracks:

- **Branch heads** — `BTreeMap<String, IrNodeId>` mapping branch names to their current head commit.
- **Current branch** — defaults to `main`, changes with `checkout` / `switch`.
- **Commit counter** — auto-increments to generate IDs (`commit_1`, `commit_2`, …).

Each `commit` statement creates a node on the current branch and an edge from the previous commit. `branch` creates a new branch pointing at the current `HEAD`. `merge` creates a commit with two parent edges. `cherry-pick` creates a commit with an edge from the specified source commit. Direction tokens with trailing punctuation (`LR;`) are accepted via a hardening fix.

### Mindmap — indentation-based hierarchy

The mindmap parser uses indentation depth to determine parent-child relationships:

1. Count leading spaces for each line to determine depth.
2. Maintain an ancestry stack indexed by depth.
3. Each node's parent is the nearest ancestor at `depth - 1`.
4. Shape is determined by bracket syntax: `[text]` = rect, `(text)` = rounded, `((text))` = circle, `{{text}}` = hexagon, `)text(` = cloud, `))text((` = bang/filled-circle.
5. `::icon(name)` directives attach icon metadata to the preceding node.

The radial layout then allocates an angular range to each child proportional to its leaf-descendant count, so visually heavy subtrees get more screen real estate.

### Block-beta — column spanning

The block-beta parser supports CSS-grid-like column spanning:

```
block-beta
  columns 3
  A["Wide Block"]:2    %% spans 2 columns
  B["Normal"]          %% spans 1 column
  space                %% empty cell
  C["Full Width"]:3    %% spans all 3 columns
  block:nested
    X
    Y
  end
```

The `:N` suffix sets `grid_span = N` on the node. `space` and `space:N` create empty cells. `block:id … end` creates a sub-grid within the parent grid. The grid layout computes block widths as `base_width × N + spacing × (N - 1)`, effectively merging N adjacent cells.

### Class diagram — generics, namespaces, cardinality

The class parser handles:

- **Generic type parameters** — `class List~T~` and `class Map~K,V~` produce a `generics: ["T"]` field on the node, which the SVG renderer formats as `List<T>`.
- **Three-compartment box rendering** — class name (with stereotype `<<interface>>` etc.), attributes (with `+` / `-` / `#` / `~` visibility prefixes), and methods (with parameter types and return types) get their own horizontal stripes inside the node.
- **Inheritance, composition, aggregation, dependency, realization** — each with a distinct arrowhead (`<|--`, `*--`, `o--`, `..>`, `..|>`).
- **Cardinality labels** — `Customer "1" -- "*" Order : places` produces multiplicity badges at each endpoint.
- **Namespace blocks** — `namespace Foo { class A; class B }` groups classes into a cluster.

### State diagram — composites, pseudo-states, notes

The state parser handles:

- **Composite states** — `state Outer { ... }` produces a cluster containing the inner state graph.
- **Pseudo-states** — `[*]` (start/end), `<<fork>>`, `<<join>>`, `<<choice>>` are each tagged in IR and rendered with distinct shapes (filled circle, vertical bar, diamond).
- **History states** — `[H]` and `[H*]` (shallow vs deep).
- **State notes** — `note left of S: text` and `note right of S: text` produce attached note nodes that the layout engine positions adjacent to the state without affecting rank assignment.
- **Transition guards and actions** — `A --> B : guard / action` parses both into edge metadata.

### Sequence diagram — fragments, notes, lifecycle

The sequence parser is the most syntactically dense and supports:

- **Participants** with optional aliases, `actor` vs `participant`, and **participant groups** (with color support).
- **Messages** — `->>`, `-->>`, `-)`, `-x`, with sync / async / dotted / cross variants.
- **Activations** — `activate Foo` / `deactivate Foo` produce activation bars on the lifeline.
- **Notes** — `Note left of Foo`, `Note right of Foo`, `Note over Foo,Bar` render as rounded-corner boxes near the relevant lifelines.
- **Interaction fragments** — `alt` / `opt` / `loop` / `par` / `critical` / `break`, each rendering as a dashed-border rectangle with a `kind` tab. Fragments can nest, and `else` separators inside `alt`/`par` produce labeled internal sections.
- **Lifecycle events** — `create participant` and `destroy Foo` mark participant lifecycle on the lifeline. Multiple destroy markers are coalesced.

### Gantt — sections, dependencies, task types

The gantt parser handles:

- **Date/duration parsing** — absolute dates (`2026-01-15`) and durations (`5d`, `2w`).
- **Section grouping** — `section Backend` creates a swimlane that groups subsequent tasks.
- **Dependencies** — `after taskA`, `after taskA, taskB` produces edges into the dependency graph.
- **Task types** — `done`, `active`, `crit`, `milestone` change the visual styling (color, border, marker).
- **Calendar validation** — invalid dates are caught early and surfaced as parse diagnostics rather than producing a malformed layout.

The gantt layout dispatcher emits a time-axis bar layout where each task's `x` is determined by its start date and width by its duration, with sections producing horizontal bands.

### Kanban — metadata

The kanban parser accepts metadata blocks on cards:

```
kanban
  Todo
    Task 1@{wip: 3, priority: high, assigned: alice}
  Doing
    Task 2@{assigned: bob}
```

`wip` triggers warning styling on the column when exceeded, `priority` maps to color, and `assigned` becomes a badge on the card.

### Sankey — flow-weighted column layout

The sankey parser produces an IR where edges carry numeric values (flow weights). The dedicated sankey layout:

1. Assigns nodes to columns by reachability from source nodes (in-degree 0).
2. Scales node heights proportional to their total flow: `30 + max(in_degree, out_degree) × 14.0` px.
3. Spaces columns by `rank_spacing + 136px` (extra margin for flow band rendering).
4. Iteratively relaxes within-column ordering to minimize flow band crossings.

### Quadrant, Pie, XyChart — chart-style layouts

| Type | Parser features | Layout |
|---|---|---|
| `quadrantChart` | Axis labels, four quadrant labels, data points with `[0,1]` coords (e.g., `[0.3, 0.7]`) | 2D scatter on normalized axes with central cross |
| `pie` | Slice values, title, `showData` toggle | Wedge angle computation + perimeter label anchoring with collision avoidance |
| `xyChart` | Axis configuration (`x-axis "Quarter" [Q1, Q2, Q3, Q4]`), series declarations (`bar` / `line` / `area`) with named series | Cartesian coordinate mapping with category padding and per-series rendering |

---

## The render scene IR

Between layout and the final render backends (SVG, terminal, Canvas2D) there is an intermediate **render scene** that abstracts away backend specifics. This lets new render targets be added without touching layout code.

### Scene structure

```
RenderScene
├── bounds: RenderRect
└── root: RenderGroup (id = "diagram-root")
    ├── transform: identity matrix (6-component affine [a, b, c, d, e, f])
    ├── clip:      RenderClip::Rect(bounds)
    └── children:  Vec<RenderItem>
        ├── Cluster layer  (backgrounds + titles)
        ├── Edge layer     (paths with arrowheads)
        ├── Node layer     (shapes with fills/strokes)
        └── Label layer    (text elements with font metrics)
```

Each `RenderItem` is one of:

- **`Group`** — container with optional transform and clip region. Transforms compose through the CGA rotor stack so successive scales and rotations remain numerically stable.
- **`Path`** — SVG-style path commands (`MoveTo`, `LineTo`, `BezierTo`, `ArcTo`, `Close`) with fill / stroke / dash / line-cap / line-join.
- **`Text`** — positioned text with font metrics, alignment, and optional rotation.

Every render item carries a `RenderSource` tag indicating what it represents (`Node`, `Edge`, `Cluster`, `Label`, `Decoration`), enabling backends to apply type-specific styling without parsing the geometry. This is how the SVG renderer applies class names like `fm-edge` vs `fm-cluster-bg` without inspecting paint properties.

---

## Font metrics and text measurement

The engine doesn't have access to a browser font renderer at layout time, so it uses a heuristic character-width model.

### Width classes

| Class | Multiplier | Characters |
|---|---|---|
| Very Narrow | 0.4× | `i l ! ' . , : ;` etc. |
| Narrow | 0.6× | `I j t f r ( ) [ ]` |
| Half | 0.5× | space |
| Normal | 1.0× | Most characters |
| Wide | 1.2× | `w m` |
| Very Wide | 1.5× | `W M @ % &` |

### Font family presets

| Family | Avg char ratio | Used when |
|---|---|---|
| System UI / Sans-Serif | 0.55 | Default |
| Monospace | 0.60 | Code labels |
| Serif | 0.52 | Document-style diagrams |
| Condensed | 0.45 | Dense layouts |

CJK and emoji characters are full-width-aware (east-asian width is correctly classified).

### Text wrapping and truncation

Greedy word-fit wrapping by default; when a word is wider than the target width it's placed on its own line. Truncation falls back to character-by-character measurement until the remaining text plus an ellipsis fits the target width.

## Node sizing model

```
node_width  = max(label_width + 72.0, 100.0)
node_height = max(label_height + 44.0, 52.0)
```

72px horizontal padding (36px per side) and 44px vertical padding (22px per side) give labels breathing room. Minimums (100×52) ensure even empty or single-character nodes are visually meaningful and clickable. Shape does not affect the bounding box; a diamond and a rectangle with the same label get the same allocated space.

### Spacing constants

| Constant | Default | Purpose |
|---|---|---|
| `node_spacing` | 80 px | Horizontal gap between adjacent nodes in the same rank |
| `rank_spacing` | 120 px | Vertical gap between ranks |
| `cluster_padding` | 52 px | Padding inside cluster/subgraph boundaries (all 4 sides) |

## Security model

frankenmermaid processes untrusted input (user-provided diagram text) and produces output that may be embedded in web pages (SVG). The security model addresses injection at multiple layers.

### XML/SVG injection

All text content passes through escape functions before being embedded in SVG:

| Context | Escapes | Why |
|---|---|---|
| XML attributes | `& < > " '` → entities | Prevents attribute breakout |
| XML text | `& <` → entities | Prevents element injection. `>` is intentionally NOT escaped to preserve CSS child combinators in embedded stylesheets |
| CSS tokens | Strip everything except `[a-z0-9_-]` | Prevents CSS injection via class names |

SVG elements are constructed programmatically (not string-concatenated), so there is no path for injecting arbitrary SVG elements through diagram input.

### Link sanitization

Links are disabled by default (`enable_links = false`). When enabled:

| `MermaidSanitizeMode` | Behavior |
|---|---|
| `Strict` (default) | URL scheme validation. `javascript:`, `vbscript:`, `data:`, `file:`, `blob:` blocked. Only `http:`, `https:`, and relative URLs allowed |
| `Lenient` | All URL schemes permitted (use only in trusted environments) |

| `MermaidLinkMode` | Effect |
|---|---|
| `Off` | No links rendered regardless of `enable_links` |
| `Inline` | Render links directly as clickable SVG anchors |
| `Footnote` | Emit `data-link` metadata for external tooling instead of anchors |

Case-insensitive `javascript:` detection (catching `JaVaScRiPt:` and CSS comment obfuscation) is part of the sanitizer test corpus.

### Input limits

`MermaidConfig` enforces input size limits to prevent denial-of-service via pathological diagrams:

| Limit | Default | Effect when exceeded |
|---|---|---|
| `max_nodes` | 200 | Degradation warning, reduced visual fidelity |
| `max_edges` | 400 | Degradation warning, simplified edge routing |
| `max_label_chars` | 48 | Labels truncated with `…` |
| `max_label_lines` | 3 | Multi-line labels capped |
| `max_input_bytes` | 5,000,000 | Parse refused; enforced via bounded reads on stdin and files |
| `route_budget` | 4,000 | Routing simplified once exceeded |
| `layout_iteration_budget` | 200 | Refinement phases skipped once exceeded |

FxHash collision resistance is verified by a dedicated DoS-resistance test corpus.

## Determinism guarantees

Deterministic output is an explicit design goal. The concrete engineering choices:

### Ordered data structures

Layout-critical and parser code uses `BTreeMap` / `BTreeSet` exclusively (`fm-layout` and `fm-parser` contain zero `HashMap`/`HashSet` usage). Where hash-keyed lookup is genuinely needed elsewhere, `fm-core` exposes `NodeMap` / `NodeSet` / `EdgeMap` type aliases backed by `FxHashMap` / `FxHashSet` from `rustc-hash` — the FxHash hasher is deterministic with no random seed, so iteration order within a single process is stable across runs given the same insertion order. Standard-library `HashMap`/`HashSet` with the default `RandomState` are never used in the IR, layout, or render paths.

### Stable node ordering

Before any layout phase that depends on node order, nodes are sorted by a stable priority function:

```
Primary:   node ID (string comparison)
Secondary: node index (declaration order)
```

### Floating-point discipline

IEEE 754 arithmetic is deterministic for identical inputs on the same platform. The codebase avoids operations that could introduce platform-dependent results:

- No `f32::sin`/`cos` in layout-critical paths.
- Explicit drift correction after allocating angular spans in radial layout.
- Epsilon-based comparisons (`0.001`) for collinearity tests, never exact float equality.

### Cache key discipline

The FNX analysis cache key includes `fnx_enabled`, so a cached layout produced with FNX-on cannot leak across to an FNX-off render.

### Verification

```rust
#[test]
fn traced_layout_is_deterministic() {
    let ir = sample_ir();
    let first = layout_diagram_traced(&ir);
    let second = layout_diagram_traced(&ir);
    assert_eq!(first, second); // Bit-for-bit equality
}
```

Property-based tests verify determinism across random graph shapes (up to 20 nodes × 5 directions per run, with proptest case count configurable via `.ci/quality-gates.toml`).

## Performance and scaling

The engine is designed for diagrams in the 1–500 node range (typical documentation diagrams), with graceful degradation up to 10,000+ nodes via the guardrail fallback chain. Approximate per-phase scaling:

| Phase | Complexity | 10 nodes | 100 nodes | 1,000 nodes |
|---|---|---|---|---|
| Parsing | O(n) | <1 ms | <1 ms | ~5 ms |
| Cycle removal | O(V+E) | <1 ms | <1 ms | ~2 ms |
| Rank assignment | O(V+E) | <1 ms | <1 ms | ~3 ms |
| Crossing minimization | O(E × sweeps) | <1 ms | ~5 ms | ~200 ms |
| Coordinate assignment (Brandes-Köpf) | O(V) | <1 ms | <1 ms | ~1 ms |
| Edge routing | O(E) | <1 ms | <1 ms | ~5 ms |
| SVG rendering | O(V+E) | <1 ms | ~2 ms | ~15 ms |
| **Total** | | **<5 ms** | **~10 ms** | **~230 ms** |

For very large diagrams, the force-directed layout with Barnes-Hut optimization (n > 100, O(n log n)) is often a better choice than Sugiyama, whose crossing minimization dominates for very dense graphs.

Criterion benchmarks in `crates/fm-layout/benches/` cover crossing minimization (E-graph vs greedy) and incremental layout. CI tracks regressions via a benchmark regression harness (`bd-ml2r.11.3`) with determinism replay and configurable warn/fail thresholds in `.ci/quality-gates.toml`.

## Pressure-adaptive runtime

The pressure-adaptive runtime (`bd-3uz` epic) treats parse / layout / render as cooperating stages that share a global compute budget rather than as a fixed-cost pipeline. The runtime owns:

- A **cross-stage global budget broker** that allocates time and operation counts across parse, layout, and render. A single stage can't starve the others; if parser recovery has already burned half the budget, the layout dispatcher knows it must pick something cheaper.
- A **capability claim matrix** (`fm-cli capabilities --pretty`) — an executable contract describing which surfaces are implemented, partial, or planned, with evidence references. The matrix is consumed by both CI release-signoff and the runtime dispatcher (an algorithm declared `partial` for diagram type X won't be auto-selected).
- A **MermaidGuardReport** + **MermaidDegradationPlan** pair that describes how to gracefully reduce visual fidelity under pressure. Example degradation operators include: disable drop shadows, simplify edge routing to straight lines, drop minor labels, downgrade terminal tier from `rich` to `compact`, switch SVG gradients off, fall back from Sugiyama to Tree.
- A **deterministic degradation operator algebra** so that the same pressure level produces the same degradation plan across runs.
- A **strict / compat / recover parser support contract** (`--parse-mode`) with deterministic fallback semantics: `strict` rejects anything not perfectly mermaid-js-compatible, `compat` is the default best-effort mode, `recover` allows the most aggressive recovery including dangling-edge placeholder creation and fuzzy keyword acceptance.

### MermaidLayoutDecisionLedger

Every layout pass emits a `MermaidLayoutDecisionLedger` — a sequence of `MermaidLayoutDecisionRecord` entries capturing every decision the engine made. The CLI surfaces this via `fm-cli render --verbose` and as line-delimited JSON through `MermaidLayoutDecisionLedger::to_jsonl()`. Each record carries:

| Field | Meaning |
|---|---|
| `kind` | Record kind (`"dispatch"`, `"guard"`, `"degradation"`, …) |
| `trace_id` / `decision_id` / `policy_id` | Stable observability IDs (from `franken-kernel`) tying this record to the broader trace |
| `schema_version` | Semver for the ledger format itself — allows downstream consumers to detect breaking changes |
| `requested_algorithm` | What was originally asked for (CLI flag / config / `Auto`) |
| `selected_algorithm` | What actually ran |
| `capability_unavailable` | `true` when the requested algorithm was rejected because the capability claim matrix marks it `partial` / `unavailable` for this diagram type |
| `decision_mode` | Which dispatch mode produced the choice (`"auto"`, `"explicit"`, `"fallback"`, …) |
| `dispatch_reason` | Plain-English reason for the dispatch (e.g., `"diagram_type_dispatch"`) |
| `guard_reason` | Plain-English reason if a guardrail fired |
| `fallback_applied` | Did the guardrail fallback chain trigger? |
| `confidence_permille` | Dispatcher confidence in parts-per-thousand (0–1000) |
| `selected_expected_loss_permille` | Expected-loss estimate for the selected algorithm |
| `node_count` / `edge_count` / `crossing_count` / `reversed_edges` | Realized graph and quality metrics |
| `estimated_layout_time_ms` / `estimated_layout_iterations` / `estimated_route_ops` | Cost estimates that fed the dispatch decision |
| `pressure_source` / `pressure_tier` | Where the runtime pressure came from and how severe it is |
| `budget_total_ms` / `budget_exhausted` | Global-budget-broker state at the time of the decision |
| `state_posterior` / `expected_loss` | Per-algorithm probability and loss weightings (Bayesian-flavored decision math) |
| `alternatives` | The runner-up algorithms with their own loss estimates |
| `notes` | Free-form diagnostic notes |

A companion `MermaidLayoutDecisionExplanation` type exposes the same record at three escalating detail tiers, so a CLI consumer can pick whichever verbosity makes sense for the situation: a `level_0_traffic_light` summary, a `level_1_plain_english` paragraph, and a `level_2_constraint_checks` block listing each individual constraint and whether it passed.

The ledger is structured and deterministic modulo wall-clock fields, so it can be diffed across commits to detect changes in algorithm choice or quality metrics. The `decision_contract` CI gate does exactly that.

---

## Evidence and release signoff

Every render emits enough structured evidence to be auditable in CI:

- **Capability claim matrix** — `fm-cli capabilities --pretty` (also embedded at the top of this README via generated comment markers).
- **Layout decision ledger** — describes algorithm selection, cycle strategy, refinement plan, guardrail fallbacks, and FNX witness metadata.
- **Evidence bundles** — `fm-cli` emits evidence with timestamps tying together input hash, parse/layout/render times, node/edge counts, layout bounds, output artifact hash, and pass/fail reason.
- **Evidence binary** — a separate `evidence` binary handles structured pass/fail evidence persistence for CI consumption.
- **Release signoff** — CI gate aggregation pulls golden, property-based, invariant, performance-regression, determinism, decision-contract, degradation, and override-policy gates into a single signoff decision per release.
- **Demo evidence guard** — CI verifies the showcase HTML deploys cleanly and snapshots render evidence runs.

## Quality and testing

- **Unit tests** inline `#[cfg(test)]` across every component crate.
- **Integration tests** in `tests/` and `crates/*/tests/` (~24 dedicated test files) exercise the full parse → layout → render pipeline.
- **Golden SVG snapshots** with `BLESS=1 cargo test -p frankenmermaid-cli --test golden_svg_test` for regression safety. The harness emits structured JSON evidence per scenario (input hash, output artifact hash, dimensions, timings, degradation tier, pass/fail reason).
- **Golden layout checksums** verify byte-identical layout output across commits.
- **Property-based tests** (proptest) on parser totality, IR serde round-trip, layout determinism (random chain graphs × 5 directions), non-overlapping nodes, non-negative layout stats, SVG totality (always valid), and terminal bounds enforcement.
- **Conformance harness** — `frankentui_conformance_test.rs` drives ~26 fixture-backed cases against the FrankenTUI reference implementation, with coverage tracking in [`FEATURE_PARITY.md`](FEATURE_PARITY.md).
- **Regression harness** — `fm-regression-harness` ingests a real-world Mermaid corpus and emits an HTML thumbnail-grid report with per-case pass/fail and timing.
- **Fuzz targets** — cargo-fuzz harnesses for the parser and the full pipeline (in `fuzz/`).
- **Adversarial / DoS** — FxHash collision corpus, comment-obfuscated style sanitization, brace-adjacent DOT headers, symbol-only DOT identifiers, deep cyclic graphs (stack-overflow regression), E-graph memory-explosion fault tests.
- **Clippy pedantic + nursery** lints enabled workspace-wide with `-D warnings`.
- **Zero unsafe code** — `#![forbid(unsafe_code)]` enforced in every crate.

```bash
# Full quality gate
cargo check --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --check
cargo test --workspace
```

### Property-based invariants

The proptest harness verifies invariants that must hold for *all* inputs, not just hand-picked cases. Each test run generates 48–64 random inputs (configurable via `.ci/quality-gates.toml` `[property_test] cases`):

**Layout invariants**

- **Determinism** — for any random chain graph (1–20 nodes, any of 5 directions), `layout(graph) == layout(graph)`. Two runs produce bit-identical output.
- **Non-overlapping nodes** — no two nodes in the output have overlapping bounding boxes (within floating-point tolerance).
- **Non-negative stats** — `total_edge_length`, `reversed_edge_total_length`, `bounds.width`, `bounds.height` are all `>= 0` for any input.
- **Rank monotonicity** — under Sugiyama, every non-reversed edge has `target.rank > source.rank`.

**SVG render invariants**

- **Totality** — `render_svg(ir)` always produces valid SVG (starts with `<svg`, ends with `</svg>`) for any IR, including empty diagrams and IRs containing only diagnostics.
- **Count accuracy** — the SVG carries `data-nodes="N"` and `data-edges="M"` attributes that exactly match the IR's node and edge counts.
- **No bare `>` outside element delimiters** — all text/attribute content is properly entity-escaped.

**Terminal render invariants**

- **Bounds enforcement** — `render_term_with_config(ir, config, cols, rows)` always produces a `TermRenderResult` where `result.width <= cols` and `result.height <= rows`. The renderer scales down rather than overflow.

**Parser invariants**

- **Totality** — `parse(input)` never panics for any input string. Tested against random strings up to 256 characters including non-ASCII, control characters, and adversarial patterns.
- **Confidence bounds** — detection confidence is always in `[0.0, 1.0]`.
- **Serde round-trip** — `deserialize(serialize(ir)) == ir`. The IR survives JSON serialization and deserialization without data loss.
- **IR builder idempotence** — building the same IR from the same input twice produces equal IR structures.

### CI quality gates

The full CI matrix is driven from `.ci/quality-gates.toml`. Each gate can be enabled/disabled and configured per environment. The gates currently wired:

| Gate | What it verifies |
|---|---|
| `golden_checksum` | Golden SVG snapshots match (FNV-1a hash comparison) |
| `property_test` | Proptest invariants hold for N random cases |
| `invariant_proof` | Invariant-proof harness passes (`crates/fm-cli/tests/invariant_proof_harness.rs`) |
| `performance_regression` | Criterion benchmarks within `warn_threshold_pct` / `fail_threshold_pct` vs `baseline_path`; sample count and SLO file are also configurable |
| `determinism` | `cargo test ... determinism` passes (golden layout checksums + repeated-run equality) |
| `evidence_ledger` | Every render emits a structured evidence record |
| `decision_contract` | The layout decision ledger matches the contracted schema |
| `degradation` | Degradation operator algebra produces the expected reduced output under pressure |
| `release_gate_overrides` | Override-policy harness verifies that release-gate overrides are properly authorized |
| `release_signoff` | The release-signoff command aggregates all gate results into a single go/no-go decision |
| `demo_evidence` | Showcase HTML deploys cleanly and renders expected examples |

The aggregator surfaces a single pass/fail signal to the CI workflow per push, with individual gate results available as JSON artifacts.

## Validate pipeline

`fm-cli validate` runs four diagnostic collection stages — `parse`, `fnx`, `layout`, `render` — and produces a sorted, deduplicated report.

| Stage | What it checks |
|---|---|
| `parse` | Parser warnings, init directive errors, structured IR diagnostics, unstructured recovery warnings, unknown diagram type, empty diagram (no nodes and no edges) |
| `fnx` | Optional FNX structural diagnostics (hub detection, bridges, disconnected components, cycle scoring, structured recommendations) — only present when FNX is enabled |
| `layout` | Algorithm capability unavailable for the diagram type, guardrail fallback applied, cycles detected and edges reversed |
| `render` | SVG envelope validation (output starts with `<svg` and ends with `</svg>`) |

Diagnostics are sorted by six keys for consistent output (severity, source line, source column, stage, error code, message text). The `--fail-on` flag controls which severity level causes a non-zero exit code.

## Deployment and rollback

### Demo surfaces

1. **GitHub Pages** at <https://dicklesworthstone.github.io/frankenmermaid/> — deployed automatically on push to `main` via `.github/workflows/pages.yml`. Includes the showcase HTML, WASM binary, and `web/` + `web_react/` host pages.
2. **Cloudflare Pages** — deployed via the CI pipeline with staged smoke checks driven by `scripts/cloudflare_pages_ops.py`.

Both deployment paths include automated smoke checks (HTTP 200 on root page + WASM artifact, plus showcase render evidence). Showcase E2E lives in `scripts/showcase_harness.py` and `scripts/run_static_web_e2e.py`.

### Rollback

```bash
# GitHub Pages: re-run the workflow on a known-good commit
gh workflow run "Deploy GitHub Pages" --ref <known-good-sha>

# Or revert + push
git revert HEAD && git push origin main

# Cloudflare Pages: select a previous deployment in the dashboard (all versions retained)

# WASM bundle: rebuild from a known-good commit
git checkout <known-good-sha>
./build-wasm.sh
```

## Use cases and integration patterns

### CI snapshot testing for documentation diagrams

Deterministic output plus FNV-1a-hashed golden snapshots means diagram drift fails CI the same way any other regression does:

```bash
# In CI
fm-cli render docs/architecture.mmd --format svg --output /tmp/arch.svg
diff <(sha256sum docs/architecture.svg) <(sha256sum /tmp/arch.svg)
```

For richer reporting, the golden harness emits structured JSON evidence per scenario (input hash, output artifact hash, layout dimensions, parse/layout/render timings, degradation tier, pass/fail reason) so a single failing diagram surfaces the exact reason instead of a binary mismatch.

### Editor integrations

The WASM API, the lens system, and source-span attributes between them cover the pieces an editor needs: render, structural edit, click-to-source.

```ts
import { init, renderSvg, diagramLens, applyLensEdit } from '@frankenmermaid/core';
await init();

// Initial render with source spans
const svg = renderSvg(source, { embedSourceSpans: true });
container.innerHTML = svg;

// Click-to-source: SVG elements carry data-fm-source-span="line:col"
container.addEventListener('click', (e) => {
  const span = (e.target as Element).closest('[data-fm-source-span]')?.getAttribute('data-fm-source-span');
  if (span) editor.revealPosition(parsePosition(span));
});

// Structural edits via the lens — preserves formatting of unchanged regions
const lens = diagramLens(source);
const newSource = applyLensEdit(lens, { kind: 'rename-node', from: 'A', to: 'Start' });
editor.setValue(newSource);
```

### Static-site documentation pipelines

For Hugo/Jekyll/MkDocs/Docusaurus sites, generate SVGs at build time so the published site doesn't ship a JavaScript renderer:

```bash
find docs -name '*.mmd' -print0 | xargs -0 -P "$(nproc)" -I{} sh -c '
  fm-cli render "$1" --format svg --theme corporate --output "${1%.mmd}.svg"
' _ {}
```

The `--theme corporate` (or any built-in preset) and the SVG accessibility features mean the output works in dark mode, screen readers, and high-contrast environments without further processing.

### Dashboards and live status pages

Dashboards that re-render many large graphs (1,000+ nodes) on every tick benefit from the Canvas2D backend via WASM. The incremental layout engine keeps the per-tick cost proportional to what actually changed, not to the total graph size.

### Diagram linters and migration tools

`fm-cli parse --full --pretty` emits the entire IR including diagnostics, so you can write linters that:

- Reject diagrams with `Compatibility` diagnostics (anything that differs from mermaid-js).
- Reject any `Recovery` diagnostic (force authors to declare nodes explicitly rather than relying on placeholders).
- Enforce a minimum confidence on diagram-type detection (catch typos before they become silent fallbacks to `Flowchart`).
- Audit `click` directives against an URL allowlist.

```bash
# Fail CI on any recovery diagnostic
fm-cli validate diagram.mmd --format json | \
  jq -e 'all(.diagnostics[]; .category != "recovery")'
```

### Slide-deck embedding

Render to SVG with `--theme blueprint` or `--theme dark` and embed directly in HTML/Markdown slide engines (Reveal.js, slidev, Marp). Because output is deterministic, you can commit the SVG and avoid build-time dependencies in slide repos.

### Differential rendering for review

`fm-cli diff` produces both human-readable terminal diffs and machine-readable JSON. Use the JSON form to gate PRs on diagram structural changes without reviewing the full SVG diff:

```bash
gh pr diff --name-only HEAD~ HEAD | grep '\.mmd$' | while read -r f; do
  git show HEAD~:"$f" > /tmp/old.mmd
  fm-cli diff /tmp/old.mmd "$f" --format json > "diff-$f.json"
done
```

---

## Showcase and demo features

The [live demo](https://dicklesworthstone.github.io/frankenmermaid/) at `dicklesworthstone.github.io/frankenmermaid/` exercises the engine in-browser via the WASM bundle. The showcase HTML (`frankenmermaid_demo_showcase.html`) is a single self-contained file and is part of the regression suite — every push to `main` runs a demo-evidence guard that verifies the showcase still renders cleanly.

Features in the showcase:

| Surface | What it does |
|---|---|
| **Live editor** | Type Mermaid in the left pane, see the rendered diagram in the right pane. Renders incrementally so successive keystrokes are sub-frame |
| **Presenter mode** | Step-sequenced guided tour through the 80+ included examples, with narrated explanations of each diagram family |
| **Style studio** | Live theme variable editor — tweak `themeVariables.primaryColor`, `lineColor`, `clusterBkg`, etc. and see the result without reloading |
| **Compare mode** | Renders the same diagram with both frankenmermaid and a hosted mermaid.js instance side-by-side. The hosted E2E harness records differential summaries so the compare path is tracked as evidence |
| **Layout lab** | Switch between Sugiyama, force, tree, radial, etc. on the fly for the same input — useful for picking the right `--layout-algorithm` for a given graph |
| **Diagnostics panel** | Surfaces parser/layout/render diagnostics with source spans, severities, and suggestions |
| **Determinism checker** | Re-runs the pipeline N times and reports whether the SVG output is bit-identical across runs (it should be) |
| **Diff metadata tracking** | When you edit, the diagnostics panel shows added/removed/changed nodes and edges |
| **Split-shell layout** | The shell itself uses a CSS grid layout that adapts from desktop to mobile, with URL state sync so deep-links to specific examples work |

The hosted E2E harness preserves per-run `trace_id` lineage between the browser-side run and the Rust runtime, so deterministic-replay evidence can be tied back to the same observability IDs.

---

## Troubleshooting

### `fm-cli: command not found`

```bash
# If installed via the curl script, ensure ~/.local/bin is on PATH
export PATH="$HOME/.local/bin:$PATH"

# If installed via `cargo install`, ensure ~/.cargo/bin is on PATH
export PATH="$HOME/.cargo/bin:$PATH"
```

### WASM bundle builds but the browser demo is blank

```bash
# Rebuild WASM artifacts
./build-wasm.sh

# Serve over HTTP — file:// won't load WASM
python3 -m http.server 4173
```

### Labels overlap on dense graphs

Increase spacing or switch layout algorithm:

```toml
[layout]
algorithm    = "force"
node_spacing = 120
rank_spacing = 180
```

### Large diagrams feel slow in the browser

Switch to the Canvas2D backend and disable visual effects:

```toml
[svg]
shadows   = false
gradients = false
```

For 1,000+ node graphs, use the Canvas2D backend via the WASM `Diagram` class rather than SVG.

### Output differs from a mermaid-js screenshot

frankenmermaid is not a pixel-for-pixel clone of mermaid-js. It uses its own layout algorithms that often produce better results but will differ from upstream. Check diagnostics:

```bash
fm-cli validate input.mmd --verbose
fm-cli detect input.mmd --json
```

### Diagram type detected wrongly

Add an explicit header rather than relying on content heuristics:

```mermaid
flowchart LR
A --> B
```

### FNX analysis times out

`docs/FNX_USER_GUIDE.md` covers FNX troubleshooting in detail. Quick fix: pass `--fnx-mode disabled` for a single command, or set `[fnx.budget] timeout_ms = …` in the config.

## Limitations

- **Sequence diagrams** support participants, messages, activation boxes, notes, fragments (alt/opt/loop/par/critical/break), participant groups, and lifecycle events (create/destroy). Some edge cases in deeply nested fragments may still be refined; the conformance fixture corpus covers the common shapes.
- **XyChart** has dedicated layout and SVG rendering, including axis ticks, bar/line/area series, and category padding. Mixed-series and dual-axis polish is still evolving.
- **`classDef` / `style` directives** are fully applied in SVG. Terminal and Canvas backends still use theme defaults (the structured style system is wired into core; backend-specific application is incremental).
- **Very large SVGs** (10k+ nodes) can be heavy for browsers. Use the Canvas2D backend via WASM for interactive exploration.
- **PNG export** rasterizes the SVG output. CSS animations and hover effects are not preserved in static PNGs.
- **WebGPU backend** is plumbed in the WebRenderer selection logic but the implementation is a fallback to Canvas2D; full WebGPU rendering is a planned epic.
- **FNX directed analysis** (Phase 2) is in canary rollout via the `RolloutPhase` state machine in `fm-core/src/canary.rs` (`Disabled → Canary → Partial → Full`, with `RolledBack` for health-criteria violations). Enable explicitly with `--features fnx-experimental-directed` if you want to drive it.
- **`@frankenmermaid/core` npm package** is not on npm yet. The publish CI job exists and is gated to `refs/tags/v*` pushes; the project hasn't cut a tagged release. Until then, use the WASM bundle from the live demo or build locally with `./build-wasm.sh`.
- Some niche Mermaid syntax may parse with warnings and produce different visual output from mermaid-js; the `Compatibility` diagnostic category surfaces these explicitly.

## Documentation

- [`AGENTS.md`](AGENTS.md) — Guidelines for AI coding agents working in this codebase
- [`CHANGELOG.md`](CHANGELOG.md) — Chronological capability-wave changelog with commit links
- [`FEATURE_PARITY.md`](FEATURE_PARITY.md) — Authoritative parity status vs the FrankenTUI reference
- [`CRATES_IO_PUBLISHING.md`](CRATES_IO_PUBLISHING.md) — Publishing strategy and blocker resolution plan
- [`docs/FNX_INTEGRATION.md`](docs/FNX_INTEGRATION.md) — Authoritative FNX architecture contract
- [`docs/FNX_USER_GUIDE.md`](docs/FNX_USER_GUIDE.md) — When/how to use graph intelligence features
- [`docs/FNX_MIGRATION.md`](docs/FNX_MIGRATION.md) — Risk-tiered adoption guide with validation checklists
- [`docs/FNX_COMPATIBILITY_MATRIX.md`](docs/FNX_COMPATIBILITY_MATRIX.md) — Phase-by-phase capabilities
- [`docs/FNX_PHASE2_ROLLOUT.md`](docs/FNX_PHASE2_ROLLOUT.md) — Directed rollout policy, gates, and rollback triggers

## FAQ

### Is this a fork of mermaid-js?

No. It's a clean Rust implementation with its own parser, layout engine, and render pipeline. It reads the same Mermaid syntax but shares no code with mermaid-js.

### Can I migrate from `mermaid.initialize(...)` configs?

Yes. `frankenmermaid` accepts Mermaid-style `%%{init: {...}}%%` directives (when `parser.enable_init_directives = true`) and maps them to native config keys. CLI flags and TOML files take priority over inline init.

### Does it handle malformed diagrams?

Yes. The parser is explicitly designed to recover and produce best-effort output with diagnostics. It never panics on bad input. Property-based tests verify totality across random strings up to 256 characters including non-ASCII, control characters, and adversarial patterns.

### Which output format should I use?

| Use case | Format |
|---|---|
| Documentation / web embedding | `svg` |
| Static image sharing | `png` (requires `--features png`) |
| CI logs / terminal preview | `term` or `ascii` |
| Large interactive browser views | Canvas2D via the WASM `Diagram` class |
| Tooling integration | `json` (`fm-cli parse` and `fm-cli validate --format json`) |

### Is output deterministic for CI snapshots?

Yes. Deterministic tie-breaking and stable pipeline behavior are explicit design goals. The golden test suite verifies this via FNV-1a hash comparison of the rendered output.

### What is `legacy_mermaid_code/` in this repo?

A reference corpus (including upstream mermaid-js source/docs). It's not a port target and is gitignored (the historical gitlink was retired on 2026-04-21). Use it only for syntax/behavior edge-case validation.

### How does the layout algorithm get chosen?

When `algorithm = "auto"` (the default), the capability-aware dispatcher selects based on diagram type and graph topology (density, branching factor, cycle presence). You can override with `--layout-algorithm <name>` or in `[layout].algorithm`. The chosen algorithm is recorded in the layout decision ledger.

### What cycle strategy should I use?

| Strategy | Best for |
|---|---|
| `greedy` | Fast, good enough for most graphs |
| `dfs-back` | Predictable back-edge selection |
| `mfas` | Minimum reversed edges (better visual quality) |
| `cycle-aware` | Full SCC detection with cluster collapse (best quality, slightly slower) |

### How does the Sugiyama layout handle cycles?

Directed graphs with cycles can't be drawn in layers. The engine temporarily reverses selected edges to break cycles, runs the full layout, then marks those edges as `reversed: true` in the output. Renderers can draw reversed edges with dashed lines or special styling. The `cycle-aware` strategy additionally detects SCCs and can collapse them into visual clusters. All cycle-removal traversals use iterative stack-based variants — there is no recursion-depth limit on deep cyclic graphs.

### What happens with very large diagrams?

The guardrails kick in automatically. Before running layout, the engine estimates the computational cost. If the estimate exceeds the time budget (default 250 ms), it falls back to a cheaper algorithm. The fallback chain (Sugiyama → Tree → Grid; Force → Tree → Grid; Radial → Tree → Sugiyama) ensures that even 10,000-node graphs produce output in bounded time.

### Can I use the IR directly for tooling?

Yes. `fm-cli parse --full --pretty` emits the full IR as JSON including nodes, edges, clusters, labels, diagnostics, styles, and metadata. The WASM API also exposes `parse`, `diagramLens`, and `parseLens` for bidirectional editor integrations.

### How does the braille terminal rendering work?

Each terminal cell represents a 2×4 grid of sub-pixels using Unicode braille characters (U+2800–U+28FF). The renderer draws into a boolean pixel buffer using Bresenham's line algorithm, then encodes 8-pixel blocks into single braille code points. This gives an effective resolution of 2× the terminal width and 4× the terminal height — enough for smooth diagonal lines and curves.

### Why Rust instead of JavaScript?

Three reasons. (1) Determinism: Rust's lack of GC pauses and IEEE 754 discipline make output stability achievable. (2) Footprint: the WASM bundle ships without a JS runtime dependency. (3) Portability: the same code runs natively for CLI and in-browser via the wasm-bindgen surface — no JS engine required for the CLI / batch path.

### How does DOT format support work?

The DOT bridge parser recognizes Graphviz `digraph`/`graph` declarations, extracts nodes and edges with attributes, and converts them to `MermaidDiagramIr` with `DiagramType::Flowchart`. DOT files get the same layout algorithms, SVG themes, and terminal rendering as native Mermaid input. This covers the structural subset (nodes, edges, subgraphs, labels, HTML labels, attribute lists, escape sequences) rather than full Graphviz visual attribute passthrough.

### What does the FNX feature flag actually do?

When `fnx-integration` is on, `fm-layout` consults `franken_networkx` for graph metrics (centrality, connectivity, bridges, cycle scoring) during layout. The native engine still has final authority — FNX is advisory only. Centrality results show up as CSS classes on rendered nodes (`fm-node-centrality-high`, etc.). The `--fnx-mode {auto|enabled|disabled}` flag toggles this at the CLI. With `fnx-experimental-directed` on, Phase-2 directed algorithms (SCC, WCC, directed cycles, reachability) become available through the canary rollout state machine.

### Is the WASM binary small?

The release profile is tuned for it: `opt-level = "z"`, `lto = true`, `codegen-units = 1`, `panic = "abort"`, `strip = true`. The layout crate is the exception (`opt-level = 3`) because it's the computational bottleneck. The exact bundle size depends on which features you enable; the default WASM build is competitive with mermaid-js's gzipped bundle while running orders of magnitude faster on large graphs.

## Using frankenmermaid as a Rust library

Beyond the CLI and WASM surfaces, every workspace crate is a usable Rust library. The compile-time API mirrors the runtime pipeline:

```rust
use fm_core::MermaidDiagramIr;
use fm_parser::parse;                 // also: parse_with_mode, parse_with_mode_and_config
use fm_layout::{
    layout_diagram_with_config, CycleStrategy, LayoutAlgorithm, LayoutConfig,
};
use fm_render_svg::{render_svg_with_layout, SvgRenderConfig};

fn render(input: &str) -> String {
    // 1. Parse → IR. The parser never returns Err — it recovers and surfaces
    //    issues as ParseResult.warnings + ir.diagnostics, so there's no `?`.
    let parsed = parse(input);
    let ir: &MermaidDiagramIr = &parsed.ir;

    // 2. Layout. The defaults are good; override only what you need.
    let layout_config = LayoutConfig {
        algorithm: LayoutAlgorithm::Auto,
        cycle_strategy: CycleStrategy::CycleAware,
        ..LayoutConfig::default()
    };
    let layout = layout_diagram_with_config(ir, layout_config);

    // 3. Render. render_svg(ir) is a convenience wrapper that lays out
    //    internally; pass an explicit layout when you want to reuse one.
    render_svg_with_layout(ir, &layout, &SvgRenderConfig::default())
}
```

Convenience entry points cover the common case: `fm_parser::parse(input)` does detection + parsing in one call (use `parse_with_mode_and_config` for non-default `MermaidParseMode` / `ParserConfig`); `fm_layout::layout_diagram(ir)` runs the auto-dispatcher with default config; `fm_render_svg::render_svg(ir)` lays out internally and emits SVG. Reach for the longer-form `*_with_config` variants for batch / library use where you want explicit control.

### When to depend on individual crates

| Crate | When you need it directly |
|---|---|
| `fm-core` | You're consuming the IR (linters, formatters, exporters, diagram editors). Everything else depends on this |
| `fm-parser` | You only want parsing (e.g., to feed a custom renderer). Includes the DOT bridge |
| `fm-layout` | You have your own IR but want our layout algorithms |
| `fm-render-svg` | You have your own IR + layout but want our SVG output (gradients, themes, accessibility) |
| `fm-render-term` | You want braille/block/half-block terminal rendering for an unrelated graph type — the renderer is mostly generic over the `DiagramLayout` shape |
| `fm-render-canvas` | You're embedding into a non-WASM Canvas-like target (the trait `Canvas2dContext` is implementable against your own backend) |
| `fm-regression-harness` | You're building your own visual regression test harness over a Mermaid corpus |

### Traced layout

The traced variant returns a layout plus a structured trace describing every decision the engine made:

```rust
use fm_layout::layout_diagram_traced;

let traced = layout_diagram_traced(&ir);

// Layout result + quality stats live on `traced.layout`
let stats = &traced.layout.stats;
println!("nodes: {}, edges: {}", stats.node_count, stats.edge_count);
println!("crossings before refinement: {}", stats.crossing_count_before_refinement);
println!("crossings final: {}", stats.crossing_count);
println!("reversed edges: {}", stats.reversed_edges);
println!("total edge length: {:.1}", stats.total_edge_length);

// Pipeline decisions live on `traced.trace`
let dispatch = &traced.trace.dispatch;
let guard = &traced.trace.guard;
println!("requested: {:?}", dispatch.requested);
println!("selected (after dispatch): {:?}", dispatch.selected);
println!("selected (after guard): {:?}", guard.selected_algorithm);
println!("guard fallback applied: {}", guard.fallback_applied);
println!("guard reason: {}", guard.reason);

// Incremental-recompute bookkeeping
let inc = &traced.trace.incremental;
println!("cache hit: {}, recomputed {}/{} nodes in {} µs",
    inc.cache_hit, inc.recomputed_nodes, inc.total_nodes, inc.recompute_duration_us);
```

The trace is stable across runs (modulo wall-clock fields you can filter out); the layout decision ledger and the golden test harness both read from it.

---

## Acknowledgments and references

frankenmermaid borrows heavily from prior graph-drawing research and from the open-source diagram-tool ecosystem.

### Diagram syntax and visual conventions

- **[mermaid-js](https://github.com/mermaid-js/mermaid)** by Knut Sveidqvist and contributors. The Mermaid syntax this project parses is theirs; the `legacy_mermaid_code/` reference corpus includes the upstream source for behavior cross-checking. The intentional differences from upstream are surfaced through the `Compatibility` diagnostic category and the conformance harness.
- **[Graphviz / DOT](https://graphviz.org/)** by AT&T Research / John Ellson et al. The DOT bridge parser lets you feed Graphviz syntax through the same pipeline; the architectural separation of "syntax" and "layout" comes from the Graphviz tradition.
- **[FrankenTUI](https://github.com/Dicklesworthstone/frankentui)** — the immediate parent project from which the original Mermaid parser/layout/render code was extracted. The conformance harness still uses FrankenTUI as a behavioral reference.

### Algorithms

- **Sugiyama hierarchical layout** — Kozo Sugiyama, Shojiro Tagawa, and Mitsuhiko Toda, *"Methods for Visual Understanding of Hierarchical System Structures"* (1981).
- **Brandes-Köpf coordinate assignment** — Ulrik Brandes and Boris Köpf, *"Fast and Simple Horizontal Coordinate Assignment"* (2001). Four-alignment median + vertical compaction.
- **Crossing-count via merge-sort inversions** — Wilhelm Barth, Petra Mutzel, and Michael Jünger, *"Simple and Efficient Bilayer Cross Counting"* (2002).
- **Tarjan's strongly connected components** — Robert Tarjan, *"Depth-First Search and Linear Graph Algorithms"* (1972).
- **Fruchterman-Reingold force-directed layout** — Thomas Fruchterman and Edward Reingold, *"Graph Drawing by Force-Directed Placement"* (1991).
- **Barnes-Hut N-body approximation** — Josh Barnes and Piet Hut, *"A Hierarchical O(N log N) Force-Calculation Algorithm"* (1986).
- **Reingold-Tilford tidy tree** — Edward Reingold and John Tilford, *"Tidier Drawings of Trees"* (1981).
- **Adapton self-adjusting computation** — Matthew Hammer et al., *"Adapton: Composable, Demand-Driven Incremental Computation"* (2014). Our implementation is a deliberately simplified, single-threaded, coarse-grained variant.
- **Cache-oblivious layout (van Emde Boas)** — Michael Bender, Erik Demaine, and Martin Farach-Colton, *"Cache-Oblivious B-Trees"* (2000). The `build_veb()` BFS-index tree gives good locality at every level of the memory hierarchy without tuning.
- **Equality saturation / e-graphs** — Max Willsey et al., *"egg: Fast and Extensible Equality Saturation"* (2021), via the [`egg`](https://crates.io/crates/egg) crate.
- **Conformal geometric algebra** — Hestenes/Doran/Lasenby's modern geometric algebra. Our rotor-based transform composition follows the textbook construction adapted to 2D screen space.
- **Minimum feedback arc set heuristics** — Peter Eades, Xuemin Lin, and W.F. Smyth, *"A Fast and Effective Heuristic for the Feedback Arc Set Problem"* (1993).

### Rust / WASM stack

- **[clap](https://github.com/clap-rs/clap)** for CLI parsing.
- **[serde](https://serde.rs/)** for IR / config serialization.
- **[thiserror](https://github.com/dtolnay/thiserror)** for ergonomic error derivation.
- **[wasm-bindgen](https://github.com/rustwasm/wasm-bindgen)** + **[js-sys](https://docs.rs/js-sys)** + **[web-sys](https://docs.rs/web-sys)** for the WASM bridge.
- **[tracing](https://github.com/tokio-rs/tracing)** + **[tracing-subscriber](https://docs.rs/tracing-subscriber)** for structured observability.
- **[notify](https://github.com/notify-rs/notify)** for the `watch` feature.
- **[tiny_http](https://crates.io/crates/tiny_http)** for the `serve` feature.
- **[resvg](https://github.com/RazrFalcon/resvg)** + **[usvg](https://github.com/RazrFalcon/resvg/tree/master/crates/usvg)** for the optional PNG rasterization path.
- **[unicode-segmentation](https://github.com/unicode-rs/unicode-segmentation)** for grapheme-aware label handling.
- **[json5](https://github.com/callum-oakley/json5-rs)** for init-directive parsing fallback.

The optional graph-intelligence layer is built on **[franken_networkx](https://github.com/Dicklesworthstone/franken_networkx)**, which itself is a Rust port/reimplementation of selected NetworkX algorithms.

---

## Glossary of frankenmermaid-specific terms

Project-specific vocabulary that shows up in the trace output, the decision ledger, and the source. Quick reference:

| Term | Meaning |
|---|---|
| **IR** | The shared `MermaidDiagramIr` — every parser produces one, every renderer consumes one |
| **Capability claim matrix** | An executable, evidence-backed manifest of what surfaces (CLI commands, diagram types, render targets) are implemented vs partial vs planned. Generated into the README and consumed by CI |
| **Capability profile** | A pinned subset of the capability matrix used by the runtime dispatcher to avoid auto-selecting algorithms that are only `partial` for the current diagram type |
| **Layout decision ledger** | The structured trace of every dispatch / guard / degradation decision the layout engine made, persisted as line-delimited JSON |
| **Decision explanation** | The same decision rendered at three escalating detail tiers — traffic light, plain English, constraint checks — for use by humans and downstream tooling |
| **Guard report** | `MermaidGuardReport` — describes which guardrails fired during a layout pass and why |
| **Degradation plan** | `MermaidDegradationPlan` — the structured list of degradation operators applied to reduce cost under pressure |
| **Degradation operator** | A single composable visual-fidelity reduction (disable shadows, simplify edges, drop minor labels, downgrade tier, …) |
| **Pressure** | Current load on the cross-stage global budget broker, with `pressure_source` (parser / layout / render / external) and `pressure_tier` (none / low / medium / high) |
| **Witness** | A bookkeeping record describing the analysis a particular subsystem performed for a single render. The FNX witness records which centrality metrics were computed, on which projection, and how long they took |
| **Evidence bundle** | A self-describing JSON artifact persisted by the `evidence` binary, containing input hash, output artifact hash, timings, dispatch decisions, and pass/fail status |
| **Lens** | A bidirectional view (`diagramLens` / `parseLens`) over source text that preserves trivia so structural edits don't disturb unchanged regions |
| **Trivia** | Comments, whitespace, quote-style — everything the parser would normally throw away. The lens system preserves these so emitted edits keep the original spelling of unchanged tokens |
| **Trace ID** | Stable observability ID (from `franken-kernel`) shared across the decision ledger, witness records, and evidence artifacts for a single render |
| **Conformance fixture** | A `(input, expected-output)` pair in `crates/fm-cli/tests/frankentui_conformance_cases.json` that pins behavior against the FrankenTUI reference |
| **Bead** | A task tracker entry (`bd-XXXX`) in `.beads/` — used for dependency-aware work planning. Bead IDs appear in commit messages |
| **Rollout phase** | One of `RolloutPhase::{Disabled, Canary, Partial, Full, RolledBack}` from `fm-core/src/canary.rs`, gating what fraction of requests use Phase-2 FNX. Maps to the conceptual Shadow / Advisory / Full Integration modes documented in [`docs/FNX_PHASE2_ROLLOUT.md`](docs/FNX_PHASE2_ROLLOUT.md) |

---

## Naming

The name **frankenmermaid** is descriptive in the same vein as FrankenTUI: a Rust reincarnation of an upstream tool that has been opened up, stripped for parts, and rebuilt on a different chassis without trying to be a one-for-one clone. The "franken" prefix has become a small naming pattern across related projects in this maintainer's repos (`frankentui`, `franken_networkx`, `franken-kernel`, `frankensearch`, …) — they share design principles (Rust-first, deterministic, agent-friendly, evidence-emitting) and sometimes share infrastructure like `franken-kernel` for trace/decision IDs.

---

## Compatibility caveats vs mermaid-js

frankenmermaid intentionally differs from mermaid-js in several places where the upstream behavior is either unsafe, ambiguous, or hostile to determinism. Every intentional difference is surfaced through the `Compatibility` diagnostic category so tooling can detect them programmatically.

| Area | mermaid-js | frankenmermaid |
|---|---|---|
| Output hash stability | Not guaranteed across versions or runs | Explicit goal; locked by golden tests |
| Empty diagram | Throws | Renders a valid empty SVG with a `data-empty="true"` marker and emits a `Structural` warning |
| Unknown diagram type | Throws on parse | Falls back to `Flowchart` with low-confidence detection + warning |
| Dangling-edge reference | Throws | Auto-creates an implicit placeholder node + `Recovery` warning |
| `click` directive with `javascript:` URL | Rendered (subject to `securityLevel`) | Blocked in `Strict` sanitize mode regardless of `enable_links`; allowed only in `Lenient` mode + `enable_links = true` |
| `themeVariables` color with `data:` / `vbscript:` schemes | Sometimes silently passes through | Stripped during sanitization |
| Init directive trust | Inline init always applied | Off by default (`parser.enable_init_directives = false`); enable explicitly when the input source is trusted |
| Recursion-deep cyclic graphs | Risk of stack overflow on some platforms | All cycle-removal traversals are iterative; bounded by the layout iteration budget |
| Hash-map iteration order in output | Iteration order can vary | Uses `BTreeMap` everywhere for stable iteration |
| Layout under pressure | Best-effort, may produce arbitrarily slow renders on pathological input | Bounded by time / iteration / route-op guardrails with deterministic degradation operators |

If pixel-identical mermaid-js output is required, use the **compare mode** in the showcase — it runs the diagram through both engines side-by-side and surfaces the differences as structured evidence.

---

## A worked end-to-end example

Walking a single flowchart through the full pipeline:

### 1. Input

```
flowchart LR
  Start([Start]) --> Parse{Valid?}
  Parse -->|yes| Layout[Layout engine]
  Parse -->|no| Reject[Reject + diagnostics]
  Layout --> Render[Renderer]
  Render --> Done([Done])
```

### 2. Detection

`fm_parser::parse` invokes the five-tier detection pipeline internally:

- Tier 1 (DOT): no match (no `digraph`/`graph` keyword).
- Tier 2 (exact keyword): `flowchart` matched at the start → `DiagramType::Flowchart`, confidence `1.0`, method `ExactKeyword`.

Tiers 3–5 are skipped.

### 3. Parsing → IR

The flowchart parser builds the IR via `IrBuilder`:

```
nodes = [
  IrNode { id: "Start",  label: Some("Start"),               shape: Stadium, implicit: false, … },
  IrNode { id: "Parse",  label: Some("Valid?"),              shape: Diamond, implicit: false, … },
  IrNode { id: "Layout", label: Some("Layout engine"),       shape: Rect,    implicit: false, … },
  IrNode { id: "Reject", label: Some("Reject + diagnostics"), shape: Rect,   implicit: false, … },
  IrNode { id: "Render", label: Some("Renderer"),            shape: Rect,    implicit: false, … },
  IrNode { id: "Done",   label: Some("Done"),                shape: Stadium, implicit: false, … },
]
edges = [
  IrEdge { from: Node("Start"),  to: Node("Parse"),  label: None,        arrow: Arrow, … },
  IrEdge { from: Node("Parse"),  to: Node("Layout"), label: Some("yes"), arrow: Arrow, … },
  IrEdge { from: Node("Parse"),  to: Node("Reject"), label: Some("no"),  arrow: Arrow, … },
  IrEdge { from: Node("Layout"), to: Node("Render"), label: None,        arrow: Arrow, … },
  IrEdge { from: Node("Render"), to: Node("Done"),   label: None,        arrow: Arrow, … },
]
direction = LR
```

(Labels are actually `Option<IrLabelId>` referencing a separate interned `labels: Vec<IrLabel>`; endpoints are `IrEndpoint::Node(IrNodeId)` or `IrEndpoint::Port(IrPortId)`. Shown here in resolved / human-readable form for readability.)

No diagnostics are emitted — the input is well-formed.

### 4. Dispatch

The capability-aware dispatcher inspects the graph:

- 6 nodes, 5 edges, DAG (no cycles).
- Density is low, branching is shallow.
- Diagram type is `Flowchart`.

Decision: `Sugiyama` with `CycleAware` strategy (the default cycle strategy still kicks in, even on DAGs, because it's cheap on acyclic input). Dispatcher confidence `~950‰`. The decision ledger records `requested_algorithm = "auto"`, `selected_algorithm = "sugiyama"`, `dispatch_reason = "diagram_type_dispatch"`, `fallback_applied = false`.

### 5. Layout

Sugiyama phases execute in order:

1. Cycle removal: no cycles, no edges reversed.
2. Rank assignment: `Start = 0`, `Parse = 1`, `{Layout, Reject} = 2`, `Render = 3`, `Done = 4` (longest-path heuristic; `Reject` and `Layout` share rank 2).
3. Crossing minimization: trivial — no crossings possible at this size.
4. Refinement: nothing to refine.
5. Coordinate assignment (Brandes-Köpf): produces balanced x/y positions with `LR` direction (rank axis = x, order axis = y).
6. Edge routing: orthogonal Manhattan paths. The two `Parse → {Layout, Reject}` edges get the `yes` / `no` midpoint labels.
7. Post-processing: cluster boundaries (none here), coordinate normalization.

Layout stats: `crossing_count = 0`, `total_edge_length = …`, `reversed_edge_total_length = 0`, `bounds = (W, H)`.

### 6. Render scene

The scene IR receives a `RenderGroup` with four layers: clusters (empty), edges (5 orthogonal paths with arrowheads + 2 midpoint labels), nodes (6 shapes — 2 stadiums, 1 diamond, 3 rects), labels (positioned text).

### 7. SVG output

The SVG renderer emits:

- A root `<svg>` with `viewBox`, `<title>`, `<desc>`, and accessibility attributes.
- A `<defs>` block with the gradient, drop-shadow, and arrowhead marker definitions for the active theme.
- A `<style>` block with the active theme's CSS custom properties (15 base + derived; see "Theme system" above for the breakdown).
- One `<g class="fm-cluster-layer">` (empty), one `<g class="fm-edge-layer">` containing 5 `<path>` elements, one `<g class="fm-node-layer">` containing 6 shape elements, one `<g class="fm-label-layer">` with positioned text.

If `--embed-source-spans` was passed, every node and edge group carries `data-fm-source-span="LINE:COL"` linking back to the input.

### 8. Evidence

`evidence` is appended with the input hash, output artifact hash, parse/layout/render timings, node/edge counts, layout bounds, degradation tier (`full` — no degradation was needed), and pass/fail reason. A second run with identical input produces a byte-identical SVG and a structurally identical evidence record (modulo wall-clock fields), which the determinism CI gate verifies.

---

## Performance tuning guide

The auto dispatcher plus guardrails handle the common cases, so most diagrams need no tuning at all. When something is genuinely slow:

### "My render is slow"

1. Pass `--verbose` and inspect the decision ledger. Was Sugiyama selected on a 5,000-node graph? If so, the crossing-minimization phase is dominating.
2. Override with `--layout-algorithm force` (Barnes-Hut activates above 100 nodes) or `--layout-algorithm tree` for sparse graphs.
3. Increase the layout iteration / route budgets in `MermaidConfig` if you have a custom embedding.
4. For repeated renders of similar input (an editor / dashboard), use the incremental layout engine via the WASM `Diagram` class — successive renders skip clean stages.

### "My SVG is huge"

1. Disable visual effects: `[svg] shadows = false`, `gradients = false`.
2. Disable source-span attributes (drop `--embed-source-spans`).
3. Use the Canvas2D renderer via WASM for interactive use; SVG is best for static / printable output.

### "My terminal output is slow on a remote SSH session"

1. Force a lower tier: `--format ascii` skips Unicode entirely.
2. Set `[term] tier = "compact"` in the config.
3. Disable the minimap: `[term] minimap = false`.

### "My WASM bundle is too big"

The default build is `opt-level = "z"` + `lto = true` + `strip = true`. Further size reductions usually mean disabling features. `build-wasm.sh` produces a minimal-feature bundle by default.

### "I want even better layout quality, time isn't a concern"

1. Set `[layout] cycle_strategy = "cycle-aware"` (default) but increase `layout_iteration_budget` in `MermaidConfig`.
2. Enable the E-graph crossing minimizer (it's enabled by default for small graphs; the budget controls when it falls back).
3. Run with `fnx-integration` enabled — FNX centrality and bridge metrics improve cycle-strategy choices.

---

## Diagnostic catalogue

Diagnostics carry a structured `(severity, category, error_code, rule_id)` tuple. The categories are:

| Category | When emitted | Example error codes |
|---|---|---|
| **Lexer** | Tokenization problems | `mermaid/lex/invalid-char`, `mermaid/lex/unterminated-string` |
| **Parser** | Syntax errors | `mermaid/parse/expected-arrow`, `mermaid/parse/malformed-init` |
| **Semantic** | Valid syntax, questionable intent | `mermaid/semantic/duplicate-node`, `mermaid/semantic/conflicting-shape` |
| **Recovery** | Parser took corrective action | `mermaid/diag/recovery`, `mermaid/diag/implicit-node`, `mermaid/diag/auto-close-delim` |
| **Inference** | Intent was inferred from ambiguous input | `mermaid/diag/fuzzy-match`, `mermaid/diag/fallback-flowchart` |
| **Compatibility** | Feature works but differs from mermaid-js | `mermaid/diag/compat-classdef-default`, `mermaid/diag/compat-link-sanitized` |

`fm-cli validate --format json` emits each diagnostic as a `StructuredDiagnostic`:

```json
{
  "error_code": "mermaid/diag/recovery",
  "severity": "warning",
  "category": "recovery",
  "message": "Node 'X' auto-created as placeholder",
  "source_line": 7,
  "source_column": 12,
  "rule_id": "implicit-node",
  "confidence": 0.85,
  "remediation_hint": "Add explicit node declaration: X[Label]"
}
```

Diagnostics are sorted deterministically by `(severity, line, column, stage, error_code, message)` so any two runs over the same input produce identical reports.

---

## CI/CD integration recipes

### GitHub Actions: render diagrams + check for drift

```yaml
name: Diagrams
on: [push, pull_request]
jobs:
  render:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install frankenmermaid
        run: curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh | bash
      - name: Render diagrams
        run: |
          export PATH="$HOME/.local/bin:$PATH"
          find docs -name '*.mmd' | while read -r f; do
            fm-cli render "$f" --format svg --output "${f%.mmd}.svg"
          done
      - name: Check for drift
        run: git diff --exit-code -- docs/
```

### Reject diagrams with parse warnings

```yaml
- name: Validate
  run: |
    fail=0
    find docs -name '*.mmd' | while read -r f; do
      fm-cli validate "$f" --fail-on warning || fail=1
    done
    exit $fail
```

### Snapshot a render and gate on hash

```yaml
- name: Render to a fixed digest
  run: |
    fm-cli render docs/architecture.mmd --format svg --output /tmp/arch.svg
    expected=$(cat docs/architecture.sha256)
    actual=$(sha256sum /tmp/arch.svg | cut -d' ' -f1)
    [ "$expected" = "$actual" ] || { echo "SVG drift!"; diff <(cat docs/architecture.svg) /tmp/arch.svg; exit 1; }
```

### Differential PR review

```yaml
- name: Diff changed diagrams
  if: github.event_name == 'pull_request'
  run: |
    git fetch origin "${{ github.base_ref }}"
    git diff --name-only "origin/${{ github.base_ref }}" '*.mmd' | while read -r f; do
      git show "origin/${{ github.base_ref }}:$f" > /tmp/old.mmd 2>/dev/null || continue
      fm-cli diff /tmp/old.mmd "$f" --format summary
    done
```

---

## Theme gallery

Each of the 10 theme presets is tuned for a different audience. The defining characteristic of each:

| Theme | Background | Node fill | Edge | Accent | When to pick it |
|---|---|---|---|---|---|
| **Default** | Off-white | Light gray-blue | Steel blue | Cobalt | General documentation, clean and unopinionated |
| **Dark** | Near-black slate | Dark blue-gray | Light gray | Cyan | Dark-mode sites, slide decks with dark backgrounds |
| **Forest** | Cream | Sage green | Forest green | Olive | Organic, soft documentation, designer-friendly |
| **Neutral** | Pure white | Light gray | Mid gray | Charcoal | Black-and-white printables, minimalist sites |
| **Corporate** | White | Slate blue | Steel | Navy | Internal company docs, enterprise reports |
| **Neon** | Black | Dark indigo | Magenta | Cyan/lime | Tech demos, hackathon posters |
| **Pastel** | Off-white | Soft peach/lavender | Muted gray | Coral | Friendly UX docs, designed-for-humans |
| **HighContrast** | Pure black or white | Solid white/black | Solid stroke | High-contrast accent | WCAG AAA compliance, accessible PDFs |
| **Monochrome** | White | White with black stroke | Black | Black | Newspaper-style, etching-style, print |
| **Blueprint** | Cyan-dark blue | Transparent | White stroke | White text | Architectural / technical-drawing aesthetic |

All themes honor `themeVariables` overrides for any individual color, so a `Dark` theme with a custom `primaryColor: "#ff6b6b"` is one line of init away. Custom themes are also supported — implement `From<ThemePreset> for ThemeColors` on your own enum (see `crates/fm-render-svg/src/theme.rs:114` for the pattern).

---

## Building the WASM bundle

`build-wasm.sh` is the canonical WASM build entry point. It:

1. Verifies `wasm-pack` and `wasm-opt` (from binaryen) are on `PATH` and aborts with an actionable error message if either is missing.
2. Ensures the `wasm32-unknown-unknown` target is installed.
3. Runs `wasm-pack build crates/fm-wasm --release --target web --out-dir pkg --out-name frankenmermaid` with size-tuned `RUSTFLAGS` (`-C target-feature=+bulk-memory,+mutable-globals,+nontrapping-fptoint,+sign-ext,+reference-types,+multivalue` plus `-Zlocation-detail=none -Zfmt-debug=none`) and forces the `fm-layout` release profile to `opt-level = "z"` for the WASM build only.
4. Runs `wasm-opt -Oz --all-features --converge` over the resulting `.wasm` to drive size down further.
5. Rewrites `pkg/package.json` with the canonical `@frankenmermaid/core` metadata (name, description, repository, homepage, bugs URL, keywords, files list, capability-matrix metadata).
6. Copies the project `README.md` into `pkg/README.md` so the npm registry page mirrors the GitHub page.
7. Enforces a hard **500 KB gzip ceiling** on the resulting `.wasm`; the build fails if the bundle exceeds it.

The script takes no flags — its behavior is deterministic given a fixed `Cargo.lock`. The hardcoded `--release --target web` choices reflect the package's intended use case (in-browser ES module). If you need a different target preset (`bundler`, `nodejs`, `no-modules`) for a non-browser embedding, invoke `wasm-pack build` directly with your own arguments.

Actual measured bundle (from the current `pkg/`):

| Artifact | Raw | Gzipped |
|---|---|---|
| `frankenmermaid_bg.wasm` | ~1.05 MB | ~433 KB |
| `frankenmermaid.js` (wasm-bindgen glue) | ~53 KB | ~8 KB |
| `frankenmermaid.d.ts` (TypeScript types) | ~3.1 KB | ~1 KB |

The 500 KB gzip ceiling is enforced by `MAX_GZIP_BYTES=$((500 * 1024))` in `build-wasm.sh`. The default build excludes the optional `fnx-integration` feature because the `franken_networkx` crates are heavyweight relative to the size budget. If you need FNX in the browser, build with `--features fnx-integration` directly (bypassing `build-wasm.sh`'s size ceiling) and accept the larger bundle.

---

## Running the regression harness

The `fm-regression-harness` crate ingests a corpus of real-world Mermaid diagrams (a mix of public examples, mermaid-js test fixtures, and adversarial inputs) and emits an HTML report with a thumbnail grid. Each card includes:

- A side-by-side render thumbnail (frankenmermaid output + small diff against last commit's output).
- Parse / layout / render timings.
- Diagnostics count by severity.
- Pass/fail status against CI-friendly performance thresholds.

```bash
# Run against the bundled corpus
cargo run -p fm-regression-harness --release -- \
  --corpus tests/corpus \
  --out target/regression-report.html

# Filter by case name regex
cargo run -p fm-regression-harness --release -- \
  --corpus tests/corpus \
  --filter 'cycle|sankey' \
  --out target/regression-report.html
```

The HTML report uses no JavaScript and renders cleanly when served as a static file.

---

## Observability and tracing

frankenmermaid uses the `tracing` crate throughout. Enable structured tracing by initializing a subscriber before invoking the library:

```rust
use tracing_subscriber::{EnvFilter, fmt};

fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .with_target(true)
    .with_thread_ids(false)
    .json()
    .init();
```

Then run with `RUST_LOG=fm_layout=debug,fm_parser=info,fm_render_svg=warn fm-cli render input.mmd`. Spans of interest:

| Span | Fields |
|---|---|
| `fm_parser::parse` | `diagram_type`, `confidence`, `method` |
| `fm_layout::layout_diagram` | `algorithm`, `cycle_strategy`, `node_count`, `edge_count` |
| `fm_layout::sugiyama::cycle_removal` | `strategy`, `reversed_edges`, `cycles` |
| `fm_layout::sugiyama::crossing_min` | `crossings_before`, `crossings_after`, `passes` |
| `fm_layout::guard` | `initial_algorithm`, `selected_algorithm`, `reason`, `fallback_applied` |
| `fm_layout::fnx::analyze` | `metrics`, `projection`, `duration_ms`, `cache_hit` |
| `fm_render_svg::render` | `node_count`, `edge_count`, `bounds_w`, `bounds_h` |

The CLI's `-v` / `-vv` / `-vvv` flags set the equivalent env filter for you. When `log_path` is set in `MermaidConfig`, structured trace output is duplicated to that file regardless of the CLI flag.

---

## Roadmap

The exhaustive backlog lives in [`.beads/`](.beads/). The largest pieces of in-flight or planned work as of this commit:

| Area | Status | Notes |
|---|---|---|
| **VS Code extension with live preview** (`bd-kgi4`) | Planned | Uses the WASM build; expected to be faster than the mermaid.js extension thanks to incremental computation |
| **CEGIS layout constraint synthesis** (`bd-1iwz`) | Speculative | Counterexample-guided synthesis to auto-generate layout constraints from user-provided examples |
| **Geometric/Conformal Algebra extensions** (`bd-3cd8`) | In progress | The CGA pipeline currently covers transforms + intersection queries; the broader epic includes coordinate-free PGA/CGA for layout primitives |
| **Stochastic superoptimization for hot kernels** (`bd-ejxz`) | Speculative | Apply stochastic superoptimization to the crossing-minimization inner loop |
| **WebGPU diagram renderer** (`bd-2u0.2`) | Partial | The WebRenderer selector exists; Canvas2D is the current fallback; full WebGPU implementation is queued |
| **Web Worker / OffscreenCanvas path** (`bd-2u0.6`) | Planned | Off-main-thread rendering for large in-browser diagrams |
| **LP/MIP solver backend for constraint layout** (`bd-1fef.2`) | Partial | Constraint-based layout exists; the LP/MIP backend would replace the heuristic solver for hard constraints |
| **`@frankenmermaid/core` npm publish** | CI wired, awaiting tag | The npm-publish CI job exists (`bd-hye0`) and is gated to `refs/tags/v*`. The first `v0.1.0` tag will publish to npm automatically; the job idempotently no-ops if the version is already on npm |
| **Crates.io publish** | Blocker-clear | Per `CRATES_IO_PUBLISHING.md`, all blockers are resolved as of 2026-04-21 (`franken-kernel` migrated). Awaiting publish-order execution |
| **Swiss Tables for node/edge maps** (`bd-2gr9`) | Planned | Replace `BTreeMap` hot paths with deterministic hash-based maps; needs a determinism story |
| **Triage UBS warning baseline** (`bd-tp4z`) | In progress | Bringing the UBS scanner output to zero |

---

## Trying it without installing

To confirm a diagram renders the way you expect, paste it into the live demo:

<https://dicklesworthstone.github.io/frankenmermaid/>

The demo uses the same WASM build that will ship to npm once publishing is live; anything `fm-cli render` does locally, the demo does in the browser. On top of that, the showcase adds presenter mode, a style studio, compare mode against hosted mermaid.js, a layout lab for swapping algorithms, a diagnostics panel, and a determinism checker that re-runs your input N times and verifies bit-identical output.

For batch / CI use without installing, a 30-second container recipe:

```dockerfile
# Save as Dockerfile, then: `docker build -t frankenmermaid:local .`
FROM rust:1.95-slim
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates git build-essential \
 && curl -fsSL https://raw.githubusercontent.com/Dicklesworthstone/frankenmermaid/main/install.sh | bash \
 && rm -rf /var/lib/apt/lists/*
ENV PATH="/root/.local/bin:$PATH"
ENTRYPOINT ["fm-cli"]
```

```bash
docker run --rm -v "$PWD:/work" -w /work frankenmermaid:local render input.mmd --format svg --output out.svg
```

---

## Mental model: what the IR represents

A `MermaidDiagramIr` is a lossless representation of *what the author wrote*, normalized to canonical names but not yet decorated with layout coordinates or visual style. Three rules describe how to think about it:

1. **The IR has more structure than the source text.** A flowchart's `A --> B` produces two `IrNode` entries (created via `intern_node`) and one `IrEdge`, even though the source never explicitly declares the nodes. Implicit nodes carry `implicit: true` so renderers can distinguish them. Conversely, the IR has less style than the source — `classDef` / `style` directives become structured `IrStyle` references rather than CSS strings.
2. **The IR has more semantics than the surface syntax.** `A -->|yes| B` produces an `IrEdge` with a label, but the *meaning* of that edge — generic flow, ER relationship, sequence message, gantt dependency, gitGraph commit edge — is captured by `IrEdgeKind` on the indexed `MermaidGraphIr` view, not by the surface arrow.
3. **The IR is detachable from rendering.** Two different renderers consume the same IR and produce visually different output: an SVG renderer with gradients vs a terminal renderer with braille pixels. The IR doesn't know about gradients or braille; the renderers do. Parse-once-render-everywhere works because that separation is real, not approximate.

When debugging unexpected output, the first thing to do is `fm-cli parse --full --pretty input.mmd` and inspect the IR. If the IR looks wrong, the parser is wrong. If the IR looks right but the output is wrong, the layout or renderer is wrong. This separation is also why golden tests work: they hash the SVG, but you can also hash the IR (via serde JSON round-trip) for parser-level regression detection.

---

## Algorithm complexity cheat sheet

The asymptotic complexity of every algorithm currently in `fm-layout`. Times in the "Per-graph cost" column use n = node count and m = edge count.

| Algorithm | Per-graph cost | Determinism source |
|---|---|---|
| Type detection (exact + fuzzy keyword) | O(L) where L is input length | Stable keyword table |
| Five-tier detection pipeline (worst case fallback) | O(L) | Same |
| IR builder (interning) | O(n + m) | `BTreeMap` keyed by ID |
| Cycle removal (Greedy) | O(n + m) | Stable sink/source removal order |
| Cycle removal (DFS back-edge) | O(n + m) | Iterative DFS in stable node order |
| Cycle removal (MFAS approximation) | O((n + m) log n) per SCC | Stable sort by `(out − in)` degree |
| Cycle removal (cycle-aware + SCC) | O(n + m) for Tarjan, then per-component MFAS | Tarjan with index/lowlink |
| Rank assignment (longest-path) | O(n + m) | Min-heap with stable priority |
| Crossing counting (merge-sort inversions) | O(m log m) per rank pair | Stable sort |
| Barycenter crossing minimization | O(R × (m + n log n)) where R = sweeps (≤ 4) | Tie-break by stable node index |
| Transpose refinement | O(R × n × ⌀² ) where ⌀ = avg rank size, R ≤ 10 passes, early exit on zero | Stable adjacent-swap order |
| Sift refinement | O(n × ⌀ × m) worst case | Stable scan order |
| E-graph equality saturation (when enabled) | Bounded by node-budget + timeout, falls back to Sugiyama otherwise | Egg's deterministic extraction |
| Brandes-Köpf coordinate assignment | O(n + m) per alignment × 4 alignments | Stable median selection |
| Orthogonal edge routing | O(m) base + O(m × O) where O = obstacles checked per edge | Stable obstacle order |
| CGA intersection queries | O(1) per query | Pure floating-point math |
| Cluster boundary computation | O(n) | Stable member iteration |
| Force-directed (naive) | O(I × n²) where I ≤ 500 iterations | Hash-seeded init + deterministic FP |
| Force-directed (Barnes-Hut, n > 100) | O(I × n log n) | Same |
| Tree layout (Reingold-Tilford) | O(n) | Stable BFS order |
| Radial layout | O(n) | Stable leaf-count + drift correction |
| Sankey layout (with k relaxation passes) | O(k × m) | Stable column assignment |
| Grid layout | O(n) | Column declaration order |
| Sequence layout | O(P × M) where P = participants, M = messages | Stable message order |
| Gantt layout | O(T) where T = tasks | Stable section/task order |
| GitGraph layout | O(C) where C = commits | Stable branch lane assignment |
| Pie / Quadrant / Packet layouts | O(n) | Trivial / declarative |
| Incremental layout (cache hit) | O(d) where d = changed nodes (Laplacian boundary smoothing only) | Adapton DCG |
| Layout decision ledger build | O(K) where K = decision points | — |
| SVG document emission | O(n + m + s) where s = style/defs entries | Stable scene-walk order |
| Terminal sub-cell encoding (Braille) | O(width × height) | — |
| Diff engine (structural) | O(n₁ + n₂ + m₁ + m₂) | `BTreeMap` joins |
| FNX centrality (FxHash-backed) | O(n + m) for degree, O(n × (n + m)) for betweenness if enabled | Stable graph projection |

Constants are reasonable: a typical documentation diagram of 50 nodes / 80 edges parses in well under 1 ms and renders to SVG in 2–5 ms on a modern laptop. The 250 ms guardrail budget targets ~5,000-edge graphs and trips fallbacks earlier on adversarial inputs.

---

## Edge routing deep dive

Of all the layout phases, edge routing produces the most visible diffs on small input changes; the path of any given edge depends on the positions of every node it might intersect. The full pipeline:

### Phase A — Endpoint resolution

For each `IrEdge`, the router resolves `from` / `to` (which are `IrEndpoint::Unresolved` / `Node(id)` / `Port(id)`) to actual nodes. Port-rooted edges (ER, class diagrams) resolve to the parent node's port-side anchor point; node-rooted edges resolve to a side midpoint chosen based on the relative positions of source and target after layout (e.g., right-side anchor when the target is to the east).

### Phase B — Base path

A first-pass orthogonal Manhattan path is produced. For an edge going from `(x₁, y₁)` to `(x₂, y₂)` in `LR` direction, the base path is typically three segments: right from source midpoint, down/up to target row, right to target midpoint. The Manhattan style produces clean documentation-style edges; the `spline` routing alternative produces Bezier curves through the same waypoints.

### Phase C — Obstacle test (CGA-backed)

Each segment is tested against the bounding boxes of nodes it might pass through. CGA intersection queries return precise intersection points; if a segment intersects a node, it's split at the intersection and re-routed around the obstacle by inserting one or two additional bend points. Without this pass, edges would happily clip through unrelated nodes on dense layouts.

### Phase D — Self-loop expansion

If `source == target`, the edge is replaced with a rectangular loop extending right (or down, depending on direction) and back into the same node. The loop's overshoot distance is `max(node_width, node_height) × 0.4`.

### Phase E — Parallel edge offset

When multiple edges connect the same `(source, target)` pair, each gets a lateral offset so they're visually distinguishable. With `edge_bundling = true`, edges past the bundle threshold are replaced with a single representative carrying a `×N` count label.

### Phase F — Reversed-edge tagging

Edges that were reversed during cycle removal are tagged `reversed: true` in the output `LayoutEdgePath`. Renderers consult this and draw them with a distinct style (typically a dashed stroke or a desaturated color), so back-edges are visually distinguishable from forward edges.

### Phase G — Boundary smoothing (incremental only)

When the layout was incrementally recomputed, any edge that crosses the boundary between dirty and clean subgraphs gets two passes of Laplacian smoothing applied to its interior waypoints. This removes the visual kinks that would otherwise appear at the boundary, without retouching the clean side.

---

## Pressure-tier semantics

The pressure-adaptive runtime classifies each render into one of four pressure tiers based on the global-budget broker's current state. The tier determines how aggressively the degradation operator algebra reduces visual fidelity.

| Tier | When you'll see it | What changes |
|---|---|---|
| `none` | Default for typical input | Full visual fidelity. No degradation. |
| `low` | Mildly large input or warm cache miss | Optional refinements (sifting, E-graph saturation) get tightened budgets. Visual output is identical |
| `medium` | Approaching guardrail thresholds | Drop shadows + glow effects disabled. Edge routing prefers straight lines over orthogonal where it doesn't change topology |
| `high` | Pathological input or post-guardrail fallback | Gradients off, accessibility attributes pared down, terminal tier forced to `compact`, minor labels truncated more aggressively, edge bundling activated |

Each degradation operator is **deterministic** — the same pressure tier produces the same degradation plan across runs. The plan is recorded in the layout decision ledger so consumers can verify which operators fired. The `degradation` CI gate exercises every operator under controlled pressure and validates the resulting output.

`MermaidPressureSource` distinguishes where the pressure came from: `parser` (parsing consumed too much budget), `layout` (layout estimates exceed remaining budget), `render` (the renderer reports backpressure), or `external` (a host injected pressure via `MermaidNativePressureSignals`).

---

## Custom font metrics

The default font model is a heuristic character-width classifier tuned for system-UI / sans-serif fonts. For environments where a real font renderer is available (server-side with FreeType, browser with `OffscreenCanvas`), you can supply custom metrics:

```rust
use fm_layout::{LayoutConfig, FontMetricsProvider};
use fm_core::FontMetrics;

struct MyFontMetrics;

impl FontMetricsProvider for MyFontMetrics {
    fn measure(&self, text: &str, font_size: f32) -> FontMetrics {
        // Call into your font renderer; return measured width + height
        FontMetrics { width: …, height: …, baseline_offset: … }
    }
}

let mut config = LayoutConfig::default();
config.font_metrics = Some(Box::new(MyFontMetrics));
let layout = layout_diagram_with_config(&ir, config);
```

When custom metrics are present, the layout engine consults them for every label measurement instead of using the built-in heuristic. The trade-off: better measurement accuracy in exchange for losing the property that the layout is pure-Rust (the renderer becomes the source of font truth). For deterministic CI use, leave the default heuristic engaged.

---

## Workspace conventions

Conventions enforced across the workspace, in addition to the standard `cargo fmt` / `cargo clippy` checks:

- **`#![forbid(unsafe_code)]`** in every crate. The `forbid` attribute is stronger than `deny` — it can't be reversed by inner `#[allow]` attributes.
- **`#![deny(clippy::pedantic, clippy::nursery)]`** baseline with targeted opt-outs where the lint produces churn. CI runs `cargo clippy --workspace --all-targets -- -D warnings` and is voting.
- **No `unwrap()` on values that can fail.** `unwrap` is allowed only on values whose `None` / `Err` branch is genuinely unreachable, and the comment must explain why. `expect("...")` is preferred when documentation of the invariant is useful.
- **No `println!` in libraries.** All logging goes through `tracing`. The CLI is the one exception — it may write to stdout directly for user-facing output.
- **No script-based code rewrites.** Refactors are done by hand or by `ast-grep` with explicit patterns. Regex-based rewrites have a track record of corrupting subtle code in this workspace.
- **No file proliferation.** New files are reserved for genuinely new functionality. Variants like `module_v2.rs`, `module_new.rs`, `module_improved.rs` are forbidden.
- **No backwards-compatibility shims.** The project is pre-1.0 and changes APIs freely; obsolete code is removed cleanly rather than wrapped.
- **Determinism gates are voting.** The `determinism` CI gate runs the same render N times and rejects any byte-difference. Any change that breaks determinism must explicitly justify the trade-off in the decision-contract ledger.

These conventions are enforced both socially (in `AGENTS.md`) and mechanically (via CI gates).

---

## Anatomy of a rendered SVG

A typical `fm-cli render input.mmd --format svg` output has this structure (elided for readability):

```svg
<svg xmlns="http://www.w3.org/2000/svg"
     viewBox="0 0 672 317"
     role="img"
     aria-labelledby="diagram-title diagram-desc"
     data-nodes="4" data-edges="4"
     data-fm-diagram-type="flowchart"
     data-fm-layout-algorithm="sugiyama"
     data-fm-theme="default">

  <title id="diagram-title">Process flow</title>
  <desc id="diagram-desc">Flowchart with 4 nodes and 4 edges. Starts at "Start", ends at "End".</desc>

  <defs>
    <linearGradient id="fm-grad-node-default-v">…</linearGradient>
    <filter id="fm-shadow-default">…</filter>
    <marker id="fm-arrow-default" markerWidth="10" markerHeight="10" …>…</marker>
  </defs>

  <style>
    :root {
      --fm-bg: #ffffff;
      --fm-text-color: #1f2937;
      --fm-node-fill: #f3f4f6;
      --fm-node-stroke: #6b7280;
      --fm-edge-color: #6b7280;
      --fm-cluster-fill: rgba(229, 231, 235, 0.4);
      /* …8 accent colors… */
    }
    .fm-node-rect { fill: url(#fm-grad-node-default-v); stroke: var(--fm-node-stroke); … }
    .fm-edge      { stroke: var(--fm-edge-color); fill: none; marker-end: url(#fm-arrow-default); }
    .fm-label     { font: 14px ui-sans-serif, system-ui; fill: var(--fm-text-color); }
    /* …animation keyframes, print rules… */
  </style>

  <g id="diagram-root" clip-path="url(#diagram-clip)">
    <g class="fm-cluster-layer">  <!-- empty here, no subgraphs -->  </g>

    <g class="fm-edge-layer">
      <g class="fm-edge"
         data-fm-source-span="3:5"
         data-from="Start" data-to="Parse">
        <path d="M 110 30 L 200 30 L 200 150 L 290 150" />
      </g>
      …
    </g>

    <g class="fm-node-layer">
      <g class="fm-node fm-node-stadium"
         data-id="Start"
         data-fm-source-span="2:3"
         transform="translate(10,5)">
        <rect class="fm-node-rect" width="100" height="50" rx="25" ry="25" />
      </g>
      …
    </g>

    <g class="fm-label-layer">
      <text class="fm-label" x="60" y="35" text-anchor="middle">Start</text>
      …
    </g>
  </g>
</svg>
```

A few things this lets a host do without re-rendering:

- Introspect the render from the root: `data-*` attributes carry node/edge counts, diagram type, chosen algorithm, and theme.
- Restyle on the fly. The `<defs>` and `<style>` blocks are shared across the diagram, and the CSS uses custom properties so dark-mode and `themeVariables` overrides at the host level take effect immediately.
- Animate or hide individual layers. Every visual element lives in one of four layer groups (`cluster`, `edge`, `node`, `label`), so opaquing the edge layer or animating clusters is a single CSS rule.
- Jump from rendered element to source. With `--embed-source-spans` (or `accessibility = true`), every node and edge group carries `data-fm-source-span="LINE:COL"` for click-to-source navigation.
- Filter by structure. `data-from` / `data-to` on edges make CSS selectors like `.fm-edge[data-from='Start']` trivial.

---

## Browser compatibility and WASM bundle

The WASM build targets browsers with baseline **`WebAssembly` v1** support. It uses no shared memory and no atomics, so `SharedArrayBuffer` is not required and no cross-origin-isolation headers are needed to load the bundle. Other Web APIs are optional and only matter for specific render paths:

| Surface | Required |
|---|---|
| WebAssembly module instantiation | All modern browsers (Chrome 57+, Firefox 52+, Safari 11+, Edge 16+) |
| `CanvasRenderingContext2d` text metrics with full-width Unicode | Chrome 49+, Firefox 87+, Safari 11.1+ |
| `OffscreenCanvas` (if using off-main-thread render) | Chrome 69+, Firefox 105+, Safari 16.4+ |
| Web Workers (for off-main-thread parse/layout) | All modern browsers |
| WebGPU (when the WebGPU renderer is wired) | Chrome 113+, Firefox 121+, Safari Tech Preview |

The pre-built bundle from the live demo loads cleanly under file-served-over-HTTP and via standard CDNs; no special headers required.

Bundle composition is documented in detail under "Building the WASM bundle" above. Quick reference for the default (no-FNX) build:

| Component | Raw | Gzipped |
|---|---|---|
| `frankenmermaid_bg.wasm` | ~1.05 MB | ~433 KB |
| `frankenmermaid.js` (wasm-bindgen glue) | ~53 KB | ~8 KB |
| `frankenmermaid.d.ts` (TypeScript types) | ~3.1 KB | ~1 KB |

The build enforces a 500 KB gzip ceiling on the `.wasm` artifact (`MAX_GZIP_BYTES` in `build-wasm.sh`); the FNX feature roughly doubles the WASM size and is excluded from the default build for that reason.

---

## What's in the conformance corpus

`crates/fm-cli/tests/frankentui_conformance_cases.json` pins ~26 fixtures across every diagram family the engine claims to support, paired with the expected behavior taken from the FrankenTUI reference. Each case carries:

- A short identifier (e.g., `flowchart_simple`, `sequence_notes`, `er_relationship`).
- The raw Mermaid input.
- An expected-structure section describing what the IR must contain (node count, presence of specific shapes, edge cardinalities, …) and what the SVG must contain (specific marker IDs, specific CSS classes, etc.).
- Optional notes documenting deliberate behavioral differences from upstream mermaid-js (these are surfaced through the `Compatibility` diagnostic category at runtime).

Adding a new conformance case takes one JSON entry; the harness picks it up automatically. The coverage matrix in [`FEATURE_PARITY.md`](FEATURE_PARITY.md) tracks which surfaces have fixture-backed coverage vs implementation-only coverage.

---

## About Contributions

> *About Contributions:* Please don't take this the wrong way, but I do not accept outside contributions for any of my projects. I simply don't have the mental bandwidth to review anything, and it's my name on the thing, so I'm responsible for any problems it causes; thus, the risk-reward is highly asymmetric from my perspective. I'd also have to worry about other "stakeholders," which seems unwise for tools I mostly make for myself for free. Feel free to submit issues, and even PRs if you want to illustrate a proposed fix, but know I won't merge them directly. Instead, I'll have Claude or Codex review submissions via `gh` and independently decide whether and how to address them. Bug reports in particular are welcome. Sorry if this offends, but I want to avoid wasted time and hurt feelings. I understand this isn't in sync with the prevailing open-source ethos that seeks community contributions, but it's the only way I can move at this velocity and keep my sanity.

## License

MIT License (with OpenAI/Anthropic Rider). See [LICENSE](LICENSE).
