//! The `--` concurrency separator reaches the terminal (bd-dgnm4).
//!
//! `state Big { A --> B  --  C --> D }` declares two regions running in parallel. The layout
//! records each boundary in `extensions.cluster_dividers`; fm-render-svg drew a dashed line per
//! divider and fm-render-canvas now does too, but fm-render-term referenced that extension
//! nowhere — so the two regions ran together into one box and a reader could not tell there were
//! two. The separator is syntax the author wrote, not decoration.
//!
//! ISOLATED BY A MECHANISM CONTROL, not by diffing two sources. Deleting the `--` from the source
//! changes the region structure, hence the node placement, hence most of the picture: a whole-output
//! diff between those two renders would be dominated by relayout and would prove nothing about the
//! divider. Every test here instead renders the SAME ir and the SAME layout twice, differing only in
//! whether `extensions.cluster_dividers` is populated, so the cells that appear are the ones this
//! code path drew and nothing else.

use fm_core::MermaidDiagramIr;
use fm_layout::DiagramLayout;
use fm_render_term::{TermRenderConfig, render_term_with_layout_and_config};

/// Two concurrent regions inside one composite state.
const CONCURRENT: &str = "stateDiagram-v2\n    state Big {\n        [*] --> A\n        A --> B\n        --\n        [*] --> C\n        C --> D\n    }\n";

/// The same composite state with NO separator: one region, so no boundary exists to draw.
const SEQUENTIAL: &str = "stateDiagram-v2\n    state Big {\n        [*] --> A\n        A --> B\n        C --> D\n    }\n";

const COLS: usize = 120;
const ROWS: usize = 40;

fn parse(source: &str) -> (MermaidDiagramIr, DiagramLayout) {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    (ir, layout)
}

