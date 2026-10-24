//! Shared connector geometry for SVG, Canvas, scenes, and GPU plans.
//!
//! Smoothing belongs after routing: it may soften a corner, but it must not introduce the
//! large excursions that an unconstrained spline can make outside a routed corridor.

use fm_core::DiagramType;

use crate::{LayoutPoint, PathCmd};

/// A drawable endpoint and its forward path tangent. The start tangent points into the path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathEndpoint {
    pub point: LayoutPoint,
    pub tangent: LayoutPoint,
}

/// Endpoints of the emitted geometry, after redundant points or a tiny jog are removed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathEndpoints {
    pub start: PathEndpoint,
    pub end: PathEndpoint,
}

/// Read drawable endpoints from an existing scene path using the same tangent rules as routing.
/// Repeated points and coincident controls do not invent a direction. A closing segment returns
/// to its subpath's start; a trailing move alone cannot replace the last drawable endpoint.
#[must_use]
pub fn path_endpoints(commands: &[PathCmd]) -> Option<PathEndpoints> {
    if !matches!(commands.first(), Some(PathCmd::MoveTo { .. })) {
        return None;
    }
    let mut tracker = EndpointTracker::default();
    for &command in commands {
        tracker.observe(command);
    }
    tracker.endpoints
}

/// Graph connectors share the smoothed presentation. Specialized chart paths and sequence
/// messages keep their authored/routed polylines, including rectangular sequence self-messages.
#[must_use]
pub const fn uses_smoothed_edge_paths(diagram_type: DiagramType) -> bool {
    matches!(
        diagram_type,
        DiagramType::Flowchart
            | DiagramType::Class
            | DiagramType::State
            | DiagramType::Er
            | DiagramType::Requirement
            | DiagramType::Mindmap
            | DiagramType::C4Context
            | DiagramType::C4Container
            | DiagramType::C4Component
            | DiagramType::C4Dynamic
            | DiagramType::C4Deployment
            | DiagramType::ArchitectureBeta
            | DiagramType::BlockBeta
    )
}

/// Emit the diagram family's connector path without allocating an intermediate command vector.
/// Returns no endpoints when the route has no finite, nonzero drawable segment.
pub fn emit_edge_path<F>(
    diagram_type: DiagramType,
    points: &[LayoutPoint],
    mut emit: F,
) -> Option<PathEndpoints>
where
    F: FnMut(PathCmd),
{
    if uses_smoothed_edge_paths(diagram_type) {
        return emit_smoothed_path(
            points.len(),
            |index| (points[index].x, points[index].y),
            emit,
        );
    }

    let mut endpoints = EndpointTracker::default();
    for (index, point) in points.iter().enumerate() {
        let command = if index == 0 {
            PathCmd::MoveTo {
                x: point.x,
                y: point.y,
            }
        } else {
            PathCmd::LineTo {
                x: point.x,
                y: point.y,
            }
        };
        endpoints.observe(command);
        emit(command);
    }
    endpoints.endpoints
}

/// Emit bounded rounded elbows for orthogonal routes and bounded Catmull-Rom cubics for
/// other routes. The point accessor lets streaming renderers apply their origin without copying
/// the route. This is also the SVG serializer's geometry, preserving its established output.
pub fn emit_smoothed_path<F, E>(count: usize, point_at: F, mut emit: E) -> Option<PathEndpoints>
where
    F: FnMut(usize) -> (f32, f32),
    E: FnMut(PathCmd),
{
    let mut endpoints = EndpointTracker::default();
    emit_smoothed_commands(count, point_at, |command| {
        endpoints.observe(command);
        emit(command);
    });
    endpoints.endpoints
}

