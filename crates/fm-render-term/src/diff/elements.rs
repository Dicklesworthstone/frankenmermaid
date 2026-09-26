//! Identity-aware comparison of semantic node and edge properties.

use super::{DiagramChange, DiagramDiff, DiffStatus, NodeChange};
use fm_core::{ArrowType, IrEndpoint, IrNode, IrStyleTarget, MermaidDiagramIr};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

type Properties = BTreeMap<String, String>;

/// Edge changes include semantics outside the arrow and plain-text label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum EdgeChange {
    ArrowChanged { old: ArrowType, new: ArrowType },
    LabelChanged { old: String, new: String },
    ErNotationChanged { old: Option<String>, new: Option<String> },
    /// Guard/action, endpoint cardinality, animation, placement side, rich label,
    /// technology, co-arrow, or declared styling changed. See `element_changes`
    /// on the containing diff for the escaped before/after metadata.
    MetadataChanged,
}

/// A compared edge. Port endpoints retain their port names rather than collapsing
/// onto their owning node; unresolved endpoints are retained, never dropped.
#[derive(Debug, Clone, Serialize)]
pub struct DiffEdge {
    pub from_id: String,
    pub to_id: String,
    pub status: DiffStatus,
    pub arrow: ArrowType,
    pub changes: Vec<EdgeChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum NodeIdentity {
    Named(String),
    Invalid(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum EndpointKey {
    Node(NodeIdentity),
    Port(NodeIdentity, String),
    InvalidPort(usize),
    Unresolved,
}

fn node_identity(ir: &MermaidDiagramIr, index: usize) -> NodeIdentity {
    ir.nodes.get(index).map_or(NodeIdentity::Invalid(index), |node| {
        NodeIdentity::Named(node.id.clone())
    })
}

pub(super) fn endpoint_key(ir: &MermaidDiagramIr, endpoint: IrEndpoint) -> EndpointKey {
    match endpoint {
        IrEndpoint::Node(id) => EndpointKey::Node(node_identity(ir, id.0)),
        IrEndpoint::Port(id) => ir.ports.get(id.0).map_or(EndpointKey::InvalidPort(id.0), |port| {
            EndpointKey::Port(node_identity(ir, port.node.0), port.name.clone())
        }),
        IrEndpoint::Unresolved => EndpointKey::Unresolved,
    }
}

impl EndpointKey {
    fn display(&self) -> String {
        let node_name = |node: &NodeIdentity| match node {
            NodeIdentity::Named(name) => name.clone(),
            NodeIdentity::Invalid(index) => format!("<invalid node {index}>"),
        };
        match self {
            Self::Node(node) => node_name(node),
            Self::Port(node, name) => format!("{}::port({name:?})", node_name(node)),
            Self::InvalidPort(index) => format!("<invalid port {index}>"),
            Self::Unresolved => "<unresolved>".to_string(),
        }
    }
}

/// Declaration channels are kept separate: this is a semantic source comparison,
/// not a new CSS cascade implementation that could disagree with a renderer.
#[derive(Default)]
struct StyleDeclarations {
    classes: BTreeMap<String, Properties>,
    class_refs: BTreeMap<String, Properties>,
    nodes: BTreeMap<usize, Properties>,
    links: BTreeMap<usize, Properties>,
    link_default: Properties,
}

impl StyleDeclarations {
    fn new(ir: &MermaidDiagramIr) -> Self {
        let mut result = Self::default();
        for definition in &ir.style_defs {
            result.classes.entry(definition.name.clone()).or_default()
                .extend(definition.properties.clone());
        }
        for reference in &ir.style_refs {
            let properties = fm_core::parse_style_string(&reference.style).properties;
            if properties.is_empty() {
                continue;
            }
            match &reference.target {
                IrStyleTarget::Class(name) => {
                    result.class_refs.entry(name.clone()).or_default().extend(properties);
                }
                IrStyleTarget::Node(id) => {
                    result.nodes.entry(id.0).or_default().extend(properties);
                }
                IrStyleTarget::Link(index) => {
                    result.links.entry(*index).or_default().extend(properties);
                }
                IrStyleTarget::LinkDefault => result.link_default.extend(properties),
                IrStyleTarget::Cluster(_) => {}
            }
        }
        result
    }
}

fn node_metadata(ir: &MermaidDiagramIr, index: usize, node: &IrNode, styles: &StyleDeclarations) -> String {
    let empty = Properties::new();
    let classes: Vec<_> = node.classes.iter().filter_map(|name| {
        let definitions = styles.classes.get(name).unwrap_or(&empty);
        let references = styles.class_refs.get(name).unwrap_or(&empty);
        (!definitions.is_empty() || !references.is_empty()).then_some((name, definitions, references))
    }).collect();
    let markup = node.label.and_then(|id| ir.label_markup.get(&id))
        .map(Vec::as_slice).unwrap_or(&[]);
    let actors = node.journey_meta.as_ref().map(|meta| meta.actors.as_slice()).unwrap_or(&[]);
    let inline = node.inline_style.as_ref().map(|style| &style.properties).unwrap_or(&empty);
    format!("{:?}", (
        node.icon(), node.callback(), node.link_target(), &node.menu_links,
        actors, markup, inline, styles.nodes.get(&index).unwrap_or(&empty), classes,
    ))
}

pub(super) fn augment_nodes(old: &MermaidDiagramIr, new: &MermaidDiagramIr, diff: &mut DiagramDiff) {
    let old_styles = StyleDeclarations::new(old);
    let new_styles = StyleDeclarations::new(new);
    let old_nodes: BTreeMap<_, _> = old.nodes.iter().enumerate()
        .map(|(index, node)| (node.id.as_str(), (index, node))).collect();
    let new_nodes: BTreeMap<_, _> = new.nodes.iter().enumerate()
        .map(|(index, node)| (node.id.as_str(), (index, node))).collect();
    for node in &mut diff.nodes {
        let (Some(&(old_index, old_node)), Some(&(new_index, new_node))) =
            (old_nodes.get(node.id.as_str()), new_nodes.get(node.id.as_str())) else {
                continue;
            };
        let before = node_metadata(old, old_index, old_node, &old_styles);
        let after = node_metadata(new, new_index, new_node, &new_styles);
        if before == after {
            continue;
        }
        if node.status == DiffStatus::Unchanged {
            node.status = DiffStatus::Changed;
            diff.unchanged_nodes -= 1;
            diff.changed_nodes += 1;
        }
        if !node.changes.contains(&NodeChange::MetadataChanged) {
            node.changes.push(NodeChange::MetadataChanged);
        }
        diff.element_changes.push(DiagramChange {
            field: format!("node {:?}.metadata", node.id), before, after,
        });
    }
}

#[derive(Debug)]
struct EdgeSnapshot {
    arrow: ArrowType,
    label: String,
    er_notation: Option<String>,
    metadata: String,
}

fn edge_snapshot(ir: &MermaidDiagramIr, index: usize, styles: &StyleDeclarations) -> EdgeSnapshot {
    let edge = &ir.edges[index];
    let empty = Properties::new();
    let mut extras = edge.extras.as_deref().cloned().unwrap_or_default();
    // This field already has a specific change variant.
    extras.er_notation = None;
    let markup = edge.label.and_then(|id| ir.label_markup.get(&id))
        .map(Vec::as_slice).unwrap_or(&[]);
    let inline = edge.inline_style.as_ref().map(|style| &style.properties).unwrap_or(&empty);
    let side = |endpoint| match endpoint {
        IrEndpoint::Port(id) => ir.ports.get(id.0).map(|port| port.side_hint),
        _ => None,
    };
    EdgeSnapshot {
        arrow: edge.arrow,
        label: edge.label.and_then(|id| ir.labels.get(id.0))
            .map(|label| label.text.clone()).unwrap_or_default(),
        er_notation: edge.er_notation().map(str::to_string),
        metadata: format!("{:?}", (
            extras, markup, inline, &styles.link_default,
            styles.links.get(&index).unwrap_or(&empty), side(edge.from), side(edge.to),
        )),
    }
}

/// Includes intrinsic semantics but excludes external style declarations. Used
/// when resolving a linkStyle target to a stable edge rather than its array slot.
pub(super) fn intrinsic_edge_key(ir: &MermaidDiagramIr, index: usize) -> String {
    let Some(edge) = ir.edges.get(index) else {
        return format!("<invalid edge {index}>");
    };
    format!("{:?}", (
        endpoint_key(ir, edge.from), endpoint_key(ir, edge.to),
        edge_snapshot(ir, index, &StyleDeclarations::default()),
    ))
}

pub(super) fn ordered_messages(ir: &MermaidDiagramIr) -> Vec<String> {
    let styles = StyleDeclarations::new(ir);
    ir.edges.iter().enumerate().map(|(index, edge)| format!("{:?}", (
        endpoint_key(ir, edge.from), endpoint_key(ir, edge.to), edge_snapshot(ir, index, &styles),
    ))).collect()
}

pub(super) struct EdgeDiff {
    pub edges: Vec<DiffEdge>,
    pub counts: (usize, usize, usize, usize),
    pub details: Vec<DiagramChange>,
}

type EdgePair = (EndpointKey, EndpointKey);

fn edge_groups(ir: &MermaidDiagramIr) -> BTreeMap<EdgePair, Vec<usize>> {
    let mut groups: BTreeMap<EdgePair, Vec<usize>> = BTreeMap::new();
    for (index, edge) in ir.edges.iter().enumerate() {
        groups.entry((endpoint_key(ir, edge.from), endpoint_key(ir, edge.to)))
            .or_default().push(index);
    }
    groups
}

fn compare_edges(old: &EdgeSnapshot, new: &EdgeSnapshot) -> Vec<EdgeChange> {
    let mut changes = Vec::new();
    if old.arrow != new.arrow {
        changes.push(EdgeChange::ArrowChanged { old: old.arrow, new: new.arrow });
    }
    if old.label != new.label {
        changes.push(EdgeChange::LabelChanged { old: old.label.clone(), new: new.label.clone() });
    }
    if old.er_notation != new.er_notation {
        changes.push(EdgeChange::ErNotationChanged {
            old: old.er_notation.clone(), new: new.er_notation.clone(),
        });
    }
    if old.metadata != new.metadata {
        changes.push(EdgeChange::MetadataChanged);
    }
    changes
}

pub(super) fn diff_edges(old: &MermaidDiagramIr, new: &MermaidDiagramIr) -> EdgeDiff {
    let old_styles = StyleDeclarations::new(old);
    let new_styles = StyleDeclarations::new(new);
    let old_snapshots: Vec<_> = (0..old.edges.len()).map(|i| edge_snapshot(old, i, &old_styles)).collect();
    let new_snapshots: Vec<_> = (0..new.edges.len()).map(|i| edge_snapshot(new, i, &new_styles)).collect();
    let mut old_groups = edge_groups(old);
    let mut new_groups = edge_groups(new);
    let pairs: BTreeSet<_> = old_groups.keys().chain(new_groups.keys()).cloned().collect();
    let mut edges = Vec::new();
    let mut details = Vec::new();
    for pair in pairs {
        let old_indices = old_groups.remove(&pair).unwrap_or_default();
        let new_indices = new_groups.remove(&pair).unwrap_or_default();
        let from_id = pair.0.display();
        let to_id = pair.1.display();
        let edge_result = |snapshot: &EdgeSnapshot, status, changes| DiffEdge {
            from_id: from_id.clone(), to_id: to_id.clone(), status,
            arrow: snapshot.arrow, changes,
        };
        // Exact semantic matches come first. A queue preserves multiplicity and
        // deterministic tie-breaking without pairwise scans of parallel edges.
        let mut available: BTreeMap<String, VecDeque<usize>> = BTreeMap::new();
        for &index in &new_indices {
            available.entry(format!("{:?}", new_snapshots[index])).or_default().push_back(index);
        }
        let mut matched = BTreeSet::new();
        let mut old_pending = Vec::new();
        for index in old_indices {
            let key = format!("{:?}", old_snapshots[index]);
            if let Some(new_index) = available.get_mut(&key).and_then(VecDeque::pop_front) {
                matched.insert(new_index);
                edges.push(edge_result(&new_snapshots[new_index], DiffStatus::Unchanged, Vec::new()));
            } else {
                old_pending.push(index);
            }
        }
        let new_pending: Vec<_> = new_indices.into_iter().filter(|index| !matched.contains(index)).collect();
        let paired = old_pending.len().min(new_pending.len());
        for (&old_index, &new_index) in old_pending.iter().zip(&new_pending) {
            let before = &old_snapshots[old_index];
            let after = &new_snapshots[new_index];
            let changes = compare_edges(before, after);
            let status = if changes.is_empty() { DiffStatus::Unchanged } else { DiffStatus::Changed };
            if before.metadata != after.metadata {
                details.push(DiagramChange {
                    field: format!("edge {from_id:?} -> {to_id:?} (old {old_index}, new {new_index}).metadata"),
                    before: before.metadata.clone(), after: after.metadata.clone(),
                });
            }
            edges.push(edge_result(after, status, changes));
        }
        for &index in &old_pending[paired..] {
            edges.push(edge_result(&old_snapshots[index], DiffStatus::Removed, Vec::new()));
        }
        for &index in &new_pending[paired..] {
            edges.push(edge_result(&new_snapshots[index], DiffStatus::Added, Vec::new()));
        }
    }
    let mut counts = (0, 0, 0, 0);
    for edge in &edges {
        match edge.status {
            DiffStatus::Added => counts.0 += 1,
            DiffStatus::Removed => counts.1 += 1,
            DiffStatus::Changed => counts.2 += 1,
            DiffStatus::Unchanged => counts.3 += 1,
        }
    }
    EdgeDiff { edges, counts, details }
}
