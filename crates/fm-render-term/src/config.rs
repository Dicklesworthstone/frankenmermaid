//! Terminal renderer configuration types.

use fm_core::{
    IrEndpoint, MermaidDiagramIr, MermaidFidelity, MermaidGlyphMode, MermaidLinkMode,
    MermaidRenderMode, MermaidTier,
};
use fm_layout::DiagramLayout;

/// Configuration for terminal diagram rendering.
#[derive(Debug, Clone)]
/// ⚠️ THERE IS NO COLOUR OPTION HERE, AND THAT IS THE ANSWER TO "why does the terminal ignore user
/// styling" (bd-lvj3). The canvas backend was fixed to honour `style`, `classDef` and `inline_style`
/// by resolving them to a stroke/fill it already had; the terminal has no such channel to resolve
/// them INTO. Measured: `renderer.rs` emits no ANSI anywhere — its only `\u{1b}` is the INPUT of
/// `strips_terminal_control_characters_from_labels`, which sanitises user-supplied escapes and says
/// nothing about the renderer's own output — and the sole ANSI emitter in this crate is the minimap.
///
/// So the terminal half of bd-lvj3 is NOT a small follow-up to the canvas fix. It is a new
/// capability: colour output, which must be OPT-IN because `-f term` is routinely piped to a file
/// and escape bytes there are corruption, not colour. `MinimapConfig::use_color` (default `false`)
/// is the precedent to copy if anyone builds it.
///
/// The one `classes` reader in this crate, `is_block_beta_space_node`, matches a STRUCTURAL marker
/// class, not a style — so "the terminal reads `classes` only" describes a layout probe, not partial
/// styling support.
pub struct TermRenderConfig {
    /// Rendering fidelity tier (Compact/Normal/Rich/Auto).
    pub tier: MermaidTier,
    /// Optional explicit 4-tier fidelity override (`Outline`/`Compact`/`Normal`/`Rich`).
    pub fidelity_override: Option<MermaidFidelity>,
    /// Sub-cell rendering mode (CellOnly/Braille/Block/HalfBlock/Auto).
    pub render_mode: MermaidRenderMode,
    /// Glyph mode (Unicode box-drawing vs ASCII fallback).
    pub glyph_mode: MermaidGlyphMode,
    /// Maximum width in terminal columns.
    pub max_width: usize,
    /// Maximum height in terminal rows.
    pub max_height: usize,
    /// Maximum label characters before truncation.
    pub max_label_chars: usize,
    /// Maximum label lines before truncation.
    pub max_label_lines: usize,
    /// Show selection highlighting.
    pub show_selection: bool,
    /// Currently selected node index (into `ir.nodes`) for interactive highlighting.
    pub selected_node: Option<usize>,
    /// Secondary highlighted node indices (e.g., neighbors of `selected_node`).
    pub highlighted_nodes: Vec<usize>,
    /// Link rendering mode (`Off`, `Inline`, `Footnote`). When `Off`, falls back to `ir.meta.init.config.link_mode`.
    pub link_mode: MermaidLinkMode,
    /// Show cluster decorations.
    pub show_clusters: bool,
    /// Enable diagonal edge optimization.
    pub diagonal_edges: bool,
    /// Padding around the diagram (in cells).
    pub padding: usize,
    /// Overlay a minimap onto the rendered terminal output.
    pub show_minimap: bool,
    /// Today's date, as `YYYY-MM-DD`, for the gantt today marker. `None` draws no marker.
    ///
    /// SUPPLIED, NEVER READ FROM THE CLOCK, exactly as `SvgRenderConfig::gantt_today` and
    /// `CanvasRenderConfig::gantt_today` are. Output that is a function of wall time is a defect
    /// class this repo has already been bitten by three times; a renderer calling `now()` would make
    /// every terminal snapshot depend on the day it ran. The default is `None`, so nothing about an
    /// existing render changes; the CLI injects the real date.
    pub gantt_today: Option<String>,
    /// Whether to append a diagnostic debug overlay banner (`[debug: nodes=... edges=...]`)
    /// showing layout statistics (`bd-0l1x2`).
    pub debug_overlay: bool,
}

