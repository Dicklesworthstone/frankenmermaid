//! Differential test: class-diagram member generics, against what mermaid-js ACTUALLY draws.
//!
//! THE DIVERGENCE THIS PINS. `class Box { +List~int~ items }` is mermaid's documented spelling for
//! a generic member type. Measured on the pinned 11.15.0 bundle it draws `+List<int> items`; this
//! renderer drew `+List~int~ items`, tildes and all, in the SVG, on the canvas and in the terminal.
//! The class NAME had been converted since `class List~T~` shipped (`format!("{name}<{}>")`) and
//! relation endpoints since bd-9erl — the member row was the one sibling of that family nobody had
//! ported, which is the shape the repo keeps finding: a convention followed by two of three
//! branches is a defect report about the third.
//!
//! THE ORACLE is `tests/fixtures/mermaid_class_generics.tsv`, produced by
//! `scripts/headtohead/class_generics_battery.mjs` from the pinned bundle by ASKING it, member by
//! member, what it displays. It is a measurement, not a transcription: mermaid's rewrite is not
//! "replace each `~…~` pair", and the rows that prove it are the ones no hand-written table would
//! contain — `a~T~ b~U~` → `a<T< b>U>`, `weird~ x` unchanged, `Map~String, int~` converted while
//! `Pair~A, B, C~` is not.
//!
//! WHAT IS ASSERTED, and why each half is needed:
//!
//! 1. THE PARSE. Our member name must equal the `id` mermaid parsed out of the same line. Without
//!    this the display check could pass on a member we had already mangled — agreeing about the
//!    output of a function we fed different input.
//! 2. THE DISPLAY. `visibility + rewritten name` must equal mermaid's own `displayText`. Every
//!    fixture row is an ATTRIBUTE with an EXPLICIT visibility marker, so this equality is about the
//!    rewrite and nothing else.
//! 3. THE NAIVE CONTROL. A hand-written pair-substitution — the implementation this codebase would
//!    plausibly have shipped — must FAIL the fixture. A differential test that a wrong
//!    implementation also passes is not evidence of parity, it is a fixture that cannot tell the
//!    two apart.
//! 4. THE WIRING. The rewrite has to reach the drawn `<text>`, not just the helper. Three of the
//!    five row builders in this workspace are outside this crate, so the end-to-end case renders a
//!    real diagram and reads the text runs back.
//!
//! NOT ASSERTED: mermaid's ` : ` spacing around a method return type (it writes `+f() : T`, we
//! write `+f(): T`) and its EMPTY visibility for an unmarked member (it draws `name`, we draw
//! `+name`). The latter is covered by its own parity test below; folding it into the generated
//! generic fixture would make the generic differential fail for a separate reason.

use std::{fs, path::Path};

/// One measured row of the incumbent's behaviour.
struct Row {
    member: String,
    /// The member id mermaid parsed — the exact string it feeds to its own rewrite.
    id: String,
    /// mermaid's `getDisplayDetails().displayText`.
    display: String,
}

fn fixture() -> Vec<Row> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/mermaid_class_generics.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let member = columns.next().expect("member column").to_string();
            let id = columns.next().expect("id column").to_string();
            let display = columns.next().expect("display column").to_string();
            assert!(columns.next().is_none(), "unexpected 4th column in {member:?}");
            Row { member, id, display }
        })
        .collect();
    // A fixture that regenerated empty, or into a single column, would make every assertion below
    // vacuous. Check the instrument before reading it.
    assert!(rows.len() >= 20, "fixture holds only {} rows", rows.len());
    assert!(
        rows.iter().filter(|row| row.display.contains('<')).count() >= 10,
        "fixture contains no rewritten rows — did the battery run against a bundle that parses?"
    );
    rows
}

fn visibility_symbol(vis: fm_core::ClassVisibility) -> &'static str {
    match vis {
        fm_core::ClassVisibility::Unmarked => "",
        fm_core::ClassVisibility::Public => "+",
        fm_core::ClassVisibility::Private => "-",
        fm_core::ClassVisibility::Protected => "#",
        fm_core::ClassVisibility::Package => "~",
    }
}

/// The one attribute declared by a single-member class body.
fn only_attribute(member: &str) -> fm_core::IrClassMember {
    let source = format!("classDiagram\nclass Foo {{\n{member}\n}}\n");
    let ir = fm_parser::parse(&source).ir;
    let meta = ir
        .nodes
        .iter()
        .find_map(|node| node.class_meta.as_deref())
        .unwrap_or_else(|| panic!("{member:?} produced no class metadata"));
    assert_eq!(
        (meta.attributes.len(), meta.methods.len()),
        (1, 0),
        "{member:?} should parse as exactly one attribute, got {:?} / {:?}",
        meta.attributes,
        meta.methods
    );
    meta.attributes[0].clone()
}

