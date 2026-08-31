//! A mermaid entity code decodes in EVERY label position, not just the ones someone reported.
//!
//! `#35;` is mermaid's documented escape for a character that would otherwise break the syntax, and
//! the pinned incumbent decodes it wherever text is drawn. This engine decoded it in 20 of 24 label
//! positions and left it raw in four.
//!
//! ORACLE — pinned mermaid-11.15.0.min.js
//! (sha256 `70137e77bb273bb2ef972b86e8b0400cca8be53cb25bfc45911a186dc98665de`), rendered in Chromium
//! over CDP and read back from the VISIBLE drawn text of each diagram:
//!
//! ```text
//!   flowchart node (control)   DECODED   "H#1 | H#1"
//!   pie slice label            DECODED   "100% | P#1"
//!   quadrant point             DECODED   "P#1 | L | H | L | H"
//!   timeline event             DECODED   "2024 | 2024 | E#1 | E#1 | T"
//!   treemap leaf               DECODED   "L#1 | 5"
//! ```
//!
//! ⚠️ THE EXTRACTION HAD TO INCLUDE `foreignObject`. Reading only `text`/`tspan` reported the
//! flowchart CONTROL as having no drawn text at all, because mermaid renders flowchart labels as
//! HTML inside a `foreignObject`. A sweep that had used the control to validate its own extractor
//! would have concluded the extractor was broken; one that skipped the control would have shipped a
//! selector that silently misses every flowchart label.
//!
//! ⚠️ THE PLANTED NEGATIVE IS THE SWEEP ITSELF. This is the third time this parser has been bitten by
//! one label helper being fixed while its forks were not — `clean_label` carries a comment about
//! exactly that (bd-j06n2), where edge labels and titles went through a second private function of
//! the same name. A fix aimed at the reported position passes a test written for that position and
//! leaves the rest raw. Only a test that walks EVERY label-bearing position can fail on the ones
//! nobody thought to report, which is why the table below is exhaustive rather than a sample.

use std::collections::BTreeMap;

/// Every label-bearing position reachable from the public parse API, each carrying `#35;`.
///
/// The token decodes to `#`, so `H#35;1` must become `H#1`. Both spellings are searched for, so a
/// position that drops the label entirely is reported as `ABSENT` rather than passing by silence.
fn cases() -> Vec<(&'static str, String)> {
    let arrow = "-->";
    vec![
        (
            "flowchart node label",
            "flowchart LR\n  A[\"H#35;1\"]\n".to_string(),
        ),
        (
            "flowchart edge label",
            format!("flowchart LR\n  A {arrow}|\"E#35;1\"| B\n"),
        ),
        (
            "subgraph title",
            "flowchart LR\n  subgraph \"S#35;1\"\n    A\n  end\n".to_string(),
        ),
        (
            "front-matter title",
            "---\ntitle: T#35;1\n---\nflowchart LR\n  A\n".to_string(),
        ),
        ("class name", "classDiagram\n  class C#35;1\n".to_string()),
        (
            "class member",
            "classDiagram\n  class C {\n    +m#35;1()\n  }\n".to_string(),
        ),
        (
            "sequence participant",
            "sequenceDiagram\n  participant P#35;1\n".to_string(),
        ),
        (
            "sequence message",
            "sequenceDiagram\n  A->>B: M#35;1\n".to_string(),
        ),
        (
            "sequence note",
            "sequenceDiagram\n  participant A\n  Note over A: N#35;1\n".to_string(),
        ),
        (
            "state description",
            "stateDiagram-v2\n  s1 : D#35;1\n".to_string(),
        ),
        ("pie slice label", "pie\n  \"P#35;1\" : 10\n".to_string()),
        (
            "gantt task",
            "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  G#35;1 :a1, 2024-01-01, 1d\n"
                .to_string(),
        ),
        (
            "gantt section",
            "gantt\n  dateFormat YYYY-MM-DD\n  section S#35;1\n  A :a1, 2024-01-01, 1d\n"
                .to_string(),
        ),
        (
            "journey task",
            "journey\n  title J\n  section S\n    T#35;1: 5: Me\n".to_string(),
        ),
        (
            "er entity",
            "erDiagram\n  E#35;1 ||--o{ B : has\n".to_string(),
        ),
        (
            "requirement text",
            "requirementDiagram\n  requirement R {\n  id: 1\n  text: X#35;1\n  risk: low\n  \
             verifymethod: test\n  }\n"
                .to_string(),
        ),
        (
            "quadrant point",
            format!(
                "quadrantChart\n  x-axis L {arrow} H\n  y-axis L {arrow} H\n  P#35;1: [0.3, 0.4]\n"
            ),
        ),
        (
            "timeline event",
            "timeline\n  title T\n  2024 : E#35;1\n".to_string(),
        ),
        (
            "mindmap node",
            "mindmap\n  root((R))\n    C#35;1\n".to_string(),
        ),
        ("sankey node", "sankey-beta\n\nA#35;1,B,5\n".to_string()),
        ("treemap leaf", "treemap-beta\n\"L#35;1\": 5\n".to_string()),
        (
            "xychart title",
            "xychart-beta\n  title \"X#35;1\"\n  x-axis [a]\n  y-axis \"r\" 0 --> 10\n  bar [5]\n"
                .to_string(),
        ),
        (
            "kanban item",
            "kanban\n  col1[Col]\n    t1[K#35;1]\n".to_string(),
        ),
        ("block label", "block-beta\n  A[\"B#35;1\"]\n".to_string()),
    ]
}

