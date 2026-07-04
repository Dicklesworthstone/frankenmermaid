//! CGA-based intersection queries for edge routing.
//!
//! This module provides intersection detection using Conformal Geometric Algebra,
//! replacing manual AABB checks with proper segment-rectangle intersection tests.

use fm_core::cga::{CgaLineSegment, CgaPoint, CgaRect};

use crate::{LayoutPoint, LayoutRect};

/// Convert a LayoutPoint to a CgaPoint.
#[inline(always)]
fn to_cga_point(p: LayoutPoint) -> CgaPoint {
    CgaPoint::new(f64::from(p.x), f64::from(p.y))
}

/// Check if a line segment intersects any obstacle rectangle.
///
/// Returns true if the segment crosses the obstacle boundary OR is inside it.
#[must_use]
#[allow(dead_code)]
pub fn segment_intersects_obstacles(
    start: LayoutPoint,
    end: LayoutPoint,
    obstacles: &[LayoutRect],
    margin: f32,
) -> bool {
    let seg = CgaLineSegment::new(to_cga_point(start), to_cga_point(end));
    let start_cga = to_cga_point(start);
    let margin_f64 = f64::from(margin);

    for obs in obstacles {
        // Expand obstacle by margin
        let expanded = CgaRect::new(
            f64::from(obs.x) - margin_f64,
            f64::from(obs.y) - margin_f64,
            f64::from(obs.width) + 2.0 * margin_f64,
            f64::from(obs.height) + 2.0 * margin_f64,
        );

        // Check boundary crossings OR if segment is entirely inside
        if !expanded.intersect_segment(&seg).is_empty() || expanded.contains(&start_cga) {
            return true;
        }
    }
    false
}

/// Find the first obstacle that a segment intersects.
///
/// Returns the obstacle index and intersection points, or None if no intersection.
#[must_use]
#[allow(dead_code)]
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
    find_vertical_segment_nudge_iter(segment_start, segment_end, obstacles.iter(), margin)
}

/// Check candidate obstacle indices and return the nudge for the lowest-index hit.
#[must_use]
pub fn find_vertical_segment_nudge_by_indices(
    segment_start: LayoutPoint,
    segment_end: LayoutPoint,
    obstacles: &[LayoutRect],
    candidate_indices: &[usize],
    margin: f32,
) -> Option<f32> {
    let seg = CgaLineSegment::new(to_cga_point(segment_start), to_cga_point(segment_end));
    let start_cga = to_cga_point(segment_start);
    let margin_f64 = f64::from(margin);
    let mid_x = f64::from(segment_start.x);
    let seg_min_x = f64::from(segment_start.x.min(segment_end.x));
    let seg_max_x = f64::from(segment_start.x.max(segment_end.x));
    let seg_min_y = f64::from(segment_start.y.min(segment_end.y));
    let seg_max_y = f64::from(segment_start.y.max(segment_end.y));

    // The routing result is the nudge of the *minimum-index* obstacle the segment
    // intersects. Callers used to pre-sort `candidate_indices` ascending and take the
    // first hit; we instead scan the (unsorted) candidates once and keep the smallest
    // intersecting index. Byte-identical result, no O(K log K) sort. The `idx >= best_idx`
    // guard prunes the expensive CGA test for candidates that cannot lower the minimum.
    let mut best_idx = usize::MAX;
    let mut best_nudge = None;
    for &idx in candidate_indices {
        if idx >= best_idx {
            continue;
        }
        let Some(obs) = obstacles.get(idx) else {
            continue;
        };
        if let Some(nudge) = vertical_nudge_for_obstacle(
            &seg, &start_cga, obs, margin_f64, mid_x, seg_min_x, seg_max_x, seg_min_y, seg_max_y,
        ) {
            best_idx = idx;
            best_nudge = Some(nudge);
        }
    }
    best_nudge
}

