//! Differential test: the class-diagram METHOD row, against what mermaid-js ACTUALLY draws.
//!
//! `class_generics_differential.rs` is deliberately attribute-only, so its equality could be about
//! the `~T~` rewrite and nothing else. This is the other half. A method row adds two things to the
//! same string and they are separate contracts:
//!
//!   * THE RETURN-TYPE TAIL (bd-ci658). mermaid writes `+getName() : String`; every one of this
//!     workspace's five row builders wrote `+getName(): String`. One character, on every typed
//!     method row in every class diagram, and enough to fail an exact cross-engine text comparison
//!     while every token still agrees. This test pins it.
//!   * THE CLASSIFIER (bd-r2gll, CLOSED). mermaid does not put `$`/`*` in the text at all — it
//!     returns `text-decoration:underline;` / `font-style:italic;` in `cssStyle` beside it, because
//!     UML underlines a static member and italicises an abstract one. We appended the raw
//!     character and styled nothing. Both halves moved together, because deleting the character
//!     alone would have lost the static/abstract distinction entirely: the marker is now a style in
//!     every backend that can express one, and those four rows are IN the text comparison below
//!     rather than excluded from it. `the_classifier_is_a_style_not_a_character` checks the style
//!     half and, as its negative, that the character is gone from the drawn runs.
//!
//! THE ORACLE is `tests/fixtures/mermaid_class_methods.tsv`, produced by
//! `scripts/headtohead/class_method_battery.mjs` from the pinned 11.15.0 bundle.
//!
//! ONE MEASURED DIVERGENCE IS TAKEN DELIBERATELY, and it is in the fixture rather than hidden: when
//! the author writes their own colon (`+getName(): String`), mermaid captures it INTO the return
//! type and then draws a second one — `+getName() : : String`. We draw one. Reproducing a double
//! colon would be matching the incumbent into visibly broken output, so those two rows are
//! allowlisted below, and the allowlist ASSERTS each entry still diverges: an entry naming a case
//! that now matches is a permanent hole, not a passing test.

use std::{collections::BTreeSet, fs, path::Path};

struct Row {
    member: String,
    /// mermaid's `$`/`*`, empty when the method has none.
    classifier: String,
    /// mermaid's own `getDisplayDetails().displayText`.
    display: String,
    /// The `cssStyle` it returns beside that text.
    css: String,
}

/// Rows where we knowingly differ, each with the reason. Asserted to still differ.
const DELIBERATE: [&str; 2] = ["+getName() : String", "+getName(): String"];

fn fixture() -> Vec<Row> {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/mermaid_class_methods.tsv");
    let text = fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("fixture {} unreadable: {err}", path.display()));
    let rows: Vec<Row> = text
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut columns = line.split('\t');
            let member = columns.next().expect("member column").to_string();
            let classifier = columns.next().expect("classifier column").to_string();
            let display = columns.next().expect("display column").to_string();
            let css = columns.next().unwrap_or("").to_string();
            Row {
                member,
                classifier,
                display,
                css,
            }
        })
        .collect();
    assert!(rows.len() >= 15, "fixture holds only {} rows", rows.len());
    assert!(
        rows.iter()
            .filter(|row| row.display.contains(" : "))
            .count()
            >= 8,
        "fixture has no spaced return-type rows — it cannot pin bd-ci658"
    );
    assert!(
        rows.iter().filter(|row| !row.classifier.is_empty()).count() >= 4,
        "fixture lost its classifier rows — bd-r2gll's evidence would have to be regenerated"
    );
    rows
}

