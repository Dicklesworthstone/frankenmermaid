//! A treemap's rectangle AREAS must be proportional to its values (bd-9ghyo).
//!
//! This is the invariant the whole diagram type exists to communicate: a reader compares two
//! rectangles by eye and concludes something about two numbers. An implementation that laid the
//! tiles out in a row of EQUAL size would still look like a treemap, still nest correctly, still
//! label every tile — and would say something false about every value in the document. So the
//! proportionality is asserted numerically here rather than left to the SVG-level tests.
//!
//! Reference geometry MEASURED from the pinned mermaid 11.15.0 bundle in Chromium 151
//! (`scratchpad/treemap_probe.mjs`): two leaves of 10 and 20 tiled into a 960x310 box came out
//! 313x310 and 637x310 — areas 97030 and 197470, a ratio of 2.035 against the nominal 2.0, the
//! excess being the 10px gutter that sits between them.

use fm_layout::layout_diagram;

fn tiles(source: &str) -> Vec<(String, f64, f32, f32, f32, f32)> {
    let ir = fm_parser::parse(source).ir;
    let layout = layout_diagram(&ir);
    let meta = ir.treemap_meta.as_ref().expect("no treemap_meta");
    layout
        .extensions
        .treemap_tiles
        .iter()
        .map(|tile| {
            let item = &meta.nodes[tile.node];
            (
                item.label.clone(),
                tile.value,
                tile.bounds.x,
                tile.bounds.y,
                tile.bounds.width,
                tile.bounds.height,
            )
        })
        .collect()
}

fn area_of(t: &(String, f64, f32, f32, f32, f32)) -> f64 {
    f64::from(t.4) * f64::from(t.5)
}

fn find<'a>(
    all: &'a [(String, f64, f32, f32, f32, f32)],
    label: &str,
) -> &'a (String, f64, f32, f32, f32, f32) {
    all.iter()
        .find(|t| t.0 == label)
        .unwrap_or_else(|| panic!("no tile labelled {label}: {all:?}"))
}

/// A treemap reaches the treemap layout, not the general graph one.
///
/// Asserted on `treemap_tiles` rather than on a selector return value: no other algorithm in the
/// crate writes that extension, so a non-empty one IS the evidence that this diagram was tiled
/// rather than routed. It is also the artifact the renderer actually consumes, which a selector
/// enum is not.
#[test]
fn a_treemap_reaches_the_treemap_layout() {
    let ir = fm_parser::parse("treemap\n\"R\"\n    \"a\": 1\n").ir;
    assert_eq!(ir.diagram_type, fm_core::DiagramType::Treemap);
    let layout = layout_diagram(&ir);
    assert!(
        !layout.extensions.treemap_tiles.is_empty(),
        "a treemap produced no tiles, so it fell through to the general graph selector"
    );
    assert!(
        layout.nodes.is_empty(),
        "a treemap produced graph node boxes, which nothing draws"
    );
}

/// THE INVARIANT: leaf area is proportional to leaf value.
///
/// Stated as a RATIO between two leaves rather than as absolute areas, so it is independent of the
/// canvas size and of how much padding a section takes — the things that may legitimately change —
/// while remaining sensitive to the thing that must not.
#[test]
fn leaf_area_is_proportional_to_value() {
    let all = tiles("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n");
    let a = find(&all, "A");
    let b = find(&all, "B");
    let ratio = area_of(b) / area_of(a);
    assert!(
        (ratio - 2.0).abs() < 0.12,
        "B is worth 2x A but covers {ratio:.3}x the area: A={:?} B={:?}",
        a,
        b
    );
}