impl Default for TermRenderConfig {
    fn default() -> Self {
        Self {
            tier: MermaidTier::Auto,
            fidelity_override: None,
            render_mode: MermaidRenderMode::Braille,
            glyph_mode: MermaidGlyphMode::Unicode,
            max_width: 120,
            max_height: 40,
            max_label_chars: 24,
            max_label_lines: 2,
            show_selection: false,
            selected_node: None,
            highlighted_nodes: Vec::new(),
            link_mode: MermaidLinkMode::Off,
            show_clusters: true,
            diagonal_edges: true,
            padding: 1,
            show_minimap: false,
            gantt_today: None,
            debug_overlay: false,
        }
    }
}

impl TermRenderConfig {
    /// Create a compact configuration for small terminals.
    #[must_use]
    pub fn compact() -> Self {
        Self {
            tier: MermaidTier::Compact,
            render_mode: MermaidRenderMode::CellOnly,
            max_width: 80,
            max_height: 24,
            max_label_chars: 12,
            max_label_lines: 1,
            show_clusters: false,
            diagonal_edges: false,
            ..Self::default()
        }
    }

    /// Create an outline configuration for ultra-constrained viewports (`<16x6` or skeleton mode).
    #[must_use]
    pub fn outline() -> Self {
        Self {
            tier: MermaidTier::Compact,
            fidelity_override: Some(MermaidFidelity::Outline),
            render_mode: MermaidRenderMode::CellOnly,
            max_width: 40,
            max_height: 12,
            max_label_chars: 8,
            max_label_lines: 1,
            show_clusters: false,
            diagonal_edges: false,
            ..Self::default()
        }
    }

    /// Create a rich configuration for large high-resolution terminals.
    #[must_use]
    pub fn rich() -> Self {
        Self {
            tier: MermaidTier::Rich,
            render_mode: MermaidRenderMode::Braille,
            max_width: 200,
            max_height: 60,
            max_label_chars: 48,
            max_label_lines: 3,
            ..Self::default()
        }
    }

    /// Synchronize selection highlighting from an interactive [`SelectionState`].
    pub fn with_selection(mut self, selection: &SelectionState) -> Self {
        self.show_selection = selection.selected_node.is_some();
        self.selected_node = selection.selected_node;
        self.highlighted_nodes = selection.highlighted_nodes.clone();
        self
    }

    /// Apply a degradation plan to this config, adjusting fidelity and glyph mode.
    pub fn apply_degradation(&mut self, plan: &fm_core::MermaidDegradationPlan) {
        if let Some(glyph_mode) = plan.force_glyph_mode {
            self.glyph_mode = glyph_mode;
        }
        if plan.collapse_clusters {
            self.show_clusters = false;
        }
        if plan.hide_labels {
            self.max_label_chars = 0;
            self.max_label_lines = 0;
        }
        match plan.target_fidelity {
            fm_core::MermaidFidelity::Compact => {
                self.tier = MermaidTier::Compact;
                self.fidelity_override = Some(MermaidFidelity::Compact);
                self.render_mode = MermaidRenderMode::CellOnly;
            }
            fm_core::MermaidFidelity::Outline => {
                self.tier = MermaidTier::Compact;
                self.fidelity_override = Some(MermaidFidelity::Outline);
                self.render_mode = MermaidRenderMode::CellOnly;
                self.show_clusters = false;
                self.diagonal_edges = false;
            }
            _ => {}
        }
    }

