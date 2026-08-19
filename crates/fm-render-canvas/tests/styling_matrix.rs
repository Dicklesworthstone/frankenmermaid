//! THE STYLING MATRIX: which SURFACE honours which PROPERTY, enumerated (bd-lvj3).
//!
//! bd-lvj3 has looked finished three times, and each probe wave found more missing channels. The
//! reason is that the gaps are not a list — they are a MATRIX with holes. Each drawing surface
//! reads only the properties someone happened to add to it, so "is styling supported?" has no
//! single answer; it has one answer per (surface, property) pair.
//!
//! This file enumerates the pairs and asserts reality matches the table, IN BOTH DIRECTIONS:
//!
//!   * `Honoured` that stops working is a REGRESSION and fails here.
//!   * `KnownGap` that starts working also FAILS, demanding the row be flipped. That direction is
//!     the one that keeps this file honest: without it the table rots into a list of stale
//!     excuses, every gap stays "known" forever, and the file quietly stops describing the code.
//!
//! It is deliberately a coarse instrument. It asks only "did the declared value reach the
//! operation stream at all" — not whether it landed on the right shape, in the right order, or
//! got restored afterwards. Those are the dedicated tests in node_styling.rs, edge_label_color.rs,
//! node_dash.rs, cluster_border.rs and the rest, which is where the real controls live. This is
//! the map, not the territory: its job is to make the SHAPE of the remaining work visible and to
//! stop the bead being declared done off a sample.
//!
//! Each case declares EXACTLY ONE property, which is what makes a shared needle unambiguous — if
//! only `color` is declared, a `SetFillStyle("#ff00ff")` can only have come from the text.
//!
//! ⚠️ HOW MUCH OF THE RENDERER THIS TABLE ACTUALLY COVERS — read this before concluding from a
//! green run that canvas styling is finished. Every row is `Honoured`, and that means every pair
//! ENUMERATED here works. It does not mean the renderer honours styling.
//!
//! Measured by auditing which `draw_*` method in `renderer.rs` consults any `resolve_*` helper:
//! THREE of NINETEEN do.
//!
//!     consults styling   draw_nodes, draw_edges, draw_clusters
//!     consults none      draw_pie_wedges, draw_sequence_fragments, draw_sequence_notes,
//!                        draw_sequence_mirror_headers, draw_sequence_lifecycle_markers,
//!                        draw_activation_bars, draw_state_notes, draw_gantt_today_marker,
//!                        draw_quadrant_axis_labels, draw_packet_field_continuations,
//!                        draw_bands, draw_axis_ticks, draw_cluster_dividers, draw_marker,
//!                        draw_path_markers, draw_generic_diagram_title
//!
//! So a sequence fragment, a pie wedge, a gantt marker or a state note cannot honour a declared
//! anything — there is no code path from the merge chain to those surfaces at all. The three
//! surfaces this table covers are the three that were wired; the other sixteen were never asked.
//!
//! ⚠️ The first version of that audit reported `draw_pie_wedges` as consulting EVERY resolver. It
//! was an artifact: the scan ran past the end of the `impl` block and swept in the free `resolve_*`
//! functions defined below it. Bounded to the impl, pie wedges consult none. Recorded because the
//! wrong answer was the flattering one, and it would have shrunk this list by the single most
//! interesting entry.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Support {
    /// The declared value reaches the canvas today.
    Honoured,
    /// It does not. Confirmed against the SVG arm, which emits it — so this is a real
    /// disagreement between the two renderers, not a property nobody supports.
    ///
    /// Currently unconstructed, because every row is `Honoured`. KEPT DELIBERATELY: this variant
    /// is the vocabulary for recording the next gap someone finds, and the bidirectional check in
    /// `the_styling_matrix_matches_the_table` is built around it. Deleting it to silence a
    /// dead-code warning would mean the next person has to rebuild the mechanism before they can
    /// record a fact — which is how a gap ends up in a commit message instead of a gate.
    #[allow(dead_code)]
    KnownGap,
}

struct Case {
    surface: &'static str,
    property: &'static str,
    source: &'static str,
    needle: &'static str,
    expected: Support,
}

const SUBGRAPH_HEAD: &str = "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n";

