//! A C4 boundary is captioned with its LABEL, not with the source syntax (bd-039t).
//!
//! `parse_c4_boundary` built the cluster title as
//! `format!("{function_name}({boundary_key}, {display_label})")`, so
//! `System_Boundary(bank, "Banking System")` drew a box captioned
//! `System_Boundary(bank, Banking System)` — the user's own C4 declaration presented as the
//! boundary's name. The variable holding the label was already called `display_label`, and it was
//! then wrapped back into the call it came from.
//!
//! GROUNDED ON THE INCUMBENT: the pinned mermaid 11.15.0 bundle contains ZERO occurrences of the
//! string `System_Boundary(`. It parses the keyword as a grammar token and draws the boundary's
//! label, so it never constructs a display string of that shape at all.
//!
//! ⚠️ WHY THE POSITIVE ASSERTION ALONE IS WORTHLESS HERE, which is the lesson this file exists to
//! carry: `Banking System` is a SUBSTRING of `System_Boundary(bank, Banking System)`. A test that
//! only checks the label is present passes identically before and after the fix. That is exactly
//! how the defect hid from `renderer_agreement.rs`, whose `c4_boundary` case substring-matched
//! `Internal` and reported the SVG as correct while the terminal — which truncates to the box
//! width — lost the label inside the reconstructed call. The NEGATIVE assertion is the whole test.

/// The declared label is drawn, and the SYNTAX is not.
#[test]
fn a_c4_boundary_is_captioned_with_its_label_not_its_syntax() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse(
            "C4Context\n  title Sys\n  System_Boundary(bank, \"Banking System\") {\n    \
             Person(a, \"Alice\", \"A user\")\n  }\n",
        )
        .ir,
    );

    assert!(
        svg.contains("Banking System"),
        "the boundary's declared label is missing:\n{svg}"
    );
    // THE DISCRIMINATING ASSERTION. Without it this test passes on the pre-fix renderer too.
    assert!(
        !svg.contains("System_Boundary("),
        "the boundary is still captioned with its own source syntax:\n{svg}"
    );
}

/// Every boundary keyword takes the same path, so a fix applied to one must not leave the others
/// reconstructing their syntax. `Deployment_Node` is included because it is the one that does not
/// end in `_Boundary` and is easiest to miss.
#[test]
fn no_boundary_keyword_leaks_its_syntax_into_the_output() {
    for (source, label, syntax) in [
        (
            "C4Context\n  title S\n  System_Boundary(b, \"Alpha\") {\n    Person(p, \"P\", \"d\")\n  }\n",
            "Alpha",
            "System_Boundary(",
        ),
        (
            "C4Container\n  title S\n  Container_Boundary(b, \"Beta\") {\n    Person(p, \"P\", \"d\")\n  }\n",
            "Beta",
            "Container_Boundary(",
        ),
        (
            "C4Context\n  title S\n  Enterprise_Boundary(b, \"Gamma\") {\n    Person(p, \"P\", \"d\")\n  }\n",
            "Gamma",
            "Enterprise_Boundary(",
        ),
        (
            "C4Deployment\n  title S\n  Deployment_Node(b, \"Delta\") {\n    Person(p, \"P\", \"d\")\n  }\n",
            "Delta",
            "Deployment_Node(",
        ),
    ] {
        let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
        assert!(
            svg.contains(label),
            "{syntax} lost its declared label {label:?}:\n{svg}"
        );
        assert!(
            !svg.contains(syntax),
            "{syntax} leaked its source syntax into the output:\n{svg}"
        );
    }
}

/// CONTROL: a boundary declared with NO label falls back to its alias, which is what mermaid shows
/// too. Without this, "use the label" could be implemented as "use argument 2" and silently caption
/// an unlabelled boundary with an empty string.
#[test]
fn an_unlabelled_boundary_falls_back_to_its_alias() {
    let svg = fm_render_svg::render_svg(
        &fm_parser::parse(
            "C4Context\n  title S\n  System_Boundary(lonely) {\n    Person(a, \"Alice\", \"d\")\n  }\n",
        )
        .ir,
    );

    assert!(
        svg.contains("lonely"),
        "an unlabelled boundary lost its alias too:\n{svg}"
    );
}
