//! ER attribute cells on the canvas sit at shared column offsets, inside their box (bd-jbrzc).
//!
//! bd-xxvch split an ER attribute row into four cells at shared column offsets in the SVG writers
//! and — crucially — made fm-layout size the entity box from that same column geometry
//! (`fm_core::er_cell_columns`). fm-render-canvas kept drawing `attr.display_row()` as ONE fused
//! run, so a vector surface was being measured by a rule it did not follow: the reserved width went
//! unused and the fields did not line up, while SVG aligned them from the same numbers.
//!
//! ⚠️ THE SKEW FIXTURE IS THE POINT. Column geometry and fused geometry agree whenever the widest
//! type and the widest name belong to the SAME attribute, which is the common case and hides the
//! difference. `T { verylongtypename a / t verylongattributename PK }` separates them: the widest
//! type is on row one and the widest name on row two.
//!
//! ⚠️ A CONTROL IS REQUIRED, and this is why: in fm-render-svg SIX of the seven cell tests passed
//! with the box mis-sized by 60px. Text content, cell order, composite-key handling and
//! writer-lockstep are all blind to geometry. So the assertions here are about POSITIONS — every
//! drawn cell inside the box, and corresponding cells sharing a column across rows.

use fm_render_canvas::{Canvas2dRenderer, CanvasRenderConfig, DrawOperation, MockCanvas2dContext};

/// The fixture the bead names: widest type and widest name on DIFFERENT attributes.
const SKEW: &str = "erDiagram\n    T {\n        verylongtypename a\n        \
                    t verylongattributename PK\n    }\n";

/// Every operation the renderer issued, with auto-fit off so one coordinate system is in play.
fn operations(source: &str) -> Vec<DrawOperation> {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
        auto_fit: false,
        ..CanvasRenderConfig::default()
    });
    let mut ctx = MockCanvas2dContext::new(2000.0, 1200.0);
    renderer.render(&layout, &ir, &mut ctx);
    ctx.operations().to_vec()
}

/// Every `FillText` the renderer issued, as `(text, x, y)`.
fn texts(source: &str) -> Vec<(String, f64, f64)> {
    let ir = fm_parser::parse(source).ir;
    let layout = fm_layout::layout_diagram(&ir);
    // ⚠️ AUTO-FIT OFF, AND THAT IS LOAD-BEARING FOR THIS FILE. With it on, the renderer scales and
    // translates everything to the viewport, so a recorded `fill_text` x is in SCREEN space while
    // `layout.nodes[..].bounds` is in LAYOUT space — comparing them measures the viewport transform,
    // not the geometry. The first version of this test did exactly that and reported a 15px
    // "overflow" that was the fit factor.
    let mut renderer = Canvas2dRenderer::new(CanvasRenderConfig {
        auto_fit: false,
        ..CanvasRenderConfig::default()
    });
    let mut ctx = MockCanvas2dContext::new(2000.0, 1200.0);
    renderer.render(&layout, &ir, &mut ctx);
    ctx.operations()
        .iter()
        .filter_map(|op| match op {
            DrawOperation::FillText(text, x, y) => Some((text.clone(), *x, *y)),
            _ => None,
        })
        .collect()
}

fn find(all: &[(String, f64, f64)], text: &str) -> (f64, f64) {
    all.iter()
        .find(|(t, _, _)| t == text)
        .map(|(_, x, y)| (*x, *y))
        .unwrap_or_else(|| {
            panic!(
                "no cell drawn for {text:?}; drawn: {:?}",
                all.iter().map(|(t, _, _)| t).collect::<Vec<_>>()
            )
        })
}

/// THE SPLIT: each field is its own drawn cell, not one fused run.
///
/// The negative half is the fused string: if it is still being drawn, the four separate cells are
/// not, and the entity is rendered by the rule the box was NOT sized by.
#[test]
fn each_attribute_field_is_its_own_drawn_cell() {
    let all = texts(SKEW);
    for cell in ["verylongtypename", "a", "t", "verylongattributename", "PK"] {
        assert!(
            all.iter().any(|(t, _, _)| t == cell),
            "no separate cell for {cell:?}; drawn: {:?}",
            all.iter().map(|(t, _, _)| t).collect::<Vec<_>>()
        );
    }
    assert!(
        !all.iter().any(|(t, _, _)| t.contains("verylongtypename a")),
        "the fused row is still being drawn, so the cells are not the drawing"
    );
}

