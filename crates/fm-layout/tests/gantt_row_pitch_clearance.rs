//! GUARD: adjacent gantt task rows must keep a positive vertical gap (the row pitch must clear the bar height).
//!
//! THE REGRESSION. On `gantt_basic` the two task bars overlapped vertically: Design's bottom edge sat 2.18px below
//! Build's top edge (bar height 66.5px against a row pitch of node_spacing*0.72+24). The fix restores a positive gap
//! (measured ~6.3px) by sizing the bar height down and giving the row pitch a bar-height floor.
//!
//! The assertion is on the VERTICAL row gap of the `gantt_basic` golden fixture, which is exactly the quantity that
//! went negative. It prints the per-row gaps so the demonstration shows the number at every commit.

use fm_core::DiagramType;

// The gantt_basic golden fixture, verbatim.
const GANTT: &str = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  \
    Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n";

#[test]
fn gantt_task_rows_keep_a_positive_vertical_gap() {
    let ir = fm_parser::parse(GANTT).ir;
    assert_eq!(
        ir.diagram_type,
        DiagramType::Gantt,
        "fixture is not a gantt"
    );
    let layout = fm_layout::layout_diagram(&ir);

    // Task bars as (id, top, bottom), sorted top-to-bottom.
    let mut rows: Vec<(String, f32, f32)> = layout
        .nodes
        .iter()
        .map(|node| {
            (
                node.node_id.clone(),
                node.bounds.y,
                node.bounds.y + node.bounds.height,
            )
        })
        .collect();
    assert!(
        rows.len() >= 2,
        "the gantt laid out {} task rows; need >= 2",
        rows.len()
    );
    rows.sort_by(|a, b| a.1.partial_cmp(&b.1).expect("finite y"));

    let mut min_gap = f32::INFINITY;
    let mut worst = (String::new(), String::new());
    for pair in rows.windows(2) {
        let (prev_id, _prev_top, prev_bottom) = &pair[0];
        let (next_id, next_top, _next_bottom) = &pair[1];
        let gap = next_top - prev_bottom;
        eprintln!("GANTT_ROW_GAP {prev_id} -> {next_id}: {gap:.2}px");
        if gap < min_gap {
            min_gap = gap;
            worst = (prev_id.clone(), next_id.clone());
        }
    }
    eprintln!("GANTT_MIN_ROW_GAP {min_gap:.2}px (between {worst:?})");

    assert!(
        min_gap > 1.0,
        "adjacent gantt rows overlap: smallest vertical gap is {min_gap:.2}px between {worst:?}. \
         The row pitch must clear the bar height (gantt bars overlapped by ~2.18px before the fix).",
    );
}