fn find_vertical_segment_nudge_iter<'a>(
    segment_start: LayoutPoint,
    segment_end: LayoutPoint,
    obstacles: impl IntoIterator<Item = &'a LayoutRect>,
    margin: f32,
) -> Option<f32> {
    let seg = CgaLineSegment::new(to_cga_point(segment_start), to_cga_point(segment_end));
    let start_cga = to_cga_point(segment_start);
    let margin_f64 = f64::from(margin);
    let mid_x = f64::from(segment_start.x);

    // Segment axis-aligned bounding box (in f64, matching the CGA math). Used as a
    // cheap conservative reject below.
    let seg_min_x = f64::from(segment_start.x.min(segment_end.x));
    let seg_max_x = f64::from(segment_start.x.max(segment_end.x));
    let seg_min_y = f64::from(segment_start.y.min(segment_end.y));
    let seg_max_y = f64::from(segment_start.y.max(segment_end.y));

    for obs in obstacles {
        if let Some(nudge) = vertical_nudge_for_obstacle(
            &seg, &start_cga, obs, margin_f64, mid_x, seg_min_x, seg_max_x, seg_min_y, seg_max_y,
        ) {
            return Some(nudge);
        }
    }
    None
}

/// Nudge for a single obstacle if the vertical segment intersects it, else `None`.
/// Extracted so the ordered linear scan (`find_vertical_segment_nudge_iter`) and the
/// indexed min-index scan (`find_vertical_segment_nudge_by_indices`) share identical
/// intersection + nudge arithmetic — the two only differ in candidate-selection order.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn vertical_nudge_for_obstacle(
    seg: &CgaLineSegment,
    start_cga: &CgaPoint,
    obs: &LayoutRect,
    margin_f64: f64,
    mid_x: f64,
    seg_min_x: f64,
    seg_max_x: f64,
    seg_min_y: f64,
    seg_max_y: f64,
) -> Option<f32> {
    // Cheap AABB rejection before the (expensive) CGA test. The segment lies within its
    // bounding box and `start` within that box, so if the box does not overlap the
    // margin-expanded obstacle the CGA test is guaranteed to report no intersection.
    let exp_min_x = f64::from(obs.x) - margin_f64;
    let exp_max_x = f64::from(obs.x) + f64::from(obs.width) + margin_f64;
    let exp_min_y = f64::from(obs.y) - margin_f64;
    let exp_max_y = f64::from(obs.y) + f64::from(obs.height) + margin_f64;
    if seg_max_x < exp_min_x
        || seg_min_x > exp_max_x
        || seg_max_y < exp_min_y
        || seg_min_y > exp_max_y
    {
        return None;
    }

    let expanded = CgaRect::new(
        f64::from(obs.x) - margin_f64,
        f64::from(obs.y) - margin_f64,
        f64::from(obs.width) + 2.0 * margin_f64,
        f64::from(obs.height) + 2.0 * margin_f64,
    );

    // Check boundary crossings OR if segment is entirely inside
    if !expanded.intersect_segment(seg).is_empty() || expanded.contains(start_cga) {
        // Nudge to closer side
        let left = f64::from(obs.x) - margin_f64;
        let right = f64::from(obs.x) + f64::from(obs.width) + margin_f64;
        let left_dist = (mid_x - left).abs();
        let right_dist = (mid_x - right).abs();

        if left_dist <= right_dist {
            Some(left as f32)
        } else {
            Some(right as f32)
        }
    } else {
        None
    }
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
    find_horizontal_segment_nudge_iter(segment_start, segment_end, obstacles.iter(), margin)
}

