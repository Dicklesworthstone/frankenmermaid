//! What the terminal renderer does when a diagram is taller than the terminal.
//!
//! This file is tracked deliberately (bd-uk8w). The defect below was found under an UNTRACKED,
//! GITIGNORED reproducing test sitting in `crates/fm-cli/tests/` — a directory where cargo compiles
//! every top-level `.rs` as its own integration target with no `mod` declaration anywhere, so an
//! unanchored `repro_*.rs` ignore rule turned it into a gate that ran on one machine and existed
//! for nobody else. Its one assertion was `contains("Node49")`, which passes.

use fm_core::{
    ArrowType, DiagramType, GraphDirection, IrEdge, IrEndpoint, IrLabel, IrLabelId, IrNode,
    IrNodeId, MermaidDiagramIr,
};
use fm_render_term::{TermRenderConfig, render_term_with_config};

/// A vertical chain of `n` nodes: `Node0 --> Node1 --> ... --> Node{n-1}`.
fn vertical_chain(n: usize) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    ir.direction = GraphDirection::TB;
    for i in 0..n {
        ir.labels.push(IrLabel {
            text: format!("Node{i}"),
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: format!("N{i}"),
            label: Some(IrLabelId(i)),
            ..Default::default()
        });
        if i > 0 {
            ir.edges.push(IrEdge {
                from: IrEndpoint::Node(IrNodeId(i - 1)),
                to: IrEndpoint::Node(IrNodeId(i)),
                arrow: ArrowType::Arrow,
                ..Default::default()
            });
        }
    }
    ir
}

/// Indices of `Node{i}` labels ABSENT from the rendered text.
///
/// ⚠️ Deliberately NOT `output.contains("Node1")`: `Node1` is a substring of `Node11`, so a plain
/// `contains` reports absent labels as present and under-counts the loss. The label must not be
/// followed by another digit. A digit-suffixed label set is a booby trap for substring search.
fn labels_absent_from_output(output: &str, n: usize) -> Vec<usize> {
    (0..n)
        .filter(|i| {
            let needle = format!("Node{i}");
            !output.match_indices(&needle).any(|(at, _)| {
                !output[at + needle.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            })
        })
        .collect()
}

/// A chain far taller than the terminal loses nodes, and the result must SAY SO.
///
/// 50 boxes do not fit in 24 rows and clipping is a legitimate answer to an impossible viewport.
/// Reporting full fidelity while over half the diagram is missing is not.
///
/// The assertion that carries the weight is the LAST one: the geometric occlusion count and the
/// number of labels genuinely absent from the rendered text are two independent measurements of the
/// same loss, and they have to agree. A collision count that merely looked plausible would not be
/// evidence of anything.
#[test]
fn a_chain_taller_than_the_terminal_reports_the_nodes_it_could_not_draw() {
    let ir = vertical_chain(50);
    let result = render_term_with_config(&ir, &TermRenderConfig::default(), 80, 24);
    let absent = labels_absent_from_output(&result.output, 50);

    assert_eq!(
        result.node_count, 50,
        "node_count must keep meaning `nodes the layout produced`"
    );
    assert!(
        !absent.is_empty(),
        "50 nodes cannot fit in 24 rows; if none are missing this test no longer measures anything"
    );
    assert!(
        absent.contains(&0),
        "expected the root to be among the casualties; absent were {absent:?}"
    );
    assert_eq!(
        result.occluded_node_count,
        absent.len(),
        "the reported loss disagrees with the rendered text: reported {}, but {} labels are actually \
         absent ({absent:?})",
        result.occluded_node_count,
        absent.len()
    );
}

/// CONTROL: a diagram that fits must report ZERO loss.
///
/// A signal that fires on the easy case is worse than no signal, so this is the half of the
/// contract that keeps the new count from simply being alarmist.
#[test]
fn a_chain_that_fits_reports_no_loss() {
    let ir = vertical_chain(3);
    let result = render_term_with_config(&ir, &TermRenderConfig::default(), 200, 200);

    assert_eq!(
        labels_absent_from_output(&result.output, 3),
        Vec::<usize>::new(),
        "a 3-node chain in a 200x200 terminal must render whole"
    );
    assert_eq!(
        result.occluded_node_count, 0,
        "reported loss on a diagram that fits"
    );
    assert_eq!(result.node_count, 3);
}

/// CONTROL: rendering itself must not change — this is a reporting fix.
///
/// The renderer already kept its output inside the viewport, and a "fix" that bought fidelity by
/// letting the canvas grow past the terminal would wrap and corrupt the caller's screen while
/// making the test above pass.
#[test]
fn rendered_output_still_never_exceeds_the_requested_viewport() {
    let ir = vertical_chain(50);
    let (cols, rows) = (80, 24);
    let result = render_term_with_config(&ir, &TermRenderConfig::default(), cols, rows);

    assert!(
        result.width <= cols,
        "render reported width {} for a {cols}-column terminal",
        result.width
    );
    assert!(
        result.height <= rows,
        "render reported height {} for a {rows}-row terminal",
        result.height
    );
    for (n, line) in result.output.lines().enumerate() {
        assert!(
            line.chars().count() <= cols,
            "line {n} is {} columns wide in a {cols}-column terminal",
            line.chars().count()
        );
    }
}

/// The reported loss must track the rendered text at EVERY viewport, not just one.
///
/// This is the assertion that makes the count evidence rather than a plausible-looking number: five
/// viewports, and at each one the geometric count and the labels genuinely absent from the output
/// must agree. It also pins monotonicity — more room may never lose MORE nodes.
#[test]
fn reported_loss_tracks_the_rendered_text_at_every_viewport() {
    let ir = vertical_chain(50);
    let measure = |cols: usize, rows: usize| {
        let r = render_term_with_config(&ir, &TermRenderConfig::default(), cols, rows);
        let absent = labels_absent_from_output(&r.output, 50).len();
        assert_eq!(
            r.occluded_node_count, absent,
            "at {cols}x{rows} the reported loss is {} but {absent} labels are absent",
            r.occluded_node_count
        );
        absent
    };

    let cramped = measure(80, 24);
    let medium = measure(80, 60);
    let roomy = measure(80, 400);
    let huge = measure(400, 400);

    assert!(cramped > medium, "a 24-row terminal must lose more than a 60-row one");
    assert!(medium >= roomy && roomy >= huge, "more room must never lose more nodes");
    assert!(cramped > 0 && huge > 0, "this test measures nothing if nothing is ever lost");
}

/// GATE for the `base_scale` ceiling — the canvas never grows to the terminal it was given.
///
/// `layout_to_cell_dimensions` sizes the canvas as `bounds * base_scale` and then clamps that DOWN
/// to the viewport, so the hardcoded scale acts as an absolute ceiling: a 50-node chain renders onto
/// a 47x40 canvas and loses 12 nodes whether the terminal is 80x400 or 400x400. Enlarging the
/// terminal buys nothing.
///
/// Ignored because it fails today and the fix is a rendering change, tracked separately. It states
/// the contract a fix has to meet: given room, use it.
#[test]
#[ignore = "bd-beqx sibling: base_scale caps the canvas at 47x40 regardless of viewport"]
fn a_terminal_with_room_to_spare_loses_nothing() {
    let ir = vertical_chain(50);
    let r = render_term_with_config(&ir, &TermRenderConfig::default(), 400, 400);
    assert_eq!(
        r.occluded_node_count,
        0,
        "a 50-node chain in a 400x400 terminal still lost nodes; canvas was {}x{}",
        r.width,
        r.height
    );
}
