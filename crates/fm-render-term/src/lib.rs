#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

//! Terminal renderer for FrankenMermaid diagrams.
//!
//! This crate provides terminal-based rendering of `MermaidDiagramIr` diagrams with:
//!
//! - **Multi-tier fidelity**: Compact, Normal, and Rich rendering modes
//! - **Sub-cell canvas modes**: Braille (2x4), Block (2x2), HalfBlock (1x2), and CellOnly
//! - **Unicode and ASCII support**: Box-drawing characters with ASCII fallback
//! - **Diagram diffing**: Visual comparison of two diagrams with status highlighting
//! - **Minimap rendering**: Scaled overview with optional viewport indicator
//! - **ASCII detection**: Detect and normalize ASCII art diagrams in text
//!
//! # Quick Start
//!
//! ```rust
//! use fm_core::{DiagramType, MermaidDiagramIr};
//! use fm_render_term::{render_term, render_term_with_config, TermRenderConfig};
//!
//! // Create an IR (normally from fm-parser).
//! let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
//!
//! // Render with default settings.
//! let output = render_term(&ir);
//! println!("{}", output);
//!
//! // Render with custom configuration.
//! let config = TermRenderConfig::rich();
//! let result = render_term_with_config(&ir, &config, 120, 40);
//! println!("{}", result.output);
//! ```
//!
//! # Modules
//!
//! - [`canvas`]: Sub-cell pixel canvas for high-resolution terminal rendering
//! - [`config`]: Configuration types for rendering options
//! - [`glyphs`]: Unicode and ASCII box-drawing character sets
//! - [`renderer`]: Core diagram rendering logic
//! - [`diff`]: Diagram diffing and comparison
//! - [`minimap`]: Scaled overview rendering
//! - [`ascii`]: ASCII diagram detection and normalization

#![forbid(unsafe_code)]

pub mod ascii;
pub mod canvas;
pub mod config;
pub mod diff;
pub mod glyphs;
pub mod minimap;
pub mod renderer;
pub mod transform;

// Re-exports for convenient access.
pub use config::{ResolvedConfig, TermRenderConfig};
pub use diff::{
    DiagramDiff, DiffEdge, DiffNode, DiffStatus, diff_diagrams, render_diff_plain,
    render_diff_summary, render_diff_terminal, render_diff_terminal_with_config,
};
pub use glyphs::{BoxGlyphs, ClusterGlyphs, EdgeGlyphs, ShapeGlyphs};
pub use minimap::{
    MinimapConfig, MinimapCorner, MinimapDensity, MinimapDetailLevel, MinimapRect, MinimapResult,
    Viewport, minimap_cell_to_layout_point, render_minimap, render_minimap_ascii,
    render_minimap_colored, viewport_to_minimap_rect,
};
pub use renderer::{
    TermDebugMetrics, TermRenderResult, TermRenderer, render_diagram, render_diagram_with_config,
    render_diagram_with_layout_and_config,
};

use fm_core::MermaidDiagramIr;
use fm_layout::DiagramLayout;

/// Render a diagram to terminal output with default settings.
///
/// This is the main entry point for terminal rendering. For more control,
/// use [`render_term_with_config`] instead.
///
/// # Example
///
/// ```rust
/// use fm_core::{DiagramType, MermaidDiagramIr};
/// use fm_render_term::render_term;
///
/// let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
/// let output = render_term(&ir);
/// assert!(!output.is_empty());
/// ```
#[must_use]
pub fn render_term(ir: &MermaidDiagramIr) -> String {
    let result = render_diagram(ir);
    result.output
}

/// Render a diagram with custom configuration.
///
/// # Arguments
///
/// * `ir` - The diagram IR to render
/// * `config` - Rendering configuration
/// * `cols` - Available terminal columns
/// * `rows` - Available terminal rows
///
/// # Example
///
/// ```rust
/// use fm_core::{DiagramType, MermaidDiagramIr};
/// use fm_render_term::{render_term_with_config, TermRenderConfig};
///
/// let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
/// let config = TermRenderConfig::compact();
/// let result = render_term_with_config(&ir, &config, 80, 24);
/// assert!(result.width <= 80);
/// ```
#[must_use]
pub fn render_term_with_config(
    ir: &MermaidDiagramIr,
    config: &TermRenderConfig,
    cols: usize,
    rows: usize,
) -> TermRenderResult {
    render_diagram_with_config(ir, config, cols, rows)
}