/// THE ALIGNMENT: corresponding cells share a column across rows.
///
/// This is what a column layout IS, and what the fused row could not do — measured on the fixture,
/// SVG puts both second cells at x=195.39 while a fused row puts each wherever its first cell
/// happened to end. Asserted as an equality between two rows rather than against a literal, so it
/// stays true if the box moves.
#[test]
fn corresponding_cells_share_a_column_across_rows() {
    let all = texts(SKEW);
    let (type_a, _) = find(&all, "verylongtypename");
    let (type_b, _) = find(&all, "t");
    let (name_a, _) = find(&all, "a");
    let (name_b, _) = find(&all, "verylongattributename");

    assert!(
        (type_a - type_b).abs() < 0.01,
        "the two type cells start at {type_a} and {type_b}, so column 0 is not shared"
    );
    assert!(
        (name_a - name_b).abs() < 0.01,
        "the two name cells start at {name_a} and {name_b}, so column 1 is not shared"
    );
    // And the columns are genuinely distinct — an implementation that put every cell at the same x
    // would satisfy both assertions above.
    assert!(
        name_a > type_a + 1.0,
        "the name column starts at {name_a}, not past the type column at {type_a}"
    );
}

/// THE CONTROL THE BEAD REQUIRES: every drawn cell falls INSIDE the entity box.
///
/// The box is sized by fm-layout from the same `er_cell_columns` geometry, so this is the assertion
/// that catches the two halves disagreeing — and it is the one six of seven SVG cell tests could
/// not make, since content and order are blind to a box mis-sized by 60px.
///
/// ⚠️ COMPARED AGAINST THE BOX AS DRAWN, NOT AGAINST `layout.nodes[..].bounds`. Two earlier versions
/// of this test compared recorded text positions to layout-space bounds and reported overflows of
/// 15px and then 43px that were neither: the renderer applies its own `padding` (28) and a viewport
/// transform, and it measures at `config.font_size` 14 where layout measures at metrics font size
/// 15. Both numbers were real, and neither was a cell outside its box. Reading the rect out of the
/// SAME operation stream removes the whole class of error — and it asserts what a viewer actually
/// sees, which is the thing that matters.
#[test]
fn every_drawn_cell_falls_inside_the_entity_box() {
    let ops = operations(SKEW);
    // The entity box is the widest rect drawn; an ER diagram of one entity has no larger one.
    let (bx, _by, bw, _bh) = ops
        .iter()
        .filter_map(|op| match op {
            // The entity box is a path `rect`, not a `stroke_rect` — read off the operation
            // stream rather than assumed, after two guesses about which primitive draws it.
            DrawOperation::Rect(x, y, w, h)
            | DrawOperation::StrokeRect(x, y, w, h)
            | DrawOperation::FillRect(x, y, w, h) => Some((*x, *y, *w, *h)),
            _ => None,
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .expect("no entity rect was drawn");
    let all = texts(SKEW);

    let cells = ["verylongtypename", "a", "t", "verylongattributename", "PK"];
    for cell in cells {
        let (x, _) = find(&all, cell);
        assert!(
            x >= bx - 0.01,
            "cell {cell:?} starts at {x}, left of the box edge {bx}"
        );
        assert!(
            x <= bx + bw + 0.01,
            "cell {cell:?} starts at {x}, past the box's right edge {} (box {bx} w {bw})",
            bx + bw
        );
    }

    // A cell's START being inside is necessary but weak: the widest column must END inside too, or
    // the longest attribute name runs out through the border.
    let (name_x, _) = find(&all, "verylongattributename");
    let widest = "verylongattributename".len() as f64 * 6.0;
    assert!(
        name_x + widest <= bx + bw + 0.01,
        "the widest name runs from {name_x} to {} against a box ending at {}",
        name_x + widest,
        bx + bw
    );

    // And the key column is genuinely off to the right — an implementation that stacked every cell
    // at one offset would satisfy every containment check above.
    let (pk_x, _) = find(&all, "PK");
    assert!(
        pk_x > bx + bw * 0.4,
        "the key column starts at {pk_x}, barely past the box's left edge {bx}: the cells are \
         probably still stacked at one offset"
    );
}

/// A one-attribute entity, where column and fused geometry agree, is unchanged in content.
///
/// The control that says the split did not break the common case it is invisible in.
#[test]
fn the_common_case_still_draws_every_field() {
    let all = texts("erDiagram\n    U {\n        string name PK\n    }\n");
    for cell in ["string", "name", "PK"] {
        assert!(
            all.iter().any(|(t, _, _)| t == cell),
            "the ordinary case lost {cell:?}; drawn: {:?}",
            all.iter().map(|(t, _, _)| t).collect::<Vec<_>>()
        );
    }
}

/// An attribute with no key and no comment draws only the two cells it has.
///
/// Empty cells are SKIPPED, not drawn as empty strings at their column offset — otherwise the
/// drawn-cell count stops meaning anything and a blank text ends up in the accessibility tree.
#[test]
fn empty_cells_are_not_drawn() {
    let all = texts("erDiagram\n    V {\n        string name\n    }\n");
    assert!(
        !all.iter().any(|(t, _, _)| t.is_empty()),
        "an empty cell was drawn: {:?}",
        all.iter().map(|(t, _, _)| t).collect::<Vec<_>>()
    );
}
