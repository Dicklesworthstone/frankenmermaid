//! Accessibility features for SVG diagrams.
//!
//! Provides ARIA attributes, text alternatives, and accessibility CSS utilities.

use std::borrow::Cow;
use std::fmt::Write as _;

use fm_core::{ArrowType, IrNode, MermaidDiagramIr};
use fm_layout::DiagramLayout;

/// Generate an accessible description for a diagram.
#[must_use]
pub fn describe_diagram(ir: &MermaidDiagramIr) -> String {
    describe_diagram_with_layout(ir, None)
}

/// Generate an accessible description for a diagram with optional layout context.
#[must_use]
pub fn describe_diagram_with_layout(
    ir: &MermaidDiagramIr,
    layout: Option<&DiagramLayout>,
) -> String {
    // Build the description straight into one `String` instead of collecting a `Vec<String>` of
    // `format!`-allocated parts and `join(". ")`-ing them. `join(". ")` inserts ". " BETWEEN parts,
    // so the first part is written bare and every subsequent (conditionally present) part is prefixed
    // with ". " — byte-identical to the old collect+join. Drops the parts `Vec`, each part's
    // intermediate `format!` `String`, and the final join allocation per render. Capacity covers the
    // bounded worst case (counts + direction + up to 3 key nodes + 3 relationship sentences + the
    // layout line) so the common desc never reallocs — the old `join` allocated the result exactly.
    let mut desc = String::with_capacity(512);

    let type_desc = match ir.diagram_type.as_str() {
        "flowchart" => "flowchart diagram",
        "sequence" => "sequence diagram",
        "class" => "class diagram",
        "state" => "state diagram",
        "gantt" => "Gantt chart",
        "pie" => "pie chart",
        "er" | "erDiagram" => "entity-relationship diagram",
        "journey" => "user journey diagram",
        "mindmap" => "mindmap",
        "timeline" => "timeline",
        "quadrant" => "quadrant chart",
        _ => "diagram",
    };

    let diagnostics = ir.diagnostic_counts();
    let _ = write!(
        desc,
        "{} with {} node{} and {} edge{}",
        leading_type_phrase(type_desc),
        ir.nodes.len(),
        plural_suffix(ir.nodes.len()),
        ir.edges.len(),
        plural_suffix(ir.edges.len())
    );

    if !ir.clusters.is_empty() {
        let _ = write!(
            desc,
            ". organized in {} group{}",
            ir.clusters.len(),
            plural_suffix(ir.clusters.len())
        );
    }

    let direction_desc = match ir.direction {
        fm_core::GraphDirection::LR => "flowing left to right",
        fm_core::GraphDirection::RL => "flowing right to left",
        fm_core::GraphDirection::TB | fm_core::GraphDirection::TD => "flowing top to bottom",
        fm_core::GraphDirection::BT => "flowing bottom to top",
    };
    let _ = write!(desc, ". {direction_desc}");

    // Write the joined lists element-by-element rather than `join(sep)`-ing into a temporary String:
    // `join(sep)` == first element, then each subsequent prefixed with `sep`. Byte-identical, drops the
    // intermediate join allocation.
    let key_nodes = summarize_key_nodes(ir);
    if let Some((first, rest)) = key_nodes.split_first() {
        let _ = write!(desc, ". Key nodes: {first}");
        for node in rest {
            let _ = write!(desc, ", {node}");
        }
    }

    let relationships = summarize_key_relationships(ir);
    if let Some((first, rest)) = relationships.split_first() {
        let _ = write!(desc, ". Key relationships: {first}");
        for rel in rest {
            let _ = write!(desc, "; {rel}");
        }
    }

    if diagnostics.warnings > 0 || diagnostics.errors > 0 {
        let mut diag_parts = Vec::new();
        if diagnostics.warnings > 0 {
            diag_parts.push(format!(
                "{} warning{}",
                diagnostics.warnings,
                plural_suffix(diagnostics.warnings)
            ));
        }
        if diagnostics.errors > 0 {
            diag_parts.push(format!(
                "{} error{}",
                diagnostics.errors,
                plural_suffix(diagnostics.errors)
            ));
        }
        let _ = write!(desc, ". Diagnostics: {}", diag_parts.join(", "));
    }

    if let Some(layout) = layout {
        let _ = write!(
            desc,
            ". Layout spans {:.0} by {:.0} units with {} rendered node box{} and {} routed edge path{}",
            layout.bounds.width,
            layout.bounds.height,
            layout.nodes.len(),
            plural_suffix(layout.nodes.len()),
            layout.edges.len(),
            plural_suffix(layout.edges.len())
        );
        if layout.stats.crossing_count > 0 {
            let _ = write!(
                desc,
                ". The layout currently contains {} edge crossing{}",
                layout.stats.crossing_count,
                plural_suffix(layout.stats.crossing_count)
            );
        }
    }

    desc.push('.');
    desc
}

