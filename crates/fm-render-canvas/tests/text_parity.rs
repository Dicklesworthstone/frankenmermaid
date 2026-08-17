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
