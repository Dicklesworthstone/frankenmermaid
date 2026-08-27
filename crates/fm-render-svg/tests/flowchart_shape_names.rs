//! mermaid 11 `@{ shape: … }` names for shapes this renderer ALREADY DRAWS (bd-3ra5y).
//!
//! THE DEFECT, in two halves that share one cause — a hand-kept name table that had drifted from
//! the registry it mirrors:
//!
//!   1. `lean-r` mapped and `lean-l` did not. `trap-b` mapped and `trap-t` did not. Both missing
//!      shapes are DRAWN by this renderer today and reachable through their bracket spellings
//!      (`[\X\]`, `[\X/]`). An author writing a correct name was told it was "not a recognised
//!      shape name" — sent to fix a spelling that was already right, for a shape we render.
//!   2. `UNIMPLEMENTED_UPSTREAM_SHAPES` held 12 of 80 author-facing names, and only shortNames at
//!      that. So `shape: notch-rect` correctly said "not implemented yet" while `shape: card` —
//!      THE SAME SHAPE, the other published name for it — said "check the spelling".
//!
//! Every name asserted here was read out of the pinned mermaid 11.15.0 bundle's shape registry
//! (`shortName` and `aliases`), never guessed. Three names I first wrote from memory —
//! `inv-parallelogram`, `asymmetric`, `rect-left-inv-arrow` — are absent from that registry and are
//! deliberately NOT accepted; `a_name_the_registry_does_not_publish_is_not_accepted` pins that.

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

/// The `fm-node-shape-*` class of the first node, i.e. the geometry actually drawn.
fn shape_class(source: &str) -> String {
    let svg = render(source);
    let start = svg
        .find("<g id=\"fm-node-a-")
        .expect("CONTROL FAILED: no <g> for node a");
    let open_tag = &svg[start..start + svg[start..].find('>').expect("unterminated tag")];
    let at = open_tag
        .find("fm-node-shape-")
        .unwrap_or_else(|| panic!("no shape class in `{open_tag}`"));
    open_tag[at..]
        .split([' ', '"'])
        .next()
        .expect("non-empty")
        .to_string()
}

fn shaped(name: &str) -> String {
    shape_class(&format!(
        "flowchart LR\n  A@{{ shape: {name}, label: \"X\" }}\n  A --> B\n"
    ))
}

fn warning_for(name: &str) -> String {
    let source = format!("flowchart LR\n  A@{{ shape: {name} }}\n  A --> B\n");
    fm_parser::parse(&source)
        .warnings
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("expected a warning for `{name}`"))
}

/// THE DEFECT: each of these is a shape we draw, whose registry name was unmapped.
#[test]
fn shape_names_for_shapes_we_already_draw_are_honoured() {
    for (name, expected) in [
        ("lean-l", "fm-node-shape-inv-parallelogram"),
        ("trap-t", "fm-node-shape-inv-trapezoid"),
        ("odd", "fm-node-shape-asymmetric"),
        ("tri", "fm-node-shape-triangle"),
        ("f-circ", "fm-node-shape-filled-circle"),
        ("cross-circ", "fm-node-shape-crossed-circle"),
        ("cloud", "fm-node-shape-cloud"),
    ] {
        assert_eq!(
            shaped(name),
            expected,
            "`shape: {name}` drew the wrong geometry"
        );
    }
}

/// ⚠️ THE CLAIM THAT MATTERS: the two spellings of one shape must agree.
///
/// `@{ shape: lean-l }` and `[\X\]` are two ways to ask for the same shape. If they disagree, one
/// of them is wrong no matter how good either looks alone — and the bracket form is the one that
/// already worked, so it is the reference.
#[test]
fn a_shape_name_and_its_bracket_spelling_agree() {
    for (name, bracket) in [
        ("lean-r", "A[/X/]"),
        ("lean-l", "A[\\X\\]"),
        ("trap-b", "A[/X\\]"),
        ("trap-t", "A[\\X/]"),
        ("odd", "A>X]"),
    ] {
        let by_bracket = shape_class(&format!("flowchart LR\n  {bracket}\n  A --> B\n"));
        assert_eq!(
            shaped(name),
            by_bracket,
            "`shape: {name}` and `{bracket}` disagree about the same shape"
        );
    }
}

/// Registry ALIASES must reach the same shape as their shortName — an author picks whichever the
/// docs showed them, and the table has to answer for both.
///
/// ⚠️ THE `!= rect` ASSERTION IS NOT DECORATION, and I only added it because a disarm caught this
/// test passing vacuously. Comparing an alias against its shortName is satisfied when BOTH are
/// unmapped: each falls back to the default rectangle, the two agree perfectly, and the test is
/// green while neither name works. Equality alone cannot tell "both right" from "both broken".
#[test]
fn registry_aliases_reach_the_same_shape_as_their_short_name() {
    for (short_name, aliases) in [
        ("lean-l", ["lean-left", "out-in"]),
        ("trap-t", ["trapezoid-top", "inv-trapezoid"]),
        ("tri", ["triangle", "extract"]),
        ("f-circ", ["filled-circle", "junction"]),
        ("cross-circ", ["crossed-circle", "summary"]),
    ] {
        let expected = shaped(short_name);
        assert_ne!(
            expected, "fm-node-shape-rect",
            "`{short_name}` fell back to the default rectangle, so this comparison proves nothing"
        );
        for alias in aliases {
            assert_eq!(
                shaped(alias),
                expected,
                "alias `{alias}` disagrees with its shortName `{short_name}`"
            );
        }
    }
}