fn leading_type_phrase(type_desc: &str) -> String {
    if type_desc.starts_with("A ") || type_desc.starts_with("a ") {
        type_desc.to_string()
    } else {
        format!("A {type_desc}")
    }
}

fn plural_suffix(count: usize) -> &'static str {
    if count == 1 { "" } else { "s" }
}

fn summarize_key_nodes(ir: &MermaidDiagramIr) -> Vec<Cow<'_, str>> {
    ir.nodes
        .iter()
        // A block-beta `space` is a grid spacer with no label, so `node_label` falls back to its
        // GENERATED id and the description read "Key nodes: a, __space_4, c" — an internal name
        // announced to a screen reader for a cell drawn at opacity 0 (bd-ukj2). Uses the renderer's
        // own predicate rather than a private copy, for the same reason the node path does.
        .filter(|node| !crate::is_block_beta_space_node(node))
        .filter_map(|node| node_label(node, ir))
        .filter(|label| !label.is_empty())
        .take(3)
        .collect()
}

fn summarize_key_relationships(ir: &MermaidDiagramIr) -> Vec<String> {
    ir.edges
        .iter()
        .filter_map(|edge| {
            let from = ir
                .resolve_endpoint_node(edge.from)
                .and_then(|id| ir.nodes.get(id.0))?;
            let to = ir
                .resolve_endpoint_node(edge.to)
                .and_then(|id| ir.nodes.get(id.0))?;
            Some(describe_edge(
                Some(from),
                Some(to),
                edge.arrow,
                edge.co_arrow(),
                edge.label
                    .and_then(|label_id| ir.labels.get(label_id.0))
                    .map(|label| label.text.as_str()),
                ir,
            ))
        })
        .take(3)
        .collect()
}

fn node_label<'a>(node: &'a IrNode, ir: &'a MermaidDiagramIr) -> Option<Cow<'a, str>> {
    // Borrow the name (trimmed label text, or the id fallback) instead of `to_string()`/`clone()` —
    // `summarize_key_nodes` only `join`s these into the `<desc>`, which reads them by reference.
    // Byte-identical: `str::trim` returns a slice of the same bytes.
    node.label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|label| label.text.trim())
        .filter(|label| !label.is_empty())
        .map(Cow::Borrowed)
        .or_else(|| (!node.id.is_empty()).then_some(Cow::Borrowed(node.id.as_str())))
}

