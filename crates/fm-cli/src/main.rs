#![forbid(unsafe_code)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

//! FrankenMermaid CLI - render and validate Mermaid diagrams.
//!
//! # Commands
//!
//! - `render`: Convert Mermaid diagrams to SVG, PNG, or terminal output
//! - `parse`: Output diagram IR as JSON for tooling/debugging
//! - `detect`: Show detected diagram type and confidence
//! - `validate`: Check input for errors and report diagnostics
//! - `watch`: Re-render on file change (requires `watch` feature)
//! - `serve`: Start local HTTP server with live-reload playground (requires `serve` feature)

// mimalloc as the global allocator on native builds. The pipeline is allocation-heavy
// (parse interning, per-edge layout point vecs, the render output buffer); a full-pipeline
// profile put glibc malloc/free/consolidate at ~9% of the wide render, and swapping to
// mimalloc measured a deterministic ~9-10% pipeline instruction/time reduction with
// byte-identical output. Declaring a `#[global_allocator]` static is safe (no `unsafe`),
// so it coexists with `#![forbid(unsafe_code)]`. Native only; wasm keeps its own allocator.
#[cfg(not(target_arch = "wasm32"))]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "png")]
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use crossterm::style::{
    Attribute, Color, Print, ResetColor, SetAttribute, SetBackgroundColor, SetForegroundColor,
};
use crossterm::terminal::{
    self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{execute, queue};
use fm_core::{
    DiagramType, MermaidBudgetLedger, MermaidDiagramIr, MermaidGlyphMode,
    MermaidLayoutDecisionExplanation, MermaidLayoutDecisionLedger, MermaidLinkMode,
    MermaidNativePressureSignals, MermaidParseMode, MermaidPressureReport, MermaidTier,
    StructuredDiagnostic, capability_matrix, capability_matrix_json_pretty,
    mermaid_layout_guard_observability,
};
#[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
use fm_layout::fnx_diagnostics::{FnxAnalysisResults, FnxDiagnosticSeverity, analyze_structure};
use fm_layout::{
    CycleStrategy, EdgeRouting, LayoutAlgorithm, LayoutConfig, LayoutGuardrails, TracedLayout,
    build_layout_decision_ledger, build_layout_guard_report_with_pressure,
    layout_diagram_traced_with_config_and_guardrails, layout_source_map,
};
use fm_parser::{
    FlowchartBatchParsePlan, FlowchartBatchParseRef, FlowchartBatchParseScratch, ParserConfig,
    capture_format_complement, detect_type_with_confidence_and_config, first_significant_line,
    parse_evidence_json, parse_with_mode, parse_with_mode_and_config,
};
use fm_render_svg::{
    A11yConfig, CertifiedSvgBatchPrefix, SvgBatchRenderer, SvgBatchRendererSeed, SvgRenderConfig,
    ThemePreset, describe_diagram_with_layout, render_svg_with_layout,
};
use fm_render_term::{
    TermRenderConfig, diff_diagrams, render_diff_plain, render_diff_summary,
    render_diff_terminal_with_config, render_term_with_layout_and_config,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

const DEFAULT_MAX_INPUT_BYTES: usize = 5_000_000;

fn parse_positive_font_size_arg(value: &str) -> std::result::Result<f32, String> {
    let parsed = value
        .parse::<f32>()
        .map_err(|err| format!("invalid font size '{value}': {err}"))?;
    if parsed.is_finite() && parsed > 0.0 {
        Ok(parsed)
    } else {
        Err(format!(
            "invalid font size '{value}': expected a finite value greater than 0"
        ))
    }
}

fn parse_positive_dimension_arg(value: &str) -> std::result::Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|err| format!("invalid dimension '{value}': {err}"))?;
    if parsed > 0 {
        Ok(parsed)
    } else {
        Err(format!(
            "invalid dimension '{value}': expected an integer greater than 0"
        ))
    }
}

/// FrankenMermaid CLI - render and validate Mermaid diagrams.
#[derive(Debug, Parser)]
#[command(
    name = "fm-cli",
    version,
    about = "FrankenMermaid CLI - render and validate Mermaid diagrams",
    long_about = "A Rust-first Mermaid-compatible diagram engine.\n\n\
        Supports parsing, layout, and rendering of flowcharts, sequence diagrams,\n\
        class diagrams, and more."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,

    /// Config file path. If omitted, auto-discovers `./frankenmermaid.toml`
    /// and then `~/.config/frankenmermaid/config.toml`.
    #[arg(long, global = true)]
    config: Option<String>,

    /// Enable verbose logging (can be repeated for more detail: -v, -vv, -vvv)
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    /// Suppress all output except errors
    #[arg(short, long, global = true)]
    quiet: bool,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Render a Mermaid diagram to SVG, PNG, or terminal output.
    Render {
        /// Input file path or "-" for stdin. If omitted, reads from stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Parser support contract mode.
        #[arg(long, value_enum)]
        parse_mode: Option<ParseModeArg>,

        /// Requested layout algorithm family.
        #[arg(long, value_enum)]
        layout_algorithm: Option<LayoutAlgorithmArg>,

        /// Output format
        #[arg(short, long, value_enum)]
        format: Option<OutputFormat>,

        /// Theme name (default, dark, forest, neutral)
        #[arg(short, long)]
        theme: Option<String>,

        /// Font size in pixels.
        #[arg(long, value_parser = parse_positive_font_size_arg)]
        font_size: Option<f32>,

        /// Output file path. If omitted, writes to stdout.
        #[arg(short, long)]
        output: Option<String>,

        /// Output width (for PNG/terminal)
        #[arg(short = 'W', long, value_parser = parse_positive_dimension_arg)]
        width: Option<u32>,

        /// Output height (for PNG/terminal)
        #[arg(short = 'H', long, value_parser = parse_positive_dimension_arg)]
        height: Option<u32>,

        /// Output as JSON with metadata (timing, dimensions, etc.)
        /// Requires `--output` so stdout can remain machine-readable.
        #[arg(long)]
        json: bool,

        /// Embed source-span metadata attributes in SVG output.
        #[arg(long, default_value_t = false)]
        embed_source_spans: bool,

        /// Suppress embedded source-span metadata in SVG output.
        #[arg(long, default_value_t = false)]
        no_embed_source_spans: bool,

        /// Optional JSON artifact path mapping rendered SVG element IDs back to input spans.
        #[arg(long)]
        source_map_out: Option<String>,

        /// FNX integration mode (auto=feature-detect, enabled=force on, disabled=force off).
        #[arg(long, value_enum, default_value = "auto")]
        fnx_mode: FnxModeArg,

        /// FNX graph projection strategy for analysis algorithms.
        #[arg(long, value_enum, default_value = "undirected")]
        fnx_projection: FnxProjectionArg,

        /// FNX fallback behavior when analysis exceeds budget or fails.
        #[arg(long, value_enum, default_value = "graceful")]
        fnx_fallback: FnxFallbackArg,
    },

    /// Render many diagrams concurrently as ONE job.
    ///
    /// This is the capability mermaid-js structurally cannot match: it renders on a single
    /// JavaScript main thread, so a documentation site or CI batch is serialized no matter how
    /// many cores the machine has. Here every input is an independent pipeline, so the whole
    /// batch scales across physical cores. Per-diagram output is byte-identical to `render`;
    /// the only difference is that the configuration is resolved once for the batch instead of
    /// once per process, and the diagrams run concurrently.
    RenderBatch {
        /// Input file paths. Every path is rendered; "-" (stdin) is not accepted here.
        #[arg(required = true, num_args = 1..)]
        inputs: Vec<String>,

        /// Directory for rendered output. Created if missing. Each input `<stem>` becomes
        /// `<out-dir>/<stem>.<ext>`, so input basenames must be unique.
        #[arg(long)]
        out_dir: String,

        /// Worker threads for the batch. Defaults to available parallelism.
        /// `1` runs the batch serially through the identical code path.
        #[arg(long)]
        jobs: Option<usize>,

        /// Parser support contract mode.
        #[arg(long, value_enum)]
        parse_mode: Option<ParseModeArg>,

        /// Requested layout algorithm family.
        #[arg(long, value_enum)]
        layout_algorithm: Option<LayoutAlgorithmArg>,

        /// Output format. PNG is rejected: rasterization is not batch-safe here.
        #[arg(short, long, value_enum)]
        format: Option<OutputFormat>,

        /// Theme name (default, dark, forest, neutral)
        #[arg(short, long)]
        theme: Option<String>,

        /// Font size in pixels.
        #[arg(long, value_parser = parse_positive_font_size_arg)]
        font_size: Option<f32>,

        /// Emit one JSON summary line per input to stdout (path, bytes, status).
        #[arg(long)]
        json: bool,

        /// Continue after a failing input instead of stopping at the first error.
        /// Failures are always reported and always set a non-zero exit status.
        #[arg(long)]
        keep_going: bool,

        /// Disable the persistent unchanged-output cache for this batch.
        #[arg(long)]
        no_cache: bool,

        /// Assert that `--changed-input` names the complete set of source or output paths changed
        /// since the preceding successful batch. Unlisted entries may bypass per-file metadata.
        #[arg(long, conflicts_with = "no_cache")]
        trust_change_set: bool,

        /// Input whose source or materialized output changed since the preceding successful batch.
        /// Repeat for every change; requires `--trust-change-set`.
        #[arg(long, value_name = "PATH", requires = "trust_change_set")]
        changed_input: Vec<String>,

        /// Read complete change sets as newline-delimited JSON arrays from stdin and render one
        /// epoch per line without restarting this process. Requires `--trust-change-set`.
        #[arg(
            long,
            conflicts_with_all = ["no_cache", "changed_input"],
            requires = "trust_change_set"
        )]
        change_set_stdin: bool,

        /// Read one JSON object mapping batch input paths to their final UTF-8 source bodies,
        /// apply those source updates once, and render the coalesced final state as one job.
        #[arg(
            long,
            conflicts_with_all = [
                "no_cache",
                "changed_input",
                "change_set_stdin",
                "final_state_stream"
            ],
            requires = "trust_change_set"
        )]
        final_state_stdin: bool,

        /// Read newline-delimited final-state JSON objects and render one complete observable
        /// transaction per line without restarting this process. When final source, output, and
        /// acknowledgment materialization are all deferred to EOF, overwritten revisions are
        /// validated and coalesced so only the completed state is rendered.
        #[arg(
            long,
            conflicts_with_all = [
                "no_cache",
                "changed_input",
                "change_set_stdin",
                "final_state_stdin"
            ],
            requires = "trust_change_set"
        )]
        final_state_stream: bool,

        /// Hold each diagram's newest rendered bytes in memory and materialize only the final
        /// output tree when the change-set or final-state stream reaches EOF. This deletes
        /// transient writes for build systems that consume only the completed revision.
        #[arg(long)]
        final_output_only: bool,

        /// Hold each updated source body in memory and materialize only the final source files
        /// when a final-state stream reaches EOF. Transaction acknowledgments then certify the
        /// in-memory render state; callers that observe source files between ACKs must omit this.
        #[arg(long)]
        final_source_only: bool,

        /// Emit one aggregate acknowledgment after the final-state stream reaches EOF instead of
        /// flushing one acknowledgment per transaction. Callers that need only the completed job
        /// can pipeline every revision without a synchronous process round trip per input line.
        #[arg(long)]
        final_ack_only: bool,

        /// Assert that every non-empty final-state stream record is a complete snapshot containing
        /// every batch input. With all three final-only flags, superseded records remain
        /// length/UTF-8 framed but only the newest snapshot is JSON-decoded and rendered.
        #[arg(long)]
        complete_snapshot_stream: bool,

        /// FNX integration mode (auto=feature-detect, enabled=force on, disabled=force off).
        #[arg(long, value_enum, default_value = "auto")]
        fnx_mode: FnxModeArg,

        /// FNX graph projection strategy for analysis algorithms.
        #[arg(long, value_enum, default_value = "undirected")]
        fnx_projection: FnxProjectionArg,

        /// FNX fallback behavior when analysis exceeds budget or fails.
        #[arg(long, value_enum, default_value = "graceful")]
        fnx_fallback: FnxFallbackArg,
    },

    /// Parse a diagram and output its IR as JSON.
    Parse {
        /// Input file path or "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Parser support contract mode.
        #[arg(long, value_enum)]
        parse_mode: Option<ParseModeArg>,

        /// Output full IR (default is summary)
        #[arg(long)]
        full: bool,

        /// Pretty-print JSON output
        #[arg(long)]
        pretty: bool,
    },

    /// Detect the diagram type and show confidence information.
    Detect {
        /// Input file path or "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Output as JSON
        #[arg(long)]
        json: bool,
    },

    /// Compare two Mermaid diagrams and emit a diff.
    Diff {
        /// Old input file path, inline diagram text, or "-" for stdin.
        old_input: String,

        /// New input file path or inline diagram text.
        new_input: String,

        /// Parser support contract mode.
        #[arg(long, value_enum)]
        parse_mode: Option<ParseModeArg>,

        /// Diff output format.
        #[arg(long, value_enum, default_value = "terminal")]
        format: DiffOutputFormat,

        /// Color mode for terminal/summary output.
        #[arg(long, value_enum, default_value = "auto")]
        color: ColorChoice,

        /// Output width for side-by-side terminal rendering.
        #[arg(short = 'W', long, value_parser = parse_positive_dimension_arg)]
        width: Option<u32>,

        /// Output height for side-by-side terminal rendering.
        #[arg(short = 'H', long, value_parser = parse_positive_dimension_arg)]
        height: Option<u32>,

        /// Output file path. If omitted, writes to stdout.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Validate a diagram and report diagnostics.
    Validate {
        /// Input file path or "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Parser support contract mode.
        #[arg(long, value_enum)]
        parse_mode: Option<ParseModeArg>,

        /// Requested layout algorithm family for validation/layout evidence.
        #[arg(long, value_enum)]
        layout_algorithm: Option<LayoutAlgorithmArg>,

        /// Validation output format.
        #[arg(long, value_enum, default_value = "text")]
        format: ValidateOutputFormat,

        /// Exit with non-zero status when diagnostics at this severity (or higher) exist.
        #[arg(long, value_enum, default_value = "error")]
        fail_on: FailOnSeverity,

        /// Optional path to write machine-readable diagnostics JSON artifact.
        #[arg(long)]
        diagnostics_out: Option<String>,

        /// FNX integration mode (auto=feature-detect, enabled=force on, disabled=force off).
        #[arg(long, value_enum, default_value = "auto")]
        fnx_mode: FnxModeArg,

        /// FNX graph projection strategy for analysis algorithms.
        #[arg(long, value_enum, default_value = "undirected")]
        fnx_projection: FnxProjectionArg,

        /// FNX fallback behavior when analysis exceeds budget or fails.
        #[arg(long, value_enum, default_value = "graceful")]
        fnx_fallback: FnxFallbackArg,
    },

    /// Emit the executable capability claim matrix as JSON.
    Capabilities {
        /// Pretty-print JSON output.
        #[arg(long)]
        pretty: bool,

        /// Optional path to write the JSON artifact.
        #[arg(short, long)]
        output: Option<String>,
    },

    /// Emit a canonical layout determinism manifest for the embedded golden corpus.
    #[command(hide = true)]
    DeterminismManifest,

    /// Launch an interactive split-pane terminal editor with live diagram preview.
    Interactive {
        /// Input file path or "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Parser support contract mode.
        #[arg(long, value_enum)]
        parse_mode: Option<ParseModeArg>,

        /// Initial UI theme.
        #[arg(short, long)]
        theme: Option<String>,
    },

    /// Watch a file and re-render on changes (requires `watch` feature).
    #[cfg(feature = "watch")]
    Watch {
        /// Input file path to watch.
        input: String,

        /// Output format
        #[arg(short, long, value_enum, default_value = "term")]
        format: OutputFormat,

        /// Output file path. If omitted, writes to stdout.
        #[arg(short, long)]
        output: Option<String>,

        /// Clear screen before each render
        #[arg(long)]
        clear: bool,
    },

    /// Start a local HTTP server with live-reload playground (requires `serve` feature).
    #[cfg(feature = "serve")]
    Serve {
        /// Port to listen on
        #[arg(short, long, default_value = "8080")]
        port: u16,

        /// Host to bind to
        #[arg(long, default_value = "127.0.0.1")]
        host: String,

        /// Open browser automatically
        #[arg(long)]
        open: bool,
    },
}

/// Output format for render command.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    /// SVG vector graphics
    Svg,
    /// PNG raster image (requires `png` feature)
    Png,
    /// Terminal/ASCII art output
    Term,
    /// ASCII-only output (no Unicode box-drawing)
    Ascii,
}

/// Output format for validate command.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum ValidateOutputFormat {
    /// Human-readable text report.
    Text,
    /// Compact JSON.
    Json,
    /// Pretty-printed JSON.
    Pretty,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum DiffOutputFormat {
    Summary,
    Plain,
    Terminal,
    Json,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum ColorChoice {
    Auto,
    Always,
    Never,
}

/// Severity threshold used for CI validation failure gates.
#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum FailOnSeverity {
    None,
    Hint,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum ParseModeArg {
    Strict,
    Compat,
    Recover,
}

impl ParseModeArg {
    const fn to_core(self) -> MermaidParseMode {
        match self {
            Self::Strict => MermaidParseMode::Strict,
            Self::Compat => MermaidParseMode::Compat,
            Self::Recover => MermaidParseMode::Recover,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum, PartialEq, Eq)]
enum LayoutAlgorithmArg {
    Auto,
    Sugiyama,
    Force,
    Tree,
    Radial,
    Timeline,
    Gantt,
    Sankey,
    Kanban,
    Grid,
}

impl LayoutAlgorithmArg {
    const fn to_layout(self) -> LayoutAlgorithm {
        match self {
            Self::Auto => LayoutAlgorithm::Auto,
            Self::Sugiyama => LayoutAlgorithm::Sugiyama,
            Self::Force => LayoutAlgorithm::Force,
            Self::Tree => LayoutAlgorithm::Tree,
            Self::Radial => LayoutAlgorithm::Radial,
            Self::Timeline => LayoutAlgorithm::Timeline,
            Self::Gantt => LayoutAlgorithm::Gantt,
            Self::Sankey => LayoutAlgorithm::Sankey,
            Self::Kanban => LayoutAlgorithm::Kanban,
            Self::Grid => LayoutAlgorithm::Grid,
        }
    }
}

/// FNX integration mode for graph analysis features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum FnxModeArg {
    /// Automatically detect and use FNX when the feature is available.
    #[default]
    Auto,
    /// Force FNX integration on (error if unavailable).
    Enabled,
    /// Force FNX integration off (skip all FNX analysis).
    Disabled,
}

impl FnxModeArg {
    /// Check if FNX should be used based on mode and feature availability.
    #[must_use]
    fn should_use_fnx(self) -> bool {
        match self {
            Self::Auto => cfg!(all(
                feature = "fnx-integration",
                not(target_arch = "wasm32")
            )),
            Self::Enabled => true,
            Self::Disabled => false,
        }
    }

    /// Get string representation for logging.
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

/// FNX graph projection strategy for analysis algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum FnxProjectionArg {
    /// Use undirected graph projection (ignore edge direction).
    #[default]
    Undirected,
    /// Use directed graph projection (preserve edge direction).
    Directed,
    /// Automatically select projection based on diagram type.
    Auto,
}

impl FnxProjectionArg {
    /// Get string representation for logging.
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Undirected => "undirected",
            Self::Directed => "directed",
            Self::Auto => "auto",
        }
    }
}

/// FNX fallback behavior when analysis exceeds budget or fails.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
enum FnxFallbackArg {
    /// Gracefully fall back to baseline heuristics without error.
    #[default]
    Graceful,
    /// Fail with error instead of falling back (strict mode).
    Strict,
    /// Log warning but continue with fallback.
    Warn,
}

