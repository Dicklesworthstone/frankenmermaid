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

const CASE_IDS: &[&str] = &[
    "flowchart_simple",
    "flowchart_cycle",
    "sequence_basic",
    "class_basic",
    "state_basic",
    "gantt_basic",
    "pie_basic",
    "malformed_recovery",
];

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

    for case_id in CASE_IDS {
        let input_path = base.join(format!("{case_id}.mmd"));
        let input = fs::read_to_string(&input_path)
            .unwrap_or_else(|err| panic!("failed reading {}: {err}", input_path.display()));

        let parsed = parse(&input);
        let canonical = canonical_layout(&parsed.ir);
        let checksum = fnv_hex(&canonical);

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
            "layout_algorithm": "auto",
            "node_count": ir.nodes.len(),
            "edge_count": ir.edges.len(),
            "layout_width": round6(layout.bounds.width),
            "layout_height": round6(layout.bounds.height),
        });

        if bless {
            checksums.insert(case_id.to_string(), entry);
        } else if let Some(expected) = checksums.get(*case_id) {
            let expected_checksum = expected["layout_checksum"].as_str().unwrap_or("");
            if checksum != expected_checksum {
                eprintln!(
                    "LAYOUT CHECKSUM MISMATCH for {case_id}:\n  expected: {expected_checksum}\n  got:      {checksum}"
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
    for case_id in CASE_IDS {
        let input_path = base.join(format!("{case_id}.mmd"));
        let input = fs::read_to_string(&input_path)
            .unwrap_or_else(|err| panic!("failed reading {}: {err}", input_path.display()));

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
