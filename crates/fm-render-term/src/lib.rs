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
pub use ascii::{
    CharClass, DiagramBlock, LineClass, align_right_border, classify_char, classify_line,
    detect_diagram_blocks, normalize_box_drawing,
};
pub use config::{
    ResolvedConfig, SelectionState, TermRenderConfig, build_adjacency, navigate_direction,
    select_fidelity,
};
pub use diff::{
    DiagramDiff, DiffEdge, DiffNode, DiffStatus, diff_diagrams, render_diff_plain,
    render_diff_summary, render_diff_terminal, render_diff_terminal_with_config,
};
pub use glyphs::{BoxGlyphs, ClusterGlyphs, EdgeGlyphs, ShapeGlyphs};
pub use minimap::{
    MinimapConfig, MinimapCorner, MinimapDensity, MinimapDetailLevel, MinimapRect, MinimapResult,
    Viewport, minimap_cell_to_layout_point, overlay_minimap, render_minimap, render_minimap_ascii,
    render_minimap_colored, render_minimap_from_layout, render_minimap_with_viewport,
    viewport_to_minimap_rect,
};
pub use renderer::{
    TermRenderResult, TermRenderer, render_diagram, render_diagram_with_config,
    render_diagram_with_layout_and_config,
};
pub use transform::TermTransform;

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

/// Render a terminal diagram and overlay a minimap with viewport indicator (`bd-z8971`).
#[must_use]
pub fn render_term_with_minimap(
    ir: &MermaidDiagramIr,
    config: &TermRenderConfig,
    minimap_config: &MinimapConfig,
    visible_rect: Option<&fm_layout::LayoutRect>,
    cols: usize,
    rows: usize,
) -> TermRenderResult {
    let layout = fm_layout::layout_diagram(ir);
    let mut main_result = render_term_with_layout_and_config(ir, &layout, config, cols, rows);
    let viewport = visible_rect
        .map(|rect| Viewport::from_layout_rect(&layout.bounds, rect))
        .unwrap_or_default()
        .normalized();
    let minimap = render_minimap_from_layout(&layout, minimap_config, Some(&viewport));
    if minimap.width > 0
        && minimap.height > 0
        && main_result.width >= minimap.width + 2
        && main_result.height >= minimap.height + 2
    {
        main_result.output = overlay_minimap(
            &main_result.output,
            &minimap,
            main_result.width,
            main_result.height,
            minimap_config.corner,
        );
    }
    main_result
}

/// Return `(has_changes, total_changes)` for the structural and semantic diff between `old` and `new` (`bd-q255e`).
#[inline]
#[must_use]
pub fn diff_change_counts(old: &MermaidDiagramIr, new: &MermaidDiagramIr) -> (bool, usize) {
    let diff = diff_diagrams(old, new);
    (diff.has_changes(), diff.total_changes())
}

/// Resolve the effective terminal tier, 4-level fidelity, and sub-cell render mode for a layout (`bd-q255e`).
#[inline]
#[must_use]
pub fn resolve_term_tier_and_fidelity(
    config: &TermRenderConfig,
    layout: &DiagramLayout,
    cols: usize,
    rows: usize,
) -> (
    fm_core::MermaidTier,
    fm_core::MermaidFidelity,
    fm_core::MermaidRenderMode,
) {
    let tier = config.effective_tier(cols, rows);
    let mode = config.effective_render_mode(tier);
    let w = layout.bounds.width.max(1.0);
    let h = layout.bounds.height.max(1.0);
    let scale_x = cols as f32 / w;
    let scale_y = rows as f32 / h;
    let node_area: f32 = layout
        .nodes
        .iter()
        .map(|n| n.bounds.width * n.bounds.height)
        .sum();
    let density = (node_area / (w * h)).clamp(0.0, 1.0);
    let fidelity =
        config.effective_fidelity(scale_x, scale_y, density, layout.nodes.len(), cols, rows);
    (tier, fidelity, mode)
}

/// Advance or rewind interactive terminal node selection in index order (`bd-q255e`).
#[inline]
pub fn step_term_selection(selection: &mut SelectionState, ir: &MermaidDiagramIr, forward: bool) {
    let adj = build_adjacency(ir);
    if forward {
        selection.select_next(ir.nodes.len(), &adj);
    } else {
        selection.select_prev(ir.nodes.len(), &adj);
    }
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
    fn term_transform_and_minimap_overlay_integrate_cleanly() {
        let ir = sample_ir();
        let layout = fm_layout::layout_diagram(&ir);
        let config = TermRenderConfig::rich();
        let resolved = ResolvedConfig::resolve(&config, 120, 40);
        let renderer = TermRenderer::new(resolved);
        let transform = renderer
            .layout_transform(&layout, ir.direction)
            .expect("valid layout transform");
        assert!(transform.uniform_scale() > 0.0);
        assert!(transform.aspect() > 0.0);

        let first_center = layout.nodes[0].bounds.center();
        let (col, row) = renderer.map_layout_point_to_cells(&layout, ir.direction, first_center);
        assert!(col > 0 || row > 0);

        let minimap_config = MinimapConfig {
            max_width: 12,
            max_height: 6,
            ..Default::default()
        };
        let with_minimap = render_term_with_minimap(
            &ir,
            &config,
            &minimap_config,
            Some(&layout.nodes[0].bounds),
            120,
            40,
        );
        assert!(!with_minimap.output.is_empty());

        assert!(!transform.is_pure_rotor());
        assert!(
            (transform.rotor_part().to_affine_matrix().a - transform.uniform_scale()).abs() < 1e-6
        );

        let (tier, fidelity, mode) = resolve_term_tier_and_fidelity(&config, &layout, 120, 40);
        assert_eq!(tier, fm_core::MermaidTier::Rich);
        assert_eq!(fidelity, fm_core::MermaidFidelity::Rich);
        assert_eq!(mode, fm_core::MermaidRenderMode::Braille);

        let mut sel = SelectionState::default();
        step_term_selection(&mut sel, &ir, true);
        assert_eq!(sel.selected_node, Some(0));
        step_term_selection(&mut sel, &ir, false);
        assert_eq!(sel.selected_node, Some(1));

        let (has_changes, total_changes) = diff_change_counts(&ir, &linear_ir(3));
        assert!(has_changes);
        assert!(total_changes > 0);
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
