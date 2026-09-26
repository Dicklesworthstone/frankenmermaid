//! Document semantics that are not represented by the node/edge comparison.

use fm_core::{DiagramType, IrNodeId, MermaidDiagramIr, Span};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Debug;

/// A changed semantic field. Field names are stable; values are escaped diagnostic
/// representations, not a second serialized-IR schema. Source positions, diagnostics,
/// parser modes, support claims and runtime guard reports are deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiagramChange {
    pub field: String,
    pub before: String,
    pub after: String,
}

fn record<T: PartialEq + Debug>(
    changes: &mut Vec<DiagramChange>,
    field: &str,
    old: &T,
    new: &T,
) {
    if old != new {
        changes.push(DiagramChange {
            field: field.to_string(),
            before: format!("{old:?}"),
            after: format!("{new:?}"),
        });
    }
}

/// Normalize IR-local node indices into a common, name-ordered identity space.
/// Adding a node must not shift only one side of the comparison. Invalid indices
/// remain distinguishable and cannot alias a valid node.
fn remap_node(ir: &MermaidDiagramIr, names: &BTreeMap<&str, usize>, id: &mut IrNodeId) {
    id.0 = ir
        .nodes
        .get(id.0)
        .and_then(|node| names.get(node.id.as_str()))
        .copied()
        .unwrap_or_else(|| names.len().saturating_add(id.0));
}

fn ordered_messages(ir: &MermaidDiagramIr) -> Vec<String> {
    ir.edges
        .iter()
        .map(|edge| {
            let endpoint = |value| {
                ir.resolve_endpoint_node(value)
                    .and_then(|id| ir.node(id))
                    .map(|node| node.id.as_str())
            };
            let label = edge.label.and_then(|id| ir.labels.get(id.0));
            format!(
                "{:?}",
                (endpoint(edge.from), endpoint(edge.to), edge.arrow, label.map(|l| &l.text))
            )
        })
        .collect()
}

pub(super) fn diff_metadata(old: &MermaidDiagramIr, new: &MermaidDiagramIr) -> Vec<DiagramChange> {
    let mut changes = Vec::new();
    macro_rules! field {
        ($name:literal, $($member:ident).+) => {
            record(&mut changes, $name, &old.$($member).+, &new.$($member).+);
        };
    }
    field!("diagram_type", diagram_type);
    field!("direction", direction);
    field!("title", meta.title);
    field!("title_from_front_matter", meta.title_from_front_matter);
    field!("accessibility.title", meta.acc_title);
    field!("accessibility.description", meta.acc_descr);
    field!("layout.block_columns", meta.block_beta_columns);
    field!("layout.node_spacing", meta.node_spacing);
    field!("layout.rank_spacing", meta.rank_spacing);
    field!("layout.edge_routing", meta.edge_routing);
    field!("configuration", meta.init.config);
    field!("theme", meta.theme_overrides);
    field!("c4.legend", meta.c4_show_legend);
    field!("pie", pie_meta);

    let names: BTreeMap<&str, usize> = old
        .nodes
        .iter()
        .chain(&new.nodes)
        .map(|node| node.id.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .enumerate()
        .map(|(index, name)| (name, index))
        .collect();

    let sequence = |ir: &MermaidDiagramIr| {
        ir.sequence_meta.clone().map(|mut meta| {
            for activation in &mut meta.activations {
                remap_node(ir, &names, &mut activation.participant);
            }
            for note in &mut meta.notes {
                for participant in &mut note.participants {
                    remap_node(ir, &names, participant);
                }
            }
            for group in &mut meta.participant_groups {
                for participant in &mut group.participants {
                    remap_node(ir, &names, participant);
                }
            }
            for event in &mut meta.lifecycle_events {
                remap_node(ir, &names, &mut event.participant);
            }
            meta
        })
    };
    record(&mut changes, "sequence", &sequence(old), &sequence(new));
    if old.diagram_type == DiagramType::Sequence || new.diagram_type == DiagramType::Sequence {
        let participants = |ir: &MermaidDiagramIr| {
            ir.nodes.iter().map(|node| node.id.clone()).collect::<Vec<_>>()
        };
        record(
            &mut changes,
            "sequence.participant_order",
            &participants(old),
            &participants(new),
        );
        let old_order = ordered_messages(old);
        let new_order = ordered_messages(new);
        if old_order != new_order {
            // Ordinary edge edits already have structural records. A pure reorder is
            // otherwise invisible because graph edges are compared as a multiset.
            let mut old_set = old_order.clone();
            let mut new_set = new_order.clone();
            old_set.sort();
            new_set.sort();
            if old_set == new_set {
                record(&mut changes, "sequence.message_order", &old_order, &new_order);
            }
        }
    }

    let gantt = |ir: &MermaidDiagramIr| {
        ir.gantt_meta.clone().map(|mut meta| {
            for task in &mut meta.tasks {
                remap_node(ir, &names, &mut task.node);
            }
            meta
        })
    };
    record(&mut changes, "gantt", &gantt(old), &gantt(new));

    let xy = |ir: &MermaidDiagramIr| {
        ir.xy_chart_meta.clone().map(|mut meta| {
            for series in &mut meta.series {
                for node in &mut series.nodes {
                    remap_node(ir, &names, node);
                }
            }
            meta
        })
    };
    record(&mut changes, "xychart", &xy(old), &xy(new));

    let treemap = |ir: &MermaidDiagramIr| {
        ir.treemap_meta.clone().map(|mut meta| {
            for item in &mut meta.nodes {
                item.span = Span::default();
            }
            meta
        })
    };
    record(&mut changes, "treemap", &treemap(old), &treemap(new));

    let radar = |ir: &MermaidDiagramIr| {
        ir.radar_meta.clone().map(|mut meta| {
            for axis in &mut meta.axes {
                axis.span = Span::default();
            }
            for curve in &mut meta.curves {
                curve.span = Span::default();
            }
            meta
        })
    };
    record(&mut changes, "radar", &radar(old), &radar(new));
    changes
}
