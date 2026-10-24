//! `venn-beta` layout: area-proportional circles for sets, labels at region centres.
//!
//! The pinned mermaid 11.15.0 delegates to venn.js; this follows the same model rather than its
//! code:
//!
//! - every set is a circle whose AREA is its declared size (default 10);
//! - every declared `union` asks for its sets to overlap by its size (default `10 / k²` for a
//!   k-way union); an undeclared pair is asked to be DISJOINT, as upstream treats a missing area;
//! - pairwise target distances come from inverting the two-circle lens area (bisection), centres
//!   are placed greedily — most-overlapping set first, each next set at the candidate point that
//!   best matches its distances to the sets already placed — then refined by stress-minimising
//!   gradient steps;
//! - the result is rotated so the two largest circles sit side by side, scaled into the upstream
//!   800×450 canvas, and every labelled region gets its text at the point of LARGEST MARGIN: inside
//!   all of its own circles and outside every other one, as venn.js's `computeTextCentre` does.
//!
//! Set circles, union labels and `text` entries are ordinary IR nodes, so every backend draws a
//! Venn diagram from node boxes alone.

use std::f32::consts::PI;
use std::sync::Arc;

use fm_core::{IrVennMeta, MermaidDiagramIr};

use crate::{
    DiagramLayout, LayoutExtensions, LayoutNodeBox, LayoutRect, LayoutSpacing, LayoutStats,
    LayoutTrace, TracedLayout, compute_bounds, display_node_label_ref, push_snapshot,
};

/// Upstream's default `venn.width` / `venn.height` / `venn.padding`.
const CANVAS_WIDTH: f32 = 800.0;
const CANVAS_HEIGHT: f32 = 450.0;
const CANVAS_PADDING: f32 = 15.0;
/// Every backend fits a node label into its box minus 20 on each axis.
const LABEL_FIT_INSET: f32 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Circle {
    x: f32,
    y: f32,
    r: f32,
}

/// Area of the lens where two circles whose centres are `d` apart overlap.
fn lens_area(r1: f32, r2: f32, d: f32) -> f32 {
    if d >= r1 + r2 {
        return 0.0;
    }
    if d <= (r1 - r2).abs() {
        let r = r1.min(r2);
        return PI * r * r;
    }
    let a1 = ((d * d + r1 * r1 - r2 * r2) / (2.0 * d * r1))
        .clamp(-1.0, 1.0)
        .acos();
    let a2 = ((d * d + r2 * r2 - r1 * r1) / (2.0 * d * r2))
        .clamp(-1.0, 1.0)
        .acos();
    let k = (-d + r1 + r2) * (d + r1 - r2) * (d - r1 + r2) * (d + r1 + r2);
    0.5f32.mul_add(-k.max(0.0).sqrt(), r1 * r1 * a1 + r2 * r2 * a2)
}

