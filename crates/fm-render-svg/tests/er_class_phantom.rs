//! `class CUSTOMER bad` in an ER diagram must not become an entity (bd-25lru).
//!
//! It did, and this one is worth reading carefully because it was INVISIBLE to the check the rest
//! of this phantom family is caught by. The phantom entity had no label to draw, so a
//! "the directive is not in the drawn text" assertion passed while the defect was fully present:
//!
//!   - `data-nodes` went from 2 to 3 and the phantom got its own `data-id` group;
//!   - it took LAYOUT SPACE — the viewBox grew from `0 0 326.125 437` to `0 0 395.425 623.5`,
//!     shifting the real entities;
//!   - the accessibility `<desc>` announced "Key nodes: CUSTOMER, ORDER, class CUSTOMER bad."
//!     A screen reader read the author's own directive out as an entity name.
//!
//! So these tests assert on the node set, the viewBox and the description — not on drawn text.
//! A phantom is not only what a sighted reader can see.
//!
//! The incumbent settles that this is valid input: mermaid 11.15.0 returns a clean PARSED for the
//! fixture (not even the no-DOM runtime error), and ER is not a free-text diagram type, so
//! acceptance is meaningful here.
//!
//! ER already handled `style`, `linkStyle`, `click`, `accTitle` and `accDescr` correctly via the
//! shared directive predicate. `class` is deliberately absent from that predicate, because in a
//! CLASS diagram `class A` DECLARES a node — so ER opts in to the same named guard state diagrams
//! use (bd-0audg) rather than the shared list growing a rule that would delete every bare class
//! declaration in the corpus.

use std::sync::LazyLock;

const STYLED: &str = "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n  classDef bad fill:#f00\n  \
                      class CUSTOMER bad\n";
const CLEAN: &str = "erDiagram\n  CUSTOMER ||--o{ ORDER : places\n";

static CLEAN_SVG: LazyLock<String> =
    LazyLock::new(|| fm_render_svg::render_svg(&fm_parser::parse(CLEAN).ir));

fn attr<'a>(svg: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("{name}=\"");
    let start = svg.find(&needle)? + needle.len();
    let end = svg[start..].find('"')? + start;
    Some(&svg[start..end])
}

fn desc(svg: &str) -> Option<&str> {
    let start = svg.find("<desc>")? + "<desc>".len();
    let end = svg[start..].find("</desc>")? + start;
    Some(&svg[start..end])
}

fn ids(ir: &fm_core::MermaidDiagramIr) -> Vec<&str> {
    ir.nodes.iter().map(|node| node.id.as_str()).collect()
}

/// THE DEFECT: the directive styles its target and declares nothing.
#[test]
fn an_er_class_directive_is_styled_but_never_becomes_an_entity() {
    let ir = fm_parser::parse(STYLED).ir;

    assert_eq!(
        ids(&ir),
        vec!["CUSTOMER", "ORDER"],
        "the directive was interned as an entity"
    );

    // CONTROL, and the half a naive fix breaks: the style must still reach its target. Dropping the
    // line entirely would remove the phantom and silently lose the styling.
    let styled = ir
        .nodes
        .iter()
        .find(|node| node.id.as_str() == "CUSTOMER")
        .expect("CONTROL FAILED: CUSTOMER was not declared");
    assert!(
        styled.classes.iter().any(|applied| applied == "bad"),
        "the class stopped being applied; CUSTOMER carries {:?}",
        styled.classes
    );
}

/// THE PHANTOM TOOK LAYOUT SPACE, so the styled chart must now measure exactly like the clean one.
///
/// This is the assertion that would have caught the defect where a drawn-text check could not: the
/// phantom had no label, but it had a box, and the box moved everything else.
#[test]
fn styling_an_entity_does_not_change_the_diagram_geometry() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(STYLED).ir);

    assert_eq!(
        attr(&svg, "viewBox"),
        attr(&CLEAN_SVG, "viewBox"),
        "a style directive changed the diagram's geometry, so it is still occupying layout space"
    );
    assert_eq!(
        attr(&svg, "data-nodes"),
        attr(&CLEAN_SVG, "data-nodes"),
        "the styled chart reports a different entity count than the identical unstyled one"
    );
    // NON-VACUITY: the comparison is only meaningful if both charts actually laid something out.
    assert!(
        attr(&CLEAN_SVG, "data-nodes").is_some_and(|count| count == "2"),
        "CONTROL FAILED: the reference chart did not render its two entities"
    );
}

/// THE ACCESSIBLE DESCRIPTION must not announce the directive as an entity.
///
/// The severity that makes this a P1 rather than cosmetic: assistive technology read out
/// "class CUSTOMER bad" as a named node of the diagram.
#[test]
fn the_accessible_description_does_not_announce_the_directive() {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(STYLED).ir);
    let description = desc(&svg).expect("the document carries a description");

    assert!(
        !description.contains("class CUSTOMER bad"),
        "the a11y description announced the author's directive as an entity: {description:?}"
    );
    // NON-VACUITY: it must still describe the real entities, or this passes on an empty string.
    assert!(
        description.contains("CUSTOMER") && description.contains("ORDER"),
        "CONTROL FAILED: the description names neither real entity: {description:?}"
    );
}

/// CONTROL: an entity legitimately NAMED `class` keeps its relationship.
///
/// The guard bails on a relation operator for exactly this case. Without it, the fix for a phantom
/// entity would have deleted a real one — the bd-ij0f shape, where a widened filter eats valid input.
#[test]
fn an_entity_named_class_is_not_swallowed_by_the_guard() {
    let ir = fm_parser::parse("erDiagram\n  class ||--o{ ORDER : places\n").ir;
    assert_eq!(
        ids(&ir),
        vec!["class", "ORDER"],
        "an entity named `class` was swallowed as a directive"
    );
    assert_eq!(ir.edges.len(), 1, "its relationship was dropped");
}

/// CONTROL: the state diagram this guard was originally written for still behaves.
///
/// The predicate was renamed and given a second caller here; pinning the first caller in the same
/// file is what makes that a shared rule rather than two copies drifting apart.
#[test]
fn the_state_diagram_caller_of_the_same_guard_still_behaves() {
    let ir =
        fm_parser::parse("stateDiagram-v2\n  [*] --> A\n  classDef bad fill:#f00\n  class A bad\n")
            .ir;
    assert!(
        !ids(&ir).iter().any(|id| id.contains("class")),
        "the state directive was interned as a state: {:?}",
        ids(&ir)
    );
    assert!(
        ir.nodes
            .iter()
            .any(|node| node.classes.iter().any(|applied| applied == "bad")),
        "the state style stopped being applied"
    );
}
