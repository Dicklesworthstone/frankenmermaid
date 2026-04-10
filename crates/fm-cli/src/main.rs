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

#[cfg(feature = "png")]
use std::collections::BTreeMap;
use std::io::{self, IsTerminal, Read, Write};
use std::path::{Path, PathBuf};
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
    MermaidLayoutDecisionLedger, MermaidNativePressureSignals, MermaidParseMode, MermaidTier,
    StructuredDiagnostic, capability_matrix, capability_matrix_json_pretty,
    mermaid_layout_guard_observability,
};
use fm_layout::{
    CycleStrategy, EdgeRouting, LayoutAlgorithm, LayoutConfig, LayoutGuardrails, TracedLayout,
    build_layout_decision_ledger, build_layout_guard_report_with_pressure,
    layout_diagram_traced_with_config_and_guardrails, layout_source_map,
};
use fm_parser::{
    ParserConfig, detect_type_with_confidence_and_config, first_significant_line,
    parse_evidence_json, parse_with_mode, parse_with_mode_and_config,
};
use fm_render_svg::{
    A11yConfig, SvgRenderConfig, ThemePreset, describe_diagram_with_layout, render_svg_with_layout,
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
    shadows: Option<bool>,
    gradients: Option<bool>,
    accessibility: Option<bool>,
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
        } => {
            let format = resolve_output_format(format, &loaded_config.file)?;
            let layout_algorithm = resolve_layout_algorithm(layout_algorithm, &loaded_config.file)?;
            let theme = resolve_theme_name(theme, &loaded_config.file);
            let layout_config = build_layout_config(&loaded_config.file, font_size)?;
            let svg_base_config = build_base_svg_render_config(&loaded_config.file)?;
            let term_base_config = build_base_term_render_config(&loaded_config.file)?;
            let show_back_edges = resolve_show_back_edges(&loaded_config.file);
            let show_minimap = term_base_config.show_minimap;
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
                },
            )
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
        } => cmd_watch(&input, format, output.as_deref(), clear),

        #[cfg(feature = "serve")]
        Command::Serve { port, host, open } => cmd_serve(&host, port, open),
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