/// Generate a text alternative for a node.
#[must_use]
pub fn describe_node(node: &IrNode, ir: &MermaidDiagramIr) -> String {
    let label = node
        .label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|l| l.text.as_str())
        .unwrap_or(&node.id);

    let shape_desc = match node.shape {
        // bd-7ls21. Described by what a reader SEES, not by mermaid's registry key: "notch-rect"
        // means nothing spoken aloud, and a screen-reader user gets the shape's identity from this
        // string alone.
        fm_core::NodeShape::NotchedRect => "notched rectangle",
        fm_core::NodeShape::LinedRect => "lined rectangle",
        fm_core::NodeShape::SmallCircle => "small circle",
        fm_core::NodeShape::WindowPane => "window pane",
        fm_core::NodeShape::DataStore => "data store",
        fm_core::NodeShape::TextBlock => "text block",
        fm_core::NodeShape::BraceLeft => "left curly brace",
        fm_core::NodeShape::BraceRight => "right curly brace",
        fm_core::NodeShape::Braces => "curly braces",
        fm_core::NodeShape::DividedRect => "divided rectangle",
        fm_core::NodeShape::FramedCircle => "framed circle",
        fm_core::NodeShape::FlippedTriangle => "downward triangle",
        fm_core::NodeShape::NotchedPentagon => "notched pentagon",
        fm_core::NodeShape::SlopedRect => "sloped rectangle",
        fm_core::NodeShape::HorizontalCylinder => "horizontal cylinder",
        fm_core::NodeShape::TaggedRect => "tagged rectangle",
        fm_core::NodeShape::LinedCylinder => "lined cylinder",
        fm_core::NodeShape::Document => "document",
        fm_core::NodeShape::LinedDocument => "lined document",
        fm_core::NodeShape::LightningBolt => "lightning bolt",
        fm_core::NodeShape::Flag => "flag",
        fm_core::NodeShape::HalfRoundedRect => "half-rounded rectangle",
        fm_core::NodeShape::StackedDocument => "stacked documents",
        fm_core::NodeShape::StackedRect => "stacked rectangles",
        fm_core::NodeShape::Bang => "starburst",
        fm_core::NodeShape::CurvedTrapezoid => "curved trapezoid",
        fm_core::NodeShape::TaggedDocument => "tagged document",
        fm_core::NodeShape::BowTieRect => "stored data block",
        fm_core::NodeShape::Hourglass => "hourglass",
        fm_core::NodeShape::Rect => "rectangle",
        fm_core::NodeShape::Rounded => "rounded rectangle",
        fm_core::NodeShape::Stadium => "stadium shape",
        fm_core::NodeShape::Diamond => "diamond",
        fm_core::NodeShape::Hexagon => "hexagon",
        fm_core::NodeShape::Circle => "circle",
        fm_core::NodeShape::FilledCircle => "filled circle",
        fm_core::NodeShape::DoubleCircle => "double circle",
        fm_core::NodeShape::Cylinder => "cylinder",
        fm_core::NodeShape::Trapezoid => "trapezoid",
        fm_core::NodeShape::HorizontalBar => "horizontal bar",
        fm_core::NodeShape::Subroutine => "subroutine box",
        fm_core::NodeShape::Asymmetric => "flag shape",
        fm_core::NodeShape::Note => "note",
        fm_core::NodeShape::InvTrapezoid => "inverted trapezoid",
        fm_core::NodeShape::Triangle => "triangle",
        fm_core::NodeShape::Pentagon => "pentagon",
        fm_core::NodeShape::Star => "star",
        fm_core::NodeShape::Cloud => "cloud",
        fm_core::NodeShape::Tag => "tag",
        fm_core::NodeShape::CrossedCircle => "crossed circle",
        fm_core::NodeShape::Parallelogram => "parallelogram",
        fm_core::NodeShape::InvParallelogram => "inverted parallelogram",
    };

    let mut description = format!("Node: {label}, {shape_desc}");

    // A JOURNEY STEP declares two things beyond its name — a SCORE and its ACTORS — and neither was
    // reachable by anyone (bd-fsj42). The parser stores them as class hooks (`journey-score-5`,
    // `journey-actor-Alice`), which is a styling affordance, not information: no CSS rule targets
    // them, no text run carries them, and the accessible name said only `Node: TaskOne, rounded
    // rectangle`. A reader could not tell who performs a task or how it scored. mermaid draws both —
    // a face for the score, a coloured circle per actor.
    //
    // Read back from the classes because that is where the parser puts them; the casing survives
    // there (`journey-actor-Alice`, not `alice`), so the actor is announced as the author wrote it.
    //
    // Gated on `journey-step` so this cannot alter any other diagram type's description, and the
    // bare `journey-actor` marker is skipped — it flags that a step HAS actors and names none.
    description.push_str(&journey_step_description_suffix(node));

    description
}

