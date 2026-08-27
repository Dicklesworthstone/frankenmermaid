//! `accTitle:` / `accDescr:` are honoured everywhere and must not be called unsupported (bd-xym5x).
//!
//! THE DEFECT. Nine of the twenty-one diagram families this parser supports emitted
//! `unsupported <family> syntax: accTitle: …` for a directive they were, at the same moment,
//! HONOURING — `extract_accessibility_directives` runs before every family's line loop and had
//! already put the value in `ir.meta`, so the title and description reached `<title>`/`<desc>` in
//! all twenty-one. The warning sent an author to fix a line that was already correct, which is the
//! warning-channel failure bd-xfmm named: readers who are told a working line is broken learn to
//! ignore the channel.
//!
//! Measured across all twenty-one families through the shipped wasm bundle: `title/desc` carried in
//! 21 of 21, warned about in NINE — sequence, requirement, sankey, gitGraph, xychart, C4, treemap,
//! radar and packet-beta. Two of the nine (treemap, radar) were written days earlier in this same
//! worklist, with their own wording for the same mistake.
//!
//! REFERENCE, measured in Chromium 151 against the pinned mermaid 11.15.0 bundle, with a nonsense
//! line as the control that proves the probe can see a rejection at all:
//!
//! ```text
//!   sequence requirement gitGraph xychart C4 treemap radar   parse, and render the value
//!   packet-beta                                              parses, and renders the value
//!   flowchart (control, we never warned)                     parses, renders the value
//!   sankey                                                   PARSE ERROR on the accTitle line
//!   zzTotallyBogus: nope (control)                           PARSE ERROR
//! ```
//!
//! ⚠️ SANKEY IS A DELIBERATE DIVERGENCE, NOT AN OVERSIGHT. mermaid's sankey grammar rejects the
//! directive outright. We accept it, because this parser is best-effort and the value is already
//! extracted — refusing the line would mean discarding an accessibility title we are holding. What
//! is not defensible is honouring it while calling it unsupported.
//!
//! ⚠️ THE NEGATIVE CASE. Silencing a warning is a one-line edit that could as easily swallow the
//! line before its value is read, or swallow real syntax alongside it — trading a false warning for
//! a silent drop, which is worse. So four things are asserted together: the value ARRIVES, an
//! unrecognised line in each fixed family STILL warns, a name that merely starts with `accTitle`
//! still warns, and each family's own syntax still parses.

use fm_core::DiagramType;

const ACC: &str = "  accTitle: My Title\n  accDescr: My Description\n";

/// The nine families that warned, plus their diagram type and one line of their own syntax.
///
/// The last field is a line that is genuinely NOT valid in that family — the negative control the
/// silencing must not swallow.
fn cases() -> Vec<(&'static str, DiagramType, String, &'static str)> {
    vec![
        (
            "sequence",
            DiagramType::Sequence,
            format!("sequenceDiagram\n{ACC}  Alice->>Bob: hi\n"),
            "zzBogusStatement here",
        ),
        (
            "requirement",
            DiagramType::Requirement,
            format!(
                "requirementDiagram\n{ACC}  requirement r {{\n  id: 1\n  text: t\n  risk: low\n  verifymethod: test\n  }}\n"
            ),
            "zzBogusStatement here",
        ),
        (
            "sankey",
            DiagramType::Sankey,
            format!("sankey-beta\n{ACC}  a,b,1\n"),
            "zzBogusStatement here",
        ),
        (
            "gitGraph",
            DiagramType::GitGraph,
            format!("gitGraph\n{ACC}  commit\n"),
            "zzBogusStatement here",
        ),
        (
            "xychart",
            DiagramType::XyChart,
            format!("xychart-beta\n  title T\n{ACC}  bar [1,2,3]\n"),
            "zzBogusStatement here",
        ),
        (
            "C4",
            DiagramType::C4Context,
            format!("C4Context\n  title T\n{ACC}  Person(a, \"A\")\n"),
            "zzBogusStatement here",
        ),
        (
            "treemap",
            DiagramType::Treemap,
            format!("treemap\n{ACC}\"R\"\n    \"a\": 10\n"),
            "zzBogusStatement here",
        ),
        (
            "radar",
            DiagramType::Radar,
            format!("radar-beta\n{ACC}  axis a, b, c\n  curve x{{1,2,3}}\n"),
            "zzBogusStatement here",
        ),
        // The ninth, found by the differential rather than by the wording sweep.
        (
            "packet",
            DiagramType::PacketBeta,
            format!("packet-beta\n{ACC}  0-7: \"a\"\n"),
            "zzBogus: not-a-range",
        ),
    ]
}