impl FnxFallbackArg {
    /// Get string representation for logging.
    #[must_use]
    const fn as_str(self) -> &'static str {
        match self {
            Self::Graceful => "graceful",
            Self::Strict => "strict",
            Self::Warn => "warn",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidConfigFile {
    core: FrankenmermaidCoreConfig,
    parser: FrankenmermaidParserConfig,
    layout: FrankenmermaidLayoutConfig,
    render: FrankenmermaidRenderConfig,
    svg: FrankenmermaidSvgConfig,
    term: FrankenmermaidTermConfig,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidCoreConfig {
    deterministic: Option<bool>,
    max_input_bytes: Option<usize>,
    fallback_on_error: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidParserConfig {
    intent_inference: Option<bool>,
    fuzzy_keyword_distance: Option<usize>,
    auto_close_delimiters: Option<bool>,
    create_placeholder_nodes: Option<bool>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidLayoutConfig {
    algorithm: Option<String>,
    cycle_strategy: Option<String>,
    node_spacing: Option<f32>,
    rank_spacing: Option<f32>,
    edge_routing: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidRenderConfig {
    default_format: Option<String>,
    show_back_edges: Option<bool>,
    reduced_motion: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidSvgConfig {
    theme: Option<String>,
    rounded_corners: Option<f32>,
    padding: Option<f32>,
    shadows: Option<bool>,
    gradients: Option<bool>,
    accessibility: Option<bool>,
    enable_links: Option<bool>,
    link_mode: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
#[serde(default, deny_unknown_fields)]
struct FrankenmermaidTermConfig {
    tier: Option<String>,
    unicode: Option<bool>,
    minimap: Option<bool>,
}

#[derive(Debug, Clone, Default)]
struct LoadedCliConfig {
    file: FrankenmermaidConfigFile,
}

impl FailOnSeverity {
    const fn rank(self) -> u8 {
        match self {
            Self::None => u8::MAX,
            Self::Hint => 1,
            Self::Info => 2,
            Self::Warning => 3,
            Self::Error => 4,
        }
    }
}

/// Result of rendering a diagram.
#[derive(Debug, Serialize)]
struct RenderResult {
    format: String,
    parse_mode: String,
    embedded_source_spans: bool,
    accessibility_summary: String,
    layout_requested: String,
    layout_selected: String,
    layout_guard_reason: String,
    layout_guard_fallback_applied: bool,
    layout_guard_time_budget_exceeded: bool,
    layout_guard_iteration_budget_exceeded: bool,
    layout_guard_route_budget_exceeded: bool,
    layout_guard_estimated_time_ms: usize,
    layout_guard_estimated_iterations: usize,
    layout_guard_estimated_route_ops: usize,
    layout_band_count: usize,
    layout_tick_count: usize,
    source_span_node_count: usize,
    source_span_edge_count: usize,
    source_span_cluster_count: usize,
    source_map_entry_count: usize,
    source_map_out: Option<String>,
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    pressure_source: String,
    pressure_tier: String,
    pressure_telemetry_available: bool,
    pressure_conservative_fallback: bool,
    pressure_score_permille: u16,
    trace_id: String,
    decision_id: String,
    policy_id: String,
    schema_version: String,
    layout_decision_ledger: MermaidLayoutDecisionLedger,
    layout_decision_explanation: MermaidLayoutDecisionExplanation,
    layout_decision_ledger_jsonl: String,
    budget_total_ms: u64,
    parse_budget_ms: u64,
    layout_budget_ms: u64,
    render_budget_ms: u64,
    budget_exhausted: bool,
    parse_used_ms: u64,
    layout_used_ms: u64,
    render_used_ms: u64,
    degradation_target_fidelity: String,
    degradation_reduce_decoration: bool,
    degradation_simplify_routing: bool,
    degradation_hide_labels: bool,
    degradation_collapse_clusters: bool,
    degradation_force_glyph_mode: Option<String>,
    output_bytes: usize,
    width: Option<u32>,
    height: Option<u32>,
    parse_time_ms: f64,
    layout_time_ms: f64,
    render_time_ms: f64,
    total_time_ms: f64,
    warnings: Vec<String>,
    // FNX witness metadata (additive, for fnx-assisted paths)
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_witness: Option<FnxWitness>,
}

/// FNX analysis witness metadata for telemetry and debugging.
#[derive(Debug, Clone, Serialize)]
struct FnxWitness {
    /// Whether FNX integration was enabled for this render.
    enabled: bool,
    /// Whether FNX analysis was actually used (may be disabled by config).
    used: bool,
    /// Projection mode used for graph analysis.
    projection_mode: String,
    /// List of algorithms that were invoked.
    algorithms_invoked: Vec<String>,
    /// Time spent in FNX analysis (microseconds).
    analysis_time_us: u64,
    /// Whether any analysis was budget-limited.
    budget_exceeded: bool,
    /// Fallback level if degradation occurred.
    fallback_level: String,
    /// Fallback reason code if degradation occurred.
    fallback_reason: String,
    /// Hash of the analysis results for determinism verification.
    results_hash: String,
}

/// Worker count for `render-batch` when `--jobs` is not given: **physical** cores, not logical.
///
/// `available_parallelism()` reports logical CPUs, so on an SMT machine the default asked for two
/// workers per physical core. Batch workers are compute-bound and cache-resident -- each one is
/// parsing, laying out and rendering a whole diagram -- so a pair of SMT siblings shares one core's
/// L1/L2 and execution units without adding execution resources, and the extra thread costs
/// scheduling, allocator arenas and cache pressure. Measured on a Threadripper PRO 5975WX
/// (32 physical / 64 logical), interleaved rounds, whole-job wall clock:
///
/// | job | `--jobs 32` | `--jobs 64` | 64 vs 32 |
/// |---|---:|---:|---|
/// | `docs_site_200` (200 docs) | 5.20 ms | 6.86 ms | **+34%** (median of 11 rounds; all 11 slower) |
/// | `ci_docs_2000` (2000 docs) | 26.9 ms | 28.9 ms | **+7%** (median of 7 rounds) |
///
/// The whole scaling curve peaks exactly at the physical core count: on `ci_docs_2000` the speedup
/// over one worker is 6.9x at 8, 12.4x at 16, **17.5x at 32**, then falls back to 14.6x at 64.
/// Output is byte-identical at every width (verified 1/8/16/32/64 over five corpus jobs), so this
/// only changes how long the same bytes take to produce.
///
/// An explicit `--jobs` is still honoured exactly as given, including values above the physical
/// core count -- this changes the default only.
fn default_batch_workers() -> usize {
    let logical = std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
    physical_core_count().map_or(logical, |physical| physical.clamp(1, logical))
}

/// Physical core count from Linux sysfs topology, or `None` when it cannot be determined.
///
/// Every logical CPU publishes the sibling set it shares a physical core with, so the number of
/// distinct sibling sets is the number of physical cores. Read rather than computed from a crate:
/// this is one directory walk at startup, once per batch. Any unreadable or unexpected topology
/// yields `None` and the caller keeps the previous `available_parallelism()` behaviour, which is
/// also what non-Linux targets get.
fn physical_core_count() -> Option<usize> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut sibling_sets = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir("/sys/devices/system/cpu").ok()? {
        let path = entry.ok()?.path();
        let is_cpu_dir = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| {
                name.starts_with("cpu") && name[3..].bytes().all(|b| b.is_ascii_digit())
            });
        if !is_cpu_dir {
            continue;
        }
        // `thread_siblings_list` is identical for every sibling of one core, so it doubles as the
        // core's identity without needing to parse the CPU numbers out of it.
        let Ok(siblings) = std::fs::read_to_string(path.join("topology/thread_siblings_list"))
        else {
            continue;
        };
        sibling_sets.insert(siblings.trim().to_string());
    }
    (!sibling_sets.is_empty()).then_some(sibling_sets.len())
}

#[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
fn fnx_results_hash(parts: &[&str]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{:016x}", hash)
}

#[derive(Debug)]
struct RenderOutcome {
    rendered: Vec<u8>,
    render_result: Option<RenderResult>,
}

const BATCH_RENDER_CACHE_VERSION: u32 = 1;
const BATCH_RENDER_CACHE_FILE: &str = ".frankenmermaid-batch-cache-v1.json";

#[derive(Debug)]
struct BatchCachePolicy<'a> {
    use_cache: bool,
    trust_change_set: bool,
    changed_inputs: &'a [String],
    source_overrides: Option<&'a std::collections::BTreeMap<String, String>>,
    session: Option<&'a mut BatchRenderCacheSession>,
    plan: Option<&'a BatchRenderPlan>,
    report: Option<BatchReportCarry<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BatchReportCarry<'a> {
    plan_key: &'a str,
    logical_input_count: usize,
    inherited_diagrams: usize,
    inherited_cache_hits: usize,
    inherited_total_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BatchRenderCacheEntry {
    key: String,
    #[serde(default)]
    source_digest: String,
    #[serde(default)]
    options_key: String,
    #[serde(default)]
    source_bytes: u64,
    #[serde(default)]
    source_modified_ns: String,
    bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BatchRenderCacheManifest {
    version: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clean_batch: Option<TrustedBatchSummary>,
    entries: std::collections::BTreeMap<String, BatchRenderCacheEntry>,
}

impl Default for BatchRenderCacheManifest {
    fn default() -> Self {
        Self {
            version: BATCH_RENDER_CACHE_VERSION,
            clean_batch: None,
            entries: std::collections::BTreeMap::new(),
        }
    }
}

#[derive(Debug)]
struct BatchRenderPlan {
    input_set: std::collections::BTreeSet<String>,
    input_indices: std::collections::BTreeMap<String, usize>,
    destinations: Vec<PathBuf>,
    destination_names: Vec<String>,
    destination_displays: Vec<String>,
    requested_workers: usize,
    cache_path: PathBuf,
    option_cache_digest: Option<String>,
    cache_active: bool,
    key: String,
}

impl BatchRenderPlan {
    fn new(
        inputs: &[String],
        out_dir: &str,
        jobs: Option<usize>,
        use_cache: bool,
        options: &RenderCommandOptions<'_>,
    ) -> Result<Self> {
        if options.format == OutputFormat::Png {
            anyhow::bail!(
                "render-batch does not support --format png; rasterization is not batch-safe here"
            );
        }
        if let Some(bad) = inputs.iter().find(|input| input.as_str() == "-") {
            anyhow::bail!(
                "render-batch reads files, not stdin; got {bad:?}. Pass explicit paths instead."
            );
        }

        let requested_workers = jobs.unwrap_or_else(default_batch_workers);
        if requested_workers == 0 {
            anyhow::bail!("--jobs must be at least 1");
        }

        std::fs::create_dir_all(out_dir)
            .with_context(|| format!("cannot create output directory {out_dir}"))?;
        report_executing_elf_sha256_once()?;

        let extension = batch_output_extension(options.format);
        let out_root = Path::new(out_dir);
        let mut destinations = Vec::with_capacity(inputs.len());
        let mut destination_names = Vec::with_capacity(inputs.len());
        let mut destination_displays = Vec::with_capacity(inputs.len());
        let mut stems: std::collections::BTreeMap<String, &str> = std::collections::BTreeMap::new();
        for input in inputs {
            let stem = Path::new(input)
                .file_stem()
                .map(|value| value.to_string_lossy().into_owned())
                .ok_or_else(|| anyhow::anyhow!("input {input:?} has no file name"))?;
            if let Some(previous) = stems.insert(stem.clone(), input.as_str()) {
                anyhow::bail!(
                    "inputs {previous:?} and {input:?} share basename {stem:?}; \
                     batch output would be order-dependent"
                );
            }
            let destination_name = format!("{stem}.{extension}");
            let destination = out_root.join(&destination_name);
            destination_names.push(destination_name);
            destination_displays.push(destination.display().to_string());
            destinations.push(destination);
        }

        let cache_path = out_root.join(BATCH_RENDER_CACHE_FILE);
        let executable_identity = use_cache
            .then(|| {
                let executable = std::env::current_exe().ok()?;
                let metadata = executable.metadata().ok()?;
                let modified = metadata
                    .modified()
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()?;
                Some(format!(
                    "{}:{}:{}",
                    env!("CARGO_PKG_VERSION"),
                    metadata.len(),
                    modified.as_nanos()
                ))
            })
            .flatten();
        let cache_active = use_cache && executable_identity.is_some();
        let option_cache_digest = executable_identity
            .as_ref()
            .map(|identity| sha256_hex(format!("{identity}\0{options:?}").as_bytes()));
        let key = sha256_hex(&serde_json::to_vec(&(
            inputs,
            out_dir,
            &option_cache_digest,
        ))?);

        Ok(Self {
            input_set: inputs.iter().cloned().collect(),
            input_indices: inputs
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, input)| (input, index))
                .collect(),
            destinations,
            destination_names,
            destination_displays,
            requested_workers,
            cache_path,
            option_cache_digest,
            cache_active,
            key,
        })
    }

    fn project(&self, inputs: &[String]) -> Result<Self> {
        let mut destinations = Vec::with_capacity(inputs.len());
        let mut destination_names = Vec::with_capacity(inputs.len());
        let mut destination_displays = Vec::with_capacity(inputs.len());
        for input in inputs {
            let index =
                self.input_indices.get(input).copied().ok_or_else(|| {
                    anyhow::anyhow!("cannot project unknown batch input {input:?}")
                })?;
            destinations.push(self.destinations[index].clone());
            destination_names.push(self.destination_names[index].clone());
            destination_displays.push(self.destination_displays[index].clone());
        }

        Ok(Self {
            input_set: inputs.iter().cloned().collect(),
            input_indices: inputs
                .iter()
                .cloned()
                .enumerate()
                .map(|(index, input)| (input, index))
                .collect(),
            destinations,
            destination_names,
            destination_displays,
            requested_workers: self.requested_workers,
            cache_path: self.cache_path.clone(),
            option_cache_digest: self.option_cache_digest.clone(),
            cache_active: self.cache_active,
            key: self.key.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct TrustedBatchSummary {
    plan_key: String,
    input_count: usize,
    total_bytes: usize,
}

// Persistent editors commonly revisit several diagrams while undoing or switching branches. Keep
// exact rendered revisions across that working set, but enforce both cardinality and byte ceilings
// so a pathological SVG cannot turn the process-local acceleration into unbounded retention.
const REVISION_OUTPUT_CACHE_MAX_ENTRIES: usize = 256;
const REVISION_OUTPUT_CACHE_MAX_BYTES: usize = 32 * 1024 * 1024;
const REVISION_OUTPUT_CACHE_CONTROL_ENTRIES: usize = 2;

#[derive(Debug)]
struct BatchRevisionOutput {
    bytes: Arc<Vec<u8>>,
    last_used: u64,
}

struct BatchRenderWorkerPool {
    threads: usize,
    pool: rayon::ThreadPool,
}

impl std::fmt::Debug for BatchRenderWorkerPool {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BatchRenderWorkerPool")
            .field("threads", &self.threads)
            .finish_non_exhaustive()
    }
}

impl BatchRenderWorkerPool {
    fn new(threads: usize) -> Result<Self> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .map_err(|error| {
                anyhow::anyhow!("cannot build {threads}-thread render pool: {error}")
            })?;
        Ok(Self { threads, pool })
    }
}

#[derive(Debug, Default)]
struct BatchRenderCacheSession {
    manifest: Option<BatchRenderCacheManifest>,
    manifest_modified: Option<std::time::SystemTime>,
    dirty: bool,
    trusted_batch: Option<TrustedBatchSummary>,
    certified_sources: std::collections::BTreeMap<String, BatchRenderCacheEntry>,
    elide_certified_source_writes: bool,
    pressure: OnceLock<Arc<MermaidPressureReport>>,
    reuse_pressure: bool,
    revision_outputs: std::collections::HashMap<String, BatchRevisionOutput>,
    revision_output_bytes: usize,
    revision_output_clock: u64,
    revision_output_max_entries: usize,
    revision_output_max_bytes: usize,
    reuse_revision_outputs: bool,
    reuse_complete_revision_transactions: bool,
    reuse_worker_pool: bool,
    worker_pool: std::sync::Mutex<Option<Arc<BatchRenderWorkerPool>>>,
    defer_output_writes: bool,
    deferred_outputs: std::collections::BTreeMap<PathBuf, Arc<Vec<u8>>>,
}

impl BatchRenderCacheSession {
    fn begin_stream(
        &mut self,
        cache_path: &Path,
        plan: Option<&BatchRenderPlan>,
        admit_clean_batch: bool,
    ) -> Result<()> {
        self.reuse_pressure = std::env::var_os("FM_DISABLE_SESSION_PRESSURE_SNAPSHOT").is_none();
        self.reuse_revision_outputs =
            std::env::var_os("FM_DISABLE_SESSION_REVISION_CACHE").is_none();
        self.reuse_complete_revision_transactions =
            std::env::var_os("FM_DISABLE_RESIDENT_TRANSACTION_REPLAY").is_none();
        self.reuse_worker_pool = std::env::var_os("FM_DISABLE_SESSION_WORKER_POOL").is_none();
        self.revision_output_max_entries =
            if std::env::var_os("FM_SESSION_REVISION_CACHE_TWO_ENTRY_CONTROL").is_some() {
                REVISION_OUTPUT_CACHE_CONTROL_ENTRIES
            } else {
                REVISION_OUTPUT_CACHE_MAX_ENTRIES
            };
        self.revision_output_max_bytes =
            if self.revision_output_max_entries == REVISION_OUTPUT_CACHE_CONTROL_ENTRIES {
                usize::MAX
            } else {
                REVISION_OUTPUT_CACHE_MAX_BYTES
            };
        let (mut manifest, mut modified) = load_batch_render_cache(cache_path);
        self.trusted_batch = admit_clean_batch
            .then(|| plan.and_then(|plan| trusted_batch_from_manifest(&manifest, plan)))
            .flatten();
        self.certified_sources.clear();
        if self.trusted_batch.is_some()
            && let Some(plan) = plan
        {
            for (input, &index) in &plan.input_indices {
                let Some(entry) = manifest.entries.get(&plan.destination_names[index]) else {
                    self.certified_sources.clear();
                    break;
                };
                self.certified_sources.insert(input.clone(), entry.clone());
            }
        }

        // A clean certificate is a transaction commit record. Remove it before this process can
        // touch any output, while retaining the admitted summary in memory. If the process dies,
        // the next process repairs in full; graceful EOF writes a fresh certificate in `flush`.
        if manifest.clean_batch.take().is_some() {
            let encoded = serde_json::to_vec(&manifest)?;
            std::fs::write(cache_path, encoded)
                .with_context(|| format!("cannot invalidate {}", cache_path.display()))?;
            modified = cache_path
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok();
        }
        self.manifest = Some(manifest);
        self.manifest_modified = modified;
        Ok(())
    }

    fn worker_pool(&self, threads: usize) -> Result<Arc<BatchRenderWorkerPool>> {
        let mut worker_pool = self
            .worker_pool
            .lock()
            .map_err(|_| anyhow::anyhow!("resident render worker pool is poisoned"))?;
        if let Some(pool) = worker_pool.as_ref().filter(|pool| pool.threads == threads) {
            return Ok(Arc::clone(pool));
        }
        let pool = Arc::new(BatchRenderWorkerPool::new(threads)?);
        *worker_pool = Some(Arc::clone(&pool));
        Ok(pool)
    }

    fn pressure_report(&self) -> Arc<MermaidPressureReport> {
        if !self.reuse_pressure {
            return Arc::new(MermaidNativePressureSignals::sample().into_report());
        }
        Arc::clone(
            self.pressure
                .get_or_init(|| Arc::new(MermaidNativePressureSignals::sample().into_report())),
        )
    }

    fn revision_output(&mut self, key: &str) -> Option<Arc<Vec<u8>>> {
        if !self.reuse_revision_outputs {
            return None;
        }
        if !self.revision_outputs.contains_key(key) {
            return None;
        }
        self.revision_output_clock = self.revision_output_clock.saturating_add(1);
        let entry = self.revision_outputs.get_mut(key)?;
        entry.last_used = self.revision_output_clock;
        Some(Arc::clone(&entry.bytes))
    }

    fn remember_revision_output(&mut self, key: String, bytes: Arc<Vec<u8>>) {
        if !self.reuse_revision_outputs
            || self.revision_output_max_entries == 0
            || bytes.len() > self.revision_output_max_bytes
        {
            return;
        }
        if let Some(previous) = self.revision_outputs.remove(&key) {
            self.revision_output_bytes = self
                .revision_output_bytes
                .saturating_sub(previous.bytes.len());
        }
        self.revision_output_clock = self.revision_output_clock.saturating_add(1);
        self.revision_output_bytes = self.revision_output_bytes.saturating_add(bytes.len());
        self.revision_outputs.insert(
            key,
            BatchRevisionOutput {
                bytes,
                last_used: self.revision_output_clock,
            },
        );
        while self.revision_outputs.len() > self.revision_output_max_entries
            || self.revision_output_bytes > self.revision_output_max_bytes
        {
            let Some(oldest_key) = self
                .revision_outputs
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(oldest) = self.revision_outputs.remove(&oldest_key) {
                self.revision_output_bytes = self
                    .revision_output_bytes
                    .saturating_sub(oldest.bytes.len());
            }
        }
    }

    fn replay_resident_transaction(
        &mut self,
        plan: &BatchRenderPlan,
        transaction: &PreparedBatchFinalState,
        defer_source_writes: bool,
    ) -> Result<Option<usize>> {
        let updates = &transaction.updates;
        if transaction.source_digests.len() != updates.len() {
            anyhow::bail!("resident transaction digest count does not match its update count");
        }
        if !self.reuse_complete_revision_transactions || updates.is_empty() {
            return Ok(None);
        }
        let Some(summary) = self.trusted_batch.as_ref().filter(|summary| {
            summary.plan_key == plan.key && summary.input_count == plan.input_indices.len()
        }) else {
            return Ok(None);
        };
        let previous_total_bytes = summary.total_bytes;
        let Some(options_key) = plan.option_cache_digest.as_ref() else {
            return Ok(None);
        };
        if self.manifest.is_none() {
            return Ok(None);
        }

        let mut old_bytes = 0usize;
        let mut new_bytes = 0usize;
        let mut replacements = Vec::with_capacity(updates.len());
        for ((input, source), source_digest) in
            updates.iter().zip(transaction.source_digests.iter())
        {
            let Some(index) = plan.input_indices.get(input).copied() else {
                return Ok(None);
            };
            let Some(previous_entry) = self
                .manifest
                .as_ref()
                .and_then(|manifest| manifest.entries.get(&plan.destination_names[index]))
                .filter(|entry| batch_cache_entry_matches_key(entry, options_key))
            else {
                return Ok(None);
            };
            let Some(previous_bytes) = usize::try_from(previous_entry.bytes).ok() else {
                return Ok(None);
            };
            let previous_source_modified_ns = previous_entry.source_modified_ns.clone();
            old_bytes = old_bytes
                .checked_add(previous_bytes)
                .ok_or_else(|| anyhow::anyhow!("resident transaction byte count overflow"))?;

            let key = format!("{source_digest}:{options_key}");
            let Some(rendered) = self.revision_output(&key) else {
                return Ok(None);
            };
            new_bytes = new_bytes
                .checked_add(rendered.len())
                .ok_or_else(|| anyhow::anyhow!("resident transaction byte count overflow"))?;
            let (source_bytes, source_modified_ns) = if defer_source_writes {
                (
                    u64::try_from(source.len())
                        .map_err(|_| anyhow::anyhow!("source is too large to cache"))?,
                    previous_source_modified_ns,
                )
            } else {
                let Ok(source_metadata) = Path::new(input).metadata() else {
                    return Ok(None);
                };
                let Some(source_modified_ns) = source_metadata
                    .modified()
                    .ok()
                    .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|duration| duration.as_nanos().to_string())
                else {
                    return Ok(None);
                };
                (source_metadata.len(), source_modified_ns)
            };
            let entry = BatchRenderCacheEntry {
                key,
                source_digest: source_digest.clone(),
                options_key: options_key.clone(),
                source_bytes,
                source_modified_ns,
                bytes: u64::try_from(rendered.len())
                    .map_err(|_| anyhow::anyhow!("rendered output is too large to cache"))?,
            };
            replacements.push((index, rendered, entry));
        }

        let total_bytes = previous_total_bytes
            .checked_sub(old_bytes)
            .and_then(|unchanged| unchanged.checked_add(new_bytes))
            .ok_or_else(|| anyhow::anyhow!("resident transaction byte accounting is invalid"))?;
        for (index, rendered, _) in &replacements {
            if !self.stage_output_if_deferred(&plan.destinations[*index], Arc::clone(rendered)) {
                std::fs::write(&plan.destinations[*index], rendered.as_slice()).with_context(
                    || format!("cannot write {}", plan.destinations[*index].display()),
                )?;
            }
        }

        let manifest = self
            .manifest
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("resident transaction lost its cache manifest"))?;
        for (index, _, entry) in replacements {
            let destination_name = &plan.destination_names[index];
            self.dirty |= manifest.entries.get(destination_name) != Some(&entry);
            manifest.entries.insert(destination_name.clone(), entry);
        }
        self.trusted_batch = Some(TrustedBatchSummary {
            plan_key: plan.key.clone(),
            input_count: plan.input_indices.len(),
            total_bytes,
        });
        Ok(Some(total_bytes))
    }

    fn materialize_deferred_sources(
        &mut self,
        plan: &BatchRenderPlan,
        sources: &std::collections::BTreeMap<String, String>,
    ) -> Result<(usize, usize, usize)> {
        let mut total_bytes = 0usize;
        let mut materialized_sources = 0usize;
        let mut certified_sources = 0usize;
        let mut refreshed_entries = Vec::with_capacity(sources.len());
        for (input, source) in sources {
            let index = plan.input_indices.get(input).copied().ok_or_else(|| {
                anyhow::anyhow!("cannot materialize unknown batch input {input:?}")
            })?;
            let expected_digest = sha256_hex(source.as_bytes());
            let certified_metadata = self
                .elide_certified_source_writes
                .then(|| {
                    let certificate = self.certified_sources.get(input)?;
                    if certificate.source_digest != expected_digest {
                        return None;
                    }
                    let metadata = Path::new(input).metadata().ok()?;
                    let source_modified_ns = metadata
                        .modified()
                        .ok()?
                        .duration_since(std::time::UNIX_EPOCH)
                        .ok()?
                        .as_nanos()
                        .to_string();
                    (certificate.source_bytes == metadata.len()
                        && certificate.source_modified_ns == source_modified_ns)
                        .then_some(metadata)
                })
                .flatten();
            let metadata = if let Some(metadata) = certified_metadata {
                certified_sources = certified_sources.saturating_add(1);
                metadata
            } else {
                std::fs::write(input, source.as_bytes())
                    .with_context(|| format!("cannot materialize final-state input {input}"))?;
                materialized_sources = materialized_sources.saturating_add(1);
                Path::new(input)
                    .metadata()
                    .with_context(|| format!("cannot inspect final-state input {input}"))?
            };
            let source_modified_ns = metadata
                .modified()
                .context("final source has no modification timestamp")?
                .duration_since(std::time::UNIX_EPOCH)
                .context("final source modification timestamp predates the Unix epoch")?
                .as_nanos()
                .to_string();
            refreshed_entries.push((index, expected_digest, metadata.len(), source_modified_ns));
            total_bytes = total_bytes
                .checked_add(source.len())
                .ok_or_else(|| anyhow::anyhow!("final source byte count overflow"))?;
        }

        let manifest = self.manifest.as_mut().ok_or_else(|| {
            anyhow::anyhow!("final source materialization lost its cache manifest")
        })?;
        let mut manifest_changed = false;
        for (index, expected_digest, source_bytes, source_modified_ns) in refreshed_entries {
            let entry = manifest
                .entries
                .get_mut(&plan.destination_names[index])
                .ok_or_else(|| anyhow::anyhow!("final source has no rendered cache entry"))?;
            if entry.source_digest != expected_digest {
                anyhow::bail!("final source digest does not match its rendered cache entry");
            }
            manifest_changed |= entry.source_bytes != source_bytes
                || entry.source_modified_ns != source_modified_ns;
            entry.source_bytes = source_bytes;
            entry.source_modified_ns = source_modified_ns;
        }
        self.dirty |= manifest_changed;
        Ok((materialized_sources, certified_sources, total_bytes))
    }

    fn stage_output_if_deferred(&mut self, destination: &Path, bytes: Arc<Vec<u8>>) -> bool {
        if !self.defer_output_writes {
            return false;
        }
        self.deferred_outputs.insert(destination.to_owned(), bytes);
        true
    }

    fn materialize_deferred_outputs(&mut self) -> Result<(usize, usize)> {
        let mut total_bytes = 0usize;
        for (destination, bytes) in &self.deferred_outputs {
            std::fs::write(destination, bytes.as_slice())
                .with_context(|| format!("cannot write {}", destination.display()))?;
            total_bytes = total_bytes.saturating_add(bytes.len());
        }
        let output_count = self.deferred_outputs.len();
        self.deferred_outputs.clear();
        Ok((output_count, total_bytes))
    }

    fn lease<'a>(&'a mut self, cache_path: &Path) -> BatchRenderCacheLease<'a> {
        if self.manifest.is_none() {
            let (manifest, modified) = load_batch_render_cache(cache_path);
            self.manifest = Some(manifest);
            self.manifest_modified = modified;
        }
        BatchRenderCacheLease {
            manifest: self.manifest.take().unwrap_or_default(),
            manifest_modified: self.manifest_modified,
            dirty: false,
            session: self,
        }
    }

    fn flush(&mut self, out_dir: &Path) -> Result<()> {
        let Some(manifest) = self.manifest.as_mut() else {
            anyhow::bail!("batch cache session still has an active manifest lease");
        };
        let certificate_changed = manifest.clean_batch != self.trusted_batch;
        if !self.dirty && !certificate_changed {
            return Ok(());
        }
        manifest.clean_batch.clone_from(&self.trusted_batch);
        let cache_path = out_dir.join(BATCH_RENDER_CACHE_FILE);
        let encoded = serde_json::to_vec(manifest)?;
        std::fs::write(&cache_path, encoded)
            .with_context(|| format!("cannot write {}", cache_path.display()))?;
        self.manifest_modified = cache_path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
        self.dirty = false;
        Ok(())
    }

    fn sparse_report_carry<'a>(
        &self,
        plan: &'a BatchRenderPlan,
        changed_inputs: &[String],
    ) -> Option<BatchReportCarry<'a>> {
        let summary = self.trusted_batch.as_ref()?;
        if summary.plan_key != plan.key || summary.input_count != plan.input_indices.len() {
            return None;
        }
        let manifest = self.manifest.as_ref()?;
        let options_key = plan.option_cache_digest.as_ref()?;
        let mut changed_bytes = 0usize;
        for input in changed_inputs {
            let index = *plan.input_indices.get(input)?;
            let entry = manifest.entries.get(&plan.destination_names[index])?;
            if !batch_cache_entry_matches_key(entry, options_key) {
                return None;
            }
            changed_bytes = changed_bytes.checked_add(usize::try_from(entry.bytes).ok()?)?;
        }
        Some(BatchReportCarry {
            plan_key: &plan.key,
            logical_input_count: summary.input_count,
            inherited_diagrams: summary.input_count.checked_sub(changed_inputs.len())?,
            inherited_cache_hits: summary.input_count.checked_sub(changed_inputs.len())?,
            inherited_total_bytes: summary.total_bytes.checked_sub(changed_bytes)?,
        })
    }
}

fn trusted_batch_from_manifest(
    manifest: &BatchRenderCacheManifest,
    plan: &BatchRenderPlan,
) -> Option<TrustedBatchSummary> {
    let summary = manifest.clean_batch.as_ref()?;
    if summary.plan_key != plan.key || summary.input_count != plan.destination_names.len() {
        return None;
    }
    let options_key = plan.option_cache_digest.as_ref()?;
    let mut total_bytes = 0usize;
    for destination_name in &plan.destination_names {
        let entry = manifest.entries.get(destination_name)?;
        if !batch_cache_entry_matches_key(entry, options_key) {
            return None;
        }
        total_bytes = total_bytes.checked_add(usize::try_from(entry.bytes).ok()?)?;
    }
    (total_bytes == summary.total_bytes).then(|| summary.clone())
}

struct BatchRenderCacheLease<'a> {
    session: &'a mut BatchRenderCacheSession,
    manifest: BatchRenderCacheManifest,
    manifest_modified: Option<std::time::SystemTime>,
    dirty: bool,
}

impl Drop for BatchRenderCacheLease<'_> {
    fn drop(&mut self) {
        self.session.manifest = Some(std::mem::take(&mut self.manifest));
        self.session.dirty |= self.dirty;
    }
}

fn load_batch_render_cache(
    cache_path: &Path,
) -> (BatchRenderCacheManifest, Option<std::time::SystemTime>) {
    let manifest = std::fs::read(cache_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<BatchRenderCacheManifest>(&bytes).ok())
        .filter(|manifest| manifest.version == BATCH_RENDER_CACHE_VERSION)
        .unwrap_or_default();
    let modified = cache_path
        .metadata()
        .and_then(|metadata| metadata.modified())
        .ok();
    (manifest, modified)
}

fn batch_cache_entry_matches_early(
    entry: &BatchRenderCacheEntry,
    options_key: &str,
    source_bytes: u64,
    source_modified_ns: &str,
    output_bytes: u64,
    output_modified: std::time::SystemTime,
    manifest_modified: std::time::SystemTime,
) -> bool {
    batch_cache_entry_matches_key(entry, options_key)
        && entry.source_bytes == source_bytes
        && entry.source_modified_ns == source_modified_ns
        && entry.bytes == output_bytes
        && output_modified <= manifest_modified
}

fn batch_cache_entry_matches_key(entry: &BatchRenderCacheEntry, options_key: &str) -> bool {
    entry.source_digest.len() == 64
        && entry
            .key
            .strip_prefix(&entry.source_digest)
            .and_then(|suffix| suffix.strip_prefix(':'))
            == Some(entry.options_key.as_str())
        && entry.options_key == options_key
}

