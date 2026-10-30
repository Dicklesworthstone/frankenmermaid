//! Measured cluster headings shared by frame sizing, routing, and renderers.
//!
//! A font declaration changes the space a heading occupies, not just its paint. Keep the
//! displayed rows and alphabetic baselines together so a large or multiline title cannot
//! escape a frame that was measured with the default font, or cover its first child.

use fm_core::{FontMetrics, FontMetricsConfig, IrStyleTarget, MermaidDiagramIr};

use crate::LayoutRect;

/// Inset shared by a cluster's title and optional C4 boundary type.
pub const CAPTION_INSET: f32 = 8.0;

/// Semantic role of a displayed caption row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClusterCaptionRowKind {
    /// A line of the authored cluster title.
    Title,
    /// A line of the bracketed C4 boundary type, following the complete title.
    BoundaryType,
}

/// A single displayed row, positioned relative to the cluster's top edge.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterCaptionRow {
    /// Complete row text; wrapping and truncation do not alter authored headings.
    pub text: String,
    /// Alphabetic baseline measured from the top of the frame.
    pub baseline: f32,
    /// Conservative width of this row, excluding the frame's horizontal insets.
    pub width: f32,
    /// Whether this is title text or the C4 type subtitle.
    pub kind: ClusterCaptionRowKind,
}

/// Resolved typography and geometry of a cluster's complete caption.
#[derive(Debug, Clone, PartialEq)]
pub struct ClusterCaptionLayout {
    /// Displayed rows in reading order.
    pub rows: Vec<ClusterCaptionRow>,
    /// Resolved size in pixels, including authored overrides.
    pub font_size: f32,
    /// An authored family stack, when valid; otherwise the surface's default applies.
    pub font_family: Option<String>,
    /// An authored weight, when valid; otherwise title and subtitle retain their defaults.
    pub font_weight: Option<&'static str>,
    /// An authored style, when valid; otherwise the surface's default applies.
    pub font_style: Option<&'static str>,
    /// Minimum frame width including both horizontal insets, or zero for no caption.
    pub width: f32,
    /// Distance from the top of the frame through the last row's descenders.
    pub height: f32,
}

impl ClusterCaptionLayout {
    /// Measure the full-detail caption used to reserve space in the layout.
    #[must_use]
    pub fn new(
        ir: &MermaidDiagramIr,
        cluster_index: usize,
        title: Option<&str>,
        metrics: &FontMetrics,
    ) -> Self {
        Self::with_default_font_size(
            ir,
            cluster_index,
            title,
            metrics,
            (metrics.font_size() * 0.9).max(10.0),
        )
    }

    /// Resolve a caption for a surface's detail level. Authored size overrides apply flat;
    /// reducing detail must not shrink an explicit `font-size` declaration.
    #[must_use]
    pub fn with_default_font_size(
        ir: &MermaidDiagramIr,
        cluster_index: usize,
        title: Option<&str>,
        metrics: &FontMetrics,
        default_font_size: f32,
    ) -> Self {
        let mut properties = std::collections::BTreeMap::new();
        for style in &ir.style_refs {
            if style.target == IrStyleTarget::Cluster(cluster_index) {
                properties.extend(fm_core::parse_style_string(&style.style).properties);
            }
        }
        // A malformed canvas font shorthand is ignored as a whole. Resolve the same bounded
        // components here for measurement and every surface, before composing that shorthand.
        let font_family = properties.get("font-family").and_then(|family| {
            let family = family.trim();
            (!family.is_empty()
                && family.len() <= 128
                && family.chars().all(|character| {
                    character.is_ascii_alphanumeric()
                        || matches!(character, ' ' | ',' | '-' | '_' | '.' | '\'' | '"')
                }))
            .then(|| family.to_owned())
        });
        let font_weight = properties.get("font-weight").and_then(|weight| {
            match weight.trim().to_ascii_lowercase().as_str() {
                "normal" => Some("normal"),
                "bold" => Some("bold"),
                "bolder" => Some("bolder"),
                "lighter" => Some("lighter"),
                "100" => Some("100"),
                "200" => Some("200"),
                "300" => Some("300"),
                "400" => Some("400"),
                "500" => Some("500"),
                "600" => Some("600"),
                "700" => Some("700"),
                "800" => Some("800"),
                "900" => Some("900"),
                _ => None,
            }
        });
        let font_style = properties.get("font-style").and_then(|style| {
            match style.trim().to_ascii_lowercase().as_str() {
                "normal" => Some("normal"),
                "italic" => Some("italic"),
                "oblique" => Some("oblique"),
                _ => None,
            }
        });
        let base_metrics = FontMetrics::new(FontMetricsConfig {
            font_size: default_font_size,
            ..metrics.config().clone()
        });
        let authored_metrics = base_metrics.with_style_overrides(
            properties.get("font-size").map(String::as_str),
            font_family.as_deref(),
        );
        let caption_metrics = authored_metrics.as_ref().unwrap_or(&base_metrics);
        let font_size = caption_metrics.font_size();
        let mut caption = Self {
            rows: Vec::new(),
            font_size,
            font_family,
            font_weight,
            font_style,
            width: 0.0,
            height: 0.0,
        };
        let metadata = ir.clusters.get(cluster_index);
        let title = title
            .or_else(|| {
                metadata
                    .and_then(|cluster| cluster.title)
                    .and_then(|label| ir.labels.get(label.0))
                    .map(|label| label.text.as_str())
            })
            .unwrap_or_default();
        let displayed_title = title
            .strip_prefix("swimlane:")
            .or_else(|| title.strip_prefix("section "))
            .unwrap_or(title);
        let mut baseline = (font_size + 2.0).max(16.0);
        if !displayed_title.trim().is_empty() {
            for line in displayed_title.lines() {
                caption.push_row(
                    line,
                    baseline,
                    ClusterCaptionRowKind::Title,
                    caption_metrics,
                );
                baseline += font_size * 1.4;
            }
        }
        if let Some(kind) = metadata
            .and_then(|cluster| cluster.c4_boundary_type.as_deref())
            .filter(|kind| !kind.is_empty())
        {
            // The subtitle follows the LAST title row, including any explicit blank rows.
            // Retain the existing single-line C4 separation rather than adding a blank line.
            if let Some(last) = caption.rows.last() {
                baseline = last.baseline + font_size * 1.25;
            }
            for line in format!("[{kind}]").lines() {
                caption.push_row(
                    line,
                    baseline,
                    ClusterCaptionRowKind::BoundaryType,
                    caption_metrics,
                );
                baseline += font_size * 1.4;
            }
        }
        caption
    }