/// What the IR says happened to the entity code in one document.
///
/// Reads the IR's `Debug` rendering rather than named per-family fields: one probe then covers every
/// diagram type, and a NEW label position added to any family is swept automatically instead of
/// silently escaping a hand-written field list.
///
/// ⚠️ THE DECODED FORM WINS, AND THE ORDER MATTERS. An undecoded token may legitimately remain in the
/// document: pie derives a node ID from the raw slice name (`normalize_identifier(slice_name)`), and
/// an ID is NOT entity-decoded — `#35;` names a different node than `#`. Treating any `#35;`
/// anywhere in the dump as "raw" therefore reported pie and quadrant as broken when their LABELS
/// decoded correctly and only their IDs kept the token. The question this asks is whether the
/// decoded form reached the IR at all; `the_sweep_reports_an_undecoded_token_as_raw` covers the
/// opposite direction, so a position that never decodes still fails.
fn verdict(source: &str) -> &'static str {
    let ir = fm_parser::parse(source).ir;
    let dump = format!("{ir:?}");
    match (dump.contains("#1"), dump.contains("#35;")) {
        (true, _) => "DECODED",
        (false, true) => "RAW",
        (false, false) => "ABSENT",
    }
}

/// ⚠️ THE SWEEP: every label position decodes, and none loses the label.
///
/// Four positions failed this when it was written — pie slice, quadrant point, timeline event and
/// treemap leaf — against 20 that already passed. A per-position fix verified by a per-position test
/// would have closed one and left three.
#[test]
fn every_label_position_decodes_the_entity_code() {
    let mut failures: BTreeMap<&str, &str> = BTreeMap::new();
    for (name, source) in cases() {
        let got = verdict(&source);
        if got != "DECODED" {
            failures.insert(name, got);
        }
    }
    assert!(
        failures.is_empty(),
        "these label positions do not decode `#35;` the way the pinned mermaid-11.15.0 does \
         ({} of {} positions): {failures:?}. RAW = the token reached the IR undecoded; ABSENT = the \
         label did not reach the IR at all, which is a different defect.",
        failures.len(),
        cases().len()
    );
}

/// CONTROL: the sweep can actually SEE an undecoded token.
///
/// ⚠️ Without this, `verdict` returning "DECODED" for everything — a `Debug` rendering that omitted
/// label text, a `contains` typo — would make the sweep above pass while testing nothing. This feeds
/// a document whose text is NOT an entity code and requires the probe to report it as such.
#[test]
fn the_sweep_reports_an_undecoded_token_as_raw() {
    // `#zzz;` is not a mermaid entity, so it must survive into the IR verbatim and be seen.
    let ir = fm_parser::parse("flowchart LR\n  A[\"H#zzz;1\"]\n").ir;
    let dump = format!("{ir:?}");
    assert!(
        dump.contains("#zzz;"),
        "the IR Debug rendering does not carry node label text, so the sweep is blind: {dump:.200}"
    );
    // And the real token IS distinguishable from its decoded form.
    assert_eq!(verdict("flowchart LR\n  A[\"H#35;1\"]\n"), "DECODED");
    assert_eq!(
        verdict("flowchart LR\n  A[\"plain\"]\n"),
        "ABSENT",
        "a document with no entity code and no `#1` must report ABSENT, or the discriminator is \
         matching something other than the decoded token"
    );
}

/// The named and numeric spellings both decode, and a non-entity `#` is left alone.
///
/// ⚠️ A fix that decodes only `#35;` passes the sweep above — every case there uses that one token.
/// mermaid accepts named codes too, and must NOT eat a bare `#` that opens nothing.
#[test]
fn named_and_numeric_codes_decode_and_a_bare_hash_survives() {
    // ⚠️ THE EXPECTED TEXT IS WRITTEN AS `Debug` RENDERS IT, NOT AS THE READER SEES IT. `#quot;`
    // decodes to `"`, which `Debug` escapes to `\"` — so searching the dump for the three characters
    // `a"b` fails on CORRECT output. This is the same escaped-entity trap that makes naive markup
    // extraction report present text as missing; the decoder was right and the first version of this
    // assertion was wrong.
    for (source, expect) in [
        ("flowchart LR\n  A[\"a#35;b\"]\n", "a#b"),
        ("flowchart LR\n  A[\"a#quot;b\"]\n", "a\\\"b"),
        ("flowchart LR\n  A[\"a#59;b\"]\n", "a;b"),
        ("flowchart LR\n  A[\"a#amp;b\"]\n", "a&b"),
        ("flowchart LR\n  A[\"a#infin;b\"]\n", "a∞b"),
    ] {
        let dump = format!("{:?}", fm_parser::parse(source).ir);
        assert!(
            dump.contains(expect),
            "expected {expect:?} in the IR for {source:?}, got {dump:.200}"
        );
    }

    // A `#` that does not open an entity is ordinary text and must survive untouched.
    let dump = format!(
        "{:?}",
        fm_parser::parse("flowchart LR\n  A[\"C# and F#\"]\n").ir
    );
    assert!(
        dump.contains("C# and F#"),
        "a bare `#` was eaten by the entity decoder: {dump:.200}"
    );
}
