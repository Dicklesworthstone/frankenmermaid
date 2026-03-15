//! Minimap rendering for diagram overview.
//!
//! Provides a scaled-down representation of the diagram with optional viewport indicator.

use fm_core::{MermaidDiagramIr, MermaidGlyphMode, MermaidRenderMode};
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
    /// Force ASCII fallback for border characters.
    pub glyph_mode: MermaidGlyphMode,
    /// Apply ANSI colors to the output.
    pub use_color: bool,
    /// Detail level selection strategy.
    pub detail_level: MinimapDetailLevel,
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
            glyph_mode: MermaidGlyphMode::Unicode,
            use_color: false,
            detail_level: MinimapDetailLevel::Auto,
        }
    }
}

/// Rendering detail level for minimap output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MinimapDetailLevel {
    /// Select detail level from node/edge density.
    #[default]
    Auto,
    /// Draw every node and edge aggressively.
    Full,
    /// Simplify dense diagrams while retaining node shapes and viewport.
    Balanced,
    /// Prefer a coarse overview for very dense diagrams.
    Sparse,
}

/// Density classification used to derive a detail level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimapDensity {
    Sparse,
    Medium,
    Dense,
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

impl Viewport {
    #[must_use]
    pub fn normalized(self) -> Self {
        Self {
            x: self.x.clamp(0.0, 1.0),
            y: self.y.clamp(0.0, 1.0),
            width: self.width.clamp(0.0, 1.0),
            height: self.height.clamp(0.0, 1.0),
        }
    }

    #[must_use]
    pub fn from_layout_rect(
        layout_bounds: &fm_layout::LayoutRect,
        rect: &fm_layout::LayoutRect,
    ) -> Self {
        let width = layout_bounds.width.max(1.0);
        let height = layout_bounds.height.max(1.0);
        Self {
            x: ((rect.x - layout_bounds.x) / width).clamp(0.0, 1.0),
            y: ((rect.y - layout_bounds.y) / height).clamp(0.0, 1.0),
            width: (rect.width / width).clamp(0.0, 1.0),
            height: (rect.height / height).clamp(0.0, 1.0),
        }
    }
}

/// Layout-space rectangle mapped into minimap cell coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MinimapRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
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
    /// Detail level used for this render.
    pub detail_level: MinimapDetailLevel,
    /// Density classification used during render.
    pub density: MinimapDensity,
}

/// Render a minimap of the diagram.
#[must_use]
pub fn render_minimap(ir: &MermaidDiagramIr, config: &MinimapConfig) -> MinimapResult {
    let layout = layout_diagram(ir);
    render_minimap_from_layout(&layout, config, None)
}

/// Render a minimap with an ASCII border and cell-only pixels.
#[must_use]
pub fn render_minimap_ascii(ir: &MermaidDiagramIr, config: &MinimapConfig) -> MinimapResult {
    let mut ascii = config.clone();
    ascii.render_mode = MermaidRenderMode::CellOnly;
    ascii.glyph_mode = MermaidGlyphMode::Ascii;
    render_minimap(ir, &ascii)
}

