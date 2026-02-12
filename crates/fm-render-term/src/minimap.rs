//! Minimap rendering for diagram overview.
//!
//! Provides a scaled-down representation of the diagram with optional viewport indicator.

use fm_core::{MermaidDiagramIr, MermaidRenderMode};
use fm_layout::{DiagramLayout, layout_diagram};

use crate::canvas::Canvas;

/// Corner placement for the minimap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MinimapCorner {
    TopLeft,
    #[default]
    TopRight,
    BottomLeft,
    BottomRight,
}

/// Configuration for minimap rendering.
#[derive(Debug, Clone)]
pub struct MinimapConfig {
    /// Maximum width in terminal cells.
    pub max_width: usize,
    /// Maximum height in terminal cells.
    pub max_height: usize,
    /// Render mode (defaults to Braille for highest density).
    pub render_mode: MermaidRenderMode,
    /// Show viewport rectangle.
    pub show_viewport: bool,
    /// Corner placement.
    pub corner: MinimapCorner,
    /// Border around the minimap.
    pub show_border: bool,
}

impl Default for MinimapConfig {
    fn default() -> Self {
        Self {
            max_width: 20,
            max_height: 10,
            render_mode: MermaidRenderMode::Braille,
            show_viewport: true,
            corner: MinimapCorner::TopRight,
            show_border: true,
        }
    }
}

/// Viewport rectangle for showing current view area.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    /// X offset into the full diagram.
    pub x: f32,
    /// Y offset into the full diagram.
    pub y: f32,
    /// Width of the visible area.
    pub width: f32,
    /// Height of the visible area.
    pub height: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            width: 1.0,
            height: 1.0,
        }
    }
}

/// Result of minimap rendering.
#[derive(Debug, Clone)]
pub struct MinimapResult {
    /// Rendered minimap string.
    pub output: String,
    /// Actual width in cells.
    pub width: usize,
    /// Actual height in cells.
    pub height: usize,
    /// Scale factor applied.
    pub scale: f32,
}

/// Render a minimap of the diagram.
#[must_use]
pub fn render_minimap(ir: &MermaidDiagramIr, config: &MinimapConfig) -> MinimapResult {
    let layout = layout_diagram(ir);
    render_minimap_from_layout(&layout, config, None)
}

/// Render a minimap with optional viewport indicator.
#[must_use]
pub fn render_minimap_with_viewport(
    ir: &MermaidDiagramIr,
    config: &MinimapConfig,
    viewport: &Viewport,
) -> MinimapResult {
    let layout = layout_diagram(ir);
    render_minimap_from_layout(&layout, config, Some(viewport))
}

/// Render a minimap from a pre-computed layout.
#[must_use]
pub fn render_minimap_from_layout(
    layout: &DiagramLayout,
    config: &MinimapConfig,
    viewport: Option<&Viewport>,
) -> MinimapResult {
    if layout.nodes.is_empty() {
        return MinimapResult {
            output: String::new(),
            width: 0,
            height: 0,
            scale: 1.0,
        };
    }

    // Calculate aspect-preserving dimensions.
    let diagram_width = layout.bounds.width.max(1.0);
    let diagram_height = layout.bounds.height.max(1.0);
    let aspect = diagram_width / diagram_height;

    let (mult_x, mult_y) = subcell_mult(config.render_mode);

    // Determine cell dimensions preserving aspect ratio.
    let (cell_width, cell_height) = if aspect > 1.0 {
        // Wider than tall.
        let w = config.max_width;
        let h = ((w as f32 / aspect) as usize).max(2).min(config.max_height);
        (w, h)
    } else {
        // Taller than wide.
        let h = config.max_height;
        let w = ((h as f32 * aspect) as usize).max(2).min(config.max_width);
        (w, h)
    };

    let pixel_width = cell_width * mult_x;
    let pixel_height = cell_height * mult_y;

    // Scale factors.
    let scale_x = pixel_width as f32 / diagram_width;
    let scale_y = pixel_height as f32 / diagram_height;
    let scale = scale_x.min(scale_y);

    // Create canvas.
    let mut canvas = Canvas::new(cell_width, cell_height, config.render_mode);

    // Offset to center diagram in canvas.
    let offset_x = (layout.bounds.x * scale_x) as isize;
    let offset_y = (layout.bounds.y * scale_y) as isize;

    // Draw nodes as dots or small rectangles.
    for node_box in &layout.nodes {
        let x = ((node_box.bounds.x - layout.bounds.x) * scale_x) as usize;
        let y = ((node_box.bounds.y - layout.bounds.y) * scale_y) as usize;
        let w = ((node_box.bounds.width * scale_x) as usize).max(1);
        let h = ((node_box.bounds.height * scale_y) as usize).max(1);

        // For very small representations, just set a pixel.
        if w <= 2 && h <= 2 {
            canvas.set_pixel(x, y);
        } else {
            canvas.fill_rect(x, y, w, h);
        }
    }

    // Draw edges as lines.
    for edge_path in &layout.edges {
        for window in edge_path.points.windows(2) {
            let x0 = ((window[0].x - layout.bounds.x) * scale_x) as isize;
            let y0 = ((window[0].y - layout.bounds.y) * scale_y) as isize;
            let x1 = ((window[1].x - layout.bounds.x) * scale_x) as isize;
            let y1 = ((window[1].y - layout.bounds.y) * scale_y) as isize;
            canvas.draw_line(x0, y0, x1, y1);
        }
    }

    // Draw viewport rectangle if enabled.
    if config.show_viewport {
        if let Some(vp) = viewport {
            let vp_x = (vp.x * pixel_width as f32) as usize;
            let vp_y = (vp.y * pixel_height as f32) as usize;
            let vp_w = (vp.width * pixel_width as f32) as usize;
            let vp_h = (vp.height * pixel_height as f32) as usize;
            canvas.draw_rect(vp_x, vp_y, vp_w.max(1), vp_h.max(1));
        }
    }

    // Render canvas to string.
    let base_output = canvas.render();

    // Add border if configured.
    let output = if config.show_border {
        add_border(&base_output, cell_width, cell_height)
    } else {
        base_output
    };

    MinimapResult {
        output,
        width: if config.show_border {
            cell_width + 2
        } else {
            cell_width
        },
        height: if config.show_border {
            cell_height + 2
        } else {
            cell_height
        },
        scale,
    }
}

