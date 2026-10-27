//! Complete treemap composition without changing the areas that encode the data.
//!
//! Text that cannot fit at its requested size is retained in a measured details column.
//! The column identifies the full hierarchy and value, including entries whose layout area
//! is too small to contain even a reference marker.

use std::collections::BTreeMap;

use fm_core::{FontMetrics, FontMetricsConfig, FontPreset, IrStyleTarget, MermaidDiagramIr};

use crate::{
    DiagramLayout, FillStyle, LayoutPoint, LayoutRect, MarkerKind, RenderClip, RenderGroup,
    RenderItem, RenderPath, RenderRect, RenderScene, RenderSource, RenderText, StrokeStyle,
    TextAlign, TextBaseline,
};

pub const TREEMAP_COLORS: [&str; 6] = [
    "#8686ff", "#ffff78", "#d7ff86", "#ff86c8", "#86e0ff", "#ffc386",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreemapTextRole {
    Label(usize),
    Value(usize),
    /// A complete root-to-entry path in the overflow details column.
    DetailPath(usize),
    /// A reference number in a tile whose full caption appears in the details column.
    Reference(usize),
    Title,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreemapTextBlock {
    pub role: TreemapTextRole,
    pub lines: Vec<String>,
    pub bounds: LayoutRect,
    pub font_size: f32,
    pub line_height: f32,
    pub align: TextAlign,
    /// None uses the renderer's resolved theme text color.
    pub color: Option<String>,
    pub opacity: f32,
    /// Tile-local text is clipped as a final safeguard against font rasterization differences.
    pub clip: Option<LayoutRect>,
}

impl TreemapTextBlock {
    pub fn rows(&self) -> impl Iterator<Item = (&str, LayoutPoint)> {
        let x = match self.align {
            TextAlign::Start => self.bounds.x,
            TextAlign::Middle => self.bounds.x + self.bounds.width * 0.5,
            TextAlign::End => self.bounds.x + self.bounds.width,
        };
        self.lines.iter().enumerate().map(move |(index, text)| {
            (
                text.as_str(),
                LayoutPoint {
                    x,
                    y: self.bounds.y + (index as f32 + 0.5) * self.line_height,
                },
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreemapTileLayout {
    /// Index into `IrTreemapMeta::nodes`, not the ordinary graph-node arena.
    pub node: usize,
    /// The exact quantitative rectangle supplied by the tiler.
    pub bounds: LayoutRect,
    pub depth: u32,
    pub is_leaf: bool,
    pub value: f64,
    pub fill: String,
    pub fill_opacity: f32,
    /// None uses the renderer's resolved theme stroke.
    pub stroke: Option<String>,
    pub stroke_opacity: f32,
    pub stroke_width: f32,
    pub classes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreemapDetailLayout {
    pub node: usize,
    pub reference: usize,
    pub swatch: LayoutRect,
    pub fill: String,
    pub opacity: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreemapChartLayout {
    pub bounds: LayoutRect,
    pub tiles: Vec<TreemapTileLayout>,
    pub text: Vec<TreemapTextBlock>,
    pub details: Vec<TreemapDetailLayout>,
}

impl TreemapChartLayout {
    #[must_use]
    pub fn new(ir: &MermaidDiagramIr, layout: &DiagramLayout, metrics: &FontMetrics) -> Self {
        let mut chart = Self {
            bounds: layout.bounds,
            tiles: Vec::new(),
            text: Vec::new(),
            details: Vec::new(),
        };
        let Some(meta) = &ir.treemap_meta else {
            return chart;
        };
        let mut slots = vec![None; meta.nodes.len()];
        let mut next_slot = 0;
        // Match SVG's container palette: each section chooses a hue; only leaves inherit.
        for tile in &layout.extensions.treemap_tiles {
            assign_slot(meta, tile.node, &mut slots, &mut next_slot);
        }
        for node in 0..meta.nodes.len() {
            assign_slot(meta, node, &mut slots, &mut next_slot);
        }
        let styles: Vec<_> = meta
            .nodes
            .iter()
            .map(|item| resolved_style(ir, &item.classes))
            .collect();
        let values = summed_values(meta);
        let mut represented = vec![false; meta.nodes.len()];
        let mut overflow = Vec::new();
        for tile in &layout.extensions.treemap_tiles {
            let Some(item) = meta.nodes.get(tile.node) else {
                continue;
            };
            if !valid_rect(tile.bounds) {
                continue;
            }
            let style = &styles[tile.node];
            let opacity = style_number(style, "opacity")
                .unwrap_or(1.0)
                .clamp(0.0, 1.0);
            let painted = TreemapTileLayout {
                node: tile.node,
                bounds: tile.bounds,
                depth: tile.depth,
                is_leaf: tile.is_leaf,
                value: tile.value,
                fill: style.get("fill").cloned().unwrap_or_else(|| {
                    TREEMAP_COLORS[slots[tile.node].unwrap_or(0) % TREEMAP_COLORS.len()].to_owned()
                }),
                fill_opacity: visible_paint(style.get("fill"))
                    * opacity
                    * style_number(style, "fill-opacity")
                        .unwrap_or(if tile.is_leaf { 0.35 } else { 0.15 })
                        .clamp(0.0, 1.0),
                stroke: style.get("stroke").cloned(),
                stroke_opacity: visible_paint(style.get("stroke"))
                    * opacity
                    * style_number(style, "stroke-opacity")
                        .unwrap_or(1.0)
                        .clamp(0.0, 1.0),
                stroke_width: style_number(style, "stroke-width").unwrap_or(1.0).max(0.0),
                classes: item.classes.clone(),
            };
            represented[tile.node] = true;
            if let Some(blocks) = tile_text(&painted, &item.label, style, metrics) {
                chart.text.extend(blocks);
            } else {
                overflow.push(tile.node);
            }
            include_stroke(&mut chart.bounds, painted.bounds, painted.stroke_width);
            chart.tiles.push(painted);
        }
        // A deeply nested section can exhaust its height before the tiler reaches its children.
        // Zero-valued entries have no area either. Both still carry authored information.
        overflow.extend(
            represented
                .iter()
                .enumerate()
                .filter_map(|(node, &present)| (!present).then_some(node)),
        );
        let details_x = chart.bounds.x + chart.bounds.width + 28.0;
        let mut details_y = layout.bounds.y;
        for (index, node) in overflow.into_iter().enumerate() {
            let style = &styles[node];
            let row_metrics = styled_metrics(metrics, style, metrics.font_size());
            let reference = index + 1;
            let path = format!("[{reference}] {}", ancestor_path(meta, node));
            let mut label = measured_block(
                &path,
                TreemapTextRole::DetailPath(node),
                300.0 * metrics.font_size() / 15.0,
                &row_metrics,
                style,
            );
            let mut value = measured_block(
                &format_treemap_value(values[node]),
                TreemapTextRole::Value(node),
                300.0 * metrics.font_size() / 15.0,
                &styled_metrics(metrics, style, metrics.font_size() * 0.8),
                style,
            );
            let swatch_size = metrics.font_size().max(10.0);
            label.bounds.x = details_x + swatch_size + 10.0;
            label.bounds.y = details_y;
            value.bounds.x = label.bounds.x;
            value.bounds.y = label.bounds.y + label.bounds.height + 3.0;
            let swatch = LayoutRect {
                x: details_x,
                y: details_y + (label.line_height - swatch_size) * 0.5,
                width: swatch_size,
                height: swatch_size,
            };
            chart.details.push(TreemapDetailLayout {
                node,
                reference,
                swatch,
                fill: style.get("fill").cloned().unwrap_or_else(|| {
                    TREEMAP_COLORS[slots[node].unwrap_or(0) % TREEMAP_COLORS.len()].to_owned()
                }),
                opacity: visible_paint(style.get("fill"))
                    * style_number(style, "opacity")
                        .unwrap_or(1.0)
                        .clamp(0.0, 1.0)
                    * style_number(style, "fill-opacity")
                        .unwrap_or(0.35)
                        .clamp(0.0, 1.0),
            });
            include(&mut chart.bounds, swatch);
            include(&mut chart.bounds, label.bounds);
            include(&mut chart.bounds, value.bounds);
            details_y = value.bounds.y + value.bounds.height + 12.0;
            chart.text.push(label);
            chart.text.push(value);

            if let Some(tile) = chart.tiles.iter().find(|tile| tile.node == node) {
                let marker_metrics = styled_metrics(metrics, style, metrics.font_size() * 0.8);
                let mut marker = measured_block(
                    &format!("[{reference}]"),
                    TreemapTextRole::Reference(node),
                    f32::MAX,
                    &marker_metrics,
                    style,
                );
                let box_height = if tile.is_leaf {
                    tile.bounds.height
                } else {
                    tile.bounds.height.min(25.0)
                };
                if marker.bounds.width + 12.0 <= tile.bounds.width
                    && marker.bounds.height + 4.0 <= box_height
                {
                    marker.align = TextAlign::Middle;
                    marker.bounds.x =
                        tile.bounds.x + (tile.bounds.width - marker.bounds.width) * 0.5;
                    marker.bounds.y = tile.bounds.y + (box_height - marker.bounds.height) * 0.5;
                    marker.clip = Some(tile.bounds);
                    chart.text.push(marker);
                }
            }
        }
        if let Some(title) = ir
            .declared_title_if_drawn()
            .filter(|title| !title.is_empty())
        {
            let title_metrics =
                styled_metrics(metrics, &BTreeMap::new(), metrics.font_size() * 1.2);
            let mut title = measured_block(
                title,
                TreemapTextRole::Title,
                chart.bounds.width,
                &title_metrics,
                &BTreeMap::new(),
            );
            title.align = TextAlign::Middle;
            title.bounds.x = chart.bounds.x + (chart.bounds.width - title.bounds.width) * 0.5;
            title.bounds.y = chart.bounds.y - title.bounds.height - 16.0;
            include(&mut chart.bounds, title.bounds);
            chart.text.push(title);
        }
        chart
    }

    /// Paint the original tile order (parents first), then measured text above every tile.
    #[must_use]
    pub fn to_scene(&self, label_color: &str, stroke_color: &str) -> RenderScene {
        let mut root = RenderGroup::new(Some("treemap-chart".to_owned()));
        root.clip = Some(RenderClip::Rect(RenderRect::from(self.bounds)));
        for tile in &self.tiles {
            let mut group = RenderGroup::new(Some(format!("treemap-tile-{}", tile.node)));
            let mut stroke = StrokeStyle::solid(
                tile.stroke.as_deref().unwrap_or(stroke_color),
                tile.stroke_width,
            );
            stroke.opacity = tile.stroke_opacity;
            group.children.push(RenderItem::Path(RenderPath {
                source: RenderSource::Diagram,
                commands: crate::shapes::rounded_rect_path(tile.bounds, 0.0),
                fill: Some(FillStyle::Solid {
                    color: tile.fill.clone(),
                    opacity: tile.fill_opacity,
                }),
                stroke: Some(stroke),
                marker_start: MarkerKind::None,
                marker_end: MarkerKind::None,
            }));
            root.children.push(RenderItem::Group(group));
        }
        for detail in &self.details {
            root.children.push(RenderItem::Path(RenderPath {
                source: RenderSource::Diagram,
                commands: crate::shapes::rounded_rect_path(detail.swatch, 0.0),
                fill: Some(FillStyle::Solid {
                    color: detail.fill.clone(),
                    opacity: detail.opacity,
                }),
                stroke: None,
                marker_start: MarkerKind::None,
                marker_end: MarkerKind::None,
            }));
        }
        for (index, block) in self.text.iter().enumerate() {
            let id = match block.role {
                TreemapTextRole::Label(node)
                | TreemapTextRole::Value(node)
                | TreemapTextRole::DetailPath(node)
                | TreemapTextRole::Reference(node) => format!("treemap-text-{node}-{index}"),
                TreemapTextRole::Title => "treemap-title".to_owned(),
            };
            let mut group = RenderGroup::new(Some(id));
            group.clip = block
                .clip
                .map(|rect| RenderClip::Rect(RenderRect::from(rect)));
            for (text, point) in block.rows() {
                group.children.push(RenderItem::Text(RenderText {
                    source: RenderSource::Diagram,
                    text: text.to_owned(),
                    x: point.x,
                    y: point.y,
                    font_size: block.font_size,
                    align: block.align,
                    baseline: TextBaseline::Middle,
                    fill: FillStyle::Solid {
                        color: block.color.as_deref().unwrap_or(label_color).to_owned(),
                        opacity: block.opacity,
                    },
                }));
            }
            root.children.push(RenderItem::Group(group));
        }
        RenderScene {
            bounds: RenderRect::from(self.bounds),
            root,
        }
    }
}

fn assign_slot(
    meta: &fm_core::IrTreemapMeta,
    node: usize,
    slots: &mut [Option<usize>],
    next: &mut usize,
) {
    let Some(item) = meta.nodes.get(node) else {
        return;
    };
    if slots[node].is_some() {
        return;
    }
    let inherited = if item.is_leaf() {
        item.parent
            .and_then(|parent| slots.get(parent).copied().flatten())
    } else {
        None
    };
    slots[node] = Some(inherited.unwrap_or_else(|| {
        let slot = *next;
        *next += 1;
        slot
    }));
}

fn resolved_style(ir: &MermaidDiagramIr, classes: &[String]) -> BTreeMap<String, String> {
    let mut style = BTreeMap::new();
    for class in std::iter::once("default").chain(classes.iter().map(String::as_str)) {
        if let Some(definition) = ir
            .style_defs
            .iter()
            .find(|definition| definition.name == class)
        {
            style.extend(definition.properties.clone());
        } else {
            for reference in &ir.style_refs {
                if matches!(&reference.target, IrStyleTarget::Class(name) if name == class) {
                    style.extend(fm_core::parse_style_string(&reference.style).properties);
                }
            }
        }
    }
    style
}

fn style_number(style: &BTreeMap<String, String>, key: &str) -> Option<f32> {
    style
        .get(key)?
        .trim()
        .trim_end_matches("px")
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
}

fn visible_paint(color: Option<&String>) -> f32 {
    if color.is_some_and(|color| {
        color.trim().eq_ignore_ascii_case("none")
            || color.trim().eq_ignore_ascii_case("transparent")
    }) {
        0.0
    } else {
        1.0
    }
}

fn styled_metrics(
    metrics: &FontMetrics,
    style: &BTreeMap<String, String>,
    size: f32,
) -> FontMetrics {
    let mut fallback_chain = metrics.config().fallback_chain.clone();
    if !fallback_chain.contains(&metrics.config().preset) {
        fallback_chain.push(metrics.config().preset);
    }
    FontMetrics::new(FontMetricsConfig {
        font_size: style_number(style, "font-size")
            .filter(|size| *size > 0.0 && *size <= 512.0)
            .unwrap_or(size.max(8.0)),
        preset: style
            .get("font-family")
            .map_or(metrics.config().preset, |family| {
                FontPreset::from_family(family)
            }),
        fallback_chain,
        ..metrics.config().clone()
    })
}

fn measured_block(
    text: &str,
    role: TreemapTextRole,
    max_width: f32,
    metrics: &FontMetrics,
    style: &BTreeMap<String, String>,
) -> TreemapTextBlock {
    // SVG can honor an authored face while Canvas scenes and the GPU atlas use their configured
    // face. Reserve the largest width of both, including the GPU's independent glyph advances
    // and square-cell overhang. Joined Unicode must not fit only because one font fuses it.
    let candidates: Vec<_> = std::iter::once(metrics.config().preset)
        .chain(metrics.config().fallback_chain.iter().copied())
        .map(|preset| {
            FontMetrics::new(FontMetricsConfig {
                preset,
                ..metrics.config().clone()
            })
        })
        .collect();
    let measure = |lines: &[String]| {
        candidates
            .iter()
            .flat_map(|candidate| {
                lines.iter().map(move |line| {
                    let advances: f32 = line
                        .chars()
                        .map(|character| {
                            let mut encoded = [0; 4];
                            candidate.estimate_width(character.encode_utf8(&mut encoded))
                        })
                        .sum();
                    candidate
                        .estimate_preserved_width(
                            line,
                            candidate.font_size(),
                            candidate.avg_char_width(),
                        )
                        .max(advances + candidate.font_size())
                })
            })
            .fold(0.0_f32, f32::max)
    };
    let mut wrap_width = (max_width - metrics.font_size()).max(1.0) / 1.2;
    let mut lines = metrics.wrap_preserved_to_width(text, wrap_width);
    let mut width = measure(&lines);
    for _ in 0..12 {
        if width <= max_width || wrap_width <= 1.0 {
            break;
        }
        wrap_width *= (max_width.max(1.0) / width).min(0.9);
        lines = metrics.wrap_preserved_to_width(text, wrap_width);
        width = measure(&lines);
    }
    let line_height = metrics.line_height_px().max(metrics.font_size() * 1.3);
    TreemapTextBlock {
        role,
        bounds: LayoutRect {
            x: 0.0,
            y: 0.0,
            width,
            height: lines.len() as f32 * line_height,
        },
        lines,
        font_size: metrics.font_size(),
        line_height,
        align: TextAlign::Start,
        color: style.get("color").cloned(),
        opacity: style_number(style, "opacity")
            .unwrap_or(1.0)
            .clamp(0.0, 1.0),
        clip: None,
    }
}

fn tile_text(
    tile: &TreemapTileLayout,
    label: &str,
    style: &BTreeMap<String, String>,
    metrics: &FontMetrics,
) -> Option<[TreemapTextBlock; 2]> {
    let label_metrics = styled_metrics(
        metrics,
        style,
        metrics.font_size()
            * if tile.is_leaf {
                14.0 / 15.0
            } else {
                12.0 / 15.0
            },
    );
    let value_metrics = styled_metrics(
        metrics,
        style,
        metrics.font_size()
            * if tile.is_leaf {
                11.0 / 15.0
            } else {
                10.0 / 15.0
            },
    );
    let padding = 6.0_f32.max(label_metrics.font_size() * 0.5);
    let inner_width = tile.bounds.width - padding * 2.0;
    let inner_height = if tile.is_leaf {
        tile.bounds.height
    } else {
        tile.bounds.height.min(25.0)
    };
    if inner_width <= 0.0 || inner_height <= 0.0 {
        return None;
    }
    let mut value = measured_block(
        &format_treemap_value(tile.value),
        TreemapTextRole::Value(tile.node),
        f32::MAX,
        &value_metrics,
        style,
    );
    let label_width = if tile.is_leaf {
        inner_width
    } else {
        inner_width - value.bounds.width - 10.0
    };
    if label_width <= 0.0 {
        return None;
    }
    let mut label = measured_block(
        label,
        TreemapTextRole::Label(tile.node),
        label_width,
        &label_metrics,
        style,
    );
    if label.bounds.width > label_width || value.bounds.width > inner_width {
        return None;
    }
    if tile.is_leaf {
        let height = label.bounds.height + 3.0 + value.bounds.height;
        if height + padding * 2.0 > inner_height {
            return None;
        }
        label.align = TextAlign::Middle;
        value.align = TextAlign::Middle;
        label.bounds.x = tile.bounds.x + (tile.bounds.width - label.bounds.width) * 0.5;
        value.bounds.x = tile.bounds.x + (tile.bounds.width - value.bounds.width) * 0.5;
        label.bounds.y = tile.bounds.y + (tile.bounds.height - height) * 0.5;
        value.bounds.y = label.bounds.y + label.bounds.height + 3.0;
    } else {
        if label.bounds.height + 4.0 > inner_height || value.bounds.height + 4.0 > inner_height {
            return None;
        }
        label.bounds.x = tile.bounds.x + padding;
        label.bounds.y = tile.bounds.y + (inner_height - label.bounds.height) * 0.5;
        value.align = TextAlign::End;
        value.bounds.x = tile.bounds.x + tile.bounds.width - padding - value.bounds.width;
        value.bounds.y = tile.bounds.y + (inner_height - value.bounds.height) * 0.5;
    }
    label.clip = Some(tile.bounds);
    value.clip = Some(tile.bounds);
    Some([label, value])
}

fn ancestor_path(meta: &fm_core::IrTreemapMeta, node: usize) -> String {
    let mut path = Vec::new();
    let mut seen = vec![false; meta.nodes.len()];
    let mut current = Some(node);
    while let Some(index) = current {
        let Some(item) = meta.nodes.get(index) else {
            break;
        };
        if seen[index] {
            break;
        }
        seen[index] = true;
        path.push(item.label.as_str());
        current = item.parent;
    }
    path.reverse();
    path.join(" / ")
}

// Iterative postorder also handles entries absent from the geometric layout without recursive
// walks through a deeply nested author tree. Invalid cyclic references contribute no new value.
fn summed_values(meta: &fm_core::IrTreemapMeta) -> Vec<f64> {
    let mut values = vec![0.0; meta.nodes.len()];
    let mut states = vec![0_u8; meta.nodes.len()];
    for start in 0..meta.nodes.len() {
        let mut stack = vec![(start, false)];
        while let Some((node, exiting)) = stack.pop() {
            let Some(item) = meta.nodes.get(node) else {
                continue;
            };
            if exiting {
                values[node] = item.value.unwrap_or_else(|| {
                    item.children
                        .iter()
                        .filter(|&&child| states.get(child) == Some(&2))
                        .map(|&child| values[child])
                        .sum()
                });
                states[node] = 2;
            } else if states[node] == 0 {
                states[node] = 1;
                stack.push((node, true));
                if item.value.is_none() {
                    stack.extend(item.children.iter().rev().map(|&child| (child, false)));
                }
            }
        }
    }
    values
}

/// Display whole and fractional quantities without inventing trailing zeros or saturating casts.
#[must_use]
pub fn format_treemap_value(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        value.to_string()
    }
}

fn valid_rect(rect: LayoutRect) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .into_iter()
        .all(f32::is_finite)
        && rect.width > 0.0
        && rect.height > 0.0
}

fn include_stroke(bounds: &mut LayoutRect, rect: LayoutRect, width: f32) {
    let inset = width * 0.5;
    include(
        bounds,
        LayoutRect {
            x: rect.x - inset,
            y: rect.y - inset,
            width: rect.width + width,
            height: rect.height + width,
        },
    );
}

fn include(bounds: &mut LayoutRect, rect: LayoutRect) {
    let right = (bounds.x + bounds.width).max(rect.x + rect.width);
    let bottom = (bounds.y + bounds.height).max(rect.y + rect.height);
    bounds.x = bounds.x.min(rect.x);
    bounds.y = bounds.y.min(rect.y);
    bounds.width = right - bounds.x;
    bounds.height = bottom - bounds.y;
}

#[cfg(test)]
mod tests {
    use super::*;

    const NESTED: &str = "treemap\n\"Root\"\n    \"Group one\"\n        \"Alpha\": 10\n        \"Beta\": 20\n    \"Group two\"\n        \"Gamma\": 30\n";

    fn chart(source: &str) -> (MermaidDiagramIr, DiagramLayout, TreemapChartLayout) {
        let ir = fm_parser::parse(source).ir;
        let layout = crate::layout_diagram(&ir);
        let chart = TreemapChartLayout::new(&ir, &layout, &FontMetrics::default_metrics());
        (ir, layout, chart)
    }

    #[test]
    fn hierarchy_colors_fill_real_tiles_without_changing_quantitative_area() {
        let (ir, layout, chart) = chart(NESTED);
        let meta = ir.treemap_meta.as_ref().unwrap();
        assert_eq!(chart.tiles.len(), layout.extensions.treemap_tiles.len());
        for (tile, original) in chart.tiles.iter().zip(&layout.extensions.treemap_tiles) {
            assert_eq!(tile.bounds, original.bounds);
            assert_eq!(tile.value, original.value);
            assert!(tile.fill_opacity > 0.0);
            if tile.is_leaf {
                let parent = meta.nodes[tile.node].parent.unwrap();
                let container = chart.tiles.iter().find(|tile| tile.node == parent).unwrap();
                assert_eq!(tile.fill, container.fill);
            }
        }
        let sections: Vec<_> = chart.tiles.iter().filter(|tile| !tile.is_leaf).collect();
        assert_ne!(sections[0].fill, sections[1].fill);
        assert_ne!(sections[1].fill, sections[2].fill);
        assert!(
            chart.details.is_empty(),
            "ordinary readable tiles need no overflow column"
        );
        let scene = chart.to_scene("#123456", "#654321");
        assert_eq!(
            scene.root.children.len(),
            chart.tiles.len() + chart.text.len()
        );
    }

    #[test]
    fn author_classes_preserve_paints_opacity_and_requested_text_size() {
        let source = "treemap\n\"Root\"\n    \"Alpha\": 10:::hot\n    \"Beta\": 20\nclassDef hot fill:#123456,stroke:#abcdef,stroke-width:3px,opacity:0.5,fill-opacity:0.8,stroke-opacity:0.6,color:#654321,font-size:24px\n";
        let (ir, _, chart) = chart(source);
        let meta = ir.treemap_meta.as_ref().unwrap();
        let node = meta
            .nodes
            .iter()
            .position(|item| item.label == "Alpha")
            .unwrap();
        let tile = chart.tiles.iter().find(|tile| tile.node == node).unwrap();
        assert_eq!(tile.fill, "#123456");
        assert_eq!(tile.stroke.as_deref(), Some("#abcdef"));
        assert_eq!(tile.stroke_width, 3.0);
        assert!((tile.fill_opacity - 0.4).abs() < 1e-6);
        assert!((tile.stroke_opacity - 0.3).abs() < 1e-6);
        let styled: Vec<_> = chart.text.iter().filter(|block| matches!(block.role, TreemapTextRole::Label(index) | TreemapTextRole::Value(index) if index == node)).collect();
        assert_eq!(styled.len(), 2);
        for block in styled {
            assert_eq!(block.font_size, 24.0);
            assert_eq!(block.color.as_deref(), Some("#654321"));
            assert_eq!(block.opacity, 0.5);
        }
    }

    #[test]
    fn tiny_tiles_keep_complete_hierarchy_values_unicode_and_spacing_in_details() {
        let source = "treemap\n\"Root\"\n    \"Tiny  👩‍💻  café leaf\": 0.00001\n    \"Large\": 1000\n    \"Zero\": 0\n";
        let (ir, layout, chart) = chart(source);
        let meta = ir.treemap_meta.as_ref().unwrap();
        assert!(!chart.details.is_empty());
        for (node, item) in meta.nodes.iter().enumerate() {
            let label = chart.text.iter().find(|block| matches!(block.role, TreemapTextRole::Label(index) | TreemapTextRole::DetailPath(index) if index == node)).unwrap();
            assert!(label.lines.concat().contains(&item.label));
            let value = chart
                .text
                .iter()
                .find(|block| block.role == TreemapTextRole::Value(node))
                .unwrap();
            assert_eq!(
                value.lines.concat(),
                format_treemap_value(meta.value_of(node))
            );
        }
        for detail in &chart.details {
            assert!(detail.swatch.x > layout.bounds.x + layout.bounds.width);
        }
        let tiny = meta
            .nodes
            .iter()
            .position(|item| item.label.starts_with("Tiny"))
            .unwrap();
        let path = chart
            .text
            .iter()
            .find(|block| block.role == TreemapTextRole::DetailPath(tiny))
            .unwrap();
        assert!(path.lines.concat().contains("Root / Tiny  👩‍💻  café leaf"));
        assert!(chart.bounds.width > layout.bounds.width);
    }

    #[test]
    fn missing_deep_geometry_still_displays_every_total_and_label() {
        let source =
            "treemap\n\"Root\"\n    \"Group\"\n        \"Inner\"\n            \"Leaf\": 5.25\n";
        let ir = fm_parser::parse(source).ir;
        let mut layout = crate::layout_diagram(&ir);
        layout.extensions.treemap_tiles.truncate(1);
        let chart = TreemapChartLayout::new(&ir, &layout, &FontMetrics::default_metrics());
        for node in 0..ir.treemap_meta.as_ref().unwrap().nodes.len() {
            let value = chart
                .text
                .iter()
                .find(|block| block.role == TreemapTextRole::Value(node))
                .unwrap();
            assert_eq!(value.lines.concat(), "5.25");
        }
        assert_eq!(chart.details.len(), 3);
        assert_eq!(chart.tiles.len(), 1);
    }

    #[test]
    fn custom_metrics_expand_details_without_moving_tiles_and_are_deterministic() {
        let (ir, layout, ordinary) = chart(NESTED);
        let metrics = FontMetrics::new(FontMetricsConfig {
            font_size: 40.0,
            line_height: 1.7,
            preset: FontPreset::Monospace,
            ..FontMetricsConfig::default()
        });
        let enlarged = TreemapChartLayout::new(&ir, &layout, &metrics);
        assert_eq!(enlarged, TreemapChartLayout::new(&ir, &layout, &metrics));
        assert!(enlarged.details.len() > ordinary.details.len());
        for (tile, original) in enlarged.tiles.iter().zip(&layout.extensions.treemap_tiles) {
            assert_eq!(tile.bounds, original.bounds);
        }
        for block in &enlarged.text {
            assert!(block.bounds.x >= enlarged.bounds.x);
            assert!(block.bounds.y >= enlarged.bounds.y);
            assert!(
                block.bounds.x + block.bounds.width
                    <= enlarged.bounds.x + enlarged.bounds.width + 0.01
            );
            assert!(
                block.bounds.y + block.bounds.height
                    <= enlarged.bounds.y + enlarged.bounds.height + 0.01
            );
        }
        assert!(format_treemap_value(1e20).starts_with("100000000000000000000"));
        assert_eq!(format_treemap_value(0.00001), "0.00001");
    }
}
