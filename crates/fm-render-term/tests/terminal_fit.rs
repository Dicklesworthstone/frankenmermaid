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
    // ⚠️ `Zulu` IS EXACTLY 4 CHARACTERS, and that is why this passes. The band label is capped at
    // the band's cell width (6 cells here), so any longer section name is truncated — see
    // `gantt_section_name_is_truncated_to_the_band_width` (bd-039t). This test is kept because it
    // still guards the label reaching the canvas at all and being drawn once, but it must NOT be
    // read as evidence that section names render correctly.
    assert!(out.contains("Zulu"), "the section name is missing from terminal output");
    assert_eq!(out.matches("Zulu").count(), 1, "the section name was drawn more than once");
}

/// A gantt section name LONGER than the band is truncated (bd-039t).
///
/// Measured: the surviving prefix is 4 characters regardless of the name — `Build` draws as `Buil`,
/// `Engineering` as `Engi` — because the band-label overlay caps the label at the band's own cell
/// width and a gantt section band is 6 cells wide in subcell mode.
///
/// ⚠️ `#[ignore]` BECAUSE IT REPRODUCES A LIVE DEFECT. Removing the cap was tried and reverted: it
/// displaced content and broke `band_label_overlay_does_not_invent_or_displace` and
/// `a_sequence_diagram_is_unaffected_by_the_axis_overlay`, and it did not fix this case either. The
/// cap is load-bearing; the fix has to place the label where there IS room.
#[test]
#[ignore = "bd-039t: gantt section names longer than the band are truncated to 4 characters"]
fn gantt_section_name_is_truncated_to_the_band_width() {
    let ir = fm_parser::parse(
        "gantt\n  dateFormat YYYY-MM-DD\n  section Engineering\n  Task :a, 2026-01-01, 5d\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 100, 40).output;

    assert!(
        out.contains("Engineering"),
        "the section name is truncated in terminal output:\n{out}"
    );
}

/// A sequence FRAGMENT's label must reach terminal output (bd-039t).
///
/// `render_subcell_mode` drew each fragment's rectangle and no text, so `loop Every day` came out
/// as a bare box while fm-render-svg drew the frame AND its label. Measured SVG vs terminal:
/// `Every day` appeared in the SVG and in neither terminal render.
#[test]
fn sequence_fragment_label_reaches_terminal_output() {
    let ir = fm_parser::parse(
        "sequenceDiagram\n  loop Every day\n    Alice->>Bob: Hi\n  end\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Every day"), "the loop condition never reached the terminal:\n{out}");
    // The messages inside the frame must survive — a label drawn over the frame's contents would
    // trade one piece of dropped content for another.
    assert!(out.contains("Alice") && out.contains("Hi"), "the frame label displaced its contents:\n{out}");
}

/// The FRAME TAG is drawn as well as the condition.
///
/// `alt` with no condition and a bare condition read differently, so the kind is part of the
/// content rather than decoration.
#[test]
fn sequence_fragment_tag_is_drawn_with_its_condition() {
    let ir = fm_parser::parse(
        "sequenceDiagram\n  alt is ok\n    Alice->>Bob: Hi\n  else nope\n    Bob->>Alice: No\n  end\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("is ok"), "the alt condition never reached the terminal:\n{out}");
    assert!(out.contains("alt"), "the frame tag never reached the terminal:\n{out}");
}

/// CONTROL: a sequence diagram with NO fragments is unchanged.
///
/// The overlay runs for every fragment in the layout. Without this, a bug that wrote a stray tag on
/// a frameless diagram would go unnoticed on the cases above, where any extra glyphs would be
/// mistaken for the labels under test.
#[test]
fn sequence_without_fragments_gains_no_frame_label() {
    let ir = fm_parser::parse("sequenceDiagram\n  Alice->>Bob: Hi\n  Bob->>Alice: Yo\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Alice") && out.contains("Hi"), "the messages are missing:\n{out}");
    assert!(
        !out.contains("loop") && !out.contains("alt"),
        "a frame tag was drawn for a diagram with no fragments:\n{out}"
    );
}

