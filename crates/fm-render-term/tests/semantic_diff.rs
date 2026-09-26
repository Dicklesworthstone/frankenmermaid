//! Public-API regressions for changes that leave the graph topology unchanged.

use fm_core::{
    Diagnostic, DiagramType, GanttDate, IrGanttMeta, IrGanttTask, IrNode, IrNodeId,
    IrRadarAxis, IrRadarCurve, IrRadarMeta, IrTreemapItem, IrTreemapMeta,
    MermaidDiagramIr, Span,
};
use fm_render_term::diff::{
    diff_diagrams, render_diff_plain, render_diff_summary, render_diff_terminal,
    structural,
};

fn assert_field(old: &MermaidDiagramIr, new: &MermaidDiagramIr, field: &str) {
    let diff = diff_diagrams(old, new);
    assert!(diff.has_changes(), "{field} must affect has_changes");
    assert!(diff.total_changes() > 0, "{field} must affect total_changes");
    assert!(
        diff.diagram_changes.iter().any(|change| change.field == field),
        "missing {field}: {:?}",
        diff.diagram_changes
    );
}

#[test]
fn sequence_note_edit_is_not_reported_as_identical() {
    let old = fm_parser::parse("sequenceDiagram\nA->>B: request\nNote over A,B: before\n").ir;
    let new = fm_parser::parse("sequenceDiagram\nA->>B: request\nNote over A,B: after\n").ir;
    assert!(!structural::diff_diagrams(&old, &new).has_changes());
    assert_field(&old, &new, "sequence");
    let plain = render_diff_plain(&diff_diagrams(&old, &new));
    assert!(plain.contains("Diagram fields:"));
    assert!(plain.contains("before"));
    assert!(plain.contains("after"));
}

#[test]
fn sequence_message_reordering_is_semantic_not_just_a_multiset() {
    let old = fm_parser::parse("sequenceDiagram\nparticipant A\nparticipant B\nA->>B: first\nA->>B: second\n").ir;
    let new = fm_parser::parse("sequenceDiagram\nparticipant A\nparticipant B\nA->>B: second\nA->>B: first\n").ir;
    assert!(!structural::diff_diagrams(&old, &new).has_changes());
    assert_field(&old, &new, "sequence.message_order");
}

#[test]
fn sequence_participant_order_is_observable() {
    let old = fm_parser::parse("sequenceDiagram\nparticipant A\nparticipant B\nA->>B: request\n").ir;
    let new = fm_parser::parse("sequenceDiagram\nparticipant B\nparticipant A\nA->>B: request\n").ir;
    assert_field(&old, &new, "sequence.participant_order");
}

#[test]
fn sequence_activation_and_numbering_are_compared() {
    for (old_source, new_source) in [
        ("sequenceDiagram\nA->>B: call\n", "sequenceDiagram\nautonumber\nA->>B: call\n"),
        ("sequenceDiagram\nA->>B: call\n", "sequenceDiagram\nA->>+B: call\ndeactivate B\n"),
    ] {
        let old = fm_parser::parse(old_source).ir;
        let new = fm_parser::parse(new_source).ir;
        assert_field(&old, &new, "sequence");
    }
}

#[test]
fn chart_and_schedule_values_are_compared_through_real_parsing() {
    for (old_source, new_source, field) in [
        ("pie\n\"Sales\": 10\n", "pie\n\"Sales\": 20\n", "pie"),
        (
            "gantt\ndateFormat YYYY-MM-DD\nBuild :build, 2024-01-01, 2d\n",
            "gantt\ndateFormat YYYY-MM-DD\nBuild :build, 2024-01-01, 5d\n",
            "gantt",
        ),
        (
            "xychart-beta\nx-axis [Jan, Feb]\nbar [10, 20]\n",
            "xychart-beta\nx-axis [Jan, Feb]\nbar [10, 30]\n",
            "xychart",
        ),
    ] {
        assert_field(&fm_parser::parse(old_source).ir, &fm_parser::parse(new_source).ir, field);
    }
}

fn radar() -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Radar);
    ir.radar_meta = Some(IrRadarMeta {
        axes: vec![IrRadarAxis { id: "speed".into(), ..Default::default() }],
        curves: vec![IrRadarCurve {
            id: "current".into(), values: vec![5.0], ..Default::default()
        }],
        ..Default::default()
    });
    ir
}

fn treemap() -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Treemap);
    ir.treemap_meta = Some(IrTreemapMeta {
        nodes: vec![IrTreemapItem {
            label: "Revenue".into(), value: Some(10.0), ..Default::default()
        }],
        roots: vec![0],
    });
    ir
}

