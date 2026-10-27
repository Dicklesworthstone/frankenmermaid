//! Measured pie composition shared by SVG, Canvas and target-agnostic scenes.
//!
//! Slice angles encode values. Labels and the legend reserve their own space rather than
//! changing those angles or relying on the bounds of invisible graph nodes around the circle.

use fm_core::{FontMetrics, FontMetricsConfig, IrPieMeta, MermaidDiagramIr};

use crate::{
    FillStyle, LayoutPoint, LayoutRect, MarkerKind, PathCmd, RenderClip, RenderGroup, RenderItem,
    RenderPath, RenderScene, RenderSource, RenderText, StrokeStyle, TextAlign, TextBaseline,
};

/// A measured text block. Each row is drawn at the position returned by [`Self::rows`].
#[derive(Debug, Clone, PartialEq)]
pub struct PieTextBlock {
    pub lines: Vec<String>,
    pub bounds: LayoutRect,
    pub font_size: f32,
    pub line_height: f32,
    pub align: TextAlign,
}

impl PieTextBlock {
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
pub struct PieSliceLayout {
    pub start_angle: f32,
    pub sweep_angle: f32,
    pub label: PieTextBlock,
    /// Rim, outside elbow and label endpoint. Present for positive slices only.
    pub leader: Option<[LayoutPoint; 3]>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PieLegendEntry {
    pub swatch: LayoutRect,
    pub label: PieTextBlock,
}

impl PieLegendEntry {
    /// The complete semantic hit target, including its identifying color swatch.
    #[must_use]
    pub fn bounds(&self) -> LayoutRect {
        LayoutRect {
            x: self.swatch.x,
            y: self.label.bounds.y,
            width: self.label.bounds.x + self.label.bounds.width - self.swatch.x,
            height: self.label.bounds.height.max(self.swatch.height),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PieChartLayout {
    pub bounds: LayoutRect,
    pub center: LayoutPoint,
    pub radius: f32,
    pub title: Option<PieTextBlock>,
    pub slices: Vec<PieSliceLayout>,
    pub legend: LayoutRect,
    pub entries: Vec<PieLegendEntry>,
}

fn measured_block(
    text: &str,
    font_size: f32,
    max_width: f32,
    metrics: &FontMetrics,
    avg_char_width: f32,
) -> PieTextBlock {
    let row_metrics = FontMetrics::new(FontMetricsConfig {
        font_size,
        ..metrics.config().clone()
    });
    let mut wrap_width = max_width / 1.25;
    let mut lines = row_metrics.wrap_to_width(text, wrap_width);
    let mut width = 0.0_f32;
    for _ in 0..8 {
        width = lines
            .iter()
            .map(|line| metrics.estimate_preserved_width(line, font_size, avg_char_width))
            .fold(0.0_f32, f32::max);
        if width <= max_width {
            break;
        }
        wrap_width *= (max_width / width).min(0.9);
        lines = row_metrics.wrap_to_width(text, wrap_width);
    }
    // A wide grapheme is indivisible. Let the legend grow to retain it instead of clipping it.
    width = lines
        .iter()
        .map(|line| metrics.estimate_preserved_width(line, font_size, avg_char_width))
        .fold(width.min(max_width), f32::max);
    let line_height = font_size * metrics.config().line_height.max(1.3);
    PieTextBlock {
        bounds: LayoutRect {
            x: 0.0,
            y: 0.0,
            width,
            height: lines.len().max(1) as f32 * line_height,
        },
        lines,
        font_size,
        line_height,
        align: TextAlign::Start,
    }
}

impl PieChartLayout {
    /// Resolve all painted geometry. The optional width calibration and minimum font are the
    /// same knobs used by the graphical renderer, so a large requested face stays inside its frame.
    #[must_use]
    pub fn new(
        meta: &IrPieMeta,
        title: Option<&str>,
        metrics: &FontMetrics,
        min_font_size: f32,
        avg_char_width: f32,
    ) -> Self {
        let base_font = metrics.font_size();
        let label_font = (base_font * 0.85).max(min_font_size);
        let title_font = (base_font * 1.2).max(min_font_size);
        let scale = label_font / 12.75;
        let padding = 15.0 * scale;
        let swatch_size = 12.0 * scale;
        let swatch_gap = 9.0 * scale;
        let row_gap = 9.0 * scale;
        let label_gap = 5.0 * scale;
        let leader_gap = 22.0 * scale;
        let radius = (112.0 + 6.0 * meta.slices.len().min(12) as f32) * scale;

        let mut entries: Vec<_> = meta
            .slices
            .iter()
            .map(|slice| {
                // Preserve Mermaid's showData format and the author's fractional value.
                let text = if meta.show_data {
                    format!("{} [{}]", slice.label, slice.value.max(0.0))
                } else {
                    slice.label.clone()
                };
                PieLegendEntry {
                    swatch: LayoutRect {
                        x: 0.0,
                        y: 0.0,
                        width: swatch_size,
                        height: swatch_size,
                    },
                    label: measured_block(
                        &text,
                        label_font,
                        252.0 * scale,
                        metrics,
                        avg_char_width,
                    ),
                }
            })
            .collect();
        let legend_text_width = entries
            .iter()
            .map(|entry| entry.label.bounds.width)
            .fold(105.0 * scale, f32::max);
        let legend_width = 2.0 * padding + swatch_size + swatch_gap + legend_text_width;
        let legend_height = entries
            .iter()
            .map(|entry| entry.label.bounds.height.max(swatch_size))
            .sum::<f32>()
            + entries.len().saturating_sub(1) as f32 * row_gap
            + 2.0 * padding;

        // Keep the existing f32 percentage-rounding contract for ordinary inputs. Normalize
        // only if that sum overflows, otherwise a valid very large chart becomes NaN geometry.
        let total = meta
            .slices
            .iter()
            .map(|slice| slice.value.max(0.0))
            .sum::<f32>();
        let widest = meta
            .slices
            .iter()
            .map(|slice| slice.value.max(0.0))
            .filter(|value| value.is_finite())
            .fold(0.0_f32, f32::max);
        let normalized_total = meta
            .slices
            .iter()
            .filter(|slice| slice.value.is_finite())
            .map(|slice| f64::from(slice.value.max(0.0)) / f64::from(widest.max(f32::EPSILON)))
            .sum::<f64>();
        let mut angle = -std::f32::consts::FRAC_PI_2;
        let mut sides = [Vec::new(), Vec::new()];
        let mut slices = Vec::with_capacity(meta.slices.len());
        for (index, slice) in meta.slices.iter().enumerate() {
            let value = slice.value.max(0.0);
            let ratio = if !value.is_finite() {
                0.0
            } else if total.is_finite() && total > 0.0 {
                value / total
            } else if normalized_total > 0.0 {
                ((f64::from(value) / f64::from(widest)) / normalized_total) as f32
            } else {
                0.0
            };
            let sweep = ratio * std::f32::consts::TAU;
            let mid = angle + sweep * 0.5;
            let mut label = measured_block(
                // Math.round's half-up behavior, including the existing f32 percent before
                // promotion: 1/8 must read 13%, where formatting 12.5 directly gives 12%.
                &format!("{}%", (f64::from(ratio * 100.0) + 0.5).floor() as i64),
                label_font,
                96.0 * scale,
                metrics,
                avg_char_width,
            );
            label.align = TextAlign::Middle;
            sides[usize::from(mid.cos() >= 0.0)].push(index);
            slices.push(PieSliceLayout {
                start_angle: angle,
                sweep_angle: sweep,
                label,
                leader: None,
            });
            angle += sweep;
        }
        let max_percent_width = slices
            .iter()
            .map(|slice| slice.label.bounds.width)
            .fold(0.0_f32, f32::max);
        let label_height = slices
            .iter()
            .map(|slice| slice.label.bounds.height)
            .fold(label_font * 1.3, f32::max);
        let label_column_height = sides
            .iter()
            .map(|side| {
                side.iter()
                    .map(|&index| slices[index].label.bounds.height)
                    .sum::<f32>()
                    + side.len().saturating_sub(1) as f32 * label_gap
            })
            .fold(0.0_f32, f32::max);
        let body_height = (2.0 * radius + 2.0 * label_height)
            .max(label_column_height)
            .max(legend_height);
        let chart_width = 2.0 * (radius + leader_gap + max_percent_width);
        let body_width = chart_width + 30.0 * scale + legend_width;
        let mut title = title.map(|title| {
            let mut block = measured_block(title, title_font, body_width, metrics, avg_char_width);
            block.align = TextAlign::Middle;
            block
        });
        // An indivisible grapheme can be wider than the wrapping budget. Reserve its real
        // width and center the whole composition under it rather than assigning a negative x.
        let width = title
            .as_ref()
            .map_or(body_width, |title| body_width.max(title.bounds.width));
        if let Some(title) = &mut title {
            title.bounds.x = (width - title.bounds.width) * 0.5;
        }
        let body_offset = (width - body_width) * 0.5;
        let title_height = title
            .as_ref()
            .map_or(0.0, |title| title.bounds.height + padding);
        let center = LayoutPoint {
            x: chart_width * 0.5 + body_offset,
            y: title_height + body_height * 0.5,
        };
        let legend = LayoutRect {
            x: chart_width + 30.0 * scale + body_offset,
            y: title_height + (body_height - legend_height) * 0.5,
            width: legend_width,
            height: legend_height,
        };
        let mut row_y = legend.y + padding;
        for entry in &mut entries {
            entry.label.bounds.x = legend.x + padding + swatch_size + swatch_gap;
            entry.label.bounds.y = row_y;
            entry.swatch.x = legend.x + padding;
            entry.swatch.y = row_y + (entry.label.line_height - swatch_size) * 0.5;
            row_y += entry.label.bounds.height.max(swatch_size) + row_gap;
        }

        for (side_index, side) in sides.iter_mut().enumerate() {
            side.sort_by(|&a, &b| {
                let mid =
                    |index: usize| slices[index].start_angle + slices[index].sweep_angle * 0.5;
                mid(a).sin().total_cmp(&mid(b).sin()).then(a.cmp(&b))
            });
            let mut next_y = title_height;
            for &index in side.iter() {
                let slice = &mut slices[index];
                let mid = slice.start_angle + slice.sweep_angle * 0.5;
                let natural_y = center.y + (radius + 10.0 * scale) * mid.sin()
                    - slice.label.bounds.height * 0.5;
                slice.label.bounds.y = natural_y.max(next_y);
                slice.label.bounds.x = if side_index == 0 {
                    center.x - radius - leader_gap - slice.label.bounds.width
                } else {
                    center.x + radius + leader_gap
                };
                next_y = slice.label.bounds.y + slice.label.bounds.height + label_gap;
            }
            let mut bottom = title_height + body_height;
            for &index in side.iter().rev() {
                let slice = &mut slices[index];
                slice.label.bounds.y = slice.label.bounds.y.min(bottom - slice.label.bounds.height);
                bottom = slice.label.bounds.y - label_gap;
            }
            for &index in side.iter() {
                let slice = &mut slices[index];
                if slice.sweep_angle <= 0.0 {
                    continue;
                }
                let mid = slice.start_angle + slice.sweep_angle * 0.5;
                let rim = LayoutPoint {
                    x: center.x + radius * mid.cos(),
                    y: center.y + radius * mid.sin(),
                };
                let label_end = LayoutPoint {
                    x: if side_index == 0 {
                        slice.label.bounds.x + slice.label.bounds.width + 4.0 * scale
                    } else {
                        slice.label.bounds.x - 4.0 * scale
                    },
                    y: slice.label.bounds.y + slice.label.bounds.height * 0.5,
                };
                slice.leader = Some([
                    rim,
                    LayoutPoint {
                        // Exit horizontally before following a displaced label. A radial
                        // elbow followed by a diagonal can cross back through the circle.
                        x: center.x
                            + if side_index == 0 {
                                -(radius + 10.0 * scale)
                            } else {
                                radius + 10.0 * scale
                            },
                        y: rim.y,
                    },
                    label_end,
                ]);
            }
        }
        // Keep an authored empty title harmless while preserving all nonempty wrapped lines.
        if title.as_ref().is_some_and(|title| title.lines.is_empty()) {
            title = None;
        }
        Self {
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width,
                height: title_height + body_height,
            },
            center,
            radius,
            title,
            slices,
            legend,
            entries,
        }
    }
}

/// Cubic circular arcs for the shared scene's path vocabulary (at most 90 degrees per segment).
fn wedge_commands(center: LayoutPoint, radius: f32, slice: &PieSliceLayout) -> Vec<PathCmd> {
    if slice.sweep_angle <= 0.0 {
        return Vec::new();
    }
    let full = slice.sweep_angle >= std::f32::consts::TAU;
    let start = LayoutPoint {
        x: center.x + radius * slice.start_angle.cos(),
        y: center.y + radius * slice.start_angle.sin(),
    };
    let mut commands = if full {
        vec![PathCmd::MoveTo {
            x: start.x,
            y: start.y,
        }]
    } else {
        vec![
            PathCmd::MoveTo {
                x: center.x,
                y: center.y,
            },
            PathCmd::LineTo {
                x: start.x,
                y: start.y,
            },
        ]
    };
    let segments = (slice.sweep_angle / std::f32::consts::FRAC_PI_2)
        .ceil()
        .max(1.0) as usize;
    let step = slice.sweep_angle / segments as f32;
    for index in 0..segments {
        let a = slice.start_angle + index as f32 * step;
        let b = a + step;
        let k = (4.0 / 3.0) * (step * 0.25).tan();
        commands.push(PathCmd::CubicTo {
            c1x: center.x + radius * (a.cos() - k * a.sin()),
            c1y: center.y + radius * (a.sin() + k * a.cos()),
            c2x: center.x + radius * (b.cos() + k * b.sin()),
            c2y: center.y + radius * (b.sin() - k * b.cos()),
            x: center.x + radius * b.cos(),
            y: center.y + radius * b.sin(),
        });
    }
    commands.push(PathCmd::Close);
    commands
}

pub(crate) fn build_pie_render_scene(ir: &MermaidDiagramIr, metrics: &FontMetrics) -> RenderScene {
    let meta = ir
        .pie_meta
        .as_ref()
        .expect("pie dispatch requires metadata");
    let pie = PieChartLayout::new(
        meta,
        ir.meta.title.as_deref().or(meta.title.as_deref()),
        metrics,
        8.0,
        metrics.avg_char_width(),
    );
    let mut root = RenderGroup::new(Some(String::from("pie-chart")));
    root.clip = Some(RenderClip::Rect(pie.bounds.into()));
    let variables = &ir.meta.theme_overrides.theme_variables;
    let text_color = variables
        .get("textColor")
        .and_then(|paint| crate::scene_paint(paint))
        .unwrap_or_else(|| String::from("#1e293b"));
    let text = |block: &PieTextBlock, source| {
        block
            .rows()
            .map(|(text, point)| {
                RenderItem::Text(RenderText {
                    source,
                    text: text.to_string(),
                    x: point.x,
                    y: point.y,
                    font_size: block.font_size,
                    align: block.align,
                    baseline: TextBaseline::Middle,
                    fill: FillStyle::Solid {
                        color: text_color.clone(),
                        opacity: 1.0,
                    },
                })
            })
            .collect::<Vec<_>>()
    };
    if let Some(title) = &pie.title {
        root.children.extend(text(title, RenderSource::Diagram));
    }
    for (index, slice) in pie.slices.iter().enumerate() {
        let source = if index < ir.nodes.len() {
            RenderSource::Node(index)
        } else {
            RenderSource::Diagram
        };
        let color = variables
            .get(&format!("pie{}", index % 8 + 1))
            .and_then(|paint| crate::scene_paint(paint))
            .unwrap_or_else(|| fm_core::mindmap::DEFAULT_BRANCH_ACCENTS[index % 8].to_string());
        root.children.push(RenderItem::Path(RenderPath {
            source,
            commands: wedge_commands(pie.center, pie.radius, slice),
            fill: Some(FillStyle::Solid {
                color: color.clone(),
                opacity: 1.0,
            }),
            stroke: Some(StrokeStyle::solid("#ffffff", 2.0)),
            marker_start: MarkerKind::None,
            marker_end: MarkerKind::None,
        }));
        if let Some(leader) = slice.leader {
            root.children.push(RenderItem::Path(RenderPath {
                source,
                commands: vec![
                    PathCmd::MoveTo {
                        x: leader[0].x,
                        y: leader[0].y,
                    },
                    PathCmd::LineTo {
                        x: leader[1].x,
                        y: leader[1].y,
                    },
                    PathCmd::LineTo {
                        x: leader[2].x,
                        y: leader[2].y,
                    },
                ],
                fill: None,
                stroke: Some(StrokeStyle {
                    opacity: 0.55,
                    ..StrokeStyle::solid(&text_color, 1.0)
                }),
                marker_start: MarkerKind::None,
                marker_end: MarkerKind::None,
            }));
        }
        root.children.extend(text(&slice.label, source));
        let entry = &pie.entries[index];
        root.children.push(crate::scene_path(
            source,
            crate::rounded_rect_path(entry.swatch, 2.0),
            Some(FillStyle::Solid {
                color,
                opacity: 1.0,
            }),
            None,
        ));
        root.children.extend(text(&entry.label, source));
    }
    RenderScene {
        bounds: pie.bounds.into(),
        root,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, FontPreset, IrPieSlice};

    fn fixture() -> IrPieMeta {
        IrPieMeta {
            title: Some("Annual platform investment".to_string()),
            show_data: true,
            slices: (0..24)
                .map(|index| IrPieSlice {
                    label: format!("Customer identity and access management for region {index}"),
                    value: if index == 0 { 90.0 } else { 1.0 },
                })
                .collect(),
        }
    }

    fn contains(outer: LayoutRect, inner: LayoutRect) -> bool {
        inner.x >= outer.x - 0.01
            && inner.y >= outer.y - 0.01
            && inner.x + inner.width <= outer.x + outer.width + 0.01
            && inner.y + inner.height <= outer.y + outer.height + 0.01
    }

    fn overlaps(a: LayoutRect, b: LayoutRect) -> bool {
        a.x < b.x + b.width - 0.01
            && b.x < a.x + a.width - 0.01
            && a.y < b.y + b.height - 0.01
            && b.y < a.y + a.height - 0.01
    }

    #[test]
    fn pie_crowded_percentages_and_wrapped_legends_have_disjoint_measured_space() {
        let meta = fixture();
        for font_size in [15.0, 24.0, 36.0] {
            for preset in [
                FontPreset::SystemUi,
                FontPreset::Monospace,
                FontPreset::Condensed,
            ] {
                let metrics = FontMetrics::new(FontMetricsConfig {
                    preset,
                    font_size,
                    ..Default::default()
                });
                let pie = PieChartLayout::new(
                    &meta,
                    meta.title.as_deref(),
                    &metrics,
                    8.0,
                    metrics.avg_char_width(),
                );
                let blocks: Vec<_> = pie
                    .slices
                    .iter()
                    .map(|slice| &slice.label)
                    .chain(pie.entries.iter().map(|entry| &entry.label))
                    .chain(pie.title.iter())
                    .collect();
                for (index, block) in blocks.iter().enumerate() {
                    assert!(
                        contains(pie.bounds, block.bounds),
                        "text must be inside the export: {:?}",
                        block.bounds
                    );
                    for other in &blocks[index + 1..] {
                        assert!(
                            !overlaps(block.bounds, other.bounds),
                            "text boxes overlap: {:?} {:?}",
                            block.bounds,
                            other.bounds
                        );
                    }
                    for line in &block.lines {
                        assert!(
                            metrics.estimate_preserved_width(
                                line,
                                block.font_size,
                                metrics.avg_char_width()
                            ) <= block.bounds.width + 0.01
                        );
                    }
                }
                assert!(pie.entries.iter().all(|entry| entry.label.lines.len() >= 2));
                for (entry, slice) in pie.entries.iter().zip(&meta.slices) {
                    assert!(contains(pie.legend, entry.label.bounds));
                    assert!(contains(pie.legend, entry.swatch));
                    assert_eq!(
                        entry.label.lines.join(" "),
                        format!("{} [{}]", slice.label, slice.value)
                    );
                }
                for slice in &pie.slices {
                    let leader = slice
                        .leader
                        .expect("a displaced positive label needs a leader");
                    assert!(
                        ((leader[0].x - pie.center.x).hypot(leader[0].y - pie.center.y)
                            - pie.radius)
                            .abs()
                            < 0.01
                    );
                    assert!((leader[2].y - slice.label.bounds.center().y).abs() < 0.01);
                    assert!(
                        (leader[1].y - leader[0].y).abs() < 0.01,
                        "the rim exit must stay horizontal"
                    );
                    assert!((leader[1].x - pie.center.x).abs() > pie.radius);
                    assert!((leader[2].x - pie.center.x).abs() > pie.radius);
                }
                for left in [true, false] {
                    let mut leaders: Vec<_> = pie
                        .slices
                        .iter()
                        .filter_map(|slice| slice.leader)
                        .filter(|leader| (leader[1].x < pie.center.x) == left)
                        .collect();
                    leaders.sort_by(|a, b| a[0].y.total_cmp(&b[0].y));
                    for pair in leaders.windows(2) {
                        assert!(
                            pair[0][2].y <= pair[1][2].y + 0.01,
                            "connector order must not cross"
                        );
                    }
                }
                assert_eq!(
                    pie,
                    PieChartLayout::new(
                        &meta,
                        meta.title.as_deref(),
                        &metrics,
                        8.0,
                        metrics.avg_char_width()
                    )
                );
            }
        }
    }

    #[test]
    fn pie_preserves_fractional_values_rounding_and_unicode_when_wrapped() {
        let name = "Global 東京 identity Cafe\u{301} 👩\u{200d}💻 access management ".repeat(6);
        let meta = IrPieMeta {
            slices: vec![
                IrPieSlice {
                    label: name.clone(),
                    value: 0.25,
                },
                IrPieSlice {
                    label: "Other".to_string(),
                    value: 1.75,
                },
            ],
            show_data: true,
            ..Default::default()
        };
        let metrics = FontMetrics::default_metrics();
        let pie = PieChartLayout::new(&meta, Some(&name), &metrics, 8.0, metrics.avg_char_width());
        assert_eq!(pie.slices[0].label.lines, ["13%"]);
        assert_eq!(pie.slices[1].label.lines, ["88%"]);
        let actual: String = pie.entries[0]
            .label
            .lines
            .join("")
            .split_whitespace()
            .collect();
        let expected: String = format!("{name} [0.25]").split_whitespace().collect();
        assert_eq!(actual, expected);
        assert!(
            pie.entries[0]
                .label
                .lines
                .iter()
                .any(|line| line.contains("Cafe\u{301}"))
        );
        assert!(
            pie.entries[0]
                .label
                .lines
                .iter()
                .any(|line| line.contains("👩\u{200d}💻"))
        );
        assert!(pie.title.as_ref().unwrap().lines.len() > 1);
        let no_values = PieChartLayout::new(
            &IrPieMeta {
                show_data: false,
                ..meta
            },
            None,
            &metrics,
            8.0,
            metrics.avg_char_width(),
        );
        assert!(
            no_values.entries.iter().all(|entry| entry
                .label
                .lines
                .iter()
                .all(|line| !line.contains('[')))
        );
    }

    #[test]
    fn pie_angles_are_proportional_and_remain_finite_for_zero_and_extreme_values() {
        let metrics = FontMetrics::default_metrics();
        for values in [
            vec![0.0],
            vec![1.0],
            vec![1e-30],
            vec![1.0, 3.0, 0.0],
            vec![1e-30, 3e-30],
            vec![1e38, 3e38],
        ] {
            let meta = IrPieMeta {
                slices: values
                    .iter()
                    .enumerate()
                    .map(|(index, &value)| IrPieSlice {
                        label: format!("Part {index}"),
                        value,
                    })
                    .collect(),
                ..Default::default()
            };
            let pie = PieChartLayout::new(&meta, None, &metrics, 8.0, metrics.avg_char_width());
            assert!(pie.bounds.width.is_finite() && pie.bounds.height.is_finite());
            let sum = values.iter().map(|&value| f64::from(value)).sum::<f64>();
            for (slice, &value) in pie.slices.iter().zip(&values) {
                assert!(slice.start_angle.is_finite() && slice.sweep_angle.is_finite());
                let ratio = if sum > 0.0 {
                    f64::from(value) / sum
                } else {
                    0.0
                };
                assert!(
                    (f64::from(slice.sweep_angle) / std::f64::consts::TAU - ratio).abs() < 1e-6
                );
                assert_eq!(slice.leader.is_some(), value > 0.0);
            }
        }
    }

    #[test]
    fn pie_wide_grapheme_title_and_calibrated_percent_rows_stay_inside_the_frame() {
        let title = vec!["👩"; 160].join("\u{200d}");
        let meta = IrPieMeta {
            slices: (0..60)
                .map(|index| IrPieSlice {
                    label: String::new(),
                    value: if index == 59 { 1.0 } else { 0.0 },
                })
                .collect(),
            ..Default::default()
        };
        let metrics = FontMetrics::default_metrics();
        let pie = PieChartLayout::new(&meta, Some(&title), &metrics, 8.0, 200.0);
        assert_eq!(pie.title.as_ref().unwrap().lines.concat(), title);
        assert!(contains(pie.bounds, pie.title.as_ref().unwrap().bounds));
        assert!(pie.slices.last().unwrap().label.lines.len() > pie.slices[0].label.lines.len());
        for (index, slice) in pie.slices.iter().enumerate() {
            assert!(contains(pie.bounds, slice.label.bounds));
            for other in &pie.slices[index + 1..] {
                assert!(!overlaps(slice.label.bounds, other.label.bounds));
            }
        }
    }

    #[test]
    fn pie_near_full_slice_keeps_its_boundary_instead_of_covering_a_small_neighbor() {
        let meta = IrPieMeta {
            slices: vec![
                IrPieSlice {
                    label: "Small".to_string(),
                    value: 0.00001,
                },
                IrPieSlice {
                    label: "Large".to_string(),
                    value: 1.0,
                },
            ],
            ..Default::default()
        };
        let metrics = FontMetrics::default_metrics();
        let pie = PieChartLayout::new(&meta, None, &metrics, 8.0, metrics.avg_char_width());
        for slice in &pie.slices {
            assert!(
                matches!(wedge_commands(pie.center, pie.radius, slice).first(), Some(PathCmd::MoveTo { x, y }) if *x == pie.center.x && *y == pie.center.y)
            );
        }
    }

    #[test]
    fn pie_shared_scene_has_wedges_percentages_and_complete_legend_content() {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Pie);
        ir.pie_meta = Some(fixture());
        let metrics = FontMetrics::default_metrics();
        let scene = build_pie_render_scene(&ir, &metrics);
        let drawn: Vec<_> = scene
            .root
            .children
            .iter()
            .filter_map(|item| match item {
                RenderItem::Text(text) => Some(text.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(drawn.contains(&"Annual platform investment"));
        assert_eq!(drawn.iter().filter(|text| text.ends_with('%')).count(), 24);
        assert_eq!(scene.root.children.iter().filter(|item| matches!(item, RenderItem::Path(path) if path.commands.iter().any(|command| matches!(command, PathCmd::CubicTo { .. })))).count(), 24);
        assert!(drawn.iter().any(|text| text.contains("[90]")));
        assert!(drawn.iter().any(|text| text.contains("23")));
        assert!(
            scene
                .root
                .children
                .iter()
                .all(|item| !matches!(item, RenderItem::Group(_)))
        );
    }
}
