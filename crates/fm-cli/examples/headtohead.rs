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

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use fm_parser::parse;
use fm_render_svg::{A11yConfig, SvgRenderConfig, render_svg_with_layout};
use rayon::prelude::*;
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

/// SHA-256 of the ELF that is *actually executing*, hashed by that ELF itself.
///
/// A hash computed by a shell step next to the run proves nothing about which binary ran: `rch`
/// compiles into an opaque per-worker pool target dir whose path cannot be predicted from here, and
/// concurrent agents have edited crates mid-benchmark in this fleet. Self-reporting closes both
/// gaps -- the number and its provenance come out of the same process. Emitted once before any
/// measurement, so it costs nothing inside a timed region.
fn self_elf_sha256() -> (String, u64) {
    let Ok(path) = std::env::current_exe() else {
        return ("unavailable".to_owned(), 0);
    };
    match std::fs::read(&path) {
        Ok(bytes) => (sha256_hex(&bytes), bytes.len() as u64),
        Err(_) => ("unreadable".to_owned(), 0),
    }
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

#[derive(Debug, PartialEq, Eq)]
struct ProcessAffinity {
    mask: Option<String>,
    cpus: Vec<usize>,
}

fn parse_cpu_list(raw: &str) -> Result<Vec<usize>, String> {
    let mut cpus = Vec::new();
    for part in raw
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        if let Some((start, end)) = part.split_once('-') {
            let start = start
                .parse::<usize>()
                .map_err(|e| format!("invalid CPU range start {start:?}: {e}"))?;
            let end = end
                .parse::<usize>()
                .map_err(|e| format!("invalid CPU range end {end:?}: {e}"))?;
            if start > end {
                return Err(format!("invalid descending CPU range {part:?}"));
            }
            cpus.extend(start..=end);
        } else {
            cpus.push(
                part.parse::<usize>()
                    .map_err(|e| format!("invalid CPU id {part:?}: {e}"))?,
            );
        }
    }
    cpus.sort_unstable();
    cpus.dedup();
    Ok(cpus)
}

/// Affinity of the process that actually executes the benchmark.
///
/// Linux exposes the authoritative mask in `/proc/self/status`. Other targets retain a portable
/// fallback containing every CPU visible through `available_parallelism`; the JSON identifies
/// that fallback explicitly so it cannot be mistaken for an OS affinity query.
fn process_affinity(available_parallelism: usize) -> ProcessAffinity {
    if let Ok(status) = std::fs::read_to_string("/proc/self/status") {
        let value = |key: &str| {
            status
                .lines()
                .find_map(|line| line.strip_prefix(key))
                .map(str::trim)
        };
        if let Some(cpu_list) = value("Cpus_allowed_list:")
            && let Ok(cpus) = parse_cpu_list(cpu_list)
            && !cpus.is_empty()
        {
            return ProcessAffinity {
                mask: value("Cpus_allowed:").map(str::to_owned),
                cpus,
            };
        }
    }
    ProcessAffinity {
        mask: Some("all-visible".to_owned()),
        cpus: (0..available_parallelism).collect(),
    }
}

/// Executes independent diagrams through either the scalar path or one persistent portable pool.
///
/// The renderer's existing per-diagram scoped-thread cap is deliberately untouched: the negative
/// evidence ledger shows that raising it above eight regresses because every render pays fresh
/// thread startup. A CI batch is a different vein. Its diagrams are independent, so one pool can
/// stay alive across every warmup, A/A arm, and measured sample. Rayon uses the native scheduler on
/// x86_64 and aarch64; there are no ISA-specific assumptions in this harness.
struct RenderExecutor {
    threads: usize,
    available_parallelism: usize,
    min_sample_ns: u64,
    calibration_target_ns: u64,
    thread_probe_enabled: bool,
    pool: Option<rayon::ThreadPool>,
}

