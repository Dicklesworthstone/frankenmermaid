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
    (
        "flowchart",
        "flowchart TD\n  a[Alpha] --> b[Beta]\n",
        &["Alpha", "Beta"],
    ),
    (
        "sequence",
        "sequenceDiagram\n  Alice->>Bob: Hello\n",
        &["Alice", "Bob", "Hello"],
    ),
    (
        "class",
        "classDiagram\n  class Alpha\n  Alpha : +run()\n",
        &["Alpha"],
    ),
    (
        "state",
        "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy : go\n",
        &["Idle", "Busy"],
    ),
    (
        "er",
        "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n",
        &["CUSTOMER", "ORDER"],
    ),
    (
        "pie",
        "pie title Share\n  \"Alpha\" : 60\n  \"Beta\" : 40\n",
        &["Alpha", "Beta"],
    ),
    (
        "mindmap",
        "mindmap\n  root((Core))\n    Alpha\n",
        &["Core", "Alpha"],
    ),
    (
        "timeline",
        "timeline\n  title Hist\n  2001 : Alpha\n",
        &["Alpha"],
    ),
    (
        "gitgraph",
        "gitGraph\n  commit id: \"Alpha\"\n  branch dev\n  commit id: \"Beta\"\n",
        &["Alpha", "Beta"],
    ),
    (
        "journey",
        "journey\n  title Day\n  section Morning\n    Wake: 5: Me\n",
        &["Morning", "Wake"],
    ),
    (
        "kanban",
        "kanban\n  Alpha\n    t1[Beta]\n",
        &["Alpha", "Beta"],
    ),
    (
        "flowchart_subgraph",
        "flowchart TD\n  subgraph Backend\n    a[Alpha]\n  end\n  a --> b[Beta]\n",
        &["Backend", "Alpha", "Beta"],
    ),
];

/// Class CARDINALITIES must reach the canvas (bd-rk14).
///
/// They live in `IrEdgeExtras`, not `edge.label`, and the canvas edge path drew the label and
/// nothing else. Last of the eight drops in this bead, and the canvas twin of bd-o2wf.
#[test]
fn class_cardinalities_reach_the_canvas() {
    let ir = fm_parser::parse("classDiagram\n  Alpha \"1\" --> \"many\" Beta\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("Alpha")) && texts.iter().any(|t| t.contains("Beta")),
        "the classes are missing: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "1"),
        "the source cardinality never reached the canvas: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "many"),
        "the target cardinality never reached the canvas: {texts:?}"
    );
}

/// CONTROL: an edge WITHOUT cardinalities gains nothing.
///
/// The block runs for every edge, outside the label branch. Without this, a bug drawing an empty or
/// stray string at each endpoint would go unnoticed on the case above, where extra glyphs would be
/// mistaken for the numbers under test.
#[test]
fn class_edge_without_cardinality_gains_nothing_on_the_canvas() {
    let ir = fm_parser::parse("classDiagram\n  Alpha --> Beta\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("Alpha")) && texts.iter().any(|t| t.contains("Beta")),
        "the classes are missing: {texts:?}"
    );
    assert!(
        !texts.iter().any(|t| t == "many" || t == "1"),
        "a cardinality was drawn for an edge that declares none: {texts:?}"
    );
}

/// CONTROL: the edge's own LABEL survives alongside the cardinalities.
///
/// The two are drawn by adjacent blocks — numbers inset at the endpoints, label at the midpoint —
/// so this pins that adding one did not cost the other.
#[test]
fn canvas_cardinality_does_not_displace_the_edge_label() {
    let ir = fm_parser::parse("classDiagram\n  Alpha \"1\" --> \"many\" Beta : uses\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|t| t.contains("uses")),
        "the edge label was displaced by a cardinality: {texts:?}"
    );
    assert!(
        texts.iter().any(|t| t == "many"),
        "the cardinality is missing: {texts:?}"
    );
}