fn subcell_mult(mode: MermaidRenderMode) -> (usize, usize) {
    match mode {
        MermaidRenderMode::Braille => (2, 4),
        MermaidRenderMode::Block => (2, 2),
        MermaidRenderMode::HalfBlock => (1, 2),
        MermaidRenderMode::CellOnly | MermaidRenderMode::Auto => (1, 1),
    }
}

fn add_border(content: &str, content_width: usize, content_height: usize) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut output = String::new();

    // Top border.
    output.push('┌');
    for _ in 0..content_width {
        output.push('─');
    }
    output.push('┐');
    output.push('\n');

    // Content lines with side borders.
    for i in 0..content_height {
        output.push('│');
        if let Some(line) = lines.get(i) {
            output.push_str(line);
            // Pad to content_width.
            let line_len = line.chars().count();
            for _ in line_len..content_width {
                output.push(' ');
            }
        } else {
            for _ in 0..content_width {
                output.push(' ');
            }
        }
        output.push('│');
        output.push('\n');
    }

    // Bottom border.
    output.push('└');
    for _ in 0..content_width {
        output.push('─');
    }
    output.push('┘');

    output
}

/// Overlay minimap onto a main diagram rendering at the specified corner.
#[must_use]
pub fn overlay_minimap(
    main_output: &str,
    minimap: &MinimapResult,
    main_width: usize,
    main_height: usize,
    corner: MinimapCorner,
) -> String {
    let main_lines: Vec<Vec<char>> = main_output.lines().map(|l| l.chars().collect()).collect();
    let minimap_lines: Vec<Vec<char>> = minimap.output.lines().map(|l| l.chars().collect()).collect();

    // Calculate placement.
    let (start_x, start_y) = match corner {
        MinimapCorner::TopLeft => (1, 1),
        MinimapCorner::TopRight => (main_width.saturating_sub(minimap.width + 1), 1),
        MinimapCorner::BottomLeft => (1, main_height.saturating_sub(minimap.height + 1)),
        MinimapCorner::BottomRight => (
            main_width.saturating_sub(minimap.width + 1),
            main_height.saturating_sub(minimap.height + 1),
        ),
    };

    // Build output.
    let mut result: Vec<Vec<char>> = Vec::with_capacity(main_height);
    for y in 0..main_height {
        let mut row: Vec<char> = main_lines
            .get(y)
            .cloned()
            .unwrap_or_else(|| vec![' '; main_width]);

        // Pad row to main_width.
        while row.len() < main_width {
            row.push(' ');
        }

        result.push(row);
    }

    // Overlay minimap.
    for (my, minimap_row) in minimap_lines.iter().enumerate() {
        let y = start_y + my;
        if y >= result.len() {
            continue;
        }
        for (mx, ch) in minimap_row.iter().enumerate() {
            let x = start_x + mx;
            if x < result[y].len() {
                result[y][x] = *ch;
            }
        }
    }

    result
        .into_iter()
        .map(|row| row.into_iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{
        ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrNode, IrNodeId,
    };

    fn sample_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
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

    #[test]
    fn renders_minimap() {
        let ir = sample_ir();
        let config = MinimapConfig::default();
        let result = render_minimap(&ir, &config);
        assert!(!result.output.is_empty());
        assert!(result.width > 0);
        assert!(result.height > 0);
    }

    #[test]
    fn minimap_with_viewport() {
        let ir = sample_ir();
        let config = MinimapConfig::default();
        let viewport = Viewport {
            x: 0.2,
            y: 0.2,
            width: 0.5,
            height: 0.5,
        };
        let result = render_minimap_with_viewport(&ir, &config, &viewport);
        assert!(!result.output.is_empty());
    }

    #[test]
    fn empty_diagram_returns_empty_minimap() {
        let ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let config = MinimapConfig::default();
        let result = render_minimap(&ir, &config);
        assert!(result.output.is_empty());
    }

    #[test]
    fn border_increases_dimensions() {
        let ir = sample_ir();
        let mut config = MinimapConfig::default();
        config.show_border = false;
        let no_border = render_minimap(&ir, &config);

        config.show_border = true;
        let with_border = render_minimap(&ir, &config);

        assert_eq!(with_border.width, no_border.width + 2);
        assert_eq!(with_border.height, no_border.height + 2);
    }
}