    fn push_row(
        &mut self,
        text: &str,
        baseline: f32,
        kind: ClusterCaptionRowKind,
        metrics: &FontMetrics,
    ) {
        // Headings are semibold by default. Keep the existing 1.4 allowance for browser font
        // variation together with the shared wide-glyph allowance used by preserved labels.
        let width = (metrics.estimate_width(text) * 1.4).max(metrics.estimate_preserved_width(
            text,
            self.font_size,
            0.0,
        ));
        self.width = self.width.max(width + CAPTION_INSET * 2.0);
        self.height = baseline + self.font_size * 0.3;
        self.rows.push(ClusterCaptionRow {
            text: text.to_owned(),
            baseline,
            width,
            kind,
        });
    }

    /// Extra top padding needed to leave room for descenders and incoming arrowheads.
    #[must_use]
    pub fn extra_header(&self, padding: f32) -> f32 {
        if self.rows.is_empty() {
            0.0
        } else {
            (self.height + 28.0 - padding).max(0.0)
        }
    }

    /// Font weight for one row, with authored values taking precedence over hierarchy.
    #[must_use]
    pub fn row_weight(&self, row: &ClusterCaptionRow) -> &'static str {
        self.font_weight.unwrap_or(match row.kind {
            ClusterCaptionRowKind::Title => "600",
            ClusterCaptionRowKind::BoundaryType => "400",
        })
    }

    /// The caption's actual row footprints, leaving empty interior space usable for routes.
    pub fn obstacles(&self, frame: LayoutRect) -> impl Iterator<Item = LayoutRect> + '_ {
        self.rows
            .iter()
            .filter(|row| !row.text.trim().is_empty())
            .map(move |row| LayoutRect {
                x: frame.x + CAPTION_INSET,
                y: frame.y + row.baseline - self.font_size,
                width: row.width,
                height: self.font_size * 1.3,
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, IrCluster, IrStyleRef, Span};

    #[test]
    fn authored_heading_font_moves_its_baseline_and_reserves_its_complete_width() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        let metrics = FontMetrics::default_metrics();
        let title = "Identity and permissions";
        let normal = ClusterCaptionLayout::new(&ir, 0, Some(title), &metrics);
        assert_eq!(normal.font_size, 13.5);
        assert_eq!(normal.rows[0].baseline, 16.0);
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Cluster(0),
            style: String::from(
                "font-size:40px;font-family:monospace;font-weight:700;font-style:italic",
            ),
            span: Span::default(),
        });
        let large = ClusterCaptionLayout::new(&ir, 0, Some(title), &metrics);
        assert_eq!(large.font_size, 40.0);
        assert_eq!(large.font_family.as_deref(), Some("monospace"));
        assert_eq!(large.row_weight(&large.rows[0]), "700");
        assert_eq!(large.font_style, Some("italic"));
        assert!(large.rows[0].baseline >= large.font_size + 2.0);
        assert!(large.width >= FontMetrics::monospace(40.0).estimate_width(title) + 16.0);
        assert!(large.width > normal.width * 2.5);
        assert!(large.extra_header(28.0) >= large.rows[0].baseline + 12.0);
        assert_eq!(large.rows[0].text, title);
    }

    #[test]
    fn c4_type_follows_every_title_line_without_rewriting_source_text() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::C4Context);
        ir.clusters.push(IrCluster {
            c4_boundary_type: Some(String::from("SYSTEM")),
            ..Default::default()
        });
        let title = "Payments\n\nRisk review";
        let caption =
            ClusterCaptionLayout::new(&ir, 0, Some(title), &FontMetrics::default_metrics());
        assert_eq!(caption.rows.len(), 4);
        assert_eq!(caption.rows[0].text, "Payments");
        assert_eq!(caption.rows[1].text, "");
        assert_eq!(caption.rows[2].text, "Risk review");
        assert_eq!(caption.rows[3].text, "[SYSTEM]");
        assert!(caption.rows[3].baseline > caption.rows[2].baseline + caption.font_size);
        assert_eq!(caption.row_weight(&caption.rows[3]), "400");
        assert_eq!(title, "Payments\n\nRisk review");
        assert_eq!(
            caption
                .obstacles(LayoutRect {
                    x: 0.0,
                    y: 0.0,
                    width: 400.0,
                    height: 200.0,
                })
                .count(),
            3,
        );
    }

    #[test]
    fn malformed_font_components_do_not_discard_valid_caption_size() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.style_refs.push(IrStyleRef {
            target: IrStyleTarget::Cluster(0),
            style: String::from(
                "font-size:32px;font-family:bad{};font-weight:heavy;font-style:slanted",
            ),
            span: Span::default(),
        });
        let caption =
            ClusterCaptionLayout::new(&ir, 0, Some("Heading"), &FontMetrics::default_metrics());
        assert_eq!(caption.font_size, 32.0);
        assert_eq!(caption.font_family, None);
        assert_eq!(caption.font_style, None);
        assert_eq!(caption.row_weight(&caption.rows[0]), "600");
    }

    #[test]
    fn styled_nested_captions_fit_their_reserved_frames_and_clear_nodes() {
        let ir = fm_parser::parse(
            "flowchart LR\n\
             subgraph platform[Commerce platform]\n\
               subgraph identity[Identity and permissions]\n\
                 A[Authenticate] --> B[Authorize]\n\
               end\n\
             end\n\
             subgraph audit[Audit trail]\n C[Record]\n end\n\
             B --> C\n C --> D[Complete]\n\
             style platform font-size:28px\n\
             style identity font-size:40px,font-family:monospace,font-weight:700\n\
             style audit font-size:30px\n",
        )
        .ir;
        let source_labels = ir.labels.clone();
        let metrics = FontMetrics::default_metrics();
        for algorithm in [
            crate::LayoutAlgorithm::Auto,
            crate::LayoutAlgorithm::Tree,
            crate::LayoutAlgorithm::Force,
        ] {
            let layout = crate::layout_diagram_traced_with_algorithm(&ir, algorithm).layout;
            assert_eq!(layout.clusters.len(), 3);
            for cluster in &layout.clusters {
                let caption = ClusterCaptionLayout::new(
                    &ir,
                    cluster.cluster_index,
                    cluster.title.as_deref(),
                    &metrics,
                );
                assert!(!caption.rows.is_empty());
                for ink in caption.obstacles(cluster.bounds) {
                    assert!(ink.x >= cluster.bounds.x && ink.y >= cluster.bounds.y);
                    assert!(
                        ink.x + ink.width <= cluster.bounds.x + cluster.bounds.width + 0.01,
                        "{algorithm:?}: full heading escapes its frame: {:?}",
                        cluster.title
                    );
                    for node in &layout.nodes {
                        let bounds = node.bounds;
                        assert!(
                            ink.x + ink.width <= bounds.x
                                || bounds.x + bounds.width <= ink.x
                                || ink.y + ink.height <= bounds.y
                                || bounds.y + bounds.height <= ink.y,
                            "{algorithm:?}: heading covers node {}",
                            node.node_id
                        );
                    }
                }
            }
            let outside = &layout
                .nodes
                .iter()
                .find(|node| node.node_id == "D")
                .unwrap()
                .bounds;
            for cluster in &layout.clusters {
                let bounds = cluster.bounds;
                assert!(
                    bounds.x + bounds.width <= outside.x
                        || outside.x + outside.width <= bounds.x
                        || bounds.y + bounds.height <= outside.y
                        || outside.y + outside.height <= bounds.y,
                    "{algorithm:?}: authored typography outgrew its compound slot"
                );
            }
        }
        assert_eq!(ir.labels, source_labels);
    }
}
