//! Layout constraint DSL types and validation.
//!
//! Defines a constraint specification language for expressing layout constraints
//! declaratively within Mermaid diagrams. Constraints are parsed from `%%{constraints: ...}%%`
//! directives and stored in the diagram IR for the layout engine to enforce.
//!
//! # Constraint Types
//!
//! | Type | Syntax | Effect |
//! |------|--------|--------|
//! | Alignment | `align([A, B, C], horizontal)` | Force nodes to share a coordinate |
//! | Grouping | `group(G1, [D, E, F], padding: 20)` | Contain nodes in a bounding box |
//! | Ordering | `order(A, left_of, B)` | Relative positioning |
//! | Spacing | `min_spacing(rank, 40)` | Minimum distance between ranks/columns |
//! | Pinning | `pin(A, 100, 200)` | Fix a node at absolute coordinates |
//! | Symmetry | `mirror([A, B], [C, D], vertical)` | Mirror two groups across an axis |
//!
//! # Integration
//!
//! Constraints are stored in `MermaidDiagramIr.constraints: Vec<LayoutConstraint>`.
//! The layout engine checks constraints after initial placement and adjusts positions
//! to satisfy them, falling back to soft constraints (penalties) when hard constraints
//! conflict.

use serde::{Deserialize, Serialize};

/// A layout constraint specification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LayoutConstraint {
    /// Force nodes to share an x or y coordinate.
    Align(AlignConstraint),
    /// Contain nodes within a bounding box with padding.
    Group(GroupConstraint),
    /// Enforce relative ordering between two nodes.
    Order(OrderConstraint),
    /// Set minimum spacing for a dimension.
    Spacing(SpacingConstraint),
    /// Fix a node at absolute coordinates.
    Pin(PinConstraint),
    /// Mirror two groups across an axis.
    Mirror(MirrorConstraint),
}

/// Alignment axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignAxis {
    /// Nodes share the same y-coordinate (horizontal alignment).
    Horizontal,
    /// Nodes share the same x-coordinate (vertical alignment).
    Vertical,
}

/// Align a set of nodes along an axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlignConstraint {
    /// Node IDs to align.
    pub nodes: Vec<String>,
    /// Alignment axis.
    pub axis: AlignAxis,
    /// Constraint strength: hard (must satisfy) or soft (penalty).
    pub strength: ConstraintStrength,
}

/// Relative ordering relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderRelation {
    LeftOf,
    RightOf,
    Above,
    Below,
}

/// Enforce relative ordering between two nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrderConstraint {
    /// The node that must be positioned according to the relation.
    pub subject: String,
    /// The relation (left_of, right_of, above, below).
    pub relation: OrderRelation,
    /// The reference node.
    pub reference: String,
    /// Minimum gap between the nodes (in layout units).
    pub min_gap: f32,
    pub strength: ConstraintStrength,
}

/// Group nodes within a bounding box.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupConstraint {
    /// Group identifier.
    pub name: String,
    /// Node IDs in this group.
    pub nodes: Vec<String>,
    /// Padding around the group bounding box.
    pub padding: f32,
    pub strength: ConstraintStrength,
}

/// Spacing dimension.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpacingDimension {
    /// Minimum distance between adjacent ranks (vertical in TB layout).
    Rank,
    /// Minimum distance between adjacent columns (horizontal in TB layout).
    Column,
    /// Minimum distance between any two nodes.
    Node,
}

/// Set minimum spacing for a layout dimension.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SpacingConstraint {
    /// Which dimension the spacing applies to.
    pub dimension: SpacingDimension,
    /// Minimum spacing value in layout units.
    pub min_value: f32,
}

/// Pin a node at absolute coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PinConstraint {
    /// Node ID to pin.
    pub node: String,
    /// X coordinate.
    pub x: f32,
    /// Y coordinate.
    pub y: f32,
}

/// Mirror two groups across an axis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MirrorConstraint {
    /// First group of node IDs.
    pub group_a: Vec<String>,
    /// Second group of node IDs (mirrored counterparts).
    pub group_b: Vec<String>,
    /// Mirror axis.
    pub axis: AlignAxis,
}

/// Constraint enforcement strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ConstraintStrength {
    /// Must be satisfied; layout fails if impossible.
    #[default]
    Hard,
    /// Best-effort; violated constraints add a penalty to layout quality score.
    Soft,
}

/// A validated set of constraints with conflict detection.
#[derive(Debug, Clone, Default)]
pub struct ConstraintSet {
    constraints: Vec<LayoutConstraint>,
    /// Detected conflicts between constraints.
    conflicts: Vec<ConstraintConflict>,
}

