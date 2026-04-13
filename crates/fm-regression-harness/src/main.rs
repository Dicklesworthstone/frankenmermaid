#![forbid(unsafe_code)]

use anyhow::{Context, Result};
use clap::Parser;
use fm_core::{DiagramType, MermaidDiagramIr};
use fm_layout::{DiagramLayout, LayoutConfig, layout_diagram_with_config};
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "fm-regression-harness")]
#[command(about = "Render regression harness for FrankenMermaid", long_about = None)]
struct Args {
    /// Directory containing .mmd fixtures (and optional .svg goldens).
    #[arg(long, default_value = "crates/fm-cli/tests/golden")]
    input_dir: PathBuf,
    /// Output directory for rendered artifacts and report.
    #[arg(long, default_value = "artifacts/regression-harness")]
    output_dir: PathBuf,
    /// Update golden .svg files in the input directory.
    #[arg(long)]
    update_goldens: bool,
    /// Exit with non-zero status on any mismatch.
    #[arg(long)]
    fail_on_mismatch: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseReport {
    case_id: String,
    input_path: String,
    golden_path: Option<String>,
    output_path: String,
    status: String,
    output_hash: String,
    golden_hash: Option<String>,
    parse_ms: u128,
    layout_ms: u128,
    render_ms: u128,
    node_count: usize,
    edge_count: usize,
    warning_count: usize,
    // Quality metrics
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<QualityMetrics>,
}

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct QualityMetrics {
    /// Number of edge crossings in the layout
    edge_crossings: usize,
    /// Standard deviation of edge lengths (lower = more uniform)
    edge_length_stddev: f64,
    /// Number of back-edges (edges going against flow direction)
    back_edge_count: usize,
    /// Number of cycles detected in the graph
    cycle_count: usize,
    /// Number of connected components
    component_count: usize,
    /// Number of bridge edges
    bridge_count: usize,
    /// Total edge length (layout quality proxy)
    total_edge_length: f64,
    /// Mean edge length
    mean_edge_length: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RunReport {
    input_dir: String,
    output_dir: String,
    total: usize,
    matched: usize,
    mismatched: usize,
    missing: usize,
    cases: Vec<CaseReport>,
}

fn main() -> Result<()> {
    let args = Args::parse();
    let run_dir = args.output_dir.join("latest");
    let current_dir = run_dir.join("current");
    let golden_dir = run_dir.join("golden");

    fs::create_dir_all(&current_dir)
        .with_context(|| format!("create output dir {}", current_dir.display()))?;
    fs::create_dir_all(&golden_dir)
        .with_context(|| format!("create output dir {}", golden_dir.display()))?;

    let mut cases = list_cases(&args.input_dir)?;
    cases.sort_by_key(|path| case_sort_key(path));

    let mut reports = Vec::new();
    let mut matched = 0;
    let mut mismatched = 0;
    let mut missing = 0;

    for input_path in cases {
        let case_id = input_path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .ok_or_else(|| anyhow::anyhow!("invalid case filename"))?
            .to_string();
        let input = fs::read_to_string(&input_path)
            .with_context(|| format!("read input {}", input_path.display()))?;

        let parse_start = Instant::now();
        let parsed = parse(&input);
        let parse_ms = parse_start.elapsed().as_millis();

        let svg_config = stable_svg_config();
        let layout_start = Instant::now();
        let layout_config = LayoutConfig {
            font_metrics: Some(svg_config.font_metrics()),
            ..Default::default()
        };
        let layout = layout_diagram_with_config(&parsed.ir, layout_config);
        let layout_ms = layout_start.elapsed().as_millis();

        let render_start = Instant::now();
        let rendered = render_svg_with_layout(&parsed.ir, &layout, &svg_config);
        let render_ms = render_start.elapsed().as_millis();

        let normalized = normalize_svg(&rendered);
        let output_hash = fnv_hex(&normalized);

        let output_path = current_dir.join(format!("{case_id}.svg"));
        fs::write(&output_path, &normalized)
            .with_context(|| format!("write output {}", output_path.display()))?;

        let golden_path = args.input_dir.join(format!("{case_id}.svg"));
        let mut golden_hash = None;
        let golden_existed = golden_path.exists();
        if args.update_goldens {
            fs::write(&golden_path, &normalized)
                .with_context(|| format!("update golden {}", golden_path.display()))?;
        }
        let status = if golden_path.exists() {
            let golden = fs::read_to_string(&golden_path)
                .with_context(|| format!("read golden {}", golden_path.display()))?;
            let golden_norm = normalize_svg(&golden);
            golden_hash = Some(fnv_hex(&golden_norm));
            if args.update_goldens {
                matched += 1;
                if golden_existed {
                    "updated-golden".to_string()
                } else {
                    "created-golden".to_string()
                }
            } else if golden_hash.as_ref() == Some(&output_hash) {
                matched += 1;
                "matched".to_string()
            } else {
                mismatched += 1;
                "mismatch".to_string()
            }
        } else {
            missing += 1;
            "missing-golden".to_string()
        };

        if golden_path.exists() {
            let golden_copy_path = golden_dir.join(format!("{case_id}.svg"));
            fs::write(&golden_copy_path, fs::read_to_string(&golden_path)?)
                .with_context(|| format!("write golden copy {}", golden_copy_path.display()))?;
        }

        let quality = compute_quality_metrics(&parsed.ir, &layout);

        reports.push(CaseReport {
            case_id: case_id.clone(),
            input_path: input_path.display().to_string(),
            golden_path: golden_path
                .exists()
                .then(|| golden_path.display().to_string()),
            output_path: output_path.display().to_string(),
            status,
            output_hash,
            golden_hash,
            parse_ms,
            layout_ms,
            render_ms,
            node_count: parsed.ir.nodes.len(),
            edge_count: parsed.ir.edges.len(),
            warning_count: parsed.warnings.len(),
            quality,
        });
    }

    let report = RunReport {
        input_dir: args.input_dir.display().to_string(),
        output_dir: run_dir.display().to_string(),
        total: reports.len(),
        matched,
        mismatched,
        missing,
        cases: reports,
    };

    let report_json = serde_json::to_string_pretty(&report)?;
    fs::write(run_dir.join("report.json"), format!("{report_json}\n"))
        .context("write report.json")?;
    let report_html = render_report_html(&report);
    fs::write(run_dir.join("report.html"), report_html).context("write report.html")?;

    if args.fail_on_mismatch && report.mismatched > 0 {
        anyhow::bail!("{} mismatches detected", report.mismatched);
    }

    Ok(())
}

fn stable_svg_config() -> SvgRenderConfig {
    SvgRenderConfig {
        node_gradients: false,
        glow_enabled: false,
        cluster_fill_opacity: 1.0,
        inactive_opacity: 1.0,
        shadow_blur: 3.0,
        shadow_color: String::new(),
        ..Default::default()
    }
}

fn normalize_svg(svg: &str) -> String {
    let mut normalized = svg.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv_hex(value: &str) -> String {
    format!("{:016x}", fnv1a_64(value.as_bytes()))
}

fn compute_quality_metrics(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
) -> Option<QualityMetrics> {
    // Only compute for flowcharts and similar directed diagrams
    if !matches!(ir.diagram_type, DiagramType::Flowchart | DiagramType::State) {
        return None;
    }

    if ir.edges.is_empty() {
        return Some(QualityMetrics::default());
    }

    // Compute edge lengths
    let mut edge_lengths: Vec<f64> = Vec::new();
    for edge in &layout.edges {
        if edge.points.len() >= 2 {
            let mut length = 0.0;
            for i in 1..edge.points.len() {
                let dx = f64::from(edge.points[i].x - edge.points[i - 1].x);
                let dy = f64::from(edge.points[i].y - edge.points[i - 1].y);
                length += (dx * dx + dy * dy).sqrt();
            }
            edge_lengths.push(length);
        }
    }

    let total_edge_length: f64 = edge_lengths.iter().sum();
    let mean_edge_length = if edge_lengths.is_empty() {
        0.0
    } else {
        total_edge_length / edge_lengths.len() as f64
    };

    // Standard deviation of edge lengths
    let edge_length_stddev = if edge_lengths.len() < 2 {
        0.0
    } else {
        let variance: f64 = edge_lengths
            .iter()
            .map(|&len| (len - mean_edge_length).powi(2))
            .sum::<f64>()
            / edge_lengths.len() as f64;
        variance.sqrt()
    };

    // Count edge crossings (simplified: check segment intersections)
    let edge_crossings = count_edge_crossings(layout);

    // Count back-edges (edges going upward in TB layout)
    let back_edge_count = count_back_edges(ir, layout);

    // Use layout stats for remaining metrics
    Some(QualityMetrics {
        edge_crossings,
        edge_length_stddev,
        back_edge_count,
        cycle_count: layout.stats.cycle_count,
        component_count: 1, // Layout doesn't track this directly
        bridge_count: 0,    // Would need FNX analysis
        total_edge_length,
        mean_edge_length,
    })
}

fn count_edge_crossings(layout: &DiagramLayout) -> usize {
    let mut crossings = 0;
    let edges: Vec<_> = layout.edges.iter().collect();

    for i in 0..edges.len() {
        for j in (i + 1)..edges.len() {
            let e1 = &edges[i];
            let e2 = &edges[j];

            // Skip if edges share an endpoint (they'll trivially intersect there)
            if edges_share_endpoint(e1, e2) {
                continue;
            }

            for seg1_idx in 1..e1.points.len() {
                let p1 = (
                    f64::from(e1.points[seg1_idx - 1].x),
                    f64::from(e1.points[seg1_idx - 1].y),
                );
                let p2 = (
                    f64::from(e1.points[seg1_idx].x),
                    f64::from(e1.points[seg1_idx].y),
                );

                for seg2_idx in 1..e2.points.len() {
                    let p3 = (
                        f64::from(e2.points[seg2_idx - 1].x),
                        f64::from(e2.points[seg2_idx - 1].y),
                    );
                    let p4 = (
                        f64::from(e2.points[seg2_idx].x),
                        f64::from(e2.points[seg2_idx].y),
                    );

                    if segments_intersect(p1, p2, p3, p4) {
                        crossings += 1;
                    }
                }
            }
        }
    }
    crossings
}

fn edges_share_endpoint(e1: &fm_layout::LayoutEdgePath, e2: &fm_layout::LayoutEdgePath) -> bool {
    const EPS: f32 = 1.0;
    let e1_start = e1.points.first();
    let e1_end = e1.points.last();
    let e2_start = e2.points.first();
    let e2_end = e2.points.last();

    match (e1_start, e1_end, e2_start, e2_end) {
        (Some(s1), Some(e1), Some(s2), Some(e2)) => {
            points_close(s1, s2, EPS)
                || points_close(s1, e2, EPS)
                || points_close(e1, s2, EPS)
                || points_close(e1, e2, EPS)
        }
        _ => false,
    }
}

fn points_close(a: &fm_layout::LayoutPoint, b: &fm_layout::LayoutPoint, eps: f32) -> bool {
    (a.x - b.x).abs() < eps && (a.y - b.y).abs() < eps
}

fn segments_intersect(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64), p4: (f64, f64)) -> bool {
    const EPS: f64 = 1e-9;

    let d1 = direction(p3, p4, p1);
    let d2 = direction(p3, p4, p2);
    let d3 = direction(p1, p2, p3);
    let d4 = direction(p1, p2, p4);

    // Standard crossing test
    if ((d1 > EPS && d2 < -EPS) || (d1 < -EPS && d2 > EPS))
        && ((d3 > EPS && d4 < -EPS) || (d3 < -EPS && d4 > EPS))
    {
        return true;
    }

    // Collinear overlap cases (using epsilon comparison)
    if d1.abs() < EPS && on_segment(p3, p4, p1) {
        return true;
    }
    if d2.abs() < EPS && on_segment(p3, p4, p2) {
        return true;
    }
    if d3.abs() < EPS && on_segment(p1, p2, p3) {
        return true;
    }
    if d4.abs() < EPS && on_segment(p1, p2, p4) {
        return true;
    }

    false
}

fn direction(p1: (f64, f64), p2: (f64, f64), p3: (f64, f64)) -> f64 {
    (p3.0 - p1.0) * (p2.1 - p1.1) - (p2.0 - p1.0) * (p3.1 - p1.1)
}

fn on_segment(p1: (f64, f64), p2: (f64, f64), p: (f64, f64)) -> bool {
    p.0 >= p1.0.min(p2.0) && p.0 <= p1.0.max(p2.0) && p.1 >= p1.1.min(p2.1) && p.1 <= p1.1.max(p2.1)
}

fn count_back_edges(_ir: &MermaidDiagramIr, layout: &DiagramLayout) -> usize {
    // For TB layout, count edges where target Y < source Y (going upward)
    let mut count = 0;

    for edge in &layout.edges {
        if edge.points.len() >= 2 {
            let start_y = edge.points.first().map(|p| p.y).unwrap_or(0.0);
            let end_y = edge.points.last().map(|p| p.y).unwrap_or(0.0);

            // In TB layout, Y increases downward, so back-edge goes upward
            if end_y < start_y - 1.0 {
                count += 1;
            }
        }
    }
    count
}

fn list_cases(input_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut cases = Vec::new();
    for entry in fs::read_dir(input_dir)
        .with_context(|| format!("read input dir {}", input_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("mmd") {
            cases.push(path);
        }
    }
    Ok(cases)
}

fn case_sort_key(path: &Path) -> (String, String) {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    (stem.to_string(), file_name.to_string())
}

fn render_report_html(report: &RunReport) -> String {
    let mut html = String::new();
    html.push_str("<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\"/>");
    html.push_str("<title>FrankenMermaid Regression Harness</title>");
    html.push_str("<style>");
    html.push_str(
        "body{font-family:system-ui, sans-serif; background:#0b0f14; color:#e6edf3; margin:0; padding:24px;}\n",
    );
    html.push_str(".summary{display:flex; gap:16px; margin-bottom:20px;}\n");
    html.push_str(".card{background:#121a23; padding:12px 16px; border-radius:10px;}\n");
    html.push_str(
        ".case{margin-bottom:24px; padding:16px; background:#10161f; border-radius:12px;}\n",
    );
    html.push_str(".status{font-weight:600;}\n");
    html.push_str(".grid{display:grid; grid-template-columns:1fr 1fr; gap:16px;}\n");
    html.push_str(".panel{background:#0b1119; padding:12px; border-radius:8px;}\n");
    html.push_str(".panel svg{width:100%; height:auto;}\n");
    html.push_str(
        ".label{font-size:12px; text-transform:uppercase; letter-spacing:.12em; color:#8b949e;}\n",
    );
    html.push_str(".mono{font-family:ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, 'Liberation Mono', 'Courier New', monospace;}\n");
    html.push_str("</style></head><body>");

    html.push_str("<h1>FrankenMermaid Regression Harness</h1>");
    html.push_str("<div class=\"summary\">");
    html.push_str(&format!(
        "<div class=\"card\">Total<br><span class=\"mono\">{}</span></div>",
        report.total
    ));
    html.push_str(&format!(
        "<div class=\"card\">Matched<br><span class=\"mono\">{}</span></div>",
        report.matched
    ));
    html.push_str(&format!(
        "<div class=\"card\">Mismatched<br><span class=\"mono\">{}</span></div>",
        report.mismatched
    ));
    html.push_str(&format!(
        "<div class=\"card\">Missing<br><span class=\"mono\">{}</span></div>",
        report.missing
    ));
    html.push_str("</div>");

    for case in &report.cases {
        html.push_str("<div class=\"case\">");
        html.push_str(&format!("<div class=\"label\">{}</div>", case.case_id));
        html.push_str(&format!(
            "<div class=\"status\">Status: {}</div>",
            case.status
        ));
        html.push_str("<div class=\"grid\">");
        html.push_str("<div class=\"panel\"><div class=\"label\">Golden</div>");
        if case.golden_path.is_some() {
            html.push_str(&format!(
                "<object type=\"image/svg+xml\" data=\"golden/{}.svg\"></object>",
                case.case_id
            ));
        } else {
            html.push_str("<div>Missing golden</div>");
        }
        html.push_str("</div>");
        html.push_str("<div class=\"panel\"><div class=\"label\">Current</div>");
        html.push_str(&format!(
            "<object type=\"image/svg+xml\" data=\"current/{}.svg\"></object>",
            case.case_id
        ));
        html.push_str("</div>");
        html.push_str("</div>");
        html.push_str("</div>");
    }

    html.push_str("</body></html>");
    html
}
