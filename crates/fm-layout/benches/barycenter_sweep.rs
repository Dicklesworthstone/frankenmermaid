//! Same-binary **paired-sample** A/B for the barycenter crossing-minimization sweep (`bd-9w78`).
//!
//! # Why this harness exists instead of a criterion group
//!
//! Two substrate defects make the obvious approaches invalid here:
//!
//! 1. **`rch exec` cannot pin a worker** and selects them non-deterministically, so an ORIG/CAND ratio
//!    split across two `rch exec` invocations is meaningless. Both arms must live in one binary.
//! 2. **Criterion group members run sequentially, not interleaved.** Registering `orig` and `cand` as
//!    two `bench_with_input` calls in one group does *not* cancel worker/thermal drift: each arm is
//!    measured in its own time window. To actually cancel drift the arms must be interleaved *within a
//!    single measured routine*.
//!
//! So this is a hand-rolled paired sampler. Each **round** times both arms back-to-back and emits one
//! `(orig_ns, cand_ns)` pair; the statistic reported is the median of the **per-round ratios**, whose
//! `cv` is computed over those ratios. Drift that is slow relative to a round cancels inside the pair.
//! Round order alternates (`orig,cand` / `cand,orig`) so first-mover cache/branch-predictor bias cancels
//! across rounds too.
//!
//! # Anti-DCE discipline
//!
//! Every input goes through `black_box` and every result is consumed through `black_box`, then folded
//! into a checksum that is printed. A dead-code-eliminated arm cannot produce the checksum.
//!
//! # Why this input and not `layout_wide`
//!
//! `pipeline_bench::layout_wide` builds graphs with `gen_wide()`, which the auto-selector routes to the
//! **Tree** layout: `perf` self-time of `reorder_rank_by_barycenter` there is **0.000%**. Four prior
//! rejections of this exact code were A/B'd on that bench and therefore measured nothing. The graphs
//! below reproduce the ring-of-five `cyclic_scc_100` corpus topology exactly, then scale the same shape
//! to 300 and 800 nodes. On `cyclic_scc_100` the sweep is **47.640%** of the whole
//! parse+layout+render pipeline.

use fm_core::{ArrowType, DiagramType, IrEdge, IrEndpoint, IrNode, IrNodeId, MermaidDiagramIr};
use fm_layout::{LayoutConfig, bench_internals};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::env;
use std::hint::black_box;
use std::time::{Duration, Instant};

/// Exact Rust port of `scripts/headtohead/corpus.mjs::cyclic`: rings of `ring` nodes, each fully
/// cyclic, with forward links to the next ring. `cyclic_scc_100` is 100 nodes / 195 edges.
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

/// Which implementation an arm runs. Both are reachable from `bench_internals`.
#[derive(Clone, Copy)]
enum Arm {
    DenseRank,
    SinglePass,
}

/// Time `batch` invocations of one arm, feeding inputs and results through `black_box`. Returns
/// `(nanos_per_invocation, checksum)`.
fn time_arm(
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
        // Consume BOTH results through black_box, then fold into a checksum an eliminated arm could
        // not produce.
        let crossings = black_box(crossings);
        let ordering = black_box(ordering);
        checksum = checksum
            .wrapping_add(crossings as u64)
            .wrapping_add(ordering.len() as u64);
    }
    let elapsed = start.elapsed();
    let per = u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX) / u64::from(batch.max(1));
    (per, checksum)
}

/// Smallest `batch` whose single timing spans at least this long. A sample shorter than a few timer
/// interrupts measures the kernel, not the sweep.
const MIN_SAMPLE: Duration = Duration::from_millis(2);