/// A conflict between two constraints.
#[derive(Debug, Clone)]
pub struct ConstraintConflict {
    /// Index of the first conflicting constraint.
    pub constraint_a: usize,
    /// Index of the second conflicting constraint.
    pub constraint_b: usize,
    /// Human-readable description of the conflict.
    pub description: String,
}

impl ConstraintSet {
    /// Create a new empty constraint set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a constraint, running validation.
    pub fn add(&mut self, constraint: LayoutConstraint) {
        let new_idx = self.constraints.len();

        // Check for conflicts with existing constraints.
        for (existing_idx, existing) in self.constraints.iter().enumerate() {
            if let Some(desc) = detect_conflict(existing, &constraint) {
                self.conflicts.push(ConstraintConflict {
                    constraint_a: existing_idx,
                    constraint_b: new_idx,
                    description: desc,
                });
            }
        }

        self.constraints.push(constraint);
    }

    /// Get all constraints.
    #[must_use]
    pub fn constraints(&self) -> &[LayoutConstraint] {
        &self.constraints
    }

    /// Get detected conflicts.
    #[must_use]
    pub fn conflicts(&self) -> &[ConstraintConflict] {
        &self.conflicts
    }

    /// Whether the constraint set has conflicts.
    #[must_use]
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }

    /// Number of constraints.
    #[must_use]
    pub fn len(&self) -> usize {
        self.constraints.len()
    }

    /// Whether there are no constraints.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.constraints.is_empty()
    }

    /// Get all pin constraints.
    #[must_use]
    pub fn pins(&self) -> Vec<&PinConstraint> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                LayoutConstraint::Pin(p) => Some(p),
                _ => None,
            })
            .collect()
    }

    /// Get all alignment constraints.
    #[must_use]
    pub fn alignments(&self) -> Vec<&AlignConstraint> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                LayoutConstraint::Align(a) => Some(a),
                _ => None,
            })
            .collect()
    }

    /// Get all ordering constraints.
    #[must_use]
    pub fn orderings(&self) -> Vec<&OrderConstraint> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                LayoutConstraint::Order(o) => Some(o),
                _ => None,
            })
            .collect()
    }

    /// Get all group constraints.
    #[must_use]
    pub fn groups(&self) -> Vec<&GroupConstraint> {
        self.constraints
            .iter()
            .filter_map(|c| match c {
                LayoutConstraint::Group(g) => Some(g),
                _ => None,
            })
            .collect()
    }
}