/// Leaf text of every `<text>` in the document, XML escapes undone.
///
/// Leaf segments only: a multi-line label is one `<text>` holding a `<tspan>` per line, so taking
/// inner markup whole would compare a description against a string full of `<tspan …>`.
fn text_runs(svg: &str) -> Vec<String> {
    let mut runs = Vec::new();
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
        let mut cursor = &rest[..close];
        while let Some(open) = cursor.find('<') {
            let leaf = &cursor[..open];
            if !leaf.is_empty() {
                runs.push(leaf.to_string());
            }
            cursor = &cursor[open..];
            let Some(tag_end) = cursor.find('>') else {
                break;
            };
            cursor = &cursor[tag_end + 1..];
        }
        if !cursor.is_empty() {
            runs.push(cursor.to_string());
        }
        rest = &rest[close + "</text>".len()..];
    }
    runs.into_iter()
        .map(|run| {
            run.replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&amp;", "&")
        })
        .filter(|run| !run.trim().is_empty())
        .collect()
}

/// The one method row this class draws, as the reader sees it.
fn drawn_row(member: &str) -> Vec<String> {
    let source = format!("classDiagram\nclass Foo {{\n{member}\n}}\n");
    let ir = fm_parser::parse(&source).ir;
    let svg = fm_render_svg::render_svg(&ir);
    // Everything except the class heading, which is the only other run a one-member class draws.
    text_runs(&svg)
        .into_iter()
        .filter(|run| run != "Foo")
        .collect()
}

#[test]
fn every_method_row_draws_what_mermaid_draws() {
    let mut divergent = Vec::new();
    let mut compared = 0;
    for row in fixture() {
        // Classifier rows are compared here TOO now (bd-r2gll). They used to be skipped because we
        // drew the raw `$`/`*` and mermaid never does; the display text agrees now that the marker
        // became a style, and `the_classifier_is_a_style_not_a_character` below checks the style
        // half. Leaving them skipped would assert the fix only from its own new test.
        if DELIBERATE.contains(&row.member.as_str()) {
            continue;
        }
        compared += 1;
        let drawn = drawn_row(&row.member);
        if drawn.as_slice() != [row.display.as_str()] {
            divergent.push(format!(
                "{:?}: ours {drawn:?}, mermaid [{:?}]",
                row.member, row.display
            ));
        }
    }
    // WORK PROOF: the two skip conditions above could between them empty this loop.
    assert!(
        compared >= 10,
        "only {compared} rows were actually compared"
    );
    assert!(
        divergent.is_empty(),
        "{} method row(s) diverge from mermaid 11.15.0:\n  {}",
        divergent.len(),
        divergent.join("\n  ")
    );
}

/// AN ALLOWLIST ENTRY NAMING A CASE THAT NO LONGER DIVERGES IS A PERMANENT HOLE.
#[test]
fn every_deliberate_divergence_is_still_a_divergence() {
    let rows = fixture();
    let members: BTreeSet<&str> = rows.iter().map(|row| row.member.as_str()).collect();
    for member in DELIBERATE {
        assert!(
            members.contains(member),
            "{member:?} is allowlisted but is not in the fixture at all"
        );
        let row = rows
            .iter()
            .find(|row| row.member == member)
            .expect("checked above");
        let drawn = drawn_row(member);
        assert_ne!(
            drawn.as_slice(),
            [row.display.as_str()],
            "{member:?} now matches mermaid — drop it from DELIBERATE instead of carrying a dead entry"
        );
        // And say what we DO draw, so the allowlist documents a decision rather than a mystery.
        assert_eq!(
            drawn,
            ["+getName() : String".to_string()],
            "{member:?}: we should draw one colon where mermaid draws two"
        );
    }
}

/// THE FIXTURE HAS TO BE ABLE TO SAY NO. `": "` — the tail every row builder used to write — must
/// contradict the oracle, or this test cannot tell the fix from the defect.
#[test]
fn the_fixture_rejects_the_unspaced_return_type_tail() {
    let caught = fixture()
        .into_iter()
        .filter(|row| row.classifier.is_empty() && !DELIBERATE.contains(&row.member.as_str()))
        .filter(|row| row.display.replace(" : ", ": ") != row.display)
        .count();
    assert!(
        caught >= 8,
        "the unspaced tail contradicts only {caught} row(s) — this fixture cannot discriminate"
    );
}

