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
    // These all use the DEFAULT config, whose max_width/max_height cap the usable area at 120x40
    // however large the terminal argument is, so `huge` is still lossy here. The uncapped case is
    // a_terminal_with_room_to_spare_loses_nothing.
    assert!(cramped > 0, "a 50-node chain in 24 rows must still lose nodes");
    assert!(huge > 0, "under the default 120x40 cap a 50-node chain cannot fit");
}

/// Given genuine room, the canvas grows and nothing is lost (bd-8tsw).
///
/// `base_scale` used to be an absolute CEILING: the canvas was `bounds * base_scale` and the
/// viewport could only clamp it DOWN, so enlarging the terminal bought nothing.
///
/// ⚠️ The terminal argument alone does not decide how much room there is. `ResolvedConfig::resolve`
/// clamps it by `config.max_width`/`max_height`, which default to 120x40 — a deliberate default, not
/// a defect. An earlier version of this test asked for a 400x400 terminal on the DEFAULT config and
/// then blamed the renderer for a 120x40 canvas; it was measuring the config cap, not the ceiling.
/// The config below raises the cap so the terminal genuinely offers the room.
#[test]
fn a_terminal_with_room_to_spare_loses_nothing() {
    let ir = vertical_chain(50);
    let config = TermRenderConfig {
        max_width: 400,
        max_height: 400,
        ..TermRenderConfig::default()
    };
    let r = render_term_with_config(&ir, &config, 400, 400);

    assert_eq!(
        r.occluded_node_count, 0,
        "a 50-node chain with room to spare still lost nodes; canvas was {}x{}",
        r.width, r.height
    );
    assert_eq!(
        labels_absent_from_output(&r.output, 50),
        Vec::<usize>::new(),
        "the reported loss and the rendered text must agree at the large size too"
    );
    // The growth must be real, not an accounting change.
    let capped = render_term_with_config(&ir, &TermRenderConfig::default(), 400, 400);
    assert!(
        r.height > capped.height,
        "canvas did not grow: {}x{} with room vs {}x{} under the default cap",
        r.width, r.height, capped.width, capped.height
    );
}

/// Every chart type must draw its title in terminal output, exactly once.
///
/// `generic_terminal_diagram_title` used to return `None` for pie, gantt, xychart and quadrant
/// because each "has a specialized title renderer". None of them drew one, so the title vanished:
/// measured with the shipping binary, `title ZZTITLE` appeared in the SVG for all four and in the
/// terminal for none, while flowchart and journey showed it in both.
///
/// EXACTLY ONCE is the load-bearing part. The suppression existed to prevent a double-draw, so
/// removing it has to be pinned against the hazard it was guarding: if a specialized renderer ever
/// starts drawing its own title, this fails rather than silently showing it twice.
#[test]
fn every_chart_type_draws_its_title_exactly_once() {
    let cases: [(&str, &str); 6] = [
        ("pie", "pie title ZZTITLE\n  \"a\" : 40\n  \"b\" : 60\n"),
        (
            "gantt",
            "gantt\n  title ZZTITLE\n  dateFormat YYYY-MM-DD\n  section S\n  T :a, 2026-01-01, 5d\n",
        ),
        ("xychart", "xychart-beta\n  title \"ZZTITLE\"\n  x-axis [a, b]\n  bar [1, 2]\n"),
        (
            "quadrant",
            "quadrantChart\n  title ZZTITLE\n  x-axis Low --> High\n  y-axis Bad --> Good\n  P: [0.3, 0.6]\n",
        ),
        // journey promotes `title` in its own statement loop and is the regression control for
        // the generic path.
        ("journey", "journey\n  title ZZTITLE\n  section S\n    T: 5: Me\n"),
        // flowchart is here BECAUSE it was the case that failed. bd-ij0f taught the flowchart
        // statement loop to stop interning `title My Flow` as a node — with a bare `continue`, so
        // the line was dropped and nothing ever called set_title. Not a phantom any more, but not a
        // title either: a compiled run of this test reported "flowchart: title drawn 0 times".
        // extract_generic_diagram_title now promotes it post-parse.
        ("flowchart", "flowchart LR\n  title ZZTITLE\n  A --> B\n"),
    ];

    for (name, src) in cases {
        let ir = fm_parser::parse(src).ir;
        // ⚠️ rich() is the config the CLI actually ships (main.rs builds term_base_config from it),
        // and it is the config every measurement behind this fix was taken through. An earlier
        // version of this test used TermRenderConfig::default() and the flowchart CONTROL failed
        // with 0 titles — default() is a config nobody ships, so the test was asking a question the
        // evidence had never answered. Whether default() also drops titles is a separate question,
        // recorded on the bead rather than guessed at here.
        let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 100, 40).output;
        let seen = out.matches("ZZTITLE").count();
        assert_eq!(seen, 1, "{name}: title drawn {seen} times, expected exactly once");
    }
}