/// Mermaid preserves the absence of a visibility marker. This needs both halves: restoring the old
/// default would draw `+plainField`, while suppressing every marker would make an explicit public
/// member wrong in the opposite direction.
#[test]
fn unmarked_visibility_is_empty_but_explicit_public_stays_plus() {
    let ir = fm_parser::parse(
        "classDiagram\nclass Foo {\n  plainField\n  +publicField\n  plainMethod()\n  +publicMethod()\n}\n",
    )
    .ir;
    let meta = ir
        .nodes
        .iter()
        .find_map(|node| node.class_meta.as_deref())
        .expect("class metadata");
    assert_eq!(
        meta.attributes[0].visibility,
        fm_core::ClassVisibility::Unmarked,
        "a member without a marker must retain that absence in the IR"
    );
    assert_eq!(
        meta.attributes[1].visibility,
        fm_core::ClassVisibility::Public,
        "the explicit + marker must remain public"
    );

    let svg = fm_render_svg::render_svg(&ir);
    let runs = text_runs(&svg);
    for expected in [
        "plainField",
        "+publicField",
        "plainMethod()",
        "+publicMethod()",
    ] {
        assert!(
            runs.iter().any(|run| run == expected),
            "class box did not draw {expected:?}; runs were {runs:?}"
        );
    }
    assert!(
        !runs
            .iter()
            .any(|run| run == "+plainField" || run == "+plainMethod()"),
        "an unmarked member acquired a public marker: {runs:?}"
    );
}

#[test]
fn every_member_displays_the_generics_mermaid_displays() {
    let mut divergent = Vec::new();
    for row in fixture() {
        let parsed = only_attribute(&row.member);

        // (1) SAME INPUT. Our name is the string mermaid calls `id`; if this drifts, the display
        // comparison below is comparing two different questions.
        assert_eq!(
            parsed.name, row.id,
            "{:?}: we parsed the member name as {:?}, mermaid as {:?}",
            row.member, parsed.name, row.id
        );

        // (2) SAME OUTPUT.
        let ours = format!(
            "{}{}",
            visibility_symbol(parsed.visibility),
            fm_core::class_member_display_name(&parsed.name, false)
        );
        if ours != row.display {
            divergent.push(format!("{:?}: ours {:?}, mermaid {:?}", row.member, ours, row.display));
        }
    }
    assert!(
        divergent.is_empty(),
        "{} member(s) display differently from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

/// "Replace each `~…~` pair, left to right" — the table this codebase would plausibly have written
/// from what the syntax LOOKS like, and which is wrong on real input.
fn naive_pairwise_rewrite(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut opening = true;
    for ch in input.chars() {
        if ch == '~' {
            out.push(if opening { '<' } else { '>' });
            opening = !opening;
        } else {
            out.push(ch);
        }
    }
    out
}

/// THE FIXTURE HAS TO BE ABLE TO SAY NO. A gate never observed to fail is not evidence.
#[test]
fn the_fixture_rejects_a_naive_pair_substitution() {
    let rows = fixture();
    let caught: Vec<&Row> = rows
        .iter()
        .filter(|row| {
            let vis = &row.member[..1];
            format!("{vis}{}", naive_pairwise_rewrite(&row.id)) != row.display
        })
        .collect();
    assert!(
        caught.len() >= 3,
        "the naive substitution passed all but {} row(s) — this fixture cannot discriminate",
        caught.len()
    );
    // Name the rows that carry the discrimination, so a future edit that quietly drops them from
    // the battery shows up here as a failure and not as a still-green test.
    for member in ["+weird~ x", "+a~T~ b~U~", "+Pair~A, B, C~ p"] {
        assert!(
            caught.iter().any(|row| row.member == member),
            "{member:?} no longer distinguishes the naive substitution — is it still in the battery?"
        );
    }
}

/// Every `<text>` run in the document, with the XML escapes the renderer emits put back.
///
/// Reading the runs, not `svg.contains(..)`: the member text also reaches the accessibility
/// `<desc>`, so a bare substring check would pass while the class BOX still drew tildes.
fn text_runs(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        rest = &rest[open_end + 1..];
        let Some(close) = rest.find("</text>") else {
            break;
        };
        out.push(
            rest[..close]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&"),
        );
        rest = &rest[close + "</text>".len()..];
    }
    out
}

/// END TO END: the rewrite reaches the drawn text, for an attribute AND for a method return type.
#[test]
fn a_rendered_class_box_draws_angle_brackets_not_tildes() {
    let source = "classDiagram\nclass Box {\n  +List~int~ items\n  +Map~String, int~ lookup\n  +getItems() List~int~\n}\n";
    let ir = fm_parser::parse(source).ir;
    let svg = fm_render_svg::render_svg(&ir);
    let runs = text_runs(&svg);

    for expected in ["+List<int> items", "+Map<String, int> lookup"] {
        assert!(
            runs.iter().any(|run| run == expected),
            "no <text> run drew {expected:?}; runs were {runs:?}"
        );
    }
    // The method's RETURN TYPE is a separate field with its own call site; assert it by containment
    // because the ` : ` spacing around it is a different, still-open divergence.
    assert!(
        runs.iter().any(|run| run.starts_with("+getItems()") && run.ends_with("List<int>")),
        "the method return type still carries tildes; runs were {runs:?}"
    );
    assert!(
        !runs.iter().any(|run| run.contains('~')),
        "a tilde survived into the drawn text: {runs:?}"
    );
}
