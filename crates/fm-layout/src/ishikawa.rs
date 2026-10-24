//! Ishikawa (fishbone / cause-and-effect) layout.
//!
//! The geometry follows the pinned mermaid 11.15.0 `ishikawaRenderer`, because a fishbone is read by
//! its SHAPE — head on the right, a horizontal spine, causes alternating above and below on bones
//! that lean back toward the tail — and an author comparing the two renders should see the same
//! fish:
//!
//! - the effect sits in the head, whose flat left edge is the end of the spine (`x = 0`);
//! - causes are taken in PAIRS: an even-indexed cause goes above the spine, an odd-indexed one
//!   below, and both bones of a pair start at the same spine point;
//! - each bone leans back at 82° from the spine; a cause without children gets a stub a fifth as
//!   long;
//! - a cause's descendants are spread evenly along its bone. Even depths hang on HORIZONTAL bones
//!   that reach left off their parent; odd depths on DIAGONAL bones that start on their parent's
//!   horizontal bone and run parallel to the cause bone;
//! - the next pair attaches at the left edge of everything the previous pair drew, so pairs never
//!   overlap however deep they are.
//!
//! Everything is expressed as ordinary node boxes and edge polylines, so every backend draws a
//! fishbone without a single family-specific branch: the bones are the cause → effect edges, and the
//! spine is where the first-ring edges converge on their way to the head.

use std::sync::Arc;

use fm_core::{FontMetrics, IrEndpoint, MermaidDiagramIr};

use crate::{
    DiagramLayout, EdgePoints, LayoutEdgePath, LayoutExtensions, LayoutNodeBox, LayoutPoint,
    LayoutRect, LayoutSpacing, LayoutStats, LayoutTrace, TracedLayout, compute_bounds,
    compute_edge_length_metrics, display_node_label_ref, push_snapshot,
};

/// Base length of a bone, and the spine's minimum distance below the top of the drawing.
const SPINE_BASE_LENGTH: f32 = 250.0;
/// A horizontal sub-bone with no children of its own.
const BONE_STUB: f32 = 30.0;
/// A horizontal sub-bone with children grows by `BONE_PER_CHILD` per child from this base.
const BONE_BASE: f32 = 60.0;
const BONE_PER_CHILD: f32 = 5.0;
/// Angle between a cause bone and the spine.
const BONE_ANGLE_DEGREES: f32 = 82.0;
/// Gap between the head and the first pair's attachment point.
const FIRST_ATTACH_GAP: f32 = 20.0;
/// Every backend fits a node label into its box minus 20 on each axis; boxes are sized to match so
/// no label is shrunk or wrapped differently from what was measured here.
const LABEL_FIT_INSET: f32 = 20.0;
/// Horizontal room around a boxed cause label (upstream pads its rect 20 on each side).
const CAUSE_BOX_PAD_X: f32 = 40.0;
/// Gap between the end of a sub-bone and its text.
const TEXT_GAP: f32 = 3.0;

/// One placed bone, as the upstream renderer's `BoneInfo`.
#[derive(Debug, Clone, Copy)]
struct Bone {
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    child_count: usize,
    children_drawn: usize,
}

/// One descendant of a cause, flattened in the order its row is assigned (upstream `LabelEntry`).
#[derive(Debug, Clone, Copy)]
struct Entry {
    node: usize,
    depth: usize,
    parent: Option<usize>,
    child_count: usize,
}

/// The cause tree recovered from the IR's cause → effect edges.
struct Fishbone {
    root: Option<usize>,
    children: Vec<Vec<usize>>,
    /// The edge that carries each node to its parent; `None` for the root.
    parent_edge: Vec<Option<usize>>,
}

impl Fishbone {
    fn from_ir(ir: &MermaidDiagramIr) -> Self {
        let node_count = ir.nodes.len();
        let mut children = vec![Vec::new(); node_count];
        let mut parent_edge = vec![None; node_count];
        for (edge_index, edge) in ir.edges.iter().enumerate() {
            let (IrEndpoint::Node(child), IrEndpoint::Node(parent)) = (edge.from, edge.to) else {
                continue;
            };
            // A node keeps the FIRST bone that names it; a tree has exactly one, and a second
            // (only reachable from a hand-built IR) must not make the walk revisit a subtree.
            if child.0 >= node_count
                || parent.0 >= node_count
                || child == parent
                || parent_edge[child.0].is_some()
            {
                continue;
            }
            parent_edge[child.0] = Some(edge_index);
            children[parent.0].push(child.0);
        }
        let root = (0..node_count).find(|&node| parent_edge[node].is_none());
        Self {
            root,
            children,
            parent_edge,
        }
    }
}