/// Check candidate obstacle indices and return the nudge for the lowest-index hit.
#[must_use]
pub fn find_horizontal_segment_nudge_by_indices(
    segment_start: LayoutPoint,
    segment_end: LayoutPoint,
    obstacles: &[LayoutRect],
    candidate_indices: &[usize],
    margin: f32,
) -> Option<f32> {
    let seg = CgaLineSegment::new(to_cga_point(segment_start), to_cga_point(segment_end));
    let start_cga = to_cga_point(segment_start);
    let margin_f64 = f64::from(margin);
    let mid_y = f64::from(segment_start.y);
    let seg_min_x = f64::from(segment_start.x.min(segment_end.x));
    let seg_max_x = f64::from(segment_start.x.max(segment_end.x));
    let seg_min_y = f64::from(segment_start.y.min(segment_end.y));
    let seg_max_y = f64::from(segment_start.y.max(segment_end.y));

    // See `find_vertical_segment_nudge_by_indices`: pick the nudge of the minimum-index
    // intersecting obstacle via a single scan, replacing the caller's ascending sort.
    let mut best_idx = usize::MAX;
    let mut best_nudge = None;
    for &idx in candidate_indices {
        if idx >= best_idx {
            continue;
        }
        let Some(obs) = obstacles.get(idx) else {
            continue;
        };
        if let Some(nudge) = horizontal_nudge_for_obstacle(
            &seg, &start_cga, obs, margin_f64, mid_y, seg_min_x, seg_max_x, seg_min_y, seg_max_y,
        ) {
            best_idx = idx;
            best_nudge = Some(nudge);
        }
    }
    best_nudge
}

fn find_horizontal_segment_nudge_iter<'a>(
    segment_start: LayoutPoint,
    segment_end: LayoutPoint,
    obstacles: impl IntoIterator<Item = &'a LayoutRect>,
    margin: f32,
) -> Option<f32> {
    let seg = CgaLineSegment::new(to_cga_point(segment_start), to_cga_point(segment_end));
    let start_cga = to_cga_point(segment_start);
    let margin_f64 = f64::from(margin);
    let mid_y = f64::from(segment_start.y);

    // Segment axis-aligned bounding box (in f64, matching the CGA math). Used as a
    // cheap conservative reject below.
    let seg_min_x = f64::from(segment_start.x.min(segment_end.x));
    let seg_max_x = f64::from(segment_start.x.max(segment_end.x));
    let seg_min_y = f64::from(segment_start.y.min(segment_end.y));
    let seg_max_y = f64::from(segment_start.y.max(segment_end.y));

    for obs in obstacles {
        if let Some(nudge) = horizontal_nudge_for_obstacle(
            &seg, &start_cga, obs, margin_f64, mid_y, seg_min_x, seg_max_x, seg_min_y, seg_max_y,
        ) {
            return Some(nudge);
        }
    }
    None
}

