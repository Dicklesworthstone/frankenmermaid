//! `subgraph "Quoted Title"` is a TITLE, not an identifier (bd-chz77).
//!
//! THE DEFECT. A subgraph body that is entirely one quoted string fell through to the id/title
//! splitter, which took the quoted text as the ID and found no remainder to use as a title. The
//! cluster was then captioned with `normalize_identifier`'s output:
//!
//! ```text
//!   subgraph "Plain Title"    drew `Plain_Title`   — spaces turned into underscores
//!   subgraph "x&amp;y"        drew `x`             — the entity mangled by an id normalizer
//! ```
//!
//! The reference draws `Plain Title` and `x&y`. `subgraph one["Plain Title"]` was always right,
//! because it takes the explicit-label branch — which is why this went unnoticed, and why the
//! bracketed form is compared here as the control that says the two spellings now agree.
//!
//! ⚠️ THE TITLE AND THE KEY ARE DIFFERENT THINGS AND BOTH ARE KEPT. A cluster needs a key, so it is
//! still derived from the same text — `subgraph "Sub"` stays addressable as `Sub`. What changed is
//! that the TITLE now goes through `normalize_subgraph_title`, which is `clean_label`, so it decodes
//! entities and keeps its spaces like every other drawn label. A fix that dropped the key would pass
//! every "is the title right" assertion and break every directive that names the subgraph.
//!
//! ⚠️ FOUND BY A PIN, NOT BY A NEW SWEEP. bd-rbwov's
//! `a_quoted_subgraph_title_is_still_id_normalized` asserted the BROKEN value so this follow-up
//! could not land quietly; it failed with an instruction to update it. That test is now this file's
//! subject, and the note it carried has moved here with it.
//!
//! ⚠️ ONE NEGATIVE-CONTROL ARM IS INERT, AND IT IS REPORTED RATHER THAN PAPERED OVER. Replacing
//! `normalize_subgraph_title(&quoted)` with the raw quoted string changes NO test — because the
//! entity decode is applied again downstream when the title is interned, so at this call site that
//! helper is belt-and-braces rather than load-bearing. Its one distinct behaviour is unwrapping a
//! `[...]`/`(...)` pair, and the reference renders no text at all for `subgraph "[Plain]"`, so there
//! is nothing to match and no honest test to write. The call is kept because it is the right
//! function for a subgraph title and matches the sibling call below it; what is NOT claimed is that
//! this file proves it.

const ARROW: &str = "-->";

fn cluster_titles(source: &str) -> Vec<String> {
    let parsed = fm_parser::parse(source);
    let ir = &parsed.ir;
    ir.clusters
        .iter()
        .map(|c| {
            c.title
                .and_then(|id| ir.labels.get(id.0))
                .map_or_else(String::new, |l| l.text.clone())
        })
        .collect()
}

/// Whether a `style` directive naming `key` RESOLVED against a subgraph.
///
/// ⚠️ THE DERIVED KEY IS NOT IN THE IR, so it cannot be read directly — `IrCluster::id` is an index
/// and the string key lives only in the builder's lookup. What the key is FOR is resolution, so this
/// asks the thing that depends on it: an unresolved target warns ("matches no node or subgraph id"),
/// a resolved one does not. `the_cluster_still_has_a_usable_derived_key` carries its own control
/// proving that warning still fires for a genuinely unknown target.
fn style_resolves(source: &str, key: &str) -> bool {
    !fm_parser::parse(source)
        .warnings
        .iter()
        .any(|w| w.contains("matches no node or subgraph id") && w.contains(key))
}

/// ⚠️ THE NEGATIVE CASE: the title keeps its own text, and the cluster still exists.
///
/// "The title is not `Plain_Title`" passes if the subgraph was dropped entirely, or if the title
/// went empty. The exact expected title is asserted, and the cluster count with it.
#[test]
fn a_quoted_subgraph_body_is_a_title_not_an_identifier() {
    for (raw, expected) in [
        ("\"Plain Title\"", "Plain Title"),
        ("\"x&amp;y\"", "x&y"),
        ("\"a &lt; b\"", "a < b"),
        ("\"One\"", "One"),
    ] {
        let source = format!("flowchart LR\n  subgraph {raw}\n    A {ARROW} B\n  end\n");
        let titles = cluster_titles(&source);
        assert_eq!(
            titles,
            vec![expected.to_string()],
            "subgraph {raw} did not keep its title"
        );
    }
}

