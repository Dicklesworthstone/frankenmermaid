//! The root SVG announces WHAT KIND of diagram it is, and stops claiming to be a flat image
//! (bd-6odk2).
//!
//! HOW THIS GAP WAS FOUND. The drawn-text sweep against the pinned mermaid 11.15.0 bundle went dry
//! — six parity defects, then nothing. So the next sweep asked a different question about the same
//! renders: not "is the text there" but "is it ANNOUNCED". Roles and `aria-roledescription` are
//! invisible to any comparison of drawn glyphs, which is exactly why they had never been checked.
//!
//! REFERENCE, measured in Chromium 151 across all 23 families we support
//! (`roledesc_probe.mjs`): every mermaid root carries `role="graphics-document document"` and an
//! `aria-roledescription` naming the family. Ours carried `role="img"` and no roledescription at
//! all, so a screen reader announced every diagram this crate has ever produced identically —
//! "graphic" — whatever it was.
//!
//! ⚠️ THE SHARPER HALF IS THE ROLE, NOT THE MISSING ATTRIBUTE. `img` declares the whole subtree a
//! single flat picture, and assistive technology does not walk into one. This renderer emits
//! `role="graphics-symbol"` with an `aria-label` and `tabindex="0"` on every node and edge — the
//! root was contradicting all of it, so that per-element work was unreachable in the very tools it
//! exists for.
//!
//! THE NEGATIVE CASE, in this bead's shape: the new thing must be DIFFERENT from the fallback it
//! used to collapse into. For a node shape that fallback is `Rect`; here it is `role="img"`, the
//! one root annotation that makes everything below it invisible.

use fm_core::{DiagramType, MermaidDiagramIr};
use fm_render_svg::{SvgRenderConfig, render_svg, render_svg_with_config};

fn render(source: &str) -> String {
    render_svg(&fm_parser::parse(source).ir)
}

/// The opening `<svg …>` tag only — attributes on descendants must not be mistaken for the root's.
fn root_tag(svg: &str) -> &str {
    let start = svg.find("<svg").expect("no root element");
    let end = svg[start..].find('>').expect("unterminated root") + start;
    &svg[start..=end]
}

