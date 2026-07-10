//! Same-binary **paired-sample** A/B for packed flat-CSR barycenter incidence (`bd-1buv.4`).
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
    SinglePass,
    FlatCsr,
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
            Arm::SinglePass => bench_internals::crossing_minimization_single_pass(
                black_box(ir),
                black_box(ranks),
                black_box(config),
            ),
            Arm::FlatCsr => bench_internals::crossing_minimization_flat_csr(
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
const MIN_SAMPLE: Duration = Duration::from_millis(200);

/// Size the batch from the **faster** arm.
///
/// Calibrating on the slow arm can leave the CSR arm's samples under `MIN_SAMPLE`, so the *ratio's* `cv`
/// would be dominated by timer
/// noise on the fast arm and read 5.8–13.4%. Both arms share one `batch`, so it must be chosen such
/// that the SHORTER of the two samples clears the floor; the slower arm then clears it a fortiori.
fn calibrate(ir: &MermaidDiagramIr, ranks: &BTreeMap<usize, usize>, config: &LayoutConfig) -> u32 {
    let (per_ns, _) = time_arm(Arm::FlatCsr, ir, ranks, config, 1);
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
        "orig" => Arm::SinglePass,
        "cand" => Arm::FlatCsr,
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

/// One micro-interleaved paired round. Every iteration times one invocation of each arm, and both the
/// iteration and round parity alternate which arm goes first. Summing per-arm time across the round makes
/// co-tenant scheduling land symmetrically instead of exposing two long whole-arm phases.
fn paired_round(
    arm_a: Arm,
    arm_b: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    a_first: bool,
) -> (u64, u64, u64) {
    let mut a_total = 0_u128;
    let mut b_total = 0_u128;
    let mut checksum = 0_u64;
    for iteration in 0..batch.max(1) {
        let iteration_a_first = (iteration.is_multiple_of(2)) == a_first;
        let (a_ns, b_ns) = if iteration_a_first {
            let (a, c1) = time_arm(arm_a, ir, ranks, config, 1);
            let (b, c2) = time_arm(arm_b, ir, ranks, config, 1);
            checksum = checksum.wrapping_add(c1).wrapping_add(c2);
            (a, b)
        } else {
            let (b, c2) = time_arm(arm_b, ir, ranks, config, 1);
            let (a, c1) = time_arm(arm_a, ir, ranks, config, 1);
            checksum = checksum.wrapping_add(c1).wrapping_add(c2);
            (a, b)
        };
        a_total = a_total.saturating_add(u128::from(a_ns));
        b_total = b_total.saturating_add(u128::from(b_ns));
    }
    let denominator = u128::from(batch.max(1));
    (
        u64::try_from(a_total / denominator).unwrap_or(u64::MAX),
        u64::try_from(b_total / denominator).unwrap_or(u64::MAX),
        checksum,
    )
}

/// One paired measurement: `ROUNDS` rounds, each micro-interleaving `arm_a` and `arm_b` per invocation.
/// Returns `(p50_a_ns, p50_b_ns, ratio_p50, cv_pct, mad_pct, checksum)` where `ratio = a / b` and
/// `cv` is taken over the **per-round ratios** — the quantity being claimed.
///
/// Passing the SAME arm twice makes this an **A/A null control**: it measures the harness's own noise
/// floor. Any "win" smaller than the null control's departure from 1.000 is indistinguishable from noise,
/// and any REJECT of a lever whose effect is below that floor is meaningless.
fn paired(
    arm_a: Arm,
    arm_b: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    rounds: usize,
) -> (f64, f64, f64, f64, f64, u64) {
    let mut checksum: u64 = 0;
    let mut a_samples = Vec::with_capacity(rounds);
    let mut b_samples = Vec::with_capacity(rounds);
    let mut ratios = Vec::with_capacity(rounds);
    for round in 0..rounds {
        let (a_ns, b_ns, round_checksum) = paired_round(
            arm_a,
            arm_b,
            ir,
            ranks,
            config,
            batch,
            round.is_multiple_of(2),
        );
        checksum = checksum.wrapping_add(round_checksum);
        a_samples.push(a_ns as f64);
        b_samples.push(b_ns as f64);
        ratios.push(a_ns as f64 / b_ns.max(1) as f64);
    }
    let a_p50 = median(&mut a_samples.clone());
    let b_p50 = median(&mut b_samples.clone());
    let ratio_p50 = median(&mut ratios.clone());
    let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let variance: f64 =
        ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
    let cv_pct = (variance.sqrt() / mean) * 100.0;
    let mut deviations: Vec<f64> = ratios.iter().map(|r| (r - ratio_p50).abs()).collect();
    let mad_pct = (median(&mut deviations) / ratio_p50) * 100.0;
    (a_p50, b_p50, ratio_p50, cv_pct, mad_pct, checksum)
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
        "{:<16} {:>6} {:>6} {:>11} {:>8} {:>8}  {:>11} {:>8} {:>8}",
        "case",
        "nodes",
        "edges",
        "NULL a/a",
        "null_cv",
        "null_mad",
        "A/B ratio",
        "cv_pct",
        "mad_pct"
    );

    for (label, node_count, ring) in [
        ("cyclic_scc_100", 100_usize, 5_usize),
        ("cyclic_scc_300", 300, 5),
        ("cyclic_scc_800", 800, 5),
    ] {
        let ir = cyclic_scc_ir(node_count, ring);
        let ranks = bench_internals::prepare_ranks(&ir, &config);
        let orig = bench_internals::crossing_minimization_single_pass(&ir, &ranks, &config);
        let candidate = bench_internals::crossing_minimization_flat_csr(&ir, &ranks, &config);
        assert_eq!(
            orig, candidate,
            "flat-CSR candidate changed ordering for {label}"
        );
        let batch = calibrate(&ir, &ranks, &config);

        let mut checksum: u64 = 0;
        for _ in 0..WARMUP {
            let (_, c1) = time_arm(Arm::SinglePass, &ir, &ranks, &config, batch);
            let (_, c2) = time_arm(Arm::FlatCsr, &ir, &ranks, &config, batch);
            checksum = checksum.wrapping_add(c1).wrapping_add(c2);
        }

        // NULL CONTROL first: the identical arm against itself, same interleaved routine, same batch.
        // This is the harness's noise floor. A ratio far from 1.000, or a loose cv, means the harness
        // is not fit to decide the lever -- fix the harness before drawing any conclusion.
        let (_, _, null_ratio, null_cv, null_mad, c_null) = paired(
            Arm::SinglePass,
            Arm::SinglePass,
            &ir,
            &ranks,
            &config,
            batch,
            ROUNDS,
        );
        // The real A/B, measured by the same routine.
        let (single_p50, csr_p50, ratio, cv_pct, mad_pct, c_ab) = paired(
            Arm::SinglePass,
            Arm::FlatCsr,
            &ir,
            &ranks,
            &config,
            batch,
            ROUNDS,
        );
        checksum = checksum.wrapping_add(c_null).wrapping_add(c_ab);

        println!(
            "{label:<16} {:>6} {:>6} {:>10.4}x {:>8.2} {:>8.2}  {:>10.3}x {:>8.2} {:>8.2}",
            ir.nodes.len(),
            ir.edges.len(),
            null_ratio,
            null_cv,
            null_mad,
            ratio,
            cv_pct,
            mad_pct,
        );
        println!(
            "                 single_p50={single_p50:.1}ns csr_p50={csr_p50:.1}ns \
checksum={checksum} batch={batch} rounds={ROUNDS}"
        );
    }
}
