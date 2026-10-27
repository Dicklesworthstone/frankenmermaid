//! Shared radar geometry and measured identifying text.
//!
//! Axis captions and the legend reserve space without changing any plotted value.

use fm_core::{FontMetrics, FontMetricsConfig, IrRadarMeta, RadarGraticule};

use crate::{
    FillStyle, LayoutPoint, LayoutRadar, LayoutRect, MarkerKind, PathCmd, RenderClip, RenderGroup,
    RenderItem, RenderPath, RenderRect, RenderScene, RenderSource, RenderText, StrokeStyle,
    TextAlign, TextBaseline,
};

pub const RADAR_SERIES_COLORS: [&str; 6] = [
    "#8686ff", "#ffff78", "#d7ff86", "#ff86c8", "#86e0ff", "#ffc386",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarPathRole {
    Grid,
    Axis,
    Leader,
    Series(usize),
    Legend(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadarPath {
    pub role: RadarPathRole,
    pub path: RenderPath,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RadarTextRole {
    Axis(usize),
    Legend(usize),
    Title,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadarTextBlock {
    pub role: RadarTextRole,
    pub lines: Vec<String>,
    pub bounds: LayoutRect,
    pub font_size: f32,
    pub line_height: f32,
    pub align: TextAlign,
}

impl RadarTextBlock {
    pub fn rows(&self) -> impl Iterator<Item = (&str, LayoutPoint)> {
        let x = match self.align {
            TextAlign::Start => self.bounds.x,
            TextAlign::Middle => self.bounds.x + self.bounds.width * 0.5,
            TextAlign::End => self.bounds.x + self.bounds.width,
        };
        self.lines.iter().enumerate().map(move |(index, line)| {
            (
                line.as_str(),
                LayoutPoint {
                    x,
                    y: self.bounds.y + (index as f32 + 0.5) * self.line_height,
                },
            )
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RadarChartLayout {
    pub bounds: LayoutRect,
    pub paths: Vec<RadarPath>,
    pub text: Vec<RadarTextBlock>,
}

impl RadarChartLayout {
    #[must_use]
    pub fn new(
        meta: &IrRadarMeta,
        radar: &LayoutRadar,
        title: Option<&str>,
        metrics: &FontMetrics,
    ) -> Self {
        let radius = radar.outer_radius;
        let center = radar.center;
        let mut chart = Self {
            bounds: LayoutRect {
                x: center.x - radius - 2.0,
                y: center.y - radius - 2.0,
                width: radius * 2.0 + 4.0,
                height: radius * 2.0 + 4.0,
            },
            paths: Vec::new(),
            text: Vec::new(),
        };
        for &ring in &radar.rings {
            let commands = match meta.graticule {
                RadarGraticule::Circle => circle_path(center, ring),
                RadarGraticule::Polygon => {
                    let points: Vec<_> = radar
                        .axes
                        .iter()
                        .map(|axis| LayoutPoint {
                            x: ring.mul_add(axis.angle.cos(), center.x),
                            y: ring.mul_add(axis.angle.sin(), center.y),
                        })
                        .collect();
                    radar_series_path(&points, RadarGraticule::Polygon)
                }
            };
            chart.paths.push(RadarPath {
                role: RadarPathRole::Grid,
                path: path(commands, Some(("#dedede", 0.3)), "#dedede", 1.0),
            });
        }
        for (index, axis) in radar.axes.iter().enumerate() {
            chart.paths.push(RadarPath {
                role: RadarPathRole::Axis,
                path: path(line(center, axis.tip), None, "#7c8da5", 2.0),
            });
            let Some(declared) = meta.axes.get(index) else {
                continue;
            };
            let mut label = measured_block(
                declared.display(),
                RadarTextRole::Axis(index),
                metrics.font_size(),
                220.0 * metrics.font_size() / 15.0,
                metrics,
            );
            label.align = if axis.angle.cos() > 0.2 {
                TextAlign::Start
            } else if axis.angle.cos() < -0.2 {
                TextAlign::End
            } else {
                TextAlign::Middle
            };
            label.bounds.x = axis.label_anchor.x
                - match label.align {
                    TextAlign::Start => 0.0,
                    TextAlign::Middle => label.bounds.width * 0.5,
                    TextAlign::End => label.bounds.width,
                };
            label.bounds.y = axis.label_anchor.y - label.bounds.height * 0.5;
            chart.text.push(label);
        }
        // Adjacent captions stay separate without moving data. Moved captions retain
        // leaders back to the original spoke, so their association remains explicit.
        // Compare horizontal spans across the whole wheel: centered polar captions
        // can overlap captions on either side, even when their anchors do not.
        let mut indexes: Vec<_> = (0..chart.text.len()).collect();
        indexes.sort_by(|&a, &b| chart.text[a].bounds.y.total_cmp(&chart.text[b].bounds.y));
        for (order, &index) in indexes.iter().enumerate() {
            let original = chart.text[index].bounds;
            let bottom = indexes[..order]
                .iter()
                .map(|&prior| chart.text[prior].bounds)
                .filter(|prior| {
                    prior.x < original.x + original.width + 6.0
                        && original.x < prior.x + prior.width + 6.0
                })
                .map(|prior| prior.y + prior.height)
                .fold(f32::NEG_INFINITY, f32::max);
            let label = &mut chart.text[index];
            label.bounds.y = original.y.max(bottom + 6.0);
            if label.bounds.y > original.y + 1.0 {
                let RadarTextRole::Axis(axis) = label.role else {
                    continue;
                };
                let tip = radar.axes[axis].tip;
                let target = LayoutPoint {
                    x: tip
                        .x
                        .clamp(label.bounds.x, label.bounds.x + label.bounds.width),
                    y: tip
                        .y
                        .clamp(label.bounds.y, label.bounds.y + label.bounds.height),
                };
                chart.paths.push(RadarPath {
                    role: RadarPathRole::Leader,
                    path: path(line(tip, target), None, "#7c8da5", 0.75),
                });
            }
            include(&mut chart.bounds, label.bounds);
        }
        for curve in &radar.curves {
            if curve.points.is_empty() {
                continue;
            }
            let color = RADAR_SERIES_COLORS[curve.curve % RADAR_SERIES_COLORS.len()];
            let commands = radar_series_path(&curve.points, meta.graticule);
            // A cubic lies within its endpoint/control hull, including radial overshoot.
            for command in &commands {
                for point in command_points(command).into_iter().flatten() {
                    include(
                        &mut chart.bounds,
                        LayoutRect {
                            x: point.x - 2.0,
                            y: point.y - 2.0,
                            width: 4.0,
                            height: 4.0,
                        },
                    );
                }
            }
            chart.paths.push(RadarPath {
                role: RadarPathRole::Series(curve.curve),
                path: path(commands, Some((color, 0.5)), color, 2.0),
            });
        }
        if meta.show_legend {
            let x = chart.bounds.x + chart.bounds.width + 28.0;
            let mut y = center.y - radius;
            for (index, curve) in meta.curves.iter().enumerate() {
                let mut label = measured_block(
                    curve.display(),
                    RadarTextRole::Legend(index),
                    metrics.font_size(),
                    260.0 * metrics.font_size() / 15.0,
                    metrics,
                );
                let swatch = metrics.font_size().max(12.0);
                label.bounds.x = x + swatch + 8.0;
                label.bounds.y = y;
                let rect = LayoutRect {
                    x,
                    y: y + (label.line_height - swatch) * 0.5,
                    width: swatch,
                    height: swatch,
                };
                let color = RADAR_SERIES_COLORS[index % RADAR_SERIES_COLORS.len()];
                chart.paths.push(RadarPath {
                    role: RadarPathRole::Legend(index),
                    path: path(
                        crate::shapes::rounded_rect_path(rect, 0.0),
                        Some((color, 0.5)),
                        color,
                        1.0,
                    ),
                });
                include(&mut chart.bounds, rect);
                include(&mut chart.bounds, label.bounds);
                y += label.bounds.height.max(swatch) + 10.0;
                chart.text.push(label);
            }
        }
        if let Some(title) = title.filter(|title| !title.is_empty()) {
            let mut label = measured_block(
                title,
                RadarTextRole::Title,
                metrics.font_size() + 4.0,
                chart.bounds.width,
                metrics,
            );
            label.align = TextAlign::Middle;
            label.bounds.x = chart.bounds.x + (chart.bounds.width - label.bounds.width) * 0.5;
            label.bounds.y = chart.bounds.y - label.bounds.height - 16.0;
            include(&mut chart.bounds, label.bounds);
            chart.text.push(label);
        }
        chart
    }

    /// Materialize paths and rows using the surface's resolved theme colors.
    #[must_use]
    pub fn to_scene(&self, label_color: &str, axis_color: &str) -> RenderScene {
        let mut root = RenderGroup::new(Some("radar-chart".to_owned()));
        root.clip = Some(RenderClip::Rect(RenderRect::from(self.bounds)));
        // Keep the quantitative fills behind every outline, matching the GPU's triangle
        // and stroke passes. A later translucent series must not fade an earlier outline.
        for fills in [true, false] {
            for entry in &self.paths {
                let mut path = entry.path.clone();
                if fills {
                    if path.fill.is_none() {
                        continue;
                    }
                    path.stroke = None;
                } else {
                    if path.stroke.is_none() {
                        continue;
                    }
                    path.fill = None;
                }
                if matches!(entry.role, RadarPathRole::Axis | RadarPathRole::Leader)
                    && let Some(stroke) = &mut path.stroke
                {
                    stroke.color = axis_color.to_owned();
                }
                root.children.push(RenderItem::Path(path));
            }
        }
        for block in &self.text {
            for (text, point) in block.rows() {
                root.children.push(RenderItem::Text(RenderText {
                    source: RenderSource::Diagram,
                    text: text.to_owned(),
                    x: point.x,
                    y: point.y,
                    font_size: block.font_size,
                    align: block.align,
                    baseline: TextBaseline::Middle,
                    fill: FillStyle::Solid {
                        color: label_color.to_owned(),
                        opacity: 1.0,
                    },
                }));
            }
        }
        RenderScene {
            bounds: RenderRect::from(self.bounds),
            root,
        }
    }
}

fn measured_block(
    text: &str,
    role: RadarTextRole,
    font_size: f32,
    max_width: f32,
    metrics: &FontMetrics,
) -> RadarTextBlock {
    let row_metrics = FontMetrics::new(FontMetricsConfig {
        font_size,
        ..metrics.config().clone()
    });
    let lines = row_metrics.wrap_preserved_to_width(text, max_width / 1.25);
    let width = lines
        .iter()
        .map(|line| metrics.estimate_preserved_width(line, font_size, metrics.avg_char_width()))
        .fold(0.0_f32, f32::max);
    let line_height = row_metrics.line_height_px().max(font_size * 1.3);
    RadarTextBlock {
        role,
        bounds: LayoutRect {
            x: 0.0,
            y: 0.0,
            width,
            height: lines.len() as f32 * line_height,
        },
        lines,
        font_size,
        line_height,
        align: TextAlign::Start,
    }
}

fn include(bounds: &mut LayoutRect, rect: LayoutRect) {
    let right = (bounds.x + bounds.width).max(rect.x + rect.width);
    let bottom = (bounds.y + bounds.height).max(rect.y + rect.height);
    bounds.x = bounds.x.min(rect.x);
    bounds.y = bounds.y.min(rect.y);
    bounds.width = right - bounds.x;
    bounds.height = bottom - bounds.y;
}

fn path(commands: Vec<PathCmd>, fill: Option<(&str, f32)>, stroke: &str, width: f32) -> RenderPath {
    RenderPath {
        source: RenderSource::Diagram,
        commands,
        fill: fill.map(|(color, opacity)| FillStyle::Solid {
            color: color.to_owned(),
            opacity,
        }),
        stroke: Some(StrokeStyle::solid(stroke, width)),
        marker_start: MarkerKind::None,
        marker_end: MarkerKind::None,
    }
}

fn line(from: LayoutPoint, to: LayoutPoint) -> Vec<PathCmd> {
    vec![
        PathCmd::MoveTo {
            x: from.x,
            y: from.y,
        },
        PathCmd::LineTo { x: to.x, y: to.y },
    ]
}

fn circle_path(center: LayoutPoint, radius: f32) -> Vec<PathCmd> {
    let k = radius * 0.552_284_8;
    let (x, y) = (center.x, center.y);
    vec![
        PathCmd::MoveTo { x: x + radius, y },
        PathCmd::CubicTo {
            c1x: x + radius,
            c1y: y + k,
            c2x: x + k,
            c2y: y + radius,
            x,
            y: y + radius,
        },
        PathCmd::CubicTo {
            c1x: x - k,
            c1y: y + radius,
            c2x: x - radius,
            c2y: y + k,
            x: x - radius,
            y,
        },
        PathCmd::CubicTo {
            c1x: x - radius,
            c1y: y - k,
            c2x: x - k,
            c2y: y - radius,
            x,
            y: y - radius,
        },
        PathCmd::CubicTo {
            c1x: x + k,
            c1y: y - radius,
            c2x: x + radius,
            c2y: y - k,
            x: x + radius,
            y,
        },
        PathCmd::Close,
    ]
}

/// Closed cardinal splines in circle mode; polygon mode retains straight edges.
#[must_use]
pub fn radar_series_path(points: &[LayoutPoint], graticule: RadarGraticule) -> Vec<PathCmd> {
    let Some(first) = points.first() else {
        return Vec::new();
    };
    let mut commands = vec![PathCmd::MoveTo {
        x: first.x,
        y: first.y,
    }];
    if points.len() < 3 || graticule == RadarGraticule::Polygon {
        commands.extend(points[1..].iter().map(|point| PathCmd::LineTo {
            x: point.x,
            y: point.y,
        }));
    } else {
        // The existing SVG renderer's measured cardinal tension, shared by all consumers.
        const TENSION: f32 = 0.17;
        let count = points.len();
        for index in 0..count {
            let previous = points[(index + count - 1) % count];
            let current = points[index];
            let next = points[(index + 1) % count];
            let after = points[(index + 2) % count];
            commands.push(PathCmd::CubicTo {
                c1x: (next.x - previous.x).mul_add(TENSION, current.x),
                c1y: (next.y - previous.y).mul_add(TENSION, current.y),
                c2x: (after.x - current.x).mul_add(-TENSION, next.x),
                c2y: (after.y - current.y).mul_add(-TENSION, next.y),
                x: next.x,
                y: next.y,
            });
        }
    }
    commands.push(PathCmd::Close);
    commands
}

fn command_points(command: &PathCmd) -> [Option<LayoutPoint>; 3] {
    match *command {
        PathCmd::MoveTo { x, y } | PathCmd::LineTo { x, y } => {
            [Some(LayoutPoint { x, y }), None, None]
        }
        PathCmd::CubicTo {
            c1x,
            c1y,
            c2x,
            c2y,
            x,
            y,
        } => [
            Some(LayoutPoint { x: c1x, y: c1y }),
            Some(LayoutPoint { x: c2x, y: c2y }),
            Some(LayoutPoint { x, y }),
        ],
        PathCmd::QuadTo { cx, cy, x, y } => [
            Some(LayoutPoint { x: cx, y: cy }),
            Some(LayoutPoint { x, y }),
            None,
        ],
        PathCmd::Close => [None, None, None],
    }
}
