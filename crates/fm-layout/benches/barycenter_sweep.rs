//! Same-binary **paired-sample** A/B for the zero-allocation packed crossing counter (`bd-1buv.4`).
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
//! `(orig_ns, cand_ns)` pair; the statistic reported is the median of the **per-round ratios** with a
//! bootstrap 95% CI. `cv` and MAD are report-only provenance: the verdict is gated against the A/A
//! null-median CI at a mandatory 2× margin. Drift that is slow relative to a round cancels inside the
//! pair. Round order alternates (`orig,cand` / `cand,orig`) so first-mover cache/branch-predictor bias
//! cancels across rounds too.
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

/// Historical-to-current implementation lineage. Keeping every arm in one executable lets the
/// ledger-resurrection audit re-adjudicate the formerly VOID comparisons without cross-worker drift.
#[derive(Clone, Copy)]
enum Arm {
    BTreeMap,
    DenseRank,
    SinglePass,
    FlatCsr,
    PackedCrossings,
}

impl Arm {
    const fn name(self) -> &'static str {
        match self {
            Self::BTreeMap => "btreemap",
            Self::DenseRank => "dense_rank",
            Self::SinglePass => "single_pass",
            Self::FlatCsr => "flat_csr",
            Self::PackedCrossings => "packed_crossings",
        }
    }
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
            Arm::FlatCsr => bench_internals::crossing_minimization_flat_csr(
                black_box(ir),
                black_box(ranks),
                black_box(config),
            ),
            Arm::PackedCrossings => bench_internals::crossing_minimization_packed_crossings(
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

/// Calibrated lane default: short samples plus three back-to-back replicates produce a tighter
/// null-median CI than long samples on shared workers. `cv` is not a gate.
const MIN_SAMPLE: Duration = Duration::from_millis(2);
const MIN_OF: u32 = 3;

/// Size the batch from the **faster** arm.
///
/// Calibrating on the slow arm can leave the packed arm's samples under `MIN_SAMPLE`, so the *ratio's* `cv`
/// would be dominated by timer
/// noise on the fast arm and read 5.8–13.4%. Both arms share one `batch`, so it must be chosen such
/// that the SHORTER of the two samples clears the floor; the slower arm then clears it a fortiori.
fn calibrate(
    arm_a: Arm,
    arm_b: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
) -> u32 {
    let (a_ns, _) = time_arm(arm_a, ir, ranks, config, 1);
    let (b_ns, _) = time_arm(arm_b, ir, ranks, config, 1);
    let faster_ns = a_ns.min(b_ns);
    let target = u64::try_from(MIN_SAMPLE.as_nanos()).unwrap_or(2_000_000);
    u32::try_from(target / faster_ns.max(1)).unwrap_or(1).max(1)
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

/// Percentile-bootstrap 95% CI on the median of `ratios`. The deterministic xorshift makes the
/// reported interval reproducible from the same samples.
fn bootstrap_median_ci(ratios: &[f64]) -> (f64, f64) {
    const RESAMPLES: usize = 2_000;
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
    (
        medians[RESAMPLES / 40],
        medians[RESAMPLES - 1 - RESAMPLES / 40],
    )
}

/// Minimum of `replicates` back-to-back timings. Shared-worker scheduling noise is one-sided, so
/// the minimum is the best estimate of the unpreempted cost.
fn time_arm_min(
    arm: Arm,
    ir: &MermaidDiagramIr,
    ranks: &BTreeMap<usize, usize>,
    config: &LayoutConfig,
    batch: u32,
    replicates: u32,
) -> (u64, u64) {
    let mut best = u64::MAX;
    let mut checksum = 0_u64;
    for _ in 0..replicates.max(1) {
        let (ns, arm_checksum) = time_arm(arm, ir, ranks, config, batch);
        best = best.min(ns);
        checksum = checksum.wrapping_add(arm_checksum);
    }
    (best, checksum)
}

/// Profile one arm in isolation from the exact same executable used for the paired A/B. This mode is
/// never used for the timing verdict; it exists solely for the ledger-integrity requirement that each
/// arm show non-zero self-time in the function under test.
fn profile_arm_if_requested() -> bool {
    let Ok(requested) = env::var("FM_BARYCENTER_PROFILE_ARM") else {
        return false;
    };
    let arm = match requested.as_str() {
        "orig" => Arm::FlatCsr,
        "cand" => Arm::PackedCrossings,
        "btreemap" => Arm::BTreeMap,
        "dense_rank" => Arm::DenseRank,
        "single_pass" => Arm::SinglePass,
        "flat_csr" => Arm::FlatCsr,
        "packed_crossings" => Arm::PackedCrossings,
        _ => panic!(
            "FM_BARYCENTER_PROFILE_ARM must be orig, cand, btreemap, dense_rank, \
             single_pass, flat_csr, or packed_crossings"
        ),
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
        "profile_arm={} nodes={} edges={} iterations={iterations} per_ns={per_ns} checksum={checksum}",
        arm.name(),
        ir.nodes.len(),
        ir.edges.len(),
    );
    true
}

/// SHA-256 of this executable, reported from inside the measured process. Certification records the
/// binary identity; computing it in a separate shell step could not prove it was the ELF that ran.
fn self_identity() -> String {
    use std::fmt::Write as _;

    let Ok(path) = env::current_exe() else {
        return "unavailable".to_string();
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return "unavailable".to_string();
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let digest = hasher.finalize();
    let mut sha256 = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(sha256, "{byte:02x}").expect("writing to String cannot fail");
    }
    format!("{} ({} bytes) {}", sha256, bytes.len(), path.display())
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
            let (a, c1) = time_arm_min(arm_a, ir, ranks, config, 1, MIN_OF);
            let (b, c2) = time_arm_min(arm_b, ir, ranks, config, 1, MIN_OF);
            checksum = checksum.wrapping_add(c1).wrapping_add(c2);
            (a, b)
        } else {
            let (b, c2) = time_arm_min(arm_b, ir, ranks, config, 1, MIN_OF);
            let (a, c1) = time_arm_min(arm_a, ir, ranks, config, 1, MIN_OF);
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

struct PairedStats {
    a_p50_ns: f64,
    b_p50_ns: f64,
    ratio_p50: f64,
    ratio_ci: (f64, f64),
    cv_pct: f64,
    mad_pct: f64,
    checksum: u64,
}

/// One paired measurement: `ROUNDS` rounds, each micro-interleaving `arm_a` and `arm_b` per invocation.
/// `ratio = a / b`; the claim is the median of per-round ratios and its bootstrap CI. `cv` and MAD
/// are retained only as provenance.
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
) -> PairedStats {
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
    let a_p50 = median(&mut a_samples);
    let b_p50 = median(&mut b_samples);
    let ratio_p50 = median(&mut ratios.clone());
    let ratio_ci = bootstrap_median_ci(&ratios);
    let mean: f64 = ratios.iter().sum::<f64>() / ratios.len() as f64;
    let variance: f64 =
        ratios.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / ratios.len() as f64;
    let cv_pct = (variance.sqrt() / mean) * 100.0;
    let mut deviations: Vec<f64> = ratios.iter().map(|r| (r - ratio_p50).abs()).collect();
    let mad_pct = (median(&mut deviations) / ratio_p50) * 100.0;
    PairedStats {
        a_p50_ns: a_p50,
        b_p50_ns: b_p50,
        ratio_p50,
        ratio_ci,
        cv_pct,
        mad_pct,
        checksum,
    }
}

/// A claim is certifiable only when its distance from 1.0 clears the A/A null CI half-width by
/// at least 2×. The returned margin is reportable even for an indeterminate result.
fn clears_null_ci_at_two_x(null_ci: (f64, f64), claim: f64) -> (bool, f64) {
    let half_width = (null_ci.0 - 1.0).abs().max((null_ci.1 - 1.0).abs());
    let distance = (claim - 1.0).abs();
    let margin = if half_width > 0.0 {
        distance / half_width
    } else {
        f64::INFINITY
    };
    (distance >= 2.0 * half_width, margin)
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
        "{:<17} {:<16} {:>6} {:>6} {:>11} {:>19} {:>11} {:>19} {:>9} {:>15}",
        "comparison",
        "case",
        "nodes",
        "edges",
        "NULL a/a",
        "null 95% CI",
        "A/B ratio",
        "A/B 95% CI",
        "CI margin",
        "verdict"
    );

    // Five ledger-resurrection comparisons. The first directly re-runs the 2026-06-26 VOID
    // adjacency hypothesis against the historical BTreeMap implementation; the remaining four
    // isolate each production stage in the lineage so every claimed increment gets its own null.
    let comparisons = [
        ("void_adjacency", Arm::BTreeMap, Arm::FlatCsr),
        ("dense_rank", Arm::BTreeMap, Arm::DenseRank),
        ("single_pass", Arm::DenseRank, Arm::SinglePass),
        ("flat_csr", Arm::SinglePass, Arm::FlatCsr),
        ("packed_crossings", Arm::FlatCsr, Arm::PackedCrossings),
    ];

    for (label, node_count, ring) in [
        ("cyclic_scc_100", 100_usize, 5_usize),
        ("cyclic_scc_300", 300, 5),
        ("cyclic_scc_800", 800, 5),
    ] {
        let ir = cyclic_scc_ir(node_count, ring);
        let ranks = bench_internals::prepare_ranks(&ir, &config);
        let expected = bench_internals::crossing_minimization_btreemap(&ir, &ranks, &config);
        for arm in [
            Arm::DenseRank,
            Arm::SinglePass,
            Arm::FlatCsr,
            Arm::PackedCrossings,
        ] {
            assert_eq!(
                expected,
                match arm {
                    Arm::DenseRank => {
                        bench_internals::crossing_minimization_dense_rank(&ir, &ranks, &config)
                    }
                    Arm::SinglePass => {
                        bench_internals::crossing_minimization_single_pass(&ir, &ranks, &config)
                    }
                    Arm::FlatCsr => {
                        bench_internals::crossing_minimization_flat_csr(&ir, &ranks, &config)
                    }
                    Arm::PackedCrossings => {
                        bench_internals::crossing_minimization_packed_crossings(
                            &ir, &ranks, &config,
                        )
                    }
                    Arm::BTreeMap => unreachable!("BTreeMap is the expected reference"),
                },
                "{} changed ordering for {label}",
                arm.name(),
            );
        }

        for (comparison, arm_a, arm_b) in comparisons {
            let batch = calibrate(arm_a, arm_b, &ir, &ranks, &config);
            let mut checksum: u64 = 0;
            for _ in 0..WARMUP {
                let (_, c1) = time_arm_min(arm_a, &ir, &ranks, &config, batch, MIN_OF);
                let (_, c2) = time_arm_min(arm_b, &ir, &ranks, &config, batch, MIN_OF);
                checksum = checksum.wrapping_add(c1).wrapping_add(c2);
            }

            // NULL CONTROL first: the identical baseline arm against itself, same routine and batch.
            // CV and MAD are printed but never gate the verdict.
            let null = paired(arm_a, arm_a, &ir, &ranks, &config, batch, ROUNDS);
            let real = paired(arm_a, arm_b, &ir, &ranks, &config, batch, ROUNDS);
            checksum = checksum
                .wrapping_add(null.checksum)
                .wrapping_add(real.checksum);
            let (decidable, ci_margin) = clears_null_ci_at_two_x(null.ratio_ci, real.ratio_p50);
            let verdict = if !decidable {
                "INDETERMINATE"
            } else if real.ratio_p50 > 1.0 {
                "CAND_FASTER"
            } else {
                "CAND_SLOWER"
            };

            println!(
                "{comparison:<17} {label:<16} {:>6} {:>6} {:>10.4}x \
[{:>7.4},{:>7.4}] {:>10.3}x [{:>7.4},{:>7.4}] {:>8.2}x {verdict:>15}",
                ir.nodes.len(),
                ir.edges.len(),
                null.ratio_p50,
                null.ratio_ci.0,
                null.ratio_ci.1,
                real.ratio_p50,
                real.ratio_ci.0,
                real.ratio_ci.1,
                ci_margin,
            );
            println!(
                "  arms={}/{} a_p50={:.1}ns b_p50={:.1}ns null_cv={:.2}% null_mad={:.2}% \
ab_cv={:.2}% ab_mad={:.2}% checksum={checksum} batch={batch} min_of={MIN_OF} \
rounds={ROUNDS}",
                arm_a.name(),
                arm_b.name(),
                real.a_p50_ns,
                real.b_p50_ns,
                null.cv_pct,
                null.mad_pct,
                real.cv_pct,
                real.mad_pct,
            );
        }
    }
}