impl RenderExecutor {
    fn new(threads: usize) -> Result<Self, String> {
        let available_parallelism =
            std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get);
        if threads == 0 {
            return Err("FM_H2H_THREADS must be at least 1".to_owned());
        }
        if threads > available_parallelism {
            return Err(format!(
                "FM_H2H_THREADS={threads} exceeds available_parallelism={available_parallelism}"
            ));
        }
        let min_sample_ns =
            std::env::var("FM_H2H_MIN_SAMPLE_NS")
                .ok()
                .map_or(Ok(MIN_SAMPLE_NS), |raw| {
                    raw.parse::<u64>()
                        .map_err(|e| format!("invalid FM_H2H_MIN_SAMPLE_NS={raw:?}: {e}"))
                })?;
        if min_sample_ns == 0 {
            return Err("FM_H2H_MIN_SAMPLE_NS must be at least 1".to_owned());
        }
        let calibration_target_ns = min_sample_ns.saturating_add(min_sample_ns.div_ceil(2));
        let thread_probe_enabled =
            matches!(std::env::var("FM_H2H_THREAD_PROBE").as_deref(), Ok("1"));
        let pool = if threads == 1 {
            None
        } else {
            Some(
                rayon::ThreadPoolBuilder::new()
                    .num_threads(threads)
                    .thread_name(|index| format!("fm-h2h-{index}"))
                    .build()
                    .map_err(|e| format!("cannot build {threads}-thread render pool: {e}"))?,
            )
        };
        Ok(Self {
            threads,
            available_parallelism,
            min_sample_ns,
            calibration_target_ns,
            thread_probe_enabled,
            pool,
        })
    }

    fn from_env() -> Result<Self, String> {
        let threads = std::env::var("FM_H2H_THREADS").ok().map_or(Ok(1), |raw| {
            raw.parse::<usize>()
                .map_err(|e| format!("invalid FM_H2H_THREADS={raw:?}: {e}"))
        })?;
        Self::new(threads)
    }

    fn execution_model(&self) -> &'static str {
        if self.pool.is_some() {
            "rayon_persistent_pool"
        } else {
            "scalar"
        }
    }

    /// Render every revision in deterministic input order.
    ///
    /// `IndexedParallelIterator::collect::<Vec<_>>()` preserves input order, so concatenating the
    /// result is byte-identical to the scalar path even though work completes out of order.
    fn render_all_observing(
        &self,
        texts: &[String],
        cfg: &SvgRenderConfig,
        sink: &mut Vec<String>,
        workers_seen: Option<&[AtomicBool]>,
    ) {
        sink.clear();
        if let Some(pool) = &self.pool {
            let rendered = pool.install(|| {
                texts
                    .par_iter()
                    .map(|text| {
                        if let Some(workers_seen) = workers_seen
                            && let Some(index) = rayon::current_thread_index()
                            && let Some(seen) = workers_seen.get(index)
                        {
                            seen.store(true, Ordering::Relaxed);
                        }
                        full_pipeline(std::hint::black_box(text.as_str()), cfg)
                    })
                    .collect::<Vec<_>>()
            });
            sink.extend(rendered);
        } else {
            if let Some(seen) = workers_seen.and_then(|workers| workers.first()) {
                seen.store(true, Ordering::Relaxed);
            }
            for text in texts {
                sink.push(full_pipeline(std::hint::black_box(text.as_str()), cfg));
            }
        }
    }

    fn render_all(&self, texts: &[String], cfg: &SvgRenderConfig, sink: &mut Vec<String>) {
        self.render_all_observing(texts, cfg, sink, None);
    }

    /// Observe workers that execute the exact workload, outside every timed sample.
    ///
    /// `ThreadPoolBuilder::num_threads` is only a request. Recording the distinct Rayon worker
    /// indices that actually run diagram jobs proves operation-level participation on Linux and
    /// Apple Silicon without relying on ISA- or OS-specific thread APIs.
    fn probe_operation_threads(
        &self,
        texts: &[String],
        cfg: &SvgRenderConfig,
        batch: usize,
    ) -> usize {
        let workers_seen = (0..self.threads)
            .map(|_| AtomicBool::new(false))
            .collect::<Vec<_>>();
        let mut sink = Vec::with_capacity(texts.len());
        for _ in 0..batch {
            self.render_all_observing(texts, cfg, &mut sink, Some(&workers_seen));
        }
        workers_seen
            .iter()
            .filter(|seen| seen.load(Ordering::Relaxed))
            .count()
            .max(1)
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
    /// Median absolute deviation, as a percentage of the median. Report-only: decidability is gated
    /// exclusively on the same-invocation A/A bootstrap CI below.
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
    let mid = n / 2;
    let median = if n.is_multiple_of(2) {
        let lower = xs[mid - 1];
        lower + (xs[mid] - lower) / 2
    } else {
        xs[mid]
    };
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

const BOOTSTRAP_RESAMPLES: usize = 2_000;
const MIN_NULL_ROUNDS: usize = 9;

#[derive(Debug)]
struct RatioStats {
    n: usize,
    median: f64,
    ci95_lo: f64,
    ci95_hi: f64,
    half_width: f64,
    min_decidable_2x: f64,
    cv_pct: f64,
    mad_pct: f64,
}

fn median(values: &mut [f64]) -> f64 {
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        f64::midpoint(values[mid - 1], values[mid])
    } else {
        values[mid]
    }
}