/// Nudge for a single obstacle if the horizontal segment intersects it, else `None`.
/// Shared by `find_horizontal_segment_nudge_iter` and `_by_indices` (see the vertical
/// counterpart `vertical_nudge_for_obstacle`).
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn horizontal_nudge_for_obstacle(
    seg: &CgaLineSegment,
    start_cga: &CgaPoint,
    obs: &LayoutRect,
    margin_f64: f64,
    mid_y: f64,
    seg_min_x: f64,
    seg_max_x: f64,
    seg_min_y: f64,
    seg_max_y: f64,
) -> Option<f32> {
    // Cheap AABB rejection before the (expensive) CGA test.
    let exp_min_x = f64::from(obs.x) - margin_f64;
    let exp_max_x = f64::from(obs.x) + f64::from(obs.width) + margin_f64;
    let exp_min_y = f64::from(obs.y) - margin_f64;
    let exp_max_y = f64::from(obs.y) + f64::from(obs.height) + margin_f64;
    if seg_max_x < exp_min_x
        || seg_min_x > exp_max_x
        || seg_max_y < exp_min_y
        || seg_min_y > exp_max_y
    {
        return None;
    }

    let expanded = CgaRect::new(
        f64::from(obs.x) - margin_f64,
        f64::from(obs.y) - margin_f64,
        f64::from(obs.width) + 2.0 * margin_f64,
        f64::from(obs.height) + 2.0 * margin_f64,
    );

    // Check boundary crossings OR if segment is entirely inside
    if !expanded.intersect_segment(seg).is_empty() || expanded.contains(start_cga) {
        // Nudge to closer side
        let top = f64::from(obs.y) - margin_f64;
        let bottom = f64::from(obs.y) + f64::from(obs.height) + margin_f64;
        let top_dist = (mid_y - top).abs();
        let bottom_dist = (mid_y - bottom).abs();

        if top_dist <= bottom_dist {
            Some(top as f32)
        } else {
            Some(bottom as f32)
        }
    } else {
        None
    }
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
    fn indexed_nudge_ignores_candidate_order_and_uses_lowest_hit_index() {
        let horizontal_obstacles = vec![
            LayoutRect {
                x: 20.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
            },
            LayoutRect {
                x: 70.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
            },
        ];
        let horizontal_start = LayoutPoint { x: 0.0, y: 15.0 };
        let horizontal_end = LayoutPoint { x: 100.0, y: 15.0 };
        let horizontal = find_horizontal_segment_nudge_by_indices(
            horizontal_start,
            horizontal_end,
            &horizontal_obstacles,
            &[1, 0],
            0.0,
        );
        assert_eq!(
            horizontal,
            find_horizontal_segment_nudge(
                horizontal_start,
                horizontal_end,
                &horizontal_obstacles,
                0.0
            )
        );

        let vertical_obstacles = vec![
            LayoutRect {
                x: 20.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
            },
            LayoutRect {
                x: 20.0,
                y: 50.0,
                width: 10.0,
                height: 20.0,
            },
        ];
        let vertical_start = LayoutPoint { x: 25.0, y: -10.0 };
        let vertical_end = LayoutPoint { x: 25.0, y: 90.0 };
        let vertical = find_vertical_segment_nudge_by_indices(
            vertical_start,
            vertical_end,
            &vertical_obstacles,
            &[1, 0],
            0.0,
        );
        assert_eq!(
            vertical,
            find_vertical_segment_nudge(vertical_start, vertical_end, &vertical_obstacles, 0.0)
        );
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

    #[test]
    fn segment_entirely_inside_obstacle_detected() {
        // Segment entirely inside obstacle - no boundary crossing but still intersects
        let obs = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 30.0,
        };
        let start = LayoutPoint { x: 20.0, y: 20.0 };
        let end = LayoutPoint { x: 25.0, y: 25.0 };
        assert!(segment_intersects_obstacles(start, end, &[obs], 0.0));
    }

    #[test]
    fn vertical_segment_inside_obstacle_nudge() {
        // Vertical segment entirely inside obstacle
        let obs = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 30.0,
        };
        let start = LayoutPoint { x: 25.0, y: 20.0 };
        let end = LayoutPoint { x: 25.0, y: 30.0 };
        let nudge = find_vertical_segment_nudge(start, end, &[obs], 0.0);
        assert!(nudge.is_some());
        // x=25 is 15 from left edge (10) and 15 from right edge (40)
        // Equal distance, so should nudge to left edge
        assert!((nudge.unwrap() - 10.0).abs() < 0.01);
    }

    #[test]
    fn horizontal_segment_inside_obstacle_nudge() {
        // Horizontal segment entirely inside obstacle
        let obs = LayoutRect {
            x: 10.0,
            y: 10.0,
            width: 30.0,
            height: 30.0,
        };
        let start = LayoutPoint { x: 20.0, y: 25.0 };
        let end = LayoutPoint { x: 30.0, y: 25.0 };
        let nudge = find_horizontal_segment_nudge(start, end, &[obs], 0.0);
        assert!(nudge.is_some());
        // y=25 is 15 from top edge (10) and 15 from bottom edge (40)
        // Equal distance, so should nudge to top edge
        assert!((nudge.unwrap() - 10.0).abs() < 0.01);
    }
}
