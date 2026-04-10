//! Fixture-backed FrankenTUI conformance harness for parser/render parity.

use fm_core::MermaidLinkMode;
use fm_core::{IrNode, IrXySeriesKind};
use fm_layout::layout_diagram;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_layout};
use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct FixtureCase {
    id: String,
    description: String,
    input_path: String,
    source_refs: Vec<String>,
    expected: FixtureExpectation,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectation {
    diagram_type: String,
    #[serde(default)]
    warnings: WarningExpectation,
    counts: CountExpectation,
    #[serde(default)]
    nodes: Vec<NodeExpectation>,
    #[serde(default)]
    edge_labels: Vec<String>,
    #[serde(default)]
    xychart: Option<XyChartExpectation>,
    #[serde(default)]
    svg_contains: Vec<String>,
    #[serde(default)]
    svg_not_contains: Vec<String>,
}

#[derive(Debug, Default, Deserialize)]
struct WarningExpectation {
    exact_count: Option<usize>,
    #[serde(default)]
    contains: Vec<String>,
    #[serde(default)]
    absent: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CountExpectation {
    nodes: usize,
    edges: usize,
    clusters: Option<usize>,
    subgraphs: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct NodeExpectation {
    id: String,
    #[serde(default)]
    classes: Vec<String>,
    href: Option<String>,
    callback: Option<String>,
    tooltip: Option<String>,
}

#[derive(Debug, Deserialize)]
struct XyChartExpectation {
    title: Option<String>,
    #[serde(default)]
    x_categories: Vec<String>,
    y_label: Option<String>,
    y_min: Option<f32>,
    y_max: Option<f32>,
    #[serde(default)]
    series: Vec<XySeriesExpectation>,
}

#[derive(Debug, Deserialize)]
struct XySeriesExpectation {
    kind: String,
    name: Option<String>,
    values: Vec<f32>,
}

fn test_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn manifest_path() -> PathBuf {
    test_root().join("frankentui_conformance_cases.json")
}

fn load_cases() -> Vec<FixtureCase> {
    let path = manifest_path();
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed reading {}: {err}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|err| panic!("failed parsing {}: {err}", path.display()))
}

fn normalize_svg(svg: &str) -> String {
    let mut normalized = svg.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
}

fn node_by_id<'a>(nodes: &'a [IrNode], id: &str) -> &'a IrNode {
    nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("expected node '{id}' to exist"))
}

fn edge_label_texts(ir: &fm_core::MermaidDiagramIr) -> Vec<String> {
    ir.edges
        .iter()
        .filter_map(|edge| edge.label)
        .filter_map(|label_id| ir.labels.get(label_id.0))
        .map(|label| label.text.clone())
        .collect()
}

fn assert_warning_expectation(case: &FixtureCase, warnings: &[String]) {
    if let Some(exact_count) = case.expected.warnings.exact_count {
        assert_eq!(
            warnings.len(),
            exact_count,
            "{}: warning count mismatch for {}",
            case.id,
            case.description
        );
    }

    for needle in &case.expected.warnings.contains {
        assert!(
            warnings.iter().any(|warning| warning.contains(needle)),
            "{}: expected warning containing '{needle}', got {:?}",
            case.id,
            warnings
        );
    }

    for needle in &case.expected.warnings.absent {
        assert!(
            warnings.iter().all(|warning| !warning.contains(needle)),
            "{}: warning unexpectedly contained '{needle}': {:?}",
            case.id,
            warnings
        );
    }
}

fn assert_node_expectations(case: &FixtureCase, ir: &fm_core::MermaidDiagramIr) {
    for expected in &case.expected.nodes {
        let node = node_by_id(&ir.nodes, &expected.id);

        for class_name in &expected.classes {
            assert!(
                node.classes.iter().any(|class| class == class_name),
                "{}: node '{}' missing class '{}' (actual: {:?})",
                case.id,
                expected.id,
                class_name,
                node.classes
            );
        }

        if let Some(href) = &expected.href {
            assert_eq!(
                node.href.as_deref(),
                Some(href.as_str()),
                "{}: node '{}' href mismatch",
                case.id,
                expected.id
            );
        }
        if let Some(callback) = &expected.callback {
            assert_eq!(
                node.callback.as_deref(),
                Some(callback.as_str()),
                "{}: node '{}' callback mismatch",
                case.id,
                expected.id
            );
        }
        if let Some(tooltip) = &expected.tooltip {
            assert_eq!(
                node.tooltip.as_deref(),
                Some(tooltip.as_str()),
                "{}: node '{}' tooltip mismatch",
                case.id,
                expected.id
            );
        }
    }
}