#[cfg(test)]
mod batch_render_cache_tests {
    use super::{
        BatchRenderCacheEntry, BatchRenderCacheManifest, BatchRenderCacheSession, BatchRenderPlan,
        TrustedBatchSummary, batch_cache_entry_matches_early, batch_cache_entry_matches_key,
        parse_batch_change_set_line, parse_batch_final_state_payload, trusted_batch_from_manifest,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use std::time::{Duration, UNIX_EPOCH};

    fn entry() -> BatchRenderCacheEntry {
        BatchRenderCacheEntry {
            key: format!("{}:options", "a".repeat(64)),
            source_digest: "a".repeat(64),
            options_key: "options".to_owned(),
            source_bytes: 123,
            source_modified_ns: "456".to_owned(),
            bytes: 789,
        }
    }

    fn plan() -> BatchRenderPlan {
        BatchRenderPlan {
            input_set: ["a.mmd".to_owned(), "b.mmd".to_owned()]
                .into_iter()
                .collect(),
            input_indices: [("a.mmd".to_owned(), 0), ("b.mmd".to_owned(), 1)]
                .into_iter()
                .collect(),
            destinations: vec![PathBuf::from("out/a.svg"), PathBuf::from("out/b.svg")],
            destination_names: vec!["a.svg".to_owned(), "b.svg".to_owned()],
            destination_displays: vec!["out/a.svg".to_owned(), "out/b.svg".to_owned()],
            requested_workers: 2,
            cache_path: PathBuf::from("out/cache.json"),
            option_cache_digest: Some("options".to_owned()),
            cache_active: true,
            key: "batch-plan".to_owned(),
        }
    }

    #[test]
    fn unchanged_entry_is_admitted() {
        assert!(batch_cache_entry_matches_early(
            &entry(),
            "options",
            123,
            "456",
            789,
            UNIX_EPOCH + Duration::from_secs(1),
            UNIX_EPOCH + Duration::from_secs(2),
        ));
    }

    #[test]
    fn source_or_configuration_change_is_rejected() {
        assert!(!batch_cache_entry_matches_early(
            &entry(),
            "other-options",
            123,
            "456",
            789,
            UNIX_EPOCH,
            UNIX_EPOCH,
        ));
        assert!(!batch_cache_entry_matches_early(
            &entry(),
            "options",
            124,
            "456",
            789,
            UNIX_EPOCH,
            UNIX_EPOCH,
        ));
    }

    #[test]
    fn trusted_entry_still_requires_exact_binary_options_and_digest_key() {
        let mut cached = entry();
        assert!(batch_cache_entry_matches_key(&cached, "options"));
        assert!(!batch_cache_entry_matches_key(&cached, "other-options"));

        cached.key = format!("{}:options", "b".repeat(64));
        assert!(!batch_cache_entry_matches_key(&cached, "options"));
    }

    #[test]
    fn changed_or_newer_output_is_rejected() {
        assert!(!batch_cache_entry_matches_early(
            &entry(),
            "options",
            123,
            "456",
            790,
            UNIX_EPOCH,
            UNIX_EPOCH,
        ));
        assert!(!batch_cache_entry_matches_early(
            &entry(),
            "options",
            123,
            "456",
            789,
            UNIX_EPOCH + Duration::from_secs(2),
            UNIX_EPOCH + Duration::from_secs(1),
        ));
    }

    #[test]
    fn change_set_stream_accepts_arrays_and_ignores_blank_lines() {
        assert_eq!(parse_batch_change_set_line("   ", 1).unwrap(), None);
        assert_eq!(
            parse_batch_change_set_line(r#"["a.mmd","b.mmd"]"#, 2).unwrap(),
            Some(vec!["a.mmd".to_owned(), "b.mmd".to_owned()])
        );
    }

    #[test]
    fn change_set_stream_rejects_non_array_json_with_line_context() {
        let error = parse_batch_change_set_line(r#"{"changed":["a.mmd"]}"#, 7)
            .expect_err("objects are not complete change-set arrays");
        assert!(error.to_string().contains("input line 7"));
    }

    #[test]
    fn final_state_payload_accepts_only_known_bounded_inputs() {
        let inputs = vec!["a.mmd".to_owned(), "b.mmd".to_owned()];
        let updates = parse_batch_final_state_payload(
            r#"{"b.mmd":"flowchart LR\nB-->C","a.mmd":"flowchart LR\nA-->B"}"#,
            &inputs,
            64,
        )
        .unwrap();
        assert_eq!(updates.len(), 2);
        assert_eq!(updates["a.mmd"], "flowchart LR\nA-->B");

        let unknown =
            parse_batch_final_state_payload(r#"{"c.mmd":"A-->B"}"#, &inputs, 64).unwrap_err();
        assert!(
            unknown
                .to_string()
                .contains("not one of this batch's inputs")
        );

        let oversize =
            parse_batch_final_state_payload(r#"{"a.mmd":"12345"}"#, &inputs, 4).unwrap_err();
        assert!(
            oversize
                .to_string()
                .contains("exceeding the 4-byte input limit")
        );
    }

    #[test]
    fn complete_snapshot_requires_every_batch_input() {
        let inputs = vec!["a.mmd".to_owned(), "b.mmd".to_owned()];
        let prepared = super::prepare_complete_batch_final_state_payload(
            r#"{"a.mmd":"A-->B","b.mmd":"B-->C"}"#,
            &inputs,
            64,
        )
        .unwrap();
        assert_eq!(prepared.changed_inputs, inputs);

        let error =
            super::prepare_complete_batch_final_state_payload(r#"{"a.mmd":"A-->B"}"#, &inputs, 64)
                .unwrap_err();
        assert!(error.to_string().contains("contains 1 of 2 batch inputs"));
    }

    #[test]
    fn superseded_final_state_updates_keep_only_the_completed_revision() {
        let mut completed = std::collections::BTreeMap::new();
        super::merge_superseded_final_state_updates(
            &mut completed,
            [
                ("a.mmd".to_owned(), "flowchart LR\nA-->B".to_owned()),
                ("b.mmd".to_owned(), "flowchart LR\nB-->C".to_owned()),
            ]
            .into_iter()
            .collect(),
        );
        super::merge_superseded_final_state_updates(
            &mut completed,
            [
                ("a.mmd".to_owned(), "flowchart LR\nA-->Z".to_owned()),
                ("c.mmd".to_owned(), "flowchart LR\nC-->D".to_owned()),
            ]
            .into_iter()
            .collect(),
        );

        assert_eq!(completed.len(), 3);
        assert_eq!(completed["a.mmd"], "flowchart LR\nA-->Z");
        assert_eq!(completed["b.mmd"], "flowchart LR\nB-->C");
        assert_eq!(completed["c.mmd"], "flowchart LR\nC-->D");

        let prepared = super::prepare_batch_final_state_updates(completed);
        assert_eq!(prepared.changed_inputs, ["a.mmd", "b.mmd", "c.mmd"]);
        assert_eq!(prepared.source_digests.len(), 3);
    }

    #[test]
    fn final_state_coalescing_requires_every_observation_at_eof() {
        let completed_only = super::FinalStateStreamMaterialization {
            outputs_at_eof: true,
            sources_at_eof: true,
            acknowledgments_at_eof: true,
            complete_snapshots: false,
        };
        assert!(completed_only.exposes_only_completed_state());

        for materialization in [
            super::FinalStateStreamMaterialization {
                outputs_at_eof: false,
                ..completed_only
            },
            super::FinalStateStreamMaterialization {
                sources_at_eof: false,
                ..completed_only
            },
            super::FinalStateStreamMaterialization {
                acknowledgments_at_eof: false,
                ..completed_only
            },
        ] {
            assert!(!materialization.exposes_only_completed_state());
        }
    }

    #[test]
    fn resident_payload_cache_reuses_exact_prepared_transactions() {
        let inputs = vec!["a.mmd".to_owned(), "b.mmd".to_owned()];
        let payload = r#"{"b.mmd":"flowchart LR\nB-->C","a.mmd":"flowchart LR\nA-->B"}"#;
        let mut cache = super::ResidentPayloadCache::default();

        let first = cache.prepare(payload, &inputs, 64, true).unwrap();
        let second = cache.prepare(payload, &inputs, 64, true).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(cache.decoded, 1);
        assert_eq!(cache.reused, 1);
        assert_eq!(first.source_digests.len(), first.updates.len());
        assert_eq!(first.changed_inputs, ["a.mmd", "b.mmd"]);

        let mut disabled = super::ResidentPayloadCache::default();
        let first = disabled.prepare(payload, &inputs, 64, false).unwrap();
        let second = disabled.prepare(payload, &inputs, 64, false).unwrap();
        assert!(!Arc::ptr_eq(&first, &second));
        assert_eq!(disabled.decoded, 2);
        assert_eq!(disabled.reused, 0);
    }

    #[test]
    fn process_cache_lease_restores_updated_manifest() {
        let mut session = BatchRenderCacheSession {
            manifest: Some(BatchRenderCacheManifest::default()),
            ..BatchRenderCacheSession::default()
        };
        {
            let mut lease = session.lease(Path::new("unused-while-manifest-is-loaded"));
            lease
                .manifest
                .entries
                .insert("diagram.svg".to_owned(), entry());
            lease.dirty = true;
        }
        assert!(session.dirty);
        assert!(
            session
                .manifest
                .as_ref()
                .is_some_and(|manifest| manifest.entries.contains_key("diagram.svg"))
        );
    }

    #[test]
    fn sparse_epoch_carries_only_unlisted_diagrams() {
        let mut first = entry();
        first.bytes = 100;
        let mut second = entry();
        second.bytes = 250;
        let session = BatchRenderCacheSession {
            manifest: Some(BatchRenderCacheManifest {
                version: super::BATCH_RENDER_CACHE_VERSION,
                clean_batch: None,
                entries: [("a.svg".to_owned(), first), ("b.svg".to_owned(), second)]
                    .into_iter()
                    .collect(),
            }),
            trusted_batch: Some(TrustedBatchSummary {
                plan_key: "batch-plan".to_owned(),
                input_count: 2,
                total_bytes: 350,
            }),
            ..BatchRenderCacheSession::default()
        };
        let plan = plan();

        let carry = session
            .sparse_report_carry(&plan, &["b.mmd".to_owned()])
            .expect("validated batch can execute a changed-only epoch");
        assert_eq!(carry.logical_input_count, 2);
        assert_eq!(carry.inherited_diagrams, 1);
        assert_eq!(carry.inherited_cache_hits, 1);
        assert_eq!(carry.inherited_total_bytes, 100);
    }

    #[test]
    fn clean_certificate_is_admitted_then_invalidated_before_output() {
        let plan = plan();
        let summary = TrustedBatchSummary {
            plan_key: plan.key.clone(),
            input_count: 2,
            total_bytes: 350,
        };
        let mut first = entry();
        first.bytes = 100;
        let mut second = entry();
        second.bytes = 250;
        let manifest = BatchRenderCacheManifest {
            version: super::BATCH_RENDER_CACHE_VERSION,
            clean_batch: Some(summary.clone()),
            entries: [("a.svg".to_owned(), first), ("b.svg".to_owned(), second)]
                .into_iter()
                .collect(),
        };
        assert_eq!(
            trusted_batch_from_manifest(&manifest, &plan),
            Some(summary.clone())
        );

        let directory = tempfile::tempdir().unwrap();
        let cache_path = directory.path().join(super::BATCH_RENDER_CACHE_FILE);
        std::fs::write(&cache_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        let mut session = BatchRenderCacheSession::default();
        session
            .begin_stream(&cache_path, Some(&plan), true)
            .unwrap();

        assert_eq!(session.trusted_batch, Some(summary));
        let persisted: BatchRenderCacheManifest =
            serde_json::from_slice(&std::fs::read(cache_path).unwrap()).unwrap();
        assert_eq!(persisted.clean_batch, None);
    }

    #[test]
    fn projected_plan_preserves_parent_destinations_and_identity() {
        let plan = plan();
        let projected = plan.project(&["b.mmd".to_owned()]).unwrap();

        assert_eq!(
            projected.input_set,
            ["b.mmd".to_owned()].into_iter().collect()
        );
        assert_eq!(
            projected.input_indices,
            [("b.mmd".to_owned(), 0)].into_iter().collect()
        );
        assert_eq!(projected.destinations, [PathBuf::from("out/b.svg")]);
        assert_eq!(projected.destination_names, ["b.svg"]);
        assert_eq!(projected.destination_displays, ["out/b.svg"]);
        assert_eq!(projected.requested_workers, plan.requested_workers);
        assert_eq!(projected.cache_path, plan.cache_path);
        assert_eq!(projected.option_cache_digest, plan.option_cache_digest);
        assert_eq!(projected.cache_active, plan.cache_active);
        assert_eq!(projected.key, plan.key);
    }

    #[test]
    fn persistent_session_reuses_one_pressure_snapshot() {
        let session = BatchRenderCacheSession {
            reuse_pressure: true,
            ..BatchRenderCacheSession::default()
        };

        let first = session.pressure_report();
        let second = session.pressure_report();

        assert!(Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn persistent_session_reuses_one_fixed_width_worker_pool() {
        let session = BatchRenderCacheSession::default();

        let first = session.worker_pool(2).unwrap();
        let second = session.worker_pool(2).unwrap();

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(first.threads, 2);
    }

    #[test]
    fn persistent_session_keeps_two_exact_rendered_revisions() {
        let mut session = BatchRenderCacheSession {
            reuse_revision_outputs: true,
            revision_output_max_entries: 2,
            revision_output_max_bytes: usize::MAX,
            ..BatchRenderCacheSession::default()
        };
        let first = Arc::new(vec![1_u8]);
        let second = Arc::new(vec![2_u8]);
        let third = Arc::new(vec![3_u8]);

        session.remember_revision_output("first".to_owned(), Arc::clone(&first));
        session.remember_revision_output("second".to_owned(), Arc::clone(&second));
        assert!(
            session
                .revision_output("second")
                .is_some_and(|bytes| Arc::ptr_eq(&bytes, &second))
        );
        assert!(
            session
                .revision_output("first")
                .is_some_and(|bytes| Arc::ptr_eq(&bytes, &first))
        );

        session.remember_revision_output("third".to_owned(), Arc::clone(&third));
        assert!(session.revision_output("second").is_none());
        assert!(session.revision_output("first").is_some());
        assert!(
            session
                .revision_output("third")
                .is_some_and(|bytes| Arc::ptr_eq(&bytes, &third))
        );
    }

    #[test]
    fn persistent_revision_cache_enforces_its_byte_budget() {
        let mut session = BatchRenderCacheSession {
            reuse_revision_outputs: true,
            revision_output_max_entries: 8,
            revision_output_max_bytes: 3,
            ..BatchRenderCacheSession::default()
        };

        session.remember_revision_output("first".to_owned(), Arc::new(vec![1_u8; 2]));
        session.remember_revision_output("second".to_owned(), Arc::new(vec![2_u8; 2]));
        assert!(session.revision_output("first").is_none());
        assert!(session.revision_output("second").is_some());
        assert_eq!(session.revision_output_bytes, 2);

        session.remember_revision_output("oversize".to_owned(), Arc::new(vec![3_u8; 4]));
        assert!(session.revision_output("oversize").is_none());
        assert_eq!(session.revision_output_bytes, 2);
    }

    #[test]
    fn trusted_transaction_replays_exact_changed_revisions_without_rendering() {
        let directory = tempfile::tempdir().unwrap();
        let first_input = directory.path().join("a.mmd");
        let second_input = directory.path().join("b.mmd");
        let first_destination = directory.path().join("a.svg");
        let second_destination = directory.path().join("b.svg");
        std::fs::write(&first_input, "flowchart LR\nA-->B").unwrap();
        std::fs::write(&second_input, "flowchart LR\nB-->C").unwrap();
        let first_input_path = first_input.clone();

        let first_input = first_input.display().to_string();
        let second_input = second_input.display().to_string();
        let plan = BatchRenderPlan {
            input_set: [first_input.clone(), second_input.clone()]
                .into_iter()
                .collect(),
            input_indices: [(first_input.clone(), 0), (second_input.clone(), 1)]
                .into_iter()
                .collect(),
            destinations: vec![first_destination.clone(), second_destination],
            destination_names: vec!["a.svg".to_owned(), "b.svg".to_owned()],
            destination_displays: vec![
                first_destination.display().to_string(),
                directory.path().join("b.svg").display().to_string(),
            ],
            requested_workers: 2,
            cache_path: directory.path().join("cache.json"),
            option_cache_digest: Some("options".to_owned()),
            cache_active: true,
            key: "batch-plan".to_owned(),
        };
        let mut first_entry = entry();
        first_entry.bytes = 4;
        let mut second_entry = entry();
        second_entry.bytes = 5;
        let mut session = BatchRenderCacheSession {
            manifest: Some(BatchRenderCacheManifest {
                version: super::BATCH_RENDER_CACHE_VERSION,
                clean_batch: None,
                entries: [
                    ("a.svg".to_owned(), first_entry),
                    ("b.svg".to_owned(), second_entry),
                ]
                .into_iter()
                .collect(),
            }),
            trusted_batch: Some(TrustedBatchSummary {
                plan_key: plan.key.clone(),
                input_count: 2,
                total_bytes: 9,
            }),
            reuse_revision_outputs: true,
            reuse_complete_revision_transactions: true,
            revision_output_max_entries: 8,
            revision_output_max_bytes: usize::MAX,
            defer_output_writes: true,
            ..BatchRenderCacheSession::default()
        };
        let new_source = "flowchart LR\nA-->C";
        let digest = super::sha256_hex(new_source.as_bytes());
        let rendered = Arc::new(vec![7_u8; 10]);
        session.remember_revision_output(format!("{digest}:options"), Arc::clone(&rendered));
        let transaction = super::prepare_batch_final_state_updates(
            [(first_input, new_source.to_owned())].into_iter().collect(),
        );

        assert_eq!(
            session
                .replay_resident_transaction(&plan, &transaction, true)
                .unwrap(),
            Some(15)
        );
        assert_eq!(
            std::fs::read_to_string(&first_input_path).unwrap(),
            "flowchart LR\nA-->B"
        );
        assert!(
            session
                .deferred_outputs
                .get(&first_destination)
                .is_some_and(|bytes| Arc::ptr_eq(bytes, &rendered))
        );
        assert_eq!(
            session
                .manifest
                .as_ref()
                .unwrap()
                .entries
                .get("a.svg")
                .unwrap()
                .source_digest,
            digest
        );
        assert_eq!(session.trusted_batch.as_ref().unwrap().total_bytes, 15);
        assert!(session.dirty);
        assert_eq!(
            session
                .materialize_deferred_sources(&plan, &transaction.updates)
                .unwrap(),
            (1, 0, new_source.len())
        );
        assert_eq!(
            std::fs::read_to_string(first_input_path).unwrap(),
            new_source
        );
    }

    #[test]
    fn certified_source_materialization_skips_only_exact_disk_state() {
        let directory = tempfile::tempdir().unwrap();
        let input_path = directory.path().join("diagram.mmd");
        std::fs::write(&input_path, "alpha").unwrap();
        let input = input_path.display().to_string();
        let metadata = input_path.metadata().unwrap();
        let source_modified_ns = metadata
            .modified()
            .unwrap()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            .to_string();
        let source_digest = super::sha256_hex(b"alpha");
        let entry = BatchRenderCacheEntry {
            key: format!("{source_digest}:options"),
            source_digest,
            options_key: "options".to_owned(),
            source_bytes: metadata.len(),
            source_modified_ns,
            bytes: 1,
        };
        let plan = BatchRenderPlan {
            input_set: [input.clone()].into_iter().collect(),
            input_indices: [(input.clone(), 0)].into_iter().collect(),
            destinations: vec![directory.path().join("diagram.svg")],
            destination_names: vec!["diagram.svg".to_owned()],
            destination_displays: vec![directory.path().join("diagram.svg").display().to_string()],
            requested_workers: 1,
            cache_path: directory.path().join("cache.json"),
            option_cache_digest: Some("options".to_owned()),
            cache_active: true,
            key: "batch-plan".to_owned(),
        };
        let mut session = BatchRenderCacheSession {
            manifest: Some(BatchRenderCacheManifest {
                version: super::BATCH_RENDER_CACHE_VERSION,
                clean_batch: None,
                entries: [("diagram.svg".to_owned(), entry.clone())]
                    .into_iter()
                    .collect(),
            }),
            certified_sources: [(input.clone(), entry)].into_iter().collect(),
            elide_certified_source_writes: true,
            ..BatchRenderCacheSession::default()
        };
        let mut sources = [(input.clone(), "alpha".to_owned())].into_iter().collect();

        assert_eq!(
            session
                .materialize_deferred_sources(&plan, &sources)
                .unwrap(),
            (0, 1, 5)
        );

        sources.insert(input.clone(), "omega".to_owned());
        let changed_digest = super::sha256_hex(b"omega");
        let changed_entry = session
            .manifest
            .as_mut()
            .unwrap()
            .entries
            .get_mut("diagram.svg")
            .unwrap();
        changed_entry.key = format!("{changed_digest}:options");
        changed_entry.source_digest = changed_digest;
        assert_eq!(
            session
                .materialize_deferred_sources(&plan, &sources)
                .unwrap(),
            (1, 0, 5)
        );
        assert_eq!(std::fs::read_to_string(input_path).unwrap(), "omega");
    }

    #[test]
    fn final_output_transaction_materializes_only_the_newest_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let destination = directory.path().join("diagram.svg");
        let mut session = BatchRenderCacheSession {
            defer_output_writes: true,
            ..BatchRenderCacheSession::default()
        };

        assert!(session.stage_output_if_deferred(&destination, Arc::new(vec![1_u8; 8])));
        assert!(session.stage_output_if_deferred(&destination, Arc::new(vec![2_u8; 3])));
        assert!(!destination.exists());

        assert_eq!(session.materialize_deferred_outputs().unwrap(), (1, 3));
        assert_eq!(std::fs::read(destination).unwrap(), vec![2_u8; 3]);
        assert!(session.deferred_outputs.is_empty());
    }
}

#[derive(Debug, Clone)]
struct RenderCommandOptions<'a> {
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    layout_algorithm: LayoutAlgorithm,
    layout_config: LayoutConfig,
    format: OutputFormat,
    theme: &'a str,
    font_size: Option<f32>,
    output: Option<&'a str>,
    max_input_bytes: usize,
    svg_base_config: SvgRenderConfig,
    term_base_config: TermRenderConfig,
    show_back_edges: bool,
    show_minimap: bool,
    embed_source_spans: bool,
    source_map_out: Option<&'a str>,
    dimensions: (Option<u32>, Option<u32>),
    json_output: bool,
    // FNX integration controls
    fnx_mode: FnxModeArg,
    fnx_projection: FnxProjectionArg,
    fnx_fallback: FnxFallbackArg,
}

#[derive(Debug, Clone, Copy)]
struct DiffCommandOptions<'a> {
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    format: DiffOutputFormat,
    color: ColorChoice,
    max_input_bytes: usize,
    dimensions: (Option<u32>, Option<u32>),
    output: Option<&'a str>,
}

#[derive(Debug, Clone)]
struct RenderSurfaceOptions<'a> {
    theme: &'a str,
    font_size: Option<f32>,
    svg_base_config: SvgRenderConfig,
    term_base_config: TermRenderConfig,
    show_back_edges: bool,
    show_minimap: bool,
    embed_source_spans: bool,
    dimensions: (Option<u32>, Option<u32>),
    degradation: fm_core::MermaidDegradationPlan,
}

#[derive(Debug, Clone)]
struct ValidateCommandOptions<'a> {
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    layout_algorithm: LayoutAlgorithm,
    layout_config: LayoutConfig,
    format: ValidateOutputFormat,
    fail_on: FailOnSeverity,
    diagnostics_out: Option<&'a str>,
    max_input_bytes: usize,
    svg_base_config: SvgRenderConfig,
    show_back_edges: bool,
    // FNX integration controls
    fnx_mode: FnxModeArg,
    fnx_projection: FnxProjectionArg,
    fnx_fallback: FnxFallbackArg,
}

/// Result of detecting diagram type.
#[derive(Debug, Serialize)]
struct DetectResult {
    diagram_type: String,
    confidence: String,
    support_level: String,
    first_line: String,
    detection_method: String,
}

/// Result of validating a diagram.
#[derive(Debug, Serialize)]
#[allow(clippy::struct_excessive_bools)]
struct ValidateResult {
    valid: bool,
    parse_mode: String,
    accessibility_summary: String,
    layout_requested: String,
    layout_selected: String,
    layout_guard_reason: String,
    layout_guard_fallback_applied: bool,
    layout_guard_time_budget_exceeded: bool,
    layout_guard_iteration_budget_exceeded: bool,
    layout_guard_route_budget_exceeded: bool,
    layout_guard_estimated_time_ms: usize,
    layout_guard_estimated_iterations: usize,
    layout_guard_estimated_route_ops: usize,
    layout_band_count: usize,
    layout_tick_count: usize,
    source_span_node_count: usize,
    source_span_edge_count: usize,
    source_span_cluster_count: usize,
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    pressure_source: String,
    pressure_tier: String,
    pressure_telemetry_available: bool,
    pressure_conservative_fallback: bool,
    pressure_score_permille: u16,
    trace_id: String,
    decision_id: String,
    policy_id: String,
    schema_version: String,
    layout_decision_ledger: MermaidLayoutDecisionLedger,
    layout_decision_explanation: MermaidLayoutDecisionExplanation,
    layout_decision_ledger_jsonl: String,
    budget_total_ms: u64,
    parse_budget_ms: u64,
    layout_budget_ms: u64,
    render_budget_ms: u64,
    budget_exhausted: bool,
    parse_used_ms: u64,
    layout_used_ms: u64,
    render_used_ms: u64,
    degradation_target_fidelity: String,
    degradation_reduce_decoration: bool,
    degradation_simplify_routing: bool,
    degradation_hide_labels: bool,
    degradation_collapse_clusters: bool,
    degradation_force_glyph_mode: Option<String>,
    diagnostics: Vec<ValidationDiagnostic>,
    // FNX structural analysis (when fnx-integration feature is enabled)
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_component_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_is_connected: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_articulation_point_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_bridge_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_cycle_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fnx_witness: Option<FnxWitness>,
}

#[derive(Debug, Clone, Serialize)]
struct ValidationDiagnostic {
    stage: String,
    #[serde(flatten)]
    payload: StructuredDiagnostic,
}

#[derive(Debug, Clone, Serialize)]
struct DeterminismManifest {
    version: u8,
    target_arch: &'static str,
    target_os: &'static str,
    target_env: &'static str,
    case_count: usize,
    corpus_sha256: String,
    cases: Vec<DeterminismManifestCase>,
}

#[derive(Debug, Clone, Serialize)]
struct DeterminismManifestCase {
    case_id: &'static str,
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    layout_width: f64,
    layout_height: f64,
    non_finite_value_count: usize,
    subnormal_value_count: usize,
    layout_sha256: String,
}

const DETERMINISM_CASES: [(&str, &str); 10] = [
    (
        "dense_flowchart_stress",
        include_str!("../tests/golden/dense_flowchart_stress.mmd"),
    ),
    (
        "flowchart_simple",
        include_str!("../tests/golden/flowchart_simple.mmd"),
    ),
    (
        "flowchart_cycle",
        include_str!("../tests/golden/flowchart_cycle.mmd"),
    ),
    (
        "fuzzy_keyword_recovery",
        include_str!("../tests/golden/fuzzy_keyword_recovery.mmd"),
    ),
    (
        "sequence_basic",
        include_str!("../tests/golden/sequence_basic.mmd"),
    ),
    (
        "class_basic",
        include_str!("../tests/golden/class_basic.mmd"),
    ),
    (
        "state_basic",
        include_str!("../tests/golden/state_basic.mmd"),
    ),
    (
        "gantt_basic",
        include_str!("../tests/golden/gantt_basic.mmd"),
    ),
    ("pie_basic", include_str!("../tests/golden/pie_basic.mmd")),
    (
        "malformed_recovery",
        include_str!("../tests/golden/malformed_recovery.mmd"),
    ),
];

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose, cli.quiet);
    let loaded_config = load_cli_config(cli.config.as_deref())?;
    let max_input_bytes = resolve_max_input_bytes(&loaded_config.file)?;
    let parser_config = build_parser_config(&loaded_config.file);

    match cli.command {
        Command::Render {
            input,
            parse_mode,
            layout_algorithm,
            format,
            theme,
            font_size,
            output,
            width,
            height,
            json,
            embed_source_spans,
            no_embed_source_spans,
            source_map_out,
            fnx_mode,
            fnx_projection,
            fnx_fallback,
        } => {
            let format = resolve_output_format(format, &loaded_config.file)?;
            let layout_algorithm = resolve_layout_algorithm(layout_algorithm, &loaded_config.file)?;
            let theme = resolve_theme_name(theme, &loaded_config.file);
            let layout_config = build_layout_config(&loaded_config.file, font_size)?;
            let svg_base_config = build_base_svg_render_config(&loaded_config.file)?;
            let term_base_config = build_base_term_render_config(&loaded_config.file)?;
            let show_back_edges = resolve_show_back_edges(&loaded_config.file);
            let show_minimap = term_base_config.show_minimap;
            // Log FNX configuration at debug level
            debug!(
                fnx_mode = fnx_mode.as_str(),
                fnx_projection = fnx_projection.as_str(),
                fnx_fallback = fnx_fallback.as_str(),
                fnx_available = cfg!(all(
                    feature = "fnx-integration",
                    not(target_arch = "wasm32")
                )),
                "FNX configuration"
            );

            cmd_render(
                &input,
                RenderCommandOptions {
                    parse_mode: resolve_parse_mode(parse_mode, &loaded_config.file),
                    parser_config,
                    layout_algorithm,
                    layout_config,
                    format,
                    theme: &theme,
                    font_size,
                    output: output.as_deref(),
                    max_input_bytes,
                    svg_base_config,
                    term_base_config,
                    show_back_edges,
                    show_minimap,
                    embed_source_spans: if no_embed_source_spans {
                        false
                    } else {
                        embed_source_spans || format == OutputFormat::Svg
                    },
                    source_map_out: source_map_out.as_deref(),
                    dimensions: (width, height),
                    json_output: json,
                    fnx_mode,
                    fnx_projection,
                    fnx_fallback,
                },
            )
        }

        Command::RenderBatch {
            inputs,
            out_dir,
            jobs,
            parse_mode,
            layout_algorithm,
            format,
            theme,
            font_size,
            json,
            keep_going,
            no_cache,
            trust_change_set,
            changed_input,
            change_set_stdin,
            final_state_stdin,
            final_state_stream,
            final_output_only,
            final_source_only,
            final_ack_only,
            complete_snapshot_stream,
            fnx_mode,
            fnx_projection,
            fnx_fallback,
        } => {
            let format = resolve_output_format(format, &loaded_config.file)?;
            let layout_algorithm = resolve_layout_algorithm(layout_algorithm, &loaded_config.file)?;
            let theme = resolve_theme_name(theme, &loaded_config.file);
            let layout_config = build_layout_config(&loaded_config.file, font_size)?;
            let svg_base_config = build_base_svg_render_config(&loaded_config.file)?;
            let term_base_config = build_base_term_render_config(&loaded_config.file)?;
            let show_back_edges = resolve_show_back_edges(&loaded_config.file);
            let show_minimap = term_base_config.show_minimap;
            let options = RenderCommandOptions {
                parse_mode: resolve_parse_mode(parse_mode, &loaded_config.file),
                parser_config,
                layout_algorithm,
                layout_config,
                format,
                theme: &theme,
                font_size,
                // Batch writes one file per input; the shared `output` slot is unused.
                output: None,
                max_input_bytes,
                svg_base_config,
                term_base_config,
                show_back_edges,
                show_minimap,
                embed_source_spans: format == OutputFormat::Svg,
                source_map_out: None,
                dimensions: (None, None),
                json_output: false,
                fnx_mode,
                fnx_projection,
                fnx_fallback,
            };
            if final_output_only && !change_set_stdin && !final_state_stream {
                anyhow::bail!(
                    "--final-output-only requires --change-set-stdin or --final-state-stream"
                );
            }
            if final_source_only && !final_state_stream {
                anyhow::bail!("--final-source-only requires --final-state-stream");
            }
            if final_ack_only && !final_state_stream {
                anyhow::bail!("--final-ack-only requires --final-state-stream");
            }
            if final_ack_only && json {
                anyhow::bail!("--final-ack-only cannot be combined with --json");
            }
            if complete_snapshot_stream
                && !(final_state_stream && final_output_only && final_source_only && final_ack_only)
            {
                anyhow::bail!(
                    "--complete-snapshot-stream requires --final-state-stream, \
                     --final-output-only, --final-source-only, and --final-ack-only"
                );
            }
            if final_state_stream {
                cmd_render_batch_final_state_transaction_stream(
                    &inputs,
                    &out_dir,
                    jobs,
                    keep_going,
                    json,
                    FinalStateStreamMaterialization {
                        outputs_at_eof: final_output_only,
                        sources_at_eof: final_source_only,
                        acknowledgments_at_eof: final_ack_only,
                        complete_snapshots: complete_snapshot_stream,
                    },
                    options,
                )
            } else if final_state_stdin {
                cmd_render_batch_final_state_stream(
                    &inputs, &out_dir, jobs, keep_going, json, options,
                )
            } else if change_set_stdin {
                cmd_render_batch_change_set_stream(
                    &inputs,
                    &out_dir,
                    jobs,
                    keep_going,
                    json,
                    final_output_only,
                    options,
                )
            } else {
                cmd_render_batch(
                    &inputs,
                    &out_dir,
                    jobs,
                    keep_going,
                    json,
                    BatchCachePolicy {
                        use_cache: !no_cache,
                        trust_change_set,
                        changed_inputs: &changed_input,
                        source_overrides: None,
                        session: None,
                        plan: None,
                        report: None,
                    },
                    options,
                )
            }
        }

        Command::Parse {
            input,
            parse_mode,
            full,
            pretty,
        } => cmd_parse(
            &input,
            resolve_parse_mode(parse_mode, &loaded_config.file),
            parser_config,
            full,
            pretty,
            max_input_bytes,
        ),

        Command::Detect { input, json } => cmd_detect(&input, json, max_input_bytes, parser_config),

        Command::Diff {
            old_input,
            new_input,
            parse_mode,
            format,
            color,
            width,
            height,
            output,
        } => cmd_diff(
            &old_input,
            &new_input,
            DiffCommandOptions {
                parse_mode: resolve_parse_mode(parse_mode, &loaded_config.file),
                parser_config,
                format,
                color,
                max_input_bytes,
                dimensions: (width, height),
                output: output.as_deref(),
            },
        ),

        Command::Validate {
            input,
            parse_mode,
            layout_algorithm,
            format,
            fail_on,
            diagnostics_out,
            fnx_mode,
            fnx_projection,
            fnx_fallback,
        } => cmd_validate(
            &input,
            ValidateCommandOptions {
                parse_mode: resolve_parse_mode(parse_mode, &loaded_config.file),
                parser_config,
                layout_algorithm: resolve_layout_algorithm(layout_algorithm, &loaded_config.file)?,
                layout_config: build_layout_config(&loaded_config.file, None)?,
                format,
                fail_on,
                diagnostics_out: diagnostics_out.as_deref(),
                max_input_bytes,
                svg_base_config: build_base_svg_render_config(&loaded_config.file)?,
                show_back_edges: resolve_show_back_edges(&loaded_config.file),
                fnx_mode,
                fnx_projection,
                fnx_fallback,
            },
        ),

        Command::Capabilities { pretty, output } => cmd_capabilities(pretty, output.as_deref()),

        Command::DeterminismManifest => cmd_determinism_manifest(),

        Command::Interactive {
            input,
            parse_mode,
            theme,
        } => {
            let theme = resolve_theme_name(theme, &loaded_config.file);
            cmd_interactive(
                &input,
                resolve_parse_mode(parse_mode, &loaded_config.file),
                parser_config,
                &theme,
                max_input_bytes,
            )
        }

        #[cfg(feature = "watch")]
        Command::Watch {
            input,
            format,
            output,
            clear,
        } => {
            let theme = resolve_theme_name(None, &loaded_config.file);
            let layout_config = build_layout_config(&loaded_config.file, None)?;
            let svg_base_config = build_base_svg_render_config(&loaded_config.file)?;
            let term_base_config = build_base_term_render_config(&loaded_config.file)?;
            let show_back_edges = resolve_show_back_edges(&loaded_config.file);
            let show_minimap = term_base_config.show_minimap;
            let options = RenderCommandOptions {
                parse_mode: resolve_parse_mode(None, &loaded_config.file),
                parser_config,
                layout_algorithm: resolve_layout_algorithm(None, &loaded_config.file)?,
                layout_config,
                format,
                theme: &theme,
                font_size: None,
                output: output.as_deref(),
                max_input_bytes,
                svg_base_config,
                term_base_config,
                show_back_edges,
                show_minimap,
                embed_source_spans: format == OutputFormat::Svg,
                source_map_out: None,
                dimensions: (None, None),
                json_output: false,
                fnx_mode: FnxModeArg::Auto,
                fnx_projection: FnxProjectionArg::Undirected,
                fnx_fallback: FnxFallbackArg::Graceful,
            };
            cmd_watch(&input, options, clear)
        }

        #[cfg(feature = "serve")]
        Command::Serve { port, host, open } => {
            let theme = resolve_theme_name(None, &loaded_config.file);
            let layout_config = build_layout_config(&loaded_config.file, None)?;
            let svg_base_config = build_base_svg_render_config(&loaded_config.file)?;
            let term_base_config = build_base_term_render_config(&loaded_config.file)?;
            let show_back_edges = resolve_show_back_edges(&loaded_config.file);
            let show_minimap = term_base_config.show_minimap;
            let options = RenderCommandOptions {
                parse_mode: resolve_parse_mode(None, &loaded_config.file),
                parser_config,
                layout_algorithm: resolve_layout_algorithm(None, &loaded_config.file)?,
                layout_config,
                format: OutputFormat::Svg,
                theme: &theme,
                font_size: None,
                output: None,
                max_input_bytes,
                svg_base_config,
                term_base_config,
                show_back_edges,
                show_minimap,
                embed_source_spans: true,
                source_map_out: None,
                dimensions: (None, None),
                json_output: false,
                fnx_mode: FnxModeArg::Auto,
                fnx_projection: FnxProjectionArg::Undirected,
                fnx_fallback: FnxFallbackArg::Graceful,
            };
            cmd_serve(&host, port, open, options)
        }
    }
}

fn init_tracing(verbose: u8, quiet: bool) {
    let filter = if quiet {
        "error"
    } else {
        match verbose {
            0 => "warn",
            1 => "info",
            2 => "debug",
            _ => "trace",
        }
    };

    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .without_time()
        .try_init();
}

fn discover_config_path() -> Option<PathBuf> {
    let local = PathBuf::from("frankenmermaid.toml");
    if local.exists() {
        return Some(local);
    }

    let home = std::env::var_os("HOME")?;
    let user_config = PathBuf::from(home).join(".config/frankenmermaid/config.toml");
    user_config.exists().then_some(user_config)
}

fn load_cli_config(explicit_path: Option<&str>) -> Result<LoadedCliConfig> {
    let Some(path) = explicit_path
        .map(PathBuf::from)
        .or_else(discover_config_path)
    else {
        return Ok(LoadedCliConfig::default());
    };

    let contents = std::fs::read_to_string(&path)
        .with_context(|| format!("Failed to read config file: {}", path.display()))?;
    let file = toml::from_str::<FrankenmermaidConfigFile>(&contents)
        .map_err(|err| anyhow::anyhow!("Failed to parse config file {}: {err}", path.display()))?;

    // Force eager validation so invalid enum-like values fail at load time.
    validate_runtime_config_support(&file)?;
    let _ = resolve_max_input_bytes(&file)?;
    let _ = build_parser_config(&file);
    let _ = resolve_default_output_format(&file)?;
    let _ = resolve_default_layout_algorithm(&file)?;
    let _ = build_layout_config(&file, None)?;
    let _ = build_base_svg_render_config(&file)?;
    let _ = build_base_term_render_config(&file)?;

    info!("Loaded config file: {}", path.display());

    Ok(LoadedCliConfig { file })
}

fn validate_runtime_config_support(config: &FrankenmermaidConfigFile) -> Result<()> {
    let _ = config;
    Ok(())
}

fn build_parser_config(config: &FrankenmermaidConfigFile) -> ParserConfig {
    let mut parser_config = ParserConfig::default();
    if let Some(intent_inference) = config.parser.intent_inference {
        parser_config.intent_inference = intent_inference;
    }
    if let Some(fuzzy_keyword_distance) = config.parser.fuzzy_keyword_distance {
        parser_config.fuzzy_keyword_distance = fuzzy_keyword_distance;
    }
    if let Some(auto_close_delimiters) = config.parser.auto_close_delimiters {
        parser_config.auto_close_delimiters = auto_close_delimiters;
    }
    if let Some(create_placeholder_nodes) = config.parser.create_placeholder_nodes {
        parser_config.create_placeholder_nodes = create_placeholder_nodes;
    }
    parser_config
}

fn resolve_parse_mode(
    explicit: Option<ParseModeArg>,
    config: &FrankenmermaidConfigFile,
) -> MermaidParseMode {
    explicit.map_or_else(
        || {
            if matches!(config.core.fallback_on_error, Some(false)) {
                MermaidParseMode::Strict
            } else {
                MermaidParseMode::Compat
            }
        },
        ParseModeArg::to_core,
    )
}

fn parse_output_format_name(value: &str) -> Result<OutputFormat> {
    match value.trim().to_ascii_lowercase().as_str() {
        "svg" => Ok(OutputFormat::Svg),
        "png" => Ok(OutputFormat::Png),
        "term" => Ok(OutputFormat::Term),
        "ascii" => Ok(OutputFormat::Ascii),
        other => anyhow::bail!("unknown render.default_format '{other}'"),
    }
}

fn parse_layout_algorithm_name(value: &str) -> Result<LayoutAlgorithm> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(LayoutAlgorithm::Auto),
        "sugiyama" => Ok(LayoutAlgorithm::Sugiyama),
        "force" => Ok(LayoutAlgorithm::Force),
        "tree" => Ok(LayoutAlgorithm::Tree),
        "radial" => Ok(LayoutAlgorithm::Radial),
        "sequence" => Ok(LayoutAlgorithm::Sequence),
        "timeline" => Ok(LayoutAlgorithm::Timeline),
        "gantt" => Ok(LayoutAlgorithm::Gantt),
        "xychart" => Ok(LayoutAlgorithm::XyChart),
        "sankey" => Ok(LayoutAlgorithm::Sankey),
        "kanban" => Ok(LayoutAlgorithm::Kanban),
        "grid" => Ok(LayoutAlgorithm::Grid),
        "pie" => Ok(LayoutAlgorithm::Pie),
        "quadrant" => Ok(LayoutAlgorithm::Quadrant),
        "gitgraph" => Ok(LayoutAlgorithm::GitGraph),
        "packet" => Ok(LayoutAlgorithm::Packet),
        other => anyhow::bail!("unknown layout.algorithm '{other}'"),
    }
}

fn parse_edge_routing_name(value: &str) -> Result<EdgeRouting> {
    match value.trim().to_ascii_lowercase().as_str() {
        "orthogonal" => Ok(EdgeRouting::Orthogonal),
        "spline" => Ok(EdgeRouting::Spline),
        other => anyhow::bail!("unknown layout.edge_routing '{other}'"),
    }
}

fn parse_tier_name(value: &str) -> Result<MermaidTier> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" => Ok(MermaidTier::Auto),
        "compact" => Ok(MermaidTier::Compact),
        "normal" => Ok(MermaidTier::Normal),
        "rich" => Ok(MermaidTier::Rich),
        other => anyhow::bail!("unknown term.tier '{other}'"),
    }
}

fn parse_link_mode(value: &str) -> Result<MermaidLinkMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "off" | "disabled" => Ok(MermaidLinkMode::Off),
        "inline" | "on" | "enabled" => Ok(MermaidLinkMode::Inline),
        "footnote" | "notes" => Ok(MermaidLinkMode::Footnote),
        other => anyhow::bail!("unknown svg.link_mode '{other}'"),
    }
}

fn resolve_max_input_bytes(config: &FrankenmermaidConfigFile) -> Result<usize> {
    let max_input_bytes = config
        .core
        .max_input_bytes
        .unwrap_or(DEFAULT_MAX_INPUT_BYTES);
    if max_input_bytes == 0 {
        anyhow::bail!("core.max_input_bytes must be greater than 0");
    }
    Ok(max_input_bytes)
}

fn resolve_default_output_format(config: &FrankenmermaidConfigFile) -> Result<OutputFormat> {
    config
        .render
        .default_format
        .as_deref()
        .map(parse_output_format_name)
        .transpose()
        .map(|value| value.unwrap_or(OutputFormat::Svg))
}

fn resolve_output_format(
    explicit: Option<OutputFormat>,
    config: &FrankenmermaidConfigFile,
) -> Result<OutputFormat> {
    match explicit {
        Some(format) => Ok(format),
        None => resolve_default_output_format(config),
    }
}

fn resolve_default_layout_algorithm(config: &FrankenmermaidConfigFile) -> Result<LayoutAlgorithm> {
    config
        .layout
        .algorithm
        .as_deref()
        .map(parse_layout_algorithm_name)
        .transpose()
        .map(|value| value.unwrap_or(LayoutAlgorithm::Auto))
}

fn resolve_layout_algorithm(
    explicit: Option<LayoutAlgorithmArg>,
    config: &FrankenmermaidConfigFile,
) -> Result<LayoutAlgorithm> {
    match explicit {
        Some(algorithm) => Ok(algorithm.to_layout()),
        None => resolve_default_layout_algorithm(config),
    }
}

fn resolve_theme_name(explicit: Option<String>, config: &FrankenmermaidConfigFile) -> String {
    explicit
        .or_else(|| config.svg.theme.clone())
        .unwrap_or_else(|| String::from("default"))
}

fn resolve_show_back_edges(config: &FrankenmermaidConfigFile) -> bool {
    config.render.show_back_edges.unwrap_or(true)
}

fn validate_non_negative_f32(value: f32, field: &str) -> Result<f32> {
    if value.is_finite() && value >= 0.0 {
        Ok(value)
    } else {
        anyhow::bail!("{field} must be a finite value greater than or equal to 0");
    }
}

fn validate_positive_f32(value: f32, field: &str) -> Result<f32> {
    if value.is_finite() && value > 0.0 {
        Ok(value)
    } else {
        anyhow::bail!("{field} must be a finite value greater than 0");
    }
}

fn build_layout_config(
    config_file: &FrankenmermaidConfigFile,
    font_size: Option<f32>,
) -> Result<LayoutConfig> {
    let mut config = LayoutConfig {
        font_metrics: normalize_positive_font_size(font_size).map(|size| {
            fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                font_size: size,
                ..Default::default()
            })
        }),
        ..Default::default()
    };

    if let Some(cycle_strategy) = config_file.layout.cycle_strategy.as_deref() {
        config.cycle_strategy = CycleStrategy::parse(cycle_strategy).ok_or_else(|| {
            anyhow::anyhow!("unknown layout.cycle_strategy '{}'", cycle_strategy.trim())
        })?;
    }
    if let Some(node_spacing) = config_file.layout.node_spacing {
        config.spacing.node_spacing = validate_positive_f32(node_spacing, "layout.node_spacing")?;
    }
    if let Some(rank_spacing) = config_file.layout.rank_spacing {
        config.spacing.rank_spacing = validate_positive_f32(rank_spacing, "layout.rank_spacing")?;
    }
    if let Some(edge_routing) = config_file.layout.edge_routing.as_deref() {
        config.edge_routing = parse_edge_routing_name(edge_routing)?;
    }

    Ok(config)
}

fn apply_reduced_motion_setting(
    config: &mut SvgRenderConfig,
    reduced_motion: Option<&str>,
) -> Result<()> {
    let Some(reduced_motion) = reduced_motion else {
        return Ok(());
    };
    match reduced_motion.trim().to_ascii_lowercase().as_str() {
        "always" => config.animations_enabled = false,
        "never" => config.animations_enabled = true,
        "auto" => {}
        other => anyhow::bail!("unknown render.reduced_motion '{other}'"),
    }
    Ok(())
}

fn build_base_svg_render_config(config_file: &FrankenmermaidConfigFile) -> Result<SvgRenderConfig> {
    let mut config = SvgRenderConfig::default();

    if let Some(theme) = config_file.svg.theme.as_deref() {
        config.theme = theme
            .parse::<ThemePreset>()
            .map_err(|_| anyhow::anyhow!("unknown svg.theme '{}'", theme.trim()))?;
    }
    if let Some(rounded_corners) = config_file.svg.rounded_corners {
        config.rounded_corners = validate_non_negative_f32(rounded_corners, "svg.rounded_corners")?;
    }
    if let Some(padding) = config_file.svg.padding {
        config.padding = validate_non_negative_f32(padding, "svg.padding")?;
    }
    if let Some(shadows) = config_file.svg.shadows {
        config.shadows = shadows;
    }
    if let Some(gradients) = config_file.svg.gradients {
        config.node_gradients = gradients;
    }
    if let Some(accessibility) = config_file.svg.accessibility {
        config.accessible = accessibility;
        config.a11y = if accessibility {
            A11yConfig::full()
        } else {
            A11yConfig::none()
        };
    }
    if let Some(link_mode) = config_file.svg.link_mode.as_deref() {
        config.link_mode = parse_link_mode(link_mode)?;
    }
    if let Some(enable_links) = config_file.svg.enable_links {
        if !enable_links {
            config.link_mode = MermaidLinkMode::Off;
        } else if config_file.svg.link_mode.is_none() {
            config.link_mode = MermaidLinkMode::Inline;
        }
    }
    apply_reduced_motion_setting(&mut config, config_file.render.reduced_motion.as_deref())?;

    Ok(config)
}

fn build_base_term_render_config(
    config_file: &FrankenmermaidConfigFile,
) -> Result<TermRenderConfig> {
    let mut config = TermRenderConfig::rich();

    if let Some(tier) = config_file.term.tier.as_deref() {
        config.tier = parse_tier_name(tier)?;
    }
    if let Some(unicode) = config_file.term.unicode {
        config.glyph_mode = if unicode {
            MermaidGlyphMode::Unicode
        } else {
            MermaidGlyphMode::Ascii
        };
    }
    if let Some(show_minimap) = config_file.term.minimap {
        config.show_minimap = show_minimap;
    }

    Ok(config)
}