/// The `, score N, actors: A, B` tail a JOURNEY STEP adds to its accessible name (bd-fsj42), or an
/// empty string for any other node.
///
/// SHARED, and that is the point. The node description is built in two places: `describe_node` for
/// the `Element` path, and a hand-written `<title>Node: …` in the streaming fast fragment that the
/// DEFAULT configuration actually takes. My first version enriched only `describe_node`, so the fix
/// was inert in the shipped path and live only under `include_source_spans` — a guard on one of two
/// paths, which this repo has been bitten by before. One function, two callers.
#[must_use]
pub fn journey_step_description_suffix(node: &IrNode) -> String {
    if !node.classes.iter().any(|class| class == "journey-step") {
        return String::new();
    }

    let mut suffix = String::new();
    if let Some(score) = node
        .classes
        .iter()
        .find_map(|class| class.strip_prefix("journey-score-"))
    {
        suffix.push_str(", score ");
        suffix.push_str(score);
    }
    // ⚠️ THE ACTORS COME FROM `journey_meta`, NOT FROM THE CLASSES (bd-mq273). Deriving them by
    // stripping `journey-actor-` announced the CSS-normalized spelling: an author who wrote
    // `Big Corp` heard `Big_Corp`, because a class name cannot contain a space. That mapping is not
    // reversible either — a genuine underscore is indistinguishable from a normalized one — so the
    // parser records the raw names separately and this reads those.
    let actors: Vec<&str> = node
        .journey_meta
        .as_deref()
        .map(|meta| meta.actors.iter().map(String::as_str).collect())
        .unwrap_or_default();
    if !actors.is_empty() {
        suffix.push_str(", actors: ");
        suffix.push_str(&actors.join(", "));
    }
    suffix
}

/// Generate a text alternative for an edge.
///
/// `co_arrow` carries the far-end marker of a relation marked at BOTH ends; pass
/// `edge.co_arrow()`. Passing `None` for an edge that has one draws a marker nobody is told about.
#[must_use]
pub fn describe_edge(
    from_node: Option<&IrNode>,
    to_node: Option<&IrNode>,
    arrow_type: fm_core::ArrowType,
    co_arrow: Option<fm_core::ArrowType>,
    label: Option<&str>,
    ir: &MermaidDiagramIr,
) -> String {
    let from_label = from_node
        .and_then(|n| {
            n.label
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|l| l.text.as_str())
        })
        .or_else(|| from_node.map(|n| n.id.as_str()))
        .unwrap_or("unknown");

    let to_label = to_node
        .and_then(|n| {
            n.label
                .and_then(|lid| ir.labels.get(lid.0))
                .map(|l| l.text.as_str())
        })
        .or_else(|| to_node.map(|n| n.id.as_str()))
        .unwrap_or("unknown");

    describe_edge_labels(
        Some(from_label),
        Some(to_label),
        arrow_type,
        co_arrow,
        label,
    )
}

pub(crate) fn accessible_node_label<'a>(node: &'a IrNode, ir: &'a MermaidDiagramIr) -> &'a str {
    node.label
        .and_then(|lid| ir.labels.get(lid.0))
        .map(|label| label.text.as_str())
        .unwrap_or(&node.id)
}

/// The verb phrase one `ArrowType` contributes, owner-first.
///
/// Extracted so the FAR end of a relation marked at both ends is described with the same words the
/// near end would have used had that marker been on it (bd-f9t0r). A second table would be a second
/// thing to drift: this crate has already shipped a `<desc>` saying "connects to" while the
/// `<title>` said "is inherited by", because a catch-all silently swallowed the UML phrases
/// (bd-92b6, bd-lkm9i).
fn arrow_phrase(arrow_type: ArrowType) -> &'static str {
    match arrow_type {
        ArrowType::Arrow => "points to",
        ArrowType::ThickArrow => "strongly points to",
        ArrowType::DottedArrow => "optionally points to",
        ArrowType::Circle => "relates to",
        ArrowType::Cross => "blocks",
        ArrowType::ThickLine => "strongly connects to",
        ArrowType::DottedLine => "optionally connects to",
        ArrowType::DoubleArrow => "points both ways to",
        ArrowType::DoubleThickArrow => "strongly points both ways to",
        ArrowType::DoubleDottedArrow => "optionally points both ways to",
        ArrowType::OpenArrow => "sends to",
        ArrowType::DottedOpenArrow => "optionally sends to",
        // UML relationships read owner-first, matching the phrases the SVG fragment writer puts in
        // each edge's `<title>`. Without these the catch-all below said "connects to" here while the
        // title said "is inherited by", so the two accessibility surfaces disagreed.
        ArrowType::Inheritance => "is inherited by",
        ArrowType::InheritanceReverse => "inherits",
        ArrowType::Aggregation => "aggregates",
        ArrowType::AggregationReverse => "is aggregated by",
        ArrowType::Composition => "is composed of",
        ArrowType::CompositionReverse => "composes",
        // Must match the `<title>` phrases the SVG fragment writer emits for the same two variants,
        // for the same reason the six above must: the catch-all is silent, so a missing arm here
        // makes <desc> say "connects to" while <title> says "provides" (bd-lkm9i).
        ArrowType::Lollipop => "provides",
        ArrowType::LollipopReverse => "is provided by",
        _ => "connects to",
    }
}

