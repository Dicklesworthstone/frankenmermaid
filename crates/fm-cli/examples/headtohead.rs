//! frankenmermaid side of the pinned mermaid-js head-to-head harness (bead bd-1buv.1).
//!
//! Reads a corpus JSON file produced by `scripts/headtohead/run.mjs` (the generators live in
//! `scripts/headtohead/corpus.mjs` so both engines consume byte-identical input), then times the
//! full parse -> layout -> render-to-SVG pipeline, which is the same work `mermaid.render()` does.
//!
//! Emits one JSON object per corpus item on stdout, matching the schema of `mermaid_bench.mjs`.
//! Determinism is checked in-process (length per iteration, full bytes once outside the timed
//! region), so a nondeterministic render is a harness failure rather than a quietly averaged-away
//! anomaly.
//!
//! Run via `scripts/headtohead/run.mjs`, not directly.

use std::time::Instant;

use fm_parser::parse;
use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_layout};
use serde::Deserialize;
use sha2::{Digest, Sha256};

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// Separator used to hash a multi-revision trace as one input. Must match `corpus.mjs`.
/// A single-shot item is a one-revision trace, so its joined form is just the document itself.
const REVISION_SEP: &str = "\n%%--revision--%%\n";

#[derive(Deserialize)]
struct CorpusItem {
    id: String,
    /// Every document the item renders, in order. Length 1 for single-shot items; for an edit trace,
    /// the successive full documents a live preview would re-render as the user types.
    texts: Vec<String>,
    reps: usize,
    warmup: usize,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize()
        .iter()
        .fold(String::with_capacity(64), |mut acc, b| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{b:02x}");
            acc
        })
}

/// The lean output profile: no per-element accessibility metadata, no source spans.
/// This is what `A11yConfig::none()` already produces today; it exists as a config, never as a
/// default. Reported here so output-size dominance is measured, not asserted.
fn lean_config() -> SvgRenderConfig {
    SvgRenderConfig {
        a11y: A11yConfig::none(),
        accessible: false,
        include_source_spans: false,
        ..SvgRenderConfig::default()
    }
}

fn full_pipeline(input: &str, cfg: &SvgRenderConfig) -> String {
    let parsed = parse(input);
    let layout = fm_layout::layout_diagram(&parsed.ir);
    render_svg_with_layout(&parsed.ir, &layout, cfg)
}

/// Render every revision of an item in order, exactly as `mermaid.render()` is called per keystroke.
/// Returns the total output bytes so the caller can compare against the comparator's.
fn render_all(texts: &[String], cfg: &SvgRenderConfig, sink: &mut Vec<String>) {
    sink.clear();
    for text in texts {
        sink.push(full_pipeline(text, cfg));
    }
}

struct Stats {
    n: usize,
    min: u64,
    p50: u64,
    p95: Option<u64>,
    p99: Option<u64>,
    max: u64,
    mean: f64,
    sd: f64,
    cv_pct: f64,
    /// Median absolute deviation, as a percentage of the median.
    ///
    /// Timing noise on a shared box is *one-sided*: preemption, interrupts and frequency dips can
    /// only ever make an iteration slower, never faster. That skews the sample right, which inflates
    /// the standard deviation (and so `cv_pct`) even when the bulk of iterations are tightly
    /// clustered. MAD ignores the tail, so it measures the dispersion of the uncontaminated regime.
    /// The harness gates on this and reports `min` alongside `p50` for the same reason.
    mad_pct: f64,
}