/// Every variant of [`DiagramType`], written out rather than iterated.
///
/// There is no `strum`-style enumeration in this workspace, so this list IS the enumeration — and a
/// hand-written enumeration that quietly falls behind its enum makes every "for every variant" test
/// vacuous for exactly the family most likely to be wrong. [`declaration_index`] below closes that
/// hole at compile time.
const ALL_TYPES: &[DiagramType] = &[
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

/// ⚠️ THE NEGATIVE CASE: the root must not be the flat image it used to declare itself.
///
/// This is not "the attribute changed value". The assertion is a CONTRADICTION test: the same
/// document is checked to contain focusable, individually-labelled `graphics-symbol` children, and
/// a root that says `img` is a claim that none of them exist. Either the root stops saying `img` or
/// the per-node accessibility work below it is dead weight.
#[test]
fn the_root_is_not_a_flat_image_while_carrying_navigable_children() {
    let svg = render("flowchart LR\n  A[Start] --> B[End]\n");
    let root = root_tag(&svg);

    assert!(
        svg.contains(r#"role="graphics-symbol""#) && svg.contains(r#"tabindex="0""#),
        "premise gone: this render no longer emits focusable labelled children, so the root's \
         role is no longer a contradiction and this test proves nothing:\n{svg}"
    );
    assert!(
        !root.contains(r#"role="img""#),
        "the root still declares the whole diagram one flat image, hiding its own \
         graphics-symbol children from assistive technology:\n{root}"
    );
    assert!(
        root.contains(r#"role="graphics-document document""#),
        "the root does not carry the measured role:\n{root}"
    );
}

/// Two different families must not announce the same thing.
///
/// The failure this catches is the cheap implementation: wire the attribute up, give everything one
/// constant, and every assertion about "the attribute is present" still passes. Announcing
/// "diagram" for all 23 families is no better than announcing nothing.
#[test]
fn different_families_announce_differently() {
    let cases = [
        ("flowchart LR\n  A --> B\n", "flowchart-v2"),
        ("sequenceDiagram\n  A->>B: hi\n", "sequence"),
        ("classDiagram\n  class A\n", "class"),
        ("stateDiagram-v2\n  [*] --> A\n", "stateDiagram"),
        ("erDiagram\n  A ||--o{ B : r\n", "er"),
        ("pie title V\n  \"A\" : 40\n", "pie"),
        ("mindmap\n  root((r))\n    a\n", "mindmap"),
        ("timeline\n  title T\n  2024 : x\n", "timeline"),
    ];

    let mut seen = Vec::new();
    for (source, expected) in cases {
        let svg = render(source);
        let root = root_tag(&svg);
        let attr = format!(r#"aria-roledescription="{expected}""#);
        assert!(
            root.contains(&attr),
            "expected {attr} for {source:?}, got:\n{root}"
        );
        assert!(
            !seen.contains(&expected),
            "{expected:?} was announced for two different families"
        );
        seen.push(expected);
    }
    assert_eq!(
        seen.len(),
        cases.len(),
        "families collapsed onto each other"
    );
}

/// ⚠️ THE ANNOUNCED NAME IS MEASURED, NOT OUR OWN SPELLING.
///
/// The obvious implementation is `aria-roledescription = as_str()`, and it is wrong for twelve of
/// the twenty-eight variants. These are the six spellings where the difference is visible in a
/// rendered document: reading them out of the bundle is the only way to get them, and no test that
/// merely checks "an aria-roledescription is present" can tell the two apart.
#[test]
fn the_announced_name_is_the_measured_one_not_our_type_name() {
    let cases = [
        (
            "flowchart LR\n  A --> B\n",
            DiagramType::Flowchart,
            "flowchart-v2",
        ),
        (
            "stateDiagram-v2\n  [*] --> A\n",
            DiagramType::State,
            "stateDiagram",
        ),
        (
            "xychart-beta\n  title T\n  bar [1,2,3]\n",
            DiagramType::XyChart,
            "xychart",
        ),
        (
            "block-beta\n  columns 1\n  a\n",
            DiagramType::BlockBeta,
            "block",
        ),
        (
            "packet-beta\n  0-7: \"a\"\n",
            DiagramType::PacketBeta,
            "packet",
        ),
        (
            "requirementDiagram\n  requirement r {\n  id: 1\n  text: t\n  risk: low\n  verifymethod: test\n  }\n",
            DiagramType::Requirement,
            "requirement",
        ),
    ];

    for (source, ty, measured) in cases {
        assert_ne!(
            ty.as_str(),
            measured,
            "{ty:?} was chosen as a divergence case but its own name already matches the \
             measured token — this row no longer discriminates and must be replaced"
        );
        assert_eq!(
            ty.aria_roledescription(),
            measured,
            "{ty:?} announces its own type name instead of what the reference emits"
        );

        let svg = render(source);
        let root = root_tag(&svg);
        assert!(
            root.contains(&format!(r#"aria-roledescription="{measured}""#)),
            "{source:?} did not announce {measured:?}:\n{root}"
        );
        assert!(
            !root.contains(&format!(r#"aria-roledescription="{}""#, ty.as_str())),
            "{source:?} announced our internal spelling {:?}:\n{root}",
            ty.as_str()
        );
    }
}

/// The five C4 variants deliberately share one announcement, and that is the reference behaviour.
///
/// Recorded as its own test because it is the one place the mapping is intentionally many-to-one:
/// a future reader tightening `different_families_announce_differently` would otherwise read this
/// as the collapse that test forbids.
#[test]
fn the_c4_variants_share_one_announcement_as_upstream_does() {
    for ty in [
        DiagramType::C4Context,
        DiagramType::C4Container,
        DiagramType::C4Component,
        DiagramType::C4Dynamic,
        DiagramType::C4Deployment,
    ] {
        assert_eq!(
            ty.aria_roledescription(),
            "c4",
            "{ty:?} does not announce as the c4 family"
        );
    }
}

/// Every variant announces something, and nothing announces an empty string.
#[test]
fn every_diagram_type_announces_something() {
    for ty in ALL_TYPES {
        let announced = ty.aria_roledescription();
        assert!(
            !announced.is_empty(),
            "{ty:?} has no aria-roledescription; a reader would fall back to the bare role"
        );
        assert!(
            !announced.contains('"') && !announced.contains('<'),
            "{ty:?} announces {announced:?}, which needs escaping in an attribute"
        );
    }
}

/// The position each variant must occupy in [`ALL_TYPES`].
///
/// ⚠️ THIS MATCH HAS NO WILDCARD ARM, AND THAT IS THE WHOLE MECHANISM. Adding a family to
/// `DiagramType` stops this file COMPILING, so `ALL_TYPES` cannot silently fall behind the enum the
/// way a hand-written list otherwise does. `the_variant_list_is_complete` then checks the other two
/// failure modes a compiler cannot see: a missing entry and a repeated one.
const fn declaration_index(ty: DiagramType) -> usize {
    match ty {
        DiagramType::Flowchart => 0,
        DiagramType::Sequence => 1,
        DiagramType::State => 2,
        DiagramType::Gantt => 3,
        DiagramType::Class => 4,
        DiagramType::Er => 5,
        DiagramType::Mindmap => 6,
        DiagramType::Pie => 7,
        DiagramType::GitGraph => 8,
        DiagramType::Journey => 9,
        DiagramType::Requirement => 10,
        DiagramType::Timeline => 11,
        DiagramType::QuadrantChart => 12,
        DiagramType::Sankey => 13,
        DiagramType::XyChart => 14,
        DiagramType::BlockBeta => 15,
        DiagramType::PacketBeta => 16,
        DiagramType::ArchitectureBeta => 17,
        DiagramType::C4Context => 18,
        DiagramType::C4Container => 19,
        DiagramType::C4Component => 20,
        DiagramType::C4Dynamic => 21,
        DiagramType::C4Deployment => 22,
        DiagramType::Kanban => 23,
        DiagramType::Treemap => 24,
        DiagramType::Radar => 25,
        DiagramType::Info => 26,
        DiagramType::Unknown => 27,
    }
}

/// The hand-written variant list above covers the enum exactly once.
#[test]
fn the_variant_list_is_complete() {
    const EXPECTED: usize = declaration_index(DiagramType::Unknown) + 1;
    assert_eq!(
        ALL_TYPES.len(),
        EXPECTED,
        "ALL_TYPES has {} entries for {EXPECTED} variants: a family is missing or listed twice",
        ALL_TYPES.len()
    );
    for (position, ty) in ALL_TYPES.iter().enumerate() {
        assert_eq!(
            declaration_index(*ty),
            position,
            "{ty:?} is at position {position}; ALL_TYPES is missing an earlier variant or repeats one"
        );
    }
}

/// A scene rendered without its IR announces no family rather than a wrong one.
///
/// The scene path can be handed geometry with no diagram behind it. Silence is correct there: a
/// missing `aria-roledescription` degrades to the role, which is true, where a defaulted guess is a
/// confident false statement about what the reader is looking at.
#[test]
fn a_scene_without_an_ir_announces_no_family() {
    let ir = fm_parser::parse("flowchart LR\n  A --> B\n").ir;
    let layout = fm_layout::layout_diagram(&ir);
    let scene = fm_layout::build_render_scene(&ir, &layout);
    let svg = fm_render_svg::render_scene_to_svg(&scene, &SvgRenderConfig::default());
    let root = root_tag(&svg);

    assert!(
        !root.contains("aria-roledescription"),
        "a scene with no IR guessed at a diagram family:\n{root}"
    );
    assert!(
        !root.contains(r#"role="img""#),
        "the scene path still declares itself a flat image:\n{root}"
    );
}

/// Turning accessibility off removes the whole root annotation, roledescription included.
///
/// The gate is `config.accessible`, not the attribute, so this is what proves the new attribute
/// went behind the existing switch rather than beside it.
#[test]
fn the_accessibility_switch_still_governs_the_whole_annotation() {
    let ir = fm_parser::parse("flowchart LR\n  A --> B\n").ir;
    let config = SvgRenderConfig {
        accessible: false,
        ..SvgRenderConfig::default()
    };
    let root_off = {
        let svg = render_svg_with_config(&ir, &config);
        root_tag(&svg).to_string()
    };

    assert!(
        !root_off.contains("aria-roledescription"),
        "the roledescription escaped the accessibility switch:\n{root_off}"
    );
    assert!(
        !root_off.contains("role="),
        "the root role escaped the accessibility switch:\n{root_off}"
    );

    let root_on = {
        let svg = render_svg_with_config(&ir, &SvgRenderConfig::default());
        root_tag(&svg).to_string()
    };
    assert_ne!(
        root_on, root_off,
        "the accessibility switch changes nothing about the root"
    );
}

/// An empty diagram still announces its family.
///
/// `Unknown` is the fallback, so a family that renders no nodes must not fall back with them.
#[test]
fn a_diagram_with_no_content_still_announces_its_family() {
    let ir = MermaidDiagramIr {
        diagram_type: DiagramType::Sankey,
        ..MermaidDiagramIr::default()
    };
    let svg = render_svg(&ir);
    let root = root_tag(&svg);
    assert!(
        root.contains(r#"aria-roledescription="sankey""#),
        "an empty sankey announced as something else:\n{root}"
    );
}
