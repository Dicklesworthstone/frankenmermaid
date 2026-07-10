//! Calibrate the paired-sampler harness: measure its **A/A null control** across a sweep of
//! configurations *and* across several functions, then publish which configuration can decide a claim of a
//! given size.
//!
//! # The problem this solves
//!
//! `paired(base, cand)` gives you a ratio. It does not tell you whether that ratio *means* anything. The only
//! way to know is to run `paired(base, base)` — the same arm on both sides — so that every deviation from
//! `1.000` is attributable to the harness and never to a lever.
//!
//! An earlier sweep established two things about this hardware:
//!
//! 1. **`cv < 5` is unreachable.** No in-harness knob moves it below ~12% on a loaded, unpinnable worker; a
//!    20× longer sample moved it only 15.65% → 11.63%. `rch` cannot pin a worker, so a quiet machine is luck.
//! 2. **The null *median* is tight anyway** (0.11–0.75% from `1.000`). One-sided scheduler outliers inflate
//!    `cv` and `MAD` without biasing the median of per-round ratios.
//!
//! So the gate is the **median**, not `cv`. But a median has its own sampling error, and quoting it as a bare
//! point estimate would repeat — one level up — exactly the mistake this harness exists to prevent. This bench
//! therefore reports a **bootstrap 95% confidence interval on the null median**, and derives the **minimum
//! decidable effect** for each configuration:
//!
//! > A claim of size `X` is decidable under configuration `C` iff `X` lies outside `C`'s null CI.
//! > Equivalently: `min_decidable(C) = 1 + max(|ci_hi − 1|, |ci_lo − 1|)`.
//!
//! # The floor is per-function
//!
//! It is not a property of the machine alone. A function with a different duration, allocation pattern or
//! cache footprint has a different floor on the same worker. So this sweeps **three arms** — the `BTreeMap`
//! reference, the packed dense-rank arm, and the single-pass arm — and reports a floor for each. Pick your
//! configuration from the row matching the *function you are about to benchmark*.
//!
//! # Two knobs, and two that are not
//!
//! * `min_sample` — how long one timed sample spans (via `batch`).
//! * `min_of` — inner replicates, keeping the **minimum**. Scheduler noise is one-sided, so the minimum is the
//!   maximum-likelihood estimate of the noise-free cost.
//!
//! Not knobs: `rch` cannot pin a worker (`RCH_WORKER` is ignored; `RCH-E301` refuses non-compilation commands,
//! so remote `perf` is unavailable too). Quiescing the tree is a coordination act, not a code change.
//!
//! # Configurations are interleaved, not swept sequentially
//!
//! A sequential sweep confounds the configuration with time-varying machine load — the same error that
//! arm-interleaving exists to prevent, one level up. (I made it, then fixed it: `min_of=3` appeared to help at
//! 2 ms and hurt at 10/40 ms, which is incoherent as a configuration effect.) Each round measures **every**
//! configuration once, with a rotating start.

use fm_core::{ArrowType, DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
use fm_layout::{LayoutConfig, bench_internals};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

/// SHA-256 of this executable, reported from inside the measured process. A hash computed by a shell step
/// next to the run proves nothing about which ELF actually executed.
fn self_identity() -> String {
    let Ok(path) = std::env::current_exe() else {
        return "unavailable".to_string();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return "unavailable".to_string();
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!("{:x} ({} bytes)", hasher.finalize(), bytes.len())
}

/// Exact port of `scripts/headtohead/corpus.mjs::cyclic`: rings of `ring` nodes, each fully cyclic, with
/// forward links to the next ring. 100 nodes / 195 edges, and it routes to Sugiyama.
fn cyclic_scc_ir(node_count: usize, ring: usize) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for index in 0..node_count {
        ir.nodes.push(IrNode {
            id: format!("C{index}"),
            ..IrNode::default()
        });
    }
    let edge = |ir: &mut MermaidDiagramIr, from: usize, to: usize| {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(from)),
            to: IrEndpoint::Node(IrNodeId(to)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
    };
    for index in 0..node_count {
        let ring_start = (index / ring) * ring;
        let next = ring_start + ((index - ring_start + 1) % ring);
        if next < node_count {
            edge(&mut ir, index, next);
        }
        if index + ring < node_count {
            edge(&mut ir, index, index + ring);
        }
    }
    ir
}

/// The functions whose floors we calibrate. All three live in committed `HEAD`; the peer's in-flight
/// `flat_csr` arm is deliberately not referenced.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Arm {
    BTreeMap,
    DenseRank,
    SinglePass,
}