    /// Resolve the effective tier based on available space.
    #[must_use]
    pub fn effective_tier(&self, available_cols: usize, available_rows: usize) -> MermaidTier {
        match self.tier {
            MermaidTier::Auto => {
                let area = available_cols.saturating_mul(available_rows);
                if area < 1000 {
                    MermaidTier::Compact
                } else if area < 5000 {
                    MermaidTier::Normal
                } else {
                    MermaidTier::Rich
                }
            }
            other => other,
        }
    }

    /// Resolve the 4-tier [`MermaidFidelity`] (`Outline`/`Compact`/`Normal`/`Rich`)
    /// using layout scale, density, node count, and viewport size.
    #[must_use]
    pub fn effective_fidelity(
        &self,
        scale_x: f32,
        scale_y: f32,
        density: f32,
        node_count: usize,
        available_cols: usize,
        available_rows: usize,
    ) -> MermaidFidelity {
        if let Some(override_fidelity) = self.fidelity_override {
            return override_fidelity;
        }
        select_fidelity(
            scale_x,
            scale_y,
            density,
            node_count,
            available_cols,
            available_rows,
            self.tier,
        )
    }

    /// Resolve the effective render mode based on tier.
    #[must_use]
    pub fn effective_render_mode(&self, tier: MermaidTier) -> MermaidRenderMode {
        match self.render_mode {
            MermaidRenderMode::Auto => match tier {
                MermaidTier::Compact => MermaidRenderMode::CellOnly,
                MermaidTier::Normal => MermaidRenderMode::HalfBlock,
                MermaidTier::Rich | MermaidTier::Auto => MermaidRenderMode::Braille,
            },
            other => other,
        }
    }
}

/// Select the effective terminal rendering fidelity tier from layout scale, density,
/// node count, viewport dimensions, and optional explicit tier override.
///
/// Matches FrankenTUI's `select_fidelity` contract:
/// - `cols < 16 || rows < 6` always degrades to [`MermaidFidelity::Outline`].
/// - `cols < 35 || rows < 10` degrades to [`MermaidFidelity::Compact`].
/// - Explicit `Compact`/`Normal`/`Rich` overrides are respected (clamped by viewport).
/// - `Auto` inspects `min(scale_x, scale_y)` and `density`.
#[must_use]
pub fn select_fidelity(
    scale_x: f32,
    scale_y: f32,
    density: f32,
    node_count: usize,
    cols: usize,
    rows: usize,
    override_tier: MermaidTier,
) -> MermaidFidelity {
    if cols < 16 || rows < 6 {
        return MermaidFidelity::Outline;
    }
    match override_tier {
        MermaidTier::Compact => return MermaidFidelity::Compact,
        MermaidTier::Normal => return MermaidFidelity::Normal,
        MermaidTier::Rich => {
            if cols < 30 || rows < 10 {
                return MermaidFidelity::Compact;
            }
            return MermaidFidelity::Rich;
        }
        MermaidTier::Auto => {}
    }
    if cols < 35 || rows < 10 {
        return MermaidFidelity::Compact;
    }
    let min_scale = scale_x.min(scale_y);
    if min_scale < 0.28 || density > 0.55 || (node_count > 30 && cols < 60) {
        MermaidFidelity::Outline
    } else if min_scale < 0.45 || density > 0.42 || (node_count > 18 && cols < 80) {
        MermaidFidelity::Compact
    } else if min_scale > 0.85 && density < 0.25 && cols >= 60 && rows >= 20 {
        MermaidFidelity::Rich
    } else {
        MermaidFidelity::Normal
    }
}

/// Interactive node selection state for terminal diagram navigation.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionState {
    /// Currently selected node index (into `ir.nodes`), if any.
    pub selected_node: Option<usize>,
    /// Secondary highlighted nodes (e.g., neighbors of the selected node).
    pub highlighted_nodes: Vec<usize>,
}

