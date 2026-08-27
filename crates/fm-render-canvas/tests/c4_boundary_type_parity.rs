//! Cross-renderer parity: the C4 boundary type row must reach the canvas, not just the SVG.
//!
//! WHY THIS FILE EXISTS. `3c000e9b` taught the IR to carry a C4 boundary's type and taught the SVG
//! arm to draw it as the second row mermaid draws — `System_Boundary(sb, "Sys")` renders `Sys` then
//! `[SYSTEM]`. It said plainly that the canvas did not yet honour it and that SVG therefore led.
//! That is the correct direction to lead in, and it is still a divergence until closed: the canvas
//! path is what fm-wasm ships to a browser, so a row missing here is as visible to a reader as one
//! missing from the SVG.
//!
//! This is the same defect family as bd-rk14, which measured seven box-content runs the canvas
//! dropped that the SVG drew, and bd-lvj3 before it. The pattern that keeps recurring is a channel
//! reaching one surface and not its siblings, so the guard here is a PARITY guard rather than a
//! canvas-only assertion: it reads both renderers from the SAME IR and requires they agree.
//!
//! REFERENCE BEHAVIOUR, measured in Chromium 151 against the pinned mermaid 11.15.0 bundle rather
//! than inferred: `drawInsideBoundary` rewrites the stored type to `"[" + type + "]"` and
//! `drawBoundary` draws it beneath the label. Drawn runs for the nested fixture below were
//! `Sys`/`[SYSTEM]`, `Generic`/`[custom]`, `Cont`/`[CONTAINER]`, `Corp`/`[ENTERPRISE]`.

use fm_render_canvas::{CanvasRenderConfig, MockCanvas2dContext, render_to_canvas};

/// Text the canvas actually painted, recovered from the mock context's recorded operations.
///
/// ⚠️ RECOVERED FROM DRAW CALLS, not from the IR. Asking the IR what it holds would pass on a
/// renderer that reads the field and paints nothing, which is precisely the bug being guarded.
///
/// Matched as the STRUCTURED `FillText("…"` shape, the same way `text_parity.rs` does it and for the
/// same reason: a bare substring search over the operation dump would also match a colour, a font
/// name or a coordinate, which is how a check like this quietly stops meaning anything.
fn canvas_text(source: &str) -> Vec<String> {
    let ir = fm_parser::parse(source).ir;
    let mut context = MockCanvas2dContext::new(1200.0, 900.0);
    render_to_canvas(&ir, &mut context, &CanvasRenderConfig::default());
    let dump = format!("{:?}", context.operations());
    let mut out = Vec::new();
    let mut rest = dump.as_str();
    while let Some(index) = rest.find("FillText(\"") {
        rest = &rest[index + "FillText(\"".len()..];
        if let Some(end) = rest.find('"') {
            out.push(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// The same runs as the SVG arm draws, for the same IR.
fn svg_text(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let body = rest[open + 1..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let body = body.trim().to_string();
        if !body.is_empty() {
            out.push(body);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

const NESTED: &str = "C4Context\n    title T\n    Enterprise_Boundary(e, \"Corp\") {\n      System_Boundary(sb, \"Sys\") {\n        Person(a, \"A\")\n      }\n      Boundary(gb, \"Generic\", \"custom\") {\n        Person(c, \"C\")\n      }\n      Container_Boundary(cb, \"Cont\") {\n        Person(d, \"D\")\n      }\n    }\n";

#[test]
fn every_boundary_type_row_reaches_the_canvas() {
    let drawn = canvas_text(NESTED);
    for expected in ["[ENTERPRISE]", "[SYSTEM]", "[custom]", "[CONTAINER]"] {
        assert!(
            drawn.iter().any(|run| run == expected),
            "{expected} never reached the canvas: {drawn:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL, and the shape of the defect this closes: the canvas drew the boundary
/// LABEL and stopped. A renderer that paints only the caption satisfies "the boundary is named" and
/// still loses the type.
#[test]
fn the_boundary_label_alone_does_not_satisfy_the_canvas() {
    let drawn = canvas_text(NESTED);
    for label in ["Corp", "Sys", "Generic", "Cont"] {
        assert!(
            drawn.iter().any(|run| run == label),
            "the canvas lost the boundary label {label:?} entirely: {drawn:?}"
        );
    }
    let bracketed = drawn
        .iter()
        .filter(|run| run.starts_with('[') && run.ends_with(']'))
        .count();
    assert_eq!(
        bracketed, 4,
        "four boundaries are declared, so four type rows must be painted: {drawn:?}"
    );
}

/// ⚠️ THE PARITY ASSERTION, which is the one that keeps this from drifting again. Both renderers
/// read the SAME IR, so a type row present in one and absent from the other is a divergence
/// regardless of which surface is "right".
#[test]
fn the_canvas_and_the_svg_agree_on_the_type_rows() {
    let bracketed = |runs: Vec<String>| {
        let mut rows: Vec<String> = runs
            .into_iter()
            .filter(|run| run.starts_with('[') && run.ends_with(']'))
            .collect();
        rows.sort();
        rows
    };
    assert_eq!(
        bracketed(canvas_text(NESTED)),
        bracketed(svg_text(NESTED)),
        "the canvas and the SVG disagree about the C4 boundary type rows"
    );
}

/// CONTROL: an ordinary flowchart subgraph gets no type row on either surface.
///
/// Guards the opposite failure — bracketing every cluster — which would pass every assertion above
/// while inventing a row for diagrams that have no boundary type at all.
#[test]
fn a_plain_subgraph_gets_no_bracketed_row_on_either_surface() {
    let source = "flowchart TD\n  subgraph one[One]\n    a --> b\n  end\n";
    for (surface, runs) in [("canvas", canvas_text(source)), ("svg", svg_text(source))] {
        assert!(
            !runs
                .iter()
                .any(|run| run.starts_with('[') && run.ends_with(']')),
            "{surface} drew a C4 boundary type row for a plain subgraph: {runs:?}"
        );
    }
}