fn stats(mut xs: Vec<u64>) -> Stats {
    xs.sort_unstable();
    let n = xs.len();
    // Nearest-rank percentile. A p95 drawn from <20 samples is just the max wearing a hat, and a
    // p99 from <100 samples likewise; report null rather than a number that cannot mean anything.
    let pct = |p: usize| -> u64 {
        let rank = (p * n).div_ceil(100).max(1);
        xs[rank - 1]
    };
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts and ns fit f64 exactly here"
    )]
    let mean = xs.iter().sum::<u64>() as f64 / n as f64;
    #[expect(
        clippy::cast_precision_loss,
        reason = "sample counts and ns fit f64 exactly here"
    )]
    let variance = if n > 1 {
        xs.iter().map(|&x| (x as f64 - mean).powi(2)).sum::<f64>() / (n - 1) as f64
    } else {
        0.0
    };
    let sd = variance.sqrt();
    let median = pct(50);
    let mut deviations: Vec<u64> = xs.iter().map(|&x| x.abs_diff(median)).collect();
    deviations.sort_unstable();
    let mad = deviations[(n.div_ceil(2)).saturating_sub(1)];
    #[expect(
        clippy::cast_precision_loss,
        reason = "ns magnitudes fit f64 exactly here"
    )]
    let mad_pct = if median > 0 {
        mad as f64 / median as f64 * 100.0
    } else {
        0.0
    };
    Stats {
        n,
        min: xs[0],
        p50: median,
        p95: (n >= 20).then(|| pct(95)),
        p99: (n >= 100).then(|| pct(99)),
        max: xs[n - 1],
        mean,
        sd,
        cv_pct: if mean > 0.0 { sd / mean * 100.0 } else { 0.0 },
        mad_pct,
    }
}

fn ns_json(s: &Stats) -> serde_json::Value {
    serde_json::json!({
        "n": s.n,
        "min": s.min,
        "p50": s.p50,
        "p95": s.p95,
        "p99": s.p99,
        "max": s.max,
        "mean": s.mean.round() as u64,
        "sd": s.sd.round() as u64,
    })
}

/// Each timed sample must span at least this long. A single timer interrupt or scheduler preemption
/// costs on the order of microseconds; timing a 74 us pipeline one iteration at a time therefore
/// measures the kernel as much as the renderer. Batching until a sample spans ~2 ms drops that
/// contamination to well under a percent, which is what lets the small items clear the MAD gate.
/// Batching is a *timing* device only: every iteration in a batch still renders the whole diagram.
const MIN_SAMPLE_NS: u64 = 2_000_000;

/// The result of timing one corpus item: sample statistics, the batch factor used, the reference
/// output of every revision, and their total byte count.
struct Measured {
    stats: Stats,
    batch: usize,
    reference: Vec<String>,
    output_bytes: usize,
}