fn validate_reduced_motion_name(value: &str) -> Result<()> {
    match value.trim().to_ascii_lowercase().as_str() {
        "auto" | "always" | "never" => Ok(()),
        other => anyhow::bail!("unknown render.reduced_motion '{other}'"),
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
    validate_reduced_motion_name(reduced_motion)?;
    match reduced_motion.trim().to_ascii_lowercase().as_str() {
        "always" => config.animations_enabled = false,
        "never" => config.animations_enabled = true,
        "auto" => {}
        _ => unreachable!("validated above"),
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
    } else if Path::new(input).exists() {
        let metadata =
            std::fs::metadata(input).context(format!("Failed to stat input file: {input}"))?;
        if metadata.len() > u64::try_from(max_input_bytes).unwrap_or(u64::MAX) {
            anyhow::bail!(
                "Input file '{}' is {} bytes, which exceeds core.max_input_bytes={max_input_bytes}",
                input,
                metadata.len()
            );
        }
        let file = std::fs::File::open(input).context(format!("Failed to open file: {input}"))?;
        let mut handle = file.take(
            u64::try_from(max_input_bytes)
                .unwrap_or(u64::MAX)
                .saturating_add(1),
        );
        let mut content = String::new();
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
    format!("{:x}", hasher.finalize())
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
    } = options;
    let (width, height) = dimensions;
    if json_output && output.is_none() {
        anyhow::bail!("--json requires --output so rendered output does not mix with metadata");
    }
    if source_map_out.is_some() && format != OutputFormat::Svg {
        anyhow::bail!("--source-map-out is only supported with --format svg");
    }

    let source = load_input(input, max_input_bytes)?;
    let total_start = Instant::now();
    let pressure = MermaidNativePressureSignals::sample().into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);

    // Parse
    let parse_start = Instant::now();
    let parsed = parse_with_mode_and_config(&source, parse_mode, &parser_config);
    let parse_time = parse_start.elapsed();
    budget_broker.record_parse(u64::try_from(parse_time.as_millis()).unwrap_or(u64::MAX));

    debug!(
        "Parsed: type={:?}, nodes={}, edges={}, warnings={}",
        parsed.ir.diagram_type,
        parsed.ir.nodes.len(),
        parsed.ir.edges.len(),
        parsed.warnings.len()
    );

    for warning in &parsed.warnings {
        warn!("Parse warning: {warning}");
    }

    // Layout
    let layout_start = Instant::now();
    let layout_guardrails = LayoutGuardrails {
        max_layout_time_ms: budget_broker.layout_time_budget_ms(),
        max_layout_iterations: budget_broker
            .layout_iteration_budget(LayoutGuardrails::default().max_layout_iterations),
        max_route_ops: budget_broker.route_budget(LayoutGuardrails::default().max_route_ops),
    };
    let traced_layout = fm_layout::layout_diagram_traced_with_config_and_guardrails(
        &parsed.ir,
        layout_algorithm,
        layout_config,
        layout_guardrails,
    );
    let layout = &traced_layout.layout;
    let layout_time = layout_start.elapsed();
    budget_broker.record_layout(layout_time.as_millis().min(u128::from(u64::MAX)) as u64);
    let mut guard_report =
        build_layout_guard_report_with_pressure(&parsed.ir, &traced_layout, pressure);
    let (_cx, observability) = mermaid_layout_guard_observability(
        "cli.render",
        &source,
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
        theme
    };
    let (rendered, actual_width, actual_height) = render_format(
        &parsed.ir,
        layout,
        format,
        RenderSurfaceOptions {
            theme: effective_theme,
            font_size,
            svg_base_config,
            term_base_config,
            show_back_edges,
            show_minimap,
            embed_source_spans,
            dimensions: (width, height),
            degradation: guard_report.degradation.clone(),
        },
    )?;
    let render_time = render_start.elapsed();
    budget_broker.record_render(render_time.as_millis().min(u128::from(u64::MAX)) as u64);
    guard_report.budget_broker = budget_broker.clone();
    let layout_decision_ledger =
        build_layout_decision_ledger(&parsed.ir, &traced_layout, &guard_report);
    let layout_decision_ledger_jsonl = layout_decision_ledger.to_jsonl()?;

    let total_time = total_start.elapsed();
    let source_map = layout_source_map(&parsed.ir, layout);
    let accessibility_summary = describe_diagram_with_layout(&parsed.ir, Some(layout));

    if let Some(path) = source_map_out {
        let artifact = serde_json::to_string_pretty(&source_map)?;
        std::fs::write(path, artifact)
            .context(format!("Failed to write source map file: {path}"))?;
        info!("Wrote source map artifact to: {path}");
    }

    if json_output {
        let result = RenderResult {
            format: format!("{format:?}").to_lowercase(),
            parse_mode: parse_mode.as_str().to_string(),
            embedded_source_spans: embed_source_spans,
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
            source_span_node_count: count_known_node_spans(layout),
            source_span_edge_count: count_known_edge_spans(layout),
            source_span_cluster_count: count_known_cluster_spans(layout),
            source_map_entry_count: source_map.entries.len(),
            source_map_out: source_map_out.map(str::to_string),
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
            warnings: parsed.warnings,
        };

        let json_str = serde_json::to_string_pretty(&result)?;
        println!("{json_str}");
    }

    // Write output
    match format {
        OutputFormat::Png => write_output_bytes(output, &rendered)?,
        _ => write_output(output, &String::from_utf8_lossy(&rendered))?,
    }

    info!(
        "Rendered {} via layout {}->{} with {} nodes, {} edges in {:.2}ms",
        parsed.ir.diagram_type.as_str(),
        traced_layout.trace.dispatch.requested.as_str(),
        traced_layout.trace.dispatch.selected.as_str(),
        parsed.ir.nodes.len(),
        parsed.ir.edges.len(),
        total_time.as_secs_f64() * 1000.0
    );

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
    let width = svg.find("width=\"").and_then(|i| {
        let start = i + 7;
        let end = svg[start..].find('"').map(|e| start + e)?;
        parse_svg_dimension_value(&svg[start..end])
    });

    let height = svg.find("height=\"").and_then(|i| {
        let start = i + 8;
        let end = svg[start..].find('"').map(|e| start + e)?;
        parse_svg_dimension_value(&svg[start..end])
    });

    match (width, height) {
        (Some(width), Some(height)) => (Some(width), Some(height)),
        _ => extract_viewbox_dimensions(svg).unwrap_or((width, height)),
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

#[cfg(feature = "png")]
fn resolve_svg_custom_properties_for_rasterization(svg: &str) -> String {
    let Some(style_start) = svg.find("<style>") else {
        return svg.to_string();
    };
    let style_content_start = style_start + "<style>".len();
    let Some(style_end_rel) = svg[style_content_start..].find("</style>") else {
        return svg.to_string();
    };
    let style_content_end = style_content_start + style_end_rel;
    let style_content = &svg[style_content_start..style_content_end];
    let custom_properties = extract_svg_custom_properties(style_content);
    if custom_properties.is_empty() {
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
        let Some(rel_end) = input[content_start..].find(')') else {
            output.push_str(&input[start..]);
            return output;
        };
        let end = content_start + rel_end;
        let body = &input[content_start..end];
        let property_name = body.split_once(',').map_or(body, |(name, _)| name).trim();

        if let Some(value) = custom_properties.get(property_name) {
            output.push_str(value);
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
    let (px_width, px_height) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => {
            let scale = w as f32 / size.width();
            (w, (size.height() * scale) as u32)
        }
        (None, Some(h)) => {
            let scale = h as f32 / size.height();
            ((size.width() * scale) as u32, h)
        }
        (None, None) => (size.width() as u32, size.height() as u32),
    };
    if px_width == 0 || px_height == 0 {
        anyhow::bail!("PNG dimensions must be greater than 0");
    }

    let mut pixmap =
        tiny_skia::Pixmap::new(px_width, px_height).context("Failed to create pixmap")?;

    let scale_x = px_width as f32 / size.width();
    let scale_y = px_height as f32 / size.height();

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
    let parsed = parse_with_mode_and_config(&source, parse_mode, &parser_config);

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
    } = options;

    let source = load_input(input, max_input_bytes)?;
    let total_start = Instant::now();
    let pressure = MermaidNativePressureSignals::sample().into_report();
    let mut budget_broker = MermaidBudgetLedger::new(&pressure);

    let parse_start = Instant::now();
    let parsed = parse_with_mode_and_config(&source, parse_mode, &parser_config);
    let parse_time = parse_start.elapsed();
    budget_broker.record_parse(u64::try_from(parse_time.as_millis()).unwrap_or(u64::MAX));

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
    let layout_decision_ledger_jsonl = layout_decision_ledger.to_jsonl()?;

    let mut diagnostics = collect_parse_diagnostics(&parsed);
    diagnostics.extend(collect_structural_diagnostics(&parsed));
    diagnostics.extend(collect_layout_diagnostics(&traced_layout));
    diagnostics.extend(collect_render_diagnostics(&svg_output));
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
        ColorChoice, OutputFormat, RenderSurfaceOptions, SvgRenderConfig, TermRenderConfig,
        ThemePreset, build_svg_render_config, diff_use_colors, extract_svg_dimensions,
        layout_without_back_edges, normalize_positive_font_size, parse_positive_dimension_arg,
        parse_positive_font_size_arg, render_format, terminal_size,
    };
    use fm_layout::{
        DiagramLayout, LayoutEdgePath, LayoutExtensions, LayoutPoint, LayoutRect, LayoutStats,
        layout_diagram,
    };
    use fm_parser::parse;

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
                    points: vec![
                        LayoutPoint { x: 0.0, y: 0.0 },
                        LayoutPoint { x: 1.0, y: 0.0 },
                    ],
                    reversed: false,
                    is_self_loop: false,
                    parallel_offset: 0.0,
                    bundle_count: 1,
                    bundled: false,
                },
                LayoutEdgePath {
                    edge_index: 1,
                    span: Default::default(),
                    points: vec![
                        LayoutPoint { x: 1.0, y: 0.0 },
                        LayoutPoint { x: 0.0, y: 0.0 },
                    ],
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
        assert!(!svg.shadows);
        assert!(!svg.node_gradients);
        assert!(svg.animations_enabled);
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
fn cmd_watch(input: &str, format: OutputFormat, output: Option<&str>, clear: bool) -> Result<()> {
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
    if let Err(e) = render_and_output(input, format, output, clear) {
        eprintln!("Initial render failed: {e}");
    }

    loop {
        match rx.recv_timeout(Duration::from_secs(1)) {
            Ok(Ok(_event)) => {
                // Debounce rapid events
                std::thread::sleep(Duration::from_millis(100));
                while rx.try_recv().is_ok() {}

                if let Err(e) = render_and_output(input, format, output, clear) {
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
fn render_and_output(
    input: &str,
    format: OutputFormat,
    output: Option<&str>,
    clear: bool,
) -> Result<()> {
    if clear {
        print!("\x1B[2J\x1B[H"); // Clear screen and move cursor to top-left
    }

    let source = load_input(input, DEFAULT_MAX_INPUT_BYTES)?;
    let parsed = fm_parser::parse(&source);
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let (rendered, _, _) = render_format(
        &parsed.ir,
        &layout,
        format,
        RenderSurfaceOptions {
            theme: "default",
            font_size: None,
            svg_base_config: SvgRenderConfig::default(),
            term_base_config: TermRenderConfig::rich(),
            show_back_edges: true,
            show_minimap: false,
            embed_source_spans: false,
            dimensions: (None, None),
            degradation: fm_core::MermaidDegradationPlan::default(),
        },
    )?;

    match format {
        OutputFormat::Png => write_output_bytes(output, &rendered)?,
        _ => {
            let text = String::from_utf8_lossy(&rendered);
            if output.is_some() {
                write_output(output, &text)?;
            } else {
                println!("{text}");
            }
        }
    }

    Ok(())
}

// =============================================================================
// Command: serve (optional feature)
// =============================================================================

#[cfg(feature = "serve")]
fn cmd_serve(host: &str, port: u16, open: bool) -> Result<()> {
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
            "/render" => handle_render_request(&mut request),
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
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    use tiny_http::{Header, Response};

    let content_length = request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Content-Length"))
        .and_then(|header| header.value.as_str().parse::<usize>().ok());
    if content_length.is_some_and(|len| len > DEFAULT_MAX_INPUT_BYTES) {
        return Response::from_string(format!(
            "Request body exceeds {DEFAULT_MAX_INPUT_BYTES} bytes"
        ))
        .with_status_code(413);
    }

    let mut body = String::new();
    let mut reader = request
        .as_reader()
        .take(u64::try_from(DEFAULT_MAX_INPUT_BYTES).unwrap_or(u64::MAX) + 1);
    if let Err(e) = reader.read_to_string(&mut body) {
        return Response::from_string(format!("Failed to read body: {e}")).with_status_code(400);
    }
    if body.len() > DEFAULT_MAX_INPUT_BYTES {
        return Response::from_string(format!(
            "Request body exceeds {DEFAULT_MAX_INPUT_BYTES} bytes"
        ))
        .with_status_code(413);
    }

    let parsed = fm_parser::parse(&body);
    let layout = fm_layout::layout_diagram(&parsed.ir);
    let svg = render_svg_with_layout(&parsed.ir, &layout, &SvgRenderConfig::default());

    let mut response = Response::from_data(svg.into_bytes());
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
        .args(["/c", "start", url])
        .status()?;

    Ok(())
}