fn cases() -> Vec<Case> {
    use Support::Honoured;
    vec![
        // ── node ──────────────────────────────────────────────────────────────────────────
        Case { surface: "node", property: "fill", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a fill:#ff00ff\n",
            needle: "SetFillStyle(\"#ff00ff\")" },
        Case { surface: "node", property: "stroke", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a stroke:#ff00ff\n",
            needle: "SetStrokeStyle(\"#ff00ff\")" },
        Case { surface: "node", property: "stroke-width", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a stroke-width:4px\n",
            needle: "SetLineWidth(4.0)" },
        Case { surface: "node", property: "stroke-dasharray", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a stroke-dasharray:5 5\n",
            needle: "SetLineDash([5.0, 5.0])" },
        Case { surface: "node", property: "color", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a color:#ff00ff\n",
            needle: "SetFillStyle(\"#ff00ff\")" },
        Case { surface: "node", property: "font-size", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a font-size:32px\n",
            needle: "SetFont(\"32px" },
        Case { surface: "node", property: "font-weight", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a font-weight:bold\n",
            needle: "SetFont(\"bold" },
        Case { surface: "node", property: "opacity", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a opacity:0.5\n",
            needle: "SetGlobalAlpha(0.5)" },
        Case { surface: "node", property: "font-style", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a font-style:italic\n",
            needle: "italic" },
        Case { surface: "node", property: "font-family", expected: Honoured,
            source: "flowchart TD\n  a[Alpha]\n  style a font-family:Courier\n",
            needle: "Courier" },

        // ── edge ──────────────────────────────────────────────────────────────────────────
        Case { surface: "edge", property: "stroke", expected: Honoured,
            source: "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke:#ff00ff\n",
            needle: "SetStrokeStyle(\"#ff00ff\")" },
        Case { surface: "edge", property: "stroke-width", expected: Honoured,
            source: "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-width:4px\n",
            needle: "SetLineWidth(4.0)" },
        Case { surface: "edge", property: "stroke-dasharray", expected: Honoured,
            source: "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 stroke-dasharray:7 3\n",
            needle: "SetLineDash([7.0, 3.0])" },
        Case { surface: "edge", property: "color", expected: Honoured,
            source: "flowchart TD\n  a[A] -->|hi| b[B]\n  linkStyle 0 color:#ff00ff\n",
            needle: "SetFillStyle(\"#ff00ff\")" },
        Case { surface: "edge", property: "opacity", expected: Honoured,
            source: "flowchart TD\n  a[A] --> b[B]\n  linkStyle 0 opacity:0.5\n",
            needle: "SetGlobalAlpha(0.5)" },
        Case { surface: "edge", property: "font-size", expected: Honoured,
            source: "flowchart TD\n  a[A] -->|hi| b[B]\n  linkStyle 0 font-size:22px\n",
            needle: "SetFont(\"22px" },

        // ── cluster ───────────────────────────────────────────────────────────────────────
        Case { surface: "cluster", property: "fill", expected: Honoured,
            source: "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one fill:#ff00ff\n",
            needle: "SetFillStyle(\"#ff00ff\")" },
        Case { surface: "cluster", property: "stroke", expected: Honoured,
            source: "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one stroke:#ff00ff\n",
            needle: "SetStrokeStyle(\"#ff00ff\")" },
        Case { surface: "cluster", property: "stroke-width", expected: Honoured,
            source: "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one stroke-width:5px\n",
            needle: "SetLineWidth(5.0)" },
        Case { surface: "cluster", property: "stroke-dasharray", expected: Honoured,
            source: "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one stroke-dasharray:9 4\n",
            needle: "SetLineDash([9.0, 4.0])" },
        Case { surface: "cluster", property: "color", expected: Honoured,
            source: "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one color:#ff00ff\n",
            needle: "SetFillStyle(\"#ff00ff\")" },
        Case { surface: "cluster", property: "opacity", expected: Honoured,
            source: "flowchart TD\n  subgraph one[One]\n    a[Alpha]\n  end\n  style one opacity:0.5\n",
            needle: "SetGlobalAlpha(0.5)" },

        // ── class-diagram compartment ─────────────────────────────────────────────────────
        // The compartment labels derive their own smaller fonts and are not rescaled, so a
        // font-size on a class node disagrees with the SVG arm, which cascades it to the whole
        // element. Recorded when the node font-size landed, deliberately not half-fixed there.
        Case { surface: "compartment", property: "font-size", expected: Honoured,
            source: "classDiagram\n  class A {\n    +int x\n  }\n  style A font-size:30px\n",
            needle: "SetFont(\"30px" },
    ]
}

