//! The `--- title: … ---` front-matter title is drawn for the families mermaid draws it for — and
//! for NO others.
//!
//! THE DEFECT, IN TWO HALVES, on the two surfaces this project ships.
//!
//! **1. The browser threw the whole front-matter block away, title included.**
//! `parse_front_matter_config` opened with an early return under `#[cfg(target_arch = "wasm32")]`,
//! so in the WASM bundle — the demo site and every embedder — `---\ntitle: …\n---` produced no
//! title at all, while native builds honoured it. The two targets disagreed about what a document
//! MEANS.
//!
//! The stated reason was real but over-broad: `serde_yaml` genuinely is excluded from the wasm
//! build by `[target.'cfg(not(target_arch = "wasm32"))'.dependencies]`, and that exclusion holds a
//! 700 KiB gzip ceiling the bundle sits ~1.5 KiB under, so it cannot simply be linked in. But a
//! title is a scalar on one line and never needed a YAML engine; only `config:` did. The wasm path
//! now reads the title with a scalar scan (+559 bytes gzip, measured) and still reports that config
//! keys are ignored.
//!
//! **2. Native drew the title for EVERY family.** The renderer's rule was "draw it whenever
//! `ir.meta.title` is set", so families the reference leaves bare got a title anyway.
//!
//! MEASURED REFERENCE — pinned mermaid 11.15.0 in Chromium 151, asking for BOTH spellings of a
//! title whether the text appears in the drawn SVG:
//!
//! ```text
//!                       title STATEMENT   front-matter title:
//!   flowchart                  no                yes
//!   class                      no                yes
//!   gitgraph                   no                yes
//!   requirement                no                yes
//!   block                      yes               NO
//!   kanban                     yes               NO
//!   timeline                   yes               NO
//!   C4 (every variant)         yes               NO
//!   sequence state er packet treemap radar journey gantt     yes   yes
//!   mindmap architecture sankey info                          —    no
//! ```
//!
//! ⚠️ THE TABLE, NOT A LIST, IS THE POINT OF THIS FILE — there are two ways to get this wrong and it
//! refuses both. Simply letting the front matter through (the whole of half 1, and the obvious
//! reading of "the title is missing") makes all 22 families draw one, satisfying every assertion
//! about titles APPEARING. Gating on the family alone — the obvious repair — instead drops the
//! STATEMENT title from `block`, `kanban`, `timeline` and C4, which the reference does draw. The
//! first draft of this fix did exactly that, and `text_parity` caught it on timeline.

use fm_core::DiagramType;

const TITLE: &str = "FMTITLE";

fn render_with_front_matter(body: &str) -> String {
    let source = format!("---\ntitle: {TITLE}\n---\n{body}\n");
    let parsed = fm_parser::parse(&source);
    assert_eq!(
        parsed.ir.meta.title.as_deref(),
        Some(TITLE),
        "the front matter did not even reach the IR, so the drawing assertions below would be \
         vacuous for the wrong reason: {body:?}"
    );
    fm_render_svg::render_svg(&parsed.ir)
}

/// Every family the reference titles is titled here too.
#[test]
fn a_family_the_reference_titles_draws_the_front_matter_title() {
    for (name, body) in [
        ("flowchart", "flowchart LR\n  A --> B"),
        ("sequence", "sequenceDiagram\n  A->>B: x"),
        ("class", "classDiagram\n  class A"),
        ("state", "stateDiagram-v2\n  [*] --> A"),
        ("er", "erDiagram\n  A ||--o{ B : has"),
        ("gitgraph", "gitGraph\n  commit"),
        ("packet", "packet-beta\n0-7: \"One\""),
        ("treemap", "treemap-beta\n\"A\": 10"),
        (
            "radar",
            "radar-beta\n  axis a[\"A\"], b[\"B\"], c[\"C\"]\n  curve x[\"X\"]{1,2,3}",
        ),
        (
            "requirement",
            "requirementDiagram\n  requirement r {\n    id: 1\n    text: t\n    risk: high\n    verifymethod: test\n  }",
        ),
        ("journey", "journey\n  section Go\n    Wake: 5: Me"),
        (
            "gantt",
            "gantt\n  dateFormat YYYY-MM-DD\n  section S\n  a :a1, 2024-01-01, 3d",
        ),
    ] {
        let svg = render_with_front_matter(body);
        assert!(
            svg.contains(TITLE),
            "{name}: the reference draws the front-matter title and we drew none"
        );
    }
}