/// Render a diagram with custom configuration using a pre-computed layout.
#[must_use]
pub fn render_term_with_layout_and_config(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &TermRenderConfig,
    cols: usize,
    rows: usize,
) -> TermRenderResult {
    renderer::render_diagram_with_layout_and_config(ir, layout, config, cols, rows)
}

/// Get layout statistics for a diagram without full rendering.
///
/// Useful for quick metrics when full rendering is not needed.
#[must_use]
pub fn term_stats(ir: &MermaidDiagramIr) -> (usize, usize) {
    (ir.nodes.len(), ir.edges.len())
}

/// Render a diff between two diagrams.
///
/// Returns a colored diff summary showing added, removed, and changed elements.
///
/// # Example
///
/// ```rust
/// use fm_core::{DiagramType, MermaidDiagramIr};
/// use fm_render_term::render_diff;
///
/// let old = MermaidDiagramIr::empty(DiagramType::Flowchart);
/// let new = MermaidDiagramIr::empty(DiagramType::Flowchart);
/// let diff_output = render_diff(&old, &new, true);
/// ```
#[must_use]
pub fn render_diff(old: &MermaidDiagramIr, new: &MermaidDiagramIr, use_colors: bool) -> String {
    let diff = diff_diagrams(old, new);
    if use_colors {
        render_diff_terminal(old, new, 120, 40, true)
    } else {
        render_diff_plain(&diff)
    }
}

