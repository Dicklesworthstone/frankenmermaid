# AGENTS.md — frankenmermaid

> Guidelines for AI coding agents working in this Rust codebase.

---

## RULE 0 - THE FUNDAMENTAL OVERRIDE PREROGATIVE

If I tell you to do something, even if it goes against what follows below, YOU MUST LISTEN TO ME. I AM IN CHARGE, NOT YOU.

---

## RULE NUMBER 1: NO FILE DELETION

**YOU ARE NEVER ALLOWED TO DELETE A FILE WITHOUT EXPRESS PERMISSION.** Even a new file that you yourself created, such as a test code file. You have a horrible track record of deleting critically important files or otherwise throwing away tons of expensive work. As a result, you have permanently lost any and all rights to determine that a file or folder should be deleted.

**YOU MUST ALWAYS ASK AND RECEIVE CLEAR, WRITTEN PERMISSION BEFORE EVER DELETING A FILE OR FOLDER OF ANY KIND.**

---

## Irreversible Git & Filesystem Actions — DO NOT EVER BREAK GLASS

1. **Absolutely forbidden commands:** `git reset --hard`, `git clean -fd`, `rm -rf`, or any command that can delete or overwrite code/data must never be run unless the user explicitly provides the exact command and states, in the same message, that they understand and want the irreversible consequences.
2. **No guessing:** If there is any uncertainty about what a command might delete or overwrite, stop immediately and ask the user for specific approval.
3. **Safer alternatives first:** When cleanup or rollbacks are needed, request permission to use non-destructive options (`git status`, `git diff`, `git stash`, backups) before considering destructive actions.
4. **Mandatory explicit plan:** Even after explicit authorization, restate the command verbatim, list exactly what will be affected, and wait for confirmation.
5. **Document the confirmation:** If a destructive command is approved and run, record the exact authorization text, command executed, and timestamp in session notes/final report.

---

## Git Branch: ONLY Use `main`, NEVER `master`

**The default branch is `main`. The `master` branch exists only for legacy URL compatibility.**

- **All work happens on `main`**
- **Never reference `master` in code/docs**
- **After pushing to `main`, keep `master` synced:**

```bash
git push origin main:master
```

If you find `master` references in docs/scripts/config, update them to `main` unless explicitly required for legacy mirror behavior.

---

## FrankenMermaid Project Overview

`frankenmermaid` is a Rust-first Mermaid-compatible diagram engine with a shared IR pipeline for:

- CLI (`fm-cli`)
- SVG renderer (`fm-render-svg`)
- Terminal renderer (`fm-render-term`)
- Canvas/Web rendering surface (`fm-render-canvas`)
- WASM packaging (`fm-wasm`)

### Core Goal

Extract and harden diagram parsing/layout/rendering from FrankenTUI into a standalone, modular workspace that can:

- Parse Mermaid-like input robustly (best effort + diagnostics)
- Produce deterministic layouts
- Render high-quality output across targets
- Remain resilient to malformed real-world input

### Source-of-Truth Extraction Reference

When extraction parity questions come up, use the FrankenTUI sources as behavioral reference:

- `/dp/frankentui/crates/ftui-extras/src/mermaid.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_layout.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_render.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_diff.rs`
- `/dp/frankentui/crates/ftui-extras/src/mermaid_minimap.rs`
- `/dp/frankentui/crates/ftui-extras/src/diagram_layout.rs`
- `/dp/frankentui/crates/ftui-extras/src/diagram.rs`
- `/dp/frankentui/crates/ftui-extras/src/dot_parser.rs`
- `/dp/frankentui/crates/ftui-extras/src/canvas.rs`

### `legacy_mermaid_code/` Note

`legacy_mermaid_code/` is a format and compatibility reference corpus (including mermaid-js source/docs).

- It is **reference material**, not a direct port target.
- Do not cargo-cult JS internals where a clean Rust-native design is better.
- Use it for syntax/behavior edge-case validation only.

---

## Technical Architecture

### High-Level Pipeline

```text
Input text
  -> fm-parser (detect + parse + recovery + warnings)
  -> fm-core::MermaidDiagramIr
  -> fm-layout (deterministic layout + stats)
  -> Renderers:
     - fm-render-svg
     - fm-render-term
     - fm-render-canvas
  -> Surfaces:
     - fm-cli
     - fm-wasm
```

### Workspace Crate Map

| Crate | Responsibility |
|------|----------------|
| `fm-core` | Shared IR/types/config/errors/diagnostics |
| `fm-parser` | Diagram detection + Mermaid/DOT parsing + recovery |
| `fm-layout` | Layout pipeline, node/edge geometry, stats/trace |
| `fm-render-svg` | Zero-dependency SVG document/element/path/text/defs system + renderer |
| `fm-render-term` | Terminal rendering surface (currently minimal baseline) |
| `fm-render-canvas` | Canvas rendering surface (currently minimal baseline) |
| `fm-wasm` | WASM-facing API wrapper around parse/layout/render |
| `fm-cli` | CLI surface (`detect`, `parse`, `render`) |

### Key Files

| File | Purpose |
|------|---------|
| `Cargo.toml` | Workspace members/dependencies/release profile |
| `rust-toolchain.toml` | Nightly toolchain + components/target |
| `crates/fm-core/src/lib.rs` | Core IR and config/diagnostic contracts |
| `crates/fm-parser/src/mermaid_parser.rs` | Main Mermaid parser |
| `crates/fm-parser/src/dot_parser.rs` | DOT bridge parser |
| `crates/fm-layout/src/lib.rs` | Layout pipeline and layout stats |
| `crates/fm-render-svg/src/lib.rs` | SVG rendering orchestration |
| `crates/fm-cli/src/main.rs` | End-user CLI entrypoint |
| `crates/fm-wasm/src/lib.rs` | WASM API entrypoint |