/// Describe one edge, naming BOTH ends when the relation is marked at both (bd-f9t0r).
///
/// `co_arrow` is the second marker an `o--*`-style class relation carries -- the `ArrowType` that
/// already draws that marker on that end. The two phrases share a subject and an object, so they
/// join with "and" and no new vocabulary is needed: `Alpha o--* Beta` reads "Alpha aggregates and
/// composes Beta". Without this the far diamond is drawn and never spoken, so the picture and the
/// accessible text state different relationships.
///
/// When `co_arrow` is `None` the output is byte-identical to what this produced before, which is
/// what lets the golden SVGs and the fragment writers' byte-identity tests stay untouched.
pub(crate) fn describe_edge_labels(
    from_label: Option<&str>,
    to_label: Option<&str>,
    arrow_type: ArrowType,
    co_arrow: Option<ArrowType>,
    label: Option<&str>,
) -> String {
    let from_label = from_label.unwrap_or("unknown");
    let to_label = to_label.unwrap_or("unknown");
    let arrow_desc = arrow_phrase(arrow_type);
    // Deduplicated on the PHRASE rather than the variant: two different `ArrowType`s could map to
    // the same words through the `_ => "connects to"` catch-all, and "connects to and connects to"
    // is worse than saying it once. No spelling in the fixture reaches that today; this is here so
    // that if one ever does, it degrades to the single-ended phrase instead of stuttering.
    let co_desc = co_arrow
        .map(arrow_phrase)
        .filter(|co_desc| *co_desc != arrow_desc);

    match (co_desc, label) {
        (None, None) => format!("{from_label} {arrow_desc} {to_label}"),
        (None, Some(label_text)) => {
            format!("{from_label} {arrow_desc} {to_label} with label: {label_text}")
        }
        (Some(co_desc), None) => format!("{from_label} {arrow_desc} and {co_desc} {to_label}"),
        (Some(co_desc), Some(label_text)) => {
            format!("{from_label} {arrow_desc} and {co_desc} {to_label} with label: {label_text}")
        }
    }
}

/// Generate accessibility CSS with media query support.
#[must_use]
pub fn accessibility_css() -> &'static str {
    r"
/* High contrast mode support */
@media (prefers-contrast: more) {
  svg, :root {
    --fm-bg: #ffffff !important;
    --fm-text-color: #000000 !important;
    --fm-node-fill: #ffffff !important;
    --fm-node-stroke: #000000 !important;
    --fm-edge-color: #000000 !important;
  }
  .fm-node { stroke-width: 2px !important; }
  .fm-edge { stroke-width: 2px !important; }
}

/* Reduced motion support */
@media (prefers-reduced-motion: reduce) {
  .fm-edge, .fm-node {
    animation: none !important;
    transition: none !important;
  }
}

/* Focus indicators for keyboard navigation */
.fm-node:focus, .fm-edge:focus {
  outline: 3px solid #0066cc;
  outline-offset: 2px;
}

.fm-node:focus-visible, .fm-edge:focus-visible {
  outline: 3px solid #0066cc;
  outline-offset: 2px;
}

/* Screen reader only content */
.fm-sr-only {
  position: absolute;
  width: 1px;
  height: 1px;
  padding: 0;
  margin: -1px;
  overflow: hidden;
  clip: rect(0, 0, 0, 0);
  white-space: nowrap;
  border: 0;
}
"
}

/// Configuration for accessibility features.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct A11yConfig {
    /// Whether to add ARIA attributes to elements.
    pub aria_labels: bool,
    /// Whether to add title elements for text alternatives.
    pub text_alternatives: bool,
    /// Whether to make elements keyboard-focusable.
    pub keyboard_nav: bool,
    /// Whether to include accessibility CSS (high contrast, reduced motion).
    pub accessibility_css: bool,
}

