//! Declared text must reach the CANVAS, for every diagram type.
//!
//! Third and last renderer in this family. The SVG gate (fm-render-svg `text_parity.rs`) and the
//! terminal gate (fm-render-term `declared_text_reaches_the_terminal_for_every_diagram_type`) cover
//! the other two; the canvas path is what fm-wasm ships to a browser, so a drop here is as visible
//! as either.
//!
//! This family is not hypothetical: the terminal gate's subject, bd-u3fo, was a real dropped
//! cluster title, and the SVG gate found bd-jgco on its first run.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

/// Text passed to `fill_text`, recovered from the recorded operations.
///
/// `DrawOperation` is not re-exported from the crate root, so an integration test cannot match the
/// enum directly. Reading it back from the `Debug` form is the alternative, and it is matched as the
/// STRUCTURED `FillText("…"` shape rather than as a bare substring of the whole dump — a plain
/// `contains("Alpha")` would also match a colour, a font name or a coordinate that happened to
/// contain those bytes, which is how a check like this quietly stops meaning anything.
fn drawn_text(ops_debug: &str) -> Vec<String> {
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

const CASES: &[(&str, &str, &[&str])] = &[
    ("flowchart", "flowchart TD\n  a[Alpha] --> b[Beta]\n", &["Alpha", "Beta"]),
    ("sequence", "sequenceDiagram\n  Alice->>Bob: Hello\n", &["Alice", "Bob", "Hello"]),
    ("class", "classDiagram\n  class Alpha\n  Alpha : +run()\n", &["Alpha"]),
    ("state", "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : go\n", &["Idle", "Busy"]),
    ("er", "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n", &["CUSTOMER", "ORDER"]),
    ("pie", "pie title Share\n  \"Alpha\" : 60\n  \"Beta\" : 40\n", &["Alpha", "Beta"]),
    ("mindmap", "mindmap\n  root((Core))\n    Alpha\n", &["Core", "Alpha"]),
    ("timeline", "timeline\n  title Hist\n  2001 : Alpha\n", &["Alpha"]),
    ("gitgraph", "gitGraph\n  commit id: \"Alpha\"\n  branch dev\n  commit id: \"Beta\"\n", &["Alpha", "Beta"]),
    ("journey", "journey\n  title Day\n  section Morning\n    Wake: 5: Me\n", &["Morning", "Wake"]),
    ("kanban", "kanban\n  Alpha\n    t1[Beta]\n", &["Alpha", "Beta"]),
    ("flowchart_subgraph", "flowchart TD\n  subgraph Backend\n    a[Alpha]\n  end\n  a --> b[Beta]\n", &["Backend", "Alpha", "Beta"]),
];

/// A gitGraph BRANCH NAME must reach the canvas (bd-rk14).
///
/// Measured: the layout carries `[(Lane,"main"), (Lane,"dev")]` from bd-jgco and the canvas drew
/// only `commit_1`/`commit_2`. Root cause was NOT the box-content gap the rest of this bead is
/// about — the `Lane` arm of the canvas band loop drew a dashed lifeline and no text at all, while
/// the `Section` arm beside it drew its label. The canvas twin of bd-u3fo.
#[test]
fn gitgraph_branch_names_reach_the_canvas() {
    let ir = fm_parser::parse("gitGraph\n  commit\n  branch dev\n  commit\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("main")),
        "the `main` branch name never reached the canvas: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t.contains("dev")),
        "the `dev` branch name never reached the canvas: {texts:?}"
    );
}

/// CONTROL: sequence lifelines must NOT gain a label from this.
///
/// `LayoutBandKind::Lane` is overloaded — sequence lifelines share it with named lanes — and a
/// lifeline band carries its PARTICIPANT'S NAME, which is already drawn as a head/foot header. My
/// first attempt gated on "label is non-empty" alone and drew `Alice` a third time; the crate's own
/// `canvas_mirrors_sequence_participant_headers` caught it. This pins the same property from the
/// integration side so the discriminator cannot be quietly dropped.
#[test]
fn sequence_lifelines_gain_no_band_label_on_the_canvas() {
    let ir = fm_parser::parse(
        "%%{init: {\"sequence\": {\"mirrorActors\": true}}}%%\nsequenceDiagram\n  participant Alice\n  participant Bob\n  Alice->>Bob: Hi\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    let alice = texts.iter().filter(|t| t.as_str() == "Alice").count();
    assert_eq!(
        alice, 2,
        "Alice must be drawn exactly twice (head and foot); a band label would make it three: {texts:?}"
    );
}

/// Canvas twins of three terminal fixes: stereotype, requirement rows, C4 details (bd-rk14).
///
/// All three were measured drawing in the SVG and absent from the canvas, and all three were
/// already fixed in the TERMINAL under bd-039t — the canvas simply never got them.
#[test]
fn stereotype_requirement_and_c4_reach_the_canvas() {
    let texts_for = |source: &str| {
        let ir = fm_parser::parse(source).ir;
        let mut context = MockCanvas2dContext::new(1200.0, 900.0);
        render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
        drawn_text(&format!("{:?}", context.operations()))
    };

    let stereo = texts_for(
        "classDiagram\n  class Alpha {\n    +String name\n  }\n  <<interface>> Alpha\n",
    );
    assert!(
        stereo.iter().any(|t| t.contains("interface")),
        "the class stereotype never reached the canvas: {stereo:?}"
    );
    // The members must survive: the stereotype is drawn ABOVE the name in the same box, so a
    // mis-advanced cursor would push the compartment rows out rather than fail loudly.
    assert!(
        stereo.iter().any(|t| t.contains("name")),
        "the stereotype displaced the class members: {stereo:?}"
    );

    let req = texts_for("requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  risk: high\n  }\n");
    assert!(
        req.iter().any(|t| t.contains("hello")),
        "the requirement text never reached the canvas: {req:?}"
    );
    assert!(
        req.iter().any(|t| t.contains("high")),
        "the requirement risk never reached the canvas: {req:?}"
    );

    let c4 = texts_for("C4Context\n  title S\n  Person(alice, \"Alice\", \"A user\")\n");
    assert!(
        c4.iter().any(|t| t.contains("A user")),
        "the C4 description never reached the canvas: {c4:?}"
    );
    assert!(
        c4.iter().any(|t| t.contains("Person")),
        "the C4 element type never reached the canvas: {c4:?}"
    );
}

/// CONTROL: plain nodes still take the single-label path.
///
/// Three new `else if` arms were inserted before the standard fallback, each gated on a different
/// meta field. A mis-guarded arm would capture ordinary flowchart nodes and silently change how
/// every diagram renders.
#[test]
fn plain_nodes_still_take_the_single_label_path_on_the_canvas() {
    let ir = fm_parser::parse("flowchart TD\n  a[Alpha] --> b[Beta]\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("Alpha")) && texts.iter().any(|t| t.contains("Beta")),
        "plain flowchart nodes regressed: {texts:?}"
    );
}

/// An ER entity's ATTRIBUTES must reach the CANVAS (bd-rk14).
///
/// Measured SVG vs canvas: `A { string name PK }` drew `name` and `PK` in the SVG and neither on
/// the canvas, while `class_member` passed in the same probe — the canvas could already draw
/// compartments and ER never got them. This is the canvas twin of bd-ekx2, the identical gap in the
/// terminal.
#[test]
fn er_attributes_reach_the_canvas() {
    let ir = fm_parser::parse(
        "erDiagram\n  CUSTOMER {\n    string name PK\n    int age\n  }\n  CUSTOMER ||--o{ ORDER : places\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));
    let has = |want: &str| texts.iter().any(|t| t.contains(want));

    assert!(has("CUSTOMER"), "the entity name is missing: {texts:?}");
    assert!(has("name"), "attribute `name` never reached the canvas: {texts:?}");
    assert!(has("age"), "attribute `age` never reached the canvas: {texts:?}");
    assert!(has("PK"), "the PK key marker never reached the canvas: {texts:?}");
}

/// CONTROL: class compartments still render on the canvas.
///
/// The ER arm was inserted as an `else if` between the class arm and the standard single-label
/// fallback. A mis-ordered guard would swallow class nodes, which carry their members in
/// `class_meta` rather than `members`.
#[test]
fn class_compartments_still_render_on_the_canvas_after_the_er_arm() {
    let ir = fm_parser::parse("classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("Alpha")) && texts.iter().any(|t| t.contains("run")),
        "class compartments regressed: {texts:?}"
    );
}

/// CONTROL: an ER entity with NO attributes still draws its name.
///
/// The arm is gated on `!members.is_empty()`, so a bare entity must fall through to the standard
/// single-label path rather than losing its label to an empty compartment.
#[test]
fn bare_er_entity_still_draws_its_name_on_the_canvas() {
    let ir = fm_parser::parse("erDiagram\n  CUSTOMER ||--o{ ORDER : places\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("CUSTOMER")) && texts.iter().any(|t| t.contains("ORDER")),
        "a bare ER entity lost its name: {texts:?}"
    );
}

#[test]
fn declared_text_reaches_the_canvas_for_every_diagram_type() {
    let mut failures: Vec<String> = Vec::new();
    let mut total_drawn = 0_usize;

    for (name, source, wants) in CASES {
        let ir = fm_parser::parse(source).ir;
        let mut context = MockCanvas2dContext::new(1200.0, 900.0);
        render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());

        let texts = drawn_text(&format!("{:?}", context.operations()));
        total_drawn += texts.len();

        for want in *wants {
            // Compare against the DRAWN STRINGS, not the whole operation dump.
            if !texts.iter().any(|text| text.contains(want)) {
                failures.push(format!("{name}: declared text {want:?} was never drawn"));
            }
        }
    }

    // NON-VACUITY CONTROL. If `FillText` were ever renamed, or the mock stopped recording text, or
    // `render_to_canvas` silently drew nothing, `drawn_text` would return empty for every case —
    // and then every `wants` check fails loudly rather than passing, so this control is not
    // guarding the same direction as the SVG gate's. It guards the opposite mistake: a future
    // author "fixing" a red run by loosening the extractor until it matches nothing in particular.
    assert!(
        total_drawn >= CASES.len(),
        "only {total_drawn} drawn strings recovered across {} diagram types — fewer than one per \
         case means the extractor or the mock, not the renderer, is what this test is measuring",
        CASES.len()
    );

    assert!(
        failures.is_empty(),
        "declared text never reached the canvas:\n  {}",
        failures.join("\n  ")
    );
}
