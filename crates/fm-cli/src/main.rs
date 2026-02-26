#![forbid(unsafe_code)]

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

use std::io::{self, Read, Write};
use std::path::Path;
use std::time::Instant;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use fm_core::{DiagramType, MermaidDiagramIr};
use fm_layout::layout_diagram;
use fm_parser::{detect_type, parse, parse_evidence_json};
use fm_render_svg::{SvgRenderConfig, ThemePreset, render_svg_with_config};
use fm_render_term::{TermRenderConfig, render_term_with_config};
use serde::Serialize;
use tracing::{debug, info, warn};

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

        /// Output format
        #[arg(short, long, value_enum, default_value = "svg")]
        format: OutputFormat,

        /// Theme name (default, dark, forest, neutral)
        #[arg(short, long, default_value = "default")]
        theme: String,

        /// Output file path. If omitted, writes to stdout.
        #[arg(short, long)]
        output: Option<String>,

        /// Output width (for PNG/terminal)
        #[arg(short = 'W', long)]
        width: Option<u32>,

        /// Output height (for PNG/terminal)
        #[arg(short = 'H', long)]
        height: Option<u32>,

        /// Output as JSON with metadata (timing, dimensions, etc.)
        #[arg(long)]
        json: bool,
    },

    /// Parse a diagram and output its IR as JSON.
    Parse {
        /// Input file path or "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

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

    /// Validate a diagram and report diagnostics.
    Validate {
        /// Input file path or "-" for stdin.
        #[arg(default_value = "-")]
        input: String,

        /// Output as JSON (structured diagnostics)
        #[arg(long)]
        json: bool,

        /// Exit with non-zero status on warnings (not just errors)
        #[arg(long)]
        strict: bool,
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

/// Result of rendering a diagram.
#[derive(Debug, Serialize)]
struct RenderResult {
    format: String,
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    output_bytes: usize,
    width: Option<u32>,
    height: Option<u32>,
    parse_time_ms: f64,
    layout_time_ms: f64,
    render_time_ms: f64,
    total_time_ms: f64,
    warnings: Vec<String>,
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
struct ValidateResult {
    valid: bool,
    diagram_type: String,
    node_count: usize,
    edge_count: usize,
    warnings: Vec<ValidationWarning>,
    errors: Vec<ValidationError>,
}

#[derive(Debug, Serialize)]
struct ValidationWarning {
    code: String,
    message: String,
    suggestion: Option<String>,
}

#[derive(Debug, Serialize)]
struct ValidationError {
    code: String,
    message: String,
    line: Option<usize>,
    column: Option<usize>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    init_tracing(cli.verbose, cli.quiet);

    match cli.command {
        Command::Render {
            input,
            format,
            theme,
            output,
            width,
            height,
            json,
        } => cmd_render(
            &input,
            format,
            &theme,
            output.as_deref(),
            width,
            height,
            json,
        ),

        Command::Parse {
            input,
            full,
            pretty,
        } => cmd_parse(&input, full, pretty),

        Command::Detect { input, json } => cmd_detect(&input, json),

        Command::Validate {
            input,
            json,
            strict,
        } => cmd_validate(&input, json, strict),

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
        .without_time()
        .try_init();
}

fn load_input(input: &str) -> Result<String> {
    if input == "-" {
        let mut buffer = String::new();
        io::stdin()
            .read_to_string(&mut buffer)
            .context("Failed to read from stdin")?;
        Ok(buffer)
    } else if Path::new(input).exists() {
        std::fs::read_to_string(input).context(format!("Failed to read file: {input}"))
    } else {
        // Treat as inline diagram text
        Ok(input.to_string())
    }
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

// =============================================================================
// Command: render
// =============================================================================

fn cmd_render(
    input: &str,
    format: OutputFormat,
    theme: &str,
    output: Option<&str>,
    width: Option<u32>,
    height: Option<u32>,
    json_output: bool,
) -> Result<()> {
    let total_start = Instant::now();

    // Parse
    let parse_start = Instant::now();
    let source = load_input(input)?;
    let parsed = parse(&source);
    let parse_time = parse_start.elapsed();

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
    let layout = layout_diagram(&parsed.ir);
    let layout_time = layout_start.elapsed();

    debug!(
        "Layout: bounds={}x{}, crossings={}",
        layout.bounds.width, layout.bounds.height, layout.stats.crossing_count
    );

    // Render
    let render_start = Instant::now();
    let (rendered, actual_width, actual_height) =
        render_format(&parsed.ir, format, theme, width, height)?;
    let render_time = render_start.elapsed();

    let total_time = total_start.elapsed();

    if json_output {
        let result = RenderResult {
            format: format!("{format:?}").to_lowercase(),
            diagram_type: parsed.ir.diagram_type.as_str().to_string(),
            node_count: parsed.ir.nodes.len(),
            edge_count: parsed.ir.edges.len(),
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
        eprintln!("{json_str}");
    }

    // Write output
    match format {
        OutputFormat::Png => write_output_bytes(output, &rendered)?,
        _ => write_output(output, &String::from_utf8_lossy(&rendered))?,
    }

    info!(
        "Rendered {} {} nodes, {} edges in {:.2}ms",
        parsed.ir.diagram_type.as_str(),
        parsed.ir.nodes.len(),
        parsed.ir.edges.len(),
        total_time.as_secs_f64() * 1000.0
    );

    Ok(())
}

fn render_format(
    ir: &MermaidDiagramIr,
    format: OutputFormat,
    theme: &str,
    width: Option<u32>,
    height: Option<u32>,
) -> Result<(Vec<u8>, Option<u32>, Option<u32>)> {
    match format {
        OutputFormat::Svg => {
            let mut svg_config = SvgRenderConfig::default();
            svg_config.theme = resolve_theme_preset(theme, svg_config.theme);
            let svg = render_svg_with_config(ir, &svg_config);
            // Extract dimensions from SVG if available
            let (w, h) = extract_svg_dimensions(&svg);
            Ok((svg.into_bytes(), w, h))
        }

        OutputFormat::Png => {
            #[cfg(feature = "png")]
            {
                let mut svg_config = SvgRenderConfig::default();
                svg_config.theme = resolve_theme_preset(theme, svg_config.theme);
                let svg = render_svg_with_config(ir, &svg_config);
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
            warn_if_unknown_theme(theme);
            let (cols, rows) = terminal_size(width, height);
            let config = TermRenderConfig::rich();
            let result = render_term_with_config(ir, &config, cols, rows);
            Ok((
                result.output.into_bytes(),
                Some(result.width as u32),
                Some(result.height as u32),
            ))
        }

        OutputFormat::Ascii => {
            warn_if_unknown_theme(theme);
            let (cols, rows) = terminal_size(width, height);
            let mut config = TermRenderConfig::compact();
            config.glyph_mode = fm_core::MermaidGlyphMode::Ascii;
            let result = render_term_with_config(ir, &config, cols, rows);
            Ok((
                result.output.into_bytes(),
                Some(result.width as u32),
                Some(result.height as u32),
            ))
        }
    }
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

fn warn_if_unknown_theme(theme: &str) {
    let fallback = SvgRenderConfig::default().theme;
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
        width.map(|w| w as usize).unwrap_or(default_cols),
        height.map(|h| h as usize).unwrap_or(default_rows),
    )
}

fn extract_svg_dimensions(svg: &str) -> (Option<u32>, Option<u32>) {
    // Simple regex-free extraction of width/height from SVG
    let width = svg.find("width=\"").and_then(|i| {
        let start = i + 7;
        let end = svg[start..].find('"').map(|e| start + e)?;
        svg[start..end].parse::<f32>().ok().map(|v| v as u32)
    });

    let height = svg.find("height=\"").and_then(|i| {
        let start = i + 8;
        let end = svg[start..].find('"').map(|e| start + e)?;
        svg[start..end].parse::<f32>().ok().map(|v| v as u32)
    });

    (width, height)
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
    use super::svg_to_png;

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
}

fn cmd_parse(input: &str, full: bool, pretty: bool) -> Result<()> {
    let source = load_input(input)?;
    let parsed = parse(&source);

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

fn cmd_detect(input: &str, json_output: bool) -> Result<()> {
    let source = load_input(input)?;
    let diagram_type = detect_type(&source);

    // Determine confidence based on detection method
    let first_line = source
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim().starts_with("%%"))
        .unwrap_or("")
        .trim();

    let (confidence, detection_method) = analyze_detection_confidence(&source, diagram_type);
    let support_level = get_support_level(diagram_type);

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

fn analyze_detection_confidence(
    source: &str,
    diagram_type: DiagramType,
) -> (&'static str, &'static str) {
    let first_line = source
        .lines()
        .find(|l| !l.trim().is_empty() && !l.trim().starts_with("%%"))
        .unwrap_or("")
        .trim()
        .to_lowercase();

    // Check for explicit diagram keywords
    let has_explicit_keyword = first_line.starts_with("flowchart")
        || first_line.starts_with("graph")
        || first_line.starts_with("sequencediagram")
        || first_line.starts_with("classdiagram")
        || first_line.starts_with("statediagram")
        || first_line.starts_with("erdiagram")
        || first_line.starts_with("pie")
        || first_line.starts_with("gantt")
        || first_line.starts_with("journey")
        || first_line.starts_with("mindmap")
        || first_line.starts_with("timeline")
        || first_line.starts_with("quadrantchart")
        || first_line.starts_with("requirement")
        || first_line.starts_with("digraph")
        || first_line.starts_with("graph {")
        || first_line.starts_with("strict digraph")
        || first_line.starts_with("strict graph");

    match (diagram_type, has_explicit_keyword) {
        (DiagramType::Unknown, _) => ("low", "no recognized header"),
        (_, true) => ("high", "explicit keyword match"),
        (_, false) => ("medium", "content heuristics"),
    }
}

fn get_support_level(diagram_type: DiagramType) -> &'static str {
    match diagram_type {
        DiagramType::Flowchart => "full",
        DiagramType::Sequence => "partial",
        DiagramType::Class => "partial",
        DiagramType::State => "partial",
        DiagramType::Er => "partial",
        DiagramType::Pie => "basic",
        DiagramType::Gantt => "basic",
        DiagramType::Journey => "basic",
        DiagramType::Mindmap => "basic",
        DiagramType::Timeline => "basic",
        DiagramType::QuadrantChart => "basic",
        DiagramType::Requirement => "basic",
        DiagramType::GitGraph => "unsupported",
        DiagramType::C4Context
        | DiagramType::C4Container
        | DiagramType::C4Component
        | DiagramType::C4Dynamic
        | DiagramType::C4Deployment => "unsupported",
        DiagramType::Sankey => "unsupported",
        DiagramType::XyChart => "unsupported",
        DiagramType::BlockBeta => "unsupported",
        DiagramType::PacketBeta => "basic",
        DiagramType::ArchitectureBeta => "unsupported",
        DiagramType::Unknown => "unknown",
    }
}

// =============================================================================
// Command: validate
// =============================================================================

fn cmd_validate(input: &str, json_output: bool, strict: bool) -> Result<()> {
    let source = load_input(input)?;
    let parsed = parse(&source);

    // Convert parse warnings to validation warnings
    let warnings: Vec<ValidationWarning> = parsed
        .warnings
        .iter()
        .map(|msg| ValidationWarning {
            code: categorize_warning(msg),
            message: msg.clone(),
            suggestion: suggest_fix(msg),
        })
        .collect();

    // Check for structural issues
    let mut errors: Vec<ValidationError> = Vec::new();

    // Check for unknown diagram type
    if parsed.ir.diagram_type == DiagramType::Unknown {
        errors.push(ValidationError {
            code: "E001".to_string(),
            message: "Could not detect diagram type".to_string(),
            line: Some(1),
            column: None,
        });
    }

    // Check for empty diagram
    if parsed.ir.nodes.is_empty() && parsed.ir.edges.is_empty() {
        errors.push(ValidationError {
            code: "E002".to_string(),
            message: "Diagram has no nodes or edges".to_string(),
            line: None,
            column: None,
        });
    }

    // Check for orphaned edges (referencing non-existent nodes)
    // This would require more sophisticated validation based on the IR structure

    let valid = errors.is_empty() && (!strict || warnings.is_empty());

    let result = ValidateResult {
        valid,
        diagram_type: parsed.ir.diagram_type.as_str().to_string(),
        node_count: parsed.ir.nodes.len(),
        edge_count: parsed.ir.edges.len(),
        warnings,
        errors,
    };

    if json_output {
        let output = serde_json::to_string_pretty(&result)?;
        println!("{output}");
    } else {
        if result.valid {
            println!("✓ Valid {} diagram", result.diagram_type);
        } else {
            println!("✗ Invalid diagram");
        }

        println!("  Nodes: {}", result.node_count);
        println!("  Edges: {}", result.edge_count);

        if !result.errors.is_empty() {
            println!("\nErrors:");
            for err in &result.errors {
                let location = match (err.line, err.column) {
                    (Some(l), Some(c)) => format!(" (line {l}, col {c})"),
                    (Some(l), None) => format!(" (line {l})"),
                    _ => String::new(),
                };
                println!("  [{}] {}{}", err.code, err.message, location);
            }
        }

        if !result.warnings.is_empty() {
            println!("\nWarnings:");
            for warn in &result.warnings {
                println!("  [{}] {}", warn.code, warn.message);
                if let Some(suggestion) = &warn.suggestion {
                    println!("       → {suggestion}");
                }
            }
        }
    }

    if !result.valid {
        std::process::exit(1);
    }

    Ok(())
}

fn categorize_warning(msg: &str) -> String {
    let msg_lower = msg.to_lowercase();

    if msg_lower.contains("empty") {
        "W001".to_string()
    } else if msg_lower.contains("duplicate") {
        "W002".to_string()
    } else if msg_lower.contains("unknown") || msg_lower.contains("unrecognized") {
        "W003".to_string()
    } else if msg_lower.contains("deprecated") {
        "W004".to_string()
    } else {
        "W000".to_string()
    }
}

fn suggest_fix(msg: &str) -> Option<String> {
    let msg_lower = msg.to_lowercase();

    if msg_lower.contains("empty") {
        Some("Add nodes and edges to your diagram".to_string())
    } else if msg_lower.contains("unknown") && msg_lower.contains("diagram") {
        Some("Start your diagram with a type declaration like 'flowchart LR'".to_string())
    } else {
        None
    }
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

    let source = load_input(input)?;
    let parsed = parse(&source);
    let (rendered, _, _) = render_format(&parsed.ir, format, "default", None, None)?;

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

    let header =
        Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..]).unwrap();
    Response::from_data(html.as_bytes().to_vec()).with_header(header)
}

#[cfg(feature = "serve")]
fn handle_render_request(
    request: &mut tiny_http::Request,
) -> tiny_http::Response<std::io::Cursor<Vec<u8>>> {
    use tiny_http::{Header, Response};

    let mut body = String::new();
    if let Err(e) = request.as_reader().read_to_string(&mut body) {
        return Response::from_string(format!("Failed to read body: {e}")).with_status_code(400);
    }

    let parsed = parse(&body);
    let svg = render_svg_with_config(&parsed.ir, &SvgRenderConfig::default());

    let header = Header::from_bytes(&b"Content-Type"[..], &b"image/svg+xml"[..]).unwrap();
    Response::from_data(svg.into_bytes()).with_header(header)
}

#[cfg(feature = "serve")]
fn open_browser(url: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg(url).spawn()?;

    #[cfg(target_os = "linux")]
    std::process::Command::new("xdg-open").arg(url).spawn()?;

    #[cfg(target_os = "windows")]
    std::process::Command::new("cmd")
        .args(["/c", "start", url])
        .spawn()?;

    Ok(())
}