/// Size the batch from the **faster** arm.
///
/// Calibrating on `OrigBTreeMap` (the slow arm) yielded `batch = 1`, which left the dense arm's samples
/// at ~220 µs — an order of magnitude under `MIN_SAMPLE` — so the *ratio's* `cv` was dominated by timer
/// noise on the fast arm and read 5.8–13.4%. Both arms share one `batch`, so it must be chosen such
/// that the SHORTER of the two samples clears the floor; the slower arm then clears it a fortiori.
fn calibrate(ir: &MermaidDiagramIr, ranks: &BTreeMap<usize, usize>, config: &LayoutConfig) -> u32 {
    let (per_ns, _) = time_arm(Arm::SinglePass, ir, ranks, config, 1);
    let target = u64::try_from(MIN_SAMPLE.as_nanos()).unwrap_or(2_000_000);
    u32::try_from(target / per_ns.max(1)).unwrap_or(1).max(1)
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

/// Profile one arm in isolation from the exact same executable used for the paired A/B. This mode is
/// never used for the timing verdict; it exists solely for the ledger-integrity requirement that each
/// arm show non-zero self-time in the function under test.
fn profile_arm_if_requested() -> bool {
    let Ok(requested) = env::var("FM_BARYCENTER_PROFILE_ARM") else {
        return false;
    };
    let arm = match requested.as_str() {
        "orig" => Arm::DenseRank,
        "cand" => Arm::SinglePass,
        _ => panic!("FM_BARYCENTER_PROFILE_ARM must be 'orig' or 'cand'"),
    };
    let iterations = env::var("FM_BARYCENTER_PROFILE_ITERS")
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(20_000);
    let config = LayoutConfig::default();
    let ir = cyclic_scc_ir(100, 5);
    let ranks = bench_internals::prepare_ranks(&ir, &config);
    let (per_ns, checksum) = time_arm(arm, &ir, &ranks, &config, iterations);
    println!(
        "profile_arm={requested} nodes={} edges={} iterations={iterations} per_ns={per_ns} checksum={checksum}",
        ir.nodes.len(),
        ir.edges.len(),
    );
    true
}

/// SHA-256 of this executable, reported from inside the measured process. Certification records the
/// binary identity; computing it in a separate shell step could not prove it was the ELF that ran.
fn self_identity() -> String {
    let Ok(path) = env::current_exe() else {
        return "unavailable".to_string();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return "unavailable".to_string();
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    format!(
        "{:x} ({} bytes) {}",
        hasher.finalize(),
        bytes.len(),
        path.display()
    )
}

fn main() {
    const ROUNDS: usize = 41;
    const WARMUP: u32 = 3;

    println!("bench_elf_sha256={}", self_identity());

    if profile_arm_if_requested() {
        return;
    }

    let config = LayoutConfig::default();
    println!(
        "{:<10} {:>6} {:>6} {:>13} {:>13} {:>10} {:>8} {:>8}",
        "case", "nodes", "edges", "dense_p50_us", "single_p50_us", "speedup", "cv_pct", "mad_pct"
    );

    for (label, node_count, ring) in [
        ("cyclic_scc_100", 100_usize, 5_usize),
        ("cyclic_scc_300", 300, 5),
        ("cyclic_scc_800", 800, 5),
    ] {
        let ir = cyclic_scc_ir(node_count, ring);
        let ranks = bench_internals::prepare_ranks(&ir, &config);
        let orig = bench_internals::crossing_minimization_dense_rank(&ir, &ranks, &config);
        let candidate = bench_internals::crossing_minimization_single_pass(&ir, &ranks, &config);
        assert_eq!(
            orig, candidate,
            "single-pass candidate changed ordering for {label}"
        );
        let batch = calibrate(&ir, &ranks, &config);

        let mut checksum: u64 = 0;
        for _ in 0..WARMUP {
            let (_, c1) = time_arm(Arm::DenseRank, &ir, &ranks, &config, batch);
            let (_, c2) = time_arm(Arm::SinglePass, &ir, &ranks, &config, batch);
            checksum = checksum.wrapping_add(c1).wrapping_add(c2);
        }

        let mut orig_samples = Vec::with_capacity(ROUNDS);
        let mut dense_samples = Vec::with_capacity(ROUNDS);
        let mut ratios = Vec::with_capacity(ROUNDS);
        for round in 0..ROUNDS {
            // Alternate which arm goes first so first-mover bias cancels across rounds.
            let (orig_ns, dense_ns) = if round % 2 == 0 {
                let (o, c1) = time_arm(Arm::DenseRank, &ir, &ranks, &config, batch);
                let (d, c2) = time_arm(Arm::SinglePass, &ir, &ranks, &config, batch);
                checksum = checksum.wrapping_add(c1).wrapping_add(c2);
                (o, d)
            } else {
                let (d, c2) = time_arm(Arm::SinglePass, &ir, &ranks, &config, batch);
                let (o, c1) = time_arm(Arm::DenseRank, &ir, &ranks, &config, batch);
                checksum = checksum.wrapping_add(c1).wrapping_add(c2);
                (o, d)
            };
            orig_samples.push(orig_ns as f64);
            dense_samples.push(dense_ns as f64);
            ratios.push(orig_ns as f64 / dense_ns.max(1) as f64);
        }

        let orig_p50 = median(&mut orig_samples.clone());
        let dense_p50 = median(&mut dense_samples.clone());
        let ratio_p50 = median(&mut ratios.clone());

        // cv over the PER-ROUND RATIOS -- the quantity actually being claimed.
        let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
        let variance: f64 =
            ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
        let cv_pct = (variance.sqrt() / mean) * 100.0;
        let mut deviations: Vec<f64> = ratios.iter().map(|r| (r - ratio_p50).abs()).collect();
        let mad_pct = (median(&mut deviations) / ratio_p50) * 100.0;

        println!(
            "{label:<10} {:>6} {:>6} {:>13.1} {:>13.1} {:>9.3}x {:>8.2} {:>8.2}",
            ir.nodes.len(),
            ir.edges.len(),
            orig_p50 / 1000.0,
            dense_p50 / 1000.0,
            ratio_p50,
            cv_pct,
            mad_pct,
        );
        // Printed so neither arm can be dead-code-eliminated without the output changing.
        println!("           checksum={checksum} batch={batch} rounds={ROUNDS}");
        print!("           samples_ns=");
        for (index, (orig_ns, dense_ns)) in orig_samples.iter().zip(&dense_samples).enumerate() {
            if index > 0 {
                print!(",");
            }
            print!("{orig_ns:.0}:{dense_ns:.0}");
        }
        println!();
    }
}
