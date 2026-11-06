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

use fm_core::{DiagramType, MermaidDiagramIr, MermaidGlyphMode, MermaidRenderMode, MermaidTier};
use fm_layout::{
    DiagramLayout, LayoutClusterBox, LayoutClusterDivider, LayoutExtensions, LayoutPoint,
    LayoutRect, LayoutStats,
};
use fm_render_term::{
    EdgeGlyphs, ResolvedConfig, TermRenderConfig, TermRenderer, render_term_with_layout_and_config,
};

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

fn render(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &TermRenderConfig,
) -> Vec<Vec<char>> {
    render_term_with_layout_and_config(ir, layout, config, COLS, ROWS)
        .output
        .lines()
        .map(|line| line.chars().collect())
        .collect()
}

/// The (column, row) cells the dividers add, measured against the same layout with them removed.
fn cells_added_by_dividers(ir: &MermaidDiagramIr, layout: &DiagramLayout) -> Vec<(usize, usize)> {
    cells_added_by_dividers_with_config(ir, layout, &TermRenderConfig::rich())
}

fn cells_added_by_dividers_with_config(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &TermRenderConfig,
) -> Vec<(usize, usize)> {
    let mut without = layout.clone();
    without.extensions.cluster_dividers.clear();

    let after = render(ir, layout, config);
    let before = render(ir, &without, config);

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

    for config in [
        TermRenderConfig::rich(),
        TermRenderConfig::compact(),
        TermRenderConfig {
            render_mode: MermaidRenderMode::CellOnly,
            ..TermRenderConfig::rich()
        },
    ] {
        let added = cells_added_by_dividers_with_config(&ir, &layout, &config);
        assert!(
            !added.is_empty(),
            "populating extensions.cluster_dividers changed no cell in {:?}/{:?} output",
            config.tier,
            config.render_mode,
        );
    }
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

/// The divider follows the layout axis. Side-by-side regions need a vertical rule, while
/// stacked regions need a horizontal one; forcing either orientation can cut through states.
#[test]
fn the_divider_follows_its_layout_orientation() {
    let (ir, layout) = parse(CONCURRENT);
    assert_eq!(layout.extensions.cluster_dividers.len(), 1);
    let divider = &layout.extensions.cluster_dividers[0];
    let vertical = divider.start.x == divider.end.x;
    assert!(vertical || divider.start.y == divider.end.y);
    let added = cells_added_by_dividers(&ir, &layout);
    assert!(!added.is_empty(), "no divider cells to check");

    let mut cross_axis: Vec<usize> = added
        .iter()
        .map(|&(col, row)| if vertical { col } else { row })
        .collect();
    cross_axis.sort_unstable();
    cross_axis.dedup();
    assert_eq!(
        cross_axis.len(),
        1,
        "{divider:?} touched multiple cells across its axis: {cross_axis:?}",
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
    assert_eq!(layout.extensions.cluster_dividers.len(), 1);
    let divider = &layout.extensions.cluster_dividers[0];
    let vertical = divider.start.x == divider.end.x;
    // Labels can erase pixels on a solid rule and create false dash gaps. Test the geometry
    // without that text overlay, still using exactly the same IR and layout for the control.
    let added = cells_added_by_dividers_with_config(
        &ir,
        &layout,
        &TermRenderConfig {
            show_labels: false,
            ..TermRenderConfig::rich()
        },
    );
    assert!(!added.is_empty(), "no divider cells to check");

    let mut positions: Vec<usize> = added
        .iter()
        .map(|&(col, row)| if vertical { row } else { col })
        .collect();
    positions.sort_unstable();
    positions.dedup();
    let span = positions.last().unwrap() - positions.first().unwrap() + 1;
    assert!(
        positions.len() < span,
        "{divider:?} was a solid run along its axis: {positions:?}; the boundary must retain dash gaps"
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

/// Explicit geometry makes both orientations mandatory, independently of how the layout
/// chooses to pack a parsed fixture. The empty interior also prevents labels or crossing
/// edges from supplying accidental gaps to an otherwise solid boundary.
#[test]
fn both_divider_axes_survive_all_terminal_modes() {
    let ir = MermaidDiagramIr::empty(DiagramType::State);
    for (start, end) in [
        (
            LayoutPoint { x: 20.0, y: 50.0 },
            LayoutPoint { x: 80.0, y: 50.0 },
        ),
        (
            LayoutPoint { x: 50.0, y: 20.0 },
            LayoutPoint { x: 50.0, y: 80.0 },
        ),
    ] {
        let layout = DiagramLayout {
            nodes: Vec::new(),
            clusters: vec![LayoutClusterBox {
                cluster_index: 0,
                span: Default::default(),
                title: None,
                color: None,
                bounds: LayoutRect {
                    x: 10.0,
                    y: 10.0,
                    width: 80.0,
                    height: 80.0,
                },
            }],
            cycle_clusters: Vec::new(),
            edges: Vec::new(),
            bounds: LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            stats: LayoutStats::default(),
            extensions: LayoutExtensions {
                cluster_dividers: vec![LayoutClusterDivider {
                    cluster_index: 0,
                    start,
                    end,
                }],
                ..Default::default()
            },
            dirty_regions: Vec::new(),
        };
        for (tier, mode, glyph_mode) in [
            (
                MermaidTier::Compact,
                MermaidRenderMode::CellOnly,
                MermaidGlyphMode::Unicode,
            ),
            (
                MermaidTier::Compact,
                MermaidRenderMode::Braille,
                MermaidGlyphMode::Unicode,
            ),
            (
                MermaidTier::Normal,
                MermaidRenderMode::CellOnly,
                MermaidGlyphMode::Unicode,
            ),
            (
                MermaidTier::Normal,
                MermaidRenderMode::CellOnly,
                MermaidGlyphMode::Ascii,
            ),
            (
                MermaidTier::Rich,
                MermaidRenderMode::Block,
                MermaidGlyphMode::Unicode,
            ),
            (
                MermaidTier::Rich,
                MermaidRenderMode::HalfBlock,
                MermaidGlyphMode::Unicode,
            ),
            (
                MermaidTier::Rich,
                MermaidRenderMode::Braille,
                MermaidGlyphMode::Unicode,
            ),
        ] {
            let config = TermRenderConfig {
                tier,
                render_mode: mode,
                glyph_mode,
                show_labels: false,
                show_clusters: true,
                max_width: COLS,
                max_height: ROWS,
                ..TermRenderConfig::rich()
            };
            let case = format!("{tier:?}/{mode:?}/{glyph_mode:?}, {start:?} -> {end:?}");
            let added = cells_added_by_dividers_with_config(&ir, &layout, &config);
            assert!(
                added.len() >= 2,
                "{case}: the separator is missing or collapsed to a point"
            );
            let renderer = TermRenderer::new(ResolvedConfig::resolve(&config, COLS, ROWS));
            let (x0, y0) = renderer.map_layout_point_to_cells(&layout, ir.direction, start);
            let (x1, y1) = renderer.map_layout_point_to_cells(&layout, ir.direction, end);
            let vertical = start.x == end.x;
            let fixed = if vertical { x0 } else { y0 };
            let (first, last) = if vertical { (y0, y1) } else { (x0, x1) };
            let (left, top) = renderer.map_layout_point_to_cells(
                &layout,
                ir.direction,
                LayoutPoint { x: 10.0, y: 10.0 },
            );
            let (right, bottom) = renderer.map_layout_point_to_cells(
                &layout,
                ir.direction,
                LayoutPoint { x: 90.0, y: 90.0 },
            );
            for &(col, row) in &added {
                assert_eq!(
                    if vertical { col } else { row },
                    fixed,
                    "{case}: separator moved across its layout axis"
                );
                let along = if vertical { row } else { col };
                assert!(
                    (first.min(last)..=first.max(last)).contains(&along),
                    "{case}: separator escaped its source endpoints at ({col}, {row})"
                );
                assert!(
                    col > left && col < right && row > top && row < bottom,
                    "{case}: separator escaped its cluster at ({col}, {row})"
                );
            }
            let mut positions: Vec<_> = added
                .iter()
                .map(|&(col, row)| if vertical { row } else { col })
                .collect();
            positions.sort_unstable();
            positions.dedup();
            let span = positions.last().unwrap() - positions.first().unwrap() + 1;
            assert!(
                span >= first.abs_diff(last) / 2,
                "{case}: separator covers too little of its region"
            );
            assert!(
                positions.len() < span,
                "{case}: dashed separator became a solid cell run"
            );

            if tier == MermaidTier::Compact || mode == MermaidRenderMode::CellOnly {
                let glyphs = EdgeGlyphs::for_mode(glyph_mode);
                let expected = if vertical {
                    glyphs.dotted_v
                } else {
                    glyphs.dotted_h
                };
                let rendered = render(&ir, &layout, &config);
                assert!(
                    added
                        .iter()
                        .all(|&(col, row)| rendered[row][col] == expected),
                    "{case}: cell separator lost its dashed glyph"
                );
            }
        }
    }
}
