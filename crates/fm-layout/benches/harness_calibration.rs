//! Calibrate the paired-sampler harness by measuring its **A/A null control** across a sweep of
//! configurations, so a future sub-2× claim in this repo can be decided at all.
//!
//! # Why this exists
//!
//! The first null-control reading of `barycenter_sweep` was **loose**: on `cyclic_scc_100` the identical
//! arm measured against itself gave `1.0357×` at `cv 14.17%`. A harness whose own noise floor is 14% cannot
//! decide a 5% lever, and *rejecting* a lever below that floor rejects the harness, not the lever. The
//! 3.669× barycenter win survives such a floor by two orders of magnitude; nothing subtler would.
//!
//! Three knobs are available. `rch` **cannot pin a worker**, so that lever is out of reach; quiescing the
//! tree is a coordination act, not a code one. What is left is the *shape of a sample*:
//!
//! 1. **Sample duration** (`min_sample`) — a preemption costs on the order of a millisecond. If a timed
//!    sample spans 2 ms, one preemption is a 50% outlier; if it spans 40 ms, it is 2.5%.
//! 2. **Inner min-of-k** — scheduler noise is *one-sided* (it only ever makes a sample slower). The
//!    minimum of `k` back-to-back timings is the maximum-likelihood estimate of the noise-free cost.
//!    This is the single most effective knob and it is why `cv` (which the outliers dominate) collapses
//!    toward `MAD` (which they do not).
//!
//! This bench sweeps both against a **pure A/A pair** — the same arm on both sides — so every departure
//! from `1.000` and every point of `cv` is attributable to the harness, never to a lever.
//!
//! # Reading the output
//!
//! A configuration is **fit to decide levers** when the null ratio sits within ~1% of `1.000` *and* its
//! `cv` clears the project's `< 5` gate. The smallest real effect it can then resolve is roughly the
//! null's departure from `1.000`, not its `cv`.

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
/// forward links to the next ring. `cyclic_scc_100` is 100 nodes / 195 edges and routes to Sugiyama.
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

/// One timing of the arm: `batch` invocations, inputs and result through `black_box`, folded into a
/// checksum a dead-code-eliminated arm could not produce. Returns `(nanos_per_invocation, checksum)`.
fn time_once(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
) -> (u64, u64) {
    let mut checksum: u64 = 0;
    let start = Instant::now();
    for _ in 0..batch {
        let (crossings, ordering) = bench_internals::crossing_minimization_dense_rank(
            black_box(ir),
            black_box(ranks),
            black_box(config),
        );
        let crossings = black_box(crossings);
        let ordering = black_box(ordering);
        checksum = checksum
            .wrapping_add(crossings as u64)
            .wrapping_add(ordering.len() as u64);
    }
    let elapsed = u64::try_from(start.elapsed().as_nanos()).unwrap_or(u64::MAX);
    (elapsed / u64::from(batch.max(1)), checksum)
}

