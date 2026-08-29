//! No degenerate diagram may put a non-finite number into the document.
//!
//! A `NaN` or `Infinity` in a coordinate does not throw and does not look wrong in a byte diff — it
//! makes the element silently undrawable in every SVG renderer. The inputs that produce one are
//! never the ordinary ones: they are the shapes where a denominator goes to zero, and each of the
//! families below has such a shape.
//!
//! ```text
//!   pie      every slice 0        percentage = value / total          -> 0/0
//!   xychart  y-axis 0 --> 0       scale      = height / (max - min)   -> x/0
//!   sankey   the only flow is 0   width      = value / max flow       -> 0/0
//!   radar    every curve value 0  radius     = value / max value      -> 0/0
//!   gantt    a 0d task            width      = span / duration        -> x/0
//!   treemap  every leaf 0         area share = value / total          -> 0/0
//! ```
//!
//! All of them are clean today; nothing was guarding that. This file is the guard. The corpus IS the
//! planted negative: a renderer that computes any of those ratios the obvious way, without a
//! zero-denominator branch, emits `NaN` here and passes every test built from ordinary diagrams.
//!
//! ⚠️ WHERE A `NaN` BECOMES VISIBLE DEPENDS ON WHICH WRITER IT REACHES, AND THE TWO DISAGREE.
//! This was read off the implementations, not assumed, and it decides what the corpus scan below can
//! and cannot prove:
//!
//! ```text
//!   numeric attribute   attributes::write_number_into -> write_fixed2 -> `{:.2}`   emits "NaN"
//!   path `d=` data      path::FmtNum::write_into      -> early return             emits "0"
//!   transform()         transform::fmt_num            -> early return             emits "0"
//! ```
//!
//! So a non-finite ATTRIBUTE surfaces in the document and the corpus scan catches it, while a
//! non-finite PATH COORDINATE is silently coerced to the origin and no scan of the output can see it
//! at all. That coercion is a deliberate choice with a real cost — a geometry bug draws a line to
//! `0,0` instead of failing visibly — so it is pinned directly in
//! `the_number_writers_handle_non_finite_as_documented` rather than left to the corpus, which cannot
//! reach it. Do not "strengthen" the path-data loop below into the primary guard: it is a check on
//! the tokeniser and on the coercion staying in place, not a finiteness proof.
//!
//! ⚠️ THE SCAN MUST NOT BE NAIVE, AND THE NAIVE VERSIONS BOTH FAIL LOUDLY IN THE WRONG DIRECTION.
//! Two false-positive traps were hit while establishing this invariant, and both are encoded below
//! rather than left for the next reader to rediscover:
//!
//! * A whole-document regex for `e[+-]?\d+` matches the hex colour `#1e293b`. Only NUMERIC
//!   attributes and path data are scanned here, never the raw document.
//! * Path data glues command letters to numbers — `M0 8L0 2`, `390C390Z`. Splitting on whitespace
//!   yields tokens like `"M0"`, whose numeric parse fails, reporting a defect in every diagram that
//!   draws a path. Numbers are extracted by pattern, and the command letters skipped.

/// Attributes whose value is a bare number in the documents this renderer emits.
const NUMERIC_ATTRS: &[&str] = &[
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "width",
    "height",
    "font-size",
    "stroke-width",
];

/// Every numeric attribute value in the document, as `(attribute, raw text)`.
fn numeric_attribute_values(svg: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for attr in NUMERIC_ATTRS {
        let needle = format!(" {attr}=\"");
        let mut rest = svg;
        while let Some(start) = rest.find(&needle) {
            let value_start = start + needle.len();
            let Some(end) = rest[value_start..].find('"') else {
                break;
            };
            let raw = &rest[value_start..value_start + end];
            out.push(((*attr).to_string(), raw.to_string()));
            rest = &rest[value_start + end..];
        }
    }
    out
}