---

## Toolchain: Rust & Cargo

We only use **Cargo** in this project.

- **Edition:** Rust 2024
- **Toolchain:** nightly (see `rust-toolchain.toml`)
- **Unsafe code:** forbidden (`#![forbid(unsafe_code)]`)

### Release Profile

```toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true
```

### Key Dependencies

| Crate | Purpose |
|------|---------|
| `serde`, `serde_json` | IR/config serialization and evidence output |
| `thiserror` | Error definitions |
| `clap` | CLI command parsing |
| `unicode-segmentation` | Robust grapheme/token handling in parser |
| `json5` | Mermaid init directive parsing fallback |
| `wasm-bindgen`, `js-sys`, `web-sys` | WASM/browser integration surface |
| `tracing`, `tracing-subscriber` | Instrumentation and debug logging |

---

## Code Editing Discipline

### No Script-Based Bulk Rewrites

**NEVER** run scripts that mass-edit code files via brittle regex transformations.

- Make code changes intentionally and review context.
- For repetitive simple edits: use careful, explicit tool-assisted edits.
- For nuanced changes: modify manually with full context.

### No File Proliferation

Prefer editing existing files in place.

Do **not** create clutter variants like:

- `layout_v2.rs`
- `parser_new.rs`
- `renderer_improved.rs`

Create new files only when introducing genuinely new modules that cannot cleanly fit existing structure.

---

## Backwards Compatibility

Early-stage project rule: prioritize correctness and architecture quality over compatibility shims.

- No temporary wrapper layers for deprecated APIs
- No dual-path legacy support unless explicitly requested
- Fix APIs directly and keep design clean

---

## Compiler Checks (CRITICAL)

After substantive changes, run:

```bash
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

Resolve failures properly; do not suppress warnings casually.

---

## Testing Strategy

Use multiple levels of validation:

### 1. Workspace Unit Tests

```bash
cargo test --workspace --all-targets
```

### 2. Focused Crate Tests

```bash
cargo test -p fm-core
cargo test -p fm-parser
cargo test -p fm-layout
cargo test -p fm-render-svg
cargo test -p fm-render-term
cargo test -p fm-render-canvas
cargo test -p fm-wasm
```

### 3. Parser/Renderer Smoke Checks

```bash
fm-cli detect "flowchart LR\nA-->B"
fm-cli parse "flowchart LR\nA-->B"
fm-cli render "flowchart LR\nA-->B"
```

### 4. Determinism Checks

For layout/render work, run the same input repeatedly and confirm stable output.

---

## CI/CD Pipeline

Workflow file: `.github/workflows/ci.yml`

### Jobs

| Job | Purpose |
|-----|---------|
| `check` | `fmt`, `clippy -D warnings`, `cargo test` |
| `wasm-build` | Install `wasm-pack`, build `fm-wasm` for `wasm32-unknown-unknown` |
| `coverage` | Run `cargo llvm-cov` and publish `lcov.info` artifact |

### Local Repro of CI Core

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
```

---

## Parser & Layout Notes

### Parser Principles

- Best-effort parse with warnings over hard failure when feasible
- Unsupported constructs should degrade gracefully with explicit diagnostics
- Preserve enough structure in IR for downstream layout/rendering

### Layout Principles

- Determinism is mandatory
- Stable tie-breaking for ranks/order where choices exist
- Preserve reproducible output for CI snapshot confidence

### Rendering Principles

- Keep output clean and inspectable
- Accessibility and metadata are part of correctness
- Avoid unsafe/unsanitized output paths

---

## Beads (`br`) Workflow

This repo uses `beads_rust` as task source of truth.

### Core Commands

```bash
br ready --json
br list --status open
br show <id>
br update <id> --status in_progress
br close <id> --reason "Completed"
br sync --flush-only
```

### Required Agent Behavior

1. Start with `br ready --json`
2. Claim exactly one actionable issue (`in_progress`)
3. Implement and validate
4. Close issue with clear reason
5. `br sync --flush-only` before handoff

Use issue IDs (`bd-###`) in commit messages and coordination threads.

---

## `bv` Prioritization (Robot Mode Only)

**Never run bare `bv` in agent sessions.**

Use:

```bash
bv --robot-triage
bv --robot-next
bv --robot-plan
```

Use triage recommendations to pick highest-impact unblocked work.

---

## MCP Agent Mail Coordination

When available, coordinate async with other agents:

1. Register/start session
2. Check inbox and acknowledge messages
3. Announce issue claim/start
4. Post completion notes

If Agent Mail tools are unavailable, continue productive local progress and record the blocker clearly.

---

## Session Protocol (Landing the Plane)

Before ending a work session:

1. Ensure claimed bead statuses are accurate
2. Run required quality gates for touched scope
3. Sync beads state

```bash
br sync --flush-only
```

4. Verify git state and provide a concise handoff summary (what changed, what passed, what remains)

If pushing/committing is explicitly requested, do not stop until push succeeds.

---

## Practical Guardrails

- Never revert or overwrite other agents' in-progress changes unless explicitly told.
- Treat unrelated working-tree changes as shared parallel work, not anomalies.
- Keep edits minimal, auditable, and aligned to the claimed bead.
- Prefer concrete evidence (test output, check output, file paths) over vague status updates.

---

## Note on Built-in TODO Functionality

If explicitly instructed by the user to use built-in TODO functionality, comply.