/// Render a minimap with ANSI colors applied to nodes, edges, and viewport.
#[must_use]
pub fn render_minimap_colored(
    ir: &MermaidDiagramIr,
    config: &MinimapConfig,
    viewport: Option<&Viewport>,
) -> MinimapResult {
    let mut colored = config.clone();
    colored.use_color = true;
    let layout = layout_diagram(ir);
    render_minimap_from_layout(&layout, &colored, viewport)
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
            detail_level: config.detail_level,
            density: MinimapDensity::Sparse,
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
    let density = classify_density(layout, pixel_width, pixel_height);
    let detail_level = resolve_detail_level(config.detail_level, density);

    // Create canvas.
    let mut canvas = Canvas::new(cell_width, cell_height, config.render_mode);

    // Offset to center diagram in canvas (reserved for future use).
    let _offset_x = (layout.bounds.x * scale_x) as isize;
    let _offset_y = (layout.bounds.y * scale_y) as isize;

    // Draw nodes as dots or small rectangles.
    for node_box in &layout.nodes {
        let x = ((node_box.bounds.x - layout.bounds.x) * scale_x) as usize;
        let y = ((node_box.bounds.y - layout.bounds.y) * scale_y) as usize;
        let w = ((node_box.bounds.width * scale_x) as usize).max(1);
        let h = ((node_box.bounds.height * scale_y) as usize).max(1);

        let draw_as_dot = matches!(detail_level, MinimapDetailLevel::Sparse) || (w <= 2 && h <= 2);
        if draw_as_dot {
            canvas.set_pixel(x, y);
        } else {
            canvas.fill_rect(x, y, w, h);
        }
    }

    // Draw edges as lines.
    for edge_path in &layout.edges {
        if matches!(detail_level, MinimapDetailLevel::Sparse) && edge_path.points.len() > 2 {
            if let (Some(first), Some(last)) = (edge_path.points.first(), edge_path.points.last()) {
                let x0 = ((first.x - layout.bounds.x) * scale_x) as isize;
                let y0 = ((first.y - layout.bounds.y) * scale_y) as isize;
                let x1 = ((last.x - layout.bounds.x) * scale_x) as isize;
                let y1 = ((last.y - layout.bounds.y) * scale_y) as isize;
                canvas.draw_line(x0, y0, x1, y1);
            }
            continue;
        }
        for window in edge_path.points.windows(2) {
            let x0 = ((window[0].x - layout.bounds.x) * scale_x) as isize;
            let y0 = ((window[0].y - layout.bounds.y) * scale_y) as isize;
            let x1 = ((window[1].x - layout.bounds.x) * scale_x) as isize;
            let y1 = ((window[1].y - layout.bounds.y) * scale_y) as isize;
            canvas.draw_line(x0, y0, x1, y1);
        }
    }

    // Draw viewport rectangle if enabled.
    if config.show_viewport
        && let Some(vp) = viewport
    {
        let vp = vp.normalized();
        let vp_x = (vp.x * pixel_width as f32) as usize;
        let vp_y = (vp.y * pixel_height as f32) as usize;
        let vp_w = (vp.width * pixel_width as f32) as usize;
        let vp_h = (vp.height * pixel_height as f32) as usize;
        canvas.draw_rect(vp_x, vp_y, vp_w.max(1), vp_h.max(1));
    }

    // Render canvas to string.
    let base_output = canvas.render();

    // Add border if configured.
    let output = if config.show_border {
        add_border(&base_output, cell_width, cell_height, config.glyph_mode)
    } else {
        base_output
    };
    let output = if config.use_color {
        colorize_output(&output)
    } else {
        output
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
        detail_level,
        density,
    }
}

#[must_use]
pub fn minimap_cell_to_layout_point(
    layout: &DiagramLayout,
    config: &MinimapConfig,
    cell_x: usize,
    cell_y: usize,
) -> (f32, f32) {
    let (cell_width, cell_height, _px_w, _px_h) = minimap_dimensions(layout, config);
    let content_x = cell_x.min(cell_width.saturating_sub(1)) as f32 / cell_width.max(1) as f32;
    let content_y = cell_y.min(cell_height.saturating_sub(1)) as f32 / cell_height.max(1) as f32;
    (
        layout.bounds.x + (layout.bounds.width * content_x),
        layout.bounds.y + (layout.bounds.height * content_y),
    )
}

#[must_use]
pub fn viewport_to_minimap_rect(
    layout: &DiagramLayout,
    config: &MinimapConfig,
    viewport: &Viewport,
) -> MinimapRect {
    let (cell_width, cell_height, _px_w, _px_h) = minimap_dimensions(layout, config);
    let vp = viewport.normalized();
    MinimapRect {
        x: (vp.x * cell_width as f32).floor() as usize,
        y: (vp.y * cell_height as f32).floor() as usize,
        width: ((vp.width * cell_width as f32).ceil() as usize).max(1),
        height: ((vp.height * cell_height as f32).ceil() as usize).max(1),
    }
}

#[must_use]
fn minimap_dimensions(
    layout: &DiagramLayout,
    config: &MinimapConfig,
) -> (usize, usize, usize, usize) {
    let diagram_width = layout.bounds.width.max(1.0);
    let diagram_height = layout.bounds.height.max(1.0);
    let aspect = diagram_width / diagram_height;
    let (cell_width, cell_height) = if aspect > 1.0 {
        let w = config.max_width;
        let h = ((w as f32 / aspect) as usize).max(2).min(config.max_height);
        (w, h)
    } else {
        let h = config.max_height;
        let w = ((h as f32 * aspect) as usize).max(2).min(config.max_width);
        (w, h)
    };
    let (mult_x, mult_y) = subcell_mult(config.render_mode);
    (
        cell_width,
        cell_height,
        cell_width * mult_x,
        cell_height * mult_y,
    )
}

fn classify_density(
    layout: &DiagramLayout,
    pixel_width: usize,
    pixel_height: usize,
) -> MinimapDensity {
    let pixel_budget = pixel_width.saturating_mul(pixel_height).max(1);
    let primitives = layout.nodes.len().saturating_mul(4) + layout.edges.len().saturating_mul(3);
    if primitives * 12 < pixel_budget {
        MinimapDensity::Sparse
    } else if primitives * 4 < pixel_budget {
        MinimapDensity::Medium
    } else {
        MinimapDensity::Dense
    }
}