/// Whether a value is a legitimate unit-suffixed SVG length rather than a bare number.
///
/// `width="100%"` on a backdrop and `x1="0.0%"` on a gradient are valid and are not what this file
/// tests. They are skipped rather than treated as malformed — but ONLY these forms, so a genuinely
/// broken value still fails the caller's assertion.
fn is_unit_suffixed(raw: &str) -> bool {
    for suffix in ["%", "px", "em", "rem", "pt"] {
        if let Some(head) = raw.strip_suffix(suffix)
            && !head.is_empty()
            && head.parse::<f64>().is_ok_and(f64::is_finite)
        {
            return true;
        }
    }
    false
}

/// Every NUMBER appearing in path `d=` data, with command letters skipped.
///
/// ⚠️ Not a whitespace split. `M0 8L0 2` tokenises to `M0`, `8L0`, `2` that way, and the first has
/// no numeric parse — which reads as a defect in every path the renderer draws.
fn path_data_numbers(svg: &str) -> Vec<String> {
    let mut out = Vec::new();
    let needle = " d=\"";
    let mut rest = svg;
    while let Some(start) = rest.find(needle) {
        let value_start = start + needle.len();
        let Some(end) = rest[value_start..].find('"') else {
            break;
        };
        let data = &rest[value_start..value_start + end];

        let mut current = String::new();
        for ch in data.chars() {
            if ch.is_ascii_digit() || ch == '.' || ch == '-' || ch == '+' || ch == 'e' || ch == 'E' {
                // `e`/`E` only continue a number already in progress; a bare `E` is a path command.
                if (ch == 'e' || ch == 'E') && current.is_empty() {
                    continue;
                }
                // A sign only starts a number, or follows an exponent marker.
                if (ch == '-' || ch == '+')
                    && !current.is_empty()
                    && !current.ends_with('e')
                    && !current.ends_with('E')
                {
                    // `current` is provably non-empty here: the guard above requires it.
                    out.push(std::mem::take(&mut current));
                }
                current.push(ch);
            } else if !current.is_empty() {
                out.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            out.push(current);
        }
        rest = &rest[value_start + end..];
    }
    out
}

/// The degenerate corpus: one zero-denominator shape per chart family, plus structural extremes.
fn degenerate_sources() -> Vec<(&'static str, String)> {
    let arrow = "-->";
    let mut cases: Vec<(&'static str, String)> = vec![
        ("pie every slice zero", "pie showData\n  \"A\" : 0\n  \"B\" : 0\n".to_string()),
        ("pie single slice", "pie showData\n  \"Only\" : 5\n".to_string()),
        (
            "xychart flat axis",
            format!("xychart-beta\n  x-axis [a, b]\n  y-axis \"R\" 0 {arrow} 0\n  bar [0, 0]\n"),
        ),
        (
            "xychart one point",
            format!("xychart-beta\n  x-axis [a]\n  y-axis \"R\" 0 {arrow} 10\n  bar [5]\n"),
        ),
        ("sankey zero flow", "sankey-beta\n\nA,B,0\n".to_string()),
        ("sankey huge flow", "sankey-beta\n\nA,B,1e30\n".to_string()),
        ("radar all zero", "radar-beta\n  axis a, b, c\n  curve x{0,0,0}\n".to_string()),
        ("treemap all zero", "treemap-beta\n\"A\": 0\n\"B\": 0\n".to_string()),
        (
            "gantt zero duration",
            "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  A :a1, 2024-01-01, 0d\n".to_string(),
        ),
        (
            "quadrant at origin",
            format!("quadrantChart\n  x-axis L {arrow} H\n  y-axis L {arrow} H\n  P: [0, 0]\n"),
        ),
        ("empty flowchart", "flowchart LR\n".to_string()),
        ("single node", "flowchart LR\n  A\n".to_string()),
        ("self loop only", format!("flowchart LR\n  A {arrow} A\n")),
        ("very long label", format!("flowchart LR\n  A[\"{}\"]\n", "W".repeat(4000))),
    ];

    // Deep nesting: a structural extreme rather than a numeric one, but the same class of input.
    let mut nested = String::from("flowchart LR\n");
    for depth in 0..12 {
        nested.push_str(&format!("{}subgraph s{depth}\n", "  ".repeat(depth + 1)));
    }
    nested.push_str("      A\n");
    for _ in 0..12 {
        nested.push_str("  end\n");
    }
    cases.push(("twelve-deep subgraphs", nested));
    cases
}

/// ⚠️ THE PLANTED NEGATIVE IS THE CORPUS: every one of these is a zero denominator somewhere.
///
/// A renderer computing `value / total`, `height / (max - min)` or `value / max_flow` the obvious way
/// emits `NaN` for these inputs and passes every suite built from ordinary diagrams, because an
/// ordinary diagram never has a zero total. `NaN` does not throw and does not look wrong in a diff;
/// it makes the element silently undrawable.
#[test]
fn no_degenerate_input_emits_a_non_finite_coordinate() {
    for (case, source) in degenerate_sources() {
        let ir = fm_parser::parse(&source).ir;
        let svg = fm_render_svg::render_svg(&ir);

        for (attr, raw) in numeric_attribute_values(&svg) {
            // A UNIT-SUFFIXED value is legitimate SVG and not what this test is about: a full-width
            // backdrop is `width="100%"` and a gradient stop is `x1="0.0%"`. Skipping them is safe
            // for the invariant because `"NaN"`, `"inf"` and `"Infinity"` all PARSE successfully in
            // Rust and are therefore caught by the finiteness check below, never by the parse.
            let Ok(value) = raw.parse::<f64>() else {
                assert!(
                    is_unit_suffixed(&raw),
                    "{case}: attribute {attr}=\"{raw}\" is neither a number nor a unit value"
                );
                continue;
            };
            assert!(
                value.is_finite(),
                "{case}: attribute {attr}=\"{raw}\" is not finite — the element is undrawable"
            );
        }

        for token in path_data_numbers(&svg) {
            let value: f64 = token.parse().unwrap_or_else(|_| {
                panic!("{case}: path data holds {token:?}, which is not a number")
            });
            assert!(
                value.is_finite(),
                "{case}: path data holds a non-finite number {token:?}"
            );
        }
    }
}

/// ⚠️ THE MECHANISM, ASSERTED DIRECTLY: the two number writers handle non-finite input differently.
///
/// The corpus test above cannot reach this. Its path-data loop is structurally incapable of failing
/// on a `NaN` coordinate, because `FmtNum::write_into` returns `"0"` before the number is ever
/// formatted — a geometry `NaN` reaches the document as a coordinate at the origin, not as text a
/// scan can find. An assertion that cannot fail is worse than no assertion, so the contract is
/// pinned here at the writers instead, where it IS decidable.
///
/// Both halves are planted negatives:
///
/// * Delete the `!n.is_finite()` early return in `path.rs` — a plausible "the formatter handles it"
///   simplification, and it does, by writing the four characters `NaN` into `d=` where they are not
///   a number. Nothing else in the suite fails; this does.
/// * Add a matching early return to `write_number_into` so attributes coerce too, and the corpus
///   test above goes permanently green for the wrong reason — every future zero-denominator bug
///   becomes invisible instead of caught. This half fails on that change and says why.
#[test]
fn the_number_writers_handle_non_finite_as_documented() {
    for (case, value) in [
        ("NaN", f32::NAN),
        ("+inf", f32::INFINITY),
        ("-inf", f32::NEG_INFINITY),
    ] {
        // PATH DATA: coerced to the origin, so the bad number never reaches the document.
        let path = fm_render_svg::PathBuilder::new()
            .move_to(value, 1.0)
            .line_to(2.0, value)
            .build();
        assert!(
            !path.contains("NaN") && !path.contains("inf"),
            "{case}: the path writer stopped coercing non-finite coordinates and wrote {path:?} \
             into `d=`, which is not valid path data"
        );
        assert!(
            path.contains('0'),
            "{case}: the non-finite coordinate did not become the documented `0`: {path:?}"
        );

        // NUMERIC ATTRIBUTE: NOT coerced — it surfaces, which is what makes the corpus scan above
        // able to catch a zero-denominator defect at all.
        let attr = fm_render_svg::AttributeValue::Number(value).to_string();
        assert!(
            attr.contains("NaN") || attr.contains("inf"),
            "{case}: the attribute writer began coercing non-finite values to {attr:?}. That silences \
             the degenerate-input scan in this file — every future zero-denominator bug now renders \
             as a plausible number instead of failing. If this is intended, the corpus test above is \
             no longer a guard and must be rewritten, not left passing."
        );
    }

    // CONTROL: an ordinary value is untouched by either writer, so the assertions above are about
    // non-finite handling and not about the writers being broken in general.
    assert_eq!(
        fm_render_svg::AttributeValue::Number(12.5).to_string(),
        "12.50"
    );
    assert!(
        fm_render_svg::PathBuilder::new()
            .move_to(3.0, 4.0)
            .build()
            .contains('3')
    );
}

/// The emitted numbers carry no exponent notation.
///
/// SVG's grammar permits `1e30`, so this is an OUTPUT-STABILITY property rather than a validity one:
/// the number writer produces plain decimal at every magnitude, and `sankey-beta A,B,1e30` — a value
/// that would reach exponent form through most float formatters — is in the corpus precisely to
/// exercise that. A change that starts emitting exponents is a silent change to the document format
/// that byte-comparison goldens would flag as noise across every diagram at once.
#[test]
fn emitted_numbers_use_plain_decimal_at_every_magnitude() {
    for (case, source) in degenerate_sources() {
        let ir = fm_parser::parse(&source).ir;
        let svg = fm_render_svg::render_svg(&ir);

        for (attr, raw) in numeric_attribute_values(&svg) {
            if is_unit_suffixed(&raw) {
                continue;
            }
            assert!(
                !raw.contains('e') && !raw.contains('E'),
                "{case}: attribute {attr}=\"{raw}\" uses exponent notation"
            );
        }
        for token in path_data_numbers(&svg) {
            assert!(
                !token.contains('e') && !token.contains('E'),
                "{case}: path data number {token:?} uses exponent notation"
            );
        }
    }
}

/// CONTROL: the scan actually finds numbers, so the assertions above are not vacuous.
///
/// ⚠️ Both extractors above replaced a naive version that reported false defects — a document-wide
/// exponent regex matches the hex colour `#1e293b`, and a whitespace split of `M0 8L0 2` yields
/// `"M0"`. The opposite failure is just as easy: an extractor that finds NOTHING makes every
/// assertion above pass on an empty set. This asserts a real floor on both.
#[test]
fn the_extractors_are_not_vacuous() {
    let ir = fm_parser::parse("flowchart LR\n  A[\"one\"] --> B[\"two\"]\n").ir;
    let svg = fm_render_svg::render_svg(&ir);

    let attrs = numeric_attribute_values(&svg);
    assert!(
        attrs.len() >= 8,
        "the attribute scan found only {} numeric attributes in a two-node flowchart, so the \
         finiteness assertions would be nearly vacuous",
        attrs.len()
    );
    assert!(
        attrs
            .iter()
            .all(|(_, raw)| raw.parse::<f64>().is_ok() || is_unit_suffixed(raw)),
        "the attribute scan picked up a value that is neither a number nor a unit value: {:?}",
        attrs
            .iter()
            .find(|(_, raw)| raw.parse::<f64>().is_err() && !is_unit_suffixed(raw))
            .map(|(attr, raw)| format!("{attr}={raw}"))
    );
    assert!(
        attrs
            .iter()
            .filter(|(_, raw)| raw.parse::<f64>().is_ok())
            .count()
            >= 8,
        "after skipping unit values the scan has too few BARE numbers left to be meaningful"
    );

    let numbers = path_data_numbers(&svg);
    assert!(
        !numbers.is_empty(),
        "the path scan found no numbers in a diagram that draws an edge path"
    );
    assert!(
        numbers.iter().all(|token| token.parse::<f64>().is_ok()),
        "the path tokeniser produced a non-number — it is splitting command letters into the \
         numbers again: {:?}",
        numbers.iter().find(|token| token.parse::<f64>().is_err())
    );
}
