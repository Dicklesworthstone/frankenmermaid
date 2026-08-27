//! Differential test: the `<<type>>` label mermaid draws on a C4 element.
//!
//! THE DIVERGENCE THIS PINS, and it was information loss rather than a spelling difference. Our
//! parser collapsed all twenty C4 macro spellings into four base types, so
//!
//! ```text
//!   System(web, "Web App", …)          drew  <<System>>
//!   System_Ext(email, "Email System", …) drew <<System>>
//! ```
//!
//! An external system and an internal one rendered IDENTICALLY. The same held for every `Db` and
//! `Queue` variant.
//!
//! REFERENCE. mermaid's grammar calls `addPersonOrSystem("external_system", …)` for `System_Ext`,
//! and its renderer draws the type VERBATIM:
//!
//! ```text
//!   .text("<<" + t.typeC4Shape.text + ">>")
//! ```
//!
//! so the twenty spellings produce twenty distinct labels — `person`, `external_person`, `system`,
//! `external_system`, `system_db`, `system_queue`, and the matching `container_*` / `component_*`
//! sets. Extracted from the pinned 11.15.0 bundle.
//!
//! ⚠️ NOTE THE CASE. mermaid draws the raw snake_case token, not a prettified one — `<<person>>`,
//! not `<<Person>>`. That is the opposite of the requirement diagram, where mermaid maps the
//! keyword through a display table before drawing it. Two families, two conventions, both measured
//! rather than assumed.
//!
//! ⚠️ HOW IT SURVIVED: C4 has no head-to-head corpus item and does not render under jsdom, so
//! neither `equivalence.mjs` nor `drawn_text_diff.mjs` covers it. Found by reading the bundle's
//! grammar and comparing with our own output.

fn type_labels(source: &str) -> Vec<String> {
    let svg = fm_render_svg::render_svg(&fm_parser::parse(source).ir);
    let mut out = Vec::new();
    let mut rest = svg.as_str();
    while let Some(start) = rest.find("<text") {
        rest = &rest[start..];
        let Some(open) = rest.find('>') else { break };
        let Some(close) = rest.find("</text>") else { break };
        let text = rest[open + 1..close]
            .replace("&lt;", "<")
            .replace("&gt;", ">");
        let text = text.trim();
        if text.starts_with("<<") && text.ends_with(">>") {
            out.push(text.to_string());
        }
        rest = &rest[close + "</text>".len()..];
    }
    out
}

/// Every macro spelling and the type string mermaid draws for it.
///
/// NOT transcribed from the bundle's theme config, which contains all twenty `<type>FontSize` keys
/// and therefore reveals that twenty type strings EXIST without revealing which macro yields which.
/// Each pair below was read out of the live incumbent's own diagram db, one parse per macro, by
/// `scripts/headtohead/c4_element_battery.mjs` — which reports 20/20 AGREE against this renderer.
const TABLE: &[(&str, &str)] = &[
    ("Person", "person"),
    ("Person_Ext", "external_person"),
    ("System", "system"),
    ("System_Ext", "external_system"),
    ("SystemDb", "system_db"),
    ("SystemDb_Ext", "external_system_db"),
    ("SystemQueue", "system_queue"),
    ("SystemQueue_Ext", "external_system_queue"),
    ("Container", "container"),
    ("Container_Ext", "external_container"),
    ("ContainerDb", "container_db"),
    ("ContainerDb_Ext", "external_container_db"),
    ("ContainerQueue", "container_queue"),
    ("ContainerQueue_Ext", "external_container_queue"),
    ("Component", "component"),
    ("Component_Ext", "external_component"),
    ("ComponentDb", "component_db"),
    ("ComponentDb_Ext", "external_component_db"),
    ("ComponentQueue", "component_queue"),
    ("ComponentQueue_Ext", "external_component_queue"),
];

fn diagram(macro_name: &str) -> String {
    format!("C4Context\n    title T\n    {macro_name}(a, \"A\", \"d\")\n")
}

#[test]
fn every_spelling_draws_its_own_type() {
    for (macro_name, expected) in TABLE {
        let drawn = type_labels(&diagram(macro_name));
        let want = format!("<<{expected}>>");
        assert!(
            drawn.iter().any(|label| label == &want),
            "{macro_name} must draw {want:?}; drew {drawn:?}"
        );
    }
}

/// ⚠️ THE NEGATIVE CONTROL, and the defect itself: a variant must not render as its base type.
/// Collapsing `System_Ext` to `System` makes an external system indistinguishable from an internal
/// one, which is exactly what shipped.
#[test]
fn a_variant_is_not_collapsed_into_its_base_type() {
    for (macro_name, base) in [
        ("System_Ext", "system"),
        ("SystemDb", "system"),
        ("SystemQueue", "system"),
        ("ContainerDb", "container"),
        ("Component_Ext", "component"),
    ] {
        let drawn = type_labels(&diagram(macro_name));
        assert!(
            !drawn.iter().any(|label| label == &format!("<<{base}>>")),
            "{macro_name} collapsed to the base type <<{base}>>: {drawn:?}"
        );
    }
}

/// ⚠️ CONTROL for the CASE. mermaid draws the raw token, so a title-cased `<<Person>>` — which is
/// what shipped, and which reads perfectly plausibly — is wrong.
#[test]
fn the_type_is_lowercase_as_mermaid_draws_it() {
    for (macro_name, _) in TABLE {
        let drawn = type_labels(&diagram(macro_name));
        for label in &drawn {
            assert!(
                !label.chars().any(char::is_uppercase),
                "{macro_name} drew a title-cased type {label:?}; mermaid draws the raw token"
            );
        }
    }
}

/// CONTROL: two DIFFERENT spellings must not share a label, checked pairwise so the table above
/// cannot be satisfied by a mapping that happens to be constant.
#[test]
fn no_two_spellings_share_a_label() {
    let mut seen = std::collections::BTreeMap::new();
    for (macro_name, _) in TABLE {
        let drawn = type_labels(&diagram(macro_name));
        let label = drawn.first().cloned().unwrap_or_default();
        if let Some(other) = seen.insert(label.clone(), *macro_name) {
            panic!("{macro_name} and {other} both draw {label:?}");
        }
    }
    assert_eq!(seen.len(), TABLE.len(), "some spelling drew no type label");
}