impl SelectionState {
    /// Build a `SelectionState` with `node_idx` selected and its neighbors highlighted.
    #[must_use]
    pub fn from_selected(node_idx: usize, ir: &MermaidDiagramIr) -> Self {
        let adj = build_adjacency(ir);
        let mut state = Self::default();
        state.select(node_idx, &adj);
        state
    }

    /// Select the next node in index order (wrapping around).
    pub fn select_next(&mut self, node_count: usize, adj: &[Vec<usize>]) {
        if node_count == 0 {
            self.clear();
            return;
        }
        let next = match self.selected_node {
            Some(idx) => (idx + 1) % node_count,
            None => 0,
        };
        self.select(next, adj);
    }

    /// Select the previous node in index order (wrapping around).
    pub fn select_prev(&mut self, node_count: usize, adj: &[Vec<usize>]) {
        if node_count == 0 {
            self.clear();
            return;
        }
        let prev = match self.selected_node {
            Some(0) | None => node_count - 1,
            Some(idx) => idx - 1,
        };
        self.select(prev, adj);
    }

    /// Select a specific node and populate its connected neighbors.
    pub fn select(&mut self, idx: usize, adj: &[Vec<usize>]) {
        self.selected_node = Some(idx);
        self.highlighted_nodes = adj.get(idx).cloned().unwrap_or_default();
    }

    /// Clear the current selection.
    pub fn clear(&mut self) {
        self.selected_node = None;
        self.highlighted_nodes.clear();
    }

    /// Navigate from the currently selected node in a spatial direction (`dx`, `dy`).
    pub fn navigate(
        &mut self,
        layout: &DiagramLayout,
        adj: &[Vec<usize>],
        dx: f32,
        dy: f32,
    ) -> Option<usize> {
        let current = self.selected_node?;
        let target = navigate_direction(layout, current, dx, dy)?;
        self.select(target, adj);
        Some(target)
    }
}

/// Build an undirected adjacency list from the diagram IR for neighbor highlighting.
#[must_use]
pub fn build_adjacency(ir: &MermaidDiagramIr) -> Vec<Vec<usize>> {
    let n = ir.nodes.len();
    let mut adj = vec![Vec::new(); n];
    let resolve = |ep: &IrEndpoint| -> Option<usize> {
        match ep {
            IrEndpoint::Node(id) => Some(id.0),
            IrEndpoint::Port(port_id) => ir.ports.get(port_id.0).map(|p| p.node.0),
            IrEndpoint::Unresolved => None,
        }
    };
    for edge in &ir.edges {
        if let (Some(u), Some(v)) = (resolve(&edge.from), resolve(&edge.to))
            && u < n
            && v < n
            && u != v
        {
            adj[u].push(v);
            adj[v].push(u);
        }
    }
    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }
    adj
}

/// Find the best node in direction `(dx, dy)` from `current` using layout coordinates.
///
/// `dx` and `dy` define a unit-ish direction vector (e.g. `(0.0, -1.0)` for up,
/// `(1.0, 0.0)` for right). Returns the `node_index` of the closest node whose
/// direction from `current` has a positive dot product with `(dx, dy)`, weighted
/// to prefer nodes more aligned with the direction vector.
#[must_use]
pub fn navigate_direction(
    layout: &DiagramLayout,
    current: usize,
    dx: f32,
    dy: f32,
) -> Option<usize> {
    let cur_box = layout.nodes.iter().find(|b| b.node_index == current)?;
    let cur_cx = cur_box.bounds.x + cur_box.bounds.width / 2.0;
    let cur_cy = cur_box.bounds.y + cur_box.bounds.height / 2.0;

    let mut best: Option<(usize, f32)> = None;
    for nb in &layout.nodes {
        if nb.node_index == current {
            continue;
        }
        let ncx = nb.bounds.x + nb.bounds.width / 2.0;
        let ncy = nb.bounds.y + nb.bounds.height / 2.0;
        let vx = ncx - cur_cx;
        let vy = ncy - cur_cy;
        let dot = vx * dx + vy * dy;
        if dot <= 0.1 {
            continue;
        }
        let dist = vx.hypot(vy);
        if dist < 1e-6 {
            continue;
        }
        let cos_angle = dot / dist;
        let score = dist * (2.0 - cos_angle);
        if best.is_none_or(|(_, best_score)| score < best_score) {
            best = Some((nb.node_index, score));
        }
    }
    best.map(|(idx, _)| idx)
}

