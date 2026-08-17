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
    ("er_attribute", "erDiagram\n  A {\n    string name PK\n  }\n", "name"),
    ("er_key", "erDiagram\n  A {\n    string name PK\n  }\n", "PK"),
    ("er_comment", "erDiagram\n  A {\n    string name \"who they are\"\n  }\n", "who they are"),
    ("class_member", "classDiagram\n  class Alpha {\n    +String name\n  }\n", "name"),
    ("class_stereotype", "classDiagram\n  class Alpha {\n    +String name\n  }\n  <<interface>> Alpha\n", "interface"),
    ("class_cardinality", "classDiagram\n  Alpha \"1\" --> \"many\" Beta\n", "many"),
    ("req_text", "requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  }\n", "hello"),
    ("req_risk", "requirementDiagram\n  requirement R {\n  id: 1\n  text: t\n  risk: high\n  }\n", "high"),
    ("c4_desc", "C4Context\n  title S\n  Person(a, \"Alice\", \"A user\")\n", "A user"),
    ("seq_loop", "sequenceDiagram\n  loop Every day\n    Alice->>Bob: Hi\n  end\n", "Every day"),
    ("gitgraph_branch", "gitGraph\n  commit\n  branch dev\n  commit\n", "dev"),
    ("state_note", "stateDiagram-v2\n  [*] --> A\n  note right of A : Waiting\n", "Waiting"),
    ("flowchart_subgraph", "flowchart TD\n  subgraph Backend\n    a[Alpha]\n  end\n  a --> b[Beta]\n", "Backend"),
];

/// `(case, renderer, bead)` — pairs known to disagree, each naming the bead that tracks it.
///
/// An allowlist, not a silence: a NEW disagreement fails, and an entry that starts AGREEING fails
/// too, so a fix cannot leave a permanent hole behind.
const KNOWN_GAPS: &[(&str, &str, &str)] = &[(
    "gantt_section",
    "terminal",
    "bd-039t: the band-label overlay caps a label at the band's own cell width, so a gantt section \
     name longer than 4 characters is truncated. Two attempts to lift the cap were reverted — it is \
     load-bearing, and removing it displaces sequence content.",
)];

#[test]
fn the_three_renderers_agree_on_declared_text() {
    let mut disagreements: Vec<String> = Vec::new();
    let mut stale_gaps: Vec<String> = Vec::new();
    let mut svg_hits = 0_usize;

    for (case, source, want) in CASES {
        let ir = fm_parser::parse(source).ir;

        let svg = fm_render_svg::render_svg(&ir);
        // The SVG is the REFERENCE: if it does not draw the text, this case says nothing about the
        // other two, and silently skipping would let the corpus rot into vacuity.
        assert!(
            svg.contains(want),
            "{case}: the SVG does not draw {want:?}, so this case cannot compare renderers"
        );
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