/// Aliases that were missing from shapes ALREADY mapped — the same drift, on the other side.
///
/// Each pair names the expected geometry outright rather than only comparing the two spellings,
/// for the reason given above: two unmapped names agree with each other perfectly at the default
/// rectangle.
#[test]
fn aliases_missing_from_already_mapped_shapes_now_resolve() {
    for (alias, short_name, expected) in [
        ("lean-right", "lean-r", "fm-node-shape-parallelogram"),
        ("trapezoid-bottom", "trap-b", "fm-node-shape-trapezoid"),
        ("prepare", "hex", "fm-node-shape-hexagon"),
        ("subproc", "fr-rect", "fm-node-shape-subroutine"),
    ] {
        assert_eq!(
            shaped(alias),
            expected,
            "alias `{alias}` drew the wrong shape"
        );
        assert_eq!(
            shaped(alias),
            shaped(short_name),
            "alias `{alias}` disagrees with `{short_name}`"
        );
    }
}

/// ⚠️ THE MESSAGE SPLIT, which is the whole point of the second list.
///
/// "we have not built this" and "check your spelling" send an author to different fixes, so a name
/// in the wrong bucket actively misleads. `card` and `notch-rect` are the SAME shape under two
/// published names and must give the SAME verdict; before this they gave opposite ones.
/// ⚠️ NAMES LEAVE THIS LIST BY BEING IMPLEMENTED, not by being excused, and the list is REFILLED
/// rather than allowed to shrink.
///
/// `join` went first (`NodeShape::HorizontalBar`), then `card`/`notch-rect`/`start`, then
/// `doc`/`document` (bd-7ls21).
/// Asserting an implemented name is reported as unimplemented would assert something false — but
/// simply deleting entries would quietly weaken the check, so each departure is replaced by another
/// name still in `UNIMPLEMENTED_UPSTREAM_SHAPES`. Six entries in, six entries out.
///
/// The typo control below is untouched, and the two new-shape suites
/// (`fm-parser/tests/fork_join_shape_names.rs`, `fm-render-svg/tests/mermaid11_new_shapes.rs`) each
/// assert that the names they implemented no longer warn AND that unbuilt ones still do.
#[test]
fn a_real_but_unbuilt_shape_name_is_not_called_a_typo() {
    for name in [
        "win-pane",
        "datastore",
        "text",
        "brace",
        "hourglass",
        "brace-l",
    ] {
        let warning = warning_for(name);
        assert!(
            warning.contains("does not implement yet"),
            "`{name}` is a real mermaid 11 name but was reported as a typo: {warning}"
        );
    }
}

/// CONTROL: a name the registry does not publish is still called unrecognised.
///
/// Without this the fix could be "call everything unimplemented", which would tell an author with a
/// genuine typo to wait for a feature that will never come.
#[test]
fn a_genuine_typo_is_still_reported_as_unrecognised() {
    for name in ["bogus-shape", "recangle", "leanl"] {
        let warning = warning_for(name);
        assert!(
            warning.contains("not a recognised shape name"),
            "`{name}` is not a mermaid shape but was reported as unimplemented: {warning}"
        );
    }
}

/// ⚠️ CONTROL: names I invented from memory must NOT be accepted.
///
/// `inv-parallelogram`, `asymmetric` and `rect-left-inv-arrow` are absent from the pinned registry.
/// The first two I wrote into the mapping before checking and removed once the bundle contradicted
/// me; the third is an INTERNAL alias mermaid does not expose as author syntax. Accepting any of
/// them would be inventing compatibility we cannot claim.
#[test]
fn a_name_the_registry_does_not_publish_is_not_accepted() {
    for name in [
        "inv-parallelogram",
        "asymmetric",
        "rect-left-inv-arrow",
        "rect_left_inv_arrow",
    ] {
        let warning = warning_for(name);
        assert!(
            warning.contains("not a recognised shape name"),
            "`{name}` is not published by the registry but was accepted or excused: {warning}"
        );
    }
}

/// CONTROL: an unmapped name leaves the node's shape ALONE rather than forcing a rectangle.
#[test]
fn an_unmapped_name_keeps_the_declared_shape() {
    let with_shape = shape_class("flowchart LR\n  A[[X]]@{ shape: bogus-shape }\n  A --> B\n");
    assert_eq!(
        with_shape, "fm-node-shape-subroutine",
        "an unrecognised `shape:` overwrote the shape the author had already declared"
    );
}