/// Deterministic percentile-bootstrap 95% CI on the median.
fn bootstrap_median_ci(ratios: &[f64]) -> (f64, f64) {
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut medians = Vec::with_capacity(BOOTSTRAP_RESAMPLES);
    let mut sample = vec![0.0_f64; ratios.len()];
    for _ in 0..BOOTSTRAP_RESAMPLES {
        for slot in &mut sample {
            let index = usize::try_from(next() >> 33).unwrap_or(0) % ratios.len();
            *slot = ratios[index];
        }
        medians.push(median(&mut sample));
    }
    medians.sort_by(f64::total_cmp);
    let tail = BOOTSTRAP_RESAMPLES / 40;
    (medians[tail], medians[BOOTSTRAP_RESAMPLES - 1 - tail])
}

#[expect(
    clippy::cast_precision_loss,
    reason = "short ratio samples and timing magnitudes fit f64 exactly enough for statistics"
)]
fn ratio_stats(ratios: &[f64]) -> RatioStats {
    let mut for_median = ratios.to_vec();
    let ratio_median = median(&mut for_median);
    let mean = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let variance = if ratios.len() > 1 {
        ratios.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / (ratios.len() - 1) as f64
    } else {
        0.0
    };
    let sd = variance.sqrt();
    let mut deviations = ratios
        .iter()
        .map(|x| (x - ratio_median).abs())
        .collect::<Vec<_>>();
    let mad = median(&mut deviations);
    let (ci95_lo, ci95_hi) = bootstrap_median_ci(ratios);
    let half_width = (ci95_hi - 1.0).abs().max((ci95_lo - 1.0).abs());
    RatioStats {
        n: ratios.len(),
        median: ratio_median,
        ci95_lo,
        ci95_hi,
        half_width,
        min_decidable_2x: 1.0 + 2.0 * half_width,
        cv_pct: if mean == 0.0 { 0.0 } else { sd / mean * 100.0 },
        mad_pct: if ratio_median == 0.0 {
            0.0
        } else {
            mad / ratio_median * 100.0
        },
    }
}