fn load_input(input: &str, max_input_bytes: usize) -> Result<String> {
    if input == "-" {
        let mut buffer = String::new();
        let mut handle = io::stdin().take(
            u64::try_from(max_input_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        );
        handle
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        if buffer.len() > max_input_bytes {
            anyhow::bail!(
                "Input from stdin is {} bytes, which exceeds core.max_input_bytes={max_input_bytes}",
                buffer.len()
            );
        }
        Ok(buffer)
    } else if let Some(file) = open_input_path(input)? {
        // Size gate via `fstat` on the already-open handle, NOT `fs::metadata(input)`: same length
        // and same error text, but it reads the inode we already hold instead of walking the path
        // a third time. See `open_input_path` for why that matters in a batch.
        let metadata = file
            .metadata()
            .context(format!("Failed to stat input file: {input}"))?;
        if metadata.len() > u64::try_from(max_input_bytes).unwrap_or(u64::MAX) {
            anyhow::bail!(
                "Input file '{}' is {} bytes, which exceeds core.max_input_bytes={max_input_bytes}",
                input,
                metadata.len()
            );
        }
        let mut handle = file.take(
            u64::try_from(max_input_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        );
        // Pre-size from the `fstat` length. `read_to_string` on a `Take` cannot see the underlying
        // file size, so an empty String makes it discover the length by doubling a small probe
        // buffer -- measured at ~6.6 reads per input across this corpus (3,368 reads for 512
        // files) plus the reallocs and copies that regrowing implies. One spare byte beyond the
        // known length lets the first read return the whole file and the second return 0 (EOF),
        // which is the minimum any correct reader can do. The size gate above already bounded
        // `len` by `max_input_bytes`, and over-reserving never touches the surplus pages.
        let mut content = String::with_capacity(
            usize::try_from(metadata.len())
                .unwrap_or(0)
                .saturating_add(1),
        );
        handle
            .read_to_string(&mut content)
            .context(format!("Failed to read file: {input}"))?;
        if content.len() > max_input_bytes {
            anyhow::bail!(
                "Input file '{input}' exceeds core.max_input_bytes={max_input_bytes} after UTF-8 decoding"
            );
        }
        Ok(content)
    } else {
        // Treat as inline diagram text
        if input.len() > max_input_bytes {
            anyhow::bail!(
                "Inline input is {} bytes, which exceeds core.max_input_bytes={max_input_bytes}",
                input.len()
            );
        }
        Ok(input.to_string())
    }
}

/// Open `input` as a file, or return `None` if it should be treated as inline diagram text.
///
/// This replaces `Path::new(input).exists()` + `fs::metadata(input)` + `File::open(input)`, which
/// resolved the same path three times for every input. Each resolution is a full walk that takes
/// the dentry/inode locks of every component, so in `render-batch` the walks of N inputs sharing
/// one directory contend on that directory's locks -- precisely when the batch is trying to scale
/// across cores. One walk per input removes two thirds of that contention and two thirds of the
/// per-input syscalls; the bytes read are unchanged.
///
/// The old predicate was `exists() && should_treat_input_as_path()`, where `Path::exists()` is
/// defined as "`fs::metadata` succeeded". `open` and `metadata` disagree on exactly one input
/// class: a file that can be stat-ed but not opened (mode 000) versus a path under a directory
/// that cannot be traversed. Both surface as `PermissionDenied` from `open`, and the old code sent
/// the first to an error and the second to the inline branch. So on any error other than
/// `NotFound` -- never on the hot path -- fall back to the original `exists()` probe and reproduce
/// its decision. `should_treat_input_as_path` is pure string inspection, so hoisting it above the
/// filesystem access only removes syscalls for inputs that were never going to be read as files.
fn open_input_path(input: &str) -> Result<Option<std::fs::File>> {
    if !should_treat_input_as_path(input) {
        return Ok(None);
    }
    match std::fs::File::open(input) {
        Ok(file) => Ok(Some(file)),
        // Missing path: `exists()` was false, so the old code fell through to inline text.
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => {
            if Path::new(input).exists() {
                Err(anyhow::Error::new(error)).context(format!("Failed to open file: {input}"))
            } else {
                Ok(None)
            }
        }
    }
}

fn should_treat_input_as_path(input: &str) -> bool {
    let has_path_separator = input.contains('/') || input.contains('\\');
    if has_path_separator {
        return true;
    }

    if looks_like_inline_mermaid(input) {
        return has_file_extension_hint(input);
    }

    true
}

fn has_file_extension_hint(input: &str) -> bool {
    if input.chars().any(|c| c.is_whitespace()) {
        return false;
    }
    let ext = Path::new(input)
        .extension()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase());
    ext.is_some()
}

fn looks_like_inline_mermaid(input: &str) -> bool {
    if input.contains('\n') || input.contains('\r') {
        return true;
    }

    let lowered = input.to_ascii_lowercase();
    let keywords = [
        "flowchart",
        "graph",
        "sequencediagram",
        "classdiagram",
        "statediagram",
        "erdiagram",
        "journey",
        "gantt",
        "gitgraph",
        "mindmap",
        "sankey",
        "xychart",
        "block",
        "timeline",
        "pie",
    ];
    if keywords.iter().any(|kw| lowered.contains(kw)) {
        return true;
    }

    let tokens = [
        "-->",
        "<--",
        "---",
        "==>",
        "<=>",
        "-.->",
        ":::",
        "%%{",
        "subgraph",
        "participant",
        "note",
        "classdef",
        "linkstyle",
    ];
    if tokens.iter().any(|token| input.contains(token)) {
        return true;
    }

    // Check for mermaid node definitions (ID followed by bracket shape)
    // Pattern: word followed by bracket - e.g., "A[text]", "B{decision}", "C(label)"
    // This is more specific than just checking for brackets, which would
    // false-positive on filenames like "report(final).mmd"
    has_node_definition_pattern(input)
}

/// Check for mermaid node definition patterns: alphanumeric ID followed by bracket shape.
/// Examples: `A[text]`, `node1{decision}`, `step(label)`, `id((circle))`
///
/// Excludes patterns that look like filenames (bracket followed by file extension).
fn has_node_definition_pattern(input: &str) -> bool {
    // If input looks like a filename with extension, it's probably not mermaid
    if looks_like_filename(input) {
        return false;
    }

    let chars: Vec<char> = input.chars().collect();
    let openers = ['[', '{', '('];

    for (i, &c) in chars.iter().enumerate() {
        if openers.contains(&c) && i > 0 {
            // Check if preceded by alphanumeric (node ID)
            let prev = chars[i - 1];
            if prev.is_alphanumeric() || prev == '_' {
                // Find matching closer
                let closer = match c {
                    '[' => ']',
                    '{' => '}',
                    '(' => ')',
                    _ => continue,
                };
                // Look for closer with content between
                if chars[i + 1..]
                    .iter()
                    .position(|&x| x == closer)
                    .is_some_and(|p| p > 0)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if input looks like a filename (has a file extension pattern).
fn looks_like_filename(input: &str) -> bool {
    // Look for pattern: text.ext where ext is 1-5 alphanumeric chars at end
    if let Some(dot_pos) = input.rfind('.') {
        let ext = &input[dot_pos + 1..];
        if !ext.is_empty() && ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            return true;
        }
    }
    false
}

fn layout_without_back_edges(layout: &fm_layout::DiagramLayout) -> fm_layout::DiagramLayout {
    let mut filtered = layout.clone();
    filtered.edges.retain(|edge| !edge.reversed);
    filtered
}

fn write_output(output: Option<&str>, content: &str) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(path, content).context(format!("Failed to write to: {path}"))?;
            info!("Wrote output to: {path}");
        }
        None => {
            io::stdout()
                .write_all(content.as_bytes())
                .context("Failed to write to stdout")?;
        }
    }
    Ok(())
}

fn write_output_bytes(output: Option<&str>, content: &[u8]) -> Result<()> {
    match output {
        Some(path) => {
            std::fs::write(path, content).context(format!("Failed to write to: {path}"))?;
            info!("Wrote output to: {path}");
        }
        None => {
            io::stdout()
                .write_all(content)
                .context("Failed to write to stdout")?;
        }
    }
    Ok(())
}

fn cmd_capabilities(pretty: bool, output: Option<&str>) -> Result<()> {
    let json = if pretty {
        capability_matrix_json_pretty()?
    } else {
        serde_json::to_string(&capability_matrix())?
    };
    write_output(output, &json)
}

fn cmd_determinism_manifest() -> Result<()> {
    let manifest = build_determinism_manifest();
    for case in &manifest.cases {
        anyhow::ensure!(
            case.non_finite_value_count == 0,
            "non-finite layout values detected for {}",
            case.case_id
        );
    }
    let json = serde_json::to_string_pretty(&manifest)?;
    write_output(None, &json)?;
    io::stdout().write_all(b"\n")?;
    Ok(())
}

fn build_determinism_manifest() -> DeterminismManifest {
    let cases: Vec<DeterminismManifestCase> = DETERMINISM_CASES
        .iter()
        .map(|(case_id, input)| determinism_manifest_case(case_id, input))
        .collect();
    let joined = cases
        .iter()
        .map(|case| format!("{}:{}", case.case_id, case.layout_sha256))
        .collect::<Vec<_>>()
        .join("\n");
    DeterminismManifest {
        version: 1,
        target_arch: std::env::consts::ARCH,
        target_os: std::env::consts::OS,
        target_env: option_env!("CARGO_CFG_TARGET_ENV").unwrap_or("unknown"),
        case_count: cases.len(),
        corpus_sha256: sha256_hex(joined.as_bytes()),
        cases,
    }
}

fn determinism_manifest_case(case_id: &'static str, input: &str) -> DeterminismManifestCase {
    let parsed = parse_with_mode(input, MermaidParseMode::Compat);
    let canonical = canonical_layout(&parsed.ir);
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let (non_finite_value_count, subnormal_value_count) = layout_float_anomalies(&layout);
    DeterminismManifestCase {
        case_id,
        diagram_type: parsed.ir.diagram_type.as_str().to_string(),
        node_count: parsed.ir.nodes.len(),
        edge_count: parsed.ir.edges.len(),
        layout_width: round6(layout.bounds.width),
        layout_height: round6(layout.bounds.height),
        non_finite_value_count,
        subnormal_value_count,
        layout_sha256: sha256_hex(canonical.as_bytes()),
    }
}

fn round6(v: f32) -> f64 {
    (f64::from(v) * 1_000_000.0).round() / 1_000_000.0
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    encode_hex(&hasher.finalize())
}

fn canonical_layout(ir: &MermaidDiagramIr) -> String {
    let layout = fm_layout::layout_diagram(ir);
    let mut lines: Vec<String> = Vec::new();

    let mut nodes: Vec<_> = layout.nodes.iter().collect();
    nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    for node in &nodes {
        lines.push(format!(
            "node:{} x={:.6} y={:.6} w={:.6} h={:.6}",
            node.node_id,
            round6(node.bounds.x),
            round6(node.bounds.y),
            round6(node.bounds.width),
            round6(node.bounds.height),
        ));
    }

    let mut edges: Vec<_> = layout.edges.iter().collect();
    edges.sort_by_key(|edge| edge.edge_index);
    for edge in &edges {
        let points = edge
            .points
            .iter()
            .map(|point| format!("{:.6},{:.6}", round6(point.x), round6(point.y)))
            .collect::<Vec<_>>()
            .join(";");
        lines.push(format!(
            "edge:{} reversed={} pts={}",
            edge.edge_index, edge.reversed, points
        ));
    }

    lines.push(format!(
        "bounds: x={:.6} y={:.6} w={:.6} h={:.6}",
        round6(layout.bounds.x),
        round6(layout.bounds.y),
        round6(layout.bounds.width),
        round6(layout.bounds.height),
    ));

    lines.join("\n")
}

fn layout_float_anomalies(layout: &fm_layout::DiagramLayout) -> (usize, usize) {
    let mut non_finite = 0_usize;
    let mut subnormal = 0_usize;
    let mut inspect = |value: f32| {
        if !value.is_finite() {
            non_finite += 1;
        } else if value != 0.0 && value.is_subnormal() {
            subnormal += 1;
        }
    };

    inspect(layout.bounds.x);
    inspect(layout.bounds.y);
    inspect(layout.bounds.width);
    inspect(layout.bounds.height);

    for node in &layout.nodes {
        inspect(node.bounds.x);
        inspect(node.bounds.y);
        inspect(node.bounds.width);
        inspect(node.bounds.height);
    }

    for edge in &layout.edges {
        for point in &edge.points {
            inspect(point.x);
            inspect(point.y);
        }
    }

    (non_finite, subnormal)
}

// =============================================================================
// Command: render
// =============================================================================

fn render_source(source: &str, options: &RenderCommandOptions<'_>) -> Result<RenderOutcome> {
    render_source_with_pressure(
        source,
        options,
        &MermaidNativePressureSignals::sample().into_report(),
    )
}

/// Render one diagram against an already-sampled host pressure report.
///
/// `MermaidNativePressureSignals::sample()` reads `/proc/self/status` and queries scheduler
/// affinity. Those describe the HOST, not the diagram, so sampling them per diagram is work the
/// output never depends on. In a batch it is also actively harmful: `/proc/self/status` is
/// generated by the kernel on each read and takes process mm locks, so concurrent samplers
/// serialize against each other exactly when the batch is trying to scale out. The batch path
/// samples once and shares the report; the single-diagram path is unchanged, still sampling
/// immediately before it renders.
fn render_source_with_pressure(
    source: &str,
    options: &RenderCommandOptions<'_>,
    pressure: &MermaidPressureReport,
) -> Result<RenderOutcome> {
    if source.len() > options.max_input_bytes {
        anyhow::bail!(
            "Inline input is {} bytes, which exceeds core.max_input_bytes={}",
            source.len(),
            options.max_input_bytes
        );
    }

    let total_start = Instant::now();
    let mut budget_broker = MermaidBudgetLedger::new(pressure);

    // Parse
    let parse_start = Instant::now();
    let parsed = parse_with_mode_and_config(source, options.parse_mode, &options.parser_config);
    let parse_time = parse_start.elapsed();
    budget_broker.record_parse(u64::try_from(parse_time.as_millis()).unwrap_or(u64::MAX));

    render_parsed_source_with_pressure(
        source,
        parsed,
        RenderTiming {
            parse_time,
            total_start,
        },
        budget_broker,
        options,
        pressure,
    )
}

/// Finish an already-parsed diagram through layout and rendering.
///
/// `render-batch` uses this boundary after its cross-diagram prefix compiler has parsed suffixes in
/// parallel. Single-diagram rendering still enters through [`render_source_with_pressure`], so its
/// public behavior and timing metadata are unchanged.
struct RenderTiming {
    parse_time: std::time::Duration,
    total_start: Instant,
}

fn render_parsed_source_with_pressure(
    source: &str,
    parsed: fm_parser::ParseResult,
    timing: RenderTiming,
    budget_broker: MermaidBudgetLedger,
    options: &RenderCommandOptions<'_>,
    pressure: &MermaidPressureReport,
) -> Result<RenderOutcome> {
    let fm_parser::ParseResult { ir, warnings, .. } = parsed;
    render_parsed_ir_with_pressure(
        source,
        &ir,
        &warnings,
        timing,
        budget_broker,
        options,
        pressure,
        None,
    )
}

/// Finish a parser-slot-backed diagram before that slot is overwritten by the next batch item.
fn render_batch_parse_ref_with_pressure(
    source: &str,
    parsed: FlowchartBatchParseRef<'_>,
    timing: RenderTiming,
    budget_broker: MermaidBudgetLedger,
    options: &RenderCommandOptions<'_>,
    pressure: &MermaidPressureReport,
    batch_renderer: &mut SvgBatchRenderer,
) -> Result<RenderOutcome> {
    let certified_prefix = parsed.reusable_prefix.map(|prefix| {
        CertifiedSvgBatchPrefix::new(
            Arc::clone(&prefix.identity),
            prefix.node_count,
            prefix.edge_count,
        )
    });
    render_parsed_ir_with_pressure(
        source,
        parsed.ir,
        parsed.warnings,
        timing,
        budget_broker,
        options,
        pressure,
        Some((batch_renderer, certified_prefix)),
    )
}

#[allow(clippy::too_many_arguments)]
fn render_parsed_ir_with_pressure(
    source: &str,
    ir: &MermaidDiagramIr,
    warnings: &[String],
    timing: RenderTiming,
    mut budget_broker: MermaidBudgetLedger,
    options: &RenderCommandOptions<'_>,
    pressure: &MermaidPressureReport,
    batch_renderer: Option<(&mut SvgBatchRenderer, Option<CertifiedSvgBatchPrefix>)>,
) -> Result<RenderOutcome> {
    let RenderTiming {
        parse_time,
        total_start,
    } = timing;
    debug!(
        "Parsed: type={:?}, nodes={}, edges={}, warnings={}",
        ir.diagram_type,
        ir.nodes.len(),
        ir.edges.len(),
        warnings.len()
    );

    for warning in warnings {
        warn!("Parse warning: {warning}");
    }

    // Layout
    let fnx_enabled = options.fnx_mode.should_use_fnx();
    let mut layout_config = options.layout_config.clone();
    layout_config.fnx_enabled = fnx_enabled;
    let layout_start = Instant::now();
    let layout_guardrails = LayoutGuardrails {
        max_layout_time_ms: budget_broker.layout_time_budget_ms(),
        max_layout_iterations: budget_broker
            .layout_iteration_budget(LayoutGuardrails::default().max_layout_iterations),
        max_route_ops: budget_broker.route_budget(LayoutGuardrails::default().max_route_ops),
    };
    let traced_layout = fm_layout::layout_diagram_traced_with_config_and_guardrails(
        ir,
        options.layout_algorithm,
        layout_config,
        layout_guardrails,
    );
    let layout = &traced_layout.layout;
    let layout_time = layout_start.elapsed();
    budget_broker.record_layout(layout_time.as_millis().min(u128::from(u64::MAX)) as u64);
    let mut guard_report =
        build_layout_guard_report_with_pressure(ir, &traced_layout, pressure.clone());
    let (_cx, observability) = mermaid_layout_guard_observability(
        "cli.render",
        source,
        traced_layout.trace.dispatch.selected.as_str(),
        traced_layout.trace.guard.estimated_layout_time_ms.max(1) as u64,
    );
    guard_report.observability = observability;

    debug!(
        "Layout: requested={}, selected={}, bounds={}x{}, crossings={}",
        traced_layout.trace.dispatch.requested.as_str(),
        traced_layout.trace.dispatch.selected.as_str(),
        layout.bounds.width,
        layout.bounds.height,
        layout.stats.crossing_count
    );
    if traced_layout.trace.guard.fallback_applied {
        warn!(
            "Layout guardrail fallback applied: {} -> {} ({})",
            traced_layout.trace.guard.initial_algorithm.as_str(),
            traced_layout.trace.guard.selected_algorithm.as_str(),
            traced_layout.trace.guard.reason,
        );
    }

    // Render
    let render_start = Instant::now();
    let effective_theme = if budget_broker.should_simplify_render() {
        "monochrome"
    } else {
        options.theme
    };
    let filtered_layout = (!options.show_back_edges).then(|| layout_without_back_edges(layout));
    let source_map_layout = filtered_layout.as_ref().unwrap_or(layout);

    let surface_options = || RenderSurfaceOptions {
        theme: effective_theme,
        font_size: options.font_size,
        svg_base_config: options.svg_base_config.clone(),
        term_base_config: options.term_base_config.clone(),
        show_back_edges: options.show_back_edges,
        show_minimap: options.show_minimap,
        embed_source_spans: options.embed_source_spans,
        dimensions: options.dimensions,
        degradation: guard_report.degradation.clone(),
    };
    let (rendered, actual_width, actual_height) = if options.format == OutputFormat::Svg
        && options.show_back_edges
        && let Some((renderer, certified_prefix)) = batch_renderer
    {
        let mut svg_config = build_svg_render_config(
            &options.svg_base_config,
            effective_theme,
            options.font_size,
            options.embed_source_spans,
        );
        svg_config.apply_degradation(&guard_report.degradation);
        let svg = renderer.render_borrowed(ir, Arc::clone(layout), &svg_config, certified_prefix);
        let (width, height) = extract_svg_dimensions(&svg);
        (svg.into_bytes(), width, height)
    } else {
        render_format(ir, layout, options.format, surface_options())?
    };
    let render_time = render_start.elapsed();
    budget_broker.record_render(render_time.as_millis().min(u128::from(u64::MAX)) as u64);

    let total_time = total_start.elapsed();
    let source_map = if options.json_output || options.source_map_out.is_some() {
        Some(layout_source_map(ir, source_map_layout))
    } else {
        None
    };

    if let Some(path) = options.source_map_out {
        let source_map = source_map.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Source map requested but not generated for this render")
        })?;
        let artifact = serde_json::to_string_pretty(&source_map)?;
        std::fs::write(path, artifact)
            .context(format!("Failed to write source map file: {path}"))?;
        info!("Wrote source map artifact to: {path}");
    }

    info!(
        "Rendered {} via layout {}->{} with {} nodes, {} edges in {:.2}ms",
        ir.diagram_type.as_str(),
        traced_layout.trace.dispatch.requested.as_str(),
        traced_layout.trace.dispatch.selected.as_str(),
        ir.nodes.len(),
        ir.edges.len(),
        total_time.as_secs_f64() * 1000.0
    );

    let render_result = if options.json_output {
        let accessibility_summary = describe_diagram_with_layout(ir, Some(layout));
        let source_map = source_map.as_ref().ok_or_else(|| {
            anyhow::anyhow!("Render metadata requested but source map was not generated")
        })?;
        guard_report.budget_broker = budget_broker.clone();
        let layout_decision_ledger =
            build_layout_decision_ledger(ir, &traced_layout, &guard_report);
        let layout_decision_explanation = layout_decision_ledger
            .primary_explanation()
            .expect("layout decision ledger should contain a primary entry");
        let layout_decision_ledger_jsonl = layout_decision_ledger.to_jsonl()?;
        let fnx_witness = build_fnx_witness(
            &traced_layout,
            fnx_enabled,
            options.fnx_projection,
            options.fnx_fallback,
        );

        Some(RenderResult {
            format: format!("{:?}", options.format).to_lowercase(),
            parse_mode: options.parse_mode.as_str().to_string(),
            embedded_source_spans: options.embed_source_spans,
            accessibility_summary,
            layout_requested: traced_layout.trace.dispatch.requested.as_str().to_string(),
            layout_selected: traced_layout.trace.dispatch.selected.as_str().to_string(),
            layout_guard_reason: traced_layout.trace.guard.reason.to_string(),
            layout_guard_fallback_applied: traced_layout.trace.guard.fallback_applied,
            layout_guard_time_budget_exceeded: traced_layout.trace.guard.time_budget_exceeded,
            layout_guard_iteration_budget_exceeded: traced_layout
                .trace
                .guard
                .iteration_budget_exceeded,
            layout_guard_route_budget_exceeded: traced_layout.trace.guard.route_budget_exceeded,
            layout_guard_estimated_time_ms: traced_layout.trace.guard.estimated_layout_time_ms,
            layout_guard_estimated_iterations: traced_layout
                .trace
                .guard
                .estimated_layout_iterations,
            layout_guard_estimated_route_ops: traced_layout.trace.guard.estimated_route_ops,
            layout_band_count: traced_layout.layout.extensions.bands.len(),
            layout_tick_count: traced_layout.layout.extensions.axis_ticks.len(),
            source_span_node_count: count_known_node_spans(source_map_layout),
            source_span_edge_count: count_known_edge_spans(source_map_layout),
            source_span_cluster_count: count_known_cluster_spans(source_map_layout),
            source_map_entry_count: source_map.entries.len(),
            source_map_out: options.source_map_out.map(str::to_string),
            diagram_type: ir.diagram_type.as_str().to_string(),
            node_count: ir.nodes.len(),
            edge_count: ir.edges.len(),
            pressure_source: guard_report.pressure.source.as_str().to_string(),
            pressure_tier: guard_report.pressure.tier.as_str().to_string(),
            pressure_telemetry_available: guard_report.pressure.telemetry_available,
            pressure_conservative_fallback: guard_report.pressure.conservative_fallback,
            pressure_score_permille: guard_report.pressure.quantized_score_permille,
            trace_id: guard_report.observability.trace_id.to_string(),
            decision_id: guard_report.observability.decision_id.to_string(),
            policy_id: guard_report.observability.policy_id.to_string(),
            schema_version: guard_report.observability.schema_version.to_string(),
            layout_decision_ledger,
            layout_decision_explanation,
            layout_decision_ledger_jsonl,
            budget_total_ms: budget_broker.total_budget_ms,
            parse_budget_ms: budget_broker.parse.allocated_ms,
            layout_budget_ms: budget_broker.layout.allocated_ms,
            render_budget_ms: budget_broker.render.allocated_ms,
            budget_exhausted: budget_broker.exhausted,
            parse_used_ms: budget_broker.parse.used_ms,
            layout_used_ms: budget_broker.layout.used_ms,
            render_used_ms: budget_broker.render.used_ms,
            degradation_target_fidelity: format!("{:?}", guard_report.degradation.target_fidelity),
            degradation_reduce_decoration: guard_report.degradation.reduce_decoration,
            degradation_simplify_routing: guard_report.degradation.simplify_routing,
            degradation_hide_labels: guard_report.degradation.hide_labels,
            degradation_collapse_clusters: guard_report.degradation.collapse_clusters,
            degradation_force_glyph_mode: guard_report
                .degradation
                .force_glyph_mode
                .map(|m| format!("{m:?}")),
            output_bytes: rendered.len(),
            width: actual_width,
            height: actual_height,
            parse_time_ms: parse_time.as_secs_f64() * 1000.0,
            layout_time_ms: layout_time.as_secs_f64() * 1000.0,
            render_time_ms: render_time.as_secs_f64() * 1000.0,
            total_time_ms: total_time.as_secs_f64() * 1000.0,
            warnings: warnings.to_vec(),
            fnx_witness,
        })
    } else {
        None
    };

    Ok(RenderOutcome {
        rendered,
        render_result,
    })
}

/// Build FNX witness metadata if FNX integration is enabled.
#[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
fn build_fnx_witness(
    traced_layout: &fm_layout::TracedLayout,
    fnx_enabled: bool,
    _fnx_projection: FnxProjectionArg,
    fnx_fallback: FnxFallbackArg,
) -> Option<FnxWitness> {
    if !fnx_enabled {
        return None;
    }
    let layout_selected = traced_layout.trace.dispatch.selected;
    let uses_sugiyama = layout_selected == fm_layout::LayoutAlgorithm::Sugiyama;
    // fnx_enabled is always true here (early return above), so `used` simplifies to `uses_sugiyama`
    let used = uses_sugiyama;
    let algorithms_invoked = if used {
        vec!["degree_centrality".to_string()]
    } else {
        Vec::new()
    };
    // fnx_enabled is always true here (early return above)
    let (fallback_level, fallback_reason) = if !uses_sugiyama {
        ("fnx_disabled", "not_applicable")
    } else {
        ("fnx_full", "none")
    };
    let projection_mode = "undirected";
    let node_count = traced_layout.layout.stats.node_count.to_string();
    let edge_count = traced_layout.layout.stats.edge_count.to_string();
    let crossings_before = traced_layout
        .layout
        .stats
        .crossing_count_before_refinement
        .to_string();
    let crossings_after = traced_layout.layout.stats.crossing_count.to_string();
    let results_hash = fnx_results_hash(&[
        layout_selected.as_str(),
        if used { "used" } else { "unused" },
        projection_mode,
        fnx_fallback.as_str(),
        &node_count,
        &edge_count,
        &crossings_before,
        &crossings_after,
    ]);

    Some(FnxWitness {
        enabled: fnx_enabled,
        used,
        projection_mode: projection_mode.to_string(),
        algorithms_invoked,
        analysis_time_us: 0,
        budget_exceeded: false,
        fallback_level: fallback_level.to_string(),
        fallback_reason: fallback_reason.to_string(),
        results_hash,
    })
}

#[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
fn build_fnx_validation_witness(
    fnx_enabled: bool,
    _fnx_projection: FnxProjectionArg,
    fnx_fallback: FnxFallbackArg,
    results: Option<&FnxAnalysisResults>,
    analysis_time: std::time::Duration,
) -> Option<FnxWitness> {
    if !fnx_enabled {
        return None;
    }
    // fnx_enabled is always true here (early return above)
    let (fallback_level, fallback_reason) = if results.is_none() {
        ("fnx_disabled", "not_applicable")
    } else {
        ("fnx_full", "none")
    };
    let projection_mode = "undirected";
    let mut algorithms_invoked = vec!["connected_components", "cycle_basis"];
    if results.is_some_and(|r| r.is_connected) {
        algorithms_invoked.push("articulation_points");
        algorithms_invoked.push("bridges");
    }
    let algorithms_invoked = algorithms_invoked
        .into_iter()
        .map(|entry| entry.to_string())
        .collect::<Vec<_>>();
    let component_count = results
        .map(|r| r.component_count.to_string())
        .unwrap_or_default();
    let cycle_count = results
        .map(|r| r.cycle_count.to_string())
        .unwrap_or_default();
    let bridge_count = results
        .map(|r| r.bridge_count.to_string())
        .unwrap_or_default();
    // fnx_enabled is always true here (early return above)
    let results_hash = fnx_results_hash(&[
        "enabled",
        projection_mode,
        fnx_fallback.as_str(),
        &component_count,
        &cycle_count,
        &bridge_count,
    ]);

    Some(FnxWitness {
        enabled: true,
        used: results.is_some(),
        projection_mode: projection_mode.to_string(),
        algorithms_invoked,
        analysis_time_us: analysis_time.as_micros().min(u128::from(u64::MAX)) as u64,
        budget_exceeded: false,
        fallback_level: fallback_level.to_string(),
        fallback_reason: fallback_reason.to_string(),
        results_hash,
    })
}

/// Build FNX witness metadata when FNX integration is disabled.
#[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
fn build_fnx_witness(
    _traced_layout: &fm_layout::TracedLayout,
    _fnx_enabled: bool,
    _fnx_projection: FnxProjectionArg,
    _fnx_fallback: FnxFallbackArg,
) -> Option<FnxWitness> {
    // FNX not available, no witness to report
    None
}

fn cmd_render(input: &str, options: RenderCommandOptions<'_>) -> Result<()> {
    let RenderCommandOptions {
        parse_mode,
        parser_config,
        layout_algorithm,
        layout_config,
        format,
        theme,
        font_size,
        output,
        max_input_bytes,
        svg_base_config,
        term_base_config,
        show_back_edges,
        show_minimap,
        embed_source_spans,
        source_map_out,
        dimensions,
        json_output,
        fnx_mode,
        fnx_projection,
        fnx_fallback,
    } = options;
    let (width, height) = dimensions;
    if json_output && output.is_none() {
        anyhow::bail!("--json requires --output so rendered output does not mix with metadata");
    }
    if source_map_out.is_some() && format != OutputFormat::Svg {
        anyhow::bail!("--source-map-out is only supported with --format svg");
    }
    if matches!(fnx_mode, FnxModeArg::Enabled)
        && !cfg!(all(
            feature = "fnx-integration",
            not(target_arch = "wasm32")
        ))
    {
        anyhow::bail!("--fnx-mode enabled requires fnx-integration feature");
    }
    if fnx_mode.should_use_fnx() {
        if matches!(
            fnx_projection,
            FnxProjectionArg::Directed | FnxProjectionArg::Auto
        ) {
            anyhow::bail!(
                "--fnx-projection {} is not yet supported (use undirected)",
                fnx_projection.as_str()
            );
        }
        if matches!(fnx_fallback, FnxFallbackArg::Strict | FnxFallbackArg::Warn) {
            anyhow::bail!(
                "--fnx-fallback {} is not yet supported (use graceful)",
                fnx_fallback.as_str()
            );
        }
    }

    let source = load_input(input, max_input_bytes)?;
    let outcome = render_source(
        &source,
        &RenderCommandOptions {
            parse_mode,
            parser_config,
            layout_algorithm,
            layout_config,
            format,
            theme,
            font_size,
            output,
            max_input_bytes,
            svg_base_config,
            term_base_config,
            show_back_edges,
            show_minimap,
            embed_source_spans,
            source_map_out,
            dimensions: (width, height),
            json_output,
            fnx_mode,
            fnx_projection,
            fnx_fallback,
        },
    )?;

    if let Some(result) = outcome.render_result {
        let json_str = serde_json::to_string_pretty(&result)?;
        println!("{json_str}");
    }

    // Write output
    match format {
        OutputFormat::Png => write_output_bytes(output, &outcome.rendered)?,
        _ => write_output(output, &String::from_utf8_lossy(&outcome.rendered))?,
    }

    Ok(())
}

/// Extension used for a batch output file.
fn batch_output_extension(format: OutputFormat) -> &'static str {
    match format {
        OutputFormat::Svg => "svg",
        OutputFormat::Png => "png",
        OutputFormat::Term | OutputFormat::Ascii => "txt",
    }
}

fn parse_batch_change_set_line(line: &str, line_number: usize) -> Result<Option<Vec<String>>> {
    if line.trim().is_empty() {
        return Ok(None);
    }
    serde_json::from_str(line)
        .with_context(|| format!("invalid change-set JSON array on input line {line_number}"))
        .map(Some)
}

fn parse_batch_final_state_payload(
    payload: &str,
    inputs: &[String],
    max_input_bytes: usize,
) -> Result<std::collections::BTreeMap<String, String>> {
    let updates = serde_json::from_str::<std::collections::BTreeMap<String, String>>(payload)
        .context("invalid final-state JSON object")?;
    let input_set = inputs
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    for (input, source) in &updates {
        if !input_set.contains(input.as_str()) {
            anyhow::bail!("final-state input {input:?} is not one of this batch's inputs");
        }
        if source.len() > max_input_bytes {
            anyhow::bail!(
                "final-state input {input:?} is {} bytes, exceeding the {max_input_bytes}-byte \
                 input limit",
                source.len()
            );
        }
    }
    Ok(updates)
}

#[derive(Debug)]
struct PreparedBatchFinalState {
    updates: std::collections::BTreeMap<String, String>,
    source_digests: Vec<String>,
    changed_inputs: Vec<String>,
    total_source_bytes: usize,
}

fn prepare_batch_final_state_updates(
    updates: std::collections::BTreeMap<String, String>,
) -> PreparedBatchFinalState {
    let source_digests = updates
        .values()
        .map(|source| sha256_hex(source.as_bytes()))
        .collect();
    let changed_inputs = updates.keys().cloned().collect();
    let total_source_bytes = updates
        .values()
        .map(String::len)
        .fold(0usize, usize::saturating_add);
    PreparedBatchFinalState {
        updates,
        source_digests,
        changed_inputs,
        total_source_bytes,
    }
}

fn prepare_batch_final_state_payload(
    payload: &str,
    inputs: &[String],
    max_input_bytes: usize,
) -> Result<PreparedBatchFinalState> {
    parse_batch_final_state_payload(payload, inputs, max_input_bytes)
        .map(prepare_batch_final_state_updates)
}

fn prepare_complete_batch_final_state_payload(
    payload: &str,
    inputs: &[String],
    max_input_bytes: usize,
) -> Result<PreparedBatchFinalState> {
    let prepared = prepare_batch_final_state_payload(payload, inputs, max_input_bytes)?;
    if prepared.updates.len() != inputs.len() {
        anyhow::bail!(
            "--complete-snapshot-stream final payload contains {} of {} batch inputs",
            prepared.updates.len(),
            inputs.len()
        );
    }
    Ok(prepared)
}

fn merge_superseded_final_state_updates(
    completed_state: &mut std::collections::BTreeMap<String, String>,
    updates: std::collections::BTreeMap<String, String>,
) {
    completed_state.extend(updates);
}

const RESIDENT_PAYLOAD_CACHE_MAX_ENTRIES: usize = 8;
const RESIDENT_PAYLOAD_CACHE_MAX_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug)]
struct ResidentPayloadCacheEntry {
    payload: String,
    prepared: Arc<PreparedBatchFinalState>,
    retained_bytes: usize,
    last_used: u64,
}

