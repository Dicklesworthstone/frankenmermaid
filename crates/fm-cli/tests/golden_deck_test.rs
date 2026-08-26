//! Golden deck-manifest corpus + property suites (bd-xj5nc, epic bd-z7g6k).
//!
//! The manifest is an EXTERNAL contract (schema 1.0.0, additive-only within 1.x): third-party
//! players build against its field semantics, and the showcase/CLI embed it verbatim. These
//! goldens pin full manifest JSON per fixture — discovered from disk with a minimum-count
//! floor (the `golden_layout_test` lesson: hand-listed corpora rot silently) — plus one full
//! `talk.html` byte golden so template drift (escaping regressions, a lost `<\/script` guard)
//! fails as a red diff instead of shipping as an XSS-adjacent bug.
//!
//! Regenerate with `BLESS_DECK=1 cargo test -p frankenmermaid-cli --test golden_deck_test`.

use fm_core::DeckManifest;
use fm_layout::layout_diagram;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, deck_manifest, render_svg_with_deck};
use proptest::prelude::*;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn deck_golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
        .join("deck")
}

/// Disk-discovered fixture ids, with a floor so a vanished fixture cannot silently stop
/// being checked.
fn case_ids() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(deck_golden_dir())
        .expect("read deck golden directory")
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            if path.extension()?.to_str()? != "mmd" {
                return None;
            }
            Some(path.file_stem()?.to_str()?.to_string())
        })
        .collect();
    ids.sort();
    assert!(
        ids.len() >= 8,
        "the deck golden corpus has shrunk to {} fixtures; cases are discovered from disk, so \
         a vanished fixture silently stops being checked",
        ids.len()
    );
    ids
}

fn manifest_for_case(case: &str) -> (String, DeckManifest, String) {
    let source =
        fs::read_to_string(deck_golden_dir().join(format!("{case}.mmd"))).expect("read fixture");
    let parsed = parse(&source);
    let layout = layout_diagram(&parsed.ir);
    let config = SvgRenderConfig::default();
    let (svg, manifest) = render_svg_with_deck(&parsed.ir, &layout, &config);
    let manifest = manifest.unwrap_or_else(|| panic!("fixture {case} must produce a manifest"));
    (source, manifest, svg)
}