fn emit_smoothed_commands<F, E>(count: usize, mut point_at: F, mut emit: E)
where
    F: FnMut(usize) -> (f32, f32),
    E: FnMut(PathCmd),
{
    if count == 0 {
        return;
    }
    let first = point_at(0);
    emit(PathCmd::MoveTo {
        x: first.0,
        y: first.1,
    });
    if count == 1 {
        return;
    }
    if count == 2 {
        let second = point_at(1);
        emit(PathCmd::LineTo {
            x: second.0,
            y: second.1,
        });
        return;
    }

    // Collapse tiny lateral jogs, but never shortcut a route that backtracks past its endpoints.
    const STRAIGHT_EPS: f32 = 2.0;
    let last = point_at(count - 1);
    let chord = (last.0 - first.0, last.1 - first.1);
    let chord_len = (chord.0 * chord.0 + chord.1 * chord.1).sqrt();
    if chord_len > f32::EPSILON {
        let mut straight = true;
        for index in 1..(count - 1) {
            let point = point_at(index);
            let deviation =
                ((point.0 - first.0) * chord.1 - (point.1 - first.1) * chord.0).abs() / chord_len;
            let along = ((point.0 - first.0) * chord.0 + (point.1 - first.1) * chord.1) / chord_len;
            if deviation > STRAIGHT_EPS || along < -STRAIGHT_EPS || along > chord_len + STRAIGHT_EPS
            {
                straight = false;
                break;
            }
        }
        if straight {
            emit(PathCmd::LineTo {
                x: last.0,
                y: last.1,
            });
            return;
        }
    }

    let mut previous = first;
    let orthogonal = (1..count).all(|index| {
        let point = point_at(index);
        let aligned = point.0 == previous.0 || point.1 == previous.1;
        previous = point;
        aligned && point.0.is_finite() && point.1.is_finite()
    });
    if orthogonal && first.0.is_finite() && first.1.is_finite() {
        emit_rounded_orthogonal_path(count, &mut point_at, &mut emit);
        return;
    }

    let tension: f32 = 0.25;
    for index in 0..(count - 1) {
        let previous = point_at(index.saturating_sub(1));
        let current = point_at(index);
        let next = point_at(index + 1);
        let following = point_at((index + 2).min(count - 1));
        let (lo_x, hi_x) = if current.0 <= next.0 {
            (current.0, next.0)
        } else {
            (next.0, current.0)
        };
        let (lo_y, hi_y) = if current.1 <= next.1 {
            (current.1, next.1)
        } else {
            (next.1, current.1)
        };
        // The convex hull of these controls stays inside the current segment's bounds.
        emit(PathCmd::CubicTo {
            c1x: (current.0 + (next.0 - previous.0) * tension).clamp(lo_x, hi_x),
            c1y: (current.1 + (next.1 - previous.1) * tension).clamp(lo_y, hi_y),
            c2x: (next.0 - (following.0 - current.0) * tension).clamp(lo_x, hi_x),
            c2y: (next.1 - (following.1 - current.1) * tension).clamp(lo_y, hi_y),
            x: next.0,
            y: next.1,
        });
    }
}

fn emit_rounded_orthogonal_path<F, E>(count: usize, point_at: &mut F, emit: &mut E)
where
    F: FnMut(usize) -> (f32, f32),
    E: FnMut(PathCmd),
{
    // A quadratic with this trim strays at most 1.5px from the original L-shaped centerline,
    // within the obstacle router's 8px clearance. Half-segment caps leave terminal marker stubs.
    const CORNER_TRIM: f32 = 6.0;
    let mut previous = point_at(0);
    let mut cursor = previous;
    let mut index = 1;
    let Some(mut corner) = next_orthogonal_corner(previous, count, &mut index, point_at) else {
        return;
    };
    while let Some(next) = next_orthogonal_corner(corner, count, &mut index, point_at) {
        let incoming = (corner.0 - previous.0, corner.1 - previous.1);
        let outgoing = (next.0 - corner.0, next.1 - corner.1);
        if (incoming.0 == 0.0) != (outgoing.0 == 0.0) {
            let incoming_length = incoming.0.abs() + incoming.1.abs();
            let outgoing_length = outgoing.0.abs() + outgoing.1.abs();
            let trim = CORNER_TRIM
                .min(incoming_length * 0.5)
                .min(outgoing_length * 0.5);
            let entry = (
                corner.0 - incoming.0 / incoming_length * trim,
                corner.1 - incoming.1 / incoming_length * trim,
            );
            let exit = (
                corner.0 + outgoing.0 / outgoing_length * trim,
                corner.1 + outgoing.1 / outgoing_length * trim,
            );
            if entry != cursor {
                emit(PathCmd::LineTo {
                    x: entry.0,
                    y: entry.1,
                });
            }
            emit(PathCmd::QuadTo {
                cx: corner.0,
                cy: corner.1,
                x: exit.0,
                y: exit.1,
            });
            cursor = exit;
        } else {
            // A 180-degree reversal is a routed feature, not a corner to round away.
            emit(PathCmd::LineTo {
                x: corner.0,
                y: corner.1,
            });
            cursor = corner;
        }
        previous = corner;
        corner = next;
    }
    if corner != cursor {
        emit(PathCmd::LineTo {
            x: corner.0,
            y: corner.1,
        });
    }
}