impl A11yConfig {
    /// Full accessibility features enabled.
    #[must_use]
    pub const fn full() -> Self {
        Self {
            aria_labels: true,
            text_alternatives: true,
            keyboard_nav: true,
            accessibility_css: true,
        }
    }

    /// Minimal accessibility (just ARIA labels).
    #[must_use]
    pub const fn minimal() -> Self {
        Self {
            aria_labels: true,
            text_alternatives: false,
            keyboard_nav: false,
            accessibility_css: false,
        }
    }

    /// No accessibility features.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            aria_labels: false,
            text_alternatives: false,
            keyboard_nav: false,
            accessibility_css: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, GraphDirection, MermaidDiagramIr, NodeShape};

    fn create_test_ir() -> MermaidDiagramIr {
        let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
        ir.direction = GraphDirection::LR;

        // Add test nodes
        ir.nodes.push(IrNode {
            id: "A".to_string(),
            label: None,
            shape: NodeShape::Rect,
            ..Default::default()
        });
        ir.nodes.push(IrNode {
            id: "B".to_string(),
            label: None,
            shape: NodeShape::Diamond,
            ..Default::default()
        });

        ir
    }

    #[test]
    fn describe_diagram_includes_counts() {
        let ir = create_test_ir();
        let desc = describe_diagram(&ir);
        assert!(desc.contains("2 nodes"));
        assert!(desc.contains("0 edges"));
        assert!(desc.contains("flowchart"));
        assert!(desc.contains("left to right"));
    }

    #[test]
    fn describe_diagram_with_layout_mentions_relationships_and_layout() {
        use fm_core::{ArrowType, IrEdge, IrEndpoint, IrNodeId};

        let mut ir = create_test_ir();
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(0)),
            to: IrEndpoint::Node(IrNodeId(1)),
            arrow: ArrowType::Arrow,
            ..Default::default()
        });

        let layout = fm_layout::layout_diagram(&ir);
        let desc = describe_diagram_with_layout(&ir, Some(&layout));
        assert!(desc.contains("Key nodes"));
        assert!(desc.contains("Key relationships"));
        assert!(desc.contains("Layout spans"));
        assert!(
            !desc.contains(".."),
            "sentence joins must not produce doubled punctuation: {desc}"
        );
        assert!(desc.ends_with('.'));
        assert!(desc.contains("1 edge."));
    }

    #[test]
    fn describe_diagram_pluralizes_singular_counts() {
        let mut ir = create_test_ir();
        ir.nodes.truncate(1);
        let desc = describe_diagram(&ir);

        assert!(desc.contains("1 node and 0 edges"), "{desc}");
        assert!(!desc.contains("1 nodes"), "{desc}");
        assert!(desc.ends_with('.'));
    }

    #[test]
    fn describe_node_includes_shape() {
        let ir = create_test_ir();
        let node = &ir.nodes[0];
        let desc = describe_node(node, &ir);
        assert!(desc.contains('A'));
        assert!(desc.contains("rectangle"));
    }

    #[test]
    fn describe_node_diamond_shape() {
        let ir = create_test_ir();
        let node = &ir.nodes[1];
        let desc = describe_node(node, &ir);
        assert!(desc.contains('B'));
        assert!(desc.contains("diamond"));
    }

    #[test]
    fn describe_edge_with_label() {
        let ir = create_test_ir();
        let desc = describe_edge(
            Some(&ir.nodes[0]),
            Some(&ir.nodes[1]),
            fm_core::ArrowType::Arrow,
            None,
            Some("Submit"),
            &ir,
        );
        assert!(desc.contains('A'));
        assert!(desc.contains('B'));
        assert!(desc.contains("points to"));
        assert!(desc.contains("Submit"));
    }

    #[test]
    fn describe_edge_without_label() {
        let ir = create_test_ir();
        let desc = describe_edge(
            Some(&ir.nodes[0]),
            Some(&ir.nodes[1]),
            fm_core::ArrowType::Line,
            None,
            None,
            &ir,
        );
        assert!(desc.contains("connects to"));
        assert!(!desc.contains("with label"));
    }

    /// Every co-arrow must be SPOKEN, using the same word the near end would have used (bd-f9t0r).
    ///
    /// Reads the real phrase table via `arrow_phrase` rather than restating it: a test that spells
    /// out its own expected strings passes when the table drifts, which is how a `<desc>` saying
    /// "connects to" survived beside a `<title>` saying "is inherited by" (bd-92b6, bd-lkm9i).
    /// The six here are the entire co-arrow range produced by `class_relation_co_arrow`.
    #[test]
    fn a_far_end_marker_is_named_with_the_word_its_own_end_would_use() {
        use fm_core::ArrowType;
        let range = [
            (ArrowType::Aggregation, ArrowType::AggregationReverse),
            (ArrowType::Aggregation, ArrowType::CompositionReverse),
            (ArrowType::Composition, ArrowType::LollipopReverse),
            (ArrowType::Inheritance, ArrowType::InheritanceReverse),
            (ArrowType::Lollipop, ArrowType::InheritanceReverse),
            (ArrowType::Composition, ArrowType::Arrow),
            (ArrowType::Aggregation, ArrowType::DottedArrow),
        ];
        let mut silent = Vec::new();
        for (primary, co) in range {
            let desc = describe_edge_labels(Some("Alpha"), Some("Beta"), primary, Some(co), None);
            let near = arrow_phrase(primary);
            let far = arrow_phrase(co);
            if desc != format!("Alpha {near} and {far} Beta") {
                silent.push(format!("  {primary:?} + {co:?} -> {desc:?}"));
            }
        }
        assert!(
            silent.is_empty(),
            "{} pair(s) did not name both ends:\n{}",
            silent.len(),
            silent.join("\n")
        );
    }

    /// A `None` co-arrow must leave the description BYTE-IDENTICAL to what it was before.
    ///
    /// This is what lets the golden SVGs and the fragment writers' byte-identity tests stay
    /// untouched by this change; if it ever fails, every unrelated golden is about to move.
    #[test]
    fn an_edge_without_a_far_marker_reads_exactly_as_before() {
        use fm_core::ArrowType;
        assert_eq!(
            describe_edge_labels(Some("A"), Some("B"), ArrowType::Arrow, None, None),
            "A points to B"
        );
        assert_eq!(
            describe_edge_labels(Some("A"), Some("B"), ArrowType::Line, None, Some("go")),
            "A connects to B with label: go"
        );
        // A co-arrow whose phrase equals the primary's degrades to the single-ended sentence
        // rather than stuttering "connects to and connects to".
        assert_eq!(
            describe_edge_labels(
                Some("A"),
                Some("B"),
                ArrowType::Arrow,
                Some(ArrowType::Arrow),
                None
            ),
            "A points to B"
        );
    }

    #[test]
    fn cached_edge_labels_match_node_lookup_description() -> Result<(), &'static str> {
        let ir = create_test_ir();
        let from_node = ir.nodes.first().ok_or("missing from node")?;
        let to_node = ir.nodes.get(1).ok_or("missing to node")?;
        let direct = describe_edge(
            Some(from_node),
            Some(to_node),
            fm_core::ArrowType::Arrow,
            None,
            Some("Submit"),
            &ir,
        );
        let cached = describe_edge_labels(
            Some(accessible_node_label(from_node, &ir)),
            Some(accessible_node_label(to_node, &ir)),
            fm_core::ArrowType::Arrow,
            None,
            Some("Submit"),
        );

        assert_eq!(direct, cached);
        Ok(())
    }

    #[test]
    fn accessibility_css_includes_media_queries() {
        let css = accessibility_css();
        assert!(css.contains("prefers-contrast"));
        assert!(css.contains("prefers-reduced-motion"));
    }

    #[test]
    fn accessibility_css_includes_focus_indicators() {
        let css = accessibility_css();
        assert!(css.contains(":focus"));
        assert!(css.contains("outline"));
    }

    #[test]
    fn a11y_config_full_enables_all() {
        let config = A11yConfig::full();
        assert!(config.aria_labels);
        assert!(config.text_alternatives);
        assert!(config.keyboard_nav);
        assert!(config.accessibility_css);
    }

    #[test]
    fn a11y_config_none_disables_all() {
        let config = A11yConfig::none();
        assert!(!config.aria_labels);
        assert!(!config.text_alternatives);
        assert!(!config.keyboard_nav);
        assert!(!config.accessibility_css);
    }
}