#[test]
fn metadata_only_chart_datasets_do_not_require_graph_nodes() {
    let old = radar();
    let mut new = old.clone();
    new.radar_meta.as_mut().unwrap().curves[0].values[0] = 9.0;
    assert!(!structural::diff_diagrams(&old, &new).has_changes());
    assert_field(&old, &new, "radar");

    let old = treemap();
    let mut new = old.clone();
    new.treemap_meta.as_mut().unwrap().nodes[0].value = Some(25.0);
    assert!(!structural::diff_diagrams(&old, &new).has_changes());
    assert_field(&old, &new, "treemap");
}

#[test]
fn chart_source_spans_and_diagnostics_do_not_create_changes() {
    for old in [radar(), treemap()] {
        let mut new = old.clone();
        if let Some(meta) = new.radar_meta.as_mut() {
            meta.axes[0].span = Span::at_line(100, 40);
            meta.curves[0].span = Span::at_line(101, 40);
        }
        if let Some(meta) = new.treemap_meta.as_mut() {
            meta.nodes[0].span = Span::at_line(100, 40);
        }
        new.diagnostics.push(Diagnostic::warning("different source position"));
        assert!(!diff_diagrams(&old, &new).has_changes());
    }
}

#[test]
fn backing_node_reindexing_does_not_change_a_schedule() {
    let mut old = MermaidDiagramIr::empty(DiagramType::Gantt);
    old.nodes = vec![
        IrNode { id: "A".into(), ..Default::default() },
        IrNode { id: "B".into(), ..Default::default() },
    ];
    old.gantt_meta = Some(IrGanttMeta {
        tasks: vec![IrGanttTask {
            node: IrNodeId(0), start: Some(GanttDate::Absolute("2024-01-01".into())),
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut new = old.clone();
    new.nodes.swap(0, 1);
    new.gantt_meta.as_mut().unwrap().tasks[0].node = IrNodeId(1);
    assert!(!diff_diagrams(&old, &new).has_changes());
    new.gantt_meta.as_mut().unwrap().tasks[0].node = IrNodeId(0);
    assert_field(&old, &new, "gantt");
}

#[test]
fn direction_titles_and_accessibility_are_observable() {
    let old = fm_parser::parse("flowchart TD\nA-->B\n").ir;
    let new = fm_parser::parse("flowchart LR\nA-->B\n").ir;
    assert_field(&old, &new, "direction");
    let mut new = old.clone();
    new.meta.title = Some("New architecture".into());
    new.meta.acc_descr = Some("Accessible explanation".into());
    assert_field(&old, &new, "title");
    assert_field(&old, &new, "accessibility.description");
}

#[test]
fn changing_type_without_graph_changes_is_not_identical() {
    assert_field(
        &MermaidDiagramIr::empty(DiagramType::Flowchart),
        &MermaidDiagramIr::empty(DiagramType::State),
        "diagram_type",
    );
}

#[test]
fn metadata_addition_and_removal_are_symmetric() {
    let old = MermaidDiagramIr::empty(DiagramType::Radar);
    let new = radar();
    let forward = diff_diagrams(&old, &new);
    let reverse = diff_diagrams(&new, &old);
    assert_eq!(forward.diagram_changes.len(), 1);
    assert_eq!(forward.diagram_changes[0].before, reverse.diagram_changes[0].after);
    assert_eq!(forward.diagram_changes[0].after, reverse.diagram_changes[0].before);
    assert_eq!(forward.total_changes(), reverse.total_changes());
}

#[test]
fn ordinary_graph_reports_remain_byte_identical() {
    let old = fm_parser::parse("flowchart TD\nA-->B\n").ir;
    let new = fm_parser::parse("flowchart TD\nA-->B\nB-->C\n").ir;
    let diff = diff_diagrams(&old, &new);
    let baseline = structural::diff_diagrams(&old, &new);
    assert!(diff.diagram_changes.is_empty());
    assert_eq!(diff.total_changes(), baseline.total_changes());
    assert_eq!(render_diff_plain(&diff), structural::render_diff_plain(&baseline));
    assert_eq!(render_diff_summary(&diff, true), structural::render_diff_summary(&baseline, true));
}

#[test]
fn side_by_side_reports_include_invisible_semantic_changes() {
    let old = fm_parser::parse("flowchart TD\nA-->B\n").ir;
    let mut new = old.clone();
    new.meta.acc_descr = Some("Changed accessibility metadata".into());
    let output = render_diff_terminal(&old, &new, 80, 24, false);
    assert!(output.contains("Diagram fields:"));
    assert!(output.contains("accessibility.description"));
    assert!(output.contains("Changed accessibility metadata"));
}

#[test]
fn new_report_values_escape_terminal_control_sequences() {
    let old = MermaidDiagramIr::empty(DiagramType::Flowchart);
    let mut new = old.clone();
    new.meta.acc_descr = Some("\u{1b}[31mspoof\nnext section".into());
    let output = render_diff_plain(&diff_diagrams(&old, &new));
    assert!(!output.contains('\u{1b}'));
    assert!(!output.contains("spoof\nnext section"));
}