/// Resolved configuration after auto-detection.
#[derive(Debug, Clone, Copy)]
pub struct ResolvedConfig {
    pub tier: MermaidTier,
    pub fidelity: MermaidFidelity,
    pub render_mode: MermaidRenderMode,
    pub glyph_mode: MermaidGlyphMode,
    pub cols: usize,
    pub rows: usize,
    pub max_label_chars: usize,
    pub max_label_lines: usize,
    pub show_selection: bool,
    pub selected_node: Option<usize>,
    pub link_mode: MermaidLinkMode,
    pub show_clusters: bool,
    pub diagonal_edges: bool,
    pub padding: usize,
    /// [`TermRenderConfig::gantt_today`] resolved to an epoch day number, or `None` when no date
    /// was supplied or the one supplied is not a calendar date.
    ///
    /// The NUMBER rather than the string, for two reasons. This struct is `Copy` and an
    /// `Option<String>` would end that for every consumer. And the parse belongs here: it happens
    /// once, through the same `parse_iso_day_number` the layout used to place the bars, so the
    /// renderer cannot accidentally grow a second copy of that arithmetic — which is exactly how a
    /// marker and its axis come to disagree about where a day is.
    ///
    /// Nothing about it depends on the resolved tier: a today marker is one line drawn on the
    /// canvas layer, so there is no size at which dropping it buys room for anything else.
    pub gantt_today_day: Option<i32>,
    /// Whether to append the diagnostic debug overlay line (`bd-0l1x2`).
    pub debug_overlay: bool,
}

impl ResolvedConfig {
    /// Resolve configuration for the given terminal size.
    #[must_use]
    pub fn resolve(config: &TermRenderConfig, cols: usize, rows: usize) -> Self {
        let available_cols = cols.min(config.max_width);
        let available_rows = rows.min(config.max_height);
        let tier = config.effective_tier(available_cols, available_rows);
        let fidelity = config.fidelity_override.unwrap_or(match tier {
            MermaidTier::Compact => MermaidFidelity::Compact,
            MermaidTier::Normal => MermaidFidelity::Normal,
            MermaidTier::Rich | MermaidTier::Auto => MermaidFidelity::Rich,
        });
        let render_mode = config.effective_render_mode(tier);

        // Adjust label limits based on tier.
        let (max_label_chars, max_label_lines) = match tier {
            MermaidTier::Compact => (config.max_label_chars.min(12), 1),
            MermaidTier::Normal => (
                config.max_label_chars.min(24),
                config.max_label_lines.min(2),
            ),
            MermaidTier::Rich | MermaidTier::Auto => {
                (config.max_label_chars, config.max_label_lines)
            }
        };
        let max_label_chars = max_label_chars.max(1);
        let max_label_lines = max_label_lines.max(1);

        Self {
            tier,
            fidelity,
            render_mode,
            glyph_mode: config.glyph_mode,
            cols: available_cols,
            rows: available_rows,
            max_label_chars,
            max_label_lines,
            show_selection: config.show_selection || config.selected_node.is_some(),
            selected_node: config.selected_node,
            link_mode: config.link_mode,
            show_clusters: config.show_clusters
                && !matches!(tier, MermaidTier::Compact)
                && !matches!(fidelity, MermaidFidelity::Outline),
            diagonal_edges: config.diagonal_edges,
            padding: config.padding,
            gantt_today_day: config
                .gantt_today
                .as_deref()
                .and_then(fm_layout::parse_iso_day_number),
            debug_overlay: config.debug_overlay,
        }
    }

