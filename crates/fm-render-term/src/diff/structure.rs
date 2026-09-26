//! Stable semantic views of groups, ports, constraints and shared style declarations.

use super::elements::{endpoint_key, intrinsic_edge_key};
use super::semantic::{DiagramChange, record};
use fm_core::{IrConstraint, IrEndpoint, IrLabelId, IrNodeId, IrStyleTarget, MermaidDiagramIr, Span};
use std::collections::{BTreeMap, BTreeSet};

type Properties = BTreeMap<String, String>;

fn label(ir: &MermaidDiagramIr, id: Option<IrLabelId>) -> String {
    match id {
        None => "None".to_string(),
        Some(id) => match ir.labels.get(id.0) {
            Some(value) => format!("{:?}", (
                &value.text,
                ir.label_markup.get(&id).map(Vec::as_slice).unwrap_or(&[]),
            )),
            None => format!("<invalid label {}>", id.0),
        },
    }
}

fn members(ir: &MermaidDiagramIr, ids: &[IrNodeId]) -> Vec<String> {
    let mut names: Vec<_> = ids.iter()
        .map(|id| format!("{:?}", endpoint_key(ir, IrEndpoint::Node(*id))))
        .collect();
    names.sort();
    names
}

/// A public key alone is not unique: repeated subgraphs may differ in title or
/// membership. Include both, and the ancestor chain, without following cycles.
fn subgraph_path(ir: &MermaidDiagramIr, index: usize) -> Vec<String> {
    let mut path = Vec::new();
    let mut seen = BTreeSet::new();
    let mut current = Some(index);
    while let Some(index) = current {
        if !seen.insert(index) {
            path.push("<cyclic parent>".to_string());
            break;
        }
        let Some(group) = ir.graph.subgraphs.get(index) else {
            path.push(format!("<invalid subgraph {index}>"));
            break;
        };
        path.push(format!("{:?}", (
            &group.key, label(ir, group.title), members(ir, &group.members),
        )));
        current = group.parent.map(|parent| parent.0);
    }
    path.reverse();
    path
}

struct GroupKeys {
    subgraphs: Vec<Vec<String>>,
    clusters: Vec<String>,
}

impl GroupKeys {
    fn new(ir: &MermaidDiagramIr) -> Self {
        let subgraphs: Vec<_> = (0..ir.graph.subgraphs.len())
            .map(|index| subgraph_path(ir, index)).collect();
        let mut owners: Vec<Vec<_>> = vec![Vec::new(); ir.clusters.len()];
        for (index, group) in ir.graph.subgraphs.iter().enumerate() {
            if let Some(cluster) = group.cluster
                && let Some(paths) = owners.get_mut(cluster.0)
            {
                paths.push(subgraphs[index].clone());
            }
        }
        let clusters = ir.clusters.iter().enumerate().map(|(index, cluster)| {
            owners[index].sort();
            format!("{:?}", (
                &owners[index], label(ir, cluster.title), members(ir, &cluster.members),
            ))
        }).collect();
        Self { subgraphs, clusters }
    }

    fn subgraph(&self, index: usize) -> String {
        self.subgraphs.get(index).map_or_else(
            || format!("<invalid subgraph {index}>"),
            |path| format!("{path:?}"),
        )
    }

    fn cluster(&self, index: usize) -> String {
        self.clusters.get(index).cloned()
            .unwrap_or_else(|| format!("<invalid cluster {index}>"))
    }
}

fn groups(ir: &MermaidDiagramIr, keys: &GroupKeys) -> (Vec<String>, Vec<String>) {
    let mut clusters: Vec<_> = ir.clusters.iter().enumerate().map(|(index, cluster)| {
        format!("{:?}", (
            keys.cluster(index), cluster.grid_span,
            &cluster.c4_boundary_type, &cluster.classes,
        ))
    }).collect();
    let mut subgraphs: Vec<_> = ir.graph.subgraphs.iter().enumerate().map(|(index, group)| {
        let children: Vec<_> = group.children.iter().map(|id| keys.subgraph(id.0)).collect();
        format!("{:?}", (
            keys.subgraph(index), group.direction, group.grid_span, children,
            group.cluster.map(|id| keys.cluster(id.0)),
        ))
    }).collect();
    clusters.sort();
    subgraphs.sort();
    (clusters, subgraphs)
}

