//! `A:::className` must actually style the node it names (bd-8n2b5).
//!
//! THE DEFECT: the `:::` shorthand was parsed, its position located by `find_triple_colon`, used to
//! truncate the raw token so the id could be read — and then thrown away. `NodeToken` had no field
//! to put it in. So the author's `classDef` rule was emitted into the stylesheet, the node never
//! received the class, and the rule styled NOTHING. A dead CSS rule is the worst shape this can
//! take: the diagram looks deliberate, the stylesheet "proves" the feature works, and the only way
//! to see the bug is to check that some ELEMENT actually carries the selector.
//!
//! The sibling statement form `class A urgent` worked the whole time, through a different path
//! (`FlowAst::ClassAssign` -> `add_class_to_node`). That asymmetry is what located the fault.
//!
//! ⚠️ ASSERT ON THE ELEMENT, NEVER ON THE DOCUMENT. `svg.contains("fm-node-user-urgent")` passes on
//! the BROKEN renderer, because the emitted `classDef` rule contains that exact substring. Every
//! test here reads the class attribute of the node's own `<g>`.

/// The `class="…"` of the `<g>` whose `id` starts with `fm-node-{id}-`.
///
/// Returns `None` when no such group exists, which the tests treat as a CONTROL failure rather than
/// a missing class — the two are different verdicts and must not be conflated.
fn node_classes(svg: &str, node_id: &str) -> Option<String> {
    let needle = format!("<g id=\"fm-node-{node_id}-");
    let start = svg.find(&needle)?;
    let rest = &svg[start..];
    let end = rest.find('>')?;
    let open_tag = &rest[..end];
    let class_at = open_tag.find("class=\"")? + "class=\"".len();
    let class_rest = &open_tag[class_at..];
    let close = class_rest.find('"')?;
    Some(class_rest[..close].to_string())
}

fn render(source: &str) -> String {
    fm_render_svg::render_svg(&fm_parser::parse(source).ir)
}

fn assert_node_has_class(source: &str, node_id: &str, class: &str) {
    let svg = render(source);
    let classes = node_classes(&svg, node_id)
        .unwrap_or_else(|| panic!("CONTROL FAILED: no <g> for {node_id}"));
    assert!(
        classes.split_whitespace().any(|c| c == class),
        "node {node_id} never received `{class}`; its classes were `{classes}`"
    );
}

fn assert_node_lacks_class(source: &str, node_id: &str, class: &str) {
    let svg = render(source);
    let classes = node_classes(&svg, node_id)
        .unwrap_or_else(|| panic!("CONTROL FAILED: no <g> for {node_id}"));
    assert!(
        !classes.split_whitespace().any(|c| c == class),
        "node {node_id} gained `{class}` and should not have; its classes were `{classes}`"
    );
}

/// THE DEFECT, in the position the incumbent confirms is legal: `A:::urgent --> B`.
///
/// mermaid 11.15.0 PARSES this exact source (`parse_probe.mjs` -> PARSED), so the shorthand is not
/// an extension we are free to ignore.
#[test]
fn an_inline_class_on_an_edge_endpoint_reaches_the_node() {
    assert_node_has_class(
        "flowchart LR\n  A:::urgent --> B\nclassDef urgent fill:#ff0000\n",
        "a",
        "fm-node-user-urgent",
    );
}

/// The same suffix on a BARE declaration, which takes a different exit from the statement parser.
#[test]
fn an_inline_class_on_a_bare_declaration_reaches_the_node() {
    assert_node_has_class(
        "flowchart LR\n  A[Alpha]:::urgent\n  A --> B\nclassDef urgent fill:#ff0000\n",
        "a",
        "fm-node-user-urgent",
    );
}

/// And on a LABELLED endpoint, the third distinct route to the same token parser.
#[test]
fn an_inline_class_on_a_labelled_endpoint_reaches_the_node() {
    assert_node_has_class(
        "flowchart LR\n  A[Alpha]:::urgent --> B\nclassDef urgent fill:#ff0000\n",
        "a",
        "fm-node-user-urgent",
    );
}

/// mermaid's `setClass` splits the suffix on whitespace, so `:::alpha beta` is TWO classes.
#[test]
fn an_inline_suffix_may_name_several_classes() {
    let source = "flowchart LR\n  A:::alpha beta --> B\nclassDef alpha fill:#ff0000\nclassDef beta stroke:#00ff00\n";
    assert_node_has_class(source, "a", "fm-node-user-alpha");
    assert_node_has_class(source, "a", "fm-node-user-beta");
}

