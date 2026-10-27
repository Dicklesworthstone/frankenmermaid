//! Shared quadrant-chart composition, measured labels, and bounded collision avoidance.
//!
//! Point coordinates remain data. Only identifying text moves when captions or other points
//! occupy its preferred position, with annotation rows available below a crowded plot.

use fm_core::{FontMetrics, FontMetricsConfig, IrQuadrantMeta, IrQuadrantPoint};

use crate::{DiagramLayout, LayoutPoint, LayoutRect, TextAlign};

#[derive(Debug, Clone, PartialEq)]
pub struct QuadrantPointLabel {
    pub lines: Vec<String>,
    pub bounds: LayoutRect,
    /// Marker radius including half its stroke, used for label and leader clearance.
    pub radius: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuadrantHeading {
    pub quadrant_index: usize,
    pub lines: Vec<String>,
    pub bounds: LayoutRect,
    pub font_size: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadrantPointGeometry {
    pub node_index: usize,
    pub center: LayoutPoint,
    pub radius: f32,
    pub stroke_width: f32,
}

impl QuadrantPointGeometry {
    pub const DEFAULT_RADIUS: f32 = 6.0;
    pub const DEFAULT_STROKE_WIDTH: f32 = 1.5;

    #[must_use]
    pub fn style_dimensions(point: Option<&IrQuadrantPoint>) -> (f32, f32) {
        let style = point.and_then(|point| point.style.as_ref());
        (
            style
                .and_then(|style| style.radius)
                .filter(|radius| *radius > 0.0)
                .unwrap_or(Self::DEFAULT_RADIUS),
            style
                .and_then(|style| style.stroke_width)
                .unwrap_or(Self::DEFAULT_STROKE_WIDTH),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuadrantLeader {
    pub point_index: usize,
    pub start: LayoutPoint,
    pub end: LayoutPoint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuadrantTextRole {
    XAxisLeft,
    XAxisRight,
    YAxisBottom,
    YAxisTop,
    Title,
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuadrantTextBlock {
    pub role: QuadrantTextRole,
    pub lines: Vec<String>,
    pub bounds: LayoutRect,
    pub font_size: f32,
    pub align: TextAlign,
    /// Exact alphabetic baseline of the first row.
    pub position: LayoutPoint,
}

impl QuadrantTextBlock {
    pub fn rows(&self) -> impl Iterator<Item = (&str, LayoutPoint)> {
        self.lines.iter().enumerate().map(move |(index, line)| {
            (
                line.as_str(),
                LayoutPoint {
                    x: self.position.x,
                    y: self.position.y + index as f32 * self.font_size * 1.3,
                },
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct QuadrantChartLayout {
    pub plot: LayoutRect,
    pub bounds: LayoutRect,
    pub headings: Vec<QuadrantHeading>,
    pub labels: Vec<QuadrantPointLabel>,
    pub points: Vec<QuadrantPointGeometry>,
    pub leaders: Vec<QuadrantLeader>,
    pub annotations: Vec<QuadrantTextBlock>,
    pub label_font_size: f32,
}

impl QuadrantChartLayout {
    #[must_use]
    pub fn new(
        layout: &DiagramLayout,
        meta: &IrQuadrantMeta,
        title: Option<&str>,
        offset: (f32, f32),
        metrics: &FontMetrics,
        min_font_size: f32,
        avg_char_width: f32,
    ) -> Self {
        let plot = plot_bounds(layout, meta, offset);
        let label_font_size = clamp_font_size(metrics.font_size() * 0.8, min_font_size);
        let headings = headings(meta, plot, metrics, min_font_size, avg_char_width);
        let points: Vec<_> = layout
            .nodes
            .iter()
            .enumerate()
            .map(|(index, node)| {
                let center = node.bounds.center();
                let (radius, stroke_width) =
                    QuadrantPointGeometry::style_dimensions(meta.points.get(index));
                QuadrantPointGeometry {
                    node_index: node.node_index,
                    center: LayoutPoint {
                        x: center.x + offset.0,
                        y: center.y + offset.1,
                    },
                    radius,
                    stroke_width,
                }
            })
            .collect();
        let labels = place_labels(
            layout,
            meta,
            plot,
            &points,
            &headings,
            metrics,
            label_font_size,
            avg_char_width,
        );
        let mut leaders = Vec::new();
        for (point_index, (point, label)) in points.iter().zip(&labels).enumerate() {
            let end = LayoutPoint {
                x: point
                    .center
                    .x
                    .clamp(label.bounds.x, label.bounds.x + label.bounds.width),
                y: point
                    .center
                    .y
                    .clamp(label.bounds.y, label.bounds.y + label.bounds.height),
            };
            let dx = end.x - point.center.x;
            let dy = end.y - point.center.y;
            let distance = dx.hypot(dy);
            if distance > label.radius + 10.0 {
                let start_distance = label.radius + 2.0;
                leaders.push(QuadrantLeader {
                    point_index,
                    start: LayoutPoint {
                        x: point.center.x + dx * start_distance / distance,
                        y: point.center.y + dy * start_distance / distance,
                    },
                    end,
                });
            }
        }
        let annotations = annotations(meta, title, plot, metrics, avg_char_width);
        let mut bounds = LayoutRect {
            x: layout.bounds.x + offset.0,
            y: layout.bounds.y + offset.1,
            width: layout.bounds.width,
            height: layout.bounds.height,
        };
        include(&mut bounds, plot);
        for rect in headings
            .iter()
            .map(|heading| heading.bounds)
            .chain(labels.iter().map(|label| label.bounds))
            .chain(annotations.iter().map(|text| text.bounds))
        {
            include(&mut bounds, rect);
        }
        for point in &points {
            let radius = point.radius + point.stroke_width.max(0.0) / 2.0;
            include(
                &mut bounds,
                LayoutRect {
                    x: point.center.x - radius,
                    y: point.center.y - radius,
                    width: radius * 2.0,
                    height: radius * 2.0,
                },
            );
        }
        Self {
            plot,
            bounds,
            headings,
            labels,
            points,
            leaders,
            annotations,
            label_font_size,
        }
    }
}

fn clamp_font_size(candidate: f32, min_font_size: f32) -> f32 {
    if !candidate.is_finite() {
        return min_font_size.max(1.0);
    }
    candidate.max(min_font_size)
}

fn plot_bounds(layout: &DiagramLayout, meta: &IrQuadrantMeta, offset: (f32, f32)) -> LayoutRect {
    // Match the layout engine's fixed data projection, independently of label typography.
    let metrics = FontMetrics::default_metrics();
    let width = (300.0_f32 + (layout.nodes.len() as f32 * 15.0).min(200.0)).clamp(200.0, 600.0);
    let axis_width = meta
        .x_axis_left
        .as_ref()
        .map_or(0.0, |label| metrics.estimate_dimensions(label).0);
    LayoutRect {
        x: (axis_width + 20.0).clamp(50.0, 120.0) + offset.0,
        y: 60.0 + offset.1,
        width,
        height: width,
    }
}

fn wrapped_lines(
    text: &str,
    font_size: f32,
    max_width: f32,
    metrics: &FontMetrics,
    avg_char_width: f32,
) -> (Vec<String>, f32) {
    let row_metrics = FontMetrics::new(FontMetricsConfig {
        preset: metrics.config().preset,
        font_size,
        line_height: 1.3,
        ..Default::default()
    });
    let measure = |text: &str| {
        row_metrics.estimate_preserved_width(
            text,
            font_size,
            avg_char_width * font_size / metrics.font_size().max(1.0),
        )
    };
    let mut wrap_width = max_width / 1.125;
    let mut lines = row_metrics.wrap_to_width(text, wrap_width);
    let mut width = lines
        .iter()
        .map(|line| measure(line))
        .fold(0.0_f32, f32::max);
    for _ in 0..3 {
        if width <= max_width {
            break;
        }
        wrap_width *= max_width / width;
        lines = row_metrics.wrap_to_width(text, wrap_width);
        width = lines
            .iter()
            .map(|line| measure(line))
            .fold(0.0_f32, f32::max);
    }
    (lines, width)
}

fn headings(
    meta: &IrQuadrantMeta,
    chart: LayoutRect,
    metrics: &FontMetrics,
    min_font_size: f32,
    avg_char_width: f32,
) -> Vec<QuadrantHeading> {
    let font_size = clamp_font_size(metrics.font_size() * 0.9, min_font_size);
    let mut headings = Vec::new();
    let mut overflow_y = chart.y + chart.height + (metrics.font_size() * 2.0 + 20.0).max(48.0);
    for (index, text) in meta.quadrant_labels.iter().take(4).enumerate() {
        if text.trim().is_empty() {
            continue;
        }
        let max_width = (chart.width / 2.0 - 24.0).max(1.0);
        let (mut lines, mut width) =
            wrapped_lines(text, font_size, max_width, metrics, avg_char_width);
        let mut height = font_size * 1.3 * lines.len() as f32;
        if height > chart.height / 2.0 - 20.0 {
            let key = format!("Q{}", index + 1);
            let (annotation, annotation_width) = wrapped_lines(
                &format!("{key}: {text}"),
                font_size,
                (chart.width - 24.0).max(1.0),
                metrics,
                avg_char_width,
            );
            let annotation_height = font_size * 1.3 * annotation.len() as f32;
            headings.push(QuadrantHeading {
                quadrant_index: index,
                lines: annotation,
                bounds: LayoutRect {
                    x: chart.x + 12.0,
                    y: overflow_y,
                    width: annotation_width,
                    height: annotation_height,
                },
                font_size,
            });
            overflow_y += annotation_height + 14.0;
            (lines, width) = wrapped_lines(&key, font_size, max_width, metrics, avg_char_width);
            height = font_size * 1.3 * lines.len() as f32;
        }
        headings.push(QuadrantHeading {
            quadrant_index: index,
            lines,
            bounds: LayoutRect {
                x: chart.x
                    + 12.0
                    + if matches!(index, 0 | 3) {
                        chart.width / 2.0
                    } else {
                        0.0
                    },
                y: chart.y + 8.0 + if index >= 2 { chart.height / 2.0 } else { 0.0 },
                width,
                height,
            },
            font_size,
        });
    }
    headings
}

fn overlap_area(a: LayoutRect, b: LayoutRect) -> f32 {
    let width = ((a.x + a.width).min(b.x + b.width) - a.x.max(b.x)).max(0.0);
    let height = ((a.y + a.height).min(b.y + b.height) - a.y.max(b.y)).max(0.0);
    width * height
}

#[allow(clippy::too_many_arguments)]
fn place_labels(
    layout: &DiagramLayout,
    meta: &IrQuadrantMeta,
    chart: LayoutRect,
    points: &[QuadrantPointGeometry],
    headings: &[QuadrantHeading],
    metrics: &FontMetrics,
    font_size: f32,
    avg_char_width: f32,
) -> Vec<QuadrantPointLabel> {
    let mut obstacles: Vec<_> = points
        .iter()
        .map(|point| {
            let radius = point.radius + point.stroke_width.max(0.0) / 2.0;
            LayoutRect {
                x: point.center.x - radius - 3.0,
                y: point.center.y - radius - 3.0,
                width: (radius + 3.0) * 2.0,
                height: (radius + 3.0) * 2.0,
            }
        })
        .collect();
    let mut overflow_y = chart.y + chart.height + (metrics.font_size() * 2.0 + 20.0).max(48.0);
    for heading in headings {
        if heading.bounds.y >= chart.y + chart.height {
            overflow_y = overflow_y.max(heading.bounds.y + heading.bounds.height + 14.0);
        }
        obstacles.push(LayoutRect {
            x: heading.bounds.x - 4.0,
            y: heading.bounds.y - 4.0,
            width: heading.bounds.width + 8.0,
            height: heading.bounds.height + 8.0,
        });
    }
    let mut labels = Vec::with_capacity(layout.nodes.len());
    for (index, node) in layout.nodes.iter().enumerate() {
        let text = meta
            .points
            .get(index)
            .map_or(node.node_id.as_str(), |point| point.label.as_str());
        let max_width = (chart.width * 0.38)
            .max(80.0)
            .min((chart.width - 12.0).max(1.0));
        let (mut lines, text_width) =
            wrapped_lines(text, font_size, max_width, metrics, avg_char_width);
        let width = text_width + 6.0;
        let height = font_size * 1.3 * lines.len().max(1) as f32 + 4.0;
        let point = &points[index];
        let (cx, cy) = (point.center.x, point.center.y);
        let radius = point.radius + point.stroke_width.max(0.0) / 2.0;
        let mut best = LayoutRect {
            x: chart.x + 4.0,
            y: chart.y + 4.0,
            width,
            height,
        };
        let mut best_score = (f32::INFINITY, f32::INFINITY);
        for ring in 0..if height <= chart.height / 2.0 { 16 } else { 0 } {
            let gap = radius + 5.0 + ring as f32 * (font_size * 1.3 + 6.0);
            let candidates = [
                (cx + gap, cy - height / 2.0),
                (cx - gap - width, cy - height / 2.0),
                (cx - width / 2.0, cy - gap - height),
                (cx - width / 2.0, cy + gap),
                (cx + gap, cy - gap - height),
                (cx - gap - width, cy - gap - height),
                (cx + gap, cy + gap),
                (cx - gap - width, cy + gap),
            ];
            for (x, y) in candidates {
                let candidate = LayoutRect {
                    x: x.clamp(
                        chart.x + 4.0,
                        (chart.x + chart.width - width - 4.0).max(chart.x + 4.0),
                    ),
                    y: y.clamp(
                        chart.y + 4.0,
                        (chart.y + chart.height - height - 4.0).max(chart.y + 4.0),
                    ),
                    width,
                    height,
                };
                let overlap = obstacles
                    .iter()
                    .map(|&other| overlap_area(candidate, other))
                    .sum::<f32>();
                let dx = cx - cx.clamp(candidate.x, candidate.x + width);
                let dy = cy - cy.clamp(candidate.y, candidate.y + height);
                let distance = dx * dx + dy * dy;
                if overlap < best_score.0 || (overlap == best_score.0 && distance < best_score.1) {
                    best = candidate;
                    best_score = (overlap, distance);
                }
            }
            if best_score.0 == 0.0 {
                break;
            }
        }
        if best_score.0 > 0.0 {
            let (overflow_lines, overflow_width) = wrapped_lines(
                text,
                font_size,
                (chart.width - 14.0).max(1.0),
                metrics,
                avg_char_width,
            );
            lines = overflow_lines;
            best = LayoutRect {
                x: chart.x + 4.0,
                y: overflow_y,
                width: overflow_width + 6.0,
                height: font_size * 1.3 * lines.len().max(1) as f32 + 4.0,
            };
            overflow_y += best.height + 10.0;
        }
        obstacles.push(LayoutRect {
            x: best.x - 2.0,
            y: best.y - 2.0,
            width: best.width + 4.0,
            height: best.height + 4.0,
        });
        labels.push(QuadrantPointLabel {
            lines,
            bounds: best,
            radius,
        });
    }
    labels
}

fn annotations(
    meta: &IrQuadrantMeta,
    title: Option<&str>,
    chart: LayoutRect,
    metrics: &FontMetrics,
    avg_char_width: f32,
) -> Vec<QuadrantTextBlock> {
    let mut text = Vec::new();
    let base_font_size = metrics.font_size();
    for (role, label, x, y, align, font_size) in [
        (
            QuadrantTextRole::XAxisLeft,
            meta.x_axis_left.as_deref(),
            chart.x,
            chart.y + chart.height + 20.0,
            TextAlign::Start,
            base_font_size * 0.8,
        ),
        (
            QuadrantTextRole::XAxisRight,
            meta.x_axis_right.as_deref(),
            chart.x + chart.width,
            chart.y + chart.height + 20.0,
            TextAlign::End,
            base_font_size * 0.8,
        ),
        (
            QuadrantTextRole::YAxisBottom,
            meta.y_axis_bottom.as_deref(),
            chart.x - 10.0,
            chart.y + chart.height,
            TextAlign::End,
            base_font_size * 0.8,
        ),
        (
            QuadrantTextRole::YAxisTop,
            meta.y_axis_top.as_deref(),
            chart.x - 10.0,
            chart.y + base_font_size * 0.3,
            TextAlign::End,
            base_font_size * 0.8,
        ),
        (
            QuadrantTextRole::Title,
            title,
            chart.x + chart.width / 2.0,
            chart.y - 20.0,
            TextAlign::Middle,
            base_font_size + 4.0,
        ),
    ] {
        let Some(label) = label else { continue };
        let width = metrics.estimate_preserved_width(label, font_size, avg_char_width);
        text.push(QuadrantTextBlock {
            role,
            lines: vec![label.to_owned()],
            bounds: LayoutRect {
                x: x - match align {
                    TextAlign::Start => 0.0,
                    TextAlign::Middle => width / 2.0,
                    TextAlign::End => width,
                },
                y: y - font_size,
                width,
                height: font_size * 1.3,
            },
            font_size,
            align,
            position: LayoutPoint { x, y },
        });
    }
    text
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

    fn contains(outer: LayoutRect, inner: LayoutRect) -> bool {
        inner.x >= outer.x - 0.001
            && inner.y >= outer.y - 0.001
            && inner.x + inner.width <= outer.x + outer.width + 0.001
            && inner.y + inner.height <= outer.y + outer.height + 0.001
    }

    #[test]
    fn quadrant_composition_preserves_data_and_clears_every_marker_and_heading() {
        let ir = fm_parser::parse(concat!(
            "quadrantChart\n",
            "title Deployment priorities\n",
            "x-axis Low effort --> High effort\n",
            "y-axis Low impact --> High impact\n",
            "quadrant-1 Schedule\nquadrant-2 Do first\nquadrant-3 Delegate\nquadrant-4 Reconsider\n",
            "Customer onboarding: [0.73, 0.77] radius: 18, stroke-width: 8px\n",
            "International deployment: [0.73, 0.78]\n",
            "Billing reliability: [0.74, 0.77]\n",
            "Operational readiness: [0.74, 0.78]\n",
        )).ir;
        let layout = crate::layout_diagram(&ir);
        let meta = ir.quadrant_meta.as_ref().unwrap();
        for preset in [
            fm_core::FontPreset::SystemUi,
            fm_core::FontPreset::Monospace,
        ] {
            for font_size in [15.0, 30.0] {
                let metrics = FontMetrics::new(FontMetricsConfig {
                    preset,
                    font_size,
                    ..Default::default()
                });
                let chart = QuadrantChartLayout::new(
                    &layout,
                    meta,
                    meta.title.as_deref(),
                    (24.0, 18.0),
                    &metrics,
                    20.0,
                    font_size * 0.65,
                );
                assert_eq!(
                    chart,
                    QuadrantChartLayout::new(
                        &layout,
                        meta,
                        meta.title.as_deref(),
                        (24.0, 18.0),
                        &metrics,
                        20.0,
                        font_size * 0.65,
                    )
                );
                assert_eq!(chart.points.len(), 4);
                assert_eq!(chart.labels.len(), 4);
                assert_eq!(chart.annotations.len(), 5);
                assert_eq!(chart.points[0].radius, 18.0);
                assert_eq!(chart.points[0].stroke_width, 8.0);
                assert_eq!(chart.labels[0].radius, 22.0);
                assert!(chart.label_font_size >= 20.0);
                let compact = |text: &str| {
                    text.chars()
                        .filter(|character| !character.is_whitespace())
                        .collect::<String>()
                };
                for (point, authored) in chart.points.iter().zip(&meta.points) {
                    assert!(
                        (point.center.x - chart.plot.x - authored.x * chart.plot.width).abs()
                            < 0.001
                    );
                    assert!(
                        (point.center.y - chart.plot.y - (1.0 - authored.y) * chart.plot.height)
                            .abs()
                            < 0.001
                    );
                }
                for (index, label) in chart.labels.iter().enumerate() {
                    assert!(contains(chart.bounds, label.bounds));
                    assert_eq!(
                        compact(&label.lines.concat()),
                        compact(&meta.points[index].label)
                    );
                    for other in &chart.labels[..index] {
                        assert_eq!(overlap_area(label.bounds, other.bounds), 0.0);
                    }
                    for heading in &chart.headings {
                        assert_eq!(overlap_area(label.bounds, heading.bounds), 0.0);
                    }
                    for point in &chart.points {
                        let dx = point.center.x
                            - point
                                .center
                                .x
                                .clamp(label.bounds.x, label.bounds.x + label.bounds.width);
                        let dy = point.center.y
                            - point
                                .center
                                .y
                                .clamp(label.bounds.y, label.bounds.y + label.bounds.height);
                        assert!(dx.hypot(dy) >= point.radius + point.stroke_width / 2.0);
                    }
                }
                for annotation in &chart.annotations {
                    assert!(contains(chart.bounds, annotation.bounds));
                }
                assert!(
                    !chart.leaders.is_empty(),
                    "crowded points need identifiable detached labels"
                );
                for leader in &chart.leaders {
                    let point = &chart.points[leader.point_index];
                    let radius = chart.labels[leader.point_index].radius;
                    assert!(
                        ((leader.start.x - point.center.x).hypot(leader.start.y - point.center.y)
                            - radius
                            - 2.0)
                            .abs()
                            < 0.001
                    );
                }
            }
        }
    }

    #[test]
    fn quadrant_overflow_keeps_unicode_and_expands_all_annotation_bounds() {
        let paragraph = "Deployment 東京 e\u{301}quipe 👩\u{200d}💻 review ".repeat(24);
        let heading =
            "Plan complicated international deployment after every customer readiness review "
                .repeat(12);
        let ir = fm_parser::parse(&format!(
            "quadrantChart\nquadrant-1 {heading}\n{paragraph}: [1.0, 1.0] radius: 48, stroke-width: 6px\nNearby release: [0.99, 0.99]\n",
        )).ir;
        let layout = crate::layout_diagram(&ir);
        let meta = ir.quadrant_meta.as_ref().unwrap();
        let metrics = FontMetrics::new(FontMetricsConfig {
            font_size: 32.0,
            ..Default::default()
        });
        let title = "A long identifying title that must remain entirely inside the exported frame";
        let chart =
            QuadrantChartLayout::new(&layout, meta, Some(title), (0.0, 0.0), &metrics, 8.0, 16.0);
        let compact = |value: &str| {
            value
                .chars()
                .filter(|ch| !ch.is_whitespace())
                .collect::<String>()
        };
        assert!(chart.labels[0].bounds.y > chart.plot.y + chart.plot.height);
        assert_eq!(
            compact(&chart.labels[0].lines.concat()),
            compact(&paragraph)
        );
        assert!(
            chart
                .headings
                .iter()
                .any(|heading| heading.bounds.y > chart.plot.y + chart.plot.height)
        );
        assert!(
            chart
                .headings
                .iter()
                .all(|heading| heading.quadrant_index == 0)
        );
        for rect in chart
            .headings
            .iter()
            .map(|heading| heading.bounds)
            .chain(chart.labels.iter().map(|label| label.bounds))
            .chain(chart.annotations.iter().map(|text| text.bounds))
        {
            assert!(contains(chart.bounds, rect));
        }
        let point = &chart.points[0];
        assert!(chart.bounds.x + chart.bounds.width >= point.center.x + 51.0);
        assert!(chart.bounds.y <= point.center.y - 51.0);
        assert!(
            chart.bounds.x < 0.0,
            "a wide centered title needs a left margin too"
        );
    }
}