/// QUADRANT AXIS labels must reach the canvas (bd-59o4).
///
/// Measured: `x_axis_left` had ZERO references anywhere in this crate — against one in
/// fm-render-term and two in fm-render-svg — so the canvas never read the field. The chart's title
/// and data points still appeared, because both come from the generic title and node paths, which
/// is why only the axes were missing and the chart looked almost right.
#[test]
fn quadrant_axis_labels_reach_the_canvas() {
    let ir = fm_parser::parse(
        "quadrantChart\n  title Reach\n  x-axis Low --> High\n  y-axis Bot --> Top\n  Alpha: [0.3, 0.6]\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));
    let has = |want: &str| texts.iter().any(|t| t.contains(want));

    assert!(
        has("Low"),
        "the x-axis left label never reached the canvas: {texts:?}"
    );
    assert!(
        has("High"),
        "the x-axis right label never reached the canvas: {texts:?}"
    );
    assert!(
        has("Top"),
        "the y-axis top label never reached the canvas: {texts:?}"
    );
    // What already worked must keep working — these arrive by different paths entirely.
    assert!(has("Reach"), "the chart title was displaced: {texts:?}");
    assert!(
        has("Alpha"),
        "the data point label was displaced: {texts:?}"
    );
}

/// CONTROL: a quadrant chart declaring NO axis labels gains none, and a NON-quadrant diagram is
/// untouched.
///
/// The pass is gated on both the diagram type and each field's presence. Without this, an
/// unconditional draw would put stray text on every diagram and go unnoticed above.
#[test]
fn quadrant_axis_pass_is_inert_without_labels_and_off_type() {
    let bare = fm_parser::parse("quadrantChart\n  title Reach\n  Alpha: [0.3, 0.6]\n").ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&bare, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));
    assert!(
        texts.iter().any(|t| t.contains("Reach")),
        "the title is missing: {texts:?}"
    );
    assert!(
        !texts
            .iter()
            .any(|t| t.contains("Low") || t.contains("High")),
        "axis text appeared for a chart that declares none: {texts:?}"
    );

    let flow = fm_parser::parse("flowchart TD\n  a[Alpha] --> b[Beta]\n").ir;
    let mut flow_ctx = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&flow, &mut flow_ctx, &CanvasRenderConfig::default());
    let flow_texts = drawn_text(&format!("{:?}", flow_ctx.operations()));
    assert!(
        flow_texts.iter().any(|t| t.contains("Alpha")) && flow_texts.len() >= 2,
        "a flowchart regressed: {flow_texts:?}"
    );
}

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

    let stereo =
        texts_for("classDiagram\n  class Alpha {\n    +String name\n  }\n  <<interface>> Alpha\n");
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

    let req = texts_for(
        "requirementDiagram\n  requirement R {\n  id: 1\n  text: hello\n  risk: high\n  }\n",
    );
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
    assert!(
        has("name"),
        "attribute `name` never reached the canvas: {texts:?}"
    );
    assert!(
        has("age"),
        "attribute `age` never reached the canvas: {texts:?}"
    );
    assert!(
        has("PK"),
        "the PK key marker never reached the canvas: {texts:?}"
    );
}