/// ⚠️ PLANTED NEGATIVE 1: families that ignore the FRONT-MATTER title must ignore it.
///
/// This is the half a naive implementation fails. Letting the front matter through is enough to
/// make every "the title is drawn" assertion pass while these families gain a title mermaid never
/// puts on them — precisely the state this codebase was in natively, where all 22 were titled.
#[test]
fn a_family_the_reference_leaves_untitled_draws_no_title() {
    for (name, body) in [
        ("mindmap", "mindmap\n  root((r))\n    a"),
        ("block", "block-beta\n  columns 1\n  a[\"A\"]"),
        ("kanban", "kanban\n  Todo\n    [t]"),
        ("architecture", "architecture-beta\n  service a(cloud)[A]"),
        ("sankey", "sankey-beta\n\nA,B,10"),
        ("timeline", "timeline\n  2021 : x"),
        ("C4Context", "C4Context\n  Person(p, \"P\", \"d\")"),
        (
            "C4Container",
            "C4Container\n  Container(a, \"A\", \"t\", \"d\")",
        ),
        (
            "C4Component",
            "C4Component\n  Component(a, \"A\", \"t\", \"d\")",
        ),
        ("C4Dynamic", "C4Dynamic\n  Person(p, \"P\", \"d\")"),
    ] {
        let svg = render_with_front_matter(body);
        assert!(
            !svg.contains(TITLE),
            "{name}: the reference draws NO title for this family, and we drew one — this is the \
             exact failure of letting the title through without asking which families take it"
        );
    }
}

/// The reserved title band and the drawn title agree.
///
/// ⚠️ `svg_frame` reserves height for the title and the render pass draws into it. If only one of
/// them consults the family gate, an untitled family still gets a blank strip at the top, or a
/// titled one gets its text clipped out of the viewBox — and neither fails loudly, because the SVG
/// is still well-formed. Comparing a titled render against the same diagram with no front matter
/// pins that the band appears only with the title.
#[test]
fn the_reserved_band_matches_whether_a_title_is_drawn() {
    fn view_box(svg: &str) -> String {
        let start = svg.find("viewBox=\"").expect("a viewBox") + "viewBox=\"".len();
        let rest = &svg[start..];
        rest[..rest.find('"').expect("closing quote")].to_string()
    }

    // A titled family: the band must appear, so the viewBox must GROW against the untitled control.
    let titled = render_with_front_matter("flowchart LR\n  A --> B");
    let untitled = fm_render_svg::render_svg(&fm_parser::parse("flowchart LR\n  A --> B\n").ir);
    assert!(titled.contains(TITLE));
    assert_ne!(
        view_box(&titled),
        view_box(&untitled),
        "a drawn title reserved no band, so it is drawn outside the viewBox"
    );

    // An untitled family: nothing is drawn, so nothing may be reserved either.
    let sankey_titled = render_with_front_matter("sankey-beta\n\nA,B,10");
    let sankey_plain = fm_render_svg::render_svg(&fm_parser::parse("sankey-beta\n\nA,B,10\n").ir);
    assert_eq!(
        view_box(&sankey_titled),
        view_box(&sankey_plain),
        "a family that draws no title still reserved a band for one, leaving a blank strip"
    );
}

