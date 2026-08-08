//! Geometry invariants for a produced layout, shared by every consumer that needs to decide
//! "is this layout structurally valid?" (bd-2xl.14).
//!
//! This exists so the fuzz targets and the CLI input reducer ask the *same* question. Before it,
//! `fuzz/fuzz_targets/fuzz_pipeline.rs` hand-rolled finiteness assertions over node boxes only,
//! and the reducer could not express the invariant at all — so a fuzz-found geometry violation
//! could not be shrunk by the reducer without someone re-implementing the predicate and hoping
//! the two copies agreed.
//!
//! Deliberately geometry-only, over a [`DiagramLayout`] alone: that is exactly what both callers
//! hold. Parse-level properties (confidence bounds, diagnostics) stay with the caller that owns
//! the parse result.

use std::fmt;

use crate::{DiagramLayout, LayoutRect};

/// What kind of geometry invariant a layout broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvariantKind {
    /// A coordinate or extent was NaN or infinite. Nothing downstream can place it: SVG emits an
    /// unparseable coordinate and the terminal renderer indexes off the canvas.
    NonFinite,
    /// A box reported a negative width or height. An extent is a length, so a negative one
    /// inverts the rectangle and makes containment and overlap tests silently disagree.
    NegativeExtent,
}

impl InvariantKind {
    /// Stable identifier for reports and reduction signatures.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NonFinite => "non-finite",
            Self::NegativeExtent => "negative-extent",
        }
    }
}

/// One broken invariant, located precisely enough to act on without re-running the layout.
#[derive(Debug, Clone, PartialEq)]
pub struct InvariantViolation {
    /// Which invariant was broken.
    pub kind: InvariantKind,
    /// Where it was broken, e.g. `node[3] 'Alpha'.bounds.width` or `edge[1].points[2].y`.
    pub site: String,
    /// The offending value.
    pub value: f32,
}

impl fmt::Display for InvariantViolation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} layout value at {}: {}",
            self.kind.as_str(),
            self.site,
            self.value
        )
    }
}

/// Check `rect`, reporting the first broken invariant. Extents are checked for finiteness before
/// sign, so a NaN width is reported as `NonFinite` rather than as a spurious negative.
fn check_rect(rect: &LayoutRect, site: &str, out: &mut Vec<InvariantViolation>) {
    for (label, value) in [
        ("x", rect.x),
        ("y", rect.y),
        ("width", rect.width),
        ("height", rect.height),
    ] {
        if !value.is_finite() {
            out.push(InvariantViolation {
                kind: InvariantKind::NonFinite,
                site: format!("{site}.{label}"),
                value,
            });
        }
    }
    for (label, value) in [("width", rect.width), ("height", rect.height)] {
        if value.is_finite() && value < 0.0 {
            out.push(InvariantViolation {
                kind: InvariantKind::NegativeExtent,
                site: format!("{site}.{label}"),
                value,
            });
        }
    }
}

/// Every geometry invariant violation in `layout`, in a deterministic order: node boxes, cluster
/// boxes, cycle-cluster boxes, edge points, then the diagram bounds.
///
/// Returns all of them rather than stopping at the first, because a reduction report is far more
/// useful when it can say a shrink preserved the *same* violation set.
#[must_use]
pub fn layout_geometry_violations(layout: &DiagramLayout) -> Vec<InvariantViolation> {
    let mut out = Vec::new();

    for (index, node) in layout.nodes.iter().enumerate() {
        check_rect(
            &node.bounds,
            &format!("node[{index}] '{}'.bounds", node.node_id),
            &mut out,
        );
    }
    for (index, cluster) in layout.clusters.iter().enumerate() {
        check_rect(
            &cluster.bounds,
            &format!("cluster[{index}].bounds"),
            &mut out,
        );
    }
    for (index, cluster) in layout.cycle_clusters.iter().enumerate() {
        check_rect(
            &cluster.bounds,
            &format!("cycle_cluster[{index}].bounds"),
            &mut out,
        );
    }
    for (index, edge) in layout.edges.iter().enumerate() {
        for (point_index, point) in edge.points.iter().enumerate() {
            for (label, value) in [("x", point.x), ("y", point.y)] {
                if !value.is_finite() {
                    out.push(InvariantViolation {
                        kind: InvariantKind::NonFinite,
                        site: format!("edge[{index}].points[{point_index}].{label}"),
                        value,
                    });
                }
            }
        }
    }
    check_rect(&layout.bounds, "bounds", &mut out);

    out
}

/// The first geometry invariant violation in `layout`, if any.
///
/// Cheaper to call than [`layout_geometry_violations`] at the call sites that only need a verdict,
/// and it is the form a fuzz target wants: one violation is enough to fail.
#[must_use]
pub fn first_layout_geometry_violation(layout: &DiagramLayout) -> Option<InvariantViolation> {
    layout_geometry_violations(layout).into_iter().next()
}