/// The statement form must keep working — it is the sibling that was never broken, and a fix that
/// traded one for the other would be no fix at all.
#[test]
fn the_class_statement_form_still_works() {
    assert_node_has_class(
        "flowchart LR\n  A --> B\nclass A urgent\nclassDef urgent fill:#ff0000\n",
        "a",
        "fm-node-user-urgent",
    );
}

/// ⚠️ CONTROL: `:::` INSIDE AN EDGE LABEL IS NOT A CLASS.
///
/// `A -->|x:::y| B` is a label containing a colon run. A naive implementation that scanned the
/// whole statement for `:::` would invent a class `y` and attach it to whichever node it guessed.
/// Endpoint tokens are split at the operator before the suffix is read, so the label is never seen.
#[test]
fn a_triple_colon_inside_an_edge_label_creates_no_class() {
    let source = "flowchart LR\n  A -->|x:::y| B\nclassDef y fill:#ff0000\n";
    assert_node_lacks_class(source, "a", "fm-node-user-y");
    assert_node_lacks_class(source, "b", "fm-node-user-y");
}

/// CONTROL: in an `&` node list the suffix binds to ITS OWN member, not to the whole list.
///
/// `A & B:::urgent --> C` styles B alone. Applying it to every member of the list — or to the first
/// — would pass a single-node test and quietly recolour unrelated nodes.
#[test]
fn an_inline_class_in_a_node_list_binds_only_to_its_own_member() {
    let source = "flowchart LR\n  A & B:::urgent --> C\nclassDef urgent fill:#ff0000\n";
    assert_node_has_class(source, "b", "fm-node-user-urgent");
    assert_node_lacks_class(source, "a", "fm-node-user-urgent");
    assert_node_lacks_class(source, "c", "fm-node-user-urgent");
}

/// CONTROL: a node with no suffix gains no class, so the emission is not unconditional.
#[test]
fn a_node_without_a_suffix_gains_no_user_class() {
    let svg = render("flowchart LR\n  A --> B\nclassDef urgent fill:#ff0000\n");
    let classes = node_classes(&svg, "a").expect("CONTROL FAILED: no <g> for a");
    assert!(
        !classes.contains("fm-node-user-"),
        "an unstyled node picked up a user class: `{classes}`"
    );
}

/// ⚠️ THE CLAIM MY CODE COMMENT MAKES, PINNED.
///
/// `apply_inline_classes` is deliberately absent from the two FAST document paths, on the grounds
/// that a `:::` token can never reach them. That is a load-bearing assumption — if a fast path ever
/// accepts `:`, the shorthand would start working in some positions and silently vanish in others,
/// which is precisely the bug this bead fixes. This asserts the OBSERVABLE consequence across the
/// statement shapes the fast paths handle, so the claim cannot rot into a comment that is merely
/// no longer true.
#[test]
fn a_class_suffix_never_takes_a_fast_path() {
    // Bare id edge, bare id declaration, and a bracketed label — the three fast-path shapes.
    for source in [
        "flowchart LR\n  A:::urgent --> B\nclassDef urgent fill:#ff0000\n",
        "flowchart LR\n  A:::urgent\n  A --> B\nclassDef urgent fill:#ff0000\n",
        "flowchart LR\n  A[Alpha]:::urgent --> B\nclassDef urgent fill:#ff0000\n",
    ] {
        let svg = render(source);
        let classes = node_classes(&svg, "a")
            .unwrap_or_else(|| panic!("CONTROL FAILED: no <g> for a in:\n{source}"));
        assert!(
            classes
                .split_whitespace()
                .any(|c| c == "fm-node-user-urgent"),
            "a fast path swallowed the `:::` suffix for:\n{source}\nclasses were `{classes}`"
        );
    }
}

/// The id must stop at the suffix: `A:::urgent` is node `A`, never a node literally named
/// `A:::urgent`. Interning the whole token is the phantom-node failure this repo has hit repeatedly.
#[test]
fn the_suffix_is_not_part_of_the_node_id() {
    let ir =
        fm_parser::parse("flowchart LR\n  A:::urgent --> B\nclassDef urgent fill:#ff0000\n").ir;
    let ids: Vec<&str> = ir.nodes.iter().map(|node| node.id.as_str()).collect();
    assert_eq!(
        ids.len(),
        2,
        "the `:::` suffix minted an extra node: {ids:?}"
    );
    assert!(
        !ids.iter().any(|id| id.contains(':')),
        "a node id kept its class suffix: {ids:?}"
    );
}