fn ratio_json(
    s: &RatioStats,
    kind: &str,
    arm_a_sha256: &str,
    arm_b_sha256: &str,
) -> serde_json::Value {
    let is_null = kind == "aa_null";
    serde_json::json!({
        "kind": kind,
        "n": s.n,
        "sufficient": !is_null || s.n >= MIN_NULL_ROUNDS,
        "median": s.median,
        "ci95_lo": s.ci95_lo,
        "ci95_hi": s.ci95_hi,
        "half_width": is_null.then_some(s.half_width),
        "min_decidable_2x": is_null.then_some(s.min_decidable_2x.max(1.01)),
        "cv_pct": (s.cv_pct * 100.0).round() / 100.0,
        "mad_pct": (s.mad_pct * 100.0).round() / 100.0,
        "cv_gate": "never",
        "arm_a_sha256": arm_a_sha256,
        "arm_b_sha256": arm_b_sha256,
    })
}

/// Each timed sample must span at least this long. A single timer interrupt or scheduler preemption
/// costs on the order of microseconds; timing a 74 us pipeline one iteration at a time therefore
/// measures the kernel as much as the renderer. Normal runs use a 2 ms floor; the thread-sweep
/// driver raises it to 50 ms because sub-millisecond jobs need more integration time for a stable
/// bracket across the long Chromium phase.
/// Batching is a *timing* device only: every iteration in a batch still renders the whole diagram.
const MIN_SAMPLE_NS: u64 = 2_000_000;

fn calibrated_batch(min_sample_ns: u64, fastest_warmup_ns: u64) -> usize {
    usize::try_from(min_sample_ns.div_ceil(fastest_warmup_ns.max(1)))
        .unwrap_or(usize::MAX)
        .max(1)
}

fn rescaled_batch(batch: usize, target_ns: u64, elapsed_ns: u64) -> usize {
    let scaled = u128::try_from(batch)
        .unwrap_or(u128::MAX)
        .saturating_mul(u128::from(target_ns))
        .div_ceil(u128::from(elapsed_ns.max(1)));
    usize::try_from(scaled)
        .unwrap_or(usize::MAX)
        .max(batch.saturating_add(1))
}

struct PairedMeasured {
    arm_a_stats: Stats,
    arm_b_stats: Stats,
    ratio: RatioStats,
    arm_a_reference: Vec<String>,
    arm_b_reference: Vec<String>,
    arm_a_output_bytes: usize,
    arm_b_output_bytes: usize,
}

/// Calibrate off the faster arm; both the A/A and A/B routines then use this exact batch.
fn calibrate_batch(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg_a: &SvgRenderConfig,
    cfg_b: &SvgRenderConfig,
) -> usize {
    let mut scratch: Vec<String> = Vec::with_capacity(item.texts.len());
    let mut fastest_warmup = u64::MAX;
    for _ in 0..item.warmup.max(1) {
        for cfg in [cfg_a, cfg_b] {
            let t0 = Instant::now();
            executor.render_all(&item.texts, cfg, &mut scratch);
            std::hint::black_box(&scratch);
            fastest_warmup =
                fastest_warmup.min(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
        }
    }
    if std::env::var_os("FM_H2H_FORCE_PROFILE").is_some() {
        return 1;
    }

    let mut batch = calibrated_batch(executor.min_sample_ns, fastest_warmup);
    // A single-job warmup can overestimate the steady-state cost once caches and the worker pool
    // are hot. Measure the integrated batch itself and scale proportionally until it clears a 50%
    // headroom target. The driver separately fails closed unless measured p50 still reaches the
    // declared minimum, so calibration cannot silently bless a short sample.
    for _ in 0..4 {
        let mut fastest_elapsed = u64::MAX;
        for cfg in [cfg_a, cfg_b] {
            let t0 = Instant::now();
            for _ in 0..batch {
                executor.render_all(&item.texts, cfg, &mut scratch);
                std::hint::black_box(&scratch);
            }
            fastest_elapsed =
                fastest_elapsed.min(u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX));
        }
        if fastest_elapsed >= executor.calibration_target_ns {
            return batch;
        }
        batch = rescaled_batch(batch, executor.calibration_target_ns, fastest_elapsed);
    }
    batch
}