/// A class STEREOTYPE must reach terminal output (bd-039t).
///
/// Measured SVG vs terminal: an `interface` stereotype drew in the SVG and not in the terminal.
/// Unlike the other members of this bead, this was a gap INSIDE an overlay that already existed —
/// the class compartments drew name, attributes and methods and skipped `meta.stereotype`.
#[test]
fn class_stereotype_reaches_terminal_output() {
    let ir = fm_parser::parse(
        "classDiagram\n  class Alpha {\n    +String name\n  }\n  <<interface>> Alpha\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Alpha"), "the class name is missing:\n{out}");
    assert!(out.contains("interface"), "the stereotype never reached the terminal:\n{out}");
    // The members must survive: the stereotype is inserted ABOVE the name inside the same box, so a
    // miscounted row would push the compartment rows out of the box rather than fail loudly.
    assert!(out.contains("name"), "the stereotype displaced the class members:\n{out}");

    // THE EXACT SOURCE THE PROBE REPORTED, which declares the class and its member separately
    // rather than in a block. A fix verified only against a differently-shaped fixture would not
    // have closed the case that was actually filed.
    let probe_form =
        fm_parser::parse("classDiagram\n  class Alpha\n  <<interface>> Alpha\n  Alpha : +run()\n").ir;
    let probe_out = render_term_with_config(&probe_form, &TermRenderConfig::rich(), 200, 60).output;
    assert!(
        probe_out.contains("interface"),
        "the stereotype is still missing for the separately-declared form the probe used:\n{probe_out}"
    );
}

/// CONTROL: a class WITHOUT a stereotype is unchanged.
///
/// The new row is emitted only when `meta.stereotype` is present. Without this, an unconditional
/// row would shift every class box's contents down by one and go unnoticed on the case above.
#[test]
fn class_without_stereotype_is_unchanged() {
    let ir = fm_parser::parse("classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(
        out.contains("Alpha") && out.contains("name") && out.contains("run"),
        "a stereotype-free class regressed:\n{out}"
    );
    assert!(
        !out.contains("interface"),
        "a stereotype was drawn for a class that declares none:\n{out}"
    );
}

/// A C4 element's TYPE, TECHNOLOGY and DESCRIPTION must reach terminal output (bd-039t).
///
/// Measured SVG vs terminal: `Person(a, "Alice", "A user")` drew `A user` in the SVG and not in the
/// terminal. Decorations mirror fm-render-svg, which writes the type in double angle brackets and
/// the technology in square brackets.
#[test]
fn c4_element_details_reach_terminal_output() {
    let ir = fm_parser::parse("C4Context\n  title S\n  Person(alice, \"Alice\", \"A user\")\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Alice"), "the element name is missing:\n{out}");
    assert!(out.contains("A user"), "the description never reached the terminal:\n{out}");
    assert!(out.contains("Person"), "the element type never reached the terminal:\n{out}");
}

/// CONTROL: the neighbours in the same node loop still render.
///
/// The C4 branch sits after the requirement branch, which sits after the ER branch. Each is gated on
/// a different field, and mis-ordering or mis-guarding any of them would swallow a neighbour.
#[test]
fn er_requirement_and_class_still_render_after_the_c4_branch() {
    let er = fm_parser::parse("erDiagram\n  A {\n    string name PK\n  }\n  A ||--o{ B : has\n").ir;
    let er_out = render_term_with_config(&er, &TermRenderConfig::rich(), 200, 60).output;
    assert!(er_out.contains("name") && er_out.contains("PK"), "ER regressed:\n{er_out}");

    let req =
        fm_parser::parse("requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  }\n").ir;
    let req_out = render_term_with_config(&req, &TermRenderConfig::rich(), 200, 60).output;
    assert!(req_out.contains("hello"), "requirement rows regressed:\n{req_out}");

    let cls = fm_parser::parse("classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n").ir;
    let cls_out = render_term_with_config(&cls, &TermRenderConfig::rich(), 200, 60).output;
    assert!(cls_out.contains("run"), "class compartments regressed:\n{cls_out}");
}