fn canvas_ops(source: &str) -> String {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1400.0, 1000.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    format!("{:?}", context.operations())
}

/// Every (surface, property) pair matches its recorded support, in both directions.
#[test]
fn the_styling_matrix_matches_the_table() {
    let mut wrong: Vec<String> = Vec::new();

    for case in cases() {
        let ops = canvas_ops(case.source);
        let reached = ops.to_lowercase().contains(&case.needle.to_lowercase());

        match (case.expected, reached) {
            (Support::Honoured, false) => wrong.push(format!(
                "REGRESSION: {} / {} is recorded Honoured but the declared value no longer \
                 reaches the canvas (needle {:?})",
                case.surface, case.property, case.needle
            )),
            (Support::KnownGap, true) => wrong.push(format!(
                "FIXED, UPDATE THE TABLE: {} / {} is recorded KnownGap but now reaches the \
                 canvas (needle {:?}). Flip it to Honoured.",
                case.surface, case.property, case.needle
            )),
            _ => {}
        }
    }

    assert!(wrong.is_empty(), "styling matrix is out of date:\n  {}", wrong.join("\n  "));
}

/// CONTROL: every case renders a real diagram.
///
/// Every assertion above is a substring scan, and a case whose source failed to parse would render
/// a stub — making a `KnownGap` row pass by having nothing to find. That is the failure mode this
/// file is least able to notice on its own, because a stub looks exactly like an unsupported
/// property.
#[test]
fn every_matrix_case_renders_something_real() {
    for case in cases() {
        let ops = canvas_ops(case.source);

        assert!(
            ops.len() > 200,
            "{} / {}: rendered only {} bytes, so its matrix row proves nothing",
            case.surface,
            case.property,
            ops.len()
        );
        assert!(
            ops.contains("FillText("),
            "{} / {}: nothing was drawn as text, so the source probably did not parse",
            case.surface,
            case.property
        );
    }
}

/// CONTROL: the table covers every surface, and no row is duplicated.
///
/// A matrix whose rows silently collapsed to one surface would still pass the test above while
/// describing almost nothing.
#[test]
fn the_table_covers_every_surface_without_duplicates() {
    let all = cases();

    let mut seen: Vec<(&str, &str)> = all.iter().map(|c| (c.surface, c.property)).collect();
    seen.sort_unstable();
    let before = seen.len();
    seen.dedup();
    assert_eq!(before, seen.len(), "the matrix lists a (surface, property) pair twice");

    for surface in ["node", "edge", "cluster", "compartment"] {
        assert!(
            all.iter().any(|c| c.surface == surface),
            "the matrix has no rows for the {surface} surface"
        );
    }
    // THIS ASSERTION USED TO REQUIRE A `KnownGap` ROW, and it fired when the last one closed —
    // which is exactly what it was for: "either this bead is closeable or the table rotted, and
    // both deserve a human look". It was the first, so the check is replaced rather than deleted.
    //
    // What guards the table now is its SIZE. Every row is `Honoured`, so the bidirectional check
    // has only one direction left to catch, and a table that silently lost rows would pass
    // everything while asserting less and less. A floor makes shrinkage a failure.
    assert!(
        all.len() >= 23,
        "the styling matrix shrank to {} rows; rows may be removed only when the property itself \
         is gone, never to make a failure go away",
        all.len()
    );
    assert!(
        all.iter().all(|c| c.expected == Support::Honoured),
        "a KnownGap row reappeared — record why in the bead before letting this stand"
    );
}

/// The subgraph fixture is shared by several rows; keep it referenced so it cannot drift unused.
#[test]
fn the_subgraph_fixture_parses() {
    let ops = canvas_ops(SUBGRAPH_HEAD);
    assert!(ops.contains("FillText(\"Alpha\""), "the shared subgraph fixture did not render");
}
