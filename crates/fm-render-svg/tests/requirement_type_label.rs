//! Differential test: the type label mermaid draws on a requirement node.
//!
//! THE DIVERGENCE THIS PINS. `functionalRequirement LoginReq { … }` drew `«functionalRequirement»`.
//! mermaid draws `<<Functional Requirement>>` — different words inside a differently-spelled wrapper.
//!
//! REFERENCE, transcribed from the pinned 11.15.0 bundle's own table and confirmed against its db:
//!
//! ```text
//!   this.RequirementType = {
//!     REQUIREMENT:             "Requirement",
//!     FUNCTIONAL_REQUIREMENT:  "Functional Requirement",
//!     INTERFACE_REQUIREMENT:   "Interface Requirement",
//!     PERFORMANCE_REQUIREMENT: "Performance Requirement",
//!     PHYSICAL_REQUIREMENT:    "Physical Requirement",
//!     DESIGN_CONSTRAINT:       "Design Constraint",
//!   }
//! ```
//!
//! and its renderer draws `` `<<${n.type}>>` ``. Probing `requirement_basic.mmd` through
//! `diagram_db_probe.mjs` shows the db agreeing: `LoginReq` stores `type: "Functional Requirement"`.
//!
//! ⚠️ THE KEYWORD IS NOT THE LABEL, and that is the substance. `functionalRequirement` is what the
//! author TYPES; `Functional Requirement` is what mermaid SHOWS. Rendering the keyword is the same
//! class of defect as the journey actor legend drawing `Big_Corp`: a machine-facing token reaching a
//! reader.
//!
//! ⚠️ HOW THIS SURVIVED: requirement diagrams have no head-to-head corpus item, and mermaid's
//! requirement renderer cannot run under jsdom, so NEITHER oracle covers the family —
//! `equivalence.mjs` has nothing to compare and `drawn_text_diff.mjs` reports INCUMBENT-DNF. It was
//! found by comparing the parsed DB instead, which needs no renderer.

fn runs(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else {
            break;
        };
        let text = rest[open + 1..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&amp;", "&");
        let text = text.trim().to_string();
        if !text.is_empty() && !text.contains("<t") {
            out.push(text);
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

fn requirement(keyword: &str) -> String {
    format!("requirementDiagram\n    {keyword} R {{\n        id: \"1\"\n        text: t\n    }}\n")
}

/// Every keyword in mermaid's table, and the string it displays.
const TABLE: &[(&str, &str)] = &[
    ("requirement", "Requirement"),
    ("functionalRequirement", "Functional Requirement"),
    ("interfaceRequirement", "Interface Requirement"),
    ("performanceRequirement", "Performance Requirement"),
    ("physicalRequirement", "Physical Requirement"),
    ("designConstraint", "Design Constraint"),
];

#[test]
fn every_type_draws_the_name_mermaid_displays() {
    for (keyword, display) in TABLE {
        let drawn = runs(&requirement(keyword));
        let expected = format!("<<{display}>>");
        assert!(
            drawn.iter().any(|run| run == &expected),
            "{keyword} must draw {expected:?}; drew {drawn:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL. Drawing the authored keyword satisfies nothing above for five of the six
/// types — but `requirement` alone happens to differ only in capitalisation, so a table checked on
/// that type ONLY would pass a raw-keyword implementation. Assert the keyword is absent.
#[test]
fn the_authored_keyword_is_not_drawn() {
    for (keyword, _) in TABLE {
        let drawn = runs(&requirement(keyword));
        assert!(
            !drawn.iter().any(|run| run.contains(keyword)),
            "{keyword:?} reached the drawing as the raw keyword: {drawn:?}"
        );
    }
}

/// ⚠️ CONTROL for the WRAPPER. mermaid uses ASCII angles, which is also what this renderer already
/// uses for a class stereotype — the requirement path was the odd one out with guillemets.
#[test]
fn the_wrapper_is_ascii_angles_not_guillemets() {
    let drawn = runs(&requirement("functionalRequirement"));
    assert!(
        !drawn
            .iter()
            .any(|run| run.contains('\u{00ab}') || run.contains('\u{00bb}')),
        "the requirement type is still wrapped in guillemets: {drawn:?}"
    );
}

/// CONTROL on the FALLBACK, asserted at the function rather than through a render.
///
/// ⚠️ I FIRST WROTE THIS AS A RENDER TEST AND IT FAILED, because the premise was wrong: mermaid does
/// not accept an unknown type at all. `someFutureRequirement R { … }` is a SYNTAX ERROR there
/// (`Expecting 'STYLE_SEPARATOR', 'END_ARROW_L', 'LINE', got 'STRUCT_START'`), and our parser
/// likewise builds no requirement node, so there is nothing to draw and nothing to compare.
///
/// The pass-through in `requirement_type_display` is therefore defensive code for a type mermaid may
/// add later, not observable behaviour today — so it is checked where it lives.
#[test]
fn an_unknown_type_is_passed_through_by_the_mapping() {
    assert_eq!(
        fm_core::requirement_type_display("someFutureRequirement"),
        "someFutureRequirement",
        "an unrecognised type must be returned unchanged rather than mangled"
    );
    for (keyword, display) in TABLE {
        assert_eq!(
            fm_core::requirement_type_display(keyword),
            *display,
            "{keyword} maps to the wrong display name"
        );
    }
}