fn next_orthogonal_corner<F>(
    origin: (f32, f32),
    count: usize,
    index: &mut usize,
    point_at: &mut F,
) -> Option<(f32, f32)>
where
    F: FnMut(usize) -> (f32, f32),
{
    let mut candidate = origin;
    while *index < count {
        let next = point_at(*index);
        if candidate != origin && next != candidate {
            let incoming = (candidate.0 - origin.0, candidate.1 - origin.1);
            let outgoing = (next.0 - candidate.0, next.1 - candidate.1);
            if (incoming.0 == 0.0) != (outgoing.0 == 0.0)
                || incoming.0 * outgoing.0 + incoming.1 * outgoing.1 <= 0.0
            {
                break;
            }
        }
        candidate = next;
        *index += 1;
    }
    (candidate != origin).then_some(candidate)
}

#[derive(Default)]
struct EndpointTracker {
    current: Option<LayoutPoint>,
    subpath_start: Option<LayoutPoint>,
    endpoints: Option<PathEndpoints>,
}

impl EndpointTracker {
    fn observe(&mut self, command: PathCmd) {
        let (end, controls) = match command {
            PathCmd::MoveTo { x, y } => {
                self.current = Some(LayoutPoint { x, y });
                self.subpath_start = self.current;
                return;
            }
            PathCmd::LineTo { x, y } => (LayoutPoint { x, y }, [None, None]),
            PathCmd::QuadTo { cx, cy, x, y } => {
                let control = Some(LayoutPoint { x: cx, y: cy });
                (LayoutPoint { x, y }, [control, control])
            }
            PathCmd::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } => (
                LayoutPoint { x, y },
                [
                    Some(LayoutPoint { x: c1x, y: c1y }),
                    Some(LayoutPoint { x: c2x, y: c2y }),
                ],
            ),
            PathCmd::Close => {
                let Some(end) = self.subpath_start else {
                    return;
                };
                (end, [None, None])
            }
        };
        let start = self.current.replace(end);
        let Some(start) = start else { return };
        let start_direction = controls
            .into_iter()
            .flatten()
            .chain([end])
            .find_map(|point| direction(start, point));
        let end_direction = controls
            .into_iter()
            .rev()
            .flatten()
            .chain([start])
            .find_map(|point| direction(point, end));
        let (Some(start_tangent), Some(end_tangent)) = (start_direction, end_direction) else {
            return;
        };
        let end = PathEndpoint {
            point: end,
            tangent: end_tangent,
        };
        match self.endpoints.as_mut() {
            Some(endpoints) => endpoints.end = end,
            None => {
                self.endpoints = Some(PathEndpoints {
                    start: PathEndpoint {
                        point: start,
                        tangent: start_tangent,
                    },
                    end,
                })
            }
        }
    }
}

