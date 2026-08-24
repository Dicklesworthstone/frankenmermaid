//! Every `url(#id)` must resolve, and every `<defs>` id must be used — across all diagram types.
//!
//! Two failure modes, one invariant:
//!
//!   * DANGLING — a `url(#grad)` with no `#grad` declared. SVG renders this as NOTHING, with no
//!     error and no warning: the fill, marker or filter silently vanishes. That is the dangerous
//!     direction, and nothing guarded it before this file.
//!   * DEAD — a `<defs>` child nothing references. Harmless to correctness but shipped in every
//!     render, and it is a known recurring family here rather than a hypothetical.
//!
//! The existing coverage was instance-shaped (`includes_defs_section`,
//! `includes_half_arrow_marker_defs`) — it checks that specific defs exist, not that the set is
//! internally consistent. This asserts the general property, so a new diagram type or a new
//! gradient gets the check for free.

use std::collections::BTreeSet;

/// Ids declared by `id="..."`, restricted to a given fragment.
fn ids_declared(fragment: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = fragment;
    while let Some(index) = rest.find("id=\"") {
        rest = &rest[index + 4..];
        if let Some(end) = rest.find('"') {
            out.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// Ids referenced by `url(#...)`.
fn ids_referenced(svg: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut rest = svg;
    while let Some(index) = rest.find("url(#") {
        rest = &rest[index + 5..];
        if let Some(end) = rest.find(')') {
            out.insert(rest[..end].to_string());
            rest = &rest[end..];
        }
    }
    out
}

/// Concatenated contents of every `<defs>` block.
///
/// Restricted to `<defs>` on purpose: node and edge elements carry `id`s too, but those are DOM and
/// accessibility targets, never `url(#)` referents. Counting them would report ~24 false "dead" ids
/// on a class diagram and make the gate useless.
fn defs_blocks(svg: &str) -> String {
    let mut acc = String::new();
    let mut rest = svg;
    while let Some(start) = rest.find("<defs>") {
        rest = &rest[start..];
        match rest.find("</defs>") {
            Some(end) => {
                acc.push_str(&rest[..end]);
                rest = &rest[end + "</defs>".len()..];
            }
            None => break,
        }
    }
    acc
}

const CASES: &[(&str, &str)] = &[
    (
        "flowchart_arrows",
        "flowchart TD\n  a[A] --> b[B]\n  b -.-> c[C]\n  c ==> d[D]\n  d --o e[E]\n  e --x f[F]\n",
    ),
    ("flowchart_plain", "flowchart TD\n  a[A] --> b[B]\n"),
    (
        "sequence",
        "sequenceDiagram\n  Alice->>Bob: Hi\n  Bob-->>Alice: Yo\n  Alice-)Bob: Async\n",
    ),
    (
        "class",
        "classDiagram\n  A <|-- B\n  C *-- D\n  E o-- F\n  G <.. H\n",
    ),
    ("state", "stateDiagram-v2\n  [*] --> A\n  A --> [*]\n"),
    ("er", "erDiagram\n  A ||--o{ B : has\n  C }|..|{ D : rel\n"),
    (
        "gantt",
        "gantt\n  title T\n  section S\n    X : a1, 2024-01-01, 3d\n",
    ),
    (
        "journey",
        "journey\n  title J\n  section S\n    Task: 5: Me\n",
    ),
    ("mindmap", "mindmap\n  root((R))\n    A\n    B\n"),
    ("gitgraph", "gitGraph\n  commit\n  branch dev\n  commit\n"),
    ("pie", "pie title P\n  \"A\" : 60\n  \"B\" : 40\n"),
    (
        "quadrant",
        "quadrantChart\n  title Q\n  x-axis L --> H\n  y-axis B --> T\n  A: [0.3, 0.6]\n",
    ),
    (
        "xychart",
        "xychart-beta\n  title X\n  x-axis [a, b]\n  y-axis \"y\" 0 --> 100\n  bar [50, 60]\n",
    ),
    ("block", "block-beta\n  columns 2\n  A[\"A\"] B[\"B\"]\n"),
    ("c4", "C4Context\n  title C\n  Person(a, \"A\", \"d\")\n"),
    (
        "requirement",
        "requirementDiagram\n  requirement R {\n  id: 1\n  text: t\n  }\n",
    ),
    ("sankey", "sankey-beta\n\nA,B,5\n"),
    ("kanban", "kanban\n  Col\n    t1[Card]\n"),
    ("timeline", "timeline\n  title T\n  2001 : A\n"),
    ("packet", "packet-beta\n  0-7: \"A\"\n"),
];

#[test]
fn every_url_reference_resolves_and_every_defs_id_is_used() {
    let mut dangling_report: Vec<String> = Vec::new();
    let mut dead_report: Vec<String> = Vec::new();
    let mut total_declared = 0_usize;
    let mut total_referenced = 0_usize;

    for (name, source) in CASES {
        let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
        let declared = ids_declared(&defs_blocks(&svg));
        let referenced = ids_referenced(&svg);
        total_declared += declared.len();
        total_referenced += referenced.len();

        let dangling: Vec<&str> = referenced
            .difference(&declared)
            .map(String::as_str)
            .collect();
        if !dangling.is_empty() {
            dangling_report.push(format!("{name}: {dangling:?}"));
        }
        let dead: Vec<&str> = declared
            .difference(&referenced)
            .map(String::as_str)
            .collect();
        if !dead.is_empty() {
            dead_report.push(format!("{name}: {dead:?}"));
        }
    }

    // NON-VACUITY CONTROL — this gate's most likely failure is being RIGHT FOR NO REASON.
    //
    // Both assertions below compare two sets. If the extractors ever returned empty — a changed
    // `<defs>` spelling, self-closing `<defs/>`, an attribute-order change putting something before
    // `id="` — then every difference is empty, every case passes, and a uniform green says exactly
    // what a genuinely clean corpus says. That is not hypothetical: a uniform verdict across every
    // case is the signature of a broken instrument, so the instrument has to prove it saw something
    // before its silence counts as evidence.
    //
    // Measured when written: 29 defs ids declared and 29 referenced across the corpus, with
    // flowchart_arrows at 5/5 and class at 5/5. The floors are set well below that so ordinary
    // churn does not trip them.
    assert!(
        total_declared >= 10,
        "only {total_declared} ids were found inside <defs> across {} diagram types — the \
         extractor is not seeing the document, so the checks below prove nothing",
        CASES.len()
    );
    assert!(
        total_referenced >= 10,
        "only {total_referenced} url(#) references were found across {} diagram types — the \
         extractor is not seeing the document, so the checks below prove nothing",
        CASES.len()
    );

    assert!(
        dangling_report.is_empty(),
        "url(#id) references with no matching declaration — these render as NOTHING, silently:\n  \
         {}",
        dangling_report.join("\n  ")
    );
    assert!(
        dead_report.is_empty(),
        "<defs> ids nothing references — dead weight shipped in every render:\n  {}",
        dead_report.join("\n  ")
    );
}