/// The centre distance at which two circles overlap by exactly `area` (bisection, as venn.js).
fn distance_for_overlap(r1: f32, r2: f32, area: f32) -> f32 {
    let contained = PI * r1.min(r2).powi(2);
    if area <= 0.0 {
        return r1 + r2;
    }
    if area >= contained {
        return (r1 - r2).abs();
    }
    let (mut lo, mut hi) = ((r1 - r2).abs(), r1 + r2);
    for _ in 0..64 {
        let mid = (lo + hi) / 2.0;
        if lens_area(r1, r2, mid) > area {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (lo + hi) / 2.0
}

fn distance(ax: f32, ay: f32, bx: f32, by: f32) -> f32 {
    (ax - bx).hypot(ay - by)
}

/// Place circles of the given radii so their centre distances approach `target`.
fn place_circles(radii: &[f32], target: &[Vec<f32>], overlap: &[Vec<f32>]) -> Vec<(f32, f32)> {
    let n = radii.len();
    let mut positions = vec![(0.0_f32, 0.0_f32); n];
    if n <= 1 {
        return positions;
    }
    // Most-connected set first; then repeatedly the unplaced set with the largest total overlap
    // with what is already placed (ties by declaration order, so the result is deterministic).
    let total_overlap: Vec<f32> = (0..n).map(|i| overlap[i].iter().sum()).collect();
    let mut order = Vec::with_capacity(n);
    let mut placed = vec![false; n];
    let first = (0..n)
        .max_by(|&a, &b| {
            total_overlap[a]
                .total_cmp(&total_overlap[b])
                .then(b.cmp(&a))
        })
        .unwrap_or(0);
    order.push(first);
    placed[first] = true;
    while order.len() < n {
        let next = (0..n)
            .filter(|&i| !placed[i])
            .max_by(|&a, &b| {
                let ga: f32 = order.iter().map(|&p| overlap[a][p]).sum();
                let gb: f32 = order.iter().map(|&p| overlap[b][p]).sum();
                ga.total_cmp(&gb).then(b.cmp(&a))
            })
            .unwrap_or(0);
        placed[next] = true;
        order.push(next);
    }

    let stress = |positions: &[(f32, f32)], set: usize, at: (f32, f32), done: &[usize]| -> f32 {
        done.iter()
            .map(|&other| {
                let d = distance(at.0, at.1, positions[other].0, positions[other].1);
                let want = target[set][other];
                // A disjoint pair only needs to be AT LEAST its target apart.
                let err = if overlap[set][other] <= 0.0 {
                    (want - d).max(0.0)
                } else {
                    d - want
                };
                err * err
            })
            .sum()
    };

    positions[order[0]] = (0.0, 0.0);
    for step in 1..n {
        let set = order[step];
        let done = &order[..step];
        let mut candidates = Vec::new();
        for (a_index, &a) in done.iter().enumerate() {
            let (ax, ay) = positions[a];
            let da = target[set][a];
            for k in 0_u16..24 {
                let angle = f32::from(k) * PI / 12.0;
                candidates.push((da.mul_add(angle.cos(), ax), da.mul_add(angle.sin(), ay)));
            }
            // Exact solutions: the points at the right distance from two placed sets at once.
            for &b in &done[a_index + 1..] {
                let (bx, by) = positions[b];
                let db = target[set][b];
                let gap = distance(ax, ay, bx, by);
                if gap <= f32::EPSILON || gap > da + db || gap < (da - db).abs() {
                    continue;
                }
                let along = (da * da - db * db + gap * gap) / (2.0 * gap);
                let h = (da * da - along * along).max(0.0).sqrt();
                let (ux, uy) = ((bx - ax) / gap, (by - ay) / gap);
                let (mx, my) = (along.mul_add(ux, ax), along.mul_add(uy, ay));
                candidates.push((h.mul_add(-uy, mx), h.mul_add(ux, my)));
                candidates.push((h.mul_add(uy, mx), h.mul_add(-ux, my)));
            }
        }
        let best = candidates
            .into_iter()
            .map(|candidate| (stress(&positions, set, candidate, done), candidate))
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map_or((0.0, 0.0), |(_, candidate)| candidate);
        positions[set] = best;
    }

    // Stress refinement over every pair, fixed iteration count so the result is deterministic.
    let scale = radii.iter().copied().fold(0.0_f32, f32::max).max(1e-9);
    for iteration in 0_u16..400 {
        let rate = 0.1 * (1.0 - f32::from(iteration) / 400.0);
        let mut moves = vec![(0.0_f32, 0.0_f32); n];
        for i in 0..n {
            for j in 0..n {
                if i == j {
                    continue;
                }
                let (dx, dy) = (
                    positions[i].0 - positions[j].0,
                    positions[i].1 - positions[j].1,
                );
                let d = dx.hypot(dy).max(1e-9);
                let want = target[i][j];
                let err = if overlap[i][j] <= 0.0 {
                    (want - d).max(0.0)
                } else {
                    want - d
                };
                moves[i].0 += rate * err * dx / d / (n - 1) as f32;
                moves[i].1 += rate * err * dy / d / (n - 1) as f32;
            }
        }
        let mut largest = 0.0_f32;
        for i in 0..n {
            positions[i].0 += moves[i].0;
            positions[i].1 += moves[i].1;
            largest = largest.max(moves[i].0.hypot(moves[i].1));
        }
        if largest < scale * 1e-7 {
            break;
        }
    }
    positions
}

/// How far `point` sits inside every `interior` circle and outside every `exterior` one; the
/// smallest of those clearances (negative when it is in the wrong place). venn.js `circleMargin`.
fn margin(point: (f32, f32), interior: &[Circle], exterior: &[Circle]) -> f32 {
    let mut best = f32::INFINITY;
    for c in interior {
        best = best.min(c.r - distance(point.0, point.1, c.x, c.y));
    }
    for c in exterior {
        best = best.min(distance(point.0, point.1, c.x, c.y) - c.r);
    }
    best
}

/// The point of largest margin for a region: a coarse grid over the interior circles' common box,
/// then a shrinking pattern search around the best sample.
fn region_centre(interior: &[Circle], exterior: &[Circle]) -> (f32, f32) {
    let Some(first) = interior.first() else {
        return (0.0, 0.0);
    };
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        first.x - first.r,
        first.y - first.r,
        first.x + first.r,
        first.y + first.r,
    );
    for c in &interior[1..] {
        min_x = min_x.max(c.x - c.r);
        min_y = min_y.max(c.y - c.r);
        max_x = max_x.min(c.x + c.r);
        max_y = max_y.min(c.y + c.r);
    }
    if min_x > max_x || min_y > max_y {
        return (first.x, first.y);
    }
    const GRID: u16 = 40;
    let mut best = ((min_x + max_x) / 2.0, (min_y + max_y) / 2.0);
    let mut best_margin = margin(best, interior, exterior);
    for gx in 0..=GRID {
        for gy in 0..=GRID {
            let p = (
                min_x + (max_x - min_x) * f32::from(gx) / f32::from(GRID),
                min_y + (max_y - min_y) * f32::from(gy) / f32::from(GRID),
            );
            let m = margin(p, interior, exterior);
            if m > best_margin {
                best_margin = m;
                best = p;
            }
        }
    }
    let mut step = (max_x - min_x).max(max_y - min_y) / f32::from(GRID);
    for _ in 0..60 {
        let mut improved = false;
        for (dx, dy) in [(1.0_f32, 0.0_f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let p = (dx.mul_add(step, best.0), dy.mul_add(step, best.1));
            let m = margin(p, interior, exterior);
            if m > best_margin {
                best_margin = m;
                best = p;
                improved = true;
            }
        }
        if !improved {
            step /= 2.0;
        }
    }
    best
}

/// Rotate (and mirror) the placement so it reads like upstream's: the largest circle on the left,
/// the second largest level with it on the right, and a third below the line between them.
fn orient(circles: &mut [Circle]) {
    // The largest remaining circle, ties to the earlier declaration — linear scans rather than a
    // sort, since only the top three are ever read.
    let largest = |skip: &[usize]| {
        let mut best: Option<usize> = None;
        for (i, c) in circles.iter().enumerate() {
            if !skip.contains(&i) && best.is_none_or(|b| c.r > circles[b].r) {
                best = Some(i);
            }
        }
        best
    };
    let Some(big) = largest(&[]) else {
        return;
    };
    let Some(second) = largest(&[big]) else {
        return;
    };
    let third = largest(&[big, second]);
    let (anchor, toward) = (circles[big], circles[second]);
    let angle = (toward.y - anchor.y).atan2(toward.x - anchor.x);
    let (sin, cos) = (-angle).sin_cos();
    for c in circles.iter_mut() {
        let (dx, dy) = (c.x - anchor.x, c.y - anchor.y);
        c.x = dx.mul_add(cos, -dy * sin);
        c.y = dx.mul_add(sin, dy * cos);
    }
    if let Some(third) = third
        && circles[third].y < 0.0
    {
        for c in circles.iter_mut() {
            c.y = -c.y;
        }
    }
}

/// Lay out a `venn-beta` diagram.
#[must_use]
pub(crate) fn layout_diagram_venn_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let metrics = fm_core::FontMetrics::default_metrics();
    let spacing = LayoutSpacing::default();
    let empty = IrVennMeta::default();
    let meta = ir.venn_meta.as_ref().unwrap_or(&empty);
    let n = meta.sets.len();

    // Sizes are areas in the IR's f64; the geometry runs in f32 like every other layout here (the
    // WASM bundle links only the f32 trig routines).
    #[allow(clippy::cast_possible_truncation)]
    let area = |size: f64| size as f32;
    let radii: Vec<f32> = meta
        .sets
        .iter()
        .map(|set| (area(set.size).max(f32::MIN_POSITIVE) / PI).sqrt())
        .collect();
    let index_of = |id: &str| meta.sets.iter().position(|set| set.id == id);
    // Pairwise overlap areas: a declared 2-way union gives its own size; a pair that only appears
    // inside a larger union must still overlap at least that much, or the larger region could not
    // exist; every other pair is disjoint.
    let mut overlap = vec![vec![0.0_f32; n]; n];
    let mut declared_pair = vec![vec![false; n]; n];
    for union in &meta.unions {
        let members: Vec<usize> = union.sets.iter().filter_map(|id| index_of(id)).collect();
        for (a_pos, &a) in members.iter().enumerate() {
            for &b in &members[a_pos + 1..] {
                if members.len() == 2 {
                    overlap[a][b] = area(union.size);
                    overlap[b][a] = area(union.size);
                    declared_pair[a][b] = true;
                    declared_pair[b][a] = true;
                } else if !declared_pair[a][b] {
                    overlap[a][b] = overlap[a][b].max(area(union.size));
                    overlap[b][a] = overlap[a][b];
                }
            }
        }
    }
    let mut target = vec![vec![0.0_f32; n]; n];
    for i in 0..n {
        for j in 0..n {
            if i != j {
                target[i][j] = distance_for_overlap(radii[i], radii[j], overlap[i][j]);
            }
        }
    }
    let positions = place_circles(&radii, &target, &overlap);
    let mut circles: Vec<Circle> = positions
        .iter()
        .zip(&radii)
        .map(|(&(x, y), &r)| Circle { x, y, r })
        .collect();
    orient(&mut circles);
    push_snapshot(&mut trace, "venn_circles", ir.nodes.len(), 0, 0, 0);

    // Scale into upstream's canvas, leaving room above for a drawn title.
    let title_height = if ir.declared_title_if_drawn().is_some() {
        24.0
    } else {
        0.0
    };
    if let Some(first) = circles.first().copied() {
        let (mut min_x, mut min_y, mut max_x, mut max_y) = (
            first.x - first.r,
            first.y - first.r,
            first.x + first.r,
            first.y + first.r,
        );
        for c in &circles {
            min_x = min_x.min(c.x - c.r);
            min_y = min_y.min(c.y - c.r);
            max_x = max_x.max(c.x + c.r);
            max_y = max_y.max(c.y + c.r);
        }
        let avail_w = CANVAS_PADDING.mul_add(-2.0, CANVAS_WIDTH);
        let avail_h = CANVAS_PADDING.mul_add(-2.0, CANVAS_HEIGHT - title_height);
        let scale = (avail_w / (max_x - min_x).max(1e-9)).min(avail_h / (max_y - min_y).max(1e-9));
        let offset_x = (CANVAS_WIDTH - (max_x - min_x) * scale) / 2.0;
        let offset_y =
            title_height + (CANVAS_HEIGHT - title_height - (max_y - min_y) * scale) / 2.0;
        for c in &mut circles {
            c.x = (c.x - min_x).mul_add(scale, offset_x);
            c.y = (c.y - min_y).mul_add(scale, offset_y);
            c.r *= scale;
        }
    }

    let mut boxes: Vec<Option<LayoutRect>> = vec![None; ir.nodes.len()];
    let rect = |x: f32, y: f32, width: f32, height: f32| LayoutRect {
        x,
        y,
        width,
        height,
    };
    for (set, circle) in meta.sets.iter().zip(&circles) {
        if let Some(slot) = boxes.get_mut(set.node) {
            *slot = Some(rect(
                circle.x - circle.r,
                circle.y - circle.r,
                circle.r * 2.0,
                circle.r * 2.0,
            ));
        }
    }

    // Region text: the point of largest margin inside the region's circles and outside every other
    // circle, skipping circles that wholly contain the region's (they cannot be stepped out of).
    let region = |sets: &[String]| -> Option<(f32, f32, f32)> {
        let members: Vec<usize> = sets.iter().filter_map(|id| index_of(id)).collect();
        if members.is_empty() {
            return None;
        }
        let interior: Vec<Circle> = members.iter().map(|&i| circles[i]).collect();
        let exterior: Vec<Circle> = (0..n)
            .filter(|i| !members.contains(i))
            .filter(|&i| {
                !members.iter().any(|&m| {
                    distance(circles[i].x, circles[i].y, circles[m].x, circles[m].y) + circles[m].r
                        <= circles[i].r + 1e-9
                })
            })
            .map(|i| circles[i])
            .collect();
        let centre = region_centre(&interior, &exterior);
        let room = margin(centre, &interior, &exterior)
            .max(interior.iter().map(|c| c.r).fold(f32::INFINITY, f32::min) * 0.3);
        Some((centre.0, centre.1, room))
    };
    let text_box = |node: usize, cx: f32, cy: f32| -> LayoutRect {
        let text = ir
            .nodes
            .get(node)
            .map_or("", |node| display_node_label_ref(ir, node));
        let (w, h) = metrics.estimate_dimensions(text);
        let (w, h) = (w + LABEL_FIT_INSET, h + LABEL_FIT_INSET);
        rect(cx - w / 2.0, cy - h / 2.0, w, h)
    };

    let mut region_label_offset: Vec<(Vec<String>, f32)> = Vec::new();
    for union in &meta.unions {
        let (Some(node), Some((cx, cy, _))) = (union.label_node, region(&union.sets)) else {
            continue;
        };
        if let Some(slot) = boxes.get_mut(node) {
            *slot = Some(text_box(node, cx, cy));
        }
        region_label_offset.push((union.sets.clone(), metrics.line_height_px()));
    }

    // Text entries: a grid centred in their region, nudged below the region's own label when it has
    // one, as upstream offsets its grid by the label.
    let mut grouped: Vec<(Vec<String>, Vec<usize>)> = Vec::new();
    for text in &meta.texts {
        match grouped.iter_mut().find(|(sets, _)| *sets == text.sets) {
            Some((_, nodes)) => nodes.push(text.node),
            None => grouped.push((text.sets.clone(), vec![text.node])),
        }
    }
    for (sets, nodes) in &grouped {
        let Some((cx, cy, room)) = region(sets) else {
            continue;
        };
        let has_label = if sets.len() == 1 {
            true
        } else {
            region_label_offset
                .iter()
                .any(|(label_sets, _)| label_sets == sets)
        };
        #[allow(clippy::cast_precision_loss)]
        let count = nodes.len() as f32;
        let cols = count.sqrt().ceil().max(1.0);
        let rows = (count / cols).ceil().max(1.0);
        let label_gap = if has_label {
            metrics.line_height_px() * 1.2
        } else {
            0.0
        };
        let row_height = metrics.line_height_px() * 1.4;
        let col_width = (room * 1.6 / cols).max(60.0);
        let grid_top = cy + label_gap - (rows - 1.0) * row_height / 2.0;
        for (i, &node) in nodes.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let (col, row) = ((i as f32) % cols, ((i as f32) / cols).floor());
            let x = (col - (cols - 1.0) / 2.0).mul_add(col_width, cx);
            let y = row.mul_add(row_height, grid_top);
            if let Some(slot) = boxes.get_mut(node) {
                *slot = Some(text_box(node, x, y));
            }
        }
    }

    // A set label sits at its circle's centre (the circle node's own label), so when a region
    // label is placed it never needs to avoid it; anything still unplaced (a hand-built IR) goes in
    // a row under the circles so every node has geometry.
    let mut floor = CANVAS_HEIGHT + spacing.rank_spacing;
    let mut nodes = Vec::with_capacity(ir.nodes.len());
    for (node_index, node) in ir.nodes.iter().enumerate() {
        let bounds = boxes[node_index].unwrap_or_else(|| {
            let placed = text_box(node_index, 0.0, 0.0);
            let bounds = LayoutRect {
                x: 0.0,
                y: floor,
                ..placed
            };
            floor += placed.height;
            bounds
        });
        nodes.push(LayoutNodeBox {
            node_index,
            node_id: node.id.clone(),
            rank: 0,
            order: node_index,
            span: node.span_primary,
            bounds,
        });
    }

    let canvas = LayoutRect {
        x: 0.0,
        y: 0.0,
        width: CANVAS_WIDTH,
        height: CANVAS_HEIGHT,
    };
    let content = compute_bounds(&nodes, &[], &[], spacing);
    let bounds = if nodes.is_empty() {
        canvas
    } else {
        let x = content.x.min(canvas.x);
        let y = content.y.min(canvas.y);
        LayoutRect {
            x,
            y,
            width: (content.x + content.width).max(canvas.width) - x,
            height: (content.y + content.height).max(canvas.height) - y,
        }
    };
    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        phase_iterations: trace.snapshots.len(),
        ..LayoutStats::default()
    };
    TracedLayout {
        layout: Arc::new(DiagramLayout {
            nodes,
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds,
            stats,
            extensions: LayoutExtensions::default(),
            dirty_regions: Vec::new(),
        }),
        trace,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn layout(source: &str) -> (MermaidDiagramIr, DiagramLayout) {
        let ir = fm_parser::parse(source).ir;
        let traced = layout_diagram_venn_traced(&ir);
        (ir, Arc::unwrap_or_clone(traced.layout))
    }

    fn circle_of(layout: &DiagramLayout, node: usize) -> Circle {
        let b = layout.nodes[node].bounds;
        Circle {
            x: b.x + b.width / 2.0,
            y: b.y + b.height / 2.0,
            r: b.width / 2.0,
        }
    }

    #[test]
    fn lens_area_and_its_inverse_agree() {
        for (r1, r2, area) in [(1.0, 1.0, 0.5), (2.0, 1.0, 1.2), (1.5, 1.5, 3.0)] {
            let d = distance_for_overlap(r1, r2, area);
            assert!(
                (lens_area(r1, r2, d) - area).abs() < 1e-6,
                "{r1} {r2} {area}"
            );
        }
        assert_eq!(lens_area(1.0, 1.0, 3.0), 0.0);
        assert!((lens_area(2.0, 1.0, 0.5) - PI).abs() < 1e-9);
        assert_eq!(distance_for_overlap(1.0, 2.0, 0.0), 3.0);
    }

    #[test]
    fn two_overlapping_sets_sit_side_by_side_with_the_declared_overlap() {
        let (ir, layout) = layout("venn-beta\n  set A\n  set B\n  union A,B\n");
        let meta = ir.venn_meta.as_ref().unwrap();
        let (a, b) = (
            circle_of(&layout, meta.sets[0].node),
            circle_of(&layout, meta.sets[1].node),
        );
        assert!(
            (a.y - b.y).abs() < 1e-3,
            "the two circles must be level: {a:?} {b:?}"
        );
        // Equal default sizes give equal radii; the overlap is 2.5 of each circle's 10.
        assert!((a.r - b.r).abs() < 1e-3);
        let d = distance(a.x, a.y, b.x, b.y);
        let ratio = lens_area(a.r, b.r, d) / (PI * a.r * a.r);
        assert!((ratio - 0.25).abs() < 0.01, "overlap ratio {ratio}");
    }

    #[test]
    fn undeclared_pairs_stay_apart_and_sizes_set_the_area() {
        let (ir, layout) = layout("venn-beta\n  set A:40\n  set B:10\n");
        let meta = ir.venn_meta.as_ref().unwrap();
        let (a, b) = (
            circle_of(&layout, meta.sets[0].node),
            circle_of(&layout, meta.sets[1].node),
        );
        assert!(
            distance(a.x, a.y, b.x, b.y) >= a.r + b.r - 0.5,
            "disjoint sets overlap"
        );
        assert!((a.r / b.r - 2.0).abs() < 0.01, "area 4x means radius 2x");
    }

    #[test]
    fn region_labels_land_inside_their_region_and_outside_the_rest() {
        let source = "venn-beta\n  set A\n  set B\n  set C\n  union A,B[\"AB\"]\n  union B,C[\"BC\"]\n  union A,C[\"AC\"]\n  union A,B,C[\"ABC\"]\n";
        let (ir, layout) = layout(source);
        let meta = ir.venn_meta.as_ref().unwrap();
        let circles: Vec<Circle> = meta
            .sets
            .iter()
            .map(|s| circle_of(&layout, s.node))
            .collect();
        for union in &meta.unions {
            let node = union.label_node.unwrap();
            let b = layout.nodes[node].bounds;
            let p = (b.x + b.width / 2.0, b.y + b.height / 2.0);
            for (set, c) in meta.sets.iter().zip(&circles) {
                let inside = distance(p.0, p.1, c.x, c.y) < c.r;
                assert_eq!(
                    inside,
                    union.sets.contains(&set.id),
                    "label {:?} misplaced relative to {}",
                    union.sets,
                    set.id
                );
            }
        }
    }

    #[test]
    fn deterministic_and_within_the_canvas() {
        let source = "venn-beta\n  title Teams\n  set Frontend\n  set Backend\n  set Ops\n  union Frontend,Backend[\"APIs\"]\n  union Backend,Ops\n    text Deploys\n";
        let (_, first) = layout(source);
        let (_, second) = layout(source);
        assert_eq!(first, second);
        for node in &first.nodes {
            assert!(
                node.bounds.x >= -0.5 && node.bounds.y >= -0.5,
                "{:?}",
                node.bounds
            );
            assert!(node.bounds.x + node.bounds.width <= first.bounds.width + 0.5);
        }
    }

    #[test]
    fn empty_and_single_set_documents_lay_out() {
        let (_, empty) = layout("venn-beta\n");
        assert!(empty.nodes.is_empty());
        let (_, single) = layout("venn-beta\n  set Only\n");
        assert_eq!(single.nodes.len(), 1);
        assert!(single.nodes[0].bounds.width > 100.0);
    }
}