/// Detect conflicts between two constraints.
fn detect_conflict(a: &LayoutConstraint, b: &LayoutConstraint) -> Option<String> {
    match (a, b) {
        // Two pins on the same node at different positions.
        (LayoutConstraint::Pin(pa), LayoutConstraint::Pin(pb)) => {
            if pa.node == pb.node && (pa.x != pb.x || pa.y != pb.y) {
                Some(format!(
                    "Node '{}' pinned to ({}, {}) and ({}, {})",
                    pa.node, pa.x, pa.y, pb.x, pb.y
                ))
            } else {
                None
            }
        }
        // Contradictory ordering constraints.
        (LayoutConstraint::Order(oa), LayoutConstraint::Order(ob)) => {
            let contradicts = (oa.subject == ob.reference
                && oa.reference == ob.subject
                && oa.relation == ob.relation)
                || (oa.subject == ob.subject
                    && oa.reference == ob.reference
                    && contradictory_relations(oa.relation, ob.relation));
            if contradicts {
                Some(format!(
                    "Contradictory ordering: {} {:?} {} vs {} {:?} {}",
                    oa.subject, oa.relation, oa.reference, ob.subject, ob.relation, ob.reference
                ))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Check if two order relations contradict each other.
fn contradictory_relations(a: OrderRelation, b: OrderRelation) -> bool {
    matches!(
        (a, b),
        (OrderRelation::LeftOf, OrderRelation::RightOf)
            | (OrderRelation::RightOf, OrderRelation::LeftOf)
            | (OrderRelation::Above, OrderRelation::Below)
            | (OrderRelation::Below, OrderRelation::Above)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_constraint_set() {
        let set = ConstraintSet::new();
        assert!(set.is_empty());
        assert!(!set.has_conflicts());
    }

    #[test]
    fn add_align_constraint() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Align(AlignConstraint {
            nodes: vec!["A".into(), "B".into(), "C".into()],
            axis: AlignAxis::Horizontal,
            strength: ConstraintStrength::Hard,
        }));

        assert_eq!(set.len(), 1);
        assert_eq!(set.alignments().len(), 1);
        assert!(!set.has_conflicts());
    }

    #[test]
    fn add_pin_constraint() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Pin(PinConstraint {
            node: "A".into(),
            x: 100.0,
            y: 200.0,
        }));

        assert_eq!(set.pins().len(), 1);
        assert_eq!(set.pins()[0].x, 100.0);
    }

    #[test]
    fn conflicting_pins_detected() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Pin(PinConstraint {
            node: "A".into(),
            x: 100.0,
            y: 200.0,
        }));
        set.add(LayoutConstraint::Pin(PinConstraint {
            node: "A".into(),
            x: 300.0,
            y: 400.0,
        }));

        assert!(set.has_conflicts());
        assert_eq!(set.conflicts().len(), 1);
        assert!(set.conflicts()[0].description.contains("pinned"));
    }

    #[test]
    fn non_conflicting_pins() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Pin(PinConstraint {
            node: "A".into(),
            x: 100.0,
            y: 200.0,
        }));
        set.add(LayoutConstraint::Pin(PinConstraint {
            node: "B".into(),
            x: 300.0,
            y: 400.0,
        }));

        assert!(!set.has_conflicts());
    }

    #[test]
    fn conflicting_orders_detected() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Order(OrderConstraint {
            subject: "A".into(),
            relation: OrderRelation::LeftOf,
            reference: "B".into(),
            min_gap: 10.0,
            strength: ConstraintStrength::Hard,
        }));
        set.add(LayoutConstraint::Order(OrderConstraint {
            subject: "A".into(),
            relation: OrderRelation::RightOf,
            reference: "B".into(),
            min_gap: 10.0,
            strength: ConstraintStrength::Hard,
        }));

        assert!(set.has_conflicts());
    }

    #[test]
    fn non_conflicting_orders() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Order(OrderConstraint {
            subject: "A".into(),
            relation: OrderRelation::LeftOf,
            reference: "B".into(),
            min_gap: 10.0,
            strength: ConstraintStrength::Hard,
        }));
        set.add(LayoutConstraint::Order(OrderConstraint {
            subject: "B".into(),
            relation: OrderRelation::LeftOf,
            reference: "C".into(),
            min_gap: 10.0,
            strength: ConstraintStrength::Hard,
        }));

        assert!(!set.has_conflicts());
    }

    #[test]
    fn group_constraint() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Group(GroupConstraint {
            name: "cluster1".into(),
            nodes: vec!["A".into(), "B".into(), "C".into()],
            padding: 20.0,
            strength: ConstraintStrength::Hard,
        }));

        assert_eq!(set.groups().len(), 1);
        assert_eq!(set.groups()[0].nodes.len(), 3);
    }

    #[test]
    fn spacing_constraint() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Spacing(SpacingConstraint {
            dimension: SpacingDimension::Rank,
            min_value: 40.0,
        }));

        assert_eq!(set.len(), 1);
    }

    #[test]
    fn mirror_constraint() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Mirror(MirrorConstraint {
            group_a: vec!["A".into(), "B".into()],
            group_b: vec!["C".into(), "D".into()],
            axis: AlignAxis::Vertical,
        }));

        assert_eq!(set.len(), 1);
    }

    #[test]
    fn mixed_constraints() {
        let mut set = ConstraintSet::new();
        set.add(LayoutConstraint::Align(AlignConstraint {
            nodes: vec!["A".into(), "B".into()],
            axis: AlignAxis::Horizontal,
            strength: ConstraintStrength::Hard,
        }));
        set.add(LayoutConstraint::Pin(PinConstraint {
            node: "C".into(),
            x: 50.0,
            y: 50.0,
        }));
        set.add(LayoutConstraint::Order(OrderConstraint {
            subject: "A".into(),
            relation: OrderRelation::Above,
            reference: "D".into(),
            min_gap: 20.0,
            strength: ConstraintStrength::Soft,
        }));

        assert_eq!(set.len(), 3);
        assert!(!set.has_conflicts());
    }

    #[test]
    fn constraint_serde_roundtrip() {
        let constraint = LayoutConstraint::Align(AlignConstraint {
            nodes: vec!["X".into(), "Y".into()],
            axis: AlignAxis::Vertical,
            strength: ConstraintStrength::Soft,
        });

        let json = serde_json::to_string(&constraint).unwrap();
        let deserialized: LayoutConstraint = serde_json::from_str(&json).unwrap();
        assert_eq!(constraint, deserialized);
    }

    #[test]
    fn order_relation_contradictions() {
        assert!(contradictory_relations(
            OrderRelation::LeftOf,
            OrderRelation::RightOf
        ));
        assert!(contradictory_relations(
            OrderRelation::Above,
            OrderRelation::Below
        ));
        assert!(!contradictory_relations(
            OrderRelation::LeftOf,
            OrderRelation::Above
        ));
        assert!(!contradictory_relations(
            OrderRelation::LeftOf,
            OrderRelation::LeftOf
        ));
    }

    #[test]
    fn constraint_strength_default() {
        assert_eq!(ConstraintStrength::default(), ConstraintStrength::Hard);
    }
}