#[derive(Debug, Default)]
struct ResidentPayloadCache {
    entries: Vec<ResidentPayloadCacheEntry>,
    retained_bytes: usize,
    clock: u64,
    decoded: usize,
    reused: usize,
}

impl ResidentPayloadCache {
    fn prepare(
        &mut self,
        payload: &str,
        inputs: &[String],
        max_input_bytes: usize,
        enabled: bool,
    ) -> Result<Arc<PreparedBatchFinalState>> {
        if enabled
            && let Some(index) = self
                .entries
                .iter()
                .position(|entry| entry.payload == payload)
        {
            self.clock = self.clock.saturating_add(1);
            self.entries[index].last_used = self.clock;
            self.reused = self.reused.saturating_add(1);
            return Ok(Arc::clone(&self.entries[index].prepared));
        }

        let prepared = Arc::new(prepare_batch_final_state_payload(
            payload,
            inputs,
            max_input_bytes,
        )?);
        self.decoded = self.decoded.saturating_add(1);
        if !enabled {
            return Ok(prepared);
        }

        let retained_bytes = payload
            .len()
            .saturating_add(prepared.total_source_bytes)
            .saturating_add(
                prepared
                    .changed_inputs
                    .iter()
                    .map(String::len)
                    .fold(0usize, usize::saturating_add),
            )
            .saturating_add(
                prepared
                    .source_digests
                    .iter()
                    .map(String::len)
                    .fold(0usize, usize::saturating_add),
            );
        if retained_bytes > RESIDENT_PAYLOAD_CACHE_MAX_BYTES {
            return Ok(prepared);
        }

        self.clock = self.clock.saturating_add(1);
        self.retained_bytes = self.retained_bytes.saturating_add(retained_bytes);
        self.entries.push(ResidentPayloadCacheEntry {
            payload: payload.to_owned(),
            prepared: Arc::clone(&prepared),
            retained_bytes,
            last_used: self.clock,
        });
        while self.entries.len() > RESIDENT_PAYLOAD_CACHE_MAX_ENTRIES
            || self.retained_bytes > RESIDENT_PAYLOAD_CACHE_MAX_BYTES
        {
            let Some(oldest) = self
                .entries
                .iter()
                .enumerate()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(index, _)| index)
            else {
                break;
            };
            let evicted = self.entries.remove(oldest);
            self.retained_bytes = self.retained_bytes.saturating_sub(evicted.retained_bytes);
        }
        Ok(prepared)
    }
}

fn batch_final_state_payload_limit(inputs: &[String], max_input_bytes: usize) -> usize {
    // JSON escaping can expand one source byte to six ASCII bytes. The fixed allowance covers
    // absolute path keys and object punctuation without making the decoder an unbounded buffer.
    max_input_bytes
        .saturating_mul(inputs.len().max(1))
        .saturating_mul(6)
        .saturating_add(1024 * 1024)
}

#[derive(Debug, Clone, Copy)]
struct FinalStateStreamMaterialization {
    outputs_at_eof: bool,
    sources_at_eof: bool,
    acknowledgments_at_eof: bool,
    complete_snapshots: bool,
}

impl FinalStateStreamMaterialization {
    fn exposes_only_completed_state(self) -> bool {
        self.outputs_at_eof && self.sources_at_eof && self.acknowledgments_at_eof
    }
}

#[allow(clippy::too_many_arguments)]
fn render_prepared_final_state_transaction(
    inputs: &[String],
    out_dir: &str,
    jobs: Option<usize>,
    keep_going: bool,
    json: bool,
    plan: &BatchRenderPlan,
    cache_session: &mut BatchRenderCacheSession,
    prepared: &PreparedBatchFinalState,
    source_overrides: Option<&std::collections::BTreeMap<String, String>>,
    options: &RenderCommandOptions<'_>,
) -> Result<bool> {
    let replay_started = Instant::now();
    let replayed_total_bytes = (!json)
        .then(|| {
            cache_session.replay_resident_transaction(plan, prepared, source_overrides.is_some())
        })
        .transpose()?
        .flatten();
    if let Some(total_bytes) = replayed_total_bytes {
        eprintln!(
            "rendered {0}/{0} diagram(s) ({0} persistent hits, 0 identical renders reused, 0 \
             shared prefix parses reused / 0 bytes), {total_bytes} bytes, {1} requested \
             worker(s), 0 active worker(s), {2:.3} ms",
            inputs.len(),
            plan.requested_workers,
            replay_started.elapsed().as_secs_f64() * 1000.0,
        );
    } else {
        cmd_render_batch(
            inputs,
            out_dir,
            jobs,
            keep_going,
            json,
            BatchCachePolicy {
                use_cache: true,
                trust_change_set: true,
                changed_inputs: &prepared.changed_inputs,
                source_overrides,
                session: Some(cache_session),
                plan: Some(plan),
                report: None,
            },
            options.clone(),
        )?;
    }
    Ok(replayed_total_bytes.is_some())
}

fn cmd_render_batch_final_state_transaction_stream(
    inputs: &[String],
    out_dir: &str,
    jobs: Option<usize>,
    keep_going: bool,
    json: bool,
    materialization: FinalStateStreamMaterialization,
    options: RenderCommandOptions<'_>,
) -> Result<()> {
    use std::io::{BufRead, Read, Write};

    let coalesce_superseded_revisions = materialization.exposes_only_completed_state()
        && std::env::var_os("FM_DISABLE_FINAL_STATE_COALESCING").is_none();
    let retain_only_latest_complete_snapshot = coalesce_superseded_revisions
        && materialization.complete_snapshots
        && std::env::var_os("FM_DISABLE_COMPLETE_SNAPSHOT_ELISION").is_none();
    let FinalStateStreamMaterialization {
        outputs_at_eof: final_output_only,
        sources_at_eof: final_source_only,
        acknowledgments_at_eof: final_ack_only,
        complete_snapshots: _,
    } = materialization;

    let max_payload_bytes = batch_final_state_payload_limit(inputs, options.max_input_bytes);
    let read_limit = u64::try_from(max_payload_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(2);
    let plan = BatchRenderPlan::new(inputs, out_dir, jobs, true, &options)?;
    let mut cache_session = BatchRenderCacheSession::default();
    let admit_clean_batch = std::env::var_os("FM_DISABLE_DURABLE_BATCH_CERTIFICATE").is_none();
    cache_session.begin_stream(&plan.cache_path, Some(&plan), admit_clean_batch)?;
    cache_session.defer_output_writes = final_output_only;
    cache_session.elide_certified_source_writes = retain_only_latest_complete_snapshot
        && std::env::var_os("FM_DISABLE_CERTIFIED_SOURCE_NOOP").is_none();

    let stdin = io::stdin();
    let mut reader = stdin.lock();
    let mut line_number = 0usize;
    let mut transaction = 0usize;
    let mut executed_transactions = 0usize;
    let mut replayed_transactions = 0usize;
    let mut source_materializations = 0usize;
    let mut acknowledged_updates = 0usize;
    let mut acknowledged_source_bytes = 0usize;
    let mut deferred_sources = std::collections::BTreeMap::new();
    let mut encoded_payload_bytes = 0usize;
    let mut latest_complete_payload = Vec::new();
    let mut has_latest_complete_payload = false;
    let payload_cache_enabled = std::env::var_os("FM_DISABLE_RESIDENT_PAYLOAD_CACHE").is_none();
    let mut payload_cache = ResidentPayloadCache::default();
    let mut encoded = Vec::new();
    loop {
        encoded.clear();
        let bytes_read = (&mut reader)
            .take(read_limit)
            .read_until(b'\n', &mut encoded)
            .context("cannot read final-state transaction stream")?;
        if bytes_read == 0 {
            break;
        }
        line_number += 1;
        if encoded.last() == Some(&b'\n') {
            encoded.pop();
        }
        if encoded.last() == Some(&b'\r') {
            encoded.pop();
        }
        if encoded.len() > max_payload_bytes {
            anyhow::bail!(
                "final-state JSON on input line {line_number} exceeds the \
                 {max_payload_bytes}-byte transaction limit"
            );
        }
        let payload = std::str::from_utf8(&encoded)
            .with_context(|| format!("final-state input line {line_number} is not UTF-8"))?;
        if payload.trim().is_empty() {
            continue;
        }
        if retain_only_latest_complete_snapshot {
            transaction = transaction.saturating_add(1);
            acknowledged_updates = acknowledged_updates.saturating_add(inputs.len());
            encoded_payload_bytes = encoded_payload_bytes.saturating_add(encoded.len());
            std::mem::swap(&mut latest_complete_payload, &mut encoded);
            has_latest_complete_payload = true;
            continue;
        }
        if coalesce_superseded_revisions {
            let updates = parse_batch_final_state_payload(payload, inputs, options.max_input_bytes)
                .with_context(|| {
                    format!("invalid final-state transaction on input line {line_number}")
                })?;
            let changed_inputs = updates.len();
            let total_source_bytes = updates
                .values()
                .map(String::len)
                .fold(0usize, usize::saturating_add);
            merge_superseded_final_state_updates(&mut deferred_sources, updates);
            transaction = transaction.saturating_add(1);
            acknowledged_updates = acknowledged_updates.saturating_add(changed_inputs);
            acknowledged_source_bytes =
                acknowledged_source_bytes.saturating_add(total_source_bytes);
            continue;
        }
        let prepared = payload_cache
            .prepare(
                payload,
                inputs,
                options.max_input_bytes,
                payload_cache_enabled,
            )
            .with_context(|| {
                format!("invalid final-state transaction on input line {line_number}")
            })?;
        let updates = &prepared.updates;
        let changed_inputs = &prepared.changed_inputs;
        let total_source_bytes = prepared.total_source_bytes;
        if final_source_only {
            for (input, source) in updates {
                deferred_sources.insert(input.clone(), source.clone());
            }
        } else {
            for (input, source) in updates {
                std::fs::write(input, source.as_bytes())
                    .with_context(|| format!("cannot apply final-state input {input}"))?;
                source_materializations = source_materializations.saturating_add(1);
            }
        }
        let replayed = render_prepared_final_state_transaction(
            inputs,
            out_dir,
            jobs,
            keep_going,
            json,
            &plan,
            &mut cache_session,
            &prepared,
            final_source_only.then_some(&deferred_sources),
            &options,
        )
        .with_context(|| format!("final-state transaction {} failed", transaction + 1))?;
        executed_transactions = executed_transactions.saturating_add(1);
        if replayed {
            replayed_transactions += 1;
        }
        transaction += 1;
        acknowledged_updates = acknowledged_updates.saturating_add(changed_inputs.len());
        acknowledged_source_bytes = acknowledged_source_bytes.saturating_add(total_source_bytes);

        if !final_ack_only {
            let mut stdout = io::stdout().lock();
            serde_json::to_writer(
                &mut stdout,
                &serde_json::json!({
                    "transaction": transaction,
                    "input_line": line_number,
                    "updates": changed_inputs.len(),
                    "source_bytes": total_source_bytes,
                    "status": "ok"
                }),
            )?;
            writeln!(stdout)?;
            stdout.flush()?;
        }
    }
    if retain_only_latest_complete_snapshot && has_latest_complete_payload {
        let payload = std::str::from_utf8(&latest_complete_payload)
            .context("completed final-state snapshot is not UTF-8")?;
        let prepared =
            prepare_complete_batch_final_state_payload(payload, inputs, options.max_input_bytes)
                .context("completed final-state snapshot is invalid")?;
        acknowledged_source_bytes = prepared.total_source_bytes;
        let replayed = render_prepared_final_state_transaction(
            inputs,
            out_dir,
            jobs,
            keep_going,
            json,
            &plan,
            &mut cache_session,
            &prepared,
            Some(&prepared.updates),
            &options,
        )
        .context("completed final-state snapshot failed")?;
        executed_transactions = 1;
        replayed_transactions = if replayed { 1 } else { 0 };
        deferred_sources = prepared.updates;
    } else if coalesce_superseded_revisions && !deferred_sources.is_empty() {
        let prepared = prepare_batch_final_state_updates(std::mem::take(&mut deferred_sources));
        let replayed = render_prepared_final_state_transaction(
            inputs,
            out_dir,
            jobs,
            keep_going,
            json,
            &plan,
            &mut cache_session,
            &prepared,
            Some(&prepared.updates),
            &options,
        )
        .context("coalesced final-state transaction failed")?;
        executed_transactions = 1;
        replayed_transactions = if replayed { 1 } else { 0 };
        deferred_sources = prepared.updates;
    }
    if final_source_only {
        let (sources, certified_sources, bytes) =
            cache_session.materialize_deferred_sources(&plan, &deferred_sources)?;
        source_materializations = source_materializations.saturating_add(sources);
        eprintln!(
            "materialized {sources} final source(s), reused {certified_sources} certified source(s), \
             {bytes} logical bytes at stream EOF"
        );
    }
    let (outputs, bytes) = cache_session.materialize_deferred_outputs()?;
    if final_output_only {
        eprintln!("materialized {outputs} final output(s), {bytes} bytes at stream EOF");
    }
    cache_session.flush(Path::new(out_dir))?;
    if retain_only_latest_complete_snapshot {
        eprintln!(
            "retained the newest of {transaction} complete snapshot payload(s), skipped {} \
             superseded JSON decode(s), {encoded_payload_bytes} encoded bytes",
            transaction.saturating_sub(if has_latest_complete_payload { 1 } else { 0 })
        );
    } else if coalesce_superseded_revisions {
        eprintln!(
            "coalesced {transaction} resident payload(s) containing {acknowledged_updates} update(s) \
             into {} final update(s)",
            deferred_sources.len()
        );
    } else {
        eprintln!(
            "decoded {} resident payload(s), reused {} exact payload(s)",
            payload_cache.decoded, payload_cache.reused
        );
    }
    if final_ack_only {
        let mut stdout = io::stdout().lock();
        let acknowledgment = if retain_only_latest_complete_snapshot {
            serde_json::json!({
                "transactions": transaction,
                "input_lines": line_number,
                "updates": acknowledged_updates,
                "source_bytes": acknowledged_source_bytes,
                "source_bytes_scope": "completed_state",
                "encoded_payload_bytes": encoded_payload_bytes,
                "status": "ok"
            })
        } else {
            serde_json::json!({
                "transactions": transaction,
                "input_lines": line_number,
                "updates": acknowledged_updates,
                "source_bytes": acknowledged_source_bytes,
                "status": "ok"
            })
        };
        serde_json::to_writer(&mut stdout, &acknowledgment)?;
        writeln!(stdout)?;
        stdout.flush()?;
        eprintln!("emitted one EOF acknowledgment for {transaction} transaction(s)");
    }
    eprintln!(
        "applied {transaction} resident final-state transaction(s) \
         ({executed_transactions} executed, {replayed_transactions} complete revision replay(s))"
    );
    eprintln!("materialized {source_materializations} source revision(s) during stream");
    Ok(())
}

fn cmd_render_batch_final_state_stream(
    inputs: &[String],
    out_dir: &str,
    jobs: Option<usize>,
    keep_going: bool,
    json: bool,
    options: RenderCommandOptions<'_>,
) -> Result<()> {
    use std::io::Read;

    let max_payload_bytes = batch_final_state_payload_limit(inputs, options.max_input_bytes);
    let read_limit = u64::try_from(max_payload_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut payload = String::new();
    io::stdin()
        .lock()
        .take(read_limit)
        .read_to_string(&mut payload)
        .context("cannot read final-state JSON from stdin")?;
    if payload.len() > max_payload_bytes {
        anyhow::bail!("final-state JSON exceeds the {max_payload_bytes}-byte transaction limit");
    }
    let updates = parse_batch_final_state_payload(&payload, inputs, options.max_input_bytes)?;
    let plan = BatchRenderPlan::new(inputs, out_dir, jobs, true, &options)?;
    let changed_inputs = updates.keys().cloned().collect::<Vec<_>>();
    let total_source_bytes = updates
        .values()
        .map(String::len)
        .fold(0usize, usize::saturating_add);
    for (input, source) in &updates {
        std::fs::write(input, source.as_bytes())
            .with_context(|| format!("cannot apply final-state input {input}"))?;
    }

    cmd_render_batch(
        inputs,
        out_dir,
        jobs,
        keep_going,
        json,
        BatchCachePolicy {
            use_cache: true,
            trust_change_set: true,
            changed_inputs: &changed_inputs,
            source_overrides: None,
            session: None,
            plan: Some(&plan),
            report: None,
        },
        options,
    )?;
    eprintln!(
        "applied {} coalesced final source update(s), {total_source_bytes} bytes",
        changed_inputs.len()
    );
    Ok(())
}

fn cmd_render_batch_change_set_stream(
    inputs: &[String],
    out_dir: &str,
    jobs: Option<usize>,
    keep_going: bool,
    json: bool,
    final_output_only: bool,
    options: RenderCommandOptions<'_>,
) -> Result<()> {
    use std::io::BufRead;

    let stdin = io::stdin();
    let retain_manifest =
        final_output_only || std::env::var_os("FM_DISABLE_IN_MEMORY_BATCH_MANIFEST").is_none();
    let retain_plan = std::env::var_os("FM_DISABLE_IN_MEMORY_BATCH_PLAN").is_none();
    let batch_plan = retain_plan
        .then(|| BatchRenderPlan::new(inputs, out_dir, jobs, true, &options))
        .transpose()?;
    let mut cache_session = BatchRenderCacheSession::default();
    let mut trust_first_epoch = false;
    if retain_manifest {
        let cache_path = Path::new(out_dir).join(BATCH_RENDER_CACHE_FILE);
        let admit_clean_batch = std::env::var_os("FM_DISABLE_DURABLE_BATCH_CERTIFICATE").is_none();
        cache_session.begin_stream(&cache_path, batch_plan.as_ref(), admit_clean_batch)?;
        trust_first_epoch = cache_session.trusted_batch.is_some();
    }
    let mut epoch = 0usize;
    cache_session.defer_output_writes = final_output_only;
    for (line_index, line) in stdin.lock().lines().enumerate() {
        let line_number = line_index + 1;
        let line =
            line.with_context(|| format!("cannot read change set from input line {line_number}"))?;
        let Some(changed_inputs) = parse_batch_change_set_line(&line, line_number)? else {
            continue;
        };
        epoch += 1;
        cmd_render_batch(
            inputs,
            out_dir,
            jobs,
            keep_going,
            json,
            BatchCachePolicy {
                use_cache: true,
                // The first epoch validates the on-disk base in full. Later epochs can trust the
                // process-owned manifest. A clean predecessor certificate proves that base before
                // epoch one; its on-disk copy was invalidated before this process could write.
                trust_change_set: !retain_manifest || trust_first_epoch || epoch > 1,
                changed_inputs: &changed_inputs,
                source_overrides: None,
                session: retain_manifest.then_some(&mut cache_session),
                plan: batch_plan.as_ref(),
                report: None,
            },
            options.clone(),
        )
        .with_context(|| format!("change-set epoch {epoch} failed"))?;

        let mut stdout = io::stdout().lock();
        serde_json::to_writer(
            &mut stdout,
            &serde_json::json!({
                "epoch": epoch,
                "input_line": line_number,
                "status": "ok"
            }),
        )?;
        writeln!(stdout)?;
        stdout.flush()?;
    }
    if retain_manifest {
        let (outputs, bytes) = cache_session.materialize_deferred_outputs()?;
        if final_output_only {
            eprintln!("materialized {outputs} final output(s), {bytes} bytes at stream EOF");
        }
        cache_session.flush(Path::new(out_dir))?;
    }
    Ok(())
}

fn report_executing_elf_sha256_once() -> Result<()> {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicBool, Ordering};

    static REPORTED: AtomicBool = AtomicBool::new(false);
    static DIGEST: OnceLock<std::result::Result<String, String>> = OnceLock::new();

    if std::env::var_os("FM_SELF_REPORT_ELF_SHA256").is_none()
        || REPORTED.swap(true, Ordering::Relaxed)
    {
        return Ok(());
    }
    let digest = DIGEST.get_or_init(|| {
        let executable = std::env::current_exe()
            .map_err(|error| format!("cannot resolve executing ELF: {error}"))?;
        let bytes = std::fs::read(&executable).map_err(|error| {
            format!(
                "cannot read executing ELF {}: {error}",
                executable.display()
            )
        })?;
        Ok(sha256_hex(&bytes))
    });
    match digest {
        Ok(digest) => {
            eprintln!("executing_elf_sha256={digest}");
            Ok(())
        }
        Err(error) => anyhow::bail!("{error}"),
    }
}

/// Render every input concurrently as one job.
///
/// The incumbent renders on a single JavaScript main thread, so its cost for N diagrams is the
/// sum of N. Each diagram here is an independent parse -> layout -> render with no shared mutable
/// state, so the batch is shared-nothing and scales with cores rather than accumulating.
///
/// Determinism: work is dispatched by index and every result is written to its own file, so the
/// output set does not depend on completion order. Per-file bytes are identical to `render`
/// because both go through `render_source` with the same resolved options.
fn cmd_render_batch(
    inputs: &[String],
    out_dir: &str,
    jobs: Option<usize>,
    keep_going: bool,
    json: bool,
    cache_policy: BatchCachePolicy<'_>,
    options: RenderCommandOptions<'_>,
) -> Result<()> {
    use rayon::prelude::*;

    let BatchCachePolicy {
        use_cache,
        trust_change_set,
        changed_inputs,
        source_overrides,
        mut session,
        plan,
        report,
    } = cache_policy;

    if trust_change_set && !use_cache {
        anyhow::bail!("--trust-change-set requires the persistent batch cache");
    }

    let supplied_plan = plan.is_some();
    let owned_plan = plan
        .is_none()
        .then(|| BatchRenderPlan::new(inputs, out_dir, jobs, use_cache, &options))
        .transpose()?;
    let plan = plan
        .or(owned_plan.as_ref())
        .ok_or_else(|| anyhow::anyhow!("internal error: batch plan was not constructed"))?;
    let changed_input_set = changed_inputs
        .iter()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>();
    if changed_input_set.len() != changed_inputs.len() {
        anyhow::bail!("--changed-input contains a duplicate path");
    }
    if let Some(unknown) = changed_input_set
        .iter()
        .find(|input| !plan.input_set.contains(**input))
    {
        anyhow::bail!("--changed-input {unknown:?} is not one of this batch's inputs");
    }

    let requested = plan.requested_workers;
    let started = Instant::now();
    let change_set_optimization_enabled = std::env::var_os("FM_DISABLE_BATCH_CHANGE_SET").is_none();
    let trusted_change_set_active = trust_change_set && change_set_optimization_enabled;
    let sparse_epoch_enabled = std::env::var_os("FM_DISABLE_SPARSE_CHANGE_SET_EPOCH").is_none();

    // A resident stream has already proved every unlisted input and output during its first
    // recovery epoch. Execute later epochs over the changed slice itself: this removes every
    // batch-wide Vec, map pass and Rayon dispatch for the hundreds of diagrams the caller has
    // certified unchanged. The process-owned manifest remains the source of truth, while the
    // carry preserves whole-batch accounting without rescanning it.
    if supplied_plan
        && report.is_none()
        && !json
        && trusted_change_set_active
        && sparse_epoch_enabled
        && changed_inputs.len() < inputs.len()
        && let Some(carry) = session
            .as_deref()
            .and_then(|session| session.sparse_report_carry(plan, changed_inputs))
    {
        if changed_inputs.is_empty() {
            eprintln!(
                "rendered {0}/{0} diagram(s) ({0} persistent hits, 0 identical renders reused, 0 \
                 shared prefix parses reused / 0 bytes), {1} bytes, {2} requested worker(s), 0 \
                 active worker(s), {3:.3} ms",
                carry.logical_input_count,
                carry.inherited_total_bytes,
                requested,
                started.elapsed().as_secs_f64() * 1000.0,
            );
            return Ok(());
        }
        let projected_plan = std::env::var_os("FM_DISABLE_EPOCH_PLAN_PROJECTION")
            .is_none()
            .then(|| plan.project(changed_inputs))
            .transpose()?;
        return cmd_render_batch(
            changed_inputs,
            out_dir,
            Some(requested),
            keep_going,
            false,
            BatchCachePolicy {
                use_cache,
                trust_change_set,
                changed_inputs,
                source_overrides,
                session: session.as_deref_mut(),
                plan: projected_plan.as_ref(),
                report: Some(carry),
            },
            options,
        );
    }

    let cache_path = &plan.cache_path;
    let cache_active = plan.cache_active;
    let option_cache_digest = &plan.option_cache_digest;
    let mut session_lease = if cache_active {
        session.map(|session| session.lease(cache_path))
    } else {
        None
    };
    let (mut disk_cache, mut disk_cache_modified) = if cache_active && session_lease.is_none() {
        load_batch_render_cache(cache_path)
    } else {
        (BatchRenderCacheManifest::default(), None)
    };
    if cache_active && session_lease.is_none() && disk_cache.clean_batch.take().is_some() {
        let encoded = serde_json::to_vec(&disk_cache)?;
        std::fs::write(cache_path, encoded)
            .with_context(|| format!("cannot invalidate {}", cache_path.display()))?;
        disk_cache_modified = cache_path
            .metadata()
            .and_then(|metadata| metadata.modified())
            .ok();
    }
    let (prior_cache, prior_cache_modified) = if let Some(lease) = session_lease.as_ref() {
        (&lease.manifest, lease.manifest_modified)
    } else {
        (&disk_cache, disk_cache_modified)
    };
    let cache_key_for = |digest: &str| -> Option<String> {
        option_cache_digest
            .as_ref()
            .map(|options| format!("{digest}:{options}"))
    };
    let modified_key = |metadata: &std::fs::Metadata| -> Option<String> {
        metadata
            .modified()
            .ok()?
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_nanos().to_string())
    };

    // The hot incremental path is admitted before Rayon exists. A caller-certified complete
    // change set turns every unlisted manifest entry into an O(1) key check; ordinary callers keep
    // the source/output metadata proof. Either path can bypass source reads, parser planning,
    // pressure sampling, thread startup, layout and rendering.
    let early_cached_results = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            let options_key = option_cache_digest.as_ref()?;
            let destination = &plan.destinations[index];
            let entry = prior_cache.entries.get(&plan.destination_names[index])?;

            if trusted_change_set_active {
                if changed_input_set.contains(input.as_str())
                    || !batch_cache_entry_matches_key(entry, options_key)
                {
                    return None;
                }
                let bytes = usize::try_from(entry.bytes).ok()?;
                return Some((plan.destination_displays[index].clone(), bytes));
            }

            let manifest_modified = prior_cache_modified?;
            let source_metadata = Path::new(input).metadata().ok()?;
            let output_metadata = destination.metadata().ok()?;
            if !batch_cache_entry_matches_early(
                entry,
                options_key,
                source_metadata.len(),
                &modified_key(&source_metadata)?,
                output_metadata.len(),
                output_metadata.modified().ok()?,
                manifest_modified,
            ) {
                return None;
            }
            let bytes = usize::try_from(entry.bytes).ok()?;
            Some((plan.destination_displays[index].clone(), bytes))
        })
        .collect::<Vec<_>>();
    let sparse_cache_enabled = std::env::var_os("FM_DISABLE_SPARSE_BATCH_CACHE").is_none();
    if !inputs.is_empty() && early_cached_results.iter().all(Option::is_some) {
        let elapsed = started.elapsed();
        let inherited_diagrams = report.map_or(0, |carry| carry.inherited_diagrams);
        let inherited_cache_hits = report.map_or(0, |carry| carry.inherited_cache_hits);
        let logical_input_count = report.map_or(inputs.len(), |carry| carry.logical_input_count);
        let mut total_bytes = report.map_or(0, |carry| carry.inherited_total_bytes);
        for (input, (path, bytes)) in inputs.iter().zip(early_cached_results.iter().flatten()) {
            total_bytes += *bytes;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "input": input, "output": path, "bytes": bytes, "status": "ok"
                    })
                );
            }
        }
        if !json {
            eprintln!(
                "rendered {}/{} diagram(s) ({} persistent hits, 0 identical renders reused, 0 \
                 shared prefix parses reused / 0 bytes), {total_bytes} bytes, {requested} \
                 requested worker(s), 0 active worker(s), {:.3} ms",
                inherited_diagrams + inputs.len(),
                logical_input_count,
                inherited_cache_hits + inputs.len(),
                elapsed.as_secs_f64() * 1000.0,
            );
        }
        if let Some(lease) = session_lease.as_mut() {
            lease.session.trusted_batch = Some(TrustedBatchSummary {
                plan_key: report.map_or_else(|| plan.key.clone(), |carry| carry.plan_key.into()),
                input_count: logical_input_count,
                total_bytes,
            });
        }
        return Ok(());
    }

    // Host pressure, sampled ONCE for batches that actually render. See
    // `render_source_with_pressure`: this is a host property, so per-diagram sampling is work the
    // output never depends on, and its `/proc/self/status` read serializes concurrent workers.
    let pressure = match session_lease.as_ref() {
        Some(lease) => lease.session.pressure_report(),
        None => Arc::new(MermaidNativePressureSignals::sample().into_report()),
    };

    // Phase 1: carry certified digests across metadata hits; read and content-address only misses.
    // The disabled arm below preserves the former read-all path for exact-binary mechanism tests.
    let cached_source_digest = |index: usize| -> Option<String> {
        early_cached_results[index].as_ref()?;
        prior_cache
            .entries
            .get(&plan.destination_names[index])
            .map(|entry| entry.source_digest.clone())
    };
    let load_one = |(index, input): (usize, &String)| -> Result<(String, String)> {
        if sparse_cache_enabled && let Some(digest) = cached_source_digest(index) {
            return Ok((String::new(), digest));
        }
        if let Some(source) = source_overrides.and_then(|sources| sources.get(input)) {
            return Ok((source.clone(), sha256_hex(source.as_bytes())));
        }
        let source = load_input(input, options.max_input_bytes)?;
        let digest = sha256_hex(source.as_bytes());
        Ok((source, digest))
    };
    let early_miss_count = early_cached_results
        .iter()
        .filter(|entry| entry.is_none())
        .count();
    let pool_threads = if sparse_cache_enabled {
        requested.min(early_miss_count.max(1))
    } else {
        requested
    };
    let pool = if pool_threads == 1 {
        None
    } else if let Some(lease) = session_lease.as_ref()
        && lease.session.reuse_worker_pool
    {
        Some(lease.session.worker_pool(pool_threads)?)
    } else {
        Some(Arc::new(BatchRenderWorkerPool::new(pool_threads)?))
    };
    let run_all =
        |f: &(dyn Fn(usize) -> Result<(String, usize)> + Sync)| -> Vec<Result<(String, usize)>> {
            match &pool {
                None => (0..inputs.len()).map(f).collect(),
                Some(p) => p
                    .pool
                    .install(|| (0..inputs.len()).into_par_iter().map(f).collect()),
            }
        };

    let loaded: Vec<Result<(String, String)>> = match &pool {
        None => inputs.iter().enumerate().map(load_one).collect(),
        Some(p) => p
            .pool
            .install(|| inputs.par_iter().enumerate().map(load_one).collect()),
    };

    // A hit is admitted only when the source+configuration+executable key matches, the destination
    // still has the recorded length, and it has not changed since the manifest was committed.
    // Hits never enter the parser plan or the render pool.
    let mut cached_results = loaded
        .iter()
        .enumerate()
        .map(|(index, loaded)| {
            if sparse_cache_enabled && let Some(cached) = early_cached_results[index].as_ref() {
                return Some(cached.clone());
            }
            let (_, digest) = loaded.as_ref().ok()?;
            let key = cache_key_for(digest)?;
            let manifest_modified = prior_cache_modified?;
            let destination = &plan.destinations[index];
            let entry = prior_cache.entries.get(&plan.destination_names[index])?;
            if entry.key != key {
                return None;
            }
            let metadata = destination.metadata().ok()?;
            if metadata.len() != entry.bytes || metadata.modified().ok()? > manifest_modified {
                return None;
            }
            let bytes = usize::try_from(entry.bytes).ok()?;
            Some((plan.destination_displays[index].clone(), bytes))
        })
        .collect::<Vec<_>>();
    let revision_cache_active = session_lease
        .as_ref()
        .is_some_and(|lease| lease.session.reuse_revision_outputs);
    let defer_output_writes = session_lease
        .as_ref()
        .is_some_and(|lease| lease.session.defer_output_writes);
    if revision_cache_active {
        for (index, loaded) in loaded.iter().enumerate() {
            if cached_results[index].is_some() {
                continue;
            }
            let Ok((_, digest)) = loaded else {
                continue;
            };
            let Some(key) = cache_key_for(digest) else {
                continue;
            };
            let bytes = session_lease
                .as_mut()
                .and_then(|lease| lease.session.revision_output(&key));
            let Some(bytes) = bytes else {
                continue;
            };
            let destination = &plan.destinations[index];
            let output_deferred = session_lease.as_mut().is_some_and(|lease| {
                lease
                    .session
                    .stage_output_if_deferred(destination, Arc::clone(&bytes))
            });
            if !output_deferred {
                std::fs::write(destination, bytes.as_slice())
                    .with_context(|| format!("cannot write {}", destination.display()))?;
            }
            cached_results[index] = Some((plan.destination_displays[index].clone(), bytes.len()));
        }
    }
    let cache_hit_count = report.map_or(0, |carry| carry.inherited_cache_hits)
        + cached_results.iter().flatten().count();

    // Phase 2: how many inputs share each digest? A diagram whose source repeats in the batch --
    // the same architecture snippet embedded across a docs site, or an unchanged file in a CI
    // re-render -- is the SAME parse, layout and render every time. Rendering it once and reusing
    // the bytes deletes that work outright rather than doing it faster. Only digests that
    // actually repeat are memoized, so peak memory tracks the duplicated set, not the batch.
    let mut digest_counts: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for (index, entry) in loaded.iter().enumerate() {
        if cached_results[index].is_none()
            && let Ok((_, digest)) = entry
        {
            *digest_counts.entry(digest.as_str()).or_insert(0) += 1;
        }
    }
    // Lowest input index owns each digest, so which diagram is rendered never depends on
    // completion order.
    let mut owner_of: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for (index, entry) in loaded.iter().enumerate() {
        if cached_results[index].is_none()
            && let Ok((_, digest)) = entry
        {
            owner_of.entry(digest.as_str()).or_insert(index);
        }
    }

    // Compile every exact, complete flowchart-prefix subgraph shared by two or more distinct
    // owners. Each owner still gets an independent IR, layout and render; only repeated prefix
    // tokenization/lowering/interning is removed. The plan is immutable, so suffix parsing remains
    // shared-nothing and runs on the same Rayon pool as the rest of each diagram pipeline.
    let owner_indices = loaded
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let (_, digest) = entry.as_ref().ok()?;
            (owner_of.get(digest.as_str()) == Some(&index)).then_some(index)
        })
        .collect::<Vec<_>>();
    let owner_sources = owner_indices
        .iter()
        .filter_map(|&index| {
            loaded[index]
                .as_ref()
                .ok()
                .map(|(source, _)| source.as_str())
        })
        .collect::<Vec<_>>();
    let parse_plan =
        FlowchartBatchParsePlan::new(&owner_sources, options.parse_mode, &options.parser_config);
    let parse_plan_stats = parse_plan.stats();
    let mut parse_plan_position = vec![usize::MAX; inputs.len()];
    for (position, &index) in owner_indices.iter().enumerate() {
        parse_plan_position[index] = position;
    }
    let reused = loaded
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            cached_results[index]
                .is_none()
                .then(|| entry.as_ref().ok())
                .flatten()
        })
        .filter(|(_, digest)| digest_counts.get(digest.as_str()).copied().unwrap_or(0) > 1)
        .count()
        - owner_of
            .iter()
            .filter(|(d, _)| digest_counts.get(**d).copied().unwrap_or(0) > 1)
            .count();

    // Phase 3: render each owner once; keep bytes only for digests that repeat.
    let shared: std::sync::Mutex<std::collections::BTreeMap<String, std::sync::Arc<Vec<u8>>>> =
        std::sync::Mutex::new(std::collections::BTreeMap::new());
    let render_owner = |state: &mut (FlowchartBatchParseScratch, SvgBatchRenderer),
                        index: usize|
     -> Result<(String, usize)> {
        if let Some(cached) = cached_results[index].as_ref() {
            return Ok(cached.clone());
        }
        let (source, digest) = match &loaded[index] {
            Ok(pair) => pair,
            Err(error) => return Err(anyhow::anyhow!("{error:#}")),
        };
        if owner_of.get(digest.as_str()) != Some(&index) {
            return Ok((String::new(), 0)); // not the owner; written in phase 4
        }
        let total_start = Instant::now();
        let parse_start = Instant::now();
        let (parse_scratch, renderer) = state;
        parse_plan.with_parse_scratch(
            parse_plan_position[index],
            source,
            parse_scratch,
            |parsed| {
                let parse_time = parse_start.elapsed();
                let mut budget_broker = MermaidBudgetLedger::new(&pressure);
                budget_broker
                    .record_parse(u64::try_from(parse_time.as_millis()).unwrap_or(u64::MAX));
                let outcome = render_batch_parse_ref_with_pressure(
                    source,
                    parsed,
                    RenderTiming {
                        parse_time,
                        total_start,
                    },
                    budget_broker,
                    &options,
                    &pressure,
                    renderer,
                )?;
                if !defer_output_writes {
                    let destination = &plan.destinations[index];
                    std::fs::write(destination, &outcome.rendered)
                        .with_context(|| format!("cannot write {}", destination.display()))?;
                }
                let length = outcome.rendered.len();
                if defer_output_writes
                    || revision_cache_active
                    || digest_counts.get(digest.as_str()).copied().unwrap_or(0) > 1
                {
                    shared
                        .lock()
                        .map_err(|_| anyhow::anyhow!("render cache poisoned"))?
                        .insert(digest.clone(), std::sync::Arc::new(outcome.rendered));
                }
                Ok((plan.destination_displays[index].clone(), length))
            },
        )
    };
    // A Rayon worker normally has to cold-build the same certified prefix before its private
    // renderer can reuse suffix deltas. Render one representative on the coordinator, then fork
    // that immutable snapshot into every worker. The representative's output is retained in its
    // original slot, so no diagram is rendered twice and output ordering is unchanged.
    let seed_owner_index = if options.format == OutputFormat::Svg
        && std::env::var_os("FM_DISABLE_BATCH_PREFIX_SEED").is_none()
    {
        pool.as_ref().and_then(|_| {
            owner_indices.iter().copied().find(|&index| {
                parse_plan
                    .reusable_prefix_group(parse_plan_position[index])
                    .is_some()
            })
        })
    } else {
        None
    };
    let mut coordinator_state = (
        FlowchartBatchParseScratch::default(),
        SvgBatchRenderer::default(),
    );
    let seeded_owner_result = seed_owner_index.map(|index| {
        let result =
            render_owner(&mut coordinator_state, index).map_err(|error| format!("{error:#}"));
        (index, result)
    });
    let renderer_seed: Option<SvgBatchRendererSeed> = coordinator_state.1.seed();
    let initial_state = || {
        (
            FlowchartBatchParseScratch::default(),
            renderer_seed
                .as_ref()
                .map_or_else(SvgBatchRenderer::default, SvgBatchRenderer::from_seed),
        )
    };
    let render_or_seeded = |state: &mut (FlowchartBatchParseScratch, SvgBatchRenderer),
                            index: usize| {
        if let Some((seeded_index, result)) = seeded_owner_result.as_ref()
            && *seeded_index == index
        {
            return result
                .as_ref()
                .map(|(path, bytes)| (path.clone(), *bytes))
                .map_err(|error| anyhow::anyhow!("{error}"));
        }
        render_owner(state, index)
    };
    let owner_results: Vec<Result<(String, usize)>> = match &pool {
        None => {
            let mut state = initial_state();
            (0..inputs.len())
                .map(|index| render_or_seeded(&mut state, index))
                .collect()
        }
        Some(pool) => pool.pool.install(|| {
            (0..inputs.len())
                .into_par_iter()
                .map_init(initial_state, render_or_seeded)
                .collect()
        }),
    };
    if revision_cache_active {
        let rendered_revisions = {
            let cache = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("render cache poisoned"))?;
            cache
                .iter()
                .filter_map(|(digest, bytes)| {
                    cache_key_for(digest).map(|key| (key, Arc::clone(bytes)))
                })
                .collect::<Vec<_>>()
        };
        if let Some(lease) = session_lease.as_mut() {
            for (key, bytes) in rendered_revisions {
                lease.session.remember_revision_output(key, bytes);
            }
        }
    }

    // Phase 4: every non-owner duplicate copies its owner's bytes. No parse, no layout, no render.
    //
    // The pass exists only to service duplicates. When no digest repeats -- a CI re-render of
    // distinct diagrams, a docs build where every page has its own diagram, and the shape of the
    // certified 512-diagram corpus -- every index owns its own digest, so each task would take the
    // early return below and do nothing but clone a path phase 3 already produced. Fanning N tasks
    // across the pool and joining them to copy N strings is pure overhead, so hand phase 3's
    // results straight through instead. Byte-identical: phase 3 wrote every file in this case.
    let results = if reused == 0 {
        owner_results
    } else {
        let write_duplicate = |index: usize| -> Result<(String, usize)> {
            if let Some(cached) = cached_results[index].as_ref() {
                return Ok(cached.clone());
            }
            let (_, digest) = match &loaded[index] {
                Ok(pair) => pair,
                Err(_) => return Ok((String::new(), 0)),
            };
            if owner_of.get(digest.as_str()) == Some(&index) {
                return owner_results[index]
                    .as_ref()
                    .map(|(p, n)| (p.clone(), *n))
                    .map_err(|e| anyhow::anyhow!("{e:#}"));
            }
            let bytes = {
                let cache = shared
                    .lock()
                    .map_err(|_| anyhow::anyhow!("render cache poisoned"))?;
                cache.get(digest.as_str()).cloned()
            };
            let Some(bytes) = bytes else {
                // Owner failed; surface that failure against this input too.
                return Err(anyhow::anyhow!(
                    "duplicate of an input that failed to render"
                ));
            };
            if !defer_output_writes {
                let destination = &plan.destinations[index];
                std::fs::write(destination, bytes.as_slice())
                    .with_context(|| format!("cannot write {}", destination.display()))?;
            }
            Ok((plan.destination_displays[index].clone(), bytes.len()))
        };
        run_all(&write_duplicate)
    };
    if defer_output_writes {
        let deferred = {
            let cache = shared
                .lock()
                .map_err(|_| anyhow::anyhow!("render cache poisoned"))?;
            let mut deferred = Vec::new();
            for (index, result) in results.iter().enumerate() {
                if result.is_err() || cached_results[index].is_some() {
                    continue;
                }
                let (_, digest) = loaded[index]
                    .as_ref()
                    .map_err(|error| anyhow::anyhow!("{error:#}"))?;
                let bytes = cache.get(digest.as_str()).cloned().ok_or_else(|| {
                    anyhow::anyhow!(
                        "rendered output for {} was not retained for final materialization",
                        inputs[index]
                    )
                })?;
                deferred.push((plan.destinations[index].clone(), bytes));
            }
            deferred
        };
        let lease = session_lease.as_mut().ok_or_else(|| {
            anyhow::anyhow!("final-output-only stream lost its resident cache session")
        })?;
        for (destination, bytes) in deferred {
            if !lease.session.stage_output_if_deferred(&destination, bytes) {
                anyhow::bail!("final-output-only stream stopped deferring output writes");
            }
        }
    }
    if cache_active {
        let mut commit_manifest = false;
        {
            let next_cache = if let Some(lease) = session_lease.as_mut() {
                &mut lease.manifest
            } else {
                &mut disk_cache
            };
            for (index, result) in results.iter().enumerate() {
                let Ok((_, bytes)) = result else {
                    continue;
                };
                if change_set_optimization_enabled
                    && sparse_cache_enabled
                    && early_cached_results[index].is_some()
                {
                    // This entry was admitted from the prior manifest and its source was
                    // deliberately not reopened. Re-statting it here cannot change the entry;
                    // only misses need a refreshed source timestamp after materialization.
                    continue;
                }
                let Ok((_, digest)) = &loaded[index] else {
                    continue;
                };
                let Some(key) = cache_key_for(digest) else {
                    continue;
                };
                let Some(options_key) = option_cache_digest.as_ref() else {
                    continue;
                };
                let entry_name = &plan.destination_names[index];
                let (source_bytes, source_modified_ns) = if let Some(source) =
                    source_overrides.and_then(|sources| sources.get(&inputs[index]))
                {
                    let Ok(source_bytes) = u64::try_from(source.len()) else {
                        continue;
                    };
                    let source_modified_ns = next_cache
                        .entries
                        .get(entry_name)
                        .map(|entry| entry.source_modified_ns.clone())
                        .unwrap_or_default();
                    (source_bytes, source_modified_ns)
                } else {
                    let Ok(source_metadata) = Path::new(&inputs[index]).metadata() else {
                        continue;
                    };
                    let Some(source_modified_ns) = modified_key(&source_metadata) else {
                        continue;
                    };
                    (source_metadata.len(), source_modified_ns)
                };
                let entry = BatchRenderCacheEntry {
                    key,
                    source_digest: digest.clone(),
                    options_key: options_key.clone(),
                    source_bytes,
                    source_modified_ns,
                    bytes: u64::try_from(*bytes).unwrap_or(u64::MAX),
                };
                commit_manifest |= next_cache.entries.get(entry_name) != Some(&entry);
                next_cache.entries.insert(entry_name.clone(), entry);
            }
        }
        if commit_manifest {
            if let Some(lease) = session_lease.as_mut() {
                lease.dirty = true;
            } else {
                let encoded = serde_json::to_vec(&disk_cache)?;
                std::fs::write(cache_path, encoded)
                    .with_context(|| format!("cannot write {}", cache_path.display()))?;
            }
        }
    }
    let elapsed = started.elapsed();

    let logical_input_count = report.map_or(inputs.len(), |carry| carry.logical_input_count);
    let mut rendered = report.map_or(0, |carry| carry.inherited_diagrams);
    let mut total_bytes = report.map_or(0, |carry| carry.inherited_total_bytes);
    let mut failures: Vec<String> = Vec::new();
    for (input, result) in inputs.iter().zip(results) {
        match result {
            Ok((path, bytes)) => {
                rendered += 1;
                total_bytes += bytes;
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "input": input, "output": path, "bytes": bytes, "status": "ok"
                        })
                    );
                }
            }
            Err(error) => {
                let message = format!("{input}: {error:#}");
                if json {
                    println!(
                        "{}",
                        serde_json::json!({
                            "input": input, "status": "error", "error": message
                        })
                    );
                }
                failures.push(message);
                if !keep_going {
                    break;
                }
            }
        }
    }

    if !json {
        eprintln!(
            "rendered {rendered}/{} diagram(s) ({cache_hit_count} persistent hits, {reused} \
             identical renders reused, {} shared \
             prefix parses reused / {} bytes), {total_bytes} bytes, {} requested worker(s), \
             {pool_threads} active worker(s), {:.3} ms",
            logical_input_count,
            parse_plan_stats.reused_prefix_parses,
            parse_plan_stats.reused_prefix_bytes,
            requested,
            elapsed.as_secs_f64() * 1000.0,
        );
    }
    if !failures.is_empty() {
        for failure in &failures {
            eprintln!("error: {failure}");
        }
        anyhow::bail!(
            "{} of {} diagram(s) failed",
            failures.len(),
            logical_input_count
        );
    }
    if let Some(lease) = session_lease.as_mut() {
        lease.session.trusted_batch = Some(TrustedBatchSummary {
            plan_key: report.map_or_else(|| plan.key.clone(), |carry| carry.plan_key.into()),
            input_count: logical_input_count,
            total_bytes,
        });
    }
    Ok(())
}