/// Text extent of a node's label.
///
/// ⚠️ NO AUTOMATIC WRAP. Upstream wraps cause text at 15 characters while drawing; here the label
/// is the author's text and every backend draws it from the IR, so a wrap would have to be written
/// INTO the label — changing what the diagram says (and what a source lens would write back) to
/// fix how it looks. An explicit `<br>` is honoured at parse time instead, as upstream honours it.
fn label_extent(ir: &MermaidDiagramIr, node: usize, metrics: &FontMetrics) -> (f32, f32) {
    let text = ir
        .nodes
        .get(node)
        .map_or("", |node| display_node_label_ref(ir, node));
    metrics.estimate_dimensions(text)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    (b - a).mul_add(t, a)
}

fn point(x: f32, y: f32) -> LayoutPoint {
    LayoutPoint { x, y }
}

struct Placer<'a> {
    ir: &'a MermaidDiagramIr,
    tree: &'a Fishbone,
    metrics: &'a FontMetrics,
    boxes: Vec<Option<LayoutRect>>,
    routes: Vec<Option<EdgePoints>>,
    cos_a: f32,
    sin_a: f32,
}

impl Placer<'_> {
    fn route(&mut self, node: usize, points: EdgePoints) {
        if let Some(edge) = self.tree.parent_edge.get(node).copied().flatten()
            && let Some(slot) = self.routes.get_mut(edge)
        {
            *slot = Some(points);
        }
    }

    fn place(&mut self, node: usize, rect: LayoutRect) {
        if let Some(slot) = self.boxes.get_mut(node) {
            *slot = Some(rect);
        }
    }

    /// Place a text-only label whose RIGHT edge sits `TEXT_GAP` short of `anchor_x`.
    ///
    /// `vertical` is -1 for text sitting above `y`, 0 for text centred on it, 1 for text below it —
    /// upstream's `baseline` / `middle` / `hanging`. Returns the text's left edge.
    fn place_text_left_of(&mut self, node: usize, anchor_x: f32, y: f32, vertical: i8) -> f32 {
        let (text_width, text_height) = label_extent(self.ir, node, self.metrics);
        let width = text_width + LABEL_FIT_INSET;
        let height = text_height + LABEL_FIT_INSET;
        let text_right = anchor_x - TEXT_GAP;
        let center_y = y + f32::from(vertical) * text_height / 2.0;
        self.place(
            node,
            LayoutRect {
                x: text_right + LABEL_FIT_INSET / 2.0 - width,
                y: center_y - height / 2.0,
                width,
                height,
            },
        );
        text_right - text_width
    }

    /// Draw one cause and everything below it; returns the leftmost text edge it produced.
    fn branch(
        &mut self,
        cause: usize,
        start: LayoutPoint,
        head: LayoutPoint,
        direction: f32,
        length: f32,
    ) -> f32 {
        let children = self.tree.children[cause].clone();
        let line_len = if children.is_empty() {
            length * 0.2
        } else {
            length
        };
        let dx = -self.cos_a * line_len;
        let dy = self.sin_a * line_len * direction;
        let end = point(start.x + dx, start.y + dy);

        // The boxed cause label sits beyond the end of its bone, centred on it horizontally.
        let (text_width, text_height) = label_extent(self.ir, cause, self.metrics);
        let width = text_width + CAUSE_BOX_PAD_X;
        let height = text_height + LABEL_FIT_INSET;
        self.place(
            cause,
            LayoutRect {
                x: end.x - width / 2.0,
                y: if direction < 0.0 {
                    end.y - height
                } else {
                    end.y
                },
                width,
                height,
            },
        );
        // The bone runs from the label to the spine and on along the spine to the head, so the
        // first-ring bones converging there ARE the spine.
        //
        // ⚠️ THE CORNER IS WRITTEN TWICE ON PURPOSE. The SVG backend smooths every polyline of
        // three or more points with clamped Catmull-Rom tangents, which would round the junction
        // into a curve; a repeated vertex makes both tangents at the corner lie along their own
        // segments, so the bone meets the spine at a sharp angle as a fishbone must. Backends that
        // draw straight segments (canvas, terminal) see a zero-length segment and nothing else.
        self.route(cause, smallvec::smallvec![end, start, start, head]);
        let mut leftmost = end.x - text_width / 2.0;

        if children.is_empty() {
            return leftmost;
        }

        let (entries, row_order) = self.flatten(&children, direction);
        let mut ys = vec![0.0_f32; entries.len()];
        #[allow(clippy::cast_precision_loss)]
        let slots = (entries.len() + 1) as f32;
        for (slot, &entry) in row_order.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let fraction = (slot + 1) as f32 / slots;
            ys[entry] = dy.mul_add(fraction, start.y);
        }

        let mut cause_bone = Bone {
            x0: start.x,
            y0: start.y,
            x1: end.x,
            y1: end.y,
            child_count: children.len(),
            children_drawn: 0,
        };
        let mut bones: Vec<Option<Bone>> = vec![None; entries.len()];
        let diagonal_x = -self.cos_a;
        let diagonal_y = self.sin_a * direction;

        for (index, entry) in entries.iter().enumerate() {
            let y = ys[index];
            let parent = match entry.parent {
                Some(parent) => bones[parent].as_mut(),
                None => Some(&mut cause_bone),
            };
            let Some(parent) = parent else {
                continue;
            };
            let (bx0, by0, bx1) = if entry.depth % 2 == 0 {
                // Horizontal bone: attach to the parent's diagonal at this row, reach left.
                let span = parent.y1 - parent.y0;
                let t = if span == 0.0 {
                    0.5
                } else {
                    (y - parent.y0) / span
                };
                let bx0 = lerp(parent.x0, parent.x1, t);
                #[allow(clippy::cast_precision_loss)]
                let reach = if entry.child_count > 0 {
                    BONE_PER_CHILD.mul_add(entry.child_count as f32, BONE_BASE)
                } else {
                    BONE_STUB
                };
                (bx0, y, bx0 - reach)
            } else {
                // Diagonal bone: start at an evenly spaced point on the parent's horizontal bone and
                // run parallel to the cause bone until it reaches this row.
                let k = parent.children_drawn;
                parent.children_drawn += 1;
                #[allow(clippy::cast_precision_loss)]
                let t = (parent.child_count - k.min(parent.child_count)) as f32
                    / (parent.child_count + 1) as f32;
                let bx0 = lerp(parent.x0, parent.x1, t);
                let by0 = parent.y0;
                let run = if diagonal_y == 0.0 {
                    0.0
                } else {
                    (y - by0) / diagonal_y
                };
                (bx0, by0, diagonal_x.mul_add(run, bx0))
            };

            let vertical = if entry.depth % 2 == 0 {
                0
            } else if direction < 0.0 {
                -1
            } else {
                1
            };
            let text_left = self.place_text_left_of(entry.node, bx1, y, vertical);
            leftmost = leftmost.min(text_left);
            self.route(
                entry.node,
                smallvec::smallvec![point(bx1, y), point(bx0, by0)],
            );

            if entry.child_count > 0 {
                bones[index] = Some(Bone {
                    x0: bx0,
                    y0: by0,
                    x1: bx1,
                    y1: y,
                    child_count: entry.child_count,
                    children_drawn: 0,
                });
            }
        }
        leftmost
    }

    /// Flatten a cause's descendants. Even depths take their row in PRE-order (nearer the spine),
    /// odd depths in POST-order (inside their parent's wedge) — upstream's `flattenTree`. Above the
    /// spine siblings are walked in reverse so the first-written child is still nearest the spine.
    ///
    /// Iterative: a cause tree is untrusted input and may be thousands of levels deep.
    fn flatten(&self, children: &[usize], direction: f32) -> (Vec<Entry>, Vec<usize>) {
        enum Step {
            Enter {
                node: usize,
                parent: Option<usize>,
                depth: usize,
            },
            Exit(usize),
        }
        let ordered = |nodes: &[usize]| -> Vec<usize> {
            if direction < 0.0 {
                nodes.iter().rev().copied().collect()
            } else {
                nodes.to_vec()
            }
        };

        let mut entries = Vec::new();
        let mut row_order = Vec::new();
        let mut stack: Vec<Step> = ordered(children)
            .into_iter()
            .rev()
            .map(|node| Step::Enter {
                node,
                parent: None,
                depth: 2,
            })
            .collect();
        let mut on_path = vec![false; self.tree.children.len()];
        while let Some(step) = stack.pop() {
            match step {
                Step::Exit(index) => row_order.push(index),
                Step::Enter {
                    node,
                    parent,
                    depth,
                } => {
                    if on_path.get(node).copied().unwrap_or(true) {
                        continue;
                    }
                    on_path[node] = true;
                    let grandchildren = &self.tree.children[node];
                    let index = entries.len();
                    entries.push(Entry {
                        node,
                        depth,
                        parent,
                        child_count: grandchildren.len(),
                    });
                    if depth % 2 == 0 {
                        row_order.push(index);
                    } else {
                        stack.push(Step::Exit(index));
                    }
                    for grandchild in ordered(grandchildren).into_iter().rev() {
                        stack.push(Step::Enter {
                            node: grandchild,
                            parent: Some(index),
                            depth: depth + 1,
                        });
                    }
                }
            }
        }
        (entries, row_order)
    }
}