/// A requirement's declared FIELDS must reach terminal output (bd-039t).
///
/// Measured SVG vs terminal on the same IR: `requirement R { id: 1 / text: hello / risk: high }`
/// drew `hello` and `high` in the SVG and NEITHER in the terminal. Same shape as bd-ekx2 — content
/// attached to the node that the terminal never learned to draw.
#[test]
fn requirement_fields_reach_terminal_output() {
    let ir = fm_parser::parse(
        "requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  risk: high\n  }\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains('R'), "the requirement name is missing:\n{out}");
    assert!(out.contains("hello"), "the `text:` field never reached the terminal:\n{out}");
    assert!(out.contains("high"), "the `risk:` field never reached the terminal:\n{out}");
}

/// CONTROL: a requirement declaring NO optional fields still shows its name.
///
/// The branch is gated on `requirement_meta` being present, which it is even when every field is
/// empty — so a bare requirement must still render its header rather than an empty box.
#[test]
fn requirement_without_optional_fields_still_shows_its_name() {
    let ir = fm_parser::parse("requirementDiagram\n  requirement Alpha {\n  id: 7\n  text: t\n  }\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Alpha"), "the requirement lost its name:\n{out}");
}

/// CONTROL: ER entities and class diagrams still render.
///
/// The requirement branch sits directly after the ER branch in the same loop. Ordering or guarding
/// it wrongly could swallow either neighbour, and both carry their content in different fields.
#[test]
fn er_and_class_still_render_after_the_requirement_branch() {
    let er = fm_parser::parse("erDiagram\n  A {\n    string name PK\n  }\n  A ||--o{ B : has\n").ir;
    let er_out = render_term_with_config(&er, &TermRenderConfig::rich(), 200, 60).output;
    assert!(
        er_out.contains("name") && er_out.contains("PK"),
        "ER attributes regressed:\n{er_out}"
    );

    let cls = fm_parser::parse("classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n").ir;
    let cls_out = render_term_with_config(&cls, &TermRenderConfig::rich(), 200, 60).output;
    assert!(
        cls_out.contains("Alpha") && cls_out.contains("run"),
        "class compartments regressed:\n{cls_out}"
    );
}

/// Class CARDINALITIES must reach terminal output (bd-o2wf).
///
/// They live in `IrEdgeExtras`, not in `edge.label`, and the terminal edge overlay drew the label
/// and nothing else — so this diagram rendered byte-identical in the terminal with and without
/// them, while fm-render-svg drew both.
#[test]
fn class_cardinalities_reach_terminal_output() {
    let ir = fm_parser::parse("classDiagram\n  Alpha \"1\" --> \"many\" Beta\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Alpha") && out.contains("Beta"), "the classes are missing:\n{out}");
    assert!(out.contains('1'), "the source cardinality never reached the terminal:\n{out}");
    assert!(out.contains("many"), "the target cardinality never reached the terminal:\n{out}");
}

/// CONTROL: an edge WITHOUT cardinalities is unaffected.
///
/// The overlay runs for every edge, outside the label block. Without this, a bug that wrote an
/// empty or stray string at each endpoint would go unnoticed on the case above, where any extra
/// glyphs would be mistaken for the numbers under test.
#[test]
fn class_edge_without_cardinality_is_unchanged() {
    let plain = fm_parser::parse("classDiagram\n  Alpha --> Beta\n").ir;
    let out = render_term_with_config(&plain, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("Alpha") && out.contains("Beta"), "the classes are missing:\n{out}");
    // `1` and `many` are the strings the fix introduces; neither may appear from nowhere.
    assert!(
        !out.contains("many"),
        "a cardinality was drawn for an edge that declares none:\n{out}"
    );
}

