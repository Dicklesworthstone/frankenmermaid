//! CGA-based intersection queries for edge routing.
//!
//! This module provides intersection detection using Conformal Geometric Algebra,
//! replacing manual AABB checks with proper segment-rectangle intersection tests.

use fm_core::cga::{CgaLineSegment, CgaPoint, CgaRect};

use crate::{LayoutPoint, LayoutRect};

/// Convert a LayoutPoint to a CgaPoint.
#[inline]
fn to_cga_point(p: LayoutPoint) -> CgaPoint {
    CgaPoint::new(f64::from(p.x), f64::from(p.y))
}

/// Convert a LayoutRect to a CgaRect.
#[inline]
fn to_cga_rect(r: LayoutRect) -> CgaRect {
    CgaRect::new(
        f64::from(r.x),
        f64::from(r.y),
        f64::from(r.width),
        f64::from(r.height),
    )
}

/// Check if a line segment intersects any obstacle rectangle.
///
/// Returns true if the segment crosses into any obstacle's interior.
#[must_use]
pub fn segment_intersects_obstacles(
    start: LayoutPoint,
    end: LayoutPoint,
    obstacles: &[LayoutRect],
    margin: f32,
) -> bool {
    let seg = CgaLineSegment::new(to_cga_point(start), to_cga_point(end));
    let margin_f64 = f64::from(margin);

    for obs in obstacles {
        // Expand obstacle by margin
        let expanded = CgaRect::new(
            f64::from(obs.x) - margin_f64,
            f64::from(obs.y) - margin_f64,
            f64::from(obs.width) + 2.0 * margin_f64,
            f64::from(obs.height) + 2.0 * margin_f64,
        );

        if !expanded.intersect_segment(&seg).is_empty() {
            return true;
        }
    }
    false
}

/// Find the first obstacle that a segment intersects.
///
/// Returns the obstacle index and intersection points, or None if no intersection.
#[must_use]
pub fn find_first_obstacle_intersection(
    start: LayoutPoint,
    end: LayoutPoint,
    obstacles: &[LayoutRect],
    margin: f32,
) -> Option<(usize, Vec<LayoutPoint>)> {
    let seg = CgaLineSegment::new(to_cga_point(start), to_cga_point(end));
    let margin_f64 = f64::from(margin);

    for (i, obs) in obstacles.iter().enumerate() {
        let expanded = CgaRect::new(
            f64::from(obs.x) - margin_f64,
            f64::from(obs.y) - margin_f64,
            f64::from(obs.width) + 2.0 * margin_f64,
            f64::from(obs.height) + 2.0 * margin_f64,
        );

        let intersections = expanded.intersect_segment(&seg);
        if !intersections.is_empty() {
            let points = intersections
                .into_iter()
                .map(|p| LayoutPoint {
                    x: p.x as f32,
                    y: p.y as f32,
                })
                .collect();
            return Some((i, points));
        }
    }
    None
}

/// Check if a vertical segment at `x` intersects an obstacle and return nudge.
///
/// This is a CGA-based replacement for the manual AABB check.
/// Returns the x-coordinate to nudge to if intersection found.
#[must_use]
pub fn find_vertical_segment_nudge(
    segment_start: LayoutPoint,
    segment_end: LayoutPoint,
    obstacles: &[LayoutRect],
    margin: f32,
) -> Option<f32> {
    let seg = CgaLineSegment::new(to_cga_point(segment_start), to_cga_point(segment_end));
    let margin_f64 = f64::from(margin);
    let mid_x = f64::from(segment_start.x);

    for obs in obstacles {
        let expanded = CgaRect::new(
            f64::from(obs.x) - margin_f64,
            f64::from(obs.y) - margin_f64,
            f64::from(obs.width) + 2.0 * margin_f64,
            f64::from(obs.height) + 2.0 * margin_f64,
        );

        if !expanded.intersect_segment(&seg).is_empty() {
            // Nudge to closer side
            let left = f64::from(obs.x) - margin_f64;
            let right = f64::from(obs.x) + f64::from(obs.width) + margin_f64;
            let left_dist = (mid_x - left).abs();
            let right_dist = (mid_x - right).abs();

            return if left_dist <= right_dist {
                Some(left as f32)
            } else {
                Some(right as f32)
            };
        }
    }
    None
}

