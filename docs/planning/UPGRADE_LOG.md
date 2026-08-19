# Dependency Upgrade Log

**Date:** 2026-02-20  |  **Project:** frankenmermaid  |  **Language:** Rust

## Summary
- **Updated:** 14  |  **Skipped:** 0  |  **Failed:** 0  |  **Already current:** 4
- **Clippy fixes:** 18 `collapsible_if` lints fixed (nightly now enforces let-chain collapsing)
- **MSRV:** Updated from 1.85 to 1.95

## Toolchain

### Rust nightly: already tracking latest
- **Current:** nightly-2026-02-19 (1.95.0-nightly)
- `rust-toolchain.toml` uses `channel = "nightly"` (auto-tracks latest)
- MSRV bumped from 1.85 to 1.95

## Patch Updates (semver-compatible)

### anyhow: 1.0.98 → 1.0.102
- **Breaking:** None
- **Tests:** PASSED

### clap: 4.5.32 → 4.5.60
- **Breaking:** None
- **Tests:** PASSED

### js-sys: 0.3.77 → 0.3.85
- **Breaking:** None
- **Tests:** PASSED

### serde: 1.0.219 → 1.0.228
- **Breaking:** None
- **Tests:** PASSED

### serde_json: 1.0.140 → 1.0.149
- **Breaking:** None
- **Tests:** PASSED

### thiserror: 2.0.12 → 2.0.18
- **Breaking:** None
- **Tests:** PASSED

### tracing: 0.1.41 → 0.1.44
- **Breaking:** None
- **Tests:** PASSED

### tracing-subscriber: 0.3.19 → 0.3.22
- **Breaking:** None
- **Tests:** PASSED

### wasm-bindgen: 0.2.100 → 0.2.108
- **Breaking:** None
- **Tests:** PASSED

### web-sys: 0.3.77 → 0.3.85
- **Breaking:** None
- **Tests:** PASSED

### tempfile: 3.15 → 3.25
- **Breaking:** None
- **Tests:** PASSED

## Already Current
- serde-wasm-bindgen: 0.6.5 (latest)
- unicode-segmentation: 1.12.0 (latest)
- json5: 1.3.1 (latest, no 4.x exists)
- tiny_http: 0.12.0 (latest)

## Breaking Updates (all succeeded)

### notify: 6.1 → 8.2.0 (two major bumps)
- **Breaking:** Event type moved to notify-types, serialization changed, crossbeam feature renamed
- **Migration:** No code changes needed (our simple watcher usage is API-compatible)
- **Tests:** PASSED

### resvg: 0.44 → 0.47.0
- **Breaking:** Pre-1.0; tiny-skia bumped from 0.11 to 0.12
- **Migration:** No code changes needed
- **Tests:** PASSED

### usvg: 0.44 → 0.47.0
- **Breaking:** Paired with resvg
- **Migration:** No code changes needed
- **Tests:** PASSED

## Clippy Fixes

The nightly 1.95.0 clippy now enforces `collapsible_if` for let-chain patterns. Fixed 18 instances across:
- `fm-layout/src/lib.rs` (1)
- `fm-parser/src/dot_parser.rs` (1)
- `fm-parser/src/ir_builder.rs` (1)
- `fm-parser/src/mermaid_parser.rs` (5)
- `fm-render-canvas/src/renderer.rs` (2)
- `fm-render-term/src/minimap.rs` (1)
- `fm-render-term/src/renderer.rs` (1)
- `fm-render-svg/src/attributes.rs` (1)
- `fm-render-svg/src/lib.rs` (5)

All converted nested `if let` / `if` chains to idiomatic `if let ... && let ...` / `if let ... && condition` let-chains.