    /// Get the sub-cell resolution multiplier for the render mode.
    #[must_use]
    pub const fn subcell_multiplier(&self) -> (usize, usize) {
        match self.render_mode {
            MermaidRenderMode::Braille => (2, 4), // 2 columns x 4 rows per cell
            MermaidRenderMode::Block => (2, 2),   // 2x2 per cell
            MermaidRenderMode::HalfBlock => (1, 2), // 1x2 per cell
            MermaidRenderMode::CellOnly | MermaidRenderMode::Auto => (1, 1),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_sane() {
        let config = TermRenderConfig::default();
        assert!(config.max_width >= 80);
        assert!(config.max_height >= 24);
    }

    #[test]
    fn apply_degradation_forces_ascii_and_compact() {
        let mut config = TermRenderConfig::rich();
        let plan = fm_core::MermaidDegradationPlan {
            target_fidelity: fm_core::MermaidFidelity::Compact,
            force_glyph_mode: Some(MermaidGlyphMode::Ascii),
            collapse_clusters: true,
            hide_labels: true,
            ..fm_core::MermaidDegradationPlan::default()
        };
        config.apply_degradation(&plan);
        assert_eq!(config.glyph_mode, MermaidGlyphMode::Ascii);
        assert_eq!(config.tier, MermaidTier::Compact);
        assert_eq!(config.render_mode, MermaidRenderMode::CellOnly);
        assert!(!config.show_clusters);
        assert_eq!(config.max_label_chars, 0);
    }

    #[test]
    fn apply_degradation_default_is_noop() {
        let original = TermRenderConfig::rich();
        let mut config = original.clone();
        config.apply_degradation(&fm_core::MermaidDegradationPlan::default());
        assert_eq!(config.tier, original.tier);
        assert_eq!(config.glyph_mode, original.glyph_mode);
        assert_eq!(config.show_clusters, original.show_clusters);
    }

    #[test]
    fn apply_degradation_outline_disables_everything() {
        let mut config = TermRenderConfig::rich();
        let plan = fm_core::MermaidDegradationPlan {
            target_fidelity: fm_core::MermaidFidelity::Outline,
            ..fm_core::MermaidDegradationPlan::default()
        };
        config.apply_degradation(&plan);
        assert_eq!(config.tier, MermaidTier::Compact);
        assert!(!config.show_clusters);
        assert!(!config.diagonal_edges);
    }

    #[test]
    fn auto_tier_selects_based_on_area() {
        let config = TermRenderConfig::default();
        assert_eq!(config.effective_tier(40, 20), MermaidTier::Compact);
        assert_eq!(config.effective_tier(80, 40), MermaidTier::Normal);
        assert_eq!(config.effective_tier(200, 60), MermaidTier::Rich);
    }

    #[test]
    fn resolved_config_respects_max_bounds() {
        let config = TermRenderConfig {
            max_width: 100,
            max_height: 30,
            ..Default::default()
        };
        let resolved = ResolvedConfig::resolve(&config, 200, 60);
        assert_eq!(resolved.cols, 100);
        assert_eq!(resolved.rows, 30);
    }

    #[test]
    fn braille_has_2x4_multiplier() {
        let config = ResolvedConfig {
            render_mode: MermaidRenderMode::Braille,
            tier: MermaidTier::Rich,
            fidelity: MermaidFidelity::Rich,
            glyph_mode: MermaidGlyphMode::Unicode,
            cols: 80,
            rows: 24,
            max_label_chars: 24,
            max_label_lines: 2,
            show_selection: false,
            selected_node: None,
            link_mode: MermaidLinkMode::Inline,
            show_clusters: true,
            diagonal_edges: true,
            debug_overlay: false,
            padding: 1,
            gantt_today_day: None,
        };
        assert_eq!(config.subcell_multiplier(), (2, 4));
    }
}