fn render_format(
    ir: &MermaidDiagramIr,
    layout: &fm_layout::DiagramLayout,
    format: OutputFormat,
    options: RenderSurfaceOptions<'_>,
) -> Result<(Vec<u8>, Option<u32>, Option<u32>)> {
    let RenderSurfaceOptions {
        theme,
        font_size,
        svg_base_config,
        term_base_config,
        show_back_edges,
        show_minimap,
        embed_source_spans,
        dimensions: (width, height),
        degradation,
    } = options;
    let filtered_layout = (!show_back_edges).then(|| layout_without_back_edges(layout));
    let render_layout = filtered_layout.as_ref().unwrap_or(layout);
    match format {
        OutputFormat::Svg => {
            let mut svg_config =
                build_svg_render_config(&svg_base_config, theme, font_size, embed_source_spans);
            svg_config.apply_degradation(&degradation);
            let svg = render_svg_with_layout(ir, render_layout, &svg_config);
            // Extract dimensions from SVG if available
            let (w, h) = extract_svg_dimensions(&svg);
            Ok((svg.into_bytes(), w, h))
        }

        OutputFormat::Png => {
            #[cfg(feature = "png")]
            {
                let mut svg_config =
                    build_svg_render_config(&svg_base_config, theme, font_size, embed_source_spans);
                svg_config.apply_degradation(&degradation);
                make_svg_render_config_raster_safe(&mut svg_config);
                let svg = render_svg_with_layout(ir, render_layout, &svg_config);
                let svg = resolve_svg_custom_properties_for_rasterization(&svg);
                let (png, px_width, px_height) = svg_to_png(&svg, width, height)?;
                Ok((png, Some(px_width), Some(px_height)))
            }

            #[cfg(not(feature = "png"))]
            {
                anyhow::bail!(
                    "PNG output requires the 'png' feature. \
                     Rebuild with: cargo build --features png"
                );
            }
        }

        OutputFormat::Term => {
            warn_if_unknown_theme(theme, svg_base_config.theme);
            let (cols, rows) = terminal_size(width, height);
            let mut config = term_base_config;
            config.apply_degradation(&degradation);
            let result = render_term_with_layout_and_config(ir, render_layout, &config, cols, rows);
            let output = if show_minimap {
                let minimap = fm_render_term::minimap::render_minimap_from_layout(
                    render_layout,
                    &fm_render_term::MinimapConfig {
                        max_width: cols.saturating_div(4).clamp(12, 28),
                        max_height: rows.saturating_div(4).clamp(6, 14),
                        glyph_mode: config.glyph_mode,
                        ..Default::default()
                    },
                    None,
                );
                fm_render_term::minimap::overlay_minimap(
                    &result.output,
                    &minimap,
                    result.width,
                    result.height,
                    fm_render_term::MinimapCorner::TopRight,
                )
            } else {
                result.output
            };
            Ok((
                output.into_bytes(),
                Some(u32::try_from(result.width).unwrap_or(u32::MAX)),
                Some(u32::try_from(result.height).unwrap_or(u32::MAX)),
            ))
        }

        OutputFormat::Ascii => {
            warn_if_unknown_theme(theme, svg_base_config.theme);
            let (cols, rows) = terminal_size(width, height);
            let mut config = term_base_config;
            if matches!(config.tier, MermaidTier::Auto) {
                config.tier = MermaidTier::Compact;
            }
            config.glyph_mode = fm_core::MermaidGlyphMode::Ascii;
            config.apply_degradation(&degradation);
            let result = render_term_with_layout_and_config(ir, render_layout, &config, cols, rows);
            let output = if show_minimap {
                let minimap = fm_render_term::minimap::render_minimap_from_layout(
                    render_layout,
                    &fm_render_term::MinimapConfig {
                        max_width: cols.saturating_div(4).clamp(12, 28),
                        max_height: rows.saturating_div(4).clamp(6, 14),
                        glyph_mode: config.glyph_mode,
                        ..Default::default()
                    },
                    None,
                );
                fm_render_term::minimap::overlay_minimap(
                    &result.output,
                    &minimap,
                    result.width,
                    result.height,
                    fm_render_term::MinimapCorner::TopRight,
                )
            } else {
                result.output
            };
            Ok((
                output.into_bytes(),
                Some(u32::try_from(result.width).unwrap_or(u32::MAX)),
                Some(u32::try_from(result.height).unwrap_or(u32::MAX)),
            ))
        }
    }
}

fn build_svg_render_config(
    base: &SvgRenderConfig,
    theme: &str,
    font_size: Option<f32>,
    embed_source_spans: bool,
) -> SvgRenderConfig {
    let mut svg_config = base.clone();
    svg_config.theme = resolve_theme_preset(theme, base.theme);
    svg_config.include_source_spans = embed_source_spans;
    if let Some(size) = normalize_positive_font_size(font_size) {
        svg_config.font_size = size;
    }
    svg_config
}

#[cfg(feature = "png")]
fn make_svg_render_config_raster_safe(config: &mut SvgRenderConfig) {
    // usvg/resvg only supports a browser-free subset of the CSS emitted for
    // interactive SVG output. Prefer a static attribute-driven SVG for PNG so
    // rasterization remains deterministic across theme presets.
    config.responsive = false;
    config.embed_theme_css = false;
    config.animations_enabled = false;
    config.print_optimized = false;
    config.shadows = false;
    config.glow_enabled = false;
    config.a11y.accessibility_css = false;
}

fn normalize_positive_font_size(font_size: Option<f32>) -> Option<f32> {
    font_size.filter(|size| size.is_finite() && *size > 0.0)
}

fn resolve_theme_preset(theme: &str, fallback: ThemePreset) -> ThemePreset {
    match theme.parse::<ThemePreset>() {
        Ok(theme_preset) => theme_preset,
        Err(_err) => {
            warn!(
                "Unknown theme '{theme}', falling back to '{}'",
                fallback.as_str()
            );
            fallback
        }
    }
}

fn warn_if_unknown_theme(theme: &str, fallback: ThemePreset) {
    if theme.parse::<ThemePreset>().is_err() {
        warn!(
            "Unknown theme '{theme}', falling back to '{}'",
            fallback.as_str()
        );
    }
}

fn terminal_size(width: Option<u32>, height: Option<u32>) -> (usize, usize) {
    let default_cols = 80_usize;
    let default_rows = 24_usize;

    (
        width
            .filter(|value| *value > 0)
            .map_or(default_cols, |w| w as usize),
        height
            .filter(|value| *value > 0)
            .map_or(default_rows, |h| h as usize),
    )
}

fn extract_svg_dimensions(svg: &str) -> (Option<u32>, Option<u32>) {
    // Simple regex-free extraction of width/height from SVG, with viewBox fallback for
    // responsive SVGs that use percentage sizing.
    let tag = svg_root_tag(svg);
    let width = tag.find("width=\"").and_then(|i| {
        let start = i + 7;
        let end = tag[start..].find('"').map(|e| start + e)?;
        parse_svg_dimension_value(&tag[start..end])
    });

    let height = tag.find("height=\"").and_then(|i| {
        let start = i + 8;
        let end = tag[start..].find('"').map(|e| start + e)?;
        parse_svg_dimension_value(&tag[start..end])
    });

    match (width, height) {
        (Some(width), Some(height)) => (Some(width), Some(height)),
        _ => extract_viewbox_dimensions(tag).unwrap_or((width, height)),
    }
}

fn parse_svg_dimension_value(value: &str) -> Option<u32> {
    value
        .parse::<f32>()
        .ok()
        .filter(|parsed| parsed.is_finite() && *parsed > 0.0)
        .map(|parsed| {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let res = parsed.ceil() as u32;
            res
        })
}

fn extract_viewbox_dimensions(svg: &str) -> Option<(Option<u32>, Option<u32>)> {
    let start = svg.find("viewBox=\"")? + 9;
    let end = svg[start..].find('"').map(|offset| start + offset)?;
    let mut parts = svg[start..end]
        .split(|ch: char| ch.is_ascii_whitespace() || ch == ',')
        .filter(|part| !part.is_empty());
    let _min_x = parts.next()?;
    let _min_y = parts.next()?;
    let width = parse_svg_dimension_value(parts.next()?);
    let height = parse_svg_dimension_value(parts.next()?);
    Some((width, height))
}

fn svg_root_tag(svg: &str) -> &str {
    let Some(start) = svg.find("<svg") else {
        return svg;
    };
    let Some(end_rel) = svg[start..].find('>') else {
        return svg;
    };
    let end = start + end_rel + 1;
    &svg[start..end]
}

#[cfg(feature = "png")]
fn resolve_svg_custom_properties_for_rasterization(svg: &str) -> String {
    let mut custom_properties = BTreeMap::new();
    if let Some(style_start) = svg.find("<style>") {
        let style_content_start = style_start + "<style>".len();
        if let Some(style_end_rel) = svg[style_content_start..].find("</style>") {
            let style_content_end = style_content_start + style_end_rel;
            let style_content = &svg[style_content_start..style_content_end];
            custom_properties = extract_svg_custom_properties(style_content);
        }
    }
    if custom_properties.is_empty() && !svg.contains("var(--fm-") {
        return svg.to_string();
    }

    let mut resolved = svg.to_string();
    for _ in 0..8 {
        let next = substitute_svg_var_calls(&resolved, &custom_properties);
        if next == resolved {
            break;
        }
        resolved = next;
    }
    resolved
}

#[cfg(feature = "png")]
fn extract_svg_custom_properties(style_content: &str) -> BTreeMap<String, String> {
    let mut properties = BTreeMap::new();
    for line in style_content.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("--fm-") {
            continue;
        }
        let Some((name, value)) = trimmed.split_once(':') else {
            continue;
        };
        let value = value.trim().trim_end_matches(';').trim();
        if !value.is_empty() {
            properties.insert(name.trim().to_string(), value.to_string());
        }
    }
    properties
}

#[cfg(feature = "png")]
fn substitute_svg_var_calls(input: &str, custom_properties: &BTreeMap<String, String>) -> String {
    let mut output = String::with_capacity(input.len());
    let mut cursor = 0;

    while let Some(rel_start) = input[cursor..].find("var(--fm-") {
        let start = cursor + rel_start;
        output.push_str(&input[cursor..start]);

        let content_start = start + "var(".len();
        let mut depth = 1_usize;
        let mut end = None;
        for (offset, ch) in input[content_start..].char_indices() {
            match ch {
                '(' => depth = depth.saturating_add(1),
                ')' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        end = Some(content_start + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let Some(end) = end else {
            output.push_str(&input[start..]);
            return output;
        };
        let body = &input[content_start..end];
        let (property_name, fallback) = match body.split_once(',') {
            Some((name, fallback)) => (name.trim(), Some(fallback.trim())),
            None => (body.trim(), None),
        };

        if let Some(value) = custom_properties.get(property_name) {
            output.push_str(value);
        } else if let Some(fallback) = fallback.filter(|value| !value.is_empty()) {
            output.push_str(fallback);
        } else {
            output.push_str(&input[start..=end]);
        }

        cursor = end + 1;
    }

    output.push_str(&input[cursor..]);
    output
}

#[cfg(feature = "png")]
fn svg_to_png(svg: &str, width: Option<u32>, height: Option<u32>) -> Result<(Vec<u8>, u32, u32)> {
    use resvg::tiny_skia;
    use usvg::{Options, Transform, Tree};

    let opt = Options::default();
    let tree = Tree::from_str(svg, &opt).context("Failed to parse SVG")?;

    let size = tree.size();
    let size_width = size.width();
    let size_height = size.height();
    if !size_width.is_finite()
        || !size_height.is_finite()
        || size_width <= 0.0
        || size_height <= 0.0
    {
        anyhow::bail!("SVG dimensions must be greater than 0");
    }

    let (px_width, px_height) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let scale = w as f32 / size_width;
            (w, (size_height * scale) as u32)
        }
        (None, Some(h)) => {
            let scale = h as f32 / size_height;
            ((size_width * scale) as u32, h)
        }
        (None, None) => (size_width as u32, size_height as u32),
    };
    if px_width == 0 || px_height == 0 {
        anyhow::bail!("PNG dimensions must be greater than 0");
    }

    let mut pixmap =
        tiny_skia::Pixmap::new(px_width, px_height).context("Failed to create pixmap")?;

    let scale_x = px_width as f32 / size_width;
    let scale_y = px_height as f32 / size_height;

    resvg::render(
        &tree,
        Transform::from_scale(scale_x, scale_y),
        &mut pixmap.as_mut(),
    );

    let bytes = pixmap.encode_png().context("Failed to encode PNG")?;
    Ok((bytes, px_width, px_height))
}

// =============================================================================
// Command: parse
// =============================================================================

#[cfg(all(test, feature = "png"))]
mod png_tests {
    use super::{resolve_svg_custom_properties_for_rasterization, svg_to_png};

    const SIMPLE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="100" height="50"><rect x="0" y="0" width="100" height="50" fill="#f00"/></svg>"##;
    const ZERO_SVG: &str =
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="0" height="10"></svg>"##;

    #[test]
    fn png_dimensions_default_to_svg_size() {
        let (_bytes, w, h) = svg_to_png(SIMPLE_SVG, None, None).expect("svg_to_png should succeed");
        assert_eq!(w, 100);
        assert_eq!(h, 50);
    }

    #[test]
    fn png_dimensions_preserve_aspect_when_only_width_provided() {
        let (_bytes, w, h) =
            svg_to_png(SIMPLE_SVG, Some(200), None).expect("svg_to_png should succeed");
        assert_eq!(w, 200);
        assert_eq!(h, 100);
    }

    #[test]
    fn png_dimensions_reject_zero_sized_outputs() {
        let err = svg_to_png(SIMPLE_SVG, Some(0), Some(10)).expect_err("zero width must fail");
        assert!(err.to_string().contains("greater than 0"));
    }

    #[test]
    fn png_dimensions_reject_zero_sized_svg() {
        let err = svg_to_png(ZERO_SVG, Some(100), None).expect_err("zero SVG size must fail");
        let message = err.to_string();
        assert!(
            message.contains("SVG dimensions must be greater than 0")
                || message.contains("Failed to parse SVG")
        );
    }

    #[test]
    fn png_rasterization_resolves_svg_custom_properties() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><style>:root {
  --fm-node-fill: #123456;
  --fm-node-stroke: #abcdef;
}
.fm-box { fill: var(--fm-node-fill, #ffffff); stroke: var(--fm-node-stroke, #000000); }</style><rect class="fm-box" fill="var(--fm-node-fill, #ffffff)" stroke="var(--fm-node-stroke, #000000)" x="0" y="0" width="40" height="20"/></svg>"##;

        let resolved = resolve_svg_custom_properties_for_rasterization(svg);
        assert!(resolved.contains("#123456"));
        assert!(resolved.contains("#abcdef"));
        assert!(!resolved.contains("var(--fm-node-fill"));
        assert!(!resolved.contains("var(--fm-node-stroke"));
    }

    #[test]
    fn png_rasterization_resolves_chained_svg_custom_properties() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><style>:root {
  --fm-cluster-stroke: #654321;
  --fm-edge-muted: var(--fm-cluster-stroke);
}
</style><path stroke="var(--fm-edge-muted, #000000)" d="M0 0 L40 20"/></svg>"##;

        let resolved = resolve_svg_custom_properties_for_rasterization(svg);
        assert!(resolved.contains("#654321"));
        assert!(!resolved.contains("var(--fm-edge-muted"));
    }

    #[test]
    fn png_rasterization_applies_var_fallback_without_style_block() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect fill="var(--fm-missing, #123456)" x="0" y="0" width="40" height="20"/></svg>"##;

        let resolved = resolve_svg_custom_properties_for_rasterization(svg);
        assert!(resolved.contains("#123456"));
        assert!(!resolved.contains("var(--fm-missing"));
    }

    #[test]
    fn png_rasterization_applies_var_fallback_with_parentheses() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20"><rect fill="var(--fm-missing, rgb(18, 52, 86))" x="0" y="0" width="40" height="20"/></svg>"##;

        let resolved = resolve_svg_custom_properties_for_rasterization(svg);
        assert!(resolved.contains("rgb(18, 52, 86)"));
        assert!(!resolved.contains("var(--fm-missing"));
    }
}

fn cmd_parse(
    input: &str,
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    full: bool,
    pretty: bool,
    max_input_bytes: usize,
) -> Result<()> {
    let source = load_input(input, max_input_bytes)?;
    let mut parsed = parse_with_mode_and_config(&source, parse_mode, &parser_config);
    // The `parse` command surfaces format-complement counts in its evidence summary,
    // so capture it explicitly here (the parse hot path no longer does).
    parsed.format_complement = capture_format_complement(&source);

    let output = if full {
        // Full IR output
        if pretty {
            serde_json::to_string_pretty(&parsed.ir)?
        } else {
            serde_json::to_string(&parsed.ir)?
        }
    } else {
        // Summary output (using existing parse_evidence_json)
        if pretty {
            let value: serde_json::Value = serde_json::from_str(&parse_evidence_json(&parsed))?;
            serde_json::to_string_pretty(&value)?
        } else {
            parse_evidence_json(&parsed)
        }
    };

    println!("{output}");

    for warning in &parsed.warnings {
        warn!("Parse warning: {warning}");
    }

    Ok(())
}

// =============================================================================
// Command: detect
// =============================================================================

fn cmd_detect(
    input: &str,
    json_output: bool,
    max_input_bytes: usize,
    parser_config: ParserConfig,
) -> Result<()> {
    let source = load_input(input, max_input_bytes)?;
    let detection = detect_type_with_confidence_and_config(&source, &parser_config);
    let diagram_type = detection.diagram_type;

    let first_line = first_significant_line(&source).unwrap_or("").trim();

    let confidence = confidence_label(detection.confidence);
    let detection_method = detection.method.as_str();
    let support_level = diagram_type.support_label();

    if json_output {
        let result = DetectResult {
            diagram_type: diagram_type.as_str().to_string(),
            confidence: confidence.to_string(),
            support_level: support_level.to_string(),
            first_line: first_line.chars().take(100).collect(),
            detection_method: detection_method.to_string(),
        };

        let output = serde_json::to_string_pretty(&result)?;
        println!("{output}");
    } else {
        println!("Diagram type: {}", diagram_type.as_str());
        println!("Confidence:   {confidence}");
        println!("Support:      {support_level}");
        println!("Method:       {detection_method}");
        if !first_line.is_empty() {
            println!(
                "First line:   {}",
                first_line.chars().take(60).collect::<String>()
            );
        }
    }

    Ok(())
}

fn confidence_label(confidence: f32) -> &'static str {
    if confidence >= 0.9 {
        "high"
    } else if confidence >= 0.6 {
        "medium"
    } else {
        "low"
    }
}

fn cmd_diff(old_input: &str, new_input: &str, options: DiffCommandOptions<'_>) -> Result<()> {
    let DiffCommandOptions {
        parse_mode,
        parser_config,
        format,
        color,
        max_input_bytes,
        dimensions,
        output,
    } = options;
    let (width, height) = dimensions;

    let old_source = load_input(old_input, max_input_bytes)?;
    let new_source = load_input(new_input, max_input_bytes)?;

    let old_parsed = parse_with_mode_and_config(&old_source, parse_mode, &parser_config);
    let new_parsed = parse_with_mode_and_config(&new_source, parse_mode, &parser_config);

    for warning in &old_parsed.warnings {
        warn!("Old parse warning: {warning}");
    }
    for warning in &new_parsed.warnings {
        warn!("New parse warning: {warning}");
    }

    let diff = diff_diagrams(&old_parsed.ir, &new_parsed.ir);
    let use_colors = diff_use_colors(color, output.is_none());

    let rendered = match format {
        DiffOutputFormat::Summary => render_diff_summary(&diff, use_colors),
        DiffOutputFormat::Plain => render_diff_plain(&diff),
        DiffOutputFormat::Terminal => {
            let (cols, rows) = terminal_size(width, height);
            render_diff_terminal_with_config(
                &old_parsed.ir,
                &new_parsed.ir,
                &TermRenderConfig::rich(),
                cols,
                rows,
                use_colors,
            )
        }
        DiffOutputFormat::Json => serde_json::to_string_pretty(&diff)?,
    };

    write_output(output, &rendered)
}

fn diff_use_colors(color: ColorChoice, writing_to_stdout: bool) -> bool {
    match color {
        ColorChoice::Always => true,
        ColorChoice::Never => false,
        ColorChoice::Auto => writing_to_stdout && io::stdout().is_terminal(),
    }
}

// =============================================================================
// Command: validate
// =============================================================================