/// ⚠️ PLANTED NEGATIVE 2: the `title …` STATEMENT still reaches the families that draw it.
///
/// The mirror of the case above, and the one a family-only gate fails. `block`, `kanban`,
/// `timeline` and every C4 variant ignore the front-matter title but DO draw the statement, so a
/// gate answering per family — rather than per (family, spelling) — silently deletes a title these
/// diagrams are supposed to carry. It is its own test because the two negatives pull in opposite
/// directions: no single list of families satisfies both.
#[test]
fn a_family_that_ignores_front_matter_still_draws_its_title_statement() {
    for (name, body) in [
        (
            "block",
            "block-beta\n  title FMTITLE\n  columns 1\n  a[\"A\"]",
        ),
        ("kanban", "kanban\n  title FMTITLE\n  Todo\n    [t]"),
        ("timeline", "timeline\n  title FMTITLE\n  2021 : x"),
        (
            "C4Context",
            "C4Context\n  title FMTITLE\n  Person(p, \"P\", \"d\")",
        ),
    ] {
        let parsed = fm_parser::parse(&format!("{body}\n"));
        assert_eq!(
            parsed.ir.meta.title.as_deref(),
            Some(TITLE),
            "{name}: the title statement did not reach the IR, so this assertion would be vacuous"
        );
        assert!(
            !parsed.ir.meta.title_from_front_matter,
            "{name}: a `title` STATEMENT was recorded as front-matter provenance, which inverts \
             every decision the renderer makes from it"
        );
        let svg = fm_render_svg::render_svg(&parsed.ir);
        assert!(
            svg.contains(TITLE),
            "{name}: the reference draws this family's `title` statement and we drew none"
        );
    }
}

/// The two spellings are recorded distinctly, which is what the renderer's table reads.
#[test]
fn front_matter_and_statement_titles_are_told_apart() {
    let front_matter = fm_parser::parse("---\ntitle: FMTITLE\n---\nflowchart LR\n  A --> B\n").ir;
    assert_eq!(front_matter.meta.title.as_deref(), Some(TITLE));
    assert!(front_matter.meta.title_from_front_matter);

    let statement =
        fm_parser::parse("gantt\n  title FMTITLE\n  section S\n  a :a1, 2024-01-01, 3d\n").ir;
    assert_eq!(statement.meta.title.as_deref(), Some(TITLE));
    assert!(
        !statement.meta.title_from_front_matter,
        "a statement title claimed front-matter provenance"
    );
}

/// The family gate is exhaustive, so a new `DiagramType` cannot silently inherit an answer.
///
/// The gate is written as a match over every variant with no wildcard arm; this test is the
/// runtime half of that guarantee — it walks the same list and asserts each variant renders without
/// panicking, so a variant added to `fm-core` and to this list, but not to the gate, fails to
/// compile in the renderer rather than defaulting.
#[test]
fn every_diagram_type_has_a_decided_answer() {
    let all = [
        DiagramType::Flowchart,
        DiagramType::Sequence,
        DiagramType::State,
        DiagramType::Gantt,
        DiagramType::Class,
        DiagramType::Er,
        DiagramType::Mindmap,
        DiagramType::Pie,
        DiagramType::GitGraph,
        DiagramType::Journey,
        DiagramType::Requirement,
        DiagramType::Timeline,
        DiagramType::QuadrantChart,
        DiagramType::Sankey,
        DiagramType::XyChart,
        DiagramType::BlockBeta,
        DiagramType::PacketBeta,
        DiagramType::ArchitectureBeta,
        DiagramType::C4Context,
        DiagramType::C4Container,
        DiagramType::C4Component,
        DiagramType::C4Dynamic,
        DiagramType::C4Deployment,
        DiagramType::Kanban,
        DiagramType::Treemap,
        DiagramType::Radar,
        DiagramType::Info,
        DiagramType::Unknown,
    ];
    for diagram_type in all {
        let mut ir = fm_core::MermaidDiagramIr::empty(diagram_type);
        ir.meta.title = Some(TITLE.to_string());
        let svg = fm_render_svg::render_svg(&ir);
        assert!(
            svg.contains("<svg"),
            "{diagram_type:?}: rendering an empty diagram with a title did not produce an SVG"
        );
    }
}
