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

#[test]
fn requirement_metadata_is_complete_and_contained_with_compact_line_spacing() {
    use fm_render_canvas::{Canvas2dRenderer, DrawOperation};

    let mut ir = fm_parser::parse("requirementDiagram\n    element Module {\n        type: service\n        docref: spec.md\n    }\n").ir;
    let meta = ir.nodes[0].requirement_meta.as_mut().unwrap();
    meta.req_id = Some("REQ-001".to_string());
    meta.text = Some("Preserve every declared field".to_string());
    meta.risk = Some("High".to_string());
    meta.verify_method = Some("Test".to_string());
    let font_size = 15.0;
    let layout = fm_layout::layout_diagram_with_config(
        &ir,
        fm_layout::LayoutConfig {
            font_metrics: Some(fm_core::FontMetrics::new(fm_core::FontMetricsConfig {
                font_size,
                line_height: 1.0,
                ..Default::default()
            })),
            ..Default::default()
        },
    );
    let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
        font_size: f64::from(font_size),
        auto_fit: false,
        ..Default::default()
    });
    let mut context = MockCanvas2dContext::new(1600.0, 1000.0);
    renderer.render(&layout, &ir, &mut context);
    let operations = context.operations();
    let (_, top, _, height) = operations
        .iter()
        .filter_map(|operation| match operation {
            DrawOperation::Rect(x, y, width, height)
            | DrawOperation::StrokeRect(x, y, width, height)
            | DrawOperation::FillRect(x, y, width, height) => Some((*x, *y, *width, *height)),
            _ => None,
        })
        .max_by(|left, right| left.2.total_cmp(&right.2))
        .expect("requirement box must be drawn");

    let mut previous_bottom = top;
    for expected in [
        "id: REQ-001",
        "text: Preserve every declared field",
        "type: service",
        "doc: spec.md",
        "risk: High",
        "verify: Test",
    ] {
        let y = operations
            .iter()
            .find_map(|operation| match operation {
                DrawOperation::FillText(text, _, y) if text == expected => Some(*y),
                _ => None,
            })
            .unwrap_or_else(|| panic!("declared field was dropped: {expected}"));
        let half_height = f64::from(font_size) * 0.9 / 2.0;
        assert!(
            y - half_height >= previous_bottom,
            "{expected} overlaps the preceding row"
        );
        assert!(
            y + half_height <= top + height,
            "{expected} extends below its box"
        );
        previous_bottom = y + half_height;
    }
}

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
        // mermaid stores and draws the raw type token, so the stereotype is `<<person>>` — checking
        // for the capitalised macro name would pass on a renderer that echoed the author's spelling.
        c4.iter().any(|t| t.contains("<<person>>")),
        "the C4 element type never reached the canvas: {c4:?}"
    );
}

/// Mermaid numbers C4Dynamic relationships in declaration order; Canvas must not discard that
/// semantic distinction while SVG retains it (bd-5k51.1).
#[test]
fn c4_dynamic_relationships_are_numbered_on_the_canvas() {
    let ir = fm_parser::parse(
        "C4Dynamic\n  Person(a, \"A\")\n  System(b, \"B\")\n  Rel(a, b, \"First step\")\n  Rel(b, a, \"Second step\")\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|text| text == "1: First step"),
        "the first dynamic relationship was not numbered: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "2: Second step"),
        "the second dynamic relationship was not numbered: {texts:?}"
    );
}

/// CONTROL: C4Context uses the same relationship syntax but Mermaid does not number it. A naive
/// prefix on every C4 relationship would satisfy the dynamic case while changing another diagram.
#[test]
fn c4_context_relationships_remain_unnumbered_on_the_canvas() {
    let ir = fm_parser::parse(
        "C4Context\n  Person(a, \"A\")\n  System(b, \"B\")\n  Rel(a, b, \"Request\")\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|text| text == "Request"),
        "the C4Context relationship label is missing: {texts:?}"
    );
    assert!(
        !texts.iter().any(|text| text == "1: Request"),
        "C4Context relationship was incorrectly numbered: {texts:?}"
    );
}