fn cmd_validate(input: &str, options: ValidateCommandOptions<'_>) -> Result<()> {
    let ValidateCommandOptions {
        parse_mode,
        parser_config,
        layout_algorithm,
        layout_config,
        format,
        fail_on,
        diagnostics_out,
        max_input_bytes,
        svg_base_config,
        show_back_edges,
        fnx_mode,
        fnx_projection,
        fnx_fallback,
    } = options;
    if matches!(fnx_mode, FnxModeArg::Enabled)
        && !cfg!(all(
            feature = "fnx-integration",
            not(target_arch = "wasm32")
        ))
    {
        anyhow::bail!("--fnx-mode enabled requires fnx-integration feature");
    }
    if fnx_mode.should_use_fnx() {
        if matches!(
            fnx_projection,
            FnxProjectionArg::Directed | FnxProjectionArg::Auto
        ) {
            anyhow::bail!(
                "--fnx-projection {} is not yet supported (use undirected)",
                fnx_projection.as_str()
            );
        }
        if matches!(fnx_fallback, FnxFallbackArg::Strict | FnxFallbackArg::Warn) {
            anyhow::bail!(
                "--fnx-fallback {} is not yet supported (use graceful)",
                fnx_fallback.as_str()
            );
        }
    }

    let source = load_input(input, max_input_bytes)?;
    let total_start = Instant::now();
    let pressure = MermaidNativePressureSignals::sample().into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);

    let parse_start = Instant::now();
    let parsed = parse_with_mode_and_config(&source, parse_mode, &parser_config);
    let parse_time = parse_start.elapsed();
    budget_broker.record_parse(u64::try_from(parse_time.as_millis()).unwrap_or(u64::MAX));

    let fnx_enabled = fnx_mode.should_use_fnx();
    let mut layout_config = layout_config;
    layout_config.fnx_enabled = fnx_enabled;
    let layout_start = Instant::now();
    let layout_guardrails = LayoutGuardrails {
        max_layout_time_ms: budget_broker.layout_time_budget_ms(),
        max_layout_iterations: budget_broker
            .layout_iteration_budget(LayoutGuardrails::default().max_layout_iterations),
        max_route_ops: budget_broker.route_budget(LayoutGuardrails::default().max_route_ops),
    };
    let traced_layout = layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        layout_algorithm,
        layout_config,
        layout_guardrails,
    );
    let layout_time = layout_start.elapsed();
    budget_broker.record_layout(layout_time.as_millis().min(u128::from(u64::MAX)) as u64);
    let mut guard_report =
        build_layout_guard_report_with_pressure(&parsed.ir, &traced_layout, pressure);
    let (_cx, observability) = mermaid_layout_guard_observability(
        "cli.validate",
        &source,
        traced_layout.trace.dispatch.selected.as_str(),
        traced_layout.trace.guard.estimated_layout_time_ms.max(1) as u64,
    );
    guard_report.observability = observability;
    let filtered_layout =
        (!show_back_edges).then(|| layout_without_back_edges(&traced_layout.layout));
    let layout = filtered_layout.as_ref().unwrap_or(&traced_layout.layout);
    let mut svg_config = svg_base_config;
    svg_config.include_source_spans = true;
    svg_config.apply_degradation(&guard_report.degradation);
    let render_start = Instant::now();
    let svg_output = render_svg_with_layout(&parsed.ir, layout, &svg_config);
    let render_time = render_start.elapsed();
    budget_broker.record_render(render_time.as_millis().min(u128::from(u64::MAX)) as u64);
    guard_report.budget_broker = budget_broker.clone();
    let layout_decision_ledger =
        build_layout_decision_ledger(&parsed.ir, &traced_layout, &guard_report);
    let layout_decision_explanation = layout_decision_ledger
        .primary_explanation()
        .expect("layout decision ledger should contain a primary entry");
    let layout_decision_ledger_jsonl = layout_decision_ledger.to_jsonl()?;

    let mut diagnostics = collect_parse_diagnostics(&parsed);
    diagnostics.extend(collect_structural_diagnostics(&parsed));
    diagnostics.extend(collect_layout_diagnostics(&traced_layout));
    diagnostics.extend(collect_render_diagnostics(&svg_output));

    // FNX structural analysis (when feature is enabled)
    #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
    let (fnx_results, fnx_analysis_time) = if fnx_enabled {
        let analysis_start = Instant::now();
        let results = analyze_structure(&parsed.ir);
        let analysis_time = analysis_start.elapsed();
        diagnostics.extend(collect_fnx_diagnostics(&results));
        (Some(results), analysis_time)
    } else {
        (None, std::time::Duration::ZERO)
    };
    #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
    let (_fnx_results, _fnx_analysis_time): (Option<()>, std::time::Duration) =
        (None, std::time::Duration::ZERO);

    sort_diagnostics(&mut diagnostics);

    let valid = !should_fail_validation(&diagnostics, fail_on);

    let result = ValidateResult {
        valid,
        parse_mode: parse_mode.as_str().to_string(),
        accessibility_summary: describe_diagram_with_layout(&parsed.ir, Some(layout)),
        layout_requested: traced_layout.trace.dispatch.requested.as_str().to_string(),
        layout_selected: traced_layout.trace.dispatch.selected.as_str().to_string(),
        layout_guard_reason: traced_layout.trace.guard.reason.to_string(),
        layout_guard_fallback_applied: traced_layout.trace.guard.fallback_applied,
        layout_guard_time_budget_exceeded: traced_layout.trace.guard.time_budget_exceeded,
        layout_guard_iteration_budget_exceeded: traced_layout.trace.guard.iteration_budget_exceeded,
        layout_guard_route_budget_exceeded: traced_layout.trace.guard.route_budget_exceeded,
        layout_guard_estimated_time_ms: traced_layout.trace.guard.estimated_layout_time_ms,
        layout_guard_estimated_iterations: traced_layout.trace.guard.estimated_layout_iterations,
        layout_guard_estimated_route_ops: traced_layout.trace.guard.estimated_route_ops,
        layout_band_count: traced_layout.layout.extensions.bands.len(),
        layout_tick_count: traced_layout.layout.extensions.axis_ticks.len(),
        source_span_node_count: count_known_node_spans(layout),
        source_span_edge_count: count_known_edge_spans(layout),
        source_span_cluster_count: count_known_cluster_spans(layout),
        diagram_type: parsed.ir.diagram_type.as_str().to_string(),
        node_count: parsed.ir.nodes.len(),
        edge_count: parsed.ir.edges.len(),
        pressure_source: guard_report.pressure.source.as_str().to_string(),
        pressure_tier: guard_report.pressure.tier.as_str().to_string(),
        pressure_telemetry_available: guard_report.pressure.telemetry_available,
        pressure_conservative_fallback: guard_report.pressure.conservative_fallback,
        pressure_score_permille: guard_report.pressure.quantized_score_permille,
        trace_id: guard_report.observability.trace_id.to_string(),
        decision_id: guard_report.observability.decision_id.to_string(),
        policy_id: guard_report.observability.policy_id.to_string(),
        schema_version: guard_report.observability.schema_version.to_string(),
        layout_decision_ledger,
        layout_decision_explanation,
        layout_decision_ledger_jsonl,
        budget_total_ms: budget_broker.total_budget_ms,
        parse_budget_ms: budget_broker.parse.allocated_ms,
        layout_budget_ms: budget_broker.layout.allocated_ms,
        render_budget_ms: budget_broker.render.allocated_ms,
        budget_exhausted: budget_broker.exhausted,
        parse_used_ms: budget_broker.parse.used_ms,
        layout_used_ms: budget_broker.layout.used_ms,
        render_used_ms: budget_broker.render.used_ms,
        degradation_target_fidelity: format!("{:?}", guard_report.degradation.target_fidelity),
        degradation_reduce_decoration: guard_report.degradation.reduce_decoration,
        degradation_simplify_routing: guard_report.degradation.simplify_routing,
        degradation_hide_labels: guard_report.degradation.hide_labels,
        degradation_collapse_clusters: guard_report.degradation.collapse_clusters,
        degradation_force_glyph_mode: guard_report
            .degradation
            .force_glyph_mode
            .map(|m| format!("{m:?}")),
        diagnostics,
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_enabled: Some(fnx_enabled),
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_component_count: fnx_results.as_ref().map(|r| r.component_count),
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_is_connected: fnx_results.as_ref().map(|r| r.is_connected),
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_articulation_point_count: fnx_results.as_ref().map(|r| r.articulation_point_count),
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_bridge_count: fnx_results.as_ref().map(|r| r.bridge_count),
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_cycle_count: fnx_results.as_ref().map(|r| r.cycle_count),
        #[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
        fnx_witness: build_fnx_validation_witness(
            fnx_enabled,
            fnx_projection,
            fnx_fallback,
            fnx_results.as_ref(),
            fnx_analysis_time,
        ),
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_enabled: None,
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_component_count: None,
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_is_connected: None,
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_articulation_point_count: None,
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_bridge_count: None,
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_cycle_count: None,
        #[cfg(not(all(feature = "fnx-integration", not(target_arch = "wasm32"))))]
        fnx_witness: None,
    };
    let _total_time = total_start.elapsed();

    if let Some(path) = diagnostics_out {
        let artifact = serde_json::to_string_pretty(&result)?;
        std::fs::write(path, artifact)
            .context(format!("Failed to write diagnostics file: {path}"))?;
        info!("Wrote diagnostics artifact to: {path}");
    }

    match format {
        ValidateOutputFormat::Text => print_validate_text(&result, fail_on),
        ValidateOutputFormat::Json => println!("{}", serde_json::to_string(&result)?),
        ValidateOutputFormat::Pretty => println!("{}", serde_json::to_string_pretty(&result)?),
    }

    if !result.valid {
        std::process::exit(1);
    }

    Ok(())
}

fn collect_parse_diagnostics(parsed: &fm_parser::ParseResult) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();
    const STYLE_DIRECTIVE_RULE_ID: &str = "classdef-not-applied";
    const STYLE_DIRECTIVE_MESSAGE: &str = "style directives (classDef/style/linkStyle) are parsed, but only SVG output currently \
applies them; terminal/canvas renderers use theme defaults";

    for warning in &parsed.ir.meta.init.warnings {
        let payload = StructuredDiagnostic::from_warning(warning)
            .with_rule_id("parse.init.warning")
            .with_confidence(parsed.confidence);
        diagnostics.push(ValidationDiagnostic {
            stage: "parse".to_string(),
            payload,
        });
    }

    for error in &parsed.ir.meta.init.errors {
        let payload = StructuredDiagnostic::from_error(error)
            .with_rule_id("parse.init.error")
            .with_confidence(parsed.confidence);
        diagnostics.push(ValidationDiagnostic {
            stage: "parse".to_string(),
            payload,
        });
    }

    for diagnostic in &parsed.ir.diagnostics {
        let mut payload = StructuredDiagnostic::from_diagnostic(diagnostic);
        if diagnostic.message == STYLE_DIRECTIVE_MESSAGE {
            payload = payload.with_rule_id(STYLE_DIRECTIVE_RULE_ID);
        } else if let Some(rule_id) = diagnostic.rule_id.as_deref() {
            payload = payload.with_rule_id(rule_id);
        } else {
            payload = payload.with_rule_id(format!("parse.{}", diagnostic.category.as_str()));
        }
        let payload = payload.with_confidence(parsed.confidence);
        diagnostics.push(ValidationDiagnostic {
            stage: "parse".to_string(),
            payload,
        });
    }

    for warning_message in &parsed.warnings {
        let payload = StructuredDiagnostic {
            error_code: "mermaid/warn/unstructured-parse-warning".to_string(),
            severity: "warning".to_string(),
            message: warning_message.clone(),
            span: None,
            source_line: None,
            source_column: None,
            rule_id: Some("parse.unstructured.warning".to_string()),
            confidence: Some(parsed.confidence),
            remediation_hint: parse_warning_remediation_hint(warning_message),
        };
        diagnostics.push(ValidationDiagnostic {
            stage: "parse".to_string(),
            payload,
        });
    }

    diagnostics
}

fn count_known_node_spans(layout: &fm_layout::DiagramLayout) -> usize {
    layout
        .nodes
        .iter()
        .filter(|node| !node.span.is_unknown())
        .count()
}

fn count_known_edge_spans(layout: &fm_layout::DiagramLayout) -> usize {
    layout
        .edges
        .iter()
        .filter(|edge| !edge.span.is_unknown())
        .count()
}

fn count_known_cluster_spans(layout: &fm_layout::DiagramLayout) -> usize {
    layout
        .clusters
        .iter()
        .filter(|cluster| !cluster.span.is_unknown())
        .count()
}

fn collect_structural_diagnostics(parsed: &fm_parser::ParseResult) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();

    if parsed.ir.diagram_type == DiagramType::Unknown {
        diagnostics.push(ValidationDiagnostic {
            stage: "parse".to_string(),
            payload: StructuredDiagnostic {
                error_code: "mermaid/error/unknown-diagram-type".to_string(),
                severity: "error".to_string(),
                message: "Could not detect diagram type".to_string(),
                span: None,
                source_line: Some(1),
                source_column: Some(1),
                rule_id: Some("parse.detect.unknown_type".to_string()),
                confidence: Some(parsed.confidence),
                remediation_hint: Some(
                    "Start the diagram with an explicit header such as 'flowchart LR'".to_string(),
                ),
            },
        });
    }

    if parsed.ir.nodes.is_empty() && parsed.ir.edges.is_empty() {
        diagnostics.push(ValidationDiagnostic {
            stage: "parse".to_string(),
            payload: StructuredDiagnostic {
                error_code: "mermaid/error/empty-diagram".to_string(),
                severity: "error".to_string(),
                message: "Diagram has no parseable nodes or edges".to_string(),
                span: None,
                source_line: None,
                source_column: None,
                rule_id: Some("parse.structure.empty_diagram".to_string()),
                confidence: Some(parsed.confidence),
                remediation_hint: Some("Add at least one node and one edge".to_string()),
            },
        });
    }

    diagnostics
}

#[cfg(all(feature = "fnx-integration", not(target_arch = "wasm32")))]
fn collect_fnx_diagnostics(results: &FnxAnalysisResults) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();

    for diag in &results.diagnostics {
        let severity = match diag.severity {
            FnxDiagnosticSeverity::Info => "info",
            FnxDiagnosticSeverity::Warning => "warning",
            FnxDiagnosticSeverity::Error => "error",
        };

        let (source_line, source_column) = diag
            .span
            .map(|span| {
                (
                    Some(span.start.line as usize),
                    Some(span.start.col as usize),
                )
            })
            .unwrap_or((None, None));

        diagnostics.push(ValidationDiagnostic {
            stage: "fnx".to_string(),
            payload: StructuredDiagnostic {
                error_code: format!("mermaid/{severity}/{}", diag.code.as_str().to_lowercase()),
                severity: severity.to_string(),
                message: diag.message.clone(),
                span: diag.span,
                source_line,
                source_column,
                rule_id: Some(format!("fnx.{}", diag.code.category.to_lowercase())),
                confidence: None,
                remediation_hint: diag.suggestion.clone(),
            },
        });
    }

    diagnostics
}

fn collect_layout_diagnostics(traced: &TracedLayout) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();
    let layout = &traced.layout;
    let dispatch = traced.trace.dispatch;

    let severity = if dispatch.capability_unavailable {
        "warning"
    } else {
        "info"
    };
    let remediation_hint = if dispatch.capability_unavailable {
        Some(format!(
            "Requested '{}' is unavailable for this diagram family; using '{}'",
            dispatch.requested.as_str(),
            dispatch.selected.as_str()
        ))
    } else {
        None
    };
    diagnostics.push(ValidationDiagnostic {
        stage: "layout".to_string(),
        payload: StructuredDiagnostic {
            error_code: "mermaid/info/layout-dispatch".to_string(),
            severity: severity.to_string(),
            message: format!(
                "Layout dispatch requested '{}' and selected '{}' ({})",
                dispatch.requested.as_str(),
                dispatch.selected.as_str(),
                dispatch.reason
            ),
            span: None,
            source_line: None,
            source_column: None,
            rule_id: Some("layout.dispatch.selection".to_string()),
            confidence: None,
            remediation_hint,
        },
    });

    if layout.bounds.width <= 0.0 || layout.bounds.height <= 0.0 {
        diagnostics.push(ValidationDiagnostic {
            stage: "layout".to_string(),
            payload: StructuredDiagnostic {
                error_code: "mermaid/error/layout-empty-bounds".to_string(),
                severity: "error".to_string(),
                message: "Layout produced empty bounds".to_string(),
                span: None,
                source_line: None,
                source_column: None,
                rule_id: Some("layout.bounds.empty".to_string()),
                confidence: None,
                remediation_hint: Some(
                    "Verify parser output contains connected nodes and valid labels".to_string(),
                ),
            },
        });
    }

    if layout.stats.reversed_edges > 0 {
        diagnostics.push(ValidationDiagnostic {
            stage: "layout".to_string(),
            payload: StructuredDiagnostic {
                error_code: "mermaid/warn/layout-cycle-reversal".to_string(),
                severity: "warning".to_string(),
                message: format!(
                    "Layout reversed {} edge(s) to break cycle(s)",
                    layout.stats.reversed_edges
                ),
                span: None,
                source_line: None,
                source_column: None,
                rule_id: Some("layout.cycle.reversal".to_string()),
                confidence: None,
                remediation_hint: Some(
                    "Consider tuning cycle strategy when preserving edge direction is important"
                        .to_string(),
                ),
            },
        });
    }

    diagnostics
}

fn collect_render_diagnostics(svg_output: &str) -> Vec<ValidationDiagnostic> {
    let mut diagnostics = Vec::new();

    if !svg_output.starts_with("<svg") || !svg_output.contains("</svg>") {
        diagnostics.push(ValidationDiagnostic {
            stage: "render".to_string(),
            payload: StructuredDiagnostic {
                error_code: "mermaid/error/render-svg-invalid".to_string(),
                severity: "error".to_string(),
                message: "Renderer produced invalid SVG envelope".to_string(),
                span: None,
                source_line: None,
                source_column: None,
                rule_id: Some("render.svg.envelope".to_string()),
                confidence: None,
                remediation_hint: Some(
                    "Re-run with --verbose and inspect renderer output".to_string(),
                ),
            },
        });
    }

    diagnostics
}

fn parse_warning_remediation_hint(message: &str) -> Option<String> {
    let lower = message.to_ascii_lowercase();

    if lower.contains("empty") {
        Some("Add nodes and edges to your diagram".to_string())
    } else if lower.contains("unknown") && lower.contains("diagram") {
        Some("Start your diagram with a type declaration like 'flowchart LR'".to_string())
    } else {
        None
    }
}

fn sort_diagnostics(diagnostics: &mut [ValidationDiagnostic]) {
    diagnostics.sort_by(|left, right| {
        right
            .payload
            .severity_rank()
            .cmp(&left.payload.severity_rank())
            .then_with(|| {
                left.payload
                    .source_line
                    .unwrap_or(usize::MAX)
                    .cmp(&right.payload.source_line.unwrap_or(usize::MAX))
            })
            .then_with(|| {
                left.payload
                    .source_column
                    .unwrap_or(usize::MAX)
                    .cmp(&right.payload.source_column.unwrap_or(usize::MAX))
            })
            .then_with(|| left.stage.cmp(&right.stage))
            .then_with(|| left.payload.error_code.cmp(&right.payload.error_code))
            .then_with(|| left.payload.message.cmp(&right.payload.message))
    });
}

fn should_fail_validation(diagnostics: &[ValidationDiagnostic], threshold: FailOnSeverity) -> bool {
    if threshold == FailOnSeverity::None {
        return false;
    }

    let threshold_rank = threshold.rank();
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.payload.severity_rank() >= threshold_rank)
}

fn print_validate_text(result: &ValidateResult, fail_on: FailOnSeverity) {
    if result.valid {
        println!("✓ Valid {} diagram", result.diagram_type);
    } else {
        println!("✗ Invalid {} diagram", result.diagram_type);
    }

    println!("  Nodes: {}", result.node_count);
    println!("  Edges: {}", result.edge_count);
    println!(
        "  Pressure: {} ({})",
        result.pressure_tier, result.pressure_score_permille
    );
    println!(
        "  Budget: {}ms total, exhausted={}",
        result.budget_total_ms, result.budget_exhausted
    );
    if result.degradation_reduce_decoration
        || result.degradation_simplify_routing
        || result.degradation_hide_labels
        || result.degradation_collapse_clusters
    {
        println!(
            "  Degradation: {} (decoration={}, routing={}, labels={}, clusters={})",
            result.degradation_target_fidelity,
            result.degradation_reduce_decoration,
            result.degradation_simplify_routing,
            result.degradation_hide_labels,
            result.degradation_collapse_clusters,
        );
    }
    println!("  Diagnostics: {}", result.diagnostics.len());
    println!("  Fail threshold: {fail_on:?}");

    if result.diagnostics.is_empty() {
        return;
    }

    println!("\nDiagnostics:");
    for diagnostic in &result.diagnostics {
        let location = match (
            diagnostic.payload.source_line,
            diagnostic.payload.source_column,
        ) {
            (Some(line), Some(column)) => format!(" (line {line}, col {column})"),
            (Some(line), None) => format!(" (line {line})"),
            _ => String::new(),
        };
        println!(
            "  [{}][{}][{}] {}{}",
            diagnostic.stage,
            diagnostic.payload.severity,
            diagnostic.payload.error_code,
            diagnostic.payload.message,
            location
        );
        if let Some(rule_id) = &diagnostic.payload.rule_id {
            println!("       rule_id: {rule_id}");
        }
        if let Some(hint) = &diagnostic.payload.remediation_hint {
            println!("       remediation: {hint}");
        }
    }
}

#[cfg(test)]
mod load_input_tests {
    use super::load_input;
    use std::io::Write;

    const MAX: usize = 1 << 20;

    fn write_temp(name: &str, body: &str) -> (tempfile::TempDir, String) {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join(name);
        let mut file = std::fs::File::create(&path).expect("create");
        file.write_all(body.as_bytes()).expect("write");
        let display = path.to_string_lossy().into_owned();
        (dir, display)
    }

    #[test]
    fn existing_file_is_read_byte_exactly() {
        let body = "flowchart LR\n  A[\"x\"]-->B\n";
        let (_dir, path) = write_temp("d.mmd", body);
        assert_eq!(load_input(&path, MAX).expect("read"), body);
    }

    /// The pre-sized read must not truncate at, or overrun, the `fstat` length. Sizes chosen
    /// around the capacity boundary: empty, one byte, and a body far larger than any probe buffer.
    #[test]
    fn read_is_exact_across_size_boundaries() {
        for len in [0usize, 1, 2, 8191, 8192, 8193, 65536] {
            let body = "a".repeat(len);
            let (_dir, path) = write_temp("sized.mmd", &body);
            let got = load_input(&path, MAX).expect("read");
            assert_eq!(got.len(), len, "length mismatch at {len}");
            assert_eq!(got, body, "content mismatch at {len}");
        }
    }

    /// A path-shaped argument that does not exist is inline diagram text, not an error. This is
    /// the `Path::exists() == false` branch the single-walk open must keep reproducing.
    #[test]
    fn missing_path_falls_back_to_inline_text() {
        let missing = "/nonexistent-dir-fm/nope.mmd";
        assert_eq!(load_input(missing, MAX).expect("inline"), missing);
    }

    #[test]
    fn inline_mermaid_source_is_returned_verbatim() {
        let src = "flowchart TD\n  A-->B\n";
        assert_eq!(load_input(src, MAX).expect("inline"), src);
    }

    #[test]
    fn oversize_file_is_rejected_by_the_stat_gate() {
        let (_dir, path) = write_temp("big.mmd", &"x".repeat(4096));
        let error = load_input(&path, 16).expect_err("must reject");
        let text = format!("{error:#}");
        assert!(text.contains("4096"), "expected stat size in: {text}");
        assert!(
            text.contains("core.max_input_bytes=16"),
            "expected budget in: {text}"
        );
    }

    #[test]
    fn oversize_inline_input_is_rejected() {
        let error = load_input(&"flowchart TD\n".repeat(64), 16).expect_err("must reject");
        assert!(format!("{error:#}").contains("Inline input is"));
    }
}

#[cfg(test)]
mod validate_tests {
    use super::{
        FailOnSeverity, StructuredDiagnostic, ValidationDiagnostic, collect_parse_diagnostics,
        parse_warning_remediation_hint, should_fail_validation, sort_diagnostics,
    };
    use fm_parser::parse;

    fn diagnostic(
        stage: &str,
        severity: &str,
        source_line: Option<usize>,
        error_code: &str,
    ) -> ValidationDiagnostic {
        ValidationDiagnostic {
            stage: stage.to_string(),
            payload: StructuredDiagnostic {
                error_code: error_code.to_string(),
                severity: severity.to_string(),
                message: format!("{stage}:{severity}:{error_code}"),
                span: None,
                source_line,
                source_column: None,
                rule_id: None,
                confidence: None,
                remediation_hint: None,
            },
        }
    }

    #[test]
    fn diagnostics_are_sorted_by_severity_then_location_then_code() {
        let mut diagnostics = vec![
            diagnostic("render", "warning", Some(2), "b"),
            diagnostic("parse", "error", Some(5), "z"),
            diagnostic("parse", "warning", Some(1), "a"),
            diagnostic("layout", "info", Some(1), "a"),
            diagnostic("parse", "error", Some(1), "a"),
        ];

        sort_diagnostics(&mut diagnostics);
        let ordered: Vec<(String, String, Option<usize>, String)> = diagnostics
            .iter()
            .map(|diag| {
                (
                    diag.stage.clone(),
                    diag.payload.severity.clone(),
                    diag.payload.source_line,
                    diag.payload.error_code.clone(),
                )
            })
            .collect();

        assert_eq!(
            ordered,
            vec![
                (
                    "parse".to_string(),
                    "error".to_string(),
                    Some(1),
                    "a".to_string()
                ),
                (
                    "parse".to_string(),
                    "error".to_string(),
                    Some(5),
                    "z".to_string()
                ),
                (
                    "parse".to_string(),
                    "warning".to_string(),
                    Some(1),
                    "a".to_string()
                ),
                (
                    "render".to_string(),
                    "warning".to_string(),
                    Some(2),
                    "b".to_string()
                ),
                (
                    "layout".to_string(),
                    "info".to_string(),
                    Some(1),
                    "a".to_string()
                ),
            ]
        );
    }

    #[test]
    fn fail_threshold_respects_selected_severity() {
        let diagnostics = vec![
            diagnostic("parse", "info", Some(1), "i"),
            diagnostic("layout", "warning", Some(2), "w"),
        ];

        assert!(should_fail_validation(
            &diagnostics,
            FailOnSeverity::Warning
        ));
        assert!(!should_fail_validation(&diagnostics, FailOnSeverity::Error));
        assert!(should_fail_validation(&diagnostics, FailOnSeverity::Info));
        assert!(!should_fail_validation(&diagnostics, FailOnSeverity::None));
    }

    #[test]
    fn warning_hint_detects_unknown_diagram_message() {
        let hint = parse_warning_remediation_hint("Unknown diagram type header");
        assert!(hint.is_some_and(|value| value.contains("flowchart LR")));
    }

    #[test]
    fn collect_validation_diagnostics_includes_parse_warnings() {
        let parsed = parse("");
        let diagnostics = collect_parse_diagnostics(&parsed);

        assert!(!diagnostics.is_empty());
        assert!(
            diagnostics
                .iter()
                .any(|diag| diag.payload.severity == "warning")
        );
    }
}

#[cfg(test)]
mod render_tests {
    use super::{
        ColorChoice, FnxFallbackArg, FnxModeArg, FnxProjectionArg, OutputFormat,
        RenderCommandOptions, RenderSurfaceOptions, SvgRenderConfig, TermRenderConfig, ThemePreset,
        build_svg_render_config, diff_use_colors, extract_svg_dimensions,
        layout_without_back_edges, normalize_positive_font_size, parse_positive_dimension_arg,
        parse_positive_font_size_arg, render_format, render_source, terminal_size,
    };
    use fm_core::{MermaidParseMode, MermaidSourceMap, MermaidSourceMapKind};
    use fm_layout::{
        DiagramLayout, LayoutAlgorithm, LayoutConfig, LayoutEdgePath, LayoutExtensions,
        LayoutPoint, LayoutRect, LayoutStats, layout_diagram,
    };
    use fm_parser::{ParserConfig, parse};
    use tempfile::NamedTempFile;

    #[test]
    fn term_render_uses_precomputed_layout() {
        let parsed = parse("flowchart LR\nA[Start]-->B[End]");
        let layout = layout_diagram(&parsed.ir);
        let mut empty_layout = layout;
        empty_layout.nodes.clear();
        empty_layout.edges.clear();
        empty_layout.clusters.clear();
        empty_layout.cycle_clusters.clear();

        let (rendered, _, _) = render_format(
            &parsed.ir,
            &empty_layout,
            OutputFormat::Term,
            RenderSurfaceOptions {
                theme: "default",
                font_size: None,
                svg_base_config: SvgRenderConfig::default(),
                term_base_config: TermRenderConfig::rich(),
                show_back_edges: true,
                show_minimap: false,
                embed_source_spans: false,
                dimensions: (Some(80), Some(24)),
                degradation: fm_core::MermaidDegradationPlan::default(),
            },
        )
        .expect("terminal render should succeed");

        let output = String::from_utf8(rendered).expect("terminal output should be UTF-8");
        assert!(!output.contains("Start"));
        assert!(!output.contains("End"));
    }

    #[test]
    fn svg_render_config_applies_font_size_for_all_svg_based_outputs() {
        let config = build_svg_render_config(&SvgRenderConfig::default(), "dark", Some(22.0), true);
        assert_eq!(config.theme, ThemePreset::Dark);
        assert_eq!(config.font_size, 22.0);
        assert!(config.include_source_spans);
    }

    #[test]
    fn svg_render_config_ignores_invalid_font_sizes() {
        let default_font_size =
            build_svg_render_config(&SvgRenderConfig::default(), "default", None, true).font_size;
        assert_eq!(
            build_svg_render_config(&SvgRenderConfig::default(), "default", Some(0.0), true)
                .font_size,
            default_font_size
        );
        assert_eq!(
            build_svg_render_config(&SvgRenderConfig::default(), "default", Some(-5.0), true)
                .font_size,
            default_font_size
        );
        assert_eq!(
            build_svg_render_config(&SvgRenderConfig::default(), "default", Some(f32::NAN), true)
                .font_size,
            default_font_size
        );
    }

    #[test]
    fn layout_without_back_edges_filters_reversed_edges() {
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: vec![
                LayoutEdgePath {
                    edge_index: 0,
                    span: Default::default(),
                    points: [
                        LayoutPoint { x: 0.0, y: 0.0 },
                        LayoutPoint { x: 1.0, y: 0.0 },
                    ]
                    .into_iter()
                    .collect(),
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
                LayoutEdgePath {
                    edge_index: 1,
                    span: Default::default(),
                    points: [
                        LayoutPoint { x: 1.0, y: 0.0 },
                        LayoutPoint { x: 0.0, y: 0.0 },
                    ]
                    .into_iter()
                    .collect(),
                    reversed: true,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
            ],
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        };

        let filtered = layout_without_back_edges(&layout);
        assert_eq!(filtered.edges.len(), 1);
        assert_eq!(filtered.edges[0].edge_index, 0);
        assert_eq!(layout.edges.len(), 2);
    }

    #[test]
    fn source_map_omits_hidden_back_edges() {
        let source = "flowchart LR\nA-->B\nB-->A";
        let parsed = parse(source);
        let layout = layout_diagram(&parsed.ir);
        assert!(
            layout.edges.iter().any(|edge| edge.reversed),
            "expected a reversed edge in cycle layout"
        );
        let expected_edge_count = layout_without_back_edges(&layout).edges.len();

        let source_map_path = NamedTempFile::new()
            .expect("source map temp file")
            .into_temp_path();
        let source_map_path_str = source_map_path
            .to_str()
            .expect("source map path utf-8")
            .to_string();

        let options = RenderCommandOptions {
            parse_mode: MermaidParseMode::Compat,
            parser_config: ParserConfig::default(),
            layout_algorithm: LayoutAlgorithm::Auto,
            layout_config: LayoutConfig::default(),
            format: OutputFormat::Svg,
            theme: "default",
            font_size: None,
            output: None,
            max_input_bytes: 5_000_000,
            svg_base_config: SvgRenderConfig::default(),
            term_base_config: TermRenderConfig::rich(),
            show_back_edges: false,
            show_minimap: false,
            embed_source_spans: true,
            source_map_out: Some(source_map_path_str.as_str()),
            dimensions: (None, None),
            json_output: true,
            fnx_mode: FnxModeArg::Auto,
            fnx_projection: FnxProjectionArg::Undirected,
            fnx_fallback: FnxFallbackArg::Graceful,
        };

        let outcome = render_source(source, &options).expect("render source");
        let svg = String::from_utf8(outcome.rendered).expect("svg utf-8");
        let rendered_edge_count = svg.matches("id=\"fm-edge-").count();

        let source_map_raw = std::fs::read_to_string(&source_map_path).expect("read source map");
        let source_map: MermaidSourceMap =
            serde_json::from_str(&source_map_raw).expect("parse source map");
        let mapped_edge_count = source_map
            .entries
            .iter()
            .filter(|entry| entry.kind == MermaidSourceMapKind::Edge)
            .count();

        assert_eq!(
            rendered_edge_count, expected_edge_count,
            "rendered SVG should omit reversed edges"
        );
        assert_eq!(
            mapped_edge_count, rendered_edge_count,
            "source map edge count should match rendered SVG"
        );
    }

    #[test]
    fn parse_positive_font_size_arg_rejects_invalid_values() {
        assert_eq!(parse_positive_font_size_arg("18").ok(), Some(18.0));
        assert!(parse_positive_font_size_arg("0").is_err());
        assert!(parse_positive_font_size_arg("-2").is_err());
        assert!(parse_positive_font_size_arg("NaN").is_err());
    }

    #[test]
    fn normalize_positive_font_size_filters_invalid_values() {
        assert_eq!(normalize_positive_font_size(Some(16.0)), Some(16.0));
        assert_eq!(normalize_positive_font_size(Some(0.0)), None);
        assert_eq!(normalize_positive_font_size(Some(-1.0)), None);
        assert_eq!(normalize_positive_font_size(Some(f32::INFINITY)), None);
    }

    #[test]
    fn parse_positive_dimension_arg_rejects_zero() {
        assert_eq!(parse_positive_dimension_arg("42").ok(), Some(42));
        assert!(parse_positive_dimension_arg("0").is_err());
    }

    #[test]
    fn terminal_size_falls_back_for_zero_dimensions() {
        assert_eq!(terminal_size(Some(0), Some(0)), (80, 24));
        assert_eq!(terminal_size(Some(120), Some(0)), (120, 24));
    }

    #[test]
    fn extract_svg_dimensions_falls_back_to_viewbox_for_responsive_svg() {
        let svg = r#"<svg viewBox="0 0 320.5 180.2" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert_eq!(extract_svg_dimensions(svg), (Some(321), Some(181)));
    }

    #[test]
    fn extract_svg_dimensions_rounds_positive_fractional_sizes_up() {
        let svg = r#"<svg width="0.5" height="1.2" xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert_eq!(extract_svg_dimensions(svg), (Some(1), Some(2)));
    }

    #[test]
    fn extract_svg_dimensions_ignores_child_width_height() {
        let svg = r#"<svg viewBox="0 0 10 20" width="100%" height="100%" xmlns="http://www.w3.org/2000/svg"><rect width="999" height="888"/></svg>"#;
        assert_eq!(extract_svg_dimensions(svg), (Some(10), Some(20)));
    }

    #[test]
    fn diff_color_auto_disables_ansi_when_writing_to_file() {
        assert!(!diff_use_colors(ColorChoice::Auto, false));
        assert!(diff_use_colors(ColorChoice::Always, false));
        assert!(!diff_use_colors(ColorChoice::Never, true));
    }
}

#[cfg(test)]
mod config_tests {
    use super::{
        FrankenmermaidConfigFile, LayoutAlgorithmArg, OutputFormat, build_base_svg_render_config,
        build_layout_config, resolve_layout_algorithm, resolve_output_format,
        resolve_show_back_edges, resolve_theme_name,
    };
    use fm_layout::{CycleStrategy, EdgeRouting};
    use fm_render_svg::ThemePreset;

    #[test]
    fn documented_config_sections_parse_successfully() {
        let config: FrankenmermaidConfigFile = toml::from_str(
            r#"
                [core]
                deterministic = true
                max_input_bytes = 4096
                fallback_on_error = true

                [parser]
                intent_inference = true
                fuzzy_keyword_distance = 2
                auto_close_delimiters = true
                create_placeholder_nodes = true

                [layout]
                algorithm = "tree"
                cycle_strategy = "dfs-back"
                node_spacing = 96.0
                rank_spacing = 144.0
                edge_routing = "spline"

                [render]
                default_format = "term"
                show_back_edges = true
                reduced_motion = "never"

                [svg]
                theme = "forest"
                rounded_corners = 6.0
                padding = 24.0
                shadows = false
                gradients = false
                accessibility = false

                [term]
                tier = "compact"
                unicode = false
                minimap = true
            "#,
        )
        .expect("parse documented config");

        assert_eq!(config.core.max_input_bytes, Some(4096));
        assert_eq!(config.layout.algorithm.as_deref(), Some("tree"));
        assert_eq!(config.render.default_format.as_deref(), Some("term"));
        assert_eq!(config.svg.theme.as_deref(), Some("forest"));
        assert_eq!(config.svg.padding, Some(24.0));
        assert_eq!(config.term.tier.as_deref(), Some("compact"));
    }