/// ⚠️ THE DIFFERENTIAL, OVER EVERY FAMILY: adding the two lines must add no warning.
///
/// ⚠️ AND IT IS WORDING-INDEPENDENT BECAUSE THE WORDING SWEEP UNDERCOUNTED. The discovery pass
/// filtered each family's warnings for the strings `accTitle`/`accDescr` and found eight. It missed
/// `packet-beta`, whose message is `packet field range "accTitle" is not a bit range` — the
/// directive reaches packet's `split_once(':')` field parser, so the complaint is about a RANGE and
/// a probe keyed on the directive's own name cannot see it. There were nine.
///
/// This compares the warnings a source produces WITH the directives against the same source
/// WITHOUT them. Any warning that appears only in the first is caused by the pair, whatever it says
/// — which is the only form of this check that cannot be defeated by a message that does not quote
/// the line it is about.
#[test]
fn adding_the_directives_adds_no_warning_in_any_family() {
    let mut checked = 0;
    for (name, with_acc, without_acc) in every_family() {
        let noisy = fm_parser::parse(&with_acc).warnings;
        let quiet = fm_parser::parse(&without_acc).warnings;
        let added: Vec<&String> = noisy.iter().filter(|w| !quiet.contains(w)).collect();
        assert!(
            added.is_empty(),
            "{name}: the accessibility directives introduced {} warning(s): {added:?}",
            added.len()
        );
        checked += 1;
    }
    assert_eq!(checked, 21, "the family table lost an entry");
}

/// Every family this parser supports, as (name, source with the directives, source without).
///
/// ⚠️ BOTH HALVES ARE SPELLED OUT RATHER THAN DERIVED BY DELETING LINES. A helper that stripped the
/// pair from the first source would make the two arms differ by construction and could not be wrong
/// in an interesting way; written out, a fixture whose two halves are not otherwise identical is a
/// fixture bug this test can actually surface.
fn every_family() -> Vec<(&'static str, String, String)> {
    let pairs: [(&str, &str, &str); 21] = [
        ("flowchart", "flowchart LR\n", "  A --> B\n"),
        ("sequence", "sequenceDiagram\n", "  Alice->>Bob: hi\n"),
        ("class", "classDiagram\n", "  class Animal\n"),
        ("state", "stateDiagram-v2\n", "  [*] --> Idle\n"),
        ("er", "erDiagram\n", "  A ||--o{ B : r\n"),
        (
            "gantt",
            "gantt\n",
            "  dateFormat YYYY-MM-DD\n  section S\n  T :a1, 2024-01-01, 30d\n",
        ),
        ("pie", "pie title V\n", "  \"A\" : 40\n"),
        (
            "journey",
            "journey\n  title D\n",
            "  section M\n    Wake: 3: Me\n",
        ),
        ("mindmap", "mindmap\n", "  root((r))\n    a\n"),
        ("timeline", "timeline\n  title T\n", "  2024 : x\n"),
        (
            "requirement",
            "requirementDiagram\n",
            "  requirement r {\n  id: 1\n  text: t\n  risk: low\n  verifymethod: test\n  }\n",
        ),
        (
            "quadrant",
            "quadrantChart\n  title T\n",
            "  x-axis A --> B\n",
        ),
        ("sankey", "sankey-beta\n", "  a,b,1\n"),
        ("gitGraph", "gitGraph\n", "  commit\n"),
        ("kanban", "kanban\n", "  col1\n    task\n"),
        ("xychart", "xychart-beta\n  title T\n", "  bar [1,2,3]\n"),
        ("block", "block-beta\n", "  columns 1\n  a\n"),
        ("packet", "packet-beta\n", "  0-7: \"a\"\n"),
        ("c4", "C4Context\n  title T\n", "  Person(a, \"A\")\n"),
        ("treemap", "treemap\n", "\"R\"\n    \"a\": 10\n"),
        (
            "radar",
            "radar-beta\n",
            "  axis a, b, c\n  curve x{1,2,3}\n",
        ),
    ];
    pairs
        .into_iter()
        .map(|(name, header, body)| {
            (
                name,
                format!("{header}{ACC}{body}"),
                format!("{header}{body}"),
            )
        })
        .collect()
}

/// Every family carries the values, not just the nine that used to warn.
#[test]
fn every_family_carries_the_accessibility_values() {
    for (name, with_acc, _) in every_family() {
        let parsed = fm_parser::parse(&with_acc);
        assert_eq!(
            parsed.ir.meta.acc_title.as_deref(),
            Some("My Title"),
            "{name} dropped the accessible title"
        );
        assert_eq!(
            parsed.ir.meta.acc_descr.as_deref(),
            Some("My Description"),
            "{name} dropped the accessible description"
        );
    }
}