fn ports(ir: &MermaidDiagramIr) -> Vec<String> {
    let mut result: Vec<_> = ir.ports.iter().map(|port| format!("{:?}", (
        endpoint_key(ir, IrEndpoint::Node(port.node)), &port.name, port.side_hint,
    ))).collect();
    result.sort();
    result
}

/// Merge repeated declarations for the same raw target before resolving identity.
/// In particular, two parallel edges with different linkStyle indices must retain
/// separate property maps even when their intrinsic edge identities are identical.
fn styles(ir: &MermaidDiagramIr, keys: &GroupKeys) -> Vec<String> {
    let mut definitions: BTreeMap<&str, Properties> = BTreeMap::new();
    for definition in &ir.style_defs {
        if !definition.properties.is_empty() {
            definitions.entry(&definition.name).or_default().extend(definition.properties.clone());
        }
    }
    let mut targets: BTreeMap<String, (&IrStyleTarget, Properties)> = BTreeMap::new();
    for reference in &ir.style_refs {
        let properties = fm_core::parse_style_string(&reference.style).properties;
        if !properties.is_empty() {
            targets.entry(format!("{:?}", reference.target))
                .or_insert_with(|| (&reference.target, Properties::new())).1.extend(properties);
        }
    }
    let mut result: Vec<_> = definitions.into_iter()
        .map(|(name, properties)| format!("{:?}", ("definition", name, properties))).collect();
    for (_, (target, properties)) in targets {
        let identity = match target {
            IrStyleTarget::Class(name) => format!("class {name:?}"),
            IrStyleTarget::Node(id) => format!("node {:?}", endpoint_key(ir, IrEndpoint::Node(*id))),
            IrStyleTarget::Link(index) => format!("link {}", intrinsic_edge_key(ir, *index)),
            IrStyleTarget::LinkDefault => "default links".to_string(),
            IrStyleTarget::Cluster(index) => format!("cluster {}", keys.cluster(*index)),
        };
        result.push(format!("{:?}", ("reference", identity, properties)));
    }
    result.sort();
    result
}

fn constraints(ir: &MermaidDiagramIr) -> Vec<IrConstraint> {
    let mut constraints = ir.constraints.clone();
    for constraint in &mut constraints {
        match constraint {
            IrConstraint::SameRank { node_ids, span }
            | IrConstraint::NonOverlap { node_ids, span, .. } => {
                node_ids.sort();
                *span = Span::default();
            }
            IrConstraint::MinLength { span, .. }
            | IrConstraint::Pin { span, .. }
            | IrConstraint::OrderInRank { span, .. } => *span = Span::default(),
        }
    }
    // Constraint order and OrderInRank member order are intentional solver input.
    constraints
}

pub(super) fn diff_structure(old: &MermaidDiagramIr, new: &MermaidDiagramIr) -> Vec<DiagramChange> {
    let mut changes = Vec::new();
    let old_keys = GroupKeys::new(old);
    let new_keys = GroupKeys::new(new);
    let (old_clusters, old_subgraphs) = groups(old, &old_keys);
    let (new_clusters, new_subgraphs) = groups(new, &new_keys);
    record(&mut changes, "clusters", &old_clusters, &new_clusters);
    record(&mut changes, "subgraphs", &old_subgraphs, &new_subgraphs);
    record(&mut changes, "ports", &ports(old), &ports(new));
    record(&mut changes, "styles", &styles(old, &old_keys), &styles(new, &new_keys));
    record(&mut changes, "constraints", &constraints(old), &constraints(new));
    changes
}