/// CONTROL: the cardinality must not displace an edge's own LABEL.
///
/// The numbers are written at the endpoints and the label at the midpoint, and the overlay only
/// writes into blank cells — but the two are drawn by adjacent code paths, so this pins that
/// adding one did not cost the other.
#[test]
fn class_cardinality_does_not_displace_the_edge_label() {
    let ir = fm_parser::parse("classDiagram\n  Alpha \"1\" --> \"many\" Beta : uses\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("uses"), "the edge label was displaced by a cardinality:\n{out}");
    assert!(out.contains("many"), "the cardinality is missing:\n{out}");
}

/// An ER entity's ATTRIBUTES must reach terminal output (bd-ekx2).
///
/// Measured before the fix: `CUSTOMER { string name PK / int age }` parsed to an IR carrying 2
/// members, and the terminal drew the entity NAME only — `name`, `age` and `PK` absent at 100x40,
/// 200x60 AND 400x120. Size-independent, so not the viewport ceiling of bd-8tsw. The identifier
/// `members` appeared nowhere in the terminal renderer.
#[test]
fn er_entity_attributes_reach_terminal_output() {
    let ir = fm_parser::parse(
        "erDiagram\n  CUSTOMER {\n    string name PK\n    int age\n  }\n  CUSTOMER ||--o{ ORDER : places\n",
    )
    .ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(out.contains("CUSTOMER"), "the entity name is missing:\n{out}");
    assert!(out.contains("name"), "attribute `name` never reached the terminal:\n{out}");
    assert!(out.contains("age"), "attribute `age` never reached the terminal:\n{out}");
    // The KEY marker is the part an ER reader relies on to tell a primary key from a plain column.
    assert!(out.contains("PK"), "the PK key marker never reached the terminal:\n{out}");
    // The relationship label must survive alongside the new rows — attributes drawn over the rest
    // of the diagram would trade one piece of dropped content for another.
    assert!(out.contains("places"), "the relationship label was displaced:\n{out}");
}

/// CONTROL: class compartments still render, unchanged.
///
/// The ER branch is inserted directly after the class branch in the same node loop. If it were
/// ordered or guarded wrongly it could swallow class nodes, which carry their members in
/// `class_meta` rather than `members` — this fails loudly if that happens.
#[test]
fn class_compartments_still_render_after_the_er_branch() {
    let ir = fm_parser::parse("classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(
        out.contains("Alpha") && out.contains("name") && out.contains("run"),
        "class compartments regressed:\n{out}"
    );
}

/// CONTROL: an ER entity with NO attributes still draws its name.
///
/// The new branch is gated on `!members.is_empty()`, so a bare entity must fall through to ordinary
/// node rendering rather than losing its label to an empty compartment.
#[test]
fn er_entity_without_attributes_still_shows_its_name() {
    let ir = fm_parser::parse("erDiagram\n  CUSTOMER ||--o{ ORDER : places\n").ir;
    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;

    assert!(
        out.contains("CUSTOMER") && out.contains("ORDER"),
        "a bare ER entity lost its name:\n{out}"
    );
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

/// A stateDiagram note must reach terminal output (bd-t1jj).
///
/// `layout.extensions.state_notes` is filled by the state layout arm (bd-a6l4) and drawn by
/// fm-render-svg. The terminal renderer referenced it NOWHERE, so `note right of X : ...` produced a
/// note that existed in the layout, was hashed into the layout checksum, and appeared in no terminal
/// output at any size.
///
/// The CONTROL is first: the layout must actually publish a note for this source, or the assertion
/// below would pass on a diagram that has no note to draw.
#[test]
fn a_state_note_reaches_terminal_output() {
    let ir = fm_parser::parse(
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running\n  note right of Idle : waiting for work\n",
    )
    .ir;
    let layout = fm_layout::layout_diagram(&ir);
    assert!(
        !layout.extensions.state_notes.is_empty(),
        "CONTROL FAILED: this source produced no state note, so the renderer has nothing to draw \
         and this test cannot detect the defect it was written for"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 140, 44).output;
    assert!(
        out.contains("waiting"),
        "the state note is missing from terminal output:\n{out}"
    );
    // The state it annotates must survive — a note drawn over its own subject trades one piece of
    // dropped content for another, which is the failure bd-u3fo's kanban case warned about.
    assert!(
        out.contains("Idle"),
        "the annotated state was displaced by its own note:\n{out}"
    );
    assert!(
        out.contains("Running"),
        "an unrelated state was displaced by the note overlay:\n{out}"
    );
}

/// A state diagram with NO note must be untouched.
///
/// Regression guard for the overlay itself: if this ever fails, the note pass has started drawing
/// where no note exists.
#[test]
fn a_state_diagram_without_notes_is_unaffected() {
    let ir = fm_parser::parse("stateDiagram-v2\n  [*] --> Idle\n  Idle --> Running\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    assert!(
        layout.extensions.state_notes.is_empty(),
        "CONTROL FAILED: this source produced a note, so it cannot show the overlay is inert"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 140, 44).output;
    assert_eq!(
        out.matches("Idle").count(),
        1,
        "a state appears more than once, or was disturbed by the note overlay:\n{out}"
    );
    assert_eq!(
        out.matches("Running").count(),
        1,
        "a state appears more than once, or was disturbed by the note overlay:\n{out}"
    );
}

/// A gantt task name must survive even when its bar is short and sits at the right edge (bd-t1jj).
///
/// The generic node-label path centres a label on its node and lets an oversized one overflow to the
/// RIGHT. A task whose name is wider than its bar and whose bar is near the right edge has nowhere to
/// overflow, so the name was clipped at the canvas edge and LOST. `extensions.gantt_task_labels`
/// already solved this — layout resolves each task to Inside / OutsideRight / OutsideLeft and hands
/// back an anchor, choosing OutsideLeft precisely when there is no room to the right. fm-render-svg
/// consumed it; the terminal did not.
///
/// MEASURED before the fix: at 80 columns `FinalIntegrationAndSignoffPhase` did not appear at all,
/// while the same chart at 120 columns showed it. A name that survives only if the terminal happens
/// to be wide enough is not a name a reader can rely on, which is why the narrow width is the case
/// under test.
#[test]
fn a_gantt_task_name_survives_a_short_bar_at_the_right_edge() {
    let src = "gantt\n  title Roadmap\n  dateFormat  YYYY-MM-DD\n  section Core\n  \
               ReticulateTheSplinesThoroughly :a1, 2026-01-01, 1d\n  \
               Build :a2, after a1, 6d\n  \
               FinalIntegrationAndSignoffPhase :a3, after a2, 1d\n";
    let ir = fm_parser::parse(src).ir;
    let layout = fm_layout::layout_diagram(&ir);

    // NON-VACUITY: layout must actually resolve the last task to OutsideLeft, or this test does not
    // exercise the placement path and would pass on a chart whose names all fit.
    let outside_left = layout
        .extensions
        .gantt_task_labels
        .iter()
        .any(|entry| matches!(entry.placement, fm_layout::GanttLabelPlacement::OutsideLeft));
    assert!(
        outside_left,
        "CONTROL FAILED: no task resolved to OutsideLeft, so this fixture cannot detect the defect \
         it was written for"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 80, 40).output;
    assert!(
        out.contains("FinalIntegrationAndSignoffPhase"),
        "the right-edge task name was clipped off the canvas:\n{out}"
    );
    // The other names must survive too — a placement fix that rescued one name by overwriting
    // another would trade one piece of dropped content for another.
    assert!(
        out.contains("ReticulateTheSplinesThoroughly"),
        "an earlier task name was displaced by the placement change:\n{out}"
    );
    assert!(
        out.contains("Build"),
        "the middle task name was displaced by the placement change:\n{out}"
    );
}

/// A diagram with no gantt labels keeps the centred placement it always had.
///
/// Regression guard for the lookup: every non-gantt diagram must take exactly the path it took
/// before, so this pins that a flowchart's node label is still centred and drawn exactly once.
#[test]
fn a_flowchart_label_is_unaffected_by_gantt_placement() {
    let ir = fm_parser::parse("flowchart LR\n  A[Alpha] --> B[Beta]\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    assert!(
        layout.extensions.gantt_task_labels.is_empty(),
        "CONTROL FAILED: a flowchart produced gantt task labels, so it cannot show the placement \
         path is inert"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 80, 30).output;
    assert_eq!(
        out.matches("Alpha").count(),
        1,
        "a flowchart label was duplicated or displaced:\n{out}"
    );
    assert_eq!(
        out.matches("Beta").count(),
        1,
        "a flowchart label was duplicated or displaced:\n{out}"
    );
}

/// A packet field that crosses a 32-bit row boundary must be drawn on BOTH rows (bd-t1jj).
///
/// `extensions.packet_field_continuations` gives one extra box per additional row a field occupies,
/// and fm-render-svg draws each with its label. The terminal drew only the primary box, so measured
/// on `24-47: "CrossingField"` -- primary at (768, 0, 256, 55), continuation at (0, 70, 512, 55) --
/// a 24-bit field rendered with the extent of an 8-bit one.
///
/// That is not a missing decoration. A packet diagram exists to show how wide each field is, so
/// dropping two thirds of a field's extent misstates the one thing the diagram is for.
///
/// ASSERTED BY OCCURRENCE COUNT, which is what distinguishes a drawn continuation from a drawn
/// primary: the wrapped field's name must appear TWICE, once per segment, while fields that do not
/// wrap appear exactly once. A `contains` check would have passed before the fix, because the
/// primary box always carried the name.
#[test]
fn a_wrapped_packet_field_is_drawn_on_both_rows() {
    let src = "packet-beta\n  0-15: \"SourcePort\"\n  16-23: \"Flags\"\n  24-47: \"CrossingField\"\n  48-63: \"Checksum\"\n";
    let ir = fm_parser::parse(src).ir;
    let layout = fm_layout::layout_diagram(&ir);

    // NON-VACUITY: layout must actually emit a continuation, or this test asserts nothing about the
    // renderer and would pass on a packet whose fields all fit one row.
    assert_eq!(
        layout.extensions.packet_field_continuations.len(),
        1,
        "CONTROL FAILED: expected exactly one continuation for a field crossing the 32-bit \
         boundary, so this fixture cannot detect the defect it was written for"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 120, 40).output;
    assert_eq!(
        out.matches("CrossingField").count(),
        2,
        "the wrapped field was drawn on one row only, so its extent is understated:\n{out}"
    );
    // Fields that do NOT wrap must still appear exactly once -- a continuation pass that labelled
    // every field twice would satisfy the assertion above while corrupting the rest of the packet.
    assert_eq!(
        out.matches("SourcePort").count(),
        1,
        "an unwrapped field was duplicated by the continuation pass:\n{out}"
    );
    assert_eq!(
        out.matches("Checksum").count(),
        1,
        "an unwrapped field was duplicated by the continuation pass:\n{out}"
    );
}

/// A packet whose fields all fit one row gains no continuation boxes.
///
/// Regression guard: without it, a renderer that emitted continuations unconditionally would satisfy
/// the test above.
#[test]
fn an_unwrapped_packet_gains_no_continuation() {
    let src = "packet-beta\n  0-15: \"SourcePort\"\n  16-31: \"DestPort\"\n";
    let ir = fm_parser::parse(src).ir;
    let layout = fm_layout::layout_diagram(&ir);
    assert!(
        layout.extensions.packet_field_continuations.is_empty(),
        "CONTROL FAILED: this packet produced a continuation, so it cannot show the pass is inert"
    );

    let out = render_term_with_config(&ir, &TermRenderConfig::rich(), 120, 40).output;
    assert_eq!(
        out.matches("SourcePort").count(),
        1,
        "a field was duplicated although nothing wrapped:\n{out}"
    );
    assert_eq!(
        out.matches("DestPort").count(),
        1,
        "a field was duplicated although nothing wrapped:\n{out}"
    );
}