/// Minimum of `replicates` back-to-back timings. Scheduler noise is one-sided, so the minimum is the
/// maximum-likelihood estimate of the noise-free cost.
fn time_arm(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    replicates: u32,
) -> (u64, u64) {
    let mut best = u64::MAX;
    let mut checksum: u64 = 0;
    for _ in 0..replicates.max(1) {
        let (ns, c) = time_once(ir, ranks, config, batch);
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

/// Smallest `batch` whose single timing spans at least `min_sample`.
fn calibrate(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    min_sample: Duration,
) -> u32 {
    let (per_ns, _) = time_once(ir, ranks, config, 1);
    let target = u64::try_from(min_sample.as_nanos()).unwrap_or(2_000_000);
    u32::try_from(target / per_ns.max(1)).unwrap_or(1).max(1)
}

/// The A/A null control: the identical arm on both sides, timed back-to-back per round with the order
/// alternating, statistic = median of per-round ratios, `cv` taken over those ratios.
fn null_control(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    replicates: u32,
    rounds: usize,
) -> (f64, f64, f64, u64, Duration) {
    let wall = Instant::now();
    let mut checksum: u64 = 0;
    let mut ratios = Vec::with_capacity(rounds);
    for round in 0..rounds {
        // Both sides are the same arm; alternating still matters because first-mover cache/branch state
        // is exactly the bias we are trying to expose.
        let (first, c1) = time_arm(ir, ranks, config, batch, replicates);
        let (second, c2) = time_arm(ir, ranks, config, batch, replicates);
        checksum = checksum.wrapping_add(c1).wrapping_add(c2);
        let (a, b) = if round % 2 == 0 {
            (first, second)
        } else {
            (second, first)
        };
        ratios.push(a as f64 / b.max(1) as f64);
    }
    let ratio_p50 = median(&mut ratios.clone());
    let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let variance: f64 =
        ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
    let cv_pct = (variance.sqrt() / mean) * 100.0;
    let mut deviations: Vec<f64> = ratios.iter().map(|r| (r - ratio_p50).abs()).collect();
    let mad_pct = (median(&mut deviations) / ratio_p50) * 100.0;
    (ratio_p50, cv_pct, mad_pct, checksum, wall.elapsed())
}

/// One A/A paired round for a single configuration. Returns the per-round ratio and a checksum.
fn null_round(
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    replicates: u32,
    round: usize,
) -> (f64, u64) {
    let (first, c1) = time_arm(ir, ranks, config, batch, replicates);
    let (second, c2) = time_arm(ir, ranks, config, batch, replicates);
    let (a, b) = if round % 2 == 0 {
        (first, second)
    } else {
        (second, first)
    };
    (a as f64 / b.max(1) as f64, c1.wrapping_add(c2))
}

fn stats(ratios: &[f64]) -> (f64, f64, f64) {
    let ratio_p50 = median(&mut ratios.to_vec());
    let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let variance: f64 =
        ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
    let cv_pct = (variance.sqrt() / mean) * 100.0;
    let mut deviations: Vec<f64> = ratios.iter().map(|r| (r - ratio_p50).abs()).collect();
    let mad_pct = (median(&mut deviations) / ratio_p50) * 100.0;
    (ratio_p50, cv_pct, mad_pct)
}

fn main() {
    const ROUNDS: usize = 41;
    println!("bench_elf_sha256={}", self_identity());
    println!(
        "workload=cyclic_scc_100 (100 nodes / 195 edges, Sugiyama), arm=crossing_minimization_dense_rank"
    );
    println!("A/A NULL CONTROL: both sides are the SAME arm, so every deviation is the harness.");
    println!(
        "Configurations are INTERLEAVED round-robin, not swept sequentially: a sequential sweep confounds\n         the configuration with time-varying machine load, which is the same error that arm-interleaving\n         exists to prevent. Each round measures every configuration once.\n"
    );

    let config = LayoutConfig::default();
    let ir = cyclic_scc_ir(100, 5);
    let ranks = bench_internals::prepare_ranks(&ir, &config);

    // (min_sample_ms, replicates) -> batch, calibrated once up front.
    let mut configs: Vec<(u64, u32, u32, Vec<f64>)> = Vec::new();
    for min_sample_ms in [2_u64, 10, 40] {
        let batch = calibrate(&ir, &ranks, &config, Duration::from_millis(min_sample_ms));
        for replicates in [1_u32, 3] {
            configs.push((min_sample_ms, replicates, batch, Vec::with_capacity(ROUNDS)));
        }
    }

    let mut checksum: u64 = 0;
    for (_, replicates, batch, _) in &configs {
        let (_, c) = time_arm(&ir, &ranks, &config, *batch, *replicates);
        checksum = checksum.wrapping_add(c);
    }

    let wall = Instant::now();
    for round in 0..ROUNDS {
        // Rotate the starting configuration each round so no configuration is permanently first.
        for offset in 0..configs.len() {
            let index = (round + offset) % configs.len();
            let (_, replicates, batch, _) = configs[index];
            let (ratio, c) = null_round(&ir, &ranks, &config, batch, replicates, round);
            checksum = checksum.wrapping_add(c);
            configs[index].3.push(ratio);
        }
    }
    let wall = wall.elapsed();

    println!(
        "{:>11} {:>7} {:>6} {:>12} {:>9} {:>9} {:>10}",
        "min_sample", "min_of", "batch", "null_ratio", "null_cv", "null_mad", "resolves"
    );
    for (min_sample_ms, replicates, batch, ratios) in &configs {
        let (ratio, cv, mad) = stats(ratios);
        let departure = (ratio - 1.0).abs() * 100.0;
        // The smallest effect a harness can resolve is its SYSTEMATIC floor -- how far the A/A median sits
        // from 1.000 -- not its cv, which one-sided preemption outliers inflate without biasing the median.
        println!(
            "{min_sample_ms:>9}ms {replicates:>7} {batch:>6} {ratio:>11.4}x {cv:>8.2}% {mad:>8.2}% {:>10}",
            format!("{departure:.2}%"),
        );
    }
    println!(
        "\nchecksum={checksum} rounds={ROUNDS} total_wall={:.1}s",
        wall.as_secs_f64()
    );
    println!(
        "rch cannot pin a worker, so worker choice is not a knob. Quiescing the tree is coordination,"
    );
    println!("not code. What remains is sample duration and inner min-of-k -- both swept above.");
}