impl Arm {
    const fn name(self) -> &'static str {
        match self {
            Self::BTreeMap => "btreemap",
            Self::DenseRank => "dense_rank",
            Self::SinglePass => "single_pass",
        }
    }
}

/// One timing: `batch` invocations, inputs and result through `black_box`, folded into a checksum a
/// dead-code-eliminated arm could not produce. Returns `(nanos_per_invocation, checksum)`.
fn time_once(
    arm: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
) -> (u64, u64) {
    let mut checksum: u64 = 0;
    let start = Instant::now();
    for _ in 0..batch {
        let (crossings, ordering) = match arm {
            Arm::BTreeMap => bench_internals::crossing_minimization_btreemap(
                black_box(ir),
                black_box(ranks),
                black_box(config),
            ),
            Arm::DenseRank => bench_internals::crossing_minimization_dense_rank(
                black_box(ir),
                black_box(ranks),
                black_box(config),
            ),
            Arm::SinglePass => bench_internals::crossing_minimization_single_pass(
                black_box(ir),
                black_box(ranks),
                black_box(config),
            ),
        };
        let crossings = black_box(crossings);
        let ordering = black_box(ordering);
        checksum = checksum
            .wrapping_add(crossings as u64)
            .wrapping_add(ordering.len() as u64);
    }
    let elapsed = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
    (elapsed / u64::from(batch.max(1)), checksum)
}