fn direction(from: LayoutPoint, to: LayoutPoint) -> Option<LayoutPoint> {
    let tangent = LayoutPoint {
        x: to.x - from.x,
        y: to.y - from.y,
    };
    (tangent.x.is_finite() && tangent.y.is_finite() && (tangent.x != 0.0 || tangent.y != 0.0))
        .then_some(tangent)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn route(points: &[(f32, f32)], kind: DiagramType) -> (Vec<PathCmd>, Option<PathEndpoints>) {
        let points: Vec<_> = points.iter().map(|&(x, y)| LayoutPoint { x, y }).collect();
        let mut commands = Vec::new();
        let endpoints = emit_edge_path(kind, &points, |command| commands.push(command));
        (commands, endpoints)
    }

    #[test]
    fn scene_endpoint_tangents_skip_zero_segments_and_use_remaining_controls() {
        let mut commands = vec![
            PathCmd::MoveTo { x: 0.0, y: 0.0 },
            PathCmd::CubicTo {
                c1x: 0.0,
                c1y: 0.0,
                c2x: 0.0,
                c2y: 0.0,
                x: 0.0,
                y: 0.0,
            },
            PathCmd::CubicTo {
                c1x: 0.0,
                c1y: 0.0,
                c2x: 10.0,
                c2y: 30.0,
                x: 40.0,
                y: 60.0,
            },
            PathCmd::CubicTo {
                c1x: 70.0,
                c1y: 90.0,
                c2x: 100.0,
                c2y: 100.0,
                x: 100.0,
                y: 100.0,
            },
            PathCmd::CubicTo {
                c1x: 100.0,
                c1y: 100.0,
                c2x: 100.0,
                c2y: 100.0,
                x: 100.0,
                y: 100.0,
            },
        ];
        let endpoints = path_endpoints(&commands).unwrap();
        assert_eq!(endpoints.start.point, LayoutPoint { x: 0.0, y: 0.0 });
        assert_eq!(endpoints.start.tangent, LayoutPoint { x: 10.0, y: 30.0 });
        assert_eq!(endpoints.end.point, LayoutPoint { x: 100.0, y: 100.0 });
        assert_eq!(endpoints.end.tangent, LayoutPoint { x: 30.0, y: 10.0 });
        commands.push(PathCmd::Close);
        let closed = path_endpoints(&commands).unwrap();
        assert_eq!(closed.start, endpoints.start);
        assert_eq!(closed.end.point, LayoutPoint { x: 0.0, y: 0.0 });
        assert_eq!(
            closed.end.tangent,
            LayoutPoint {
                x: -100.0,
                y: -100.0
            }
        );
        commands.push(PathCmd::MoveTo { x: 900.0, y: 800.0 });
        assert_eq!(path_endpoints(&commands), Some(closed));
    }

    #[test]
    fn rounded_graph_paths_keep_port_stubs_and_terminal_tangents() {
        let (commands, endpoints) = route(
            &[(0.0, 0.0), (4.0, 0.0), (4.0, 12.0), (8.0, 12.0)],
            DiagramType::Flowchart,
        );
        assert_eq!(
            commands,
            vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::LineTo { x: 2.0, y: 0.0 },
                PathCmd::QuadTo {
                    cx: 4.0,
                    cy: 0.0,
                    x: 4.0,
                    y: 2.0
                },
                PathCmd::LineTo { x: 4.0, y: 10.0 },
                PathCmd::QuadTo {
                    cx: 4.0,
                    cy: 12.0,
                    x: 6.0,
                    y: 12.0
                },
                PathCmd::LineTo { x: 8.0, y: 12.0 },
            ]
        );
        let endpoints = endpoints.unwrap();
        assert_eq!(endpoints.start.point, LayoutPoint { x: 0.0, y: 0.0 });
        assert_eq!(endpoints.end.point, LayoutPoint { x: 8.0, y: 12.0 });
        assert_eq!(endpoints.start.tangent, LayoutPoint { x: 2.0, y: 0.0 });
        assert_eq!(endpoints.end.tangent, LayoutPoint { x: 2.0, y: 0.0 });
    }

    #[test]
    fn endpoint_tangents_follow_the_collapsed_path_and_ignore_repeated_points() {
        let (_, endpoints) = route(
            &[
                (0.0, 0.0),
                (0.0, 0.0),
                (40.0, 0.0),
                (40.0, 1.0),
                (120.0, 1.0),
                (120.0, 1.0),
            ],
            DiagramType::Flowchart,
        );
        let endpoints = endpoints.unwrap();
        assert_eq!(endpoints.start.tangent, LayoutPoint { x: 120.0, y: 1.0 });
        assert_eq!(endpoints.end.tangent, endpoints.start.tangent);
        for kind in [DiagramType::Flowchart, DiagramType::Sequence] {
            assert!(route(&[(5.0, 9.0); 2], kind).1.is_none());
            assert!(route(&[(5.0, 9.0); 4], kind).1.is_none());
        }
        let (_, endpoints) = route(
            &[(0.0, 0.0), (0.0, 0.0), (0.0, 40.0), (0.0, 40.0)],
            DiagramType::Sequence,
        );
        let endpoints = endpoints.unwrap();
        assert_eq!(endpoints.start.tangent, LayoutPoint { x: 0.0, y: 40.0 });
        assert_eq!(endpoints.end.tangent, endpoints.start.tangent);
    }

    #[test]
    fn sequence_self_messages_and_chart_paths_keep_exact_polylines() {
        let points = [(0.0, 0.0), (40.0, 0.0), (40.0, 30.0), (0.0, 30.0)];
        for kind in [
            DiagramType::Sequence,
            DiagramType::XyChart,
            DiagramType::GitGraph,
            DiagramType::Sankey,
        ] {
            let (commands, _) = route(&points, kind);
            assert_eq!(commands.len(), points.len());
            assert!(
                commands[1..]
                    .iter()
                    .all(|command| matches!(command, PathCmd::LineTo { .. }))
            );
        }
    }

    #[test]
    fn cubic_controls_cannot_escape_the_routed_segment_bounds() {
        let points = [(10.0, 20.0), (30.0, 60.0), (70.0, 60.0), (90.0, 20.0)];
        let (commands, _) = route(&points, DiagramType::Flowchart);
        for (command, pair) in commands.iter().skip(1).zip(points.windows(2)) {
            let PathCmd::CubicTo {
                c1x,
                c1y,
                c2x,
                c2y,
                x,
                y,
            } = *command
            else {
                panic!("expected a cubic for a diagonal route")
            };
            assert_eq!((x, y), pair[1]);
            for (cx, cy) in [(c1x, c1y), (c2x, c2y)] {
                assert!((pair[0].0.min(pair[1].0)..=pair[0].0.max(pair[1].0)).contains(&cx));
                assert!((pair[0].1.min(pair[1].1)..=pair[0].1.max(pair[1].1)).contains(&cy));
            }
        }
    }
}