/// THE NEGATIVE CASE: equal-area tiling is the wrong answer, and it must be detectable.
///
/// A tiler that ignored values entirely would give every sibling the same rectangle. Two documents
/// that differ ONLY in their values must therefore produce different geometry — an assertion that
/// no value-blind implementation can pass, however plausible its output looks.
#[test]
fn different_values_produce_different_geometry() {
    let uneven = tiles("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 20\n");
    let even = tiles("treemap\n\"Root\"\n    \"A\": 10\n    \"B\": 10\n");

    let (ea, eb) = (find(&even, "A"), find(&even, "B"));
    assert!(
        (area_of(ea) - area_of(eb)).abs() / area_of(ea) < 0.02,
        "equal values did not produce equal areas: {ea:?} {eb:?}"
    );

    let (ua, ub) = (find(&uneven, "A"), find(&uneven, "B"));
    assert!(
        (area_of(ua) - area_of(ub)).abs() / area_of(ua) > 0.5,
        "unequal values produced near-equal areas, so the tiler is value-blind: {ua:?} {ub:?}"
    );
}

/// Fractional values are honoured, not truncated to integers.
#[test]
fn fractional_values_are_honoured() {
    let all = tiles("treemap\n\"Root\"\n    \"A\": 10.5\n    \"B\": 4.25\n");
    let ratio = area_of(find(&all, "A")) / area_of(find(&all, "B"));
    let expected = 10.5 / 4.25;
    assert!(
        (ratio - expected).abs() / expected < 0.10,
        "expected area ratio ~{expected:.3}, got {ratio:.3}"
    );
}

/// A child is drawn strictly INSIDE its parent — the property that makes nesting readable.
///
/// Checked against the parent's own rectangle rather than against the canvas, because containment
/// in the canvas is satisfied by a flat layout that has lost the hierarchy entirely.
#[test]
fn every_child_is_contained_by_its_parent() {
    let source = "treemap\n\"R\"\n    \"G1\"\n        \"a\": 10\n        \"b\": 20\n    \"G2\"\n        \"c\": 30\n";
    let ir = fm_parser::parse(source).ir;
    let layout = layout_diagram(&ir);
    let meta = ir.treemap_meta.as_ref().expect("no treemap_meta");

    let mut seen = std::collections::HashMap::new();
    for tile in &layout.extensions.treemap_tiles {
        seen.insert(tile.node, tile.bounds);
    }
    assert!(seen.len() >= 6, "expected 6 tiles, got {}", seen.len());

    for tile in &layout.extensions.treemap_tiles {
        let Some(parent) = meta.nodes[tile.node].parent else {
            continue;
        };
        let outer = seen[&parent];
        let inner = tile.bounds;
        assert!(
            inner.x >= outer.x - 0.01
                && inner.y >= outer.y - 0.01
                && inner.x + inner.width <= outer.x + outer.width + 0.01
                && inner.y + inner.height <= outer.y + outer.height + 0.01,
            "{} escapes its parent {}: child={inner:?} parent={outer:?}",
            meta.nodes[tile.node].label,
            meta.nodes[parent].label
        );
    }
}

/// A section's value is the DEEP sum of its descendants.
///
/// The discriminating case is `G` and `H`, which have no leaf children at all — a shallow
/// `children.map(value).sum()` reports 0 for both, and upstream measurably shows 5 on every header.
#[test]
fn a_section_value_is_the_deep_sum_of_its_descendants() {
    let ir =
        fm_parser::parse("treemap\n\"R\"\n    \"G\"\n        \"H\"\n            \"x\": 5\n").ir;
    let meta = ir.treemap_meta.as_ref().expect("no treemap_meta");
    for (index, item) in meta.nodes.iter().enumerate() {
        assert!(
            (meta.value_of(index) - 5.0).abs() < f64::EPSILON,
            "{} reports {} rather than the descendant sum 5",
            item.label,
            meta.value_of(index)
        );
    }
}

/// Siblings are tiled LARGEST FIRST, which is what upstream draws.
#[test]
fn siblings_are_tiled_largest_first() {
    let all = tiles("treemap\n\"Root\"\n    \"small\": 1\n    \"big\": 100\n");
    let leaves: Vec<&str> = all
        .iter()
        .filter(|t| t.0 != "Root")
        .map(|t| t.0.as_str())
        .collect();
    assert_eq!(
        leaves,
        vec!["big", "small"],
        "source order was preserved instead of sorting by value"
    );
}