/// Check if a horizontal segment at `y` intersects an obstacle and return nudge.
///
/// This is a CGA-based replacement for the manual AABB check.
/// Returns the y-coordinate to nudge to if intersection found.
#[must_use]
pub fn find_horizontal_segment_nudge(
    segment_start: LayoutPoint,
    segment_end: LayoutPoint,
    obstacles: &[LayoutRect],
    margin: f32,
) -> Option<f32> {
    let seg = CgaLineSegment::new(to_cga_point(segment_start), to_cga_point(segment_end));
    let margin_f64 = f64::from(margin);
    let mid_y = f64::from(segment_start.y);

    for obs in obstacles {
        let expanded = CgaRect::new(
            f64::from(obs.x) - margin_f64,
            f64::from(obs.y) - margin_f64,
            f64::from(obs.width) + 2.0 * margin_f64,
            f64::from(obs.height) + 2.0 * margin_f64,
        );

        if !expanded.intersect_segment(&seg).is_empty() {
            // Nudge to closer side
            let top = f64::from(obs.y) - margin_f64;
            let bottom = f64::from(obs.y) + f64::from(obs.height) + margin_f64;
            let top_dist = (mid_y - top).abs();
            let bottom_dist = (mid_y - bottom).abs();

            return if top_dist <= bottom_dist {
                Some(top as f32)
            } else {
                Some(bottom as f32)
            };
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_misses_obstacle() {
        let obs = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let start = LayoutPoint { x: 0.0, y: 0.0 };
        let end = LayoutPoint { x: 5.0, y: 5.0 };
        assert!(!segment_intersects_obstacles(start, end, &[obs], 0.0));
    }

    #[test]
    fn segment_hits_obstacle() {
        let obs = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        let start = LayoutPoint { x: 0.0, y: 15.0 };
        let end = LayoutPoint { x: 50.0, y: 15.0 };
        assert!(segment_intersects_obstacles(start, end, &[obs], 0.0));
    }

    #[test]
    fn segment_misses_with_margin() {
        let obs = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 20.0,
            height: 20.0,
        };
        // Segment passes just above obstacle
        let start = LayoutPoint { x: 0.0, y: 5.0 };
        let end = LayoutPoint { x: 50.0, y: 5.0 };
        assert!(!segment_intersects_obstacles(start, end, &[obs], 0.0));
        // But with margin of 6, it should hit the expanded box
        assert!(segment_intersects_obstacles(start, end, &[obs], 6.0));
    }

    #[test]
    fn vertical_nudge_left() {
        let obs = LayoutRect {
            x: 20.0,
            y: 0.0,
            width: 10.0,
            height: 20.0,
        };
        // Vertical segment at x=22 passes through obstacle
        let start = LayoutPoint { x: 22.0, y: -5.0 };
        let end = LayoutPoint { x: 22.0, y: 25.0 };
        let nudge = find_vertical_segment_nudge(start, end, &[obs], 0.0);
        assert!(nudge.is_some());
        // Should nudge left to x=20 since that's closer
        assert!((nudge.unwrap() - 20.0).abs() < 0.01);
    }

    #[test]
    fn vertical_nudge_right() {
        let obs = LayoutRect {
            x: 20.0,
            y: 0.0,
            width: 10.0,
            height: 20.0,
        };
        // Vertical segment at x=28 passes through obstacle
        let start = LayoutPoint { x: 28.0, y: -5.0 };
        let end = LayoutPoint { x: 28.0, y: 25.0 };
        let nudge = find_vertical_segment_nudge(start, end, &[obs], 0.0);
        assert!(nudge.is_some());
        // Should nudge right to x=30 since that's closer
        assert!((nudge.unwrap() - 30.0).abs() < 0.01);
    }

    #[test]
    fn horizontal_nudge_top() {
        let obs = LayoutRect {
            x: 0.0,
            y: 20.0,
            width: 20.0,
            height: 10.0,
        };
        // Horizontal segment at y=22 passes through obstacle
        let start = LayoutPoint { x: -5.0, y: 22.0 };
        let end = LayoutPoint { x: 25.0, y: 22.0 };
        let nudge = find_horizontal_segment_nudge(start, end, &[obs], 0.0);
        assert!(nudge.is_some());
        // Should nudge to top edge y=20 since that's closer
        assert!((nudge.unwrap() - 20.0).abs() < 0.01);
    }

    #[test]
    fn horizontal_nudge_bottom() {
        let obs = LayoutRect {
            x: 0.0,
            y: 20.0,
            width: 20.0,
            height: 10.0,
        };
        // Horizontal segment at y=28 passes through obstacle
        let start = LayoutPoint { x: -5.0, y: 28.0 };
        let end = LayoutPoint { x: 25.0, y: 28.0 };
        let nudge = find_horizontal_segment_nudge(start, end, &[obs], 0.0);
        assert!(nudge.is_some());
        // Should nudge to bottom edge y=30 since that's closer
        assert!((nudge.unwrap() - 30.0).abs() < 0.01);
    }

    #[test]
    fn find_first_intersection_returns_index() {
        let obstacles = vec![
            LayoutRect {
                x: 10.0,
                y: 10.0,
                width: 10.0,
                height: 10.0,
            },
            LayoutRect {
                x: 50.0,
                y: 10.0,
                width: 10.0,
                height: 10.0,
            },
        ];
        let start = LayoutPoint { x: 45.0, y: 15.0 };
        let end = LayoutPoint { x: 70.0, y: 15.0 };
        let result = find_first_obstacle_intersection(start, end, &obstacles, 0.0);
        assert!(result.is_some());
        let (idx, points) = result.unwrap();
        assert_eq!(idx, 1); // Second obstacle
        assert_eq!(points.len(), 2); // Entry and exit points
    }
}