#[test]
fn deck_manifests_match_checked_in_goldens() {
    let bless = std::env::var("BLESS_DECK").is_ok();
    for case in case_ids() {
        let (_, manifest, svg) = manifest_for_case(&case);
        let actual = serde_json::to_string_pretty(&manifest).expect("serialize manifest") + "\n";
        let golden_path = deck_golden_dir().join(format!("{case}.deck.json"));
        if bless {
            fs::write(&golden_path, &actual).expect("bless deck golden");
        }
        let expected = fs::read_to_string(&golden_path).unwrap_or_else(|_| {
            panic!("missing golden {case}.deck.json; run with BLESS_DECK=1 to create it")
        });
        assert_eq!(
            actual, expected,
            "deck manifest drift for {case}; if intentional, re-bless with BLESS_DECK=1 and \
             review the diff"
        );

        // The cross-artifact join-key contract, on every golden: each manifest element id
        // exists verbatim in the paired SVG.
        for slide in &manifest.slides {
            for node in &slide.nodes {
                assert!(
                    svg.contains(&format!("id=\"{}\"", node.element_id)),
                    "{case}: node {} missing from SVG",
                    node.element_id
                );
            }
            for edge in &slide.edges {
                assert!(
                    svg.contains(&format!("id=\"{}\"", edge.element_id)),
                    "{case}: edge {} missing from SVG",
                    edge.element_id
                );
            }
            for cluster in &slide.clusters {
                assert!(
                    svg.contains(&format!("id=\"{}\"", cluster.element_id)),
                    "{case}: cluster {} missing from SVG",
                    cluster.element_id
                );
            }
        }

        // ViewBox equality (closes the D5 loop): the manifest's viewBox equals the rendered
        // SVG's viewBox attribute NUMERICALLY — the SVG prints full float precision while
        // the manifest rounds to two decimals, so the comparison is within 0.01.
        let svg_viewbox: Vec<f32> = svg
            .split("viewBox=\"")
            .nth(1)
            .and_then(|rest| rest.split('"').next())
            .map(|numbers| {
                numbers
                    .split_whitespace()
                    .filter_map(|token| token.parse().ok())
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(svg_viewbox.len(), 4, "{case}: SVG viewBox unparsable");
        for (manifest_value, svg_value) in [
            manifest.view_box.x,
            manifest.view_box.y,
            manifest.view_box.width,
            manifest.view_box.height,
        ]
        .into_iter()
        .zip(&svg_viewbox)
        {
            assert!(
                (manifest_value - svg_value).abs() <= 0.01,
                "{case}: manifest viewBox {manifest_value} vs SVG {svg_value}"
            );
        }
    }
}

/// Semantic invariants every golden must satisfy — checked against the FRESH manifest so a
/// bad bless cannot grandfather a violation in.
#[test]
fn deck_golden_corpus_satisfies_manifest_invariants() {
    for case in case_ids() {
        let (_, manifest, _) = manifest_for_case(&case);
        assert_eq!(manifest.schema_version, "1.0.0");
        for slide in &manifest.slides {
            // Bounds within the viewBox (small epsilon for 2dp rounding).
            assert!(slide.bounds.x >= manifest.view_box.x - 0.01, "{case}");
            assert!(slide.bounds.y >= manifest.view_box.y - 0.01, "{case}");
            assert!(
                slide.bounds.x + slide.bounds.width
                    <= manifest.view_box.x + manifest.view_box.width + 0.01,
                "{case}/{}",
                slide.id
            );
            assert!(
                slide.bounds.y + slide.bounds.height
                    <= manifest.view_box.y + manifest.view_box.height + 0.01,
                "{case}/{}",
                slide.id
            );
            assert_manifest_step_invariants(&case, slide);
        }
    }
}

fn assert_manifest_step_invariants(case: &str, slide: &fm_core::DeckManifestSlide) {
    let max = slide.max_step;
    // steps[] lists exactly 1..=maxStep, each non-empty, in the engine stagger order.
    assert_eq!(slide.steps.len(), max, "{case}/{}", slide.id);
    for (index, step) in slide.steps.iter().enumerate() {
        assert_eq!(step.step, index + 1, "{case}/{}", slide.id);
        assert!(!step.element_ids.is_empty(), "{case}/{}", slide.id);
    }
    // nodes[].step / edges[].step / clusters[].step are mutually consistent with steps[]:
    // an element with step k>0 appears in exactly steps[k], and nowhere else.
    let mut by_step: Vec<BTreeSet<&str>> = vec![BTreeSet::new(); max + 1];
    for node in &slide.nodes {
        assert!(node.step <= max, "{case}/{}", slide.id);
        if node.step > 0 {
            by_step[node.step].insert(node.element_id.as_str());
        }
    }
    for edge in &slide.edges {
        assert!(edge.step <= max, "{case}/{}", slide.id);
        if edge.step > 0 {
            by_step[edge.step].insert(edge.element_id.as_str());
        }
    }
    for cluster in &slide.clusters {
        assert!(cluster.step <= max, "{case}/{}", slide.id);
        if cluster.step > 0 {
            by_step[cluster.step].insert(cluster.element_id.as_str());
        }
        // Cluster step EQUALS the min step of its in-slide members: a cluster box appears
        // with its first member. (Member steps live on nodes[]; the cluster's members are
        // exactly the slide nodes inside it, which the engine derived the min from — the
        // invariant observable here is step <= every containing member's step is not
        // reconstructable without the IR, so we assert the emitted list consistency below.)
    }
    for (step_number, listed) in slide.steps.iter().enumerate().map(|(i, s)| (i + 1, s)) {
        let expected: BTreeSet<&str> = listed.element_ids.iter().map(String::as_str).collect();
        assert_eq!(
            by_step[step_number], expected,
            "{case}/{}: steps[{step_number}] disagrees with per-element step fields",
            slide.id
        );
    }
}

#[test]
fn deck_manifest_is_bit_identical_across_runs_for_every_golden() {
    for case in case_ids() {
        let source =
            fs::read_to_string(deck_golden_dir().join(format!("{case}.mmd"))).expect("fixture");
        let parsed = parse(&source);
        let layout = layout_diagram(&parsed.ir);
        let config = SvgRenderConfig::default();
        let first =
            serde_json::to_string(&deck_manifest(&parsed.ir, &layout, &config).expect("manifest"))
                .expect("serialize");
        let second =
            serde_json::to_string(&deck_manifest(&parsed.ir, &layout, &config).expect("manifest"))
                .expect("serialize");
        assert_eq!(
            first, second,
            "{case}: manifest must be bit-identical across runs"
        );
    }
}

/// One full `talk.html` byte golden: the template + manifest + SVG + runtime are all
/// deterministic, so any drift — placeholder typos, a lost `<\/script` guard, an escaping
/// regression — is a red diff here before it is a shipped bug.
#[test]
fn deck_talk_html_matches_checked_in_golden() {
    let bless = std::env::var("BLESS_DECK").is_ok();
    let fixture = deck_golden_dir().join("flowchart_subgraphs.mmd");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_fm-cli"))
        .args(["deck", fixture.to_str().unwrap()])
        .output()
        .expect("run deck");
    assert!(
        output.status.success(),
        "deck failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual = String::from_utf8(output.stdout).expect("html utf-8");
    let golden_path = deck_golden_dir().join("flowchart_subgraphs.talk.html");
    if bless {
        fs::write(&golden_path, &actual).expect("bless talk.html golden");
    }
    let expected = fs::read_to_string(&golden_path)
        .expect("missing talk.html golden; run with BLESS_DECK=1 to create it");
    assert_eq!(
        actual, expected,
        "talk.html drift; if intentional (template/runtime change), re-bless with BLESS_DECK=1"
    );
}

// ── Property suites ───────────────────────────────────────────────────

/// A generated diagram + deck for the property suites: always a SUPPORTED family
/// (flowchart), sometimes with a subgraph, with authored/auto/no reveals and each edge
/// policy — plus arbitrary junk selectors to exercise degradation.
fn deck_source_strategy() -> impl Strategy<Value = String> {
    (
        2usize..8,                 // node count
        prop::bool::ANY,           // wrap tail nodes in a subgraph
        0usize..3,                 // reveal flavor: none / auto / groups
        0usize..3,                 // edge policy index
        prop::bool::ANY,           // include a junk selector
    )
        .prop_map(|(nodes, subgraph, reveal, edges, junk)| {
            let mut body = String::from("flowchart LR\n");
            for index in 1..nodes {
                body.push_str(&format!("  n{} --> n{}\n", index - 1, index));
            }
            if subgraph {
                body.push_str("  subgraph grp\n    n0\n  end\n");
            }
            let reveal_clause = match reveal {
                1 => ", reveal: 'auto'".to_string(),
                2 => ", reveal: [['n1'], ['n0']]".to_string(),
                _ => String::new(),
            };
            let edges_clause = match edges {
                1 => ", edges: 'touching'",
                2 => ", edges: 'none'",
                _ => "",
            };
            let junk_selector = if junk { ", 'zzz-missing'" } else { "" };
            format!(
                "%%{{deck: {{slides: [{{id: 's', nodes: ['*'{junk_selector}]{reveal_clause}{edges_clause}}}, {{id: 't', nodes: ['n0', 'n1']}}]}}}}%%\n{body}"
            )
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    /// TOTALITY: parse + layout + manifest never panic, whatever the deck says.
    #[test]
    fn deck_pipeline_is_total(source in deck_source_strategy()) {
        let parsed = parse(&source);
        let layout = layout_diagram(&parsed.ir);
        let _ = deck_manifest(&parsed.ir, &layout, &SvgRenderConfig::default());
    }

    /// TOTALITY under mutilation: truncating the directive mid-payload (an unterminated or
    /// syntactically broken deck) must degrade, never crash.
    #[test]
    fn mutilated_deck_directives_never_panic(
        source in deck_source_strategy(),
        cut in 0.1f64..0.9,
    ) {
        let deck_end = source.find("}%%").map_or(source.len(), |i| i + 3);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let cut_at = ((deck_end as f64) * cut) as usize;
        let mut mutilated = String::new();
        for (index, ch) in source.char_indices() {
            if index != cut_at {
                mutilated.push(ch);
            }
        }
        let parsed = parse(&mutilated);
        let layout = layout_diagram(&parsed.ir);
        let _ = deck_manifest(&parsed.ir, &layout, &SvgRenderConfig::default());
    }

    /// CROSS-ARTIFACT (the flagship invariant): every element id the manifest references
    /// exists verbatim in the paired SVG.
    #[test]
    fn manifest_element_ids_exist_in_the_svg(source in deck_source_strategy()) {
        let parsed = parse(&source);
        let layout = layout_diagram(&parsed.ir);
        let (svg, manifest) =
            render_svg_with_deck(&parsed.ir, &layout, &SvgRenderConfig::default());
        if let Some(manifest) = manifest {
            for slide in &manifest.slides {
                for node in &slide.nodes {
                    let needle = format!("id=\"{}\"", node.element_id);
                    prop_assert!(svg.contains(&needle), "missing node id {needle}");
                }
                for edge in &slide.edges {
                    let needle = format!("id=\"{}\"", edge.element_id);
                    prop_assert!(svg.contains(&needle), "missing edge id {needle}");
                }
                for cluster in &slide.clusters {
                    let needle = format!("id=\"{}\"", cluster.element_id);
                    prop_assert!(svg.contains(&needle), "missing cluster id {needle}");
                }
                // Bounds containment + step sanity on generated inputs too.
                prop_assert!(slide.bounds.x >= manifest.view_box.x - 0.01);
                prop_assert!(
                    slide.bounds.x + slide.bounds.width
                        <= manifest.view_box.x + manifest.view_box.width + 0.01
                );
                assert_manifest_step_invariants("generated", slide);
            }
        }
    }
}