fn assert_xychart_expectation(case: &FixtureCase, ir: &fm_core::MermaidDiagramIr) {
    let Some(expected) = &case.expected.xychart else {
        return;
    };
    let meta = ir
        .xy_chart_meta
        .as_ref()
        .unwrap_or_else(|| panic!("{}: missing xy_chart_meta", case.id));

    assert_eq!(
        meta.title, expected.title,
        "{}: xychart title mismatch",
        case.id
    );
    assert_eq!(
        meta.x_axis.categories, expected.x_categories,
        "{}: xychart x-axis categories mismatch",
        case.id
    );
    assert_eq!(
        meta.y_axis.label, expected.y_label,
        "{}: xychart y-axis label mismatch",
        case.id
    );
    assert_eq!(
        meta.y_axis.min, expected.y_min,
        "{}: xychart y-axis min mismatch",
        case.id
    );
    assert_eq!(
        meta.y_axis.max, expected.y_max,
        "{}: xychart y-axis max mismatch",
        case.id
    );
    assert_eq!(
        meta.series.len(),
        expected.series.len(),
        "{}: xychart series count mismatch",
        case.id
    );

    for (actual, expected_series) in meta.series.iter().zip(&expected.series) {
        let expected_kind = match expected_series.kind.as_str() {
            "bar" => IrXySeriesKind::Bar,
            "line" => IrXySeriesKind::Line,
            "area" => IrXySeriesKind::Area,
            other => panic!("{}: unsupported expected series kind '{other}'", case.id),
        };
        assert_eq!(
            actual.kind, expected_kind,
            "{}: xychart series kind mismatch",
            case.id
        );
        assert_eq!(
            actual.name, expected_series.name,
            "{}: xychart series name mismatch",
            case.id
        );
        assert_eq!(
            actual.values, expected_series.values,
            "{}: xychart series values mismatch",
            case.id
        );
    }
}

#[test]
fn franken_tui_fixture_cases_match_parser_and_svg_expectations() {
    let cases = load_cases();
    assert!(!cases.is_empty(), "fixture manifest should not be empty");

    for case in cases {
        assert!(
            !case.source_refs.is_empty(),
            "{}: each fixture must cite at least one FrankenTUI reference",
            case.id
        );

        let input_path = test_root().join(&case.input_path);
        let input = fs::read_to_string(&input_path)
            .unwrap_or_else(|err| panic!("failed reading {}: {err}", input_path.display()));

        let parsed = parse(&input);
        let layout = layout_diagram(&parsed.ir);
        let svg_config = SvgRenderConfig {
            link_mode: MermaidLinkMode::Inline,
            ..SvgRenderConfig::default()
        };
        let svg = normalize_svg(&render_svg_with_layout(&parsed.ir, &layout, &svg_config));

        assert_eq!(
            parsed.ir.diagram_type.as_str(),
            case.expected.diagram_type,
            "{}: diagram type mismatch",
            case.id
        );
        assert_warning_expectation(&case, &parsed.warnings);

        assert_eq!(
            parsed.ir.nodes.len(),
            case.expected.counts.nodes,
            "{}: node count mismatch",
            case.id
        );
        assert_eq!(
            parsed.ir.edges.len(),
            case.expected.counts.edges,
            "{}: edge count mismatch",
            case.id
        );
        if let Some(clusters) = case.expected.counts.clusters {
            assert_eq!(
                parsed.ir.clusters.len(),
                clusters,
                "{}: cluster count mismatch",
                case.id
            );
        }
        if let Some(subgraphs) = case.expected.counts.subgraphs {
            assert_eq!(
                parsed.ir.graph.subgraphs.len(),
                subgraphs,
                "{}: subgraph count mismatch",
                case.id
            );
        }

        assert_node_expectations(&case, &parsed.ir);
        assert_xychart_expectation(&case, &parsed.ir);

        if !case.expected.edge_labels.is_empty() {
            assert_eq!(
                edge_label_texts(&parsed.ir),
                case.expected.edge_labels,
                "{}: edge labels mismatch",
                case.id
            );
        }

        for needle in &case.expected.svg_contains {
            assert!(
                svg.contains(needle),
                "{}: rendered SVG missing substring '{needle}'",
                case.id
            );
        }
        for needle in &case.expected.svg_not_contains {
            assert!(
                !svg.contains(needle),
                "{}: rendered SVG unexpectedly contained substring '{needle}'",
                case.id
            );
        }
    }
}