/// CONTROL: class compartments still render on the canvas.
///
/// The ER arm was inserted as an `else if` between the class arm and the standard single-label
/// fallback. A mis-ordered guard would swallow class nodes, which carry their members in
/// `class_meta` rather than `members`.
#[test]
fn class_compartments_still_render_on_the_canvas_after_the_er_arm() {
    let ir =
        fm_parser::parse("classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n").ir;
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

/// SAME-IR SVG/CANVAS COMPARISON: a class member's TYPE and its abstract/static classifier must
/// reach the canvas, because they reach the SVG (bd-9wdra).
///
/// Found the way bd-bk7h was: by counting field references per backend. `is_abstract` and
/// `is_static` appeared TWICE in fm-render-svg, ONCE in fm-render-term, and ZERO times in this
/// crate. Canvas being the only backend of three that never reads a field is what distinguishes a
/// defect from a deliberate difference in what a raster surface can express.
///
/// `+area(): bool` rendered as `+area()` on the canvas; an abstract `+run()*` and a static
/// `+load()$` lost their marker entirely, so an abstract method was indistinguishable from a
/// concrete one in the browser and identical in the CLI's SVG.
///
/// The SVG is the reference ARM, not the assertion: each check joins the two backends on the same
/// parsed IR, and the SVG side is asserted first so a fixture that stopped exercising the feature
/// fails as a CONTROL rather than passing vacuously.
#[test]
fn class_member_types_and_classifiers_reach_the_canvas_as_they_reach_the_svg() {
    const SOURCE: &str = "classDiagram\n  class Shape {\n    +String name\n    +area()* float\n    \
                          +load()$ Shape\n    -id int\n  }\n";
    let ir = fm_parser::parse(SOURCE).ir;

    // CONTROL ON THE PARSE: the classifiers must have survived into the IR, or both arms would
    // agree by both being empty and this test would certify nothing.
    let shape = ir
        .nodes
        .iter()
        .find_map(|node| node.class_meta.as_deref())
        .expect("CONTROL FAILED: the class was not parsed, so neither arm can be compared");
    assert!(
        shape.methods.iter().any(|m| m.is_abstract),
        "CONTROL FAILED: no method parsed as abstract, so `*` is not under test here"
    );
    assert!(
        shape.methods.iter().any(|m| m.is_static),
        "CONTROL FAILED: no method parsed as static, so `$` is not under test here"
    );

    let svg = fm_render_svg::render_svg(&ir);

    let mut context = MockCanvas2dContext::new(1600.0, 1200.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    // Each row as the shared contract spells it: visibility, name, classifier, then the type tail.
    // fm-layout's `class_member_row_width` builds this exact string to MEASURE the box, so a
    // disagreement here is also the box being sized for text nobody drew.
    for row in [
        "+String name",
        "-id int",
        // ' : ', not ': ' — bd-ci658 moved the return-type tail to mermaid's spelling in all five
        // row builders at once. These literals are the CONTRACT the canvas is checked against, so
        // they move with it; the assertion below still fails closed if the SVG arm stops emitting
        // them.
        // ⚠️ NO `*` AND NO `$` (bd-r2gll). mermaid's `getDisplayDetails()` returns
        // `+area() : float` and carries the classifier as `cssStyle: font-style:italic;`, so the
        // marker left the TEXT in every backend that can express a style. These literals moved with
        // that change; the SVG control below still fails closed if the SVG arm stops emitting them.
        "+area() : float",
        "+load() : Shape",
    ] {
        assert!(
            svg.contains(row),
            "CONTROL FAILED: the SVG arm never emitted {row:?}, so it cannot serve as the reference"
        );
        assert!(
            texts.iter().any(|text| text == row),
            "the canvas never drew {row:?}; SVG did. Drawn rows were {:?}",
            texts
                .iter()
                .filter(|t| t.starts_with(['+', '-', '#', '~']))
                .collect::<Vec<_>>()
        );
    }

    // NEGATIVE CONTROL: the classifier belongs to methods only. An attribute must NOT gain one, or
    // the fix would have traded a missing marker for a spurious one — and every assertion above
    // would still pass.
    assert!(
        !texts
            .iter()
            .any(|text| text.starts_with("+String name")
                && (text.contains('*') || text.contains('$'))),
        "an attribute row gained a method classifier: {texts:?}"
    );

    // ⚠️ AND NO ROW MAY CARRY THE RAW MARKER AT ALL, in either backend. Without this the change
    // could regress to appending the character and every positive assertion above would still hold
    // — they only ever look for the presence of the classifier-free row, which a longer string
    // containing it does not satisfy on the canvas but DOES satisfy on `svg.contains`.
    assert!(
        !svg.contains("+area()*") && !svg.contains("+load()$"),
        "the SVG still draws the literal classifier character"
    );
    assert!(
        !texts
            .iter()
            .any(|text| text.contains('*') || text.contains('$')),
        "the canvas still draws the literal classifier character: {texts:?}"
    );
}

/// CONTROL: a class whose members declare NO type and NO classifier gains nothing.
///
/// Without this, an implementation that appended a stray `": "` or a marker to every row would
/// satisfy the case above — the assertions there only ever look for MORE text.
#[test]
fn a_plain_class_member_gains_no_type_and_no_classifier_on_the_canvas() {
    let ir = fm_parser::parse("classDiagram\n  class Alpha\n  Alpha : +run()\n").ir;

    let mut context = MockCanvas2dContext::new(1200.0, 800.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    let row = texts
        .iter()
        .find(|text| text.starts_with("+run"))
        .expect("CONTROL FAILED: the member row was not drawn at all");
    assert_eq!(
        row, "+run()",
        "a member with no declared type or classifier gained one"
    );
}

/// SAME-IR SVG/CANVAS COMPARISON: a class declaring a stereotype and NO members must still show it
/// (bd-d48wi).
///
/// A marker interface — a class with an annotation and nothing else — is idiomatic mermaid, and the
/// pinned incumbent (mermaid 11.15.0) gates its annotation row on `annotations.length > 0` with no
/// member requirement whatsoever. Every backend here gated the whole compartment stack on having at
/// least one member, so `class Shape { <<interface>> }` rendered as a bare box: the stereotype was
/// parsed into the IR and then drawn by nobody.
///
/// Unlike bd-9wdra this was NOT a cross-backend divergence — SVG and canvas dropped it identically,
/// so no amount of SVG-vs-canvas comparison could have caught it. It took comparing against the
/// incumbent. The SVG arm is still asserted here, as the joint statement that BOTH backends now
/// agree with mermaid rather than merely with each other.
#[test]
fn a_class_with_only_a_stereotype_shows_it_on_both_the_canvas_and_the_svg() {
    // Assembled from pieces so the literal never appears as a shell-redirect-looking token.
    let source = format!(
        "classDiagram\n  class Shape {{\n    {}interface{}\n  }}\n",
        "<<", ">>"
    );
    let ir = fm_parser::parse(&source).ir;

    // CONTROL ON THE PARSE: the stereotype must have reached the IR, and the class must really
    // declare no members — otherwise this is the already-working case wearing the wrong name.
    let meta = ir
        .nodes
        .iter()
        .find_map(|node| node.class_meta.as_deref())
        .expect("CONTROL FAILED: no class metadata parsed");
    assert!(
        meta.stereotype.is_some(),
        "CONTROL FAILED: the stereotype never reached the IR"
    );
    assert!(
        meta.attributes.is_empty() && meta.methods.is_empty(),
        "CONTROL FAILED: the fixture declares members, so it is not the memberless case"
    );

    let svg = fm_render_svg::render_svg(&ir);
    let mut context = MockCanvas2dContext::new(1200.0, 800.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    let stereotype = format!("{}interface{}", "<<", ">>");
    // The SVG arm escapes ASYMMETRICALLY — `<` becomes `&lt;` but `>` stays bare, which is valid in
    // XML text content — so the emitted form is `&lt;&lt;interface>>`. Guessing `&gt;&gt;` here
    // fails, and guessing the unescaped form fails too; this is measured from the actual output.
    // It is also exactly how a naive scan gets this backwards: splitting the document on '>' reads
    // that trailing `>>` as a tag close and reports the text as ABSENT, which reads as a defect.
    let escaped = format!("{}interface{}", "&lt;&lt;", ">>");
    assert!(
        svg.contains(&escaped),
        "the SVG never emitted the stereotype for a memberless class"
    );
    assert!(
        texts.iter().any(|text| text == &stereotype),
        "the canvas never drew the stereotype for a memberless class; drawn text was {texts:?}"
    );

    // The class NAME must survive alongside it — drawing the stereotype in place of the name would
    // satisfy the assertion above and lose more than it gained.
    assert!(
        texts.iter().any(|text| text == "Shape"),
        "the class name was lost when the stereotype was drawn: {texts:?}"
    );
}