/// `SHOW_LEGEND()` is part of C4's visible contract. The parser retains that request in IR and
/// SVG renders it; Canvas must not silently discard it on the browser/WASM path (bd-5k51.1).
#[test]
fn c4_show_legend_reaches_the_canvas() {
    let ir = fm_parser::parse(
        "C4Context\n  SHOW_LEGEND()\n  Person(person, \"Customer\")\n  System(system, \"Store\")\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        texts.iter().any(|text| text == "C4 Legend"),
        "the requested C4 legend was not drawn: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "◉ Person"),
        "the Person legend entry was not drawn: {texts:?}"
    );
    assert!(
        texts.iter().any(|text| text == "▭ System"),
        "the System legend entry was not drawn: {texts:?}"
    );
}

/// CONTROL: a C4 diagram without `SHOW_LEGEND()` must not receive an unsolicited overlay.
/// A naive implementation that draws the legend for every C4 diagram would pass the positive case.
#[test]
fn c4_hide_legend_remains_absent_on_the_canvas() {
    let ir = fm_parser::parse(
        "C4Context\n  HIDE_LEGEND()\n  Person(person, \"Customer\")\n  System(system, \"Store\")\n",
    )
    .ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let texts = drawn_text(&format!("{:?}", context.operations()));

    assert!(
        !texts.iter().any(|text| text == "C4 Legend"),
        "Canvas drew a C4 legend that the source hid: {texts:?}"
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

    // ⚠️ THE DRAWN SPELLING IS GUILLEMETS, NOT THE SOURCE'S ANGLE BRACKETS. `<<interface>>` is how
    // the stereotype is WRITTEN; `«interface»` is what mermaid DRAWS, and db00c405 (bd-1buv) moved
    // both our backends onto the drawn form. This test was not carried with that change and asserted
    // the old `&lt;&lt;interface>>` on the SVG arm, so it has been RED on main since 2026-08-28 —
    // masked, because `cargo test` stops at the first failing target and cluster_styling's stale ER
    // cardinality assertion was failing ahead of it. Repairing that one surfaced this one.
    //
    // The escaping note the old comment carried is still worth keeping, because it is what makes a
    // naive check here get the wrong answer: the guillemets are non-ASCII and pass through unescaped,
    // so neither `&lt;` nor `&gt;` appears at all now. Splitting the document on '>' to find text
    // would still misread a bare `>` as a tag close — the trap recorded on bd-9wdra.
    let stereotype = format!("{}interface{}", '\u{ab}', '\u{bb}');
    assert!(
        svg.contains(&stereotype),
        "the SVG never emitted the stereotype for a memberless class"
    );
    assert!(
        texts.iter().any(|text| text == &stereotype),
        "the canvas never drew the stereotype for a memberless class; drawn text was {texts:?}"
    );
    // ⚠️ AND THE SOURCE SPELLING MUST NOT SURVIVE INTO THE OUTPUT. Asserting only that the drawn
    // form is present would still pass if a backend emitted BOTH — or regressed to echoing the raw
    // `<<interface>>` alongside it — which is a different wrong picture, not a missing one.
    let written = format!("{}interface{}", "<<", ">>");
    assert!(
        !svg.contains(&written) && !texts.iter().any(|text| text == &written),
        "a backend drew the WRITTEN stereotype spelling instead of the drawn one; \
         svg_has={} canvas_texts={texts:?}",
        svg.contains(&written)
    );

    // The class NAME must survive alongside it — drawing the stereotype in place of the name would
    // satisfy the assertion above and lose more than it gained.
    assert!(
        texts.iter().any(|text| text == "Shape"),
        "the class name was lost when the stereotype was drawn: {texts:?}"
    );
}

mod sankey {
    use fm_render_canvas::{
        Canvas2dRenderer, CanvasRenderConfig, DrawOperation, GpuRenderPlan, MockCanvas2dContext,
    };

    fn canvas(
        ir: &fm_core::MermaidDiagramIr,
        layout: &fm_layout::DiagramLayout,
    ) -> MockCanvas2dContext {
        let mut context = MockCanvas2dContext::new(1600.0, 1200.0);
        let result = Canvas2dRenderer::new(CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        })
        .render(layout, ir, &mut context);
        assert_eq!(result.nodes_drawn, layout.nodes.len());
        assert_eq!(result.edges_drawn, layout.edges.len());
        assert_eq!(result.labels_drawn, texts(&context).len());
        context
    }

    fn texts(context: &MockCanvas2dContext) -> Vec<(&str, f64, f64)> {
        context
            .operations()
            .iter()
            .filter_map(|operation| match operation {
                DrawOperation::FillText(text, x, y) => Some((text.as_str(), *x, *y)),
                _ => None,
            })
            .collect()
    }

    fn gpu_texts(plan: &GpuRenderPlan) -> Vec<String> {
        plan.text_runs
            .iter()
            .map(|run| {
                assert!(run.quad_count > 0, "a displayed label must contain glyphs");
                plan.text_quads[run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
                    .iter()
                    .map(|quad| {
                        plan.glyph_atlas
                            .cells
                            .iter()
                            .find(|cell| cell.uv_min == quad.uv_min && cell.uv_max == quad.uv_max)
                            .expect("every displayed glyph must be in the atlas")
                            .glyph
                    })
                    .collect()
            })
            .collect()
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }

    fn assert_ribbons(
        context: &MockCanvas2dContext,
        layout: &fm_layout::DiagramLayout,
        plan: &GpuRenderPlan,
        widths: &[f64],
    ) {
        let mut painted = Vec::new();
        let mut points = Vec::new();
        let mut width = 0.0;
        for operation in context.operations() {
            match operation {
                DrawOperation::BeginPath => points.clear(),
                DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => points.push((*x, *y)),
                DrawOperation::SetLineWidth(value) => width = *value,
                DrawOperation::Stroke if !points.is_empty() => {
                    painted.push((points.clone(), width))
                }
                _ => {}
            }
        }
        assert_eq!(layout.edges.len(), widths.len());
        let expected_segments: usize = layout.edges.iter().map(|edge| edge.points.len() - 1).sum();
        assert_eq!(plan.edge_segments.len(), expected_segments);
        // The parser represents Sankey flows as ArrowType::Arrow, which the existing SVG and
        // Canvas paths preserve. Changing quantities must not drop or invent those markers.
        assert_eq!(plan.arrowheads.len(), layout.edges.len());
        for (edge, expected) in layout.edges.iter().zip(widths) {
            // Column separators are also stroked paths. Identify each flow by its full route,
            // rather than mistaking chart furniture for an additional flow.
            let matching: Vec<_> = painted
                .iter()
                .filter(|(points, _)| {
                    points.len() == edge.points.len()
                        && points.iter().zip(&edge.points).all(|((x, y), point)| {
                            (*x - (f64::from(point.x) - f64::from(layout.bounds.x))).abs() < 0.001
                                && (*y - (f64::from(point.y) - f64::from(layout.bounds.y))).abs()
                                    < 0.001
                        })
                })
                .collect();
            assert_eq!(matching.len(), 1, "each flow must be drawn exactly once");
            let (points, width) = matching[0];
            close(*width, *expected);
            assert_eq!(points.len(), edge.points.len());
            for ((x, y), point) in points.iter().zip(&edge.points) {
                close(*x, f64::from(point.x) - f64::from(layout.bounds.x));
                close(*y, f64::from(point.y) - f64::from(layout.bounds.y));
            }
            let segments: Vec<_> = plan
                .edge_segments
                .iter()
                .filter(|segment| segment.edge_index as usize == edge.edge_index)
                .collect();
            assert_eq!(segments.len(), edge.points.len() - 1);
            for (segment, endpoints) in segments.iter().zip(edge.points.windows(2)) {
                close(f64::from(segment.width), *expected);
                assert_eq!(segment.from, [endpoints[0].x, endpoints[0].y]);
                assert_eq!(segment.to, [endpoints[1].x, endpoints[1].y]);
            }
            let heads: Vec<_> = plan
                .arrowheads
                .iter()
                .filter(|head| head.edge_index as usize == edge.edge_index)
                .collect();
            assert_eq!(heads.len(), 1);
            let end = edge.points.last().unwrap();
            assert_eq!(heads[0].position, [end.x, end.y]);
        }
    }

    #[test]
    fn unequal_flows_draw_proportional_ribbons_and_contained_node_totals() {
        let ir = fm_parser::parse("sankey-beta\n\nA,B,100\nA,C,50\nA,D,10\n").ir;
        assert_eq!(ir.diagram_type, fm_core::DiagramType::Sankey);
        assert_eq!(ir.nodes.len(), 4);
        assert_eq!(ir.edges.len(), 3);
        let layout = fm_layout::layout_diagram(&ir);
        let context = canvas(&ir, &layout);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        assert_ribbons(&context, &layout, &plan, &[24.0, 12.0, 2.4]);

        let labels = texts(&context);
        assert_eq!(
            labels.len(),
            8,
            "names and totals, with no extra link labels"
        );
        assert_eq!(
            gpu_texts(&plan),
            labels.iter().map(|label| label.0).collect::<Vec<_>>()
        );
        for (node, rows) in layout.nodes.iter().zip(labels.chunks_exact(2)) {
            let name = ir.node_display_text(&ir.nodes[node.node_index]);
            let total = match name {
                "A" => "160",
                "B" => "100",
                "C" => "50",
                "D" => "10",
                _ => panic!("unexpected node {name}"),
            };
            assert_eq!(rows[0].0, name);
            assert_eq!(rows[1].0, total);
            close(rows[1].2 - rows[0].2, 22.5);
            for (_, x, y) in rows {
                close(
                    *x,
                    f64::from(node.bounds.x - layout.bounds.x + node.bounds.width * 0.5),
                );
                assert!(*y - 7.5 >= f64::from(node.bounds.y - layout.bounds.y));
                assert!(
                    *y + 7.5 <= f64::from(node.bounds.y - layout.bounds.y + node.bounds.height)
                );
            }
        }
        for (run, (_, _, canvas_y)) in plan.text_runs.iter().zip(&labels) {
            for quad in &plan.text_quads
                [run.first_quad as usize..(run.first_quad + run.quad_count) as usize]
            {
                close(f64::from(quad.center[1] - layout.bounds.y), *canvas_y);
            }
        }

        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &Default::default());
        for width in ["24", "12", "2.40"] {
            assert!(svg.contains(&format!("stroke-width=\"{width}\"")));
        }
        for total in ["160", "100", "50", "10"] {
            assert!(svg.contains(&format!(">{total}</tspan>")));
        }
        assert!(!svg.contains("class=\"edge-label\""));
    }

    #[test]
    fn unbalanced_and_decimal_throughput_uses_maximum_not_sum_or_raw_precision() {
        for (source, expected) in [
            (
                "sankey-beta\n\nA,M,10\nM,B,3\n",
                vec!["A", "10", "M", "10", "B", "3"],
            ),
            (
                "sankey-beta\n\nA,B,1.5\nA,C,2.25\n",
                vec!["A", "3.75", "B", "1.5", "C", "2.25"],
            ),
            (
                "sankey-beta\n\nA,B,1234.5678\n",
                vec!["A", "1234.57", "B", "1234.57"],
            ),
            ("sankey-beta\n\nA,B,1200\n", vec!["A", "1200", "B", "1200"]),
        ] {
            let ir = fm_parser::parse(source).ir;
            let layout = fm_layout::layout_diagram(&ir);
            let context = canvas(&ir, &layout);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            assert_eq!(
                texts(&context).iter().map(|row| row.0).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(gpu_texts(&plan), expected);
        }
    }

    #[test]
    fn tiny_flows_keep_a_visible_floor() {
        let ir = fm_parser::parse("sankey-beta\n\nA,B,1000\nA,C,1\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        let context = canvas(&ir, &layout);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        assert_ribbons(&context, &layout, &plan, &[24.0, 1.5]);
        assert_eq!(gpu_texts(&plan), ["A", "1001", "B", "1000", "C", "1"]);
    }

    #[test]
    fn unusable_values_keep_finite_widths_without_inventing_totals_or_edges() {
        let parsed = fm_parser::parse("sankey-beta\n\nA,B,10\nA,C,1\n").ir;
        let layout = fm_layout::layout_diagram(&parsed);
        // Keep a valid layout while testing programmatic IR values, including values the parser
        // may reject. Rendering invalid data must not invent quantities or nonfinite geometry.
        for invalid in ["0", "-5", "NaN", "inf", "not-a-number"] {
            let mut ir = parsed.clone();
            ir.labels[ir.edges[1].label.unwrap().0].text = invalid.to_owned();
            let context = canvas(&ir, &layout);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            assert_ribbons(&context, &layout, &plan, &[24.0, 1.5]);
            let expected = ["A", "10", "B", "10", "C"];
            assert_eq!(
                texts(&context).iter().map(|row| row.0).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(gpu_texts(&plan), expected);
            ir.labels[ir.edges[0].label.unwrap().0].text = invalid.to_owned();
            let context = canvas(&ir, &layout);
            let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
            assert_ribbons(&context, &layout, &plan, &[1.5, 1.5]);
            assert_eq!(gpu_texts(&plan), ["A", "B", "C"]);
            assert_eq!(
                texts(&context).iter().map(|row| row.0).collect::<Vec<_>>(),
                ["A", "B", "C"]
            );
        }
    }

    #[test]
    fn declared_edge_width_overrides_quantity_without_changing_node_totals() {
        let mut ir = fm_parser::parse("sankey-beta\n\nA,B,100\nA,C,50\n").ir;
        ir.style_refs.push(fm_core::IrStyleRef {
            target: fm_core::IrStyleTarget::Link(1),
            style: "stroke-width:7px".to_owned(),
            span: Default::default(),
        });
        ir.populate_structured_styles();
        let layout = fm_layout::layout_diagram(&ir);
        let context = canvas(&ir, &layout);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        assert_ribbons(&context, &layout, &plan, &[24.0, 7.0]);
        assert_eq!(gpu_texts(&plan), ["A", "150", "B", "100", "C", "50"]);
    }

    #[test]
    fn ordinary_numeric_edge_labels_remain_visible_at_generic_width() {
        let ir =
            fm_parser::parse("flowchart LR\nA[Alpha] ---|100| B[Beta]\nB ---|50| C[Gamma]\n").ir;
        let layout = fm_layout::layout_diagram(&ir);
        let context = canvas(&ir, &layout);
        let plan = GpuRenderPlan::from_layout(&ir, &layout, 1.5);
        assert!(plan.edge_segments.iter().all(|edge| edge.width == 1.5));
        assert_eq!(
            plan.edge_segments.len(),
            layout
                .edges
                .iter()
                .map(|edge| edge.points.len() - 1)
                .sum::<usize>()
        );
        let mut actual: Vec<_> = texts(&context).iter().map(|row| row.0).collect();
        actual.sort_unstable();
        assert_eq!(actual, ["100", "50", "Alpha", "Beta", "Gamma"]);
        let mut planned = gpu_texts(&plan);
        planned.sort_unstable();
        assert_eq!(planned, actual);
        assert!(fm_layout::sankey_node_label(&ir, 0).is_none());
        assert!(fm_layout::sankey_widest_flow(&ir).is_none());
    }
}

mod xychart {
    use fm_render_canvas::{
        Canvas2dRenderer, CanvasRenderConfig, DrawOperation, MockCanvas2dContext, TextAlign,
    };

    fn render(
        source: &str,
    ) -> (
        fm_core::MermaidDiagramIr,
        fm_layout::DiagramLayout,
        MockCanvas2dContext,
    ) {
        let ir = fm_parser::parse(source).ir;
        assert_eq!(ir.diagram_type, fm_core::DiagramType::XyChart);
        assert!(ir.xy_chart_meta.is_some());
        let layout = fm_layout::layout_diagram(&ir);
        let mut context = MockCanvas2dContext::new(1200.0, 900.0);
        let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
            auto_fit: false,
            padding: 0.0,
            ..Default::default()
        });
        let result = renderer.render(&layout, &ir, &mut context);
        let label_count = context
            .operations()
            .iter()
            .filter(|op| matches!(op, DrawOperation::FillText(..)))
            .count();
        assert_eq!(result.labels_drawn, label_count);
        assert_eq!(result.nodes_drawn, layout.nodes.len());
        (ir, layout, context)
    }

    fn close(actual: f64, expected: f64) {
        assert!((actual - expected).abs() < 0.0001, "{actual} != {expected}");
    }

    fn text(context: &MockCanvas2dContext, wanted: &str) -> (f64, f64, TextAlign, String) {
        let mut align = TextAlign::Left;
        let mut fill = String::new();
        let mut found = Vec::new();
        for op in context.operations() {
            match op {
                DrawOperation::SetTextAlign(value) => align = *value,
                DrawOperation::SetFillStyle(value) => fill.clone_from(value),
                DrawOperation::FillText(value, x, y) if value == wanted => {
                    found.push((*x, *y, align, fill.clone()));
                }
                _ => {}
            }
        }
        assert_eq!(found.len(), 1, "expected one {wanted:?}, got {found:?}");
        found.pop().unwrap()
    }

    #[derive(Debug)]
    struct PaintedPath {
        points: Vec<(f64, f64)>,
        closed: bool,
        color: String,
        alpha: f64,
        width: f64,
        filled: bool,
    }

    fn paths(context: &MockCanvas2dContext) -> Vec<PaintedPath> {
        let mut result = Vec::new();
        let (mut points, mut closed) = (Vec::new(), false);
        let (mut fill, mut stroke) = (String::new(), String::new());
        let (mut alpha, mut width) = (1.0, 1.0);
        for op in context.operations() {
            match op {
                DrawOperation::BeginPath => {
                    points.clear();
                    closed = false;
                }
                DrawOperation::MoveTo(x, y) | DrawOperation::LineTo(x, y) => points.push((*x, *y)),
                DrawOperation::ClosePath => closed = true,
                DrawOperation::SetFillStyle(value) => fill.clone_from(value),
                DrawOperation::SetStrokeStyle(value) => stroke.clone_from(value),
                DrawOperation::SetGlobalAlpha(value) => alpha = *value,
                DrawOperation::SetLineWidth(value) => width = *value,
                DrawOperation::Fill | DrawOperation::Stroke if !points.is_empty() => {
                    let filled = matches!(op, DrawOperation::Fill);
                    result.push(PaintedPath {
                        points: points.clone(),
                        closed,
                        color: if filled { fill.clone() } else { stroke.clone() },
                        alpha,
                        width,
                        filled,
                    });
                }
                _ => {}
            }
        }
        result
    }

    #[test]
    fn value_axes_and_grouped_bars_use_the_layout_scale() {
        let source = "xychart-beta\n title Orders\n x-axis \"Month\" [Jan, Feb, Mar]\n y-axis \"Units\" -20 --> 80\n bar \"Actual\" [-20, 40, 80]\n bar \"Target\" [20, 30, 60]\n";
        let (ir, layout, context) = render(source);
        let meta = ir.xy_chart_meta.as_ref().unwrap();
        assert_eq!(meta.series.len(), 2);
        let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
        let config = CanvasRenderConfig::default();

        // The ticks encode the declared numeric range, including zero and negative values.
        // Checking a category name alone used to pass while the entire value scale was absent.
        for value in (-20..=80).step_by(10) {
            let (x, y, align, fill) = text(&context, &value.to_string());
            close(x, f64::from(plot.x) - 10.0);
            close(
                y,
                f64::from(plot.y + plot.height * (80 - value) as f32 / 100.0) + 4.0,
            );
            assert_eq!(align, TextAlign::Right);
            assert_eq!(fill, config.label_color);
        }
        for (index, label) in ["Jan", "Feb", "Mar"].iter().enumerate() {
            let (x, y, align, _) = text(&context, label);
            close(
                x,
                f64::from(plot.x) + f64::from(plot.width) / 3.0 * (index as f64 + 0.5),
            );
            close(y, f64::from(plot.y + plot.height) + 24.0);
            assert_eq!(align, TextAlign::Center);
        }
        for label in ["Orders", "Month", "Units"] {
            text(&context, label);
        }

        // Every bar is painted once, in its series color, with exactly the layout's rectangle.
        let (mut fill, mut alpha) = (String::new(), 1.0);
        let mut bars = Vec::new();
        for op in context.operations() {
            match op {
                DrawOperation::SetFillStyle(value) => fill.clone_from(value),
                DrawOperation::SetGlobalAlpha(value) => alpha = *value,
                DrawOperation::FillRect(x, y, w, h) if config.accents.contains(&fill) => {
                    bars.push((fill.clone(), alpha, (*x, *y, *w, *h)));
                }
                _ => {}
            }
        }
        assert_eq!(bars.len(), 6);
        for (series_index, series) in meta.series.iter().enumerate() {
            for (index, id) in series.nodes.iter().enumerate() {
                let node = layout.nodes.iter().find(|n| n.node_index == id.0).unwrap();
                let bar = &bars[series_index * 3 + index];
                assert_eq!(bar.0, config.accents[series_index]);
                close(bar.1, 0.78);
                for (actual, expected) in [
                    (bar.2.0, node.bounds.x),
                    (bar.2.1, node.bounds.y),
                    (bar.2.2, node.bounds.width),
                    (bar.2.3, node.bounds.height),
                ] {
                    close(actual, f64::from(expected));
                }
            }
        }
        let svg = fm_render_svg::render_svg_with_layout(&ir, &layout, &Default::default());
        assert_eq!(svg.matches("class=\"fm-xychart-bar\"").count(), bars.len());
        assert!(svg.contains("class=\"fm-xychart-x-label\">Month</text>"));
        assert!(svg.contains("class=\"fm-xychart-y-label\">Units</text>"));
        // The old generic node labels included data text inside every bar. No duplicate labels.
        let texts: Vec<_> = context
            .operations()
            .iter()
            .filter_map(|op| match op {
                DrawOperation::FillText(value, ..) => Some(value),
                _ => None,
            })
            .collect();
        assert_eq!(texts.len(), 17);
    }

    #[test]
    fn horizontal_lines_and_areas_keep_their_series_and_zero_baseline() {
        let source = "xychart-beta horizontal\n title Revenue\n x-axis \"Quarter\" [Q1, Q2, Q3]\n y-axis \"Amount\" -10 --> 30\n line [0, 10, 20]\n area [-5, 5, 25]\n";
        let (ir, layout, context) = render(source);
        let meta = ir.xy_chart_meta.as_ref().unwrap();
        assert!(meta.horizontal);
        assert_eq!(meta.series[1].kind, fm_core::IrXySeriesKind::Area);
        let plot = fm_layout::xychart_plot_bounds(meta, layout.bounds);
        let config = CanvasRenderConfig::default();
        let paths = paths(&context);

        for (series_index, series) in meta.series.iter().enumerate() {
            let centers: Vec<_> = series
                .nodes
                .iter()
                .map(|id| {
                    let point = layout
                        .nodes
                        .iter()
                        .find(|node| node.node_index == id.0)
                        .unwrap()
                        .bounds
                        .center();
                    (f64::from(point.x), f64::from(point.y))
                })
                .collect();
            let lines: Vec<_> = paths
                .iter()
                .filter(|path| {
                    !path.filled && path.color == config.accents[series_index] && path.width == 3.0
                })
                .collect();
            assert_eq!(lines.len(), 1);
            assert_eq!(lines[0].points, centers);
            assert!(!lines[0].closed);
            if series_index == 1 {
                let fills: Vec<_> = paths
                    .iter()
                    .filter(|path| path.filled && path.color == config.accents[1])
                    .collect();
                assert_eq!(fills.len(), 1);
                let area = fills[0];
                assert!(area.closed);
                close(area.alpha, 0.16);
                assert_eq!(&area.points[1..4], centers.as_slice());
                let zero_x = f64::from(plot.x + plot.width / 4.0);
                assert_eq!(area.points[0], (zero_x, centers[0].1));
                assert_eq!(area.points[4], (zero_x, centers[2].1));
            }
        }
        assert_eq!(
            context
                .operations()
                .iter()
                .filter(|op| matches!(op, DrawOperation::Arc(..)))
                .count(),
            6
        );
        let (zero_x, zero_y, zero_align, _) = text(&context, "0");
        close(zero_x, f64::from(plot.x + plot.width / 4.0));
        close(zero_y, f64::from(plot.y) - 11.0);
        assert_eq!(zero_align, TextAlign::Center);
        let (q_x, q_y, q_align, _) = text(&context, "Q1");
        close(q_x, f64::from(plot.x) - 11.0);
        close(
            q_y,
            f64::from(plot.y + plot.height / 6.0) + config.font_size * 0.74 * 0.35,
        );
        assert_eq!(q_align, TextAlign::Right);
        for label in ["Revenue", "Quarter", "Amount"] {
            text(&context, label);
        }
    }

    #[test]
    fn numeric_categories_and_degenerate_series_still_have_finite_geometry() {
        for source in [
            "xychart-beta\n x-axis 10 --> 30\n y-axis 0 --> 0\n area [0, 0, 0]\n",
            "xychart-beta\n x-axis [only]\n line [4]\n",
            "xychart-beta\n title Empty\n",
        ] {
            let (_, _, context) = render(source);
            for op in context.operations() {
                let numbers: Vec<f64> = match op {
                    DrawOperation::MoveTo(x, y)
                    | DrawOperation::LineTo(x, y)
                    | DrawOperation::FillText(_, x, y) => vec![*x, *y],
                    DrawOperation::FillRect(x, y, w, h) | DrawOperation::StrokeRect(x, y, w, h) => {
                        vec![*x, *y, *w, *h]
                    }
                    DrawOperation::Arc(x, y, radius, start, end) => {
                        vec![*x, *y, *radius, *start, *end]
                    }
                    _ => Vec::new(),
                };
                assert!(
                    numbers.iter().all(|value| value.is_finite()),
                    "non-finite operation: {op:?}"
                );
            }
            if source.contains("10 --> 30") {
                for label in ["10", "20", "30"] {
                    text(&context, label);
                }
            }
        }
    }

    #[test]
    fn chart_drawing_is_invariant_under_layout_origin_translation() {
        let source = "xychart-beta\n title Sales\n x-axis [A, B]\n bar [10, 20]\n";
        let (ir, layout, _) = render(source);
        for auto_fit in [false, true] {
            let config = CanvasRenderConfig {
                auto_fit,
                ..Default::default()
            };
            let draw = |layout: &fm_layout::DiagramLayout| {
                let mut context = MockCanvas2dContext::new(1200.0, 900.0);
                Canvas2dRenderer::new(config.clone()).render(layout, &ir, &mut context);
                context
            };
            let original = draw(&layout);
            let mut moved = layout.clone();
            moved.bounds.x += 512.0;
            moved.bounds.y -= 256.0;
            for node in &mut moved.nodes {
                node.bounds.x += 512.0;
                node.bounds.y -= 256.0;
            }
            for edge in &mut moved.edges {
                for point in &mut edge.points {
                    point.x += 512.0;
                    point.y -= 256.0;
                }
            }
            let shifted = draw(&moved);
            for label in ["Sales", "A", "B", "0", "20"] {
                let first = text(&original, label);
                let second = text(&shifted, label);
                close(first.0, second.0);
                close(first.1, second.1);
                assert_eq!(first.2, second.2);
            }
        }
    }
}