fn render(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Vec<Vec<char>> {
    render_term_with_layout_and_config(ir, layout, &TermRenderConfig::rich(), COLS, ROWS)
        .output
        .lines()
        .map(|line| line.chars().collect())
        .collect()
}

/// The (column, row) cells the dividers add, measured against the same layout with them removed.
fn cells_added_by_dividers(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Vec<(usize, usize)> {
    let mut without = layout.clone();
    without.extensions.cluster_dividers.clear();

    let after = render(ir, layout);
    let before = render(ir, &without);

    let mut added = Vec::new();
    for (row, line) in after.iter().enumerate() {
        for (col, ch) in line.iter().enumerate() {
            let was = before
                .get(row)
                .and_then(|previous| previous.get(col))
                .copied()
                .unwrap_or(' ');
            if *ch != was {
                added.push((col, row));
            }
        }
    }
    added
}

/// A separator produces a divider in the layout at all. Every other test depends on this, so it is
/// asserted on its own rather than folded in as a precondition.
#[test]
fn the_separator_reaches_the_layout_as_a_divider() {
    let (_, layout) = parse(CONCURRENT);
    assert!(
        !layout.extensions.cluster_dividers.is_empty(),
        "the `--` separator produced no cluster divider, so nothing downstream can draw one"
    );
}

/// The divider is DRAWN. Without the mechanism control this would be a test that the terminal draws
/// something somewhere, which a state diagram render satisfies trivially.
#[test]
fn a_concurrency_divider_puts_cells_on_the_canvas() {
    let (ir, layout) = parse(CONCURRENT);
    assert!(
        !layout.extensions.cluster_dividers.is_empty(),
        "CONTROL FAILED: no divider in this layout, so an empty diff would prove nothing"
    );

    let added = cells_added_by_dividers(&ir, &layout);
    assert!(
        !added.is_empty(),
        "populating extensions.cluster_dividers changed no cell in the terminal output"
    );
}

/// THE NEGATIVE CASE a naive implementation fails: a composite state WITHOUT a separator must not
/// grow a rule. An implementation that drew a line per CLUSTER rather than per DIVIDER would pass
/// the test above and fail this one.
#[test]
fn a_composite_state_without_a_separator_draws_no_divider() {
    let (ir, layout) = parse(SEQUENTIAL);
    assert!(
        layout.extensions.cluster_dividers.is_empty(),
        "a single-region composite state must publish no divider"
    );
    // Non-vacuity: this source must still produce a real composite state, or the assertion above
    // holds for the boring reason that there is no cluster at all.
    assert!(
        !layout.clusters.is_empty(),
        "CONTROL FAILED: this source produced no cluster, so it is not the case being tested"
    );

    let added = cells_added_by_dividers(&ir, &layout);
    assert!(
        added.is_empty(),
        "a composite state with no `--` gained {} cell(s) from the divider path",
        added.len()
    );
}

/// The divider is a HORIZONTAL rule, not a scribble. `build_state_cluster_dividers` emits
/// `start.y == end.y`, so every cell it adds must share one row per divider.
#[test]
fn the_divider_is_a_horizontal_rule() {
    let (ir, layout) = parse(CONCURRENT);
    let divider_rows = layout.extensions.cluster_dividers.len();
    let added = cells_added_by_dividers(&ir, &layout);
    assert!(!added.is_empty(), "no divider cells to check");

    let mut rows: Vec<usize> = added.iter().map(|(_, row)| *row).collect();
    rows.sort_unstable();
    rows.dedup();
    assert!(
        rows.len() <= divider_rows,
        "{divider_rows} divider(s) touched {} distinct terminal rows {rows:?}; a horizontal rule \
         occupies one row each",
        rows.len()
    );
}

/// DASHED, not solid. This is the assertion that carries the meaning: `render_cluster_canvas` draws
/// the composite state's own border with `draw_rect`, which is solid, so a solid divider would read
/// as a second box edge rather than as a region boundary. The SVG distinguishes the two with
/// `stroke-dasharray("6,4")`; the terminal has to distinguish them too, and at cell resolution the
/// only channel available is the gap.
#[test]
fn the_divider_is_dashed_and_the_gaps_survive_cell_quantisation() {
    let (ir, layout) = parse(CONCURRENT);
    let added = cells_added_by_dividers(&ir, &layout);
    assert!(!added.is_empty(), "no divider cells to check");

    // Group by row, then look for a gap inside each row's column span. A solid rule fills every
    // column between its endpoints; a dashed one does not.
    let mut rows: Vec<usize> = added.iter().map(|(_, row)| *row).collect();
    rows.sort_unstable();
    rows.dedup();

    let mut any_row_has_a_gap = false;
    for row in rows {
        let mut columns: Vec<usize> = added
            .iter()
            .filter(|(_, r)| *r == row)
            .map(|(col, _)| *col)
            .collect();
        columns.sort_unstable();
        let (Some(first), Some(last)) = (columns.first().copied(), columns.last().copied()) else {
            continue;
        };
        let span = last - first + 1;
        if columns.len() < span {
            any_row_has_a_gap = true;
        }
    }

    assert!(
        any_row_has_a_gap,
        "every divider row was a solid run of cells; a solid rule is indistinguishable from the \
         cluster border this divider sits inside"
    );
}

/// The divider stays INSIDE the composite state it divides. A rule that ran the full width of the
/// diagram would be drawn, horizontal and dashed, and still be wrong.
#[test]
fn the_divider_stays_within_its_cluster() {
    let (ir, layout) = parse(CONCURRENT);
    let added = cells_added_by_dividers(&ir, &layout);
    assert!(!added.is_empty(), "no divider cells to check");

    let widest = layout
        .clusters
        .iter()
        .map(|cluster| f64::from(cluster.bounds.width))
        .fold(0.0_f64, f64::max);
    let diagram_width = f64::from(layout.bounds.width);
    assert!(
        widest > 0.0 && diagram_width > 0.0,
        "CONTROL FAILED: degenerate bounds make this comparison meaningless"
    );

    let columns: Vec<usize> = added.iter().map(|(col, _)| *col).collect();
    let span = columns.iter().max().unwrap_or(&0) - columns.iter().min().unwrap_or(&0) + 1;
    let allowed = ((widest / diagram_width) * f64::from(u16::try_from(COLS).unwrap_or(u16::MAX)))
        .ceil() as usize
        + 2;
    assert!(
        span <= allowed,
        "divider spans {span} columns but the widest cluster is only {allowed} columns wide"
    );
}
