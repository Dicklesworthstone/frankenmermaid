//! The same graph can have different behavior, style, grouping or named ports.

use fm_core::{
    ArchitectureSide, ArrowType, C4RelationshipDirection, EdgeAnimation,
    GraphDirection, IrCluster, IrClusterId, IrConstraint, IrEdge, IrEndpoint,
    IrGitGraphMeta, IrInlineStyle, IrLabel, IrLabelId, IrLabelSegment, IrNode, IrNodeId,
    IrPacketField, IrPacketMeta, IrPort, IrPortId, IrPortSideHint, IrQuadrantMeta,
    IrQuadrantPoint, IrStateNote, IrStyleRef, IrStyleTarget, IrSubgraph, IrSubgraphId,
    MermaidDiagramIr, Span,
};
use fm_render_term::diff::{
    DiffStatus, EdgeChange, NodeChange, diff_diagrams, render_diff_plain,
    render_diff_summary, render_diff_terminal, structural,
};

fn graph() -> MermaidDiagramIr {
    fm_parser::parse("flowchart TD\nA-->B\n").ir
}

fn has_field(old: &MermaidDiagramIr, new: &MermaidDiagramIr, field: &str) {
    let diff = diff_diagrams(old, new);
    assert!(diff.has_changes());
    assert!(diff.diagram_changes.iter().any(|change| change.field == field),
        "missing {field}: {:?}", diff.diagram_changes);
}

#[test]
fn all_edge_metadata_channels_change_the_edge_count() {
    let old = graph();
    let edits: [fn(&mut IrEdge); 10] = [
        |e| e.extras_mut().source_cardinality = Some("1".into()),
        |e| e.extras_mut().target_cardinality = Some("many".into()),
        |e| e.extras_mut().guard = Some("authorized".into()),
        |e| e.extras_mut().action = Some("persist()".into()),
        |e| e.extras_mut().co_arrow = Some(ArrowType::CompositionReverse),
        |e| e.extras_mut().animation = Some(EdgeAnimation::Slow),
        |e| e.extras_mut().technology = Some("HTTPS".into()),
        |e| e.extras_mut().source_side = Some(ArchitectureSide::Right),
        |e| e.extras_mut().target_side = Some(ArchitectureSide::Left),
        |e| e.extras_mut().c4_direction = Some(C4RelationshipDirection::Up),
    ];
    for (index, edit) in edits.into_iter().enumerate() {
        let mut new = old.clone();
        edit(&mut new.edges[0]);
        let diff = diff_diagrams(&old, &new);
        assert_eq!(diff.changed_edges, 1, "edit {index}");
        assert_eq!(diff.unchanged_edges, 0, "edit {index}");
        assert_eq!(diff.total_changes(), 1, "metadata details are not counted twice");
        assert!(diff.edges[0].changes.contains(&EdgeChange::MetadataChanged));
        assert_eq!(diff.element_changes.len(), 1);
    }
}

#[test]
fn node_interaction_actor_and_inline_style_changes_promote_nodes_once() {
    let old = graph();
    let edits: [fn(&mut IrNode); 5] = [
        |n| n.interaction_mut().callback = Some("openPanel".into()),
        |n| n.interaction_mut().icon = Some("server".into()),
        |n| n.interaction_mut().link_target = Some("_self".into()),
        |n| n.journey_meta = Some(Box::new(fm_core::IrJourneyNodeMeta {
            actors: vec!["Alice Smith".into()],
        })),
        |n| n.inline_style = Some(Box::new(fm_core::parse_style_string("fill:#ff0000"))),
    ];
    for edit in edits {
        let mut new = old.clone();
        edit(&mut new.nodes[0]);
        let diff = diff_diagrams(&old, &new);
        assert_eq!(diff.changed_nodes, 1);
        assert_eq!(diff.unchanged_nodes, 1);
        assert_eq!(diff.total_changes(), 1);
        assert!(diff.nodes.iter().find(|n| n.id == "A").unwrap().changes
            .contains(&NodeChange::MetadataChanged));
        // A second, independently detected change must not count A a second time.
        new.nodes[0].shape = fm_core::NodeShape::Diamond;
        assert_eq!(diff_diagrams(&old, &new).changed_nodes, 1);
    }
}