/// Descendant statistics for one side of the spine: (total descendants, largest single cause).
fn side_stats(tree: &Fishbone, causes: &[usize]) -> (usize, usize) {
    let mut total = 0;
    let mut largest = 0;
    for &cause in causes {
        let mut count = 0_usize;
        let mut seen = vec![false; tree.children.len()];
        let mut stack = tree.children[cause].clone();
        while let Some(node) = stack.pop() {
            if std::mem::replace(&mut seen[node], true) {
                continue;
            }
            count += 1;
            stack.extend(tree.children[node].iter().copied());
        }
        total += count;
        largest = largest.max(count);
    }
    (total, largest)
}

/// Lay out an `ishikawa` diagram as a fishbone.
#[must_use]
pub(crate) fn layout_diagram_ishikawa_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    let mut trace = LayoutTrace::default();
    let metrics = FontMetrics::default_metrics();
    let spacing = LayoutSpacing::default();
    let font_size = metrics.font_size();
    let tree = Fishbone::from_ir(ir);
    let angle = BONE_ANGLE_DEGREES.to_radians();

    let mut placer = Placer {
        ir,
        tree: &tree,
        metrics: &metrics,
        boxes: vec![None; ir.nodes.len()],
        routes: vec![None; ir.edges.len()],
        cos_a: angle.cos(),
        sin_a: angle.sin(),
    };

    if let Some(root) = tree.root {
        let causes = tree.children[root].clone();
        let upper: Vec<usize> = causes.iter().copied().step_by(2).collect();
        let lower: Vec<usize> = causes.iter().copied().skip(1).step_by(2).collect();
        let (upper_total, upper_max) = side_stats(&tree, &upper);
        let (lower_total, lower_max) = side_stats(&tree, &lower);
        let descendants = upper_total + lower_total;

        let mut upper_len = SPINE_BASE_LENGTH;
        let mut lower_len = SPINE_BASE_LENGTH;
        if descendants > 0 {
            let pool = SPINE_BASE_LENGTH * 2.0;
            let min_len = SPINE_BASE_LENGTH * 0.3;
            #[allow(clippy::cast_precision_loss)]
            {
                upper_len = min_len.max(pool * upper_total as f32 / descendants as f32);
                lower_len = min_len.max(pool * lower_total as f32 / descendants as f32);
            }
        }
        let min_spacing = font_size * 2.0;
        #[allow(clippy::cast_precision_loss)]
        {
            upper_len = upper_len.max(upper_max as f32 * min_spacing);
            lower_len = lower_len.max(lower_max as f32 * min_spacing);
        }
        let spine_y = upper_len.max(SPINE_BASE_LENGTH);

        // The head: flat left edge on the spine's end, rounded right end, the effect inside.
        let (head_text_width, head_text_height) = label_extent(ir, root, &metrics);
        let head_height = head_text_height.mul_add(2.0, 36.0).max(44.0);
        let head_width = (head_text_width + LABEL_FIT_INSET + head_height / 2.0).max(60.0);
        placer.place(
            root,
            LayoutRect {
                x: 0.0,
                y: spine_y - head_height / 2.0,
                width: head_width,
                height: head_height,
            },
        );
        let head = point(0.0, spine_y);

        let mut spine_x = -FIRST_ATTACH_GAP;
        for pair in causes.chunks(2) {
            let start = point(spine_x, spine_y);
            let mut pair_left = f32::INFINITY;
            for (side, &cause) in pair.iter().enumerate() {
                let (direction, length) = if side == 0 {
                    (-1.0, upper_len)
                } else {
                    (1.0, lower_len)
                };
                pair_left = pair_left.min(placer.branch(cause, start, head, direction, length));
            }
            if pair_left.is_finite() {
                spine_x = pair_left.min(spine_x);
            }
        }
    }
    push_snapshot(
        &mut trace,
        "ishikawa_bones",
        ir.nodes.len(),
        ir.edges.len(),
        0,
        0,
    );

    // A node the cause tree never reached (only possible in a hand-built IR) still gets a box, in a
    // row under the fish, so no backend has to cope with a node that has no geometry.
    let mut floor = placer
        .boxes
        .iter()
        .flatten()
        .map(|rect| rect.y + rect.height)
        .fold(0.0_f32, f32::max)
        + spacing.rank_spacing;
    let mut nodes = Vec::with_capacity(ir.nodes.len());
    for (node_index, node) in ir.nodes.iter().enumerate() {
        let bounds = placer.boxes[node_index].unwrap_or_else(|| {
            let (text_width, text_height) = label_extent(ir, node_index, &metrics);
            let rect = LayoutRect {
                x: 0.0,
                y: floor,
                width: text_width + CAUSE_BOX_PAD_X,
                height: text_height + LABEL_FIT_INSET,
            };
            floor += rect.height + spacing.node_spacing;
            rect
        });
        let depth = {
            let mut depth = 0_usize;
            let mut cursor = node_index;
            while let Some(edge) = tree.parent_edge[cursor] {
                let IrEndpoint::Node(parent) = ir.edges[edge].to else {
                    break;
                };
                depth += 1;
                cursor = parent.0;
                if depth > ir.nodes.len() {
                    break;
                }
            }
            depth
        };
        nodes.push(LayoutNodeBox {
            node_index,
            node_id: node.id.clone(),
            rank: depth,
            order: node_index,
            span: node.span_primary,
            bounds,
        });
    }

    let mut edges = Vec::with_capacity(ir.edges.len());
    for (edge_index, edge) in ir.edges.iter().enumerate() {
        let points = placer.routes[edge_index].take().unwrap_or_else(|| {
            let center = |endpoint: IrEndpoint| match endpoint {
                IrEndpoint::Node(node) => nodes.get(node.0).map_or(point(0.0, 0.0), |node| {
                    point(
                        node.bounds.x + node.bounds.width / 2.0,
                        node.bounds.y + node.bounds.height / 2.0,
                    )
                }),
                _ => point(0.0, 0.0),
            };
            smallvec::smallvec![center(edge.from), center(edge.to)]
        });
        edges.push(LayoutEdgePath {
            edge_index,
            span: edge.span,
            points,
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        });
    }

    // Shift the fish so its top-left sits at the origin plus the usual padding, as every other
    // specialised layout does — the spine is placed relative to the head, so x is negative here.
    let raw = compute_bounds(&nodes, &[], &edges, spacing);
    let shift_x = -raw.x;
    let shift_y = -raw.y;
    for node in &mut nodes {
        node.bounds.x += shift_x;
        node.bounds.y += shift_y;
    }
    for edge in &mut edges {
        for p in &mut edge.points {
            p.x += shift_x;
            p.y += shift_y;
        }
    }
    let bounds = compute_bounds(&nodes, &[], &edges, spacing);
    let (total_edge_length, reversed_edge_total_length) = compute_edge_length_metrics(&edges);
    let stats = LayoutStats {
        node_count: ir.nodes.len(),
        edge_count: ir.edges.len(),
        total_edge_length,
        reversed_edge_total_length,
        phase_iterations: trace.snapshots.len(),
        ..LayoutStats::default()
    };

    TracedLayout {
        layout: Arc::new(DiagramLayout {
            nodes,
            clusters: Vec::new(),
            cycle_clusters: Vec::new(),
            edges,
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
        let traced = layout_diagram_ishikawa_traced(&ir);
        (ir, Arc::unwrap_or_clone(traced.layout))
    }

    fn center(rect: LayoutRect) -> (f32, f32) {
        (rect.x + rect.width / 2.0, rect.y + rect.height / 2.0)
    }

    #[test]
    fn head_sits_right_of_every_cause_on_the_spine() {
        let (ir, layout) = layout(
            "ishikawa\n  Late delivery\n    People\n      Overtime\n    Process\n      Approval queue\n    Tools\n",
        );
        assert_eq!(layout.nodes.len(), ir.nodes.len());
        assert_eq!(layout.edges.len(), ir.edges.len());
        let head = layout.nodes[0].bounds;
        for node in &layout.nodes[1..] {
            assert!(
                node.bounds.x + node.bounds.width <= head.x + 1.0,
                "{} reaches past the head: {:?} vs head {:?}",
                node.node_id,
                node.bounds,
                head
            );
        }
        // Every first-ring bone ends on the head's flat left edge, at the spine height.
        let spine_y = center(head).1;
        for edge in &layout.edges {
            let IrEndpoint::Node(parent) = ir.edges[edge.edge_index].to else {
                panic!("ishikawa edges are node to node");
            };
            if parent.0 == 0 {
                let last = *edge.points.last().unwrap();
                assert!((last.x - head.x).abs() < 0.01);
                assert!((last.y - spine_y).abs() < 0.01);
            }
        }
    }

    #[test]
    fn causes_alternate_above_and_below_the_spine() {
        let (_, layout) = layout("ishikawa\n  Effect\n    A\n    B\n    C\n    D\n");
        let spine_y = center(layout.nodes[0].bounds).1;
        let ys: Vec<f32> = layout.nodes[1..]
            .iter()
            .map(|n| center(n.bounds).1)
            .collect();
        assert!(ys[0] < spine_y, "first cause must be above the spine");
        assert!(ys[1] > spine_y, "second cause must be below the spine");
        assert!(ys[2] < spine_y);
        assert!(ys[3] > spine_y);
        // The second pair attaches further down the spine than the first.
        let x_a = center(layout.nodes[1].bounds).0;
        let x_c = center(layout.nodes[3].bounds).0;
        assert!(x_c < x_a, "pairs must march toward the tail");
    }

    #[test]
    fn sub_causes_hang_off_their_cause_bone_without_overlapping_each_other() {
        let (_, layout) = layout(
            "ishikawa\n  Defects\n    Machine\n      Wear\n      Calibration\n        Drift\n      Vibration\n",
        );
        // Text boxes of siblings on one bone take distinct rows.
        let rows: Vec<f32> = layout.nodes[2..]
            .iter()
            .map(|n| center(n.bounds).1)
            .collect();
        for (i, a) in rows.iter().enumerate() {
            for b in &rows[i + 1..] {
                assert!((a - b).abs() > 1.0, "two sub-causes share a row: {rows:?}");
            }
        }
        // Every sub-cause bone ends where it attaches to its parent's bone, i.e. somewhere along
        // the parent's own route.
        for edge in &layout.edges {
            assert!(edge.points.len() >= 2);
            assert!(
                edge.points
                    .iter()
                    .all(|p| p.x.is_finite() && p.y.is_finite())
            );
        }
    }

    #[test]
    fn layout_is_deterministic_and_inside_its_bounds() {
        let source = "ishikawa-beta\n  Customer churn\n    Price\n      Too high\n    Support\n      Slow replies\n        Understaffed\n    Product\n";
        let (_, first) = layout(source);
        let (_, second) = layout(source);
        assert_eq!(first, second);
        let b = first.bounds;
        for node in &first.nodes {
            assert!(node.bounds.x >= b.x - 0.01 && node.bounds.y >= b.y - 0.01);
            assert!(node.bounds.x + node.bounds.width <= b.x + b.width + 0.01);
            assert!(node.bounds.y + node.bounds.height <= b.y + b.height + 0.01);
        }
    }

    #[test]
    fn effect_alone_and_empty_documents_lay_out() {
        let (_, only_head) = layout("ishikawa\n  Problem\n");
        assert_eq!(only_head.nodes.len(), 1);
        assert!(only_head.edges.is_empty());
        let (_, empty) = layout("ishikawa\n");
        assert!(empty.nodes.is_empty());
    }

    #[test]
    fn a_very_deep_cause_chain_does_not_overflow_the_stack() {
        let mut source = String::from("ishikawa\nEffect\n");
        for depth in 1..=3_000 {
            source.push_str(&" ".repeat(depth));
            source.push_str("c\n");
        }
        let (ir, layout) = layout(&source);
        assert_eq!(layout.nodes.len(), ir.nodes.len());
        assert!(layout.bounds.width.is_finite() && layout.bounds.height.is_finite());
    }
}