fn resolve_detail_level(
    requested: MinimapDetailLevel,
    density: MinimapDensity,
) -> MinimapDetailLevel {
    match requested {
        MinimapDetailLevel::Auto => match density {
            MinimapDensity::Sparse => MinimapDetailLevel::Full,
            MinimapDensity::Medium => MinimapDetailLevel::Balanced,
            MinimapDensity::Dense => MinimapDetailLevel::Sparse,
        },
        other => other,
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

fn add_border(
    content: &str,
    content_width: usize,
    content_height: usize,
    glyph_mode: MermaidGlyphMode,
) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut output = String::new();

    // Top border.
    let (tl, tr, bl, br, hz, vt) = border_glyphs(glyph_mode);
    output.push(tl);
    for _ in 0..content_width {
        output.push(hz);
    }
    output.push(tr);
    output.push('\n');

    // Content lines with side borders.
    for i in 0..content_height {
        output.push(vt);
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
        output.push(vt);
        output.push('\n');
    }

    // Bottom border.
    output.push(bl);
    for _ in 0..content_width {
        output.push(hz);
    }
    output.push(br);

    output
}

fn border_glyphs(glyph_mode: MermaidGlyphMode) -> (char, char, char, char, char, char) {
    match glyph_mode {
        MermaidGlyphMode::Ascii => ('+', '+', '+', '+', '-', '|'),
        MermaidGlyphMode::Unicode => ('┌', '┐', '└', '┘', '─', '│'),
    }
}

fn colorize_output(output: &str) -> String {
    output
        .chars()
        .map(|ch| match ch {
            '┌' | '┐' | '└' | '┘' | '─' | '│' | '+' | '-' | '|' => {
                format!("\x1b[90m{ch}\x1b[0m")
            }
            '█' | '▀' | '▄' | '⣿' | '⣀'..='⣿' => format!("\x1b[36m{ch}\x1b[0m"),
            _ => ch.to_string(),
        })
        .collect()
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
    let minimap_lines: Vec<Vec<char>> = minimap
        .output
        .lines()
        .map(|l| l.chars().collect())
        .collect();

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
    use fm_core::{ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrNode, IrNodeId};

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
        let config_no_border = MinimapConfig {
            show_border: false,
            ..Default::default()
        };
        let no_border = render_minimap(&ir, &config_no_border);

        let config_with_border = MinimapConfig {
            show_border: true,
            ..Default::default()
        };
        let with_border = render_minimap(&ir, &config_with_border);

        assert_eq!(with_border.width, no_border.width + 2);
        assert_eq!(with_border.height, no_border.height + 2);
    }

    #[test]
    fn ascii_minimap_uses_ascii_border() {
        let ir = sample_ir();
        let config = MinimapConfig {
            glyph_mode: MermaidGlyphMode::Ascii,
            ..Default::default()
        };
        let result = render_minimap_ascii(&ir, &config);
        assert!(result.output.contains('+'));
    }

    #[test]
    fn colored_minimap_emits_ansi_sequences() {
        let ir = sample_ir();
        let config = MinimapConfig::default();
        let result = render_minimap_colored(&ir, &config, None);
        assert!(result.output.contains("\x1b["));
    }

    #[test]
    fn viewport_mapping_returns_nonzero_rect() {
        let ir = sample_ir();
        let layout = layout_diagram(&ir);
        let config = MinimapConfig::default();
        let viewport = Viewport {
            x: 0.25,
            y: 0.25,
            width: 0.5,
            height: 0.5,
        };
        let rect = viewport_to_minimap_rect(&layout, &config, &viewport);
        assert!(rect.width >= 1);
        assert!(rect.height >= 1);
    }

    #[test]
    fn minimap_cell_maps_back_into_layout_bounds() {
        let ir = sample_ir();
        let layout = layout_diagram(&ir);
        let config = MinimapConfig::default();
        let (x, y) = minimap_cell_to_layout_point(&layout, &config, 0, 0);
        assert!(x >= layout.bounds.x);
        assert!(y >= layout.bounds.y);
        assert!(x <= layout.bounds.x + layout.bounds.width);
        assert!(y <= layout.bounds.y + layout.bounds.height);
    }

    #[test]
    fn viewport_from_layout_rect_normalizes_into_unit_space() {
        let rect = fm_layout::LayoutRect {
            x: 25.0,
            y: 10.0,
            width: 50.0,
            height: 20.0,
        };
        let bounds = fm_layout::LayoutRect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 40.0,
        };
        let viewport = Viewport::from_layout_rect(&bounds, &rect);
        assert_eq!(viewport.x, 0.25);
        assert_eq!(viewport.y, 0.25);
        assert_eq!(viewport.width, 0.5);
        assert_eq!(viewport.height, 0.5);
    }
}