#[test]
fn rich_label_edits_are_not_lost_when_plain_text_is_identical() {
    let mut old = graph();
    old.labels.push(IrLabel { text: "Name".into(), ..Default::default() });
    let label = IrLabelId(old.labels.len() - 1);
    old.nodes[0].label = Some(label);
    old.edges[0].label = Some(label);
    let mut new = old.clone();
    new.label_markup.insert(label, vec![IrLabelSegment::Text {
        text: "Name".into(), bold: true, italic: false, code: false, strike: false,
    }]);
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.changed_nodes, 1);
    assert_eq!(diff.changed_edges, 1);
}

#[test]
fn parser_classdef_edits_reach_only_nodes_using_that_class() {
    let old = fm_parser::parse("flowchart TD\nA-->B\nclassDef marked fill:#ff0000\nclass A marked\n").ir;
    let new = fm_parser::parse("flowchart TD\nA-->B\nclassDef marked fill:#0000ff\nclass A marked\n").ir;
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.changed_nodes, 1);
    assert_eq!(diff.nodes.iter().find(|n| n.id == "A").unwrap().status, DiffStatus::Changed);
    assert_eq!(diff.nodes.iter().find(|n| n.id == "B").unwrap().status, DiffStatus::Unchanged);
    has_field(&old, &new, "styles");
}

#[test]
fn parser_linkstyle_edits_reach_only_the_targeted_edge() {
    let old = fm_parser::parse("flowchart TD\nA-->B\nB-->C\nlinkStyle 0 stroke:#ff0000\n").ir;
    let new = fm_parser::parse("flowchart TD\nA-->B\nB-->C\nlinkStyle 0 stroke:#0000ff\n").ir;
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.changed_edges, 1);
    assert_eq!(diff.unchanged_edges, 1);
    assert_eq!(diff.edges.iter().find(|e| e.from_id == "A").unwrap().status, DiffStatus::Changed);
    has_field(&old, &new, "styles");
}

#[test]
fn parallel_edges_match_semantics_before_pairing_changes() {
    let mut old = graph();
    old.edges[0].extras_mut().guard = Some("first".into());
    let mut second = old.edges[0].clone();
    second.extras_mut().guard = Some("second".into());
    old.edges.push(second);
    let mut new = old.clone();
    new.edges.swap(0, 1);
    assert!(!diff_diagrams(&old, &new).has_changes());
    new.edges[0].extras_mut().guard = Some("updated".into());
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.changed_edges, 1);
    assert_eq!(diff.unchanged_edges, 1);
    assert!(diff.element_changes[0].before.contains("second"));
    assert!(diff.element_changes[0].after.contains("updated"));
}

#[test]
fn parallel_edge_multiplicity_is_preserved() {
    let mut old = graph();
    old.edges.push(old.edges[0].clone());
    let mut new = old.clone();
    new.edges.push(old.edges[0].clone());
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.added_edges, 1);
    assert_eq!(diff.unchanged_edges, 2);
    let reverse = diff_diagrams(&new, &old);
    assert_eq!(reverse.removed_edges, 1);
    assert_eq!(reverse.unchanged_edges, 2);
}

fn port_graph() -> MermaidDiagramIr {
    let mut ir = graph();
    ir.ports = vec![
        IrPort { node: IrNodeId(0), name: "out".into(), ..Default::default() },
        IrPort { node: IrNodeId(0), name: "other".into(), ..Default::default() },
    ];
    ir.edges[0].from = IrEndpoint::Port(IrPortId(0));
    ir
}

#[test]
fn changing_ports_on_the_same_node_changes_connectivity() {
    let old = port_graph();
    let mut new = old.clone();
    new.edges[0].from = IrEndpoint::Port(IrPortId(1));
    assert!(!structural::diff_diagrams(&old, &new).has_changes());
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.removed_edges, 1);
    assert_eq!(diff.added_edges, 1);
    assert!(diff.edges.iter().any(|e| e.from_id.contains("out")));
    assert!(diff.edges.iter().any(|e| e.from_id.contains("other")));
}

#[test]
fn port_arena_reindexing_is_not_a_change_but_side_hints_are() {
    let old = port_graph();
    let mut new = old.clone();
    new.ports.swap(0, 1);
    new.edges[0].from = IrEndpoint::Port(IrPortId(1));
    assert!(!diff_diagrams(&old, &new).has_changes());
    new.ports[1].side_hint = IrPortSideHint::Vertical;
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.changed_edges, 1);
    has_field(&old, &new, "ports");
}

