//! Golden snapshot harness for SVG rendering determinism and stability.

use fm_layout::layout_diagram;
use fm_parser::parse;
use fm_render_svg::{SvgRenderConfig, render_svg_with_config};
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

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

fn normalize_svg(svg: &str) -> String {
    let mut normalized = svg.replace("\r\n", "\n");
    if !normalized.ends_with('\n') {
        normalized.push('\n');
    }
    normalized
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

fn run_case(case_id: &str, bless: bool) {
    let base = golden_dir();
    let input_path = base.join(format!("{case_id}.mmd"));
    let expected_path = base.join(format!("{case_id}.svg"));

    let input = fs::read_to_string(&input_path)
        .unwrap_or_else(|err| panic!("failed reading {}: {err}", input_path.display()));

    let parse_start = Instant::now();
    let parsed = parse(&input);
    let parse_ms = parse_start.elapsed().as_millis();

    let layout_start = Instant::now();
    let layout = layout_diagram(&parsed.ir);
    let layout_ms = layout_start.elapsed().as_millis();

    // Keep golden fixtures focused on structural rendering stability.
    // Visual-effect defaults evolve frequently; pinning these values avoids noisy churn.
    let config = SvgRenderConfig {
        node_gradients: false,
        glow_enabled: false,
        cluster_fill_opacity: 1.0,
        inactive_opacity: 1.0,
        shadow_blur: 3.0,
        shadow_color: String::new(),
        ..Default::default()
    };
    let config_hash = fnv_hex(&format!("{config:?}"));
    let input_hash = fnv_hex(&input);

    let render_start = Instant::now();
    let rendered = render_svg_with_config(&parsed.ir, &config);
    let render_ms = render_start.elapsed().as_millis();
    let rendered = normalize_svg(&rendered);
    let output_hash = fnv_hex(&rendered);

    let rerender = normalize_svg(&render_svg_with_config(&parsed.ir, &config));
    assert_eq!(
        rendered, rerender,
        "determinism violation for case {case_id}"
    );

    if bless {
        fs::create_dir_all(&base)
            .unwrap_or_else(|err| panic!("failed creating {}: {err}", base.display()));
        fs::write(&expected_path, &rendered)
            .unwrap_or_else(|err| panic!("failed writing {}: {err}", expected_path.display()));
    }

    let expected = fs::read_to_string(&expected_path).unwrap_or_else(|err| {
        panic!(
            "missing golden snapshot {} ({err}). run with BLESS=1 to generate",
            expected_path.display()
        )
    });
    let expected = normalize_svg(&expected);
    let expected_hash = fnv_hex(&expected);

    assert_eq!(
        output_hash, expected_hash,
        "FNV hash mismatch for case {case_id}"
    );
    assert_eq!(
        rendered, expected,
        "golden snapshot content mismatch for case {case_id}"
    );

    let evidence = json!({
        "scenario_id": case_id,
        "input_hash": input_hash,
        "surface": "cli-integration",
        "renderer": "svg",
        "theme": "default",
        "config_hash": config_hash,
        "parse_ms": parse_ms,
        "layout_ms": layout_ms,
        "render_ms": render_ms,
        "node_count": parsed.ir.nodes.len(),
        "edge_count": parsed.ir.edges.len(),
        "layout_width": layout.bounds.width,
        "layout_height": layout.bounds.height,
        "diagnostic_count": parsed.warnings.len(),
        "degradation_tier": if parsed.warnings.is_empty() { "full" } else { "degraded" },
        "output_artifact_hash": output_hash,
        "pass_fail_reason": if bless { "bless-updated" } else { "matched-golden" },
    });
    println!("{evidence}");
}

#[test]
fn svg_golden_snapshots_are_stable() {
    let bless = std::env::var("BLESS").is_ok_and(|v| v == "1");
    for case_id in CASE_IDS {
        run_case(case_id, bless);
    }
}