/// THE CLASSIFIER IS STYLE, NOT TEXT — the measurement bd-r2gll rests on, kept executable so the
/// bead cannot rot against a bundle change.
#[test]
fn mermaid_carries_the_classifier_in_css_and_not_in_the_text() {
    let mut seen = 0;
    for row in fixture()
        .into_iter()
        .filter(|row| !row.classifier.is_empty())
    {
        seen += 1;
        assert!(
            !row.display.contains(&row.classifier),
            "{:?}: mermaid put {:?} in the DISPLAY TEXT after all",
            row.member,
            row.classifier
        );
        let expected = if row.classifier == "$" {
            "text-decoration:underline;"
        } else {
            "font-style:italic;"
        };
        assert_eq!(
            row.css, expected,
            "{:?}: unexpected classifier style",
            row.member
        );
    }
    assert!(seen >= 4, "only {seen} classifier rows were checked");
}

/// The classifier reaches the SVG as a STYLE, and never as a character.
///
/// REFERENCE, from `mermaid_class_methods.tsv`, which `class_method_battery.mjs` generated by
/// asking the pinned 11.15.0 bundle's own `ClassMember.getDisplayDetails()`:
///
/// ```text
///   +getName()$ String   ->  display `+getName() : String`   css `text-decoration:underline;`
///   +getName()* String   ->  display `+getName() : String`   css `font-style:italic;`
/// ```
///
/// UML underlines a static member and italicises an abstract one; mermaid follows that, and its
/// display text contains no marker at all. We appended the raw byte and styled nothing, so the same
/// method read as a different NAME.
///
/// The fixture's `cssStyle` is CSS because mermaid writes a `style=` attribute; this renderer uses
/// SVG presentation attributes for class rows — the stereotype line already emits
/// `font-style="italic"` that way — so the assertion is on the equivalent attribute. The PROPERTY
/// and its VALUE are the contract; the spelling of the carrier is not.
#[test]
fn the_classifier_is_a_style_not_a_character() {
    let mut checked = 0;
    for row in fixture() {
        if row.classifier.is_empty() {
            continue;
        }
        checked += 1;
        let source = format!("classDiagram\nclass Foo {{\n{}\n}}\n", row.member);
        let svg = fm_render_svg::render_svg(&fm_parser::parse(&source).ir);

        let (property, value) = match row.classifier.as_str() {
            "$" => ("text-decoration", "underline"),
            "*" => ("font-style", "italic"),
            other => panic!("unknown classifier {other:?} in the fixture"),
        };
        // The fixture is the oracle for the mapping itself, not just for the display text: if
        // mermaid ever moved static to italic this would fail before the render assertion does.
        assert!(
            row.css.contains(&format!("{property}:{value}")),
            "fixture disagrees: mermaid's css for {:?} is {:?}, not {property}:{value}",
            row.member,
            row.css
        );
        assert!(
            svg.contains(&format!("{property}=\"{value}\"")),
            "{:?} is {} in mermaid, but the SVG carries no {property}",
            row.member,
            if row.classifier == "$" {
                "static"
            } else {
                "abstract"
            },
        );

        // ⚠️ THE NEGATIVE HALF. Emitting the style while ALSO keeping the character satisfies every
        // assertion above and still draws the wrong name — the present defect, one step improved.
        // mermaid's display text has no marker in it at all.
        //
        // Scoped to the DRAWN RUNS, not to the document: `*` appears in the embedded stylesheet, so
        // `svg.contains("*")` is true of every diagram ever rendered and would fail this test on
        // correct output. A whole-document substring search answering a question about drawn text
        // is the same trap as grepping a report for a severity word that its own header contains.
        let drawn = drawn_row(&row.member);
        assert!(
            !drawn
                .iter()
                .any(|run| run.contains(row.classifier.as_str())),
            "{:?} still draws the literal {:?} alongside the style: {drawn:?}",
            row.member,
            row.classifier
        );
    }
    assert!(
        checked >= 4,
        "only {checked} classifier rows were checked; the fixture lost its evidence"
    );
}