#[test]
fn node_names_cannot_alias_typed_port_endpoints() {
    let mut old = port_graph();
    old.nodes.push(IrNode { id: "A::port(\"out\")".into(), ..Default::default() });
    let mut new = old.clone();
    new.edges[0].from = IrEndpoint::Node(IrNodeId(2));
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.added_edges, 1);
    assert_eq!(diff.removed_edges, 1);
}

#[test]
fn unresolved_and_invalid_endpoints_are_not_silently_dropped() {
    let old = graph();
    for endpoint in [IrEndpoint::Unresolved, IrEndpoint::Port(IrPortId(99)), IrEndpoint::Node(IrNodeId(99))] {
        let mut new = old.clone();
        new.edges.push(IrEdge { from: endpoint, to: IrEndpoint::Node(IrNodeId(0)), ..Default::default() });
        let diff = diff_diagrams(&old, &new);
        assert_eq!(diff.added_edges, 1);
        assert_eq!(diff.unchanged_edges, 1);
        assert_eq!(diff.edges.len(), 2);
    }
}

#[test]
fn empty_metadata_boxes_and_source_spans_are_not_changes() {
    let old = port_graph();
    let mut new = old.clone();
    new.edges[0].extras_mut();
    new.edges[0].inline_style = Some(Box::new(IrInlineStyle::default()));
    new.nodes[0].interaction_mut();
    new.nodes[0].inline_style = Some(Box::new(IrInlineStyle::default()));
    new.nodes[0].span_primary = Span::at_line(70, 20);
    new.edges[0].span = Span::at_line(80, 20);
    new.ports[0].span = Span::at_line(90, 20);
    assert!(!diff_diagrams(&old, &new).has_changes());
}

#[test]
fn semantic_message_order_includes_metadata_not_just_plain_labels() {
    let mut old = fm_parser::parse("sequenceDiagram\nA->>B: call\nA->>B: call\n").ir;
    old.edges[0].extras_mut().guard = Some("authorized".into());
    old.edges[1].extras_mut().guard = Some("expired".into());
    let mut new = old.clone();
    new.edges.swap(0, 1);
    let diff = diff_diagrams(&old, &new);
    assert_eq!(diff.unchanged_edges, 2);
    has_field(&old, &new, "sequence.message_order");
}

fn grouped() -> MermaidDiagramIr {
    let mut ir = graph();
    ir.clusters = vec![
        IrCluster { id: IrClusterId(0), members: vec![IrNodeId(0)], grid_span: 1, ..Default::default() },
        IrCluster { id: IrClusterId(1), members: vec![IrNodeId(1)], grid_span: 1, ..Default::default() },
    ];
    ir.graph.subgraphs = vec![
        IrSubgraph { id: IrSubgraphId(0), key: "one".into(), members: vec![IrNodeId(0)],
            cluster: Some(IrClusterId(0)), grid_span: 1, ..Default::default() },
        IrSubgraph { id: IrSubgraphId(1), key: "two".into(), members: vec![IrNodeId(1)],
            cluster: Some(IrClusterId(1)), grid_span: 1, ..Default::default() },
    ];
    ir
}

#[test]
fn groups_compare_membership_directions_classes_and_nesting() {
    let old = grouped();
    let mut new = old.clone();
    new.clusters[0].classes.push("alert".into());
    has_field(&old, &new, "clusters");
    let mut new = old.clone();
    new.graph.subgraphs[0].direction = Some(GraphDirection::LR);
    has_field(&old, &new, "subgraphs");
    let mut new = old.clone();
    new.graph.subgraphs[1].parent = Some(IrSubgraphId(0));
    new.graph.subgraphs[0].children.push(IrSubgraphId(1));
    has_field(&old, &new, "subgraphs");
    let mut new = old.clone();
    new.clusters[0].members.push(IrNodeId(1));
    has_field(&old, &new, "clusters");
}

#[test]
fn group_arena_reordering_preserves_group_and_style_identity() {
    let mut old = grouped();
    old.style_refs.push(IrStyleRef { target: IrStyleTarget::Cluster(0),
        style: "fill:#ff0000".into(), span: Span::default() });
    let mut new = old.clone();
    new.clusters.swap(0, 1);
    new.clusters[0].id = IrClusterId(0);
    new.clusters[1].id = IrClusterId(1);
    new.graph.subgraphs.swap(0, 1);
    for (index, group) in new.graph.subgraphs.iter_mut().enumerate() {
        group.id = IrSubgraphId(index);
        group.cluster = Some(IrClusterId(index));
        group.span = Span::at_line(100 + index, 30);
    }
    new.style_refs[0].target = IrStyleTarget::Cluster(1);
    new.style_refs[0].span = Span::at_line(200, 30);
    assert!(!diff_diagrams(&old, &new).has_changes());
}