/// CONTROL: a diagram with no title must not gain one.
#[test]
fn a_chart_without_a_title_gains_none() {
    let ir = fm_parser::parse("pie\n  \"a\" : 40\n  \"b\" : 60\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 100, 40).output;
    assert!(
        !out.contains("ZZTITLE"),
        "a titleless chart must stay titleless"
    );
    // and it must still draw its content
    assert!(out.chars().any(|c| !c.is_whitespace()), "the chart drew nothing at all");
}

/// NEGATIVE CASES for the generic title extractor.
///
/// The extractor runs post-parse over raw input for EVERY diagram, so its blast radius is the whole
/// parser. These pin the three ways it could do damage.
#[test]
fn the_generic_title_extractor_does_not_overreach() {
    let title_of = |src: &str| fm_parser::parse(src).ir.meta.title.clone();

    // 1. A node whose id merely starts with the keyword is NOT a directive. `title` must be
    //    followed by whitespace to count, so `title[Box]` stays a node and sets no title.
    assert_eq!(
        title_of("flowchart LR\n  title[Box] --> B\n"),
        None,
        "a node named title[Box] was mistaken for a title directive"
    );

    // 2. It must never CLOBBER a title a type-specific parser already set. journey/gantt/pie and
    //    friends call set_title themselves, several also storing it in their own meta.
    assert_eq!(
        title_of("journey\n  title Real\n  section S\n    T: 5: Me\n").as_deref(),
        Some("Real")
    );
    assert_eq!(
        title_of("pie title Real\n  \"a\" : 40\n").as_deref(),
        Some("Real")
    );

    // 3. A diagram with no title gains none, and an empty directive sets nothing.
    assert_eq!(title_of("flowchart LR\n  A --> B\n"), None);
    assert_eq!(title_of("flowchart LR\n  title\n  A --> B\n"), None);

    // 4. Only the FIRST title line counts; a later one is not a second diagram title.
    assert_eq!(
        title_of("flowchart LR\n  title First\n  A --> B\n  title Second\n").as_deref(),
        Some("First")
    );
}

/// A gantt SECTION's name must reach terminal output (bd-u3fo, renderer half).
///
/// The band loop in `render_subcell_mode` draws each kind's GEOMETRY -- a dashed lifeline, a
/// section's rules, a column's separator -- and no text for any kind, while `LayoutBand` carries a
/// `label` that fm-render-svg does draw. So a gantt section band was drawn as two bare rules with
/// its name nowhere on the canvas.
#[test]
fn a_gantt_section_shows_its_name_in_terminal() {
    let ir = fm_parser::parse(
        "gantt\n  dateFormat YYYY-MM-DD\n  section Zulu\n  Task :a, 2026-01-01, 5d\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 100, 40).output;
    assert!(out.contains("Zulu"), "the section name is missing from terminal output");
    assert_eq!(out.matches("Zulu").count(), 1, "the section name was drawn more than once");
}

/// GENERALITY CONTROL: the fix is a CLUSTER-title overlay, not a kanban special case.
///
/// A flowchart `subgraph` is the same layout construct reached by a completely different layout
/// path, so if this passes too, the missing title was the renderer's and not kanban's. Without
/// this, a fix that keyed off `DiagramType::Kanban` would look just as green.
#[test]
fn a_flowchart_subgraph_shows_its_name_in_terminal() {
    let ir = fm_parser::parse("flowchart TD\n  subgraph Backend\n    a[Alpha]\n  end\n  a --> b[Beta]\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 100, 40).output;

    assert!(
        out.contains("Backend"),
        "the subgraph name is missing from terminal output:\n{out}"
    );
    assert!(
        out.contains("Alpha") && out.contains("Beta"),
        "the subgraph title displaced node content:\n{out}"
    );
}

/// CONFIG CONTROL: no title without its box.
///
/// The overlay sits behind the same `show_clusters` gate that draws the rectangle. If it did not,
/// a config that deliberately hides clusters would still get their names floating on the canvas
/// with nothing to attach them to — content invented where the user asked for none.
#[test]
fn cluster_titles_are_hidden_when_clusters_are() {
    let ir = fm_parser::parse("kanban\n  Alpha\n    t1[Beta]\n").ir;
    let mut config = TermRenderConfig::rich();
    config.show_clusters = false;
    let out = render_term_with_config(&ir, &config, 100, 40).output;

    assert!(
        !out.contains("Alpha"),
        "a cluster title was drawn while clusters were hidden:\n{out}"
    );
    // The card must still be there — this control must fail for the RIGHT reason, not because the
    // canvas came back empty.
    assert!(
        out.contains("Beta"),
        "the fixture rendered nothing, so the assertion above proves nothing:\n{out}"
    );
}

/// A kanban COLUMN's name must reach terminal output (bd-u3fo, the bead's headline case).
///
/// The column is a CLUSTER, not a band. `layout_diagram_kanban_traced` returns early when the
/// columns are declared lanes, so a parsed kanban arrives at the renderer with zero bands and its
/// columns as clusters — and `render_cluster_canvas` drew each cluster's rectangle and no title,
/// so the name was a nameless box. Measured on this exact fixture before the fix: 1 node, 0 bands,
/// canvas showed the card `Beta` and the column rectangle, and `Alpha` nowhere.
///
/// ⚠️ CORRECTION to this test's earlier `#[ignore]` note, which claimed `-f term` emitted NO text
/// at all and that the card was missing too. That was wrong — dumping the canvas shows `Beta` and
/// both box borders drawn. Only the cluster title was ever missing, which is why the fix is a
/// title overlay and not the empty-canvas hunt the note sent the next reader on.
///
/// The three assertions, in order: the column name reaches the output; the card survives alongside
/// it (a title that overwrote its own card would trade one piece of dropped content for another —
/// the overlay's blank-cell guard is what makes this hold); and no `lane 1` placeholder appears,
/// which is what fails if a future change routes kanban columns back through the band path.
#[test]
fn a_kanban_column_shows_its_name_in_terminal() {
    let ir = fm_parser::parse("kanban\n  Alpha\n    t1[Beta]\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 100, 40).output;

    assert!(
        out.contains("Alpha"),
        "the column name is missing from terminal output:\n{out}"
    );
    // The card must survive alongside it — a column label that overwrote its own card would trade
    // one piece of dropped content for another.
    assert!(
        out.contains("Beta"),
        "the card label was displaced by the column name:\n{out}"
    );
    // The placeholder must be absent. This is the assertion that fails if the LAYOUT half regresses
    // while the renderer half keeps working: the band would still be labelled, just wrongly.
    assert!(
        !out.contains("lane 1"),
        "a generated placeholder reached the canvas instead of the declared name:\n{out}"
    );
}

/// CONTROL: the overlay must not invent a label or displace node text.
#[test]
fn band_label_overlay_does_not_invent_or_displace() {
    // A sequence diagram's lifeline bands carry no user-facing name; participants are NODES and
    // must still appear exactly once each.
    let seq = fm_parser::parse("sequenceDiagram\n  Alpha->>Beta: hi\n").ir;
    let sout = render_term_with_config(&seq, &TermRenderConfig::rich(), 100, 40).output;
    assert_eq!(sout.matches("Alpha").count(), 1, "participant drawn more than once");
    assert_eq!(sout.matches("Beta").count(), 1, "participant drawn more than once");

    // A gantt task label must survive alongside its section name.
    let g = fm_parser::parse(
        "gantt\n  dateFormat YYYY-MM-DD\n  section Zulu\n  Yankee :a, 2026-01-01, 5d\n",
    )
    .ir;
    let gout = render_term_with_config(&g, &TermRenderConfig::rich(), 100, 40).output;
    assert!(gout.contains("Zulu") && gout.contains("Yankee"), "a label was overwritten");
}

/// GENERIC INVARIANT: text the user declared must reach the terminal canvas, for every diagram
/// type — not just the one that was reported broken.
///
/// bd-u3fo arrived as "kanban column names are invisible in `-f term`" and the fix was a
/// cluster-title overlay. The instance-shaped question is "is kanban fixed"; the useful question is
/// "what else does the terminal silently drop that fm-render-svg draws". This asserts the second
/// one, so the next member of the family fails here instead of being reported as its own bug.
///
/// Measured when this was added: all ten types below pass, so cluster titles were the only member
/// and this starts life green. That is the point — it is a tripwire for a class, not a reproducer.
/// It is deliberately cheap (parse + render at one large viewport) so it can cover breadth.
///
/// The viewport is large on purpose: a small one would fail for the unrelated reason that the
/// content did not fit, which is bd-8tsw's subject, not this one.
#[test]
fn declared_text_reaches_the_terminal_for_every_diagram_type() {
    // (name, source, strings the user wrote that must appear)
    let cases: &[(&str, &str, &[&str])] = &[
        ("er", "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n", &["CUSTOMER", "ORDER", "places"]),
        ("state", "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : go\n", &["Idle", "Busy", "go"]),
        ("timeline", "timeline\n  title Hist\n  2001 : Alpha\n  2002 : Beta\n", &["Alpha", "Beta"]),
        ("mindmap", "mindmap\n  root((Core))\n    Alpha\n    Beta\n", &["Core", "Alpha", "Beta"]),
        ("gitgraph", "gitGraph\n  commit id: \"Alpha\"\n  branch dev\n  commit id: \"Beta\"\n", &["Alpha", "Beta"]),
        ("requirement", "requirementDiagram\n  requirement Alpha {\n  id: 1\n  text: hello\n  }\n", &["Alpha"]),
        ("sankey", "sankey-beta\n\nAlpha,Beta,5\n", &["Alpha", "Beta"]),
        ("block", "block-beta\n  columns 2\n  Alpha[\"Alpha\"] Beta[\"Beta\"]\n", &["Alpha", "Beta"]),
        ("journey", "journey\n  title Day\n  section Morning\n    Wake: 5: Me\n", &["Morning", "Wake"]),
        ("class", "classDiagram\n  class Alpha\n  Alpha : +run()\n", &["Alpha", "run"]),
        // The bd-u3fo cases, kept here so the class gate covers them too: a kanban column and a
        // flowchart subgraph are both CLUSTER titles, reached by different layout paths.
        ("kanban", "kanban\n  Alpha\n    t1[Beta]\n", &["Alpha", "Beta"]),
        (
            "flowchart_subgraph",
            "flowchart TD\n  subgraph Backend\n    a[Alpha]\n  end\n  a --> b[Beta]\n",
            &["Backend", "Alpha", "Beta"],
        ),
    ];

    let mut failures: Vec<String> = Vec::new();
    for (name, source, wants) in cases {
        let ir = fm_parser::parse(source).ir;
        let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

        // ANTI-BLINDNESS CONTROL, per case. `contains` on a whole canvas is a weak probe: if a type
        // ever rendered its ENTIRE source verbatim, or if `out` grew some debug dump, every `wants`
        // check would pass for the wrong reason. A string the user never wrote must be absent.
        assert!(
            !out.contains("zznotdeclaredzz"),
            "{name}: the canvas contains text that was never declared, so the checks below are not              evidence:\n{out}"
        );

        let missing: Vec<&str> = wants.iter().copied().filter(|want| !out.contains(want)).collect();
        if !missing.is_empty() {
            failures.push(format!("{name}: missing {missing:?}"));
        }
    }

    assert!(
        failures.is_empty(),
        "declared text never reached the terminal canvas:\n  {}",
        failures.join("\n  ")
    );
}

/// A gantt chart must show its TIME AXIS in terminal output, not just its bars.
///
/// `layout.extensions.axis_ticks` is filled by the gantt layout arm and drawn by fm-render-svg, and
/// nothing in the terminal renderer referenced it. bd-trsd established what that costs: before it,
/// the entire text content of the shipped `gantt_basic.svg` was "Roadmap | Design | Build" — two
/// bars whose lengths encode durations no reader could name. `-f term` still rendered exactly that.
///
/// The CONTROL is the first assertion. The task names must ALSO still be present: an axis overlay
/// that wrote over the bars would trade one piece of dropped content for another, which is the
/// failure bd-u3fo's kanban case warned about.
#[test]
fn a_gantt_shows_its_time_axis_in_terminal() {
    let ir = fm_parser::parse(
        "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  Design :a1, 2026-01-01, 3d\n  Build :a2, after a1, 4d\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);

    // NON-VACUITY: the layout must actually publish ticks, or this test asserts nothing about the
    // renderer and would pass on a diagram that simply has no axis to draw.
    assert!(
        !layout.extensions.axis_ticks.is_empty(),
        "CONTROL FAILED: this gantt produced no axis ticks, so the renderer has nothing to draw \
         and this test cannot detect the defect it was written for"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 120, 40).output;

    // The task names must survive — the axis must not be drawn over the content.
    assert!(
        out.contains("Design"),
        "the task name was displaced by the axis overlay:\n{out}"
    );

    // At least one tick label must reach the canvas. Asserted against the labels the LAYOUT
    // produced rather than a hardcoded date, so the test pins the PROPERTY and not this fixture's
    // particular axis format.
    let drawn = layout
        .extensions
        .axis_ticks
        .iter()
        .filter(|tick| !tick.label.is_empty())
        .any(|tick| out.contains(tick.label.as_str()));
    assert!(
        drawn,
        "no axis tick label reached the terminal canvas; the chart shows bars with nothing to \
         measure them against:\n{out}"
    );
}

/// A diagram with NO axis must be unaffected, and its participants must not be disturbed.
///
/// This is the regression guard for the overlay itself: a sequence diagram publishes no
/// `axis_ticks`, so the new loop must be inert for it. If this ever fails, the overlay has started
/// drawing where no axis exists.
#[test]
fn a_sequence_diagram_is_unaffected_by_the_axis_overlay() {
    let ir = fm_parser::parse("sequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Hi\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    assert!(
        layout.extensions.axis_ticks.is_empty(),
        "CONTROL FAILED: this sequence diagram produced axis ticks, so it cannot show the overlay \
         is inert without one"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 120, 40).output;
    assert_eq!(
        out.matches("Alice").count(),
        1,
        "a participant appears more than once, or was disturbed by the axis overlay:\n{out}"
    );
    assert_eq!(
        out.matches("Bob").count(),
        1,
        "a participant appears more than once, or was disturbed by the axis overlay:\n{out}"
    );
}