/// Render a minimap of a diagram.
///
/// # Example
///
/// ```rust
/// use fm_core::{DiagramType, MermaidDiagramIr};
/// use fm_render_term::{render_minimap_simple, MinimapConfig};
///
/// let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
/// let output = render_minimap_simple(&ir, 20, 10);
/// ```
#[must_use]
pub fn render_minimap_simple(ir: &MermaidDiagramIr, max_width: usize, max_height: usize) -> String {
    let config = MinimapConfig {
        max_width,
        max_height,
        ..Default::default()
    };
    let result = render_minimap(ir, &config);
    result.output
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{
        ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode,
        IrNodeId,
    };
    use proptest::prelude::*;

    fn sample_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        ir.labels.push(IrLabel {
            text: "Start".to_string(),
            ..Default::default()
        });
        ir.labels.push(IrLabel {
            text: "End".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: Some(IrLabelId(0)),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: Some(IrLabelId(1)),
            ..Default::default()
        });
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });
        ir
    }

    fn linear_ir(node_count: usize) -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        for index in 0..node_count {
            ir.labels.push(IrLabel {
                text: format!("Node {index}"),
                ..Default::default()
            });
            ir.nodes.push(IrNode {
                id: format!("N{index}"),
                label: Some(IrLabelId(index)),
                ..Default::default()
            });
        }
        for index in 1..node_count {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(index - 1)),
                to: IrEndpoint::Node(IrNodeId(index)),
                arrow: ArrowType::Arrow,
                ..Default::default()
            });
        }
        ir
    }

    #[test]
    fn render_term_produces_output() {
        let ir = sample_ir();
        let output = render_term(&ir);
        assert!(!output.is_empty());
    }

    #[test]
    fn render_with_config_respects_dimensions() {
        let ir = sample_ir();
        let config = TermRenderConfig::compact();
        let result = render_term_with_config(&ir, &config, 60, 20);
        assert!(result.width <= 60);
        assert!(result.height <= 20);
    }

    #[test]
    fn term_stats_returns_counts() {
        let ir = sample_ir();
        let (nodes, edges) = term_stats(&ir);
        assert_eq!(nodes, 2);
        assert_eq!(edges, 1);
    }

    #[test]
    fn render_diff_produces_summary() {
        let old = sample_ir();
        let new = sample_ir();
        let output = render_diff(&old, &new, false);
        assert!(output.contains("Diagram Diff Summary"));
    }

    #[test]
    fn minimap_simple_produces_output() {
        let ir = sample_ir();
        let _output = render_minimap_simple(&ir, 15, 8);
        // May be empty for very small diagrams, but should not panic.
    }

    #[test]
    fn empty_diagram_renders_without_panic() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let _output = render_term(&ir);
        // Empty diagram should produce minimal output without panicking.
    }

    #[test]
    fn rich_config_produces_larger_output() {
        let ir = sample_ir();
        let compact = render_term_with_config(&ir, &TermRenderConfig::compact(), 80, 24);
        let rich = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60);

        // Rich should generally produce more detailed output.
        assert!(rich.width >= compact.width || rich.height >= compact.height);
    }

    #[test]
    fn outline_degradation_is_a_real_textless_skeleton() {
        let ir = sample_ir();
        let mut config = TermRenderConfig::rich();
        config.apply_degradation(&fm_core::MermaidDegradationPlan {
            target_fidelity: fm_core::MermaidFidelity::Outline,
            ..fm_core::MermaidDegradationPlan::default()
        });

        assert!(!config.show_labels);
        let result = render_term_with_config(&ir, &config, 80, 24);

        assert!(!result.output.is_empty(), "outline must retain structural geometry");
        assert!(!result.output.contains("Start"));
        assert!(!result.output.contains("End"));
        assert!(result.width <= 80);
        assert!(result.height <= 24);
    }

    #[test]
    fn outline_suppresses_specialized_pie_title_and_legend_but_keeps_geometry() {
        let ir = fm_parser::parse(
            "pie title Snacks\n  \"Cats\" : 70\n  \"Dogs\" : 30\n",
        )
        .ir;
        let mut config = TermRenderConfig::rich();
        config.apply_degradation(&fm_core::MermaidDegradationPlan {
            target_fidelity: fm_core::MermaidFidelity::Outline,
            ..fm_core::MermaidDegradationPlan::default()
        });
        let result = render_term_with_config(&ir, &config, 80, 24);

        assert!(!result.output.is_empty(), "pie outline must retain wedge geometry");
        for text in ["Snacks", "Cats", "Dogs"] {
            assert!(!result.output.contains(text), "outline leaked label {text:?}");
        }
        assert!(result.output.chars().any(|ch| "#*@+=~%&".contains(ch)));
    }

    #[test]
    fn hide_labels_degradation_does_not_change_manual_label_limits() {
        let mut config = TermRenderConfig::rich();
        let limits = (config.max_label_chars, config.max_label_lines);
        config.apply_degradation(&fm_core::MermaidDegradationPlan {
            hide_labels: true,
            ..fm_core::MermaidDegradationPlan::default()
        });
        assert!(!config.show_labels);
        assert_eq!((config.max_label_chars, config.max_label_lines), limits);
    }

    #[test]
    fn debug_overlay_is_opt_in_and_preserves_render_bounds() {
        let ir = sample_ir();
        let plain = render_term_with_config(&ir, &TermRenderConfig::default(), 80, 24);
        assert!(plain.debug_metrics.is_none());

        let config = TermRenderConfig {
            debug_overlay: true,
            ..TermRenderConfig::default()
        };
        let debug = render_term_with_config(&ir, &config, 80, 24);
        let metrics = debug.debug_metrics.expect("debug metrics must be exposed when enabled");

        assert_eq!(debug.width, plain.width);
        assert_eq!(debug.height, plain.height);
        assert!(debug.width <= 80);
        assert!(debug.height <= 24);
        assert!(debug.output.contains("[layout debug]"));
        assert!(debug.output.contains("crossings="));
        assert!(debug.output.contains("bends="));
        assert!(debug.output.contains("symmetry="));
        assert!(!debug.output.contains('\u{1b}'));
        assert!(metrics.symmetry_permille <= 1000);
    }

    #[test]
    fn debug_overlay_reports_the_layout_stats_crossing_count() {
        let ir = sample_ir();
        let layout = fm_layout::layout_diagram(&ir);
        let config = TermRenderConfig {
            debug_overlay: true,
            ..TermRenderConfig::default()
        };
        let rendered = render_term_with_layout_and_config(&ir, &layout, &config, 80, 24);
        let metrics = rendered.debug_metrics.expect("debug metrics");
        assert_eq!(metrics.crossings, layout.stats.crossing_count);
        assert_eq!(
            metrics.bends,
            layout
                .edges
                .iter()
                .filter(|edge| !edge.bundled)
                .map(|edge| edge.points.len().saturating_sub(2))
                .sum::<usize>()
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(48))]

        #[test]
        fn prop_term_render_stays_within_requested_bounds(
            node_count in 0usize..30,
            cols in 40usize..180,
            rows in 12usize..80
        ) {
            let ir = linear_ir(node_count);
            let output = render_term_with_config(&ir, &TermRenderConfig::default(), cols, rows);

            prop_assert!(output.width <= cols);
            prop_assert!(output.height <= rows);
        }
    }
}