#[test]
fn cyclic_or_invalid_group_parents_terminate_without_panicking() {
    let old = grouped();
    let mut new = old.clone();
    new.graph.subgraphs[0].parent = Some(IrSubgraphId(0));
    has_field(&old, &new, "subgraphs");
    new.graph.subgraphs[0].parent = Some(IrSubgraphId(99));
    has_field(&old, &new, "subgraphs");
}

#[test]
fn style_whitespace_and_source_positions_are_not_semantic_edits() {
    let mut old = graph();
    old.style_refs.push(IrStyleRef { target: IrStyleTarget::Node(IrNodeId(0)),
        style: "fill:#ff0000,stroke:#000000".into(), span: Span::default() });
    let mut new = old.clone();
    new.style_refs[0].style = "fill: #ff0000; stroke: #000000".into();
    new.style_refs[0].span = Span::at_line(100, 80);
    assert!(!diff_diagrams(&old, &new).has_changes());
}

#[test]
fn constraints_ignore_source_spans_but_preserve_ordered_members() {
    let mut old = graph();
    old.constraints.push(IrConstraint::OrderInRank {
        node_ids: vec!["A".into(), "B".into()], span: Span::default(),
    });
    let mut new = old.clone();
    if let IrConstraint::OrderInRank { span, .. } = &mut new.constraints[0] {
        *span = Span::at_line(100, 20);
    }
    assert!(!diff_diagrams(&old, &new).has_changes());
    if let IrConstraint::OrderInRank { node_ids, .. } = &mut new.constraints[0] {
        node_ids.reverse();
    }
    has_field(&old, &new, "constraints");
}

#[test]
fn packet_quadrant_state_notes_and_git_lanes_are_compared() {
    let mut old = graph();
    old.packet_meta = Some(IrPacketMeta {
        fields: vec![IrPacketField { node: IrNodeId(0), start_bit: 0, end_bit: 7 }],
    });
    let mut new = old.clone();
    new.packet_meta.as_mut().unwrap().fields[0].end_bit = 15;
    has_field(&old, &new, "packet");

    old.quadrant_meta = Some(IrQuadrantMeta { points: vec![IrQuadrantPoint {
        label: "A".into(), x: 0.1, y: 0.2,
    }], ..Default::default() });
    let mut new = old.clone();
    new.quadrant_meta.as_mut().unwrap().points[0].x = 0.9;
    has_field(&old, &new, "quadrant");

    old.state_notes.push(IrStateNote { target: "A".into(), position: "left".into(),
        text: "before".into(), span: Span::default() });
    let mut new = old.clone();
    new.state_notes[0].span = Span::at_line(50, 20);
    assert!(!diff_diagrams(&old, &new).has_changes());
    new.state_notes[0].text = "after".into();
    has_field(&old, &new, "state_notes");

    old.git_graph_meta = Some(IrGitGraphMeta { branches: vec!["main".into(), "feature".into()],
        ..Default::default() });
    let mut new = old.clone();
    new.git_graph_meta.as_mut().unwrap().commit_lanes.insert(0, 1);
    has_field(&old, &new, "gitgraph");
}

#[test]
fn plain_and_side_by_side_counts_include_metadata_only_edge_changes() {
    let old = graph();
    let mut new = old.clone();
    new.edges[0].extras_mut().guard = Some("authorized".into());
    let diff = diff_diagrams(&old, &new);
    let summary = render_diff_summary(&diff, false);
    assert!(summary.contains("Edges:\n  ~ 1 changed\n"));
    let plain = render_diff_plain(&diff);
    assert!(plain.contains("Element Details:"));
    assert!(plain.contains("authorized"));
    let visual = render_diff_terminal(&old, &new, 80, 24, false);
    assert!(visual.contains(&summary), "must replace the graph-only summary, not search for augmented counts");
    assert!(visual.contains("Element Details:"));
}

#[test]
fn edge_metadata_does_not_inject_terminal_escape_sequences() {
    let old = graph();
    let mut new = old.clone();
    new.edges[0].extras_mut().guard = Some("\u{1b}[31mspoof\nsection".into());
    let output = render_diff_plain(&diff_diagrams(&old, &new));
    assert!(!output.contains('\u{1b}'));
    assert!(!output.contains("spoof\nsection"));
}