/// ⚠️ AND THE VALUE STILL ARRIVES — the half that separates a fix from a silencing.
///
/// Skipping the line one step earlier, before `extract_accessibility_directives` reads it, would
/// pass the test above and lose the accessible title. That is the "trade a wrong answer for a
/// silent one" failure, and it is invisible to anything that only counts warnings.
#[test]
fn the_accessibility_values_still_reach_the_ir() {
    for (name, expected_type, source, _) in cases() {
        let parsed = fm_parser::parse(&source);
        assert_eq!(
            parsed.ir.diagram_type, expected_type,
            "{name} was detected as the wrong family"
        );
        assert_eq!(
            parsed.ir.meta.acc_title.as_deref(),
            Some("My Title"),
            "{name} silenced the directive and dropped its title"
        );
        assert_eq!(
            parsed.ir.meta.acc_descr.as_deref(),
            Some("My Description"),
            "{name} silenced the directive and dropped its description"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL, IN THE TEST ITSELF: an unrecognised line still warns.
///
/// The cheap version of this fix is a skip broad enough to swallow anything the family does not
/// recognise, which turns every real syntax error in eight families into silence. Each family is
/// given a line that is genuinely not its syntax and must still be told about it.
#[test]
fn an_unrecognised_line_in_the_same_family_still_warns() {
    for (name, _, source, bogus) in cases() {
        // Insert the bogus line right after the header, where the accessibility pair sits.
        let (header, rest) = source.split_once('\n').expect("multi-line fixture");
        let with_bogus = format!("{header}\n  {bogus}\n{rest}");
        let parsed = fm_parser::parse(&with_bogus);
        assert!(
            parsed.warnings.iter().any(|w| w.contains("zzBogus")),
            "{name} swallowed an unrecognised line along with the directives: {:?}",
            parsed.warnings
        );
    }
}

/// ⚠️ AND A NAME THAT MERELY STARTS WITH `accTitle` IS NOT THE DIRECTIVE.
///
/// `strip_prefix("accTitle")` alone would match `accTitleish:` and every other identifier with that
/// prefix, silently swallowing them in eight families. The predicate keys on the DIRECTIVE FORM —
/// the keyword followed by `:` or whitespace — and this is what proves it still does.
#[test]
fn a_name_that_only_starts_with_the_keyword_is_not_swallowed() {
    for (name, _, source, _) in cases() {
        let (header, rest) = source.split_once('\n').expect("multi-line fixture");
        let with_lookalike = format!("{header}\n  accTitleish: nope\n{rest}");
        let parsed = fm_parser::parse(&with_lookalike);
        assert!(
            parsed.warnings.iter().any(|w| w.contains("accTitleish")),
            "{name} swallowed `accTitleish:`, so the skip matches on prefix rather than form: {:?}",
            parsed.warnings
        );
    }
}

/// Each family still parses its own content — the skip did not eat the diagram.
#[test]
fn each_family_still_parses_its_own_syntax() {
    /// A per-family predicate over the parsed IR, named so the table below stays readable.
    type Holds = Box<dyn Fn(&fm_core::MermaidDiagramIr) -> bool>;

    let expectations: Vec<(&str, Holds)> = vec![
        ("sequence", Box::new(|ir| !ir.edges.is_empty())),
        ("requirement", Box::new(|ir| !ir.nodes.is_empty())),
        ("sankey", Box::new(|ir| !ir.edges.is_empty())),
        ("gitGraph", Box::new(|ir| !ir.nodes.is_empty())),
        ("xychart", Box::new(|ir| ir.xy_chart_meta.is_some())),
        ("C4", Box::new(|ir| !ir.nodes.is_empty())),
        ("treemap", Box::new(|ir| ir.treemap_meta.is_some())),
        ("radar", Box::new(|ir| ir.radar_meta.is_some())),
        ("packet", Box::new(|ir| !ir.nodes.is_empty())),
    ];
    for ((name, _, source, _), (expected_name, holds)) in cases().into_iter().zip(expectations) {
        assert_eq!(name, expected_name, "the two tables fell out of step");
        let parsed = fm_parser::parse(&source);
        assert!(
            holds(&parsed.ir),
            "{name} lost its own content when the directives were skipped"
        );
    }
}

/// ⚠️ SANKEY DIVERGES FROM THE REFERENCE ON PURPOSE, AND THE DIVERGENCE IS PINNED HERE.
///
/// mermaid 11.15.0's sankey grammar REJECTS `accTitle:` with a hard parse error — the only one of
/// the twenty-one that does. We accept it and keep the value, which is a deliberate superset: the
/// directive is already extracted before the sankey loop runs, so rejecting the line would mean
/// discarding an accessibility title we are holding, in the one family where an author most needs
/// the diagram described.
///
/// Recorded as its own test so a future engine-parity sweep finds the reasoning attached to the
/// case rather than rediscovering it as a divergence bug.
#[test]
fn sankey_accepts_a_directive_the_reference_rejects() {
    let parsed = fm_parser::parse(&format!("sankey-beta\n{ACC}  a,b,1\n"));
    assert_eq!(parsed.ir.diagram_type, DiagramType::Sankey);
    assert_eq!(
        parsed.ir.meta.acc_title.as_deref(),
        Some("My Title"),
        "sankey dropped the title the reference refuses to parse at all"
    );
    assert!(
        !parsed.ir.edges.is_empty(),
        "sankey lost its record while accepting the directive"
    );
    assert!(
        parsed.warnings.is_empty(),
        "sankey warns about a line it accepts: {:?}",
        parsed.warnings
    );
}