    #[test]
    fn explicit_render_options_override_file_defaults() {
        let config: FrankenmermaidConfigFile = toml::from_str(
            r#"
                [layout]
                algorithm = "force"

                [render]
                default_format = "term"

                [svg]
                theme = "dark"
            "#,
        )
        .expect("parse config");

        assert_eq!(
            resolve_output_format(Some(OutputFormat::Svg), &config).expect("resolve format"),
            OutputFormat::Svg
        );
        assert_eq!(
            resolve_layout_algorithm(Some(LayoutAlgorithmArg::Tree), &config)
                .expect("resolve algorithm"),
            fm_layout::LayoutAlgorithm::Tree
        );
        assert_eq!(
            resolve_theme_name(Some(String::from("forest")), &config),
            "forest"
        );
    }

    #[test]
    fn file_defaults_apply_when_explicit_options_are_absent() {
        let config: FrankenmermaidConfigFile = toml::from_str(
            r#"
                [layout]
                algorithm = "sugiyama"
                cycle_strategy = "cycle-aware"
                node_spacing = 90.0
                rank_spacing = 150.0
                edge_routing = "spline"

                [render]
                default_format = "svg"
                reduced_motion = "never"

                [svg]
                theme = "dark"
                padding = 24.0
                shadows = false
                gradients = false
            "#,
        )
        .expect("parse config");

        let layout = build_layout_config(&config, None).expect("build layout config");
        assert_eq!(layout.cycle_strategy, CycleStrategy::CycleAware);
        assert_eq!(layout.edge_routing, EdgeRouting::Spline);
        assert_eq!(layout.spacing.node_spacing, 90.0);
        assert_eq!(layout.spacing.rank_spacing, 150.0);

        let svg = build_base_svg_render_config(&config).expect("build svg config");
        assert_eq!(svg.theme, ThemePreset::Dark);
        assert_eq!(svg.padding, 24.0);
        assert!(!svg.shadows);
        assert!(!svg.node_gradients);
        assert!(svg.animations_enabled);
    }

    #[test]
    fn svg_padding_rejects_negative_values() {
        let config: FrankenmermaidConfigFile = toml::from_str(
            r#"
                [svg]
                padding = -1.0
            "#,
        )
        .expect("parse config");

        let error = build_base_svg_render_config(&config)
            .expect_err("negative svg padding should be rejected");
        assert!(
            error.to_string().contains("svg.padding"),
            "error should name the invalid field: {error}"
        );
    }

    #[test]
    fn render_show_back_edges_defaults_to_true_and_reads_config() {
        let defaults = FrankenmermaidConfigFile::default();
        assert!(resolve_show_back_edges(&defaults));

        let config: FrankenmermaidConfigFile = toml::from_str(
            r"
                [render]
                show_back_edges = false
            ",
        )
        .expect("parse config");
        assert!(!resolve_show_back_edges(&config));
    }
}

#[cfg(test)]
mod input_tests {
    use super::load_input;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use tempfile::TempDir;

    static CWD_LOCK: Mutex<()> = Mutex::new(());

    struct CwdGuard {
        original: PathBuf,
    }

    impl CwdGuard {
        fn new(path: &Path) -> Self {
            let original = std::env::current_dir().expect("read cwd");
            std::env::set_current_dir(path).expect("set cwd");
            Self { original }
        }
    }

    impl Drop for CwdGuard {
        fn drop(&mut self) {
            let _ = std::env::set_current_dir(&self.original);
        }
    }

    #[test]
    fn inline_mermaid_like_input_does_not_read_existing_file() {
        let _guard = CWD_LOCK.lock().expect("lock cwd");
        let temp = TempDir::new().expect("temp dir");
        let _cwd = CwdGuard::new(temp.path());

        let path = temp.path().join("A[Start]");
        std::fs::write(&path, "file-contents").expect("write temp file");

        let input = "A[Start]";
        let loaded = load_input(input, 1024).expect("load inline input");
        assert_eq!(loaded, input);
    }

    #[test]
    fn explicit_path_wins_over_inline_heuristics() {
        let _guard = CWD_LOCK.lock().expect("lock cwd");
        let temp = TempDir::new().expect("temp dir");
        let _cwd = CwdGuard::new(temp.path());

        let path = temp.path().join("A[Start]");
        std::fs::write(&path, "file-contents").expect("write temp file");
        let input = "./A[Start]";
        let loaded = load_input(input, 1024).expect("load file input");
        assert_eq!(loaded, "file-contents");
    }

    #[test]
    fn filename_with_parens_reads_as_file_not_inline() {
        // Regression test: filenames like "report(final).mmd" should be read
        // as files, not treated as inline mermaid due to parentheses
        let _guard = CWD_LOCK.lock().expect("lock cwd");
        let temp = TempDir::new().expect("temp dir");
        let _cwd = CwdGuard::new(temp.path());

        let path = temp.path().join("report(final).mmd");
        std::fs::write(&path, "graph TD\n  A-->B").expect("write temp file");

        let input = "report(final).mmd";
        let loaded = load_input(input, 1024).expect("load file with parens");
        assert_eq!(loaded, "graph TD\n  A-->B");
    }

    #[test]
    fn filename_with_brackets_reads_as_file_not_inline() {
        // Regression test: filenames like "data[backup].txt" should be read
        // as files, not treated as inline mermaid due to brackets
        let _guard = CWD_LOCK.lock().expect("lock cwd");
        let temp = TempDir::new().expect("temp dir");
        let _cwd = CwdGuard::new(temp.path());

        let path = temp.path().join("data[backup].txt");
        std::fs::write(&path, "flowchart LR\n  X-->Y").expect("write temp file");

        let input = "data[backup].txt";
        let loaded = load_input(input, 1024).expect("load file with brackets");
        assert_eq!(loaded, "flowchart LR\n  X-->Y");
    }
}

#[cfg(test)]
mod interactive_tests {
    use super::{
        InteractiveBuffer, InteractiveSnapshot, cycle_interactive_theme, diagnostic_summary_line,
        interactive_help_line, interactive_layout, interactive_status_line,
        resolve_interactive_theme_index,
    };
    use crate::ValidationDiagnostic;
    use fm_core::StructuredDiagnostic;

    fn diagnostic(
        message: &str,
        line: Option<usize>,
        column: Option<usize>,
    ) -> ValidationDiagnostic {
        ValidationDiagnostic {
            stage: "parse".to_string(),
            payload: StructuredDiagnostic {
                error_code: "mermaid/error/test".to_string(),
                severity: "error".to_string(),
                message: message.to_string(),
                span: None,
                source_line: line,
                source_column: column,
                rule_id: None,
                confidence: None,
                remediation_hint: None,
            },
        }
    }

    #[test]
    fn theme_cycle_wraps_and_resolves_case_insensitively() {
        let dark = resolve_interactive_theme_index("DARK");
        assert_ne!(dark, 0);
        assert_eq!(cycle_interactive_theme(3), 0);
    }

    #[test]
    fn interactive_buffer_backspace_merges_lines() {
        let mut buffer = InteractiveBuffer::from_source("flowchart LR\nA-->B");
        buffer.cursor_row = 1;
        buffer.cursor_col = 0;
        buffer.backspace();

        assert_eq!(buffer.lines, vec!["flowchart LRA-->B".to_string()]);
        assert_eq!(buffer.cursor_row, 0);
    }

    #[test]
    fn interactive_buffer_insert_newline_splits_current_line() {
        let mut buffer = InteractiveBuffer::from_source("ABC");
        buffer.cursor_col = 1;
        buffer.insert_newline();

        assert_eq!(buffer.lines, vec!["A".to_string(), "BC".to_string()]);
        assert_eq!(buffer.cursor_row, 1);
        assert_eq!(buffer.cursor_col, 0);
    }

    #[test]
    fn interactive_layout_reserves_split_and_footer_rows() {
        let layout = interactive_layout(120, 30);
        assert_eq!(layout.editor_width + layout.preview_width + 1, 120);
        assert_eq!(layout.content_height, 26);
    }

    #[test]
    fn status_and_diagnostic_lines_include_core_session_context() {
        let snapshot = InteractiveSnapshot {
            diagram_type: "flowchart".to_string(),
            node_count: 2,
            edge_count: 1,
            render_time_ms: 3.5,
            preview_lines: vec![],
            diagnostics: vec![diagnostic("Broken edge", Some(2), Some(7))],
        };
        let buffer = InteractiveBuffer::from_source("flowchart LR\nA-->B");

        let status = interactive_status_line(&snapshot, &buffer, "dark");
        let diagnostics = diagnostic_summary_line(&snapshot.diagnostics);
        let help = interactive_help_line(&super::InteractiveKeyHints {
            save_supported: true,
        });

        assert!(status.contains("flowchart"));
        assert!(status.contains("nodes=2"));
        assert!(status.contains("theme=dark"));
        assert!(diagnostics.contains("Broken edge"));
        assert!(diagnostics.contains("@ 2:7"));
        assert!(help.contains("Ctrl-S"));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InteractiveTheme {
    name: &'static str,
    editor_fg: Color,
    accent_fg: Color,
    comment_fg: Color,
    preview_fg: Color,
    status_fg: Color,
    status_bg: Color,
    help_fg: Color,
    help_bg: Color,
    error_fg: Color,
    cursor_line_bg: Color,
}

const INTERACTIVE_THEMES: [InteractiveTheme; 4] = [
    InteractiveTheme {
        name: "default",
        editor_fg: Color::White,
        accent_fg: Color::Cyan,
        comment_fg: Color::DarkGrey,
        preview_fg: Color::White,
        status_fg: Color::Black,
        status_bg: Color::Cyan,
        help_fg: Color::White,
        help_bg: Color::DarkBlue,
        error_fg: Color::Red,
        cursor_line_bg: Color::DarkGrey,
    },
    InteractiveTheme {
        name: "dark",
        editor_fg: Color::Grey,
        accent_fg: Color::Magenta,
        comment_fg: Color::DarkGrey,
        preview_fg: Color::Grey,
        status_fg: Color::White,
        status_bg: Color::DarkMagenta,
        help_fg: Color::White,
        help_bg: Color::DarkGrey,
        error_fg: Color::Red,
        cursor_line_bg: Color::DarkBlue,
    },
    InteractiveTheme {
        name: "forest",
        editor_fg: Color::White,
        accent_fg: Color::Green,
        comment_fg: Color::DarkGreen,
        preview_fg: Color::White,
        status_fg: Color::Black,
        status_bg: Color::Green,
        help_fg: Color::Black,
        help_bg: Color::DarkGreen,
        error_fg: Color::Yellow,
        cursor_line_bg: Color::DarkGreen,
    },
    InteractiveTheme {
        name: "neutral",
        editor_fg: Color::White,
        accent_fg: Color::Blue,
        comment_fg: Color::Grey,
        preview_fg: Color::White,
        status_fg: Color::Black,
        status_bg: Color::Grey,
        help_fg: Color::Black,
        help_bg: Color::DarkGrey,
        error_fg: Color::DarkRed,
        cursor_line_bg: Color::DarkGrey,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct InteractiveBuffer {
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    scroll_row: usize,
    scroll_col: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InteractiveKeyHints {
    save_supported: bool,
}

#[derive(Debug, Clone)]
struct InteractiveSnapshot {
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    render_time_ms: f64,
    preview_lines: Vec<String>,
    diagnostics: Vec<ValidationDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct InteractiveLayout {
    editor_width: usize,
    preview_width: usize,
    content_height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InteractiveDrawStyle {
    fg: Color,
    bg: Option<Color>,
    bold: bool,
}

impl InteractiveBuffer {
    fn from_source(source: &str) -> Self {
        let normalized = source.replace("\r\n", "\n");
        let mut lines: Vec<String> = normalized.split('\n').map(str::to_string).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        if normalized.ends_with('\n') {
            lines.push(String::new());
        }

        Self {
            lines,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
        }
    }

    fn to_source(&self) -> String {
        self.lines.join("\n")
    }

    fn insert_char(&mut self, ch: char) {
        let line = &mut self.lines[self.cursor_row];
        let insert_at = self.cursor_col.min(line.len());
        line.insert(insert_at, ch);
        self.cursor_col = insert_at + ch.len_utf8();
    }

    fn insert_newline(&mut self) {
        let tail = self.lines[self.cursor_row].split_off(self.cursor_col);
        self.cursor_row += 1;
        self.lines.insert(self.cursor_row, tail);
        self.cursor_col = 0;
    }

    fn backspace(&mut self) {
        if self.cursor_col > 0 {
            let line = &mut self.lines[self.cursor_row];
            let previous_boundary = line[..self.cursor_col]
                .char_indices()
                .last()
                .map_or(0, |(idx, _)| idx);
            line.replace_range(previous_boundary..self.cursor_col, "");
            self.cursor_col = previous_boundary;
            return;
        }

        if self.cursor_row == 0 {
            return;
        }

        let current_line = self.lines.remove(self.cursor_row);
        self.cursor_row -= 1;
        self.cursor_col = self.lines[self.cursor_row].len();
        self.lines[self.cursor_row].push_str(&current_line);
    }

    fn move_left(&mut self) {
        if self.cursor_col > 0 {
            self.cursor_col = self.lines[self.cursor_row][..self.cursor_col]
                .char_indices()
                .last()
                .map_or(0, |(idx, _)| idx);
        } else if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.lines[self.cursor_row].len();
        }
    }

    fn move_right(&mut self) {
        let line = &self.lines[self.cursor_row];
        if self.cursor_col < line.len() {
            let next = line[self.cursor_col..]
                .chars()
                .next()
                .map_or(self.cursor_col, |ch| self.cursor_col + ch.len_utf8());
            self.cursor_col = next;
        } else if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.cursor_row > 0 {
            self.cursor_row -= 1;
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
    }

    fn move_down(&mut self) {
        if self.cursor_row + 1 < self.lines.len() {
            self.cursor_row += 1;
            self.cursor_col = self.cursor_col.min(self.lines[self.cursor_row].len());
        }
    }

    fn ensure_cursor_visible(&mut self, layout: &InteractiveLayout) {
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        } else if self.cursor_row >= self.scroll_row.saturating_add(layout.content_height) {
            self.scroll_row = self
                .cursor_row
                .saturating_add(1)
                .saturating_sub(layout.content_height);
        }

        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        } else if self.cursor_col >= self.scroll_col.saturating_add(layout.editor_width) {
            self.scroll_col = self
                .cursor_col
                .saturating_add(1)
                .saturating_sub(layout.editor_width);
        }
    }
}

fn resolve_interactive_theme_index(theme: &str) -> usize {
    INTERACTIVE_THEMES
        .iter()
        .position(|candidate| candidate.name.eq_ignore_ascii_case(theme))
        .unwrap_or(0)
}

fn cycle_interactive_theme(index: usize) -> usize {
    (index + 1) % INTERACTIVE_THEMES.len()
}

fn interactive_layout(cols: u16, rows: u16) -> InteractiveLayout {
    let total_width = usize::from(cols.max(40));
    let total_height = usize::from(rows.max(8));
    let editor_width = ((total_width.saturating_sub(1)) * 45) / 100;
    let preview_width = total_width.saturating_sub(editor_width).saturating_sub(1);
    let content_height = total_height.saturating_sub(4).max(1);
    InteractiveLayout {
        editor_width: editor_width.max(16),
        preview_width: preview_width.max(16),
        content_height,
    }
}

fn known_mermaid_keyword(line: &str) -> Option<&str> {
    const KEYWORDS: &[&str] = &[
        "flowchart",
        "graph",
        "sequenceDiagram",
        "classDiagram",
        "stateDiagram",
        "stateDiagram-v2",
        "erDiagram",
        "journey",
        "gantt",
        "pie",
        "gitGraph",
        "mindmap",
        "timeline",
        "sankey-beta",
        "xychart-beta",
        "quadrantChart",
        "C4Context",
        "C4Container",
        "C4Component",
        "C4Dynamic",
        "C4Deployment",
        "subgraph",
        "end",
        "title",
        "accTitle",
        "accDescr",
        "classDef",
        "class",
        "style",
        "linkStyle",
        "click",
    ];

    let trimmed = line.trim_start();
    KEYWORDS.iter().copied().find(|keyword| {
        trimmed
            .strip_prefix(keyword)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    })
}

fn diagnostic_summary_line(diagnostics: &[ValidationDiagnostic]) -> String {
    if let Some(diagnostic) = diagnostics.first() {
        let location = match (
            diagnostic.payload.source_line,
            diagnostic.payload.source_column,
        ) {
            (Some(line), Some(column)) => format!(" @ {line}:{column}"),
            (Some(line), None) => format!(" @ {line}"),
            _ => String::new(),
        };
        format!(
            "{} {}{}",
            diagnostic.payload.severity.to_ascii_uppercase(),
            diagnostic.payload.message,
            location
        )
    } else {
        "No diagnostics".to_string()
    }
}

fn interactive_status_line(
    snapshot: &InteractiveSnapshot,
    buffer: &InteractiveBuffer,
    theme_name: &str,
) -> String {
    format!(
        " {} | nodes={} edges={} | {:.2}ms | theme={} | Ln {}, Col {} ",
        snapshot.diagram_type,
        snapshot.node_count,
        snapshot.edge_count,
        snapshot.render_time_ms,
        theme_name,
        buffer.cursor_row + 1,
        buffer.cursor_col + 1,
    )
}

fn interactive_help_line(hints: &InteractiveKeyHints) -> String {
    if hints.save_supported {
        " Tab cycle theme | Ctrl-S save file | Ctrl-Q quit ".to_string()
    } else {
        " Tab cycle theme | Ctrl-Q quit ".to_string()
    }
}

fn build_interactive_snapshot(
    source: &str,
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    preview_width: usize,
    preview_height: usize,
) -> InteractiveSnapshot {
    let start = Instant::now();
    let parsed = parse_with_mode_and_config(source, parse_mode, &parser_config);
    let traced_layout = layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        LayoutAlgorithm::Auto,
        LayoutConfig::default(),
        LayoutGuardrails::default(),
    );
    let mut diagnostics = collect_parse_diagnostics(&parsed);
    diagnostics.extend(collect_structural_diagnostics(&parsed));
    diagnostics.extend(collect_layout_diagnostics(&traced_layout));
    sort_diagnostics(&mut diagnostics);

    let result = render_term_with_layout_and_config(
        &parsed.ir,
        &traced_layout.layout,
        &TermRenderConfig::rich(),
        preview_width.max(16),
        preview_height.max(4),
    );

    InteractiveSnapshot {
        diagram_type: parsed.ir.diagram_type.as_str().to_string(),
        node_count: parsed.ir.nodes.len(),
        edge_count: parsed.ir.edges.len(),
        render_time_ms: start.elapsed().as_secs_f64() * 1000.0,
        preview_lines: result.output.lines().map(str::to_string).collect(),
        diagnostics,
    }
}

fn visible_line_slice(line: &str, scroll_col: usize, width: usize) -> String {
    line.chars().skip(scroll_col).take(width).collect()
}

fn draw_padded_text(
    stdout: &mut io::Stdout,
    x: u16,
    y: u16,
    width: usize,
    text: &str,
    style: InteractiveDrawStyle,
) -> Result<()> {
    let clipped: String = text.chars().take(width).collect();
    queue!(stdout, MoveTo(x, y))?;
    if let Some(background) = style.bg {
        queue!(stdout, SetBackgroundColor(background))?;
    }
    queue!(stdout, SetForegroundColor(style.fg))?;
    queue!(
        stdout,
        SetAttribute(if style.bold {
            Attribute::Bold
        } else {
            Attribute::Reset
        })
    )?;
    queue!(stdout, Print(format!("{clipped:<width$}")))?;
    queue!(stdout, ResetColor, SetAttribute(Attribute::Reset))?;
    Ok(())
}

fn draw_editor_line(
    stdout: &mut io::Stdout,
    position: (u16, u16),
    width: usize,
    line: &str,
    scroll_col: usize,
    theme: InteractiveTheme,
    highlight_row: bool,
) -> Result<()> {
    let (x, y) = position;
    let visible = visible_line_slice(line, scroll_col, width);
    let trimmed = line.trim_start();
    let keyword = known_mermaid_keyword(line);
    let line_bg = highlight_row.then_some(theme.cursor_line_bg);

    queue!(stdout, MoveTo(x, y))?;
    if let Some(background) = line_bg {
        queue!(stdout, SetBackgroundColor(background))?;
    }

    let chars: Vec<char> = visible.chars().collect();
    for (index, ch) in chars.iter().enumerate() {
        let keyword_highlight = keyword.is_some_and(|value| index < value.len());
        let accent_char = keyword_highlight
            || matches!(
                ch,
                '-' | '>' | '<' | '=' | '.' | '{' | '}' | '[' | ']' | '(' | ')' | '|'
            );
        let color = if trimmed.starts_with("%%") {
            theme.comment_fg
        } else if accent_char {
            theme.accent_fg
        } else {
            theme.editor_fg
        };
        queue!(stdout, SetForegroundColor(color), Print(*ch))?;
    }

    let visible_len = chars.len();
    if visible_len < width {
        queue!(stdout, SetForegroundColor(theme.editor_fg))?;
        queue!(stdout, Print(" ".repeat(width - visible_len)))?;
    }

    queue!(stdout, ResetColor, SetAttribute(Attribute::Reset))?;
    Ok(())
}

fn draw_interactive_ui(
    stdout: &mut io::Stdout,
    buffer: &mut InteractiveBuffer,
    snapshot: &InteractiveSnapshot,
    theme_index: usize,
    save_supported: bool,
    terminal_cols: u16,
    terminal_rows: u16,
) -> Result<()> {
    let theme = INTERACTIVE_THEMES[theme_index];
    let layout = interactive_layout(terminal_cols, terminal_rows);
    buffer.ensure_cursor_visible(&layout);
    let separator_x = u16::try_from(layout.editor_width).unwrap_or(u16::MAX);
    let content_start_y = 2_u16;
    let help_y = terminal_rows.saturating_sub(2);
    let diagnostic_y = terminal_rows.saturating_sub(1);

    queue!(stdout, Hide, Clear(ClearType::All))?;
    draw_padded_text(
        stdout,
        0,
        0,
        usize::from(terminal_cols),
        &interactive_status_line(snapshot, buffer, theme.name),
        InteractiveDrawStyle {
            fg: theme.status_fg,
            bg: Some(theme.status_bg),
            bold: true,
        },
    )?;
    draw_padded_text(
        stdout,
        0,
        1,
        layout.editor_width,
        " EDITOR ",
        InteractiveDrawStyle {
            fg: theme.accent_fg,
            bg: None,
            bold: true,
        },
    )?;
    draw_padded_text(
        stdout,
        separator_x.saturating_add(1),
        1,
        layout.preview_width,
        " PREVIEW ",
        InteractiveDrawStyle {
            fg: theme.accent_fg,
            bg: None,
            bold: true,
        },
    )?;

    for row in 0..layout.content_height {
        let screen_y = content_start_y.saturating_add(u16::try_from(row).unwrap_or(u16::MAX));
        queue!(
            stdout,
            MoveTo(separator_x, screen_y),
            SetForegroundColor(theme.accent_fg),
            Print("│"),
            ResetColor
        )?;

        let line_index = buffer.scroll_row + row;
        let line = buffer.lines.get(line_index).map_or("", String::as_str);
        draw_editor_line(
            stdout,
            (0, screen_y),
            layout.editor_width,
            line,
            buffer.scroll_col,
            theme,
            line_index == buffer.cursor_row,
        )?;

        let preview_text = snapshot.preview_lines.get(row).map_or("", String::as_str);
        draw_padded_text(
            stdout,
            separator_x.saturating_add(1),
            screen_y,
            layout.preview_width,
            preview_text,
            InteractiveDrawStyle {
                fg: theme.preview_fg,
                bg: None,
                bold: false,
            },
        )?;
    }

    draw_padded_text(
        stdout,
        0,
        help_y,
        usize::from(terminal_cols),
        &interactive_help_line(&InteractiveKeyHints { save_supported }),
        InteractiveDrawStyle {
            fg: theme.help_fg,
            bg: Some(theme.help_bg),
            bold: false,
        },
    )?;
    draw_padded_text(
        stdout,
        0,
        diagnostic_y,
        usize::from(terminal_cols),
        &diagnostic_summary_line(&snapshot.diagnostics),
        InteractiveDrawStyle {
            fg: theme.error_fg,
            bg: None,
            bold: false,
        },
    )?;

    let cursor_x = u16::try_from(buffer.cursor_col.saturating_sub(buffer.scroll_col)).unwrap_or(0);
    let cursor_y = u16::try_from(
        buffer
            .cursor_row
            .saturating_sub(buffer.scroll_row)
            .saturating_add(usize::from(content_start_y)),
    )
    .unwrap_or(content_start_y);
    queue!(stdout, MoveTo(cursor_x, cursor_y), Show)?;
    stdout.flush()?;
    Ok(())
}

struct InteractiveTerminalGuard;

impl InteractiveTerminalGuard {
    fn enter(stdout: &mut io::Stdout) -> Result<Self> {
        enable_raw_mode()?;
        execute!(stdout, EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}

impl Drop for InteractiveTerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen, ResetColor);
    }
}

fn cmd_interactive(
    input: &str,
    parse_mode: MermaidParseMode,
    parser_config: ParserConfig,
    theme: &str,
    max_input_bytes: usize,
) -> Result<()> {
    if !io::stdout().is_terminal() {
        anyhow::bail!("interactive mode requires a terminal stdout");
    }

    let source = load_input(input, max_input_bytes)?;
    let mut buffer = InteractiveBuffer::from_source(&source);
    let save_path = (input != "-" && Path::new(input).exists()).then_some(input.to_string());
    let mut theme_index = resolve_interactive_theme_index(theme);
    let mut stdout = io::stdout();
    let _guard = InteractiveTerminalGuard::enter(&mut stdout)?;

    loop {
        let (cols, rows) = terminal::size().unwrap_or((120, 32));
        let layout = interactive_layout(cols, rows);
        let snapshot = build_interactive_snapshot(
            &buffer.to_source(),
            parse_mode,
            parser_config,
            layout.preview_width,
            layout.content_height,
        );
        draw_interactive_ui(
            &mut stdout,
            &mut buffer,
            &snapshot,
            theme_index,
            save_path.is_some(),
            cols,
            rows,
        )?;

        if !event::poll(std::time::Duration::from_millis(100))? {
            continue;
        }

        match event::read()? {
            Event::Key(KeyEvent {
                code: KeyCode::Char('q'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => break,
            Event::Key(KeyEvent {
                code: KeyCode::Char('s'),
                modifiers,
                ..
            }) if modifiers.contains(KeyModifiers::CONTROL) => {
                if let Some(path) = &save_path {
                    std::fs::write(path, buffer.to_source())
                        .context(format!("Failed to save interactive buffer to: {path}"))?;
                }
            }
            Event::Key(KeyEvent {
                code: KeyCode::Tab, ..
            }) => {
                theme_index = cycle_interactive_theme(theme_index);
            }
            Event::Key(KeyEvent {
                code: KeyCode::Enter,
                ..
            }) => buffer.insert_newline(),
            Event::Key(KeyEvent {
                code: KeyCode::Backspace,
                ..
            }) => buffer.backspace(),
            Event::Key(KeyEvent {
                code: KeyCode::Left,
                ..
            }) => buffer.move_left(),
            Event::Key(KeyEvent {
                code: KeyCode::Right,
                ..
            }) => buffer.move_right(),
            Event::Key(KeyEvent {
                code: KeyCode::Up, ..
            }) => buffer.move_up(),
            Event::Key(KeyEvent {
                code: KeyCode::Down,
                ..
            }) => buffer.move_down(),
            Event::Key(KeyEvent {
                code: KeyCode::Char(ch),
                modifiers,
                ..
            }) if !modifiers.intersects(KeyModifiers::CONTROL | KeyModifiers::ALT) => {
                buffer.insert_char(ch);
            }
            _ => {}
        }
    }

    Ok(())
}

// =============================================================================
// Command: watch (optional feature)
// =============================================================================

#[cfg(feature = "watch")]
fn cmd_watch(input: &str, options: RenderCommandOptions<'_>, clear: bool) -> Result<()> {
    use notify::{Config, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    let path = Path::new(input);
    if !path.exists() {
        anyhow::bail!("File not found: {input}");
    }

    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(tx, Config::default())?;
    watcher.watch(path, RecursiveMode::NonRecursive)?;

    println!("Watching {input} for changes... (Ctrl+C to stop)");

    // Initial render
    if let Err(e) = render_and_output(input, options.clone(), clear) {
        eprintln!("Initial render failed: {e}");
    }

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(_event)) => {
                // Debounce rapid events
                std::thread::sleep(Duration::from_millis(100));
                while rx.try_recv().is_ok() {}

                if let Err(e) = render_and_output(input, options.clone(), clear) {
                    eprintln!("Render error: {e}");
                }
            }
            Ok(Err(e)) => {
                eprintln!("Watch error: {e}");
            }
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                // Continue waiting
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break;
            }
        }
    }

    Ok(())
}

#[cfg(feature = "watch")]
fn render_and_output(input: &str, options: RenderCommandOptions<'_>, clear: bool) -> Result<()> {
    if clear {
        print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top-left
    }

    cmd_render(input, options)
}

// =============================================================================
// Command: serve (optional feature)
// =============================================================================

#[cfg(feature = "serve")]
fn cmd_serve(host: &str, port: u16, open: bool, options: RenderCommandOptions<'_>) -> Result<()> {
    use tiny_http::{Response, Server};

    let addr = format!("{host}:{port}");
    let server = Server::http(&addr).map_err(|e| anyhow::anyhow!("Failed to start server: {e}"))?;

    let url = format!("http://{addr}");
    println!("FrankenMermaid Playground running at: {url}");
    println!("Press Ctrl+C to stop");

    if open {
        let _ = open_browser(&url);
    }

    for mut request in server.incoming_requests() {
        let url_path = request.url();

        let response = match url_path {
            "/" => serve_playground_html(),
            "/render" => handle_render_request(&mut request, &options),
            _ => Response::from_string("Not Found").with_status_code(404),
        };

        let _ = request.respond(response);
    }

    Ok(())
}

#[cfg(feature = "serve")]
fn serve_playground_html() -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    use tiny_http::{Header, Response};

    let html = r#"<!DOCTYPE html>
<html>
<head>
    <title>FrankenMermaid Playground</title>
    <meta charset="UTF-8">
    <style>
        * { box-sizing: border-box; }
        body { font-family: system-ui, sans-serif; margin: 0; padding: 20px; background: #1a1a2e; color: #eee; }
        h1 { margin: 0 0 20px 0; color: #00d9ff; }
        .container { display: flex; gap: 20px; height: calc(100vh - 100px); }
        .panel { flex: 1; display: flex; flex-direction: column; }
        textarea { flex: 1; font-family: monospace; font-size: 14px; padding: 15px; border: 1px solid #333; border-radius: 8px; background: #0d0d1a; color: #eee; resize: none; }
        #output { flex: 1; border: 1px solid #333; border-radius: 8px; background: white; display: flex; align-items: center; justify-content: center; overflow: auto; }
        #output svg { max-width: 100%; max-height: 100%; }
        .label { font-size: 12px; color: #888; margin-bottom: 5px; }
        .error { color: #ff6b6b; padding: 20px; }
    </style>
</head>
<body>
    <h1>🧟 FrankenMermaid Playground</h1>
    <div class="container">
        <div class="panel">
            <div class="label">INPUT (Mermaid syntax)</div>
            <textarea id="input" placeholder="flowchart LR
    A[Start] --> B{Decision}
    B -->|Yes| C[Do it]
    B -->|No| D[Skip]
    C --> E[End]
    D --> E">flowchart LR
    A[Start] --> B{Decision}
    B -->|Yes| C[Do it]
    B -->|No| D[Skip]
    C --> E[End]
    D --> E</textarea>
        </div>
        <div class="panel">
            <div class="label">OUTPUT (SVG)</div>
            <div id="output"></div>
        </div>
    </div>
    <script>
        const input = document.getElementById('input');
        const output = document.getElementById('output');
        let timeout;

        async function render() {
            try {
                const res = await fetch('/render', {
                    method: 'POST',
                    body: input.value,
                    headers: { 'Content-Type': 'text/plain' }
                });
                const data = await res.text();
                if (res.ok) {
                    output.innerHTML = data;
                } else {
                    output.innerHTML = '<div class="error">' + data + '</div>';
                }
            } catch (e) {
                output.innerHTML = '<div class="error">Connection error</div>';
            }
        }

        input.addEventListener('input', () => {
            clearTimeout(timeout);
            timeout = setTimeout(render, 300);
        });

        render();
    </script>
</body>
</html>"#;

    let mut response = Response::from_data(html.as_bytes().to_vec());
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]) {
        response = response.with_header(header);
    }
    response
}

#[cfg(feature = "serve")]
fn handle_render_request(
    request: &mut tiny_http::Request,
    options: &RenderCommandOptions<'_>,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    use tiny_http::{Header, Response};

    let content_length = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Length"))
        .and_then(|header| header.value.as_str().parse::<usize>().ok());
    if content_length.is_some_and(|len| len > options.max_input_bytes) {
        return Response::from_string(format!(
            "Request body exceeds {} bytes",
            options.max_input_bytes
        ))
        .with_status_code(413);
    }

    let mut body = String::new();
    let read_limit = u64::try_from(options.max_input_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut reader = request.as_reader().take(read_limit);
    if let Err(e) = reader.read_to_string(&mut body) {
        return Response::from_string(format!("Failed to read body: {e}")).with_status_code(400);
    }
    if body.len() > options.max_input_bytes {
        return Response::from_string(format!(
            "Request body exceeds {} bytes",
            options.max_input_bytes
        ))
        .with_status_code(413);
    }

    let svg_bytes = match render_source(&body, options) {
        Ok(outcome) => outcome.rendered,
        Err(err) => {
            return Response::from_string(format!("Render error: {err}")).with_status_code(400);
        }
    };

    let mut response = Response::from_data(svg_bytes);
    if let Ok(header) = Header::from_bytes(&b"Content-Type"[..], &b"image/svg+xml"[..]) {
        response = response.with_header(header);
    }
    response
}

#[cfg(feature = "serve")]
fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(url).status()?;

    #[cfg(target_os = "linux")]
    let _ = std::process::Command::new("xdg-open").arg(url).status()?;

    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("cmd")
        .args(["/c", "start", "", url])
        .status()?;

    Ok(())
}

/// Lowercase hex encoding for digest output.
///
/// RustCrypto 0.11 moved digest results from `GenericArray` to `hybrid_array::Array`,
/// which does not implement `LowerHex`, so the previous `format!("{:x}", ..)` no
/// longer compiles. Encoding explicitly keeps this dependency-free.
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}
