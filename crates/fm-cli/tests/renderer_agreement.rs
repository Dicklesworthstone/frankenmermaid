//! The three renderers must agree on WHICH DECLARED TEXT they draw.
//!
//! SVG is the reference: where it draws text the user wrote, the terminal and the canvas must draw
//! it too. This is the gate for a class that has produced FIFTEEN real defects — bd-039t (seven
//! terminal drops) and bd-rk14 (eight canvas drops) — every one of them content the parser captured,
//! the SVG rendered, and another renderer silently discarded.
//!
//! Neither the per-renderer text-parity gates nor the SVG feature-parity gate covers this. They ask
//! "did this renderer draw the text" one renderer at a time; this asks whether the renderers AGREE,
//! which is the question a user hits when the same diagram looks different in two outputs.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};
use fm_render_term::{TermRenderConfig, render_term_with_config};

/// Text passed to the canvas's `fill_text`, recovered from the recorded operations' Debug form.
fn canvas_text(ops_debug: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = ops_debug;
    while let Some(index) = rest.find("FillText(\"") {
        rest = &rest[index + "FillText(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// `(case, source, declared text)` — text the SOURCE declares and the SVG is expected to draw.
const CASES: &[(&str, &str, &str)] = &[
    (
        "er_attribute",
        "erDiagram\n  A {\n    string name PK\n  }\n",
        "name",
    ),
    (
        "er_key",
        "erDiagram\n  A {\n    string name PK\n  }\n",
        "PK",
    ),
    (
        "er_comment",
        "erDiagram\n  A {\n    string name \"who they are\"\n  }\n",
        "who they are",
    ),
    // Cardinality was drawn by the SVG ALONE until bd-2h3pp; the canvas and terminal drew the
    // relationship line and no numbers. Gated here so that agreement is ENFORCED rather than
    // incidental — the three surfaces reached it through different code (a shared fm-core mapping,
    // then each surface's own existing label placement), and nothing else makes them stay together.
    (
        "er_cardinality",
        "erDiagram\n  CUSTOMER }o--o| ORDER : places\n",
        "0..*",
    ),
    (
        "class_member",
        "classDiagram\n  class Alpha {\n    +String name\n  }\n",
        "name",
    ),
    (
        "class_stereotype",
        "classDiagram\n  class Alpha {\n    +String name\n  }\n  <<interface>> Alpha\n",
        "interface",
    ),
    (
        "class_cardinality",
        "classDiagram\n  Alpha \"1\" --> \"many\" Beta\n",
        "many",
    ),
    (
        "req_text",
        "requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  }\n",
        "hello",
    ),
    (
        "req_risk",
        "requirementDiagram\n  requirement R {\n  id: 1\n  text: t\n  risk: high\n  }\n",
        "high",
    ),
    (
        "c4_desc",
        "C4Context\n  title S\n  Person(a, \"Alice\", \"A user\")\n",
        "A user",
    ),
    (
        "seq_loop",
        "sequenceDiagram\n  loop Every day\n    Alice->>Bob: Hi\n  end\n",
        "Every day",
    ),
    (
        "gitgraph_branch",
        "gitGraph\n  commit\n  branch dev\n  commit\n",
        "dev",
    ),
    (
        "state_note",
        "stateDiagram-v2\n  [*] --> A\n  note right of A : Waiting\n",
        "Waiting",
    ),
    (
        "flowchart_subgraph",
        "flowchart TD\n  subgraph Backend\n    a[Alpha]\n  end\n  a --> b[Beta]\n",
        "Backend",
    ),
    // ── added after bd-59o4: this corpus was NARROWER THAN ITS OWN NAME. It claimed the three
    // renderers agree while never exercising pie, xychart, quadrant, packet, c4, architecture or
    // sequence notes — and two disagreements were hiding in exactly those types. A gate is only as
    // wide as its case list.
    (
        "pie_label",
        "pie title Share\n  \"Alpha\" : 60\n  \"Beta\" : 40\n",
        "Alpha",
    ),
    ("pie_title", "pie title Share\n  \"Alpha\" : 60\n", "Share"),
    (
        "xychart_title",
        "xychart-beta\n  title Sales\n  x-axis [jan, feb]\n  bar [50, 60]\n",
        "Sales",
    ),
    (
        "xychart_axis",
        "xychart-beta\n  title Sales\n  x-axis [jan, feb]\n  bar [50, 60]\n",
        "jan",
    ),
    (
        "quadrant_title",
        "quadrantChart\n  title Reach\n  x-axis Low --> High\n  A: [0.3, 0.6]\n",
        "Reach",
    ),
    (
        "quadrant_point",
        "quadrantChart\n  title Reach\n  x-axis Low --> High\n  A: [0.3, 0.6]\n",
        "A",
    ),
    (
        "quadrant_axis",
        "quadrantChart\n  title Reach\n  x-axis Low --> High\n  A: [0.3, 0.6]\n",
        "Low",
    ),
    (
        "packet_field",
        "packet-beta\n  0-7: \"Alpha\"\n  8-15: \"Beta\"\n",
        "Alpha",
    ),
    (
        "c4_name",
        "C4Context\n  title Sys\n  Person(a, \"Alice\", \"A user\")\n",
        "Alice",
    ),
    (
        "arch_service",
        "architecture-beta\n  service db(database)[Database]\n",
        "Database",
    ),
    (
        "seq_message",
        "sequenceDiagram\n  Alice->>Bob: Hello\n",
        "Hello",
    ),
    (
        "seq_note",
        "sequenceDiagram\n  Alice->>Bob: Hi\n  Note over Alice: Ponder\n",
        "Ponder",
    ),
    (
        "timeline_event",
        "timeline\n  title Hist\n  2001 : Alpha\n",
        "Alpha",
    ),
    ("sankey_node", "sankey-beta\n\nAlpha,Beta,5\n", "Alpha"),
    // ── second widening: the types still absent after bd-59o4. All fourteen agreed on the first
    // run, so this batch adds no known gap — it locks in a clean sweep rather than recording one.
    // Worth having precisely because the PREVIOUS widening found two real defects in exactly the
    // types it had been missing.
    (
        "mindmap_root",
        "mindmap\n  root((Core))\n    Alpha\n",
        "Core",
    ),
    (
        "mindmap_child",
        "mindmap\n  root((Core))\n    Alpha\n",
        "Alpha",
    ),
    (
        "journey_task",
        "journey\n  title Day\n  section Morning\n    Wake: 5: Me\n",
        "Wake",
    ),
    (
        "journey_section",
        "journey\n  title Day\n  section Morning\n    Wake: 5: Me\n",
        "Morning",
    ),
    (
        "kanban_card",
        "kanban\n  Todo\n    t1[Write docs]\n",
        "Write docs",
    ),
    (
        "kanban_column",
        "kanban\n  Todo\n    t1[Write docs]\n",
        "Todo",
    ),
    (
        "block_label",
        "block-beta\n  columns 2\n  Alpha[\"Alpha\"] Beta[\"Beta\"]\n",
        "Alpha",
    ),
    (
        "gantt_task",
        "gantt\n  title P\n  section S\n    BuildStep : t1, 2024-01-01, 3d\n",
        "BuildStep",
    ),
    (
        "gantt_title",
        "gantt\n  title Plan\n  section S\n    T : t1, 2024-01-01, 3d\n",
        "Plan",
    ),
    // ⚠️ ADDED BECAUSE KNOWN_GAPS NAMED A CASE THAT DID NOT EXIST. `gantt_section` had an entry in
    // the gap list and no matching entry here, so the gate never evaluated it: the "an entry that
    // starts agreeing fails" half of this test cannot fire for a case it never renders, which made
    // that entry a permanent hole rather than a tracked gap. The section name is now a real case,
    // and the gap entry is gone because the terminal draws it (bd-039t).
    (
        "gantt_section",
        "gantt\n  title P\n  section Engineering\n    T : t1, 2024-01-01, 3d\n",
        "Engineering",
    ),
    (
        "state_composite",
        "stateDiagram-v2\n  state Outer {\n    A --> B\n  }\n",
        "Outer",
    ),
    (
        "er_relationship_label",
        "erDiagram\n  A ||--o{ B : places\n",
        "places",
    ),
    (
        "flowchart_edge_label",
        "flowchart TD\n  a[A] -->|yes| b[B]\n",
        "yes",
    ),
    (
        "class_method",
        "classDiagram\n  class Alpha {\n    +run()\n  }\n",
        "run",
    ),
    (
        "timeline_title",
        "timeline\n  title Hist\n  2001 : Alpha\n",
        "Hist",
    ),
    // ── third widening: the corpus was broad across diagram TYPES and thin across CONTENT KINDS.
    // Every type above is represented, but within a type only one or two of the things a user can
    // write were checked — `seq_loop` but no other fragment, `gitgraph_branch` but not a tag,
    // `quadrant_title`/`_point`/`_axis` but not the quadrant NAMES, `arch_service` but not a group.
    // The two previous widenings each found real defects in exactly the places they had not looked,
    // so the cheapest place to look next is the content a type declares that nothing asserts yet.
    (
        "seq_alt",
        "sequenceDiagram\n  alt is ok\n    Alice->>Bob: Hi\n  end\n",
        "is ok",
    ),
    (
        "seq_opt",
        "sequenceDiagram\n  opt maybe\n    Alice->>Bob: Hi\n  end\n",
        "maybe",
    ),
    (
        "seq_par",
        "sequenceDiagram\n  par One\n    Alice->>Bob: Hi\n  end\n",
        "One",
    ),
    ("gitgraph_tag", "gitGraph\n  commit tag: \"v9\"\n", "v9"),
    ("gitgraph_id", "gitGraph\n  commit id: \"Alpha\"\n", "Alpha"),
    // `journey_actor` DELIBERATELY OMITTED, and the reason is a finding rather than a shrug: the
    // parser records a journey actor as a CSS CLASS on the step node (`journey-actor-me`, via
    // `add_journey_actor_classes`), not as text, so no renderer draws the name and even the SVG
    // reference misses it. Whether an actor NAME should be drawn is a real question — mermaid shows
    // actors as marks on the task — but it is a feature question about the journey renderer, not a
    // three-way agreement question, and asserting it here would only report the SVG as broken.
    (
        "quadrant_name",
        "quadrantChart\n  title Reach\n  x-axis Low --> High\n  quadrant-1 Do it\n  A: [0.3, 0.6]\n",
        "Do it",
    ),
    (
        "arch_group",
        "architecture-beta\n  group api(cloud)[API Layer]\n  service db(database)[Database] in api\n",
        "API Layer",
    ),
    (
        "c4_boundary",
        "C4Context\n  title S\n  System_Boundary(b, \"Internal\") {\n    Person(a, \"Alice\", \"A user\")\n  }\n",
        "Internal",
    ),
    // RESTORED once bd-qdmn gave the field an IR home. It was omitted for one commit because a
    // requirement ELEMENT's `type:` reached no renderer — it reached no IR — so all three "agreed"
    // by drawing nothing and this corpus was blind to it by construction; only the SVG-reference
    // assertion caught it. Back here now that there is something to compare.
    (
        "req_element",
        "requirementDiagram\n  element E {\n  type: simulation\n  }\n",
        "simulation",
    ),
    (
        "state_transition_label",
        "stateDiagram-v2\n  A --> B : go\n",
        "go",
    ),
    (
        "timeline_section",
        "timeline\n  title Hist\n  section Age\n    2001 : Alpha\n",
        "Age",
    ),
];

/// `(case, renderer, bead)` — pairs known to disagree, each naming the bead that tracks it.
///
/// An allowlist, not a silence: a NEW disagreement fails, and an entry that starts AGREEING fails
/// too, so a fix cannot leave a permanent hole behind.
/// EMPTY. Both entries were deleted on the first build after the freeze: `c4_boundary/terminal`
/// because the gate reported it now AGREES, and `gantt_section/terminal` because it named a case
/// that did not exist in `CASES` — so it was never evaluated at all. That case is now real.
///
/// An entry that starts agreeing fails the test below, which is what removed the first one. That
/// mechanism only works for a case the gate actually renders, so an entry here MUST name a case
/// above.
const KNOWN_GAPS: &[(&str, &str, &str)] = &[];

#[test]
fn the_three_renderers_agree_on_declared_text() {
    let mut disagreements: Vec<String> = Vec::new();
    let mut stale_gaps: Vec<String> = Vec::new();
    let mut svg_misses: Vec<String> = Vec::new();
    let mut svg_hits = 0_usize;

    for (case, source, want) in CASES {
        let ir = fm_parser::parse(source).ir;

        let svg = fm_render_svg::render_svg(&ir);
        // The SVG is the REFERENCE: if it does not draw the text, this case says nothing about the
        // other two, and silently skipping would let the corpus rot into vacuity.
        //
        // COLLECTED, NOT ASSERTED IN PLACE. This was an `assert!`, which aborts the whole run on the
        // first bad case — so ONE mis-specified case blinds every case after it. Measured: adding
        // twelve cases at once, the first (`journey_actor`) tripped this and the other eleven never
        // ran, which is exactly the failure mode this corpus exists to prevent in the renderers.
        // Still loud and still fatal at the end; it just reports every offender in one run.
        if !svg.contains(want) {
            svg_misses.push(format!(
                "{case}: the SVG does not draw {want:?}, so this case cannot compare renderers"
            ));
            continue;
        }
        svg_hits += 1;

        let term = render_term_with_config(&ir, &TermRenderConfig::rich(), 200, 60).output;
        let mut context = MockCanvas2dContext::new(1200.0, 900.0);
        render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
        let canvas = canvas_text(&format!("{:?}", context.operations()));

        for (renderer, drew) in [
            ("terminal", term.contains(want)),
            ("canvas", canvas.iter().any(|t| t.contains(want))),
        ] {
            let known = KNOWN_GAPS
                .iter()
                .any(|(gap_case, gap_renderer, _)| gap_case == case && *gap_renderer == renderer);
            if !drew && !known {
                disagreements.push(format!(
                    "{case}: the SVG draws {want:?} and the {renderer} does not"
                ));
            }
            if drew && known {
                stale_gaps.push(format!(
                    "{case}/{renderer}: now agrees — delete its KNOWN_GAPS entry"
                ));
            }
        }
    }

    // Reported before the non-vacuity count below, because a miss is the REASON the count is short
    // and naming the offenders beats reporting an arithmetic shortfall.
    assert!(
        svg_misses.is_empty(),
        "the SVG reference does not draw these, so they compare nothing:\n  {}",
        svg_misses.join("\n  ")
    );

    // NON-VACUITY: every case must have contributed an SVG hit, or the loop compared nothing.
    assert_eq!(
        svg_hits,
        CASES.len(),
        "not every case produced SVG text, so the comparison is incomplete"
    );

    assert!(
        stale_gaps.is_empty(),
        "KNOWN_GAPS is out of date:\n  {}",
        stale_gaps.join("\n  ")
    );
    assert!(
        disagreements.is_empty(),
        "renderers disagree on declared text:\n  {}",
        disagreements.join("\n  ")
    );
}

/// The renderers must also agree on DECLARED STYLING, not just declared text (bd-lvj3).
///
/// That bead was filed from a measured table -- `style` statements, `classDef` fill, `classDef`
/// stroke and a sequence `rect rgb(...)` each rendered by the SVG and DROPPED by the canvas, which
/// read none of the three styling channels. Every one of those channels now has its own canvas
/// test, and this is the gate those individual tests cannot be: it asks whether the two engines
/// AGREE, which is the question a user hits when the browser preview and the exported SVG disagree
/// about the same document.
///
/// ⚠️ COMPARED ON A NORMALISED COLOUR, NOT ON BYTES. The two engines legitimately spell the same
/// colour differently -- an SVG may carry `#ff0000` in a CSS rule while the canvas records
/// `SetFillStyle("#ff0000")` or an `rgb(255,0,0)` -- so a byte comparison would fail on correct
/// output. Both spellings of each declared colour are accepted on either side.
///
/// The SVG side is checked FIRST and treated as the reference, exactly as the text gate above does:
/// if the SVG did not honour a declaration then the case cannot compare renderers, and saying so is
/// more useful than reporting a canvas miss for a colour nobody drew.
#[test]
fn the_renderers_agree_on_declared_styling() {
    // (case, source, colour as hex, colour as rgb) -- the four rows of bd-lvj3's own table.
    let cases: &[(&str, &str, &str, &str)] = &[
        (
            "style_stmt",
            "flowchart TD\n  a[A]\n  style a fill:#ff0000\n",
            "#ff0000",
            "rgb(255,0,0)",
        ),
        (
            "classdef_fill",
            "flowchart TD\n  a[A]\n  classDef hot fill:#ff0000\n  class a hot\n",
            "#ff0000",
            "rgb(255,0,0)",
        ),
        (
            "classdef_stroke",
            "flowchart TD\n  a[A]\n  classDef hot stroke:#00ff00\n  class a hot\n",
            "#00ff00",
            "rgb(0,255,0)",
        ),
        (
            "seq_rect_color",
            "sequenceDiagram\n  participant A\n  participant B\n  rect rgb(255,0,0)\n  A->>B: hi\n  end\n",
            "#ff0000",
            "rgb(255,0,0)",
        ),
    ];

    let mut svg_misses = Vec::new();
    let mut canvas_misses = Vec::new();
    let mut compared = 0_usize;

    for (case, source, hex, rgb) in cases {
        let ir = fm_parser::parse(source).ir;
        let svg = fm_render_svg::render_svg(&ir)
            .to_ascii_lowercase()
            .replace(' ', "");
        let mut context = MockCanvas2dContext::new(1200.0, 900.0);
        render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
        let canvas = format!("{:?}", context.operations())
            .to_ascii_lowercase()
            .replace(' ', "");

        let carries = |haystack: &str| haystack.contains(*hex) || haystack.contains(*rgb);

        if !carries(&svg) {
            svg_misses.push(format!(
                "{case}: the SVG does not carry {hex} or {rgb}, so this case cannot compare renderers"
            ));
            continue;
        }
        compared += 1;
        if !carries(&canvas) {
            canvas_misses.push(format!("{case}: SVG carries {hex}, the canvas does not"));
        }
    }

    // NON-VACUITY, and it is the assertion that matters most here: if the SVG stopped honouring
    // every declaration, the loop above would report zero canvas misses while comparing nothing.
    assert!(
        compared == cases.len(),
        "only {compared} of {} cases were comparable; the SVG reference itself regressed:\n  {}",
        cases.len(),
        svg_misses.join("\n  ")
    );
    assert!(
        canvas_misses.is_empty(),
        "the canvas dropped styling the SVG honoured (bd-lvj3):\n  {}",
        canvas_misses.join("\n  ")
    );
}