fn time_arm(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg: &SvgRenderConfig,
    batch: usize,
    scratch: &mut Vec<String>,
    reference_len: usize,
    stable: &mut bool,
) -> u64 {
    let t0 = Instant::now();
    for _ in 0..batch {
        executor.render_all(&item.texts, cfg, scratch);
        // Full byte comparison stays outside the timed region; the O(1) length check catches drift
        // during the rounds without charging a multi-megabyte comparison to the arm.
        *stable &= scratch.iter().map(String::len).sum::<usize>() == reference_len;
        std::hint::black_box(&scratch);
    }
    let elapsed = u64::try_from(t0.elapsed().as_nanos()).unwrap_or(u64::MAX);
    elapsed / u64::try_from(batch).unwrap_or(1).max(1)
}

/// Measure two arms back-to-back inside every round and alternate their order. The median is taken
/// over per-round ratios. This same routine is called first as A/A and then as A/B.
#[expect(
    clippy::cast_precision_loss,
    reason = "nanosecond timing magnitudes fit f64 exactly enough for ratio statistics"
)]
fn paired(
    executor: &RenderExecutor,
    item: &CorpusItem,
    cfg_a: &SvgRenderConfig,
    cfg_b: &SvgRenderConfig,
    batch: usize,
    rounds: usize,
) -> Result<PairedMeasured, String> {
    let mut arm_a_reference = Vec::with_capacity(item.texts.len());
    let mut arm_b_reference = Vec::with_capacity(item.texts.len());
    executor.render_all(&item.texts, cfg_a, &mut arm_a_reference);
    executor.render_all(&item.texts, cfg_b, &mut arm_b_reference);
    let arm_a_output_bytes = arm_a_reference.iter().map(String::len).sum();
    let arm_b_output_bytes = arm_b_reference.iter().map(String::len).sum();
    let mut scratch = Vec::with_capacity(item.texts.len());
    let mut arm_a_samples = Vec::with_capacity(rounds);
    let mut arm_b_samples = Vec::with_capacity(rounds);
    let mut ratios = Vec::with_capacity(rounds);
    let mut arm_a_stable = true;
    let mut arm_b_stable = true;

    for round in 0..rounds {
        let (arm_a_ns, arm_b_ns) = if round.is_multiple_of(2) {
            let a = time_arm(
                executor,
                item,
                cfg_a,
                batch,
                &mut scratch,
                arm_a_output_bytes,
                &mut arm_a_stable,
            );
            let b = time_arm(
                executor,
                item,
                cfg_b,
                batch,
                &mut scratch,
                arm_b_output_bytes,
                &mut arm_b_stable,
            );
            (a, b)
        } else {
            let b = time_arm(
                executor,
                item,
                cfg_b,
                batch,
                &mut scratch,
                arm_b_output_bytes,
                &mut arm_b_stable,
            );
            let a = time_arm(
                executor,
                item,
                cfg_a,
                batch,
                &mut scratch,
                arm_a_output_bytes,
                &mut arm_a_stable,
            );
            (a, b)
        };
        arm_a_samples.push(arm_a_ns);
        arm_b_samples.push(arm_b_ns);
        ratios.push(arm_a_ns as f64 / arm_b_ns.max(1) as f64);
    }

    executor.render_all(&item.texts, cfg_a, &mut scratch);
    let arm_a_exact = scratch == arm_a_reference;
    executor.render_all(&item.texts, cfg_b, &mut scratch);
    let arm_b_exact = scratch == arm_b_reference;
    if !arm_a_stable || !arm_b_stable || !arm_a_exact || !arm_b_exact {
        return Err(format!("{}: nondeterministic SVG across renders", item.id));
    }
    Ok(PairedMeasured {
        arm_a_stats: stats(arm_a_samples),
        arm_b_stats: stats(arm_b_samples),
        ratio: ratio_stats(&ratios),
        arm_a_reference,
        arm_b_reference,
        arm_a_output_bytes,
        arm_b_output_bytes,
    })
}

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: headtohead <corpus.json> [dump-svg-dir]");
        std::process::exit(2);
    });
    // Optional: write each item's final default/lean SVG to <dir>/<id>.{default,lean}.svg. Two uses:
    // settling output-contract questions against mermaid's real output, and pinning byte-identity of
    // the lean profile across a refactor (see bd-b2b6).
    let dump_dir = std::env::args().nth(2);
    if let Some(dir) = dump_dir.as_deref()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("cannot create {dir}: {e}");
        std::process::exit(2);
    }
    let raw = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(2);
    });
    let items: Vec<CorpusItem> = serde_json::from_str(&raw).unwrap_or_else(|e| {
        eprintln!("cannot parse {path}: {e}");
        std::process::exit(2);
    });
    let executor = RenderExecutor::from_env().unwrap_or_else(|e| {
        eprintln!("{e}");
        std::process::exit(2);
    });
    let affinity = process_affinity(executor.available_parallelism);
    let affinity_source = if affinity.mask.as_deref() == Some("all-visible") {
        "portable_visible_cpu_fallback"
    } else {
        "linux_proc_status"
    };

    // Line 1 of stdout: which ELF produced everything below it.
    let (elf_sha256, elf_bytes) = self_elf_sha256();
    println!(
        "{}",
        serde_json::json!({
            "engine": "frankenmermaid",
            "id": "__binary__",
            "record": "binary",
            "elf_sha256": elf_sha256,
            "elf_bytes": elf_bytes,
            "worker_threads": executor.threads,
            "thread_count_requested": executor.threads,
            "thread_probe_required": executor.thread_probe_enabled,
            "available_parallelism": executor.available_parallelism,
            "affinity_mask": affinity.mask.as_deref(),
            "affinity_cpus": &affinity.cpus,
            "affinity_source": affinity_source,
            "min_sample_ns": executor.min_sample_ns,
            "calibration_target_ns": executor.calibration_target_ns,
            "execution_model": executor.execution_model(),
        })
    );

    // Measurement aid. Each item is normally timed twice, once per profile, so `perf stat` on the whole
    // process cannot attribute instructions to one of them. Forcing BOTH passes to the same profile makes
    // the process's instruction count proportional to that profile alone, which turns a load-sensitive
    // wall-clock A/B into a deterministic, load-immune one. Unset for normal runs.
    let (default_cfg, lean_cfg) = match std::env::var("FM_H2H_FORCE_PROFILE").as_deref() {
        Ok("lean") => (lean_config(), lean_config()),
        Ok("default") => (SvgRenderConfig::default(), SvgRenderConfig::default()),
        _ => (SvgRenderConfig::default(), lean_config()),
    };
    let mut failed = false;

    for item in &items {
        if item.texts.is_empty() {
            failed = true;
            eprintln!("[frankenmermaid] FAIL {}: no revisions", item.id);
            continue;
        }
        // Size is reported two ways because a multi-document item can mean two different things.
        // For an edit trace the interesting size is the largest revision (the session grows). For a
        // doc build it is the total across the batch -- reporting only the last document would
        // describe a 40-diagram CI job by whichever diagram happened to be last. Single-shot items
        // have one revision, so both numbers equal the old one.
        let (mut nodes, mut edges, mut nodes_total, mut edges_total) = (0, 0, 0, 0);
        for text in &item.texts {
            let parsed = parse(text);
            nodes = nodes.max(parsed.ir.nodes.len());
            edges = edges.max(parsed.ir.edges.len());
            nodes_total += parsed.ir.nodes.len();
            edges_total += parsed.ir.edges.len();
        }

        let batch = calibrate_batch(&executor, item, &default_cfg, &lean_cfg);
        let thread_count_actually_used = executor
            .thread_probe_enabled
            .then(|| executor.probe_operation_threads(&item.texts, &default_cfg, batch));
        let rounds = item.reps.max(MIN_NULL_ROUNDS);
        let null_run = match paired(&executor, item, &default_cfg, &default_cfg, batch, rounds) {
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
        if null_run.arm_a_reference != null_run.arm_b_reference {
            failed = true;
            eprintln!("[frankenmermaid] FAIL {}: A/A output mismatch", item.id);
            continue;
        }
        // Same paired routine, same invocation, same batch: A/A first, then the real default/lean A/B.
        let profile_run = match paired(&executor, item, &default_cfg, &lean_cfg, batch, rounds) {
            Ok(v) => v,
            Err(e) => {
                failed = true;
                eprintln!("[frankenmermaid] FAIL {e}");
                continue;
            }
        };

        let (default_stats, lean_stats) = (&profile_run.arm_a_stats, &profile_run.arm_b_stats);
        if let Some(bad) = profile_run
            .arm_a_reference
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

        if let Some(dir) = dump_dir.as_deref() {
            let last = |v: &[String]| v.last().cloned().unwrap_or_default();
            let _ = std::fs::write(
                format!("{dir}/{}.default.svg", item.id),
                last(&profile_run.arm_a_reference),
            );
            let _ = std::fs::write(
                format!("{dir}/{}.lean.svg", item.id),
                last(&profile_run.arm_b_reference),
            );
        }

        let default_sha256 = sha256_hex(profile_run.arm_a_reference.concat().as_bytes());
        let lean_sha256 = sha256_hex(profile_run.arm_b_reference.concat().as_bytes());
        let null_a_sha256 = sha256_hex(null_run.arm_a_reference.concat().as_bytes());
        let null_b_sha256 = sha256_hex(null_run.arm_b_reference.concat().as_bytes());
        println!(
            "{}",
            serde_json::json!({
                "engine": "frankenmermaid",
                "id": item.id,
                "status": "ok",
                "warmup": item.warmup,
                "batch": batch,
                "worker_threads": executor.threads,
                "thread_count_requested": executor.threads,
                "thread_count_actually_used": thread_count_actually_used,
                "thread_probe": thread_count_actually_used.map(|observed| serde_json::json!({
                    "method": "instrumented_caller_worker_union_over_exact_workload",
                    "probe_batch": batch,
                    "caller_workers_observed": observed,
                    "portable_across_isa": true,
                    "inside_timed_region": false,
                })),
                "available_parallelism": executor.available_parallelism,
                "affinity_mask": affinity.mask.as_deref(),
                "affinity_cpus": &affinity.cpus,
                "affinity_source": affinity_source,
                "min_sample_ns": executor.min_sample_ns,
                "calibration_target_ns": executor.calibration_target_ns,
                "execution_model": executor.execution_model(),
                "revisions": item.texts.len(),
                "input_sha256": sha256_hex(joined_input.as_bytes()),
                "input_bytes": joined_input.len(),
                "nodes": nodes,
                "edges": edges,
                "nodes_total": nodes_total,
                "edges_total": edges_total,
                "pipeline_ns": ns_json(default_stats),
                "cv_pct": (default_stats.cv_pct * 100.0).round() / 100.0,
                "mad_pct": (default_stats.mad_pct * 100.0).round() / 100.0,
                "pipeline_lean_ns": ns_json(lean_stats),
                "lean_cv_pct": (lean_stats.cv_pct * 100.0).round() / 100.0,
                "lean_mad_pct": (lean_stats.mad_pct * 100.0).round() / 100.0,
                "null_control": ratio_json(
                    &null_run.ratio,
                    "aa_null",
                    &null_a_sha256,
                    &null_b_sha256,
                ),
                "profile_ab": ratio_json(
                    &profile_run.ratio,
                    "default_vs_lean",
                    &default_sha256,
                    &lean_sha256,
                ),
                "output_bytes": profile_run.arm_a_output_bytes,
                "output_bytes_lean": profile_run.arm_b_output_bytes,
                "output_sha256": default_sha256,
                "output_sha256_lean": lean_sha256,
            })
        );
        eprintln!(
            "[frankenmermaid] ok   {}  p50={:.3}ms null={:.6} [{:.6},{:.6}] bytes={} lean={}",
            item.id,
            f64::from(u32::try_from(default_stats.p50 / 1000).unwrap_or(u32::MAX)) / 1000.0,
            null_run.ratio.median,
            null_run.ratio.ci95_lo,
            null_run.ratio.ci95_hi,
            profile_run.arm_a_output_bytes,
            profile_run.arm_b_output_bytes,
        );
    }

    if failed {
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use fm_render_svg::SvgRenderConfig;

    use super::{
        RenderExecutor, bootstrap_median_ci, calibrated_batch, median, parse_cpu_list, ratio_stats,
        rescaled_batch, stats,
    };

    #[test]
    fn median_averages_the_two_middle_values() {
        assert_eq!(median(&mut [4.0, 1.0, 3.0, 2.0]), 2.5);
        assert_eq!(stats(vec![1, 3]).p50, 2);
    }

    #[test]
    fn bootstrap_ci_is_exact_for_a_perfect_null() {
        let ratios = vec![1.0; 41];
        assert_eq!(bootstrap_median_ci(&ratios), (1.0, 1.0));
        let stats = ratio_stats(&ratios);
        assert_eq!(stats.median, 1.0);
        assert_eq!(stats.half_width, 0.0);
        assert_eq!(stats.min_decidable_2x, 1.0);
    }

    #[test]
    fn calibrated_batch_rounds_up_to_the_sample_floor() {
        assert_eq!(calibrated_batch(2_000_000, 1_500_000), 2);
        assert_eq!(calibrated_batch(50_000_000, 1_000_001), 50);
        assert_eq!(calibrated_batch(2_000_000, 3_000_000), 1);
        assert_eq!(rescaled_batch(8, 75_000_000, 43_000_000), 14);
    }

    #[test]
    fn persistent_pool_preserves_scalar_output_order_and_bytes() {
        let texts = vec![
            "flowchart LR\nA[First]-->B[Second]".to_owned(),
            "sequenceDiagram\nAlice->>Bob: Hello".to_owned(),
            "classDiagram\nclass User".to_owned(),
            "stateDiagram-v2\n[*]-->Ready".to_owned(),
        ];
        let config = SvgRenderConfig::default();
        let scalar = RenderExecutor::new(1).expect("scalar executor");
        let parallel = RenderExecutor::new(2).expect("parallel executor");
        let mut scalar_output = Vec::new();
        let mut parallel_output = Vec::new();
        scalar.render_all(&texts, &config, &mut scalar_output);
        parallel.render_all(&texts, &config, &mut parallel_output);
        assert_eq!(parallel_output, scalar_output);
    }

    #[test]
    fn operation_probe_reports_workers_that_execute_diagrams() {
        let texts = (0..64)
            .map(|index| format!("flowchart LR\nA{index}[First]-->B{index}[Second]"))
            .collect::<Vec<_>>();
        let config = SvgRenderConfig::default();
        let scalar = RenderExecutor::new(1).expect("scalar executor");
        let parallel = RenderExecutor::new(2).expect("parallel executor");
        assert_eq!(scalar.probe_operation_threads(&texts, &config, 2), 1);
        assert_eq!(parallel.probe_operation_threads(&texts, &config, 2), 2);
    }

    #[test]
    fn cpu_list_parser_expands_ranges_and_deduplicates() {
        assert_eq!(
            parse_cpu_list("0-3,8,10-11,8").expect("valid CPU list"),
            vec![0, 1, 2, 3, 8, 10, 11]
        );
        assert!(parse_cpu_list("4-2").is_err());
    }
}