/// Time `reps` batched samples after `warmup` untimed ones. One sample renders *every revision* of
/// the item in order, so a single-shot item measures one render and an edit trace measures the whole
/// editing session. Output is checked for byte-stability across samples.
fn measure(item: &CorpusItem, cfg: &SvgRenderConfig) -> Result<Measured, String> {
    let mut scratch: Vec<String> = Vec::with_capacity(item.texts.len());

    let mut fastest_warmup = u64::MAX;
    for _ in 0..item.warmup.max(1) {
        let t0 = Instant::now();
        render_all(&item.texts, cfg, &mut scratch);
        std::hint::black_box(&scratch);
        fastest_warmup =
            fastest_warmup.min(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
    }
    let batch = usize::try_from(MIN_SAMPLE_NS / fastest_warmup.max(1))
        .unwrap_or(1)
        .max(1);

    let mut reference: Vec<String> = Vec::with_capacity(item.texts.len());
    render_all(&item.texts, cfg, &mut reference);
    let reference_len: usize = reference.iter().map(String::len).sum();

    let mut samples = Vec::with_capacity(item.reps);
    let mut stable = true;
    for _ in 0..item.reps {
        let t0 = Instant::now();
        for _ in 0..batch {
            render_all(&item.texts, cfg, &mut scratch);
            // Only an O(1) length check inside the timed region -- byte-comparing a 534 KB SVG per
            // iteration would inflate the measurement by several percent. The full byte comparison
            // happens once, outside the timing loop.
            stable &= scratch.iter().map(String::len).sum::<usize>() == reference_len;
            std::hint::black_box(&scratch);
        }
        let elapsed = u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX);
        samples.push(elapsed / batch as u64);
    }

    render_all(&item.texts, cfg, &mut scratch);
    if !stable || scratch != reference {
        return Err(format!("{}: nondeterministic SVG across renders", item.id));
    }
    Ok(Measured {
        stats: stats(samples),
        batch,
        reference,
        output_bytes: reference_len,
    })
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: headtohead <corpus.json>");
        std::process::exit(2);
    });
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(2);
    });
    let items: Vec<CorpusItem> = serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("cannot parse {path}: {e}");
        std::process::exit(2);
    });

    let default_cfg = SvgRenderConfig::default();
    let lean_cfg = lean_config();
    let mut failed = false;

    for item in &items {
        if item.texts.is_empty() {
            failed = true;
            eprintln!("[frankenmermaid] FAIL {}: no revisions", item.id);
            continue;
        }
        // Node/edge counts describe the final revision, which is the largest one in an edit trace.
        let parsed = parse(item.texts.last().expect("non-empty checked above"));
        let nodes = parsed.ir.nodes.len();
        let edges = parsed.ir.edges.len();

        let default_run = match measure(item, &default_cfg) {
            Ok(v) => v,
            Err(e) => {
                failed = true;
                eprintln!("[frankenmermaid] FAIL {e}");
                println!(
                    "{}",
                    serde_json::json!({
                        "engine": "frankenmermaid", "id": item.id, "status": "error", "error": e,
                    })
                );
                continue;
            }
        };
        // The lean profile is measured on the same corpus so the output-size claim and the cost of
        // reaching it are reported together.
        let lean_run = match measure(item, &lean_cfg) {
            Ok(v) => v,
            Err(e) => {
                failed = true;
                eprintln!("[frankenmermaid] FAIL {e}");
                continue;
            }
        };

        let (default_stats, lean_stats, batch) =
            (&default_run.stats, &lean_run.stats, default_run.batch);
        if let Some(bad) = default_run
            .reference
            .iter()
            .find(|svg| !svg.starts_with("<svg") || !svg.ends_with("</svg>"))
        {
            failed = true;
            eprintln!(
                "[frankenmermaid] FAIL {}: a revision's output is not a bare <svg> document ({} bytes)",
                item.id,
                bad.len()
            );
            continue;
        }

        let joined_input = item.texts.join(REVISION_SEP);

        println!(
            "{}",
            serde_json::json!({
                "engine": "frankenmermaid",
                "id": item.id,
                "status": "ok",
                "warmup": item.warmup,
                "batch": batch,
                "revisions": item.texts.len(),
                "input_sha256": sha256_hex(joined_input.as_bytes()),
                "input_bytes": joined_input.len(),
                "nodes": nodes,
                "edges": edges,
                "pipeline_ns": ns_json(default_stats),
                "cv_pct": (default_stats.cv_pct * 100.0).round() / 100.0,
                "mad_pct": (default_stats.mad_pct * 100.0).round() / 100.0,
                "pipeline_lean_ns": ns_json(lean_stats),
                "lean_cv_pct": (lean_stats.cv_pct * 100.0).round() / 100.0,
                "lean_mad_pct": (lean_stats.mad_pct * 100.0).round() / 100.0,
                "output_bytes": default_run.output_bytes,
                "output_bytes_lean": lean_run.output_bytes,
                "output_sha256": sha256_hex(default_run.reference.concat().as_bytes()),
            })
        );
        eprintln!(
            "[frankenmermaid] ok   {}  p50={:.3}ms mad={:.1}% bytes={} lean={}",
            item.id,
            f64::from(u32::try_from(default_stats.p50 / 1000).unwrap_or(u32::MAX)) / 1000.0,
            default_stats.mad_pct,
            default_run.output_bytes,
            lean_run.output_bytes,
        );
    }

    if failed {
        std::process::exit(2);
    }
}