/// Minimum of `replicates` back-to-back timings: scheduler noise is one-sided, so the minimum is the
/// maximum-likelihood estimate of the noise-free cost.
fn time_arm(
    arm: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    replicates: u32,
) -> (u64, u64) {
    let mut best = u64::MAX;
    let mut checksum: u64 = 0;
    for _ in 0..replicates.max(1) {
        let (ns, c) = time_once(arm, ir, ranks, config, batch);
        best = best.min(ns);
        checksum = checksum.wrapping_add(c);
    }
    (best, checksum)
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

/// Percentile-bootstrap 95% CI on the median of `ratios`. Deterministic xorshift so the reported interval is
/// reproducible from the same samples.
fn bootstrap_median_ci(ratios: &[f64]) -> (f64, f64) {
    const RESAMPLES: usize = 2000;
    let mut state: u64 = 0x2545_F491_4F6C_DD1D;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    let mut medians = Vec::with_capacity(RESAMPLES);
    let mut sample = vec![0.0_f64; ratios.len()];
    for _ in 0..RESAMPLES {
        for slot in &mut sample {
            let index = usize::try_from(next() >> 33).unwrap_or(0) % ratios.len();
            *slot = ratios[index];
        }
        medians.push(median(&mut sample));
    }
    medians.sort_by(f64::total_cmp);
    let lo = medians[RESAMPLES / 40]; // 2.5th percentile
    let hi = medians[RESAMPLES - 1 - RESAMPLES / 40]; // 97.5th percentile
    (lo, hi)
}

/// Smallest `batch` whose single timing spans at least `min_sample`.
fn calibrate_batch(
    arm: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    min_sample: Duration,
) -> u32 {
    let (per_ns, _) = time_once(arm, ir, ranks, config, 1);
    let target = u64::try_from(min_sample.as_nanos()).unwrap_or(2_000_000);
    u32::try_from(target / per_ns.max(1)).unwrap_or(1).max(1)
}

struct Config {
    arm: Arm,
    min_sample_ms: u64,
    min_of: u32,
    batch: u32,
    ratios: Vec<f64>,
}

fn main() {
    const ROUNDS: usize = 41;
    const MIN_SAMPLES_MS: [u64; 3] = [2, 10, 40];
    const MIN_OFS: [u32; 2] = [1, 3];

    println!("bench_elf_sha256={}", self_identity());
    println!("workload=cyclic_scc_100 (100 nodes / 195 edges, Sugiyama)");
    println!(
        "A/A NULL CONTROL: the same arm on both sides, so every deviation is the harness, not a lever."
    );
    println!(
        "Configurations are INTERLEAVED round-robin; a sequential sweep confounds config with load drift.\n"
    );

    let config = LayoutConfig::default();
    let ir = cyclic_scc_ir(100, 5);
    let ranks = bench_internals::prepare_ranks(&ir, &config);

    let mut configs: Vec<Config> = Vec::new();
    for arm in [Arm::BTreeMap, Arm::DenseRank, Arm::SinglePass] {
        for min_sample_ms in MIN_SAMPLES_MS {
            let batch = calibrate_batch(
                arm,
                &ir,
                &ranks,
                &config,
                Duration::from_millis(min_sample_ms),
            );
            for min_of in MIN_OFS {
                configs.push(Config {
                    arm,
                    min_sample_ms,
                    min_of,
                    batch,
                    ratios: Vec::with_capacity(ROUNDS),
                });
            }
        }
    }

    let mut checksum: u64 = 0;
    for cfg in &configs {
        let (_, c) = time_arm(cfg.arm, &ir, &ranks, &config, cfg.batch, cfg.min_of);
        checksum = checksum.wrapping_add(c);
    }

    let wall = Instant::now();
    for round in 0..ROUNDS {
        for offset in 0..configs.len() {
            let index = (round + offset) % configs.len();
            let (arm, batch, min_of) = {
                let cfg = &configs[index];
                (cfg.arm, cfg.batch, cfg.min_of)
            };
            let (first, c1) = time_arm(arm, &ir, &ranks, &config, batch, min_of);
            let (second, c2) = time_arm(arm, &ir, &ranks, &config, batch, min_of);
            checksum = checksum.wrapping_add(c1).wrapping_add(c2);
            // Alternate which timing is numerator: first-mover cache/branch state is the bias we expose.
            let (a, b) = if round % 2 == 0 {
                (first, second)
            } else {
                (second, first)
            };
            #[allow(clippy::cast_precision_loss)]
            configs[index].ratios.push(a as f64 / b.max(1) as f64);
        }
    }
    let wall = wall.elapsed();

    println!(
        "{:>12} {:>10} {:>6} {:>6} {:>11} {:>19} {:>9} {:>14}",
        "arm",
        "min_sample",
        "min_of",
        "batch",
        "null_median",
        "null 95% CI",
        "null_cv",
        "min_decidable"
    );
    let mut floors: Vec<(Arm, u64, u32, f64)> = Vec::new();
    for cfg in &configs {
        let mut ratios = cfg.ratios.clone();
        let med = median(&mut ratios);
        let (lo, hi) = bootstrap_median_ci(&cfg.ratios);
        #[allow(clippy::cast_precision_loss)]
        let mean: f64 = cfg.ratios.iter().sum::<f64>() / cfg.ratios.len() as f64;
        #[allow(clippy::cast_precision_loss)]
        let variance: f64 =
            cfg.ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / cfg.ratios.len() as f64;
        let cv_pct = (variance.sqrt() / mean) * 100.0;
        // A claim of size X is decidable iff X lies outside the null CI.
        let half = (hi - 1.0).abs().max((lo - 1.0).abs());
        floors.push((cfg.arm, cfg.min_sample_ms, cfg.min_of, half));
        println!(
            "{:>12} {:>8}ms {:>6} {:>6} {med:>10.4}x [{lo:>7.4},{hi:>7.4}] {cv_pct:>8.2}% {:>13.3}x",
            cfg.arm.name(),
            cfg.min_sample_ms,
            cfg.min_of,
            cfg.batch,
            1.0 + half,
        );
    }

    // Published settings: for each target effect, the cheapest configuration that decides it, per function.
    println!(
        "\nCALIBRATED SETTINGS -- cheapest config that decides an effect of size X (PER FUNCTION):"
    );
    println!(
        "{:>12} {:>12} {:>12} {:>12} {:>12} {:>12}",
        "arm", "1.02x", "1.05x", "1.10x", "1.25x", "1.50x"
    );
    for arm in [Arm::BTreeMap, Arm::DenseRank, Arm::SinglePass] {
        let mut cells = Vec::new();
        for target in [1.02_f64, 1.05, 1.10, 1.25, 1.50] {
            // Cheapest = smallest total sample time, i.e. min_sample * min_of.
            let mut best: Option<(u64, u32)> = None;
            for (a, ms, mo, half) in &floors {
                if *a == arm && target > 1.0 + *half {
                    let candidate = (*ms, *mo);
                    let cost = |c: (u64, u32)| c.0 * u64::from(c.1);
                    if best.is_none_or(|b| cost(candidate) < cost(b)) {
                        best = Some(candidate);
                    }
                }
            }
            cells.push(best.map_or_else(
                || "UNDECIDABLE".to_string(),
                |(ms, mo)| format!("{ms}ms/x{mo}"),
            ));
        }
        println!(
            "{:>12} {:>12} {:>12} {:>12} {:>12} {:>12}",
            arm.name(),
            cells[0],
            cells[1],
            cells[2],
            cells[3],
            cells[4]
        );
    }

    println!(
        "\nchecksum={checksum} rounds={ROUNDS} total_wall={:.1}s",
        wall.as_secs_f64()
    );
    println!("A claim of size X is decidable under config C iff X lies OUTSIDE C's null 95% CI.");
    println!(
        "The floor is PER-FUNCTION: read the row for the function you are about to benchmark."
    );
}