/// Panic with a located message if `layout` breaks a geometry invariant.
///
/// The entry point for fuzz targets: libfuzzer reports a crash, and the panic message names the
/// exact node/edge and field so the artifact is actionable before any reduction.
///
/// # Panics
/// If any geometry invariant is broken.
pub fn assert_layout_geometry(layout: &DiagramLayout) {
    if let Some(violation) = first_layout_geometry_violation(layout) {
        panic!("layout geometry invariant broken: {violation}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LayoutNodeBox, LayoutPoint, layout_diagram};
    use fm_core::Span;

    fn empty_layout() -> DiagramLayout {
        layout_diagram(&fm_core::MermaidDiagramIr::default())
    }

    fn node_box(id: &str, bounds: LayoutRect) -> LayoutNodeBox {
        LayoutNodeBox {
            node_index: 0,
            node_id: id.to_string(),
            rank: 0,
            order: 0,
            span: Span::default(),
            bounds,
        }
    }

    #[test]
    fn a_clean_layout_reports_nothing() {
        let layout = empty_layout();
        assert!(layout_geometry_violations(&layout).is_empty());
        assert!(first_layout_geometry_violation(&layout).is_none());
        assert_layout_geometry(&layout);
    }

    #[test]
    fn nan_node_coordinate_is_located_by_id_and_field() {
        let mut layout = empty_layout();
        layout.nodes.push(node_box(
            "Alpha",
            LayoutRect {
                x: f32::NAN,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        ));

        let violations = layout_geometry_violations(&layout);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].kind, InvariantKind::NonFinite);
        assert_eq!(violations[0].site, "node[0] 'Alpha'.bounds.x");
        assert!(violations[0].value.is_nan());
    }

    #[test]
    fn infinite_extent_is_non_finite_not_negative() {
        // A NaN or infinite width must not also be reported as a negative extent: `NaN < 0.0` is
        // false but `f32::NEG_INFINITY < 0.0` is true, so without the finiteness guard an
        // infinite width would produce two violations for one broken value.
        let mut layout = empty_layout();
        layout.nodes.push(node_box(
            "Beta",
            LayoutRect {
                x: 0.0,
                y: 0.0,
                width: f32::NEG_INFINITY,
                height: 10.0,
            },
        ));

        let violations = layout_geometry_violations(&layout);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].kind, InvariantKind::NonFinite);
        assert_eq!(violations[0].site, "node[0] 'Beta'.bounds.width");
    }

    #[test]
    fn negative_extent_is_reported_separately_from_position() {
        let mut layout = empty_layout();
        layout.nodes.push(node_box(
            "Gamma",
            LayoutRect {
                x: -5.0,
                y: -5.0,
                width: -1.0,
                height: 10.0,
            },
        ));

        let violations = layout_geometry_violations(&layout);
        // Negative x/y are legitimate — a layout may extend left of the origin — so only the
        // width is a violation.
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].kind, InvariantKind::NegativeExtent);
        assert_eq!(violations[0].site, "node[0] 'Gamma'.bounds.width");
        assert!((violations[0].value - -1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn edge_points_are_checked_not_only_node_boxes() {
        // The gap this module closes: the fuzz target used to check node boxes only, so a
        // non-finite routed edge point passed every assertion.
        let mut layout = empty_layout();
        layout.edges.push(crate::LayoutEdgePath {
            edge_index: 0,
            span: Span::default(),
            points: [
                LayoutPoint { x: 0.0, y: 0.0 },
                LayoutPoint {
                    x: 1.0,
                    y: f32::INFINITY,
                },
            ]
            .into_iter()
            .collect(),
            reversed: false,
            is_self_loop: false,
            parallel_offset: 0.0,
            bundle_count: 1,
            bundled: false,
        });

        let violations = layout_geometry_violations(&layout);
        assert_eq!(violations.len(), 1, "{violations:?}");
        assert_eq!(violations[0].site, "edge[0].points[1].y");
    }

    #[test]
    fn violations_are_ordered_nodes_then_edges_then_bounds() {
        let mut layout = empty_layout();
        layout.nodes.push(node_box(
            "Node",
            LayoutRect {
                x: f32::NAN,
                y: 0.0,
                width: 1.0,
                height: 1.0,
            },
        ));
        layout.bounds.height = f32::NAN;

        let sites: Vec<String> = layout_geometry_violations(&layout)
            .into_iter()
            .map(|violation| violation.site)
            .collect();
        assert_eq!(sites, ["node[0] 'Node'.bounds.x", "bounds.height"]);
    }
}
// A companion test that real parsed diagrams across every diagram type hold these invariants
// lives in `crates/fm-cli/tests/minimize_cli_test.rs`: it needs `fm-parser`, which this crate
// deliberately does not depend on, and it is what keeps the module from being vacuous.
