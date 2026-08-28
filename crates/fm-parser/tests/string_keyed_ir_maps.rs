//! Integer-like IR map keys must serialize as STRING keys, or the WASM `parse()` API cannot return.
//!
//! THE DEFECT. `fm-wasm` serializes with
//! `serde_wasm_bindgen::Serializer::new().serialize_maps_as_objects(true)` — required, because the
//! published TS contract declares `Record<string, …>` and the default JS `Map` output silently broke
//! consumers (bd-tm1q7). That serializer REJECTS a non-string map key. Two IR fields had one:
//!
//! ```text
//!   IrGitGraphMeta::commit_lanes      BTreeMap<usize, usize>                  every gitGraph
//!   MermaidDiagramIr::label_markup    BTreeMap<IrLabelId, Vec<IrLabelSegment>>  every markdown label
//! ```
//!
//! so `parse()` threw `Map key is not a string and cannot be an object key` for those inputs, in a
//! browser and in Node alike, while `renderSvg` on the same source succeeded. Nothing that *rendered*
//! was broken, which is why it survived.
//!
//! ⚠️ WHAT THIS FILE CAN AND CANNOT GUARD. No native serializer reproduces the WASM constraint:
//! `serde_json` and `toml` BOTH stringify integer keys without complaint (measured — `toml` emits
//! `[commit_lanes]\n0 = 0`). So a Rust test cannot fail merely because the `with = "string_keyed_map"`
//! attribute was removed, and claiming otherwise would be a vacuous gate. What it pins is the WIRE
//! SHAPE — string keys, exact values, and a full round-trip through the adapter's `deserialize` half,
//! which a serialize-only implementation would fail.
//!
//! The boundary itself is guarded by `scripts/headtohead/wasm_parse_conformance.mjs`, which drives
//! the built bundle and reported **4 failing families before this fix and 0 after**.

use fm_core::MermaidDiagramIr;

fn parse_ir(source: &str) -> MermaidDiagramIr {
    fm_parser::parse(source).ir
}

/// ⚠️ THE KEYS MUST BE STRINGS, and the values must survive unchanged.
///
/// Asserting only "the JSON contains commit_lanes" would pass for any encoding at all. The object is
/// destructured and every key is required to be a string that parses back to its lane index.
#[test]
fn gitgraph_commit_lanes_serialize_as_string_keys() {
    let ir = parse_ir("gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n");
    let meta = ir
        .git_graph_meta
        .as_ref()
        .expect("a gitGraph carries git-graph meta");
    assert!(
        !meta.commit_lanes.is_empty(),
        "fixture must exercise the map: an empty one serializes to nothing and proves nothing"
    );

    let value = serde_json::to_value(meta).expect("git-graph meta serializes");
    let lanes = value
        .get("commit_lanes")
        .and_then(serde_json::Value::as_object)
        .expect("commit_lanes is a JSON object");

    assert_eq!(lanes.len(), meta.commit_lanes.len());
    for (key, lane) in lanes {
        let index: usize = key
            .parse()
            .unwrap_or_else(|_| panic!("commit_lanes key {key:?} is not an integer-valued string"));
        assert_eq!(
            lane.as_u64().map(|n| n as usize),
            meta.commit_lanes.get(&index).copied(),
            "lane for commit {index} changed shape"
        );
    }
}

/// The same for the label-markup map, whose key is a newtype (`IrLabelId`) rather than a bare index.
#[test]
fn markdown_label_markup_serializes_as_string_keys() {
    let ir = parse_ir("flowchart LR\n  A[\"`**bold**`\"] --> B\n");
    assert!(
        !ir.label_markup.is_empty(),
        "fixture must exercise the map: a plain label populates no markup, which is exactly how a \
         sweep over `A --> B` reported flowchart healthy while markdown labels threw"
    );

    let value = serde_json::to_value(&ir).expect("ir serializes");
    let markup = value
        .get("label_markup")
        .and_then(serde_json::Value::as_object)
        .expect("label_markup is a JSON object");

    assert_eq!(markup.len(), ir.label_markup.len());
    for key in markup.keys() {
        key.parse::<usize>()
            .unwrap_or_else(|_| panic!("label_markup key {key:?} is not an integer-valued string"));
    }
}

/// ⚠️ THE ROUND TRIP IS THE HALF A SERIALIZE-ONLY ADAPTER WOULD FAIL.
///
/// Rendering keys as strings on the way out is useless if they cannot be read back: every consumer
/// of the JSON IR — and the batch-parse snapshots — deserializes it again. Both maps are compared
/// after a full `to_string` / `from_str` cycle, so an adapter missing its `deserialize` half, or one
/// whose key parsing disagrees with its key rendering, fails here.
#[test]
fn both_maps_round_trip_through_json() {
    for source in [
        "gitGraph\n  commit\n  branch dev\n  checkout dev\n  commit\n",
        "flowchart LR\n  A[\"`**bold**`\"] --> B\n",
    ] {
        let ir = parse_ir(source);
        let json = serde_json::to_string(&ir).expect("ir serializes");
        let restored: MermaidDiagramIr = serde_json::from_str(&json)
            .expect("ir deserializes: the adapter must read its own keys");

        assert_eq!(
            restored.label_markup, ir.label_markup,
            "label_markup did not survive the round trip for {source:?}"
        );
        assert_eq!(
            restored.git_graph_meta, ir.git_graph_meta,
            "git_graph_meta did not survive the round trip for {source:?}"
        );
    }
}
