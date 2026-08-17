//! Golden layout checksum tests (bd-17e4.2).
//!
//! Captures bit-exact reference layout outputs for the golden test corpus,
//! isolating layout determinism from rendering implementation changes.
//!
//! The canonical representation sorts nodes by ID and rounds coordinates
//! to 6 decimal places, then computes an FNV-1a hash. This catches any
//! non-deterministic or unintended layout position changes.
//!
//! Run with `BLESS_LAYOUT=1` to regenerate the golden checksums file.

use fm_layout::layout_diagram;
use fm_parser::parse;
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Every fixture in the golden corpus, discovered from disk rather than hand-listed.
///
/// This was a hand-maintained `const CASE_IDS` of 27 names, and it had silently stopped tracking the
/// corpus: ten `.mmd` fixtures had no layout checksum, and among them were the ONLY cases exercising
/// four dedicated layout algorithms — Sankey (sankey_basic), Kanban (kanban_basic, journey_basic),
/// Grid (block_basic) and Packet (packet_basic).
///
/// That is a direct failure of bd-17e4's Success Metric 2, "every layout algorithm has golden
/// checksum tests", and it was not theoretical. The kanban, block-beta and packet LAYOUT algorithms
/// were all rewritten on 2026-08-08 (bd-eg44, bd-7ute, bd-51tz, bd-8vr0) and
/// `layout_checksums.json` never moved, because none of those cases was in the list. The SVG byte
/// goldens caught the changes; this corpus was blind to them.
///
/// Deriving the list from the directory means a new fixture cannot be added without also getting a
/// layout checksum, so the hole cannot reopen. Sorted, so the file's entry order is deterministic.
fn case_ids() -> Vec<String> {
    let mut ids: Vec<String> = fs::read_dir(golden_dir())
        .expect("read golden fixture directory")
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
        ids.len() >= 27,
        "the golden corpus has shrunk to {} fixtures; this test discovers its cases from disk, so \
         a vanished fixture silently stops being checked",
        ids.len()
    );
    ids
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn checksums_path() -> PathBuf {
    golden_dir().join("layout_checksums.json")
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325_u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn fnv_hex(value: &str) -> String {
    format!("{:016x}", fnv1a_64(value.as_bytes()))
}

/// Round a float to 6 decimal places for deterministic comparison.
fn round6(v: f32) -> f64 {
    (f64::from(v) * 1_000_000.0).round() / 1_000_000.0
}

/// Produce a canonical string representation of a layout that depends only
/// on layout positions and edge routes, not rendering details.
///
/// Format: sorted list of `node:<id> x=<x> y=<y> w=<w> h=<h>` lines
/// followed by sorted `edge:<from>-><to> pts=<x1,y1;x2,y2;...>` lines.
fn canonical_layout(ir: &fm_core::MermaidDiagramIr) -> String {
    let layout = layout_diagram(ir);
    let mut lines: Vec<String> = Vec::new();

    // Nodes sorted by node_id for deterministic ordering.
    let mut nodes: Vec<_> = layout.nodes.iter().collect();
    nodes.sort_by(|a, b| a.node_id.cmp(&b.node_id));
    for node in &nodes {
        lines.push(format!(
            "node:{} x={:.6} y={:.6} w={:.6} h={:.6}",
            node.node_id,
            round6(node.bounds.x),
            round6(node.bounds.y),
            round6(node.bounds.width),
            round6(node.bounds.height),
        ));
    }

    // Edges sorted by edge_index for deterministic ordering.
    let mut edges: Vec<_> = layout.edges.iter().collect();
    edges.sort_by_key(|e| e.edge_index);
    for edge in &edges {
        let pts: Vec<String> = edge
            .points
            .iter()
            .map(|p| format!("{:.6},{:.6}", round6(p.x), round6(p.y)))
            .collect();
        lines.push(format!(
            "edge:{} reversed={} pts={}",
            edge.edge_index,
            edge.reversed,
            pts.join(";"),
        ));
    }

    // Bounds
    lines.push(format!(
        "bounds: x={:.6} y={:.6} w={:.6} h={:.6}",
        round6(layout.bounds.x),
        round6(layout.bounds.y),
        round6(layout.bounds.width),
        round6(layout.bounds.height),
    ));

    lines.join("\n")
}

fn load_golden_checksums() -> BTreeMap<String, serde_json::Value> {
    let path = checksums_path();
    if !path.exists() {
        return BTreeMap::new();
    }
    let content = fs::read_to_string(&path).expect("read golden checksums");
    let value: serde_json::Value = serde_json::from_str(&content).expect("parse golden checksums");
    let entries = value["entries"].as_object().cloned().unwrap_or_default();
    entries.into_iter().collect()
}

fn save_golden_checksums(checksums: &BTreeMap<String, serde_json::Value>) {
    let path = checksums_path();
    let value = json!({
        "version": 1,
        "description": "Golden layout checksums for deterministic layout verification. Regenerate with BLESS_LAYOUT=1.",
        "entries": checksums,
    });
    let content = serde_json::to_string_pretty(&value).expect("serialize checksums");
    fs::write(&path, format!("{content}\n")).expect("write golden checksums");
}

#[test]
fn layout_golden_checksums_are_stable() {
    let bless = std::env::var("BLESS_LAYOUT").is_ok_and(|v| v == "1");
    let base = golden_dir();
    let mut checksums = if bless {
        BTreeMap::new()
    } else {
        load_golden_checksums()
    };
    let mut any_failed = false;

    for case_id in &case_ids() {
        let input_path = base.join(format!("{case_id}.mmd"));
        let input = fs::read_to_string(&input_path)
            .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
            .expect("read golden layout input");

        let parsed = parse(&input);
        let canonical = canonical_layout(&parsed.ir);
        let checksum = fnv_hex(&canonical);
        // Section sub-checksums, computed ALWAYS and stored, not just printed on failure.
        //
        // A bare `layout_checksum` says "some geometry moved" and nothing more, and the signature it
        // produces is genuinely ambiguous: identical node/edge counts and identical bounds with a
        // moved checksum is what BOTH a node reshuffle inside a fixed extent AND a pure edge-routing
        // change look like. bd-38wq read that signature as "NODE POSITIONS MOVED" and sent the
        // investigation at barycenter/crossing-minimisation ordering; `canonical_layout` hashes edge
        // polyline points too, so the inference never followed from the evidence.
        //
        // Printing the current components on failure was not enough either -- the STORED golden had
        // none to compare against, so localising the drift still meant checking out the last-blessed
        // commit and rebuilding. Storing them makes the next mismatch self-diagnosing.
        let node_lines: String = canonical
            .lines()
            .filter(|line| line.starts_with("node:"))
            .collect::<Vec<_>>()
            .join("\n");
        let edge_lines: String = canonical
            .lines()
            .filter(|line| line.starts_with("edge:"))
            .collect::<Vec<_>>()
            .join("\n");
        let nodes_checksum = fnv_hex(&node_lines);
        let edges_checksum = fnv_hex(&edge_lines);

        // Verify determinism: compute twice and compare.
        let canonical2 = canonical_layout(&parsed.ir);
        assert_eq!(
            canonical, canonical2,
            "Layout is non-deterministic for {case_id}"
        );

        let ir = &parsed.ir;
        let layout = layout_diagram(ir);

        let entry = json!({
            "layout_checksum": checksum,
            "nodes_checksum": nodes_checksum,
            "edges_checksum": edges_checksum,
            "layout_algorithm": "auto",
            "node_count": ir.nodes.len(),
            "edge_count": ir.edges.len(),
            "layout_width": round6(layout.bounds.width),
            "layout_height": round6(layout.bounds.height),
        });

        if bless {
            checksums.insert(case_id.to_string(), entry);
        } else if let Some(expected) = checksums.get(case_id.as_str()) {
            let expected_checksum = expected["layout_checksum"].as_str().unwrap_or("");
            if checksum != expected_checksum {
                // Name the half that moved. Older goldens carry no component checksums, so an
                // absent stored value is reported as unknown rather than silently treated as a
                // match -- a missing field must never read as "this half is fine".
                let stored_nodes = expected["nodes_checksum"].as_str();
                let stored_edges = expected["edges_checksum"].as_str();
                let verdict = match (stored_nodes, stored_edges) {
                    (Some(n), Some(e)) => {
                        match (n != nodes_checksum, e != edges_checksum) {
                            (true, true) => "NODES AND EDGES BOTH MOVED",
                            (true, false) => "NODE POSITIONS MOVED; edge geometry is unchanged",
                            (false, true) => "EDGE GEOMETRY MOVED; node positions are unchanged",
                            (false, false) => {
                                "NEITHER component moved -- the drift is in bounds or stats"
                            }
                        }
                    }
                    _ => "UNKNOWN: this golden predates component checksums; re-bless to enable",
                };
                eprintln!(
                    "LAYOUT CHECKSUM MISMATCH for {case_id}: {verdict}\n  expected: {expected_checksum}\n  got:      {checksum}\n  nodes:  stored {} -> now {} ({} nodes)\n  edges:  stored {} -> now {} ({} edges)",
                    stored_nodes.unwrap_or("<none>"),
                    nodes_checksum,
                    canonical.lines().filter(|l| l.starts_with("node:")).count(),
                    stored_edges.unwrap_or("<none>"),
                    edges_checksum,
                    canonical.lines().filter(|l| l.starts_with("edge:")).count(),
                );
                eprintln!("  Run with BLESS_LAYOUT=1 to update.");
                any_failed = true;
            }
        } else {
            eprintln!("MISSING golden layout checksum for {case_id}. Run with BLESS_LAYOUT=1.");
            any_failed = true;
        }

        // Emit evidence
        let evidence = json!({
            "scenario_id": case_id,
            "surface": "layout-golden",
            "layout_checksum": checksum,
            "node_count": ir.nodes.len(),
            "edge_count": ir.edges.len(),
            "layout_width": round6(layout.bounds.width),
            "layout_height": round6(layout.bounds.height),
            "determinism_verified": true,
        });
        println!("{evidence}");
    }

    if bless {
        save_golden_checksums(&checksums);
        println!(
            "Blessed {} layout golden checksums to {}",
            checksums.len(),
            checksums_path().display()
        );
    }

    assert!(
        !any_failed,
        "Layout golden checksum mismatches detected. Run with BLESS_LAYOUT=1 to update."
    );
}

/// Verify that each golden case layout is deterministic across 10 runs.
#[test]
fn layout_golden_cases_are_deterministic_across_runs() {
    let base = golden_dir();
    for case_id in &case_ids() {
        let input_path = base.join(format!("{case_id}.mmd"));
        let input = fs::read_to_string(&input_path)
            .map_err(|err| format!("failed reading {}: {err}", input_path.display()))
            .expect("read golden layout input");

        let parsed = parse(&input);
        let reference = canonical_layout(&parsed.ir);

        for run in 1..=10 {
            let current = canonical_layout(&parsed.ir);
            assert_eq!(
                reference, current,
                "Determinism violation for {case_id} on run {run}"
            );
        }
    }
}