/// ⚠️ AND THE CLUSTER KEEPS A USABLE KEY, which the title assertions cannot see.
#[test]
fn the_cluster_still_has_a_usable_derived_key() {
    let titled = format!(
        "flowchart LR\n  subgraph \"Plain Title\"\n    A {ARROW} B\n  end\n  style Plain_Title fill:#f9f\n"
    );
    assert!(
        style_resolves(&titled, "Plain_Title"),
        "the derived key is gone, so nothing can style the subgraph"
    );

    let simple = format!(
        "flowchart LR\n  subgraph \"Sub\"\n    x {ARROW} y\n  end\n  style Sub fill:#f9f\n"
    );
    assert!(
        style_resolves(&simple, "Sub"),
        "a single-word quoted subgraph is no longer addressable as itself"
    );

    // CONTROL: a key that really does not exist STILL warns, so the two assertions above are about
    // resolution rather than about the warning having been switched off.
    let bogus = format!(
        "flowchart LR\n  subgraph \"Sub\"\n    x {ARROW} y\n  end\n  style NoSuchThing fill:#f9f\n"
    );
    assert!(
        !style_resolves(&bogus, "NoSuchThing"),
        "an unknown style target stopped warning, so this test cannot detect a lost key"
    );
}

/// The bracketed spelling and the quoted one now agree.
///
/// They are the same diagram written two ways, and the whole defect was that only one of them was
/// right. Comparing them makes this a statement about the two paths rather than about one literal.
#[test]
fn the_quoted_and_bracketed_spellings_agree() {
    for title in ["Plain Title", "x&amp;y", "One"] {
        let quoted = cluster_titles(&format!(
            "flowchart LR\n  subgraph \"{title}\"\n    A {ARROW} B\n  end\n"
        ));
        let bracketed = cluster_titles(&format!(
            "flowchart LR\n  subgraph one[\"{title}\"]\n    A {ARROW} B\n  end\n"
        ));
        assert_eq!(
            quoted, bracketed,
            "the quoted and bracketed spellings of {title:?} still disagree"
        );
    }
}

/// ⚠️ THE TWO-FIELD FORM IS UNTOUCHED: `subgraph id "Title"` still splits.
///
/// The new branch fires only when the quoted string is the WHOLE body. If it fired on any leading
/// quote it would swallow the title of `subgraph "id" "Title"` and lose one of the two fields — the
/// failure a single-fixture test would not see.
#[test]
fn the_two_field_form_still_splits_id_from_title() {
    for (source, key, title) in [
        (
            format!(
                "flowchart LR\n  subgraph one \"Plain Title\"\n    A {ARROW} B\n  end\n  style one fill:#f9f\n"
            ),
            "one",
            "Plain Title",
        ),
        (
            format!(
                "flowchart LR\n  subgraph \"one\" \"Plain Title\"\n    A {ARROW} B\n  end\n  style one fill:#f9f\n"
            ),
            "one",
            "Plain Title",
        ),
    ] {
        assert!(
            style_resolves(&source, key),
            "the id field was swallowed: `{key}` no longer resolves"
        );
        assert_eq!(
            cluster_titles(&source),
            vec![title.to_string()],
            "the title field was lost"
        );
    }
}

/// CONTROL: the unquoted single-word form is unchanged.
///
/// `subgraph PlainTitle` has always been an id and must stay one — the new branch requires a quote,
/// and this proves it does not fire without one.
#[test]
fn the_bare_word_form_is_unchanged() {
    let source = format!(
        "flowchart LR\n  subgraph PlainTitle\n    A {ARROW} B\n  end\n  style PlainTitle fill:#f9f\n"
    );
    assert!(
        style_resolves(&source, "PlainTitle"),
        "the bare id stopped resolving"
    );
    assert_eq!(cluster_titles(&source), vec!["PlainTitle".to_string()]);
}

/// CONTROL: the subgraph still contains its members.
///
/// Every assertion above reads the cluster's title or its key; none would notice if the rewrite had
/// cost the subgraph its contents.
#[test]
fn the_subgraph_still_contains_its_nodes() {
    let parsed = fm_parser::parse(&format!(
        "flowchart LR\n  subgraph \"Plain Title\"\n    A {ARROW} B\n  end\n  B {ARROW} C\n"
    ));
    let cluster = parsed.ir.clusters.first().expect("one cluster");
    assert_eq!(
        cluster.members.len(),
        2,
        "the subgraph lost its members: {:?}",
        cluster.members
    );
    assert_eq!(parsed.ir.nodes.len(), 3, "the diagram lost a node");
}
