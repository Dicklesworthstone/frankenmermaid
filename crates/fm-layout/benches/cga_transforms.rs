//! Head-to-head: CGA rotor composition versus direct 2D affine matrices (bd-2q3f.4, bd-9mqa).
//!
//! This exists to settle a live architectural question with a number rather than a preference:
//! bd-2q3f's criteria are written as though CGA operators are being adopted, and bd-9mqa asks
//! whether to build a real `meet()`/`join()` or to reword those criteria around the analytic
//! implementations that actually exist. Both transform paths are already implemented, so the
//! comparison needs no new abstraction — only measurement.
//!
//! It lives in fm-layout rather than fm-core because fm-core has no bench harness at all, and
//! standing one up would mean forking the ELF self-report helper that already lives here. fm-layout
//! is a legitimate consumer of `fm_core::cga`: the transform stack is what renderers and layout
//! compose against.
//!
//! WHAT THE ARMS DO. Both compose the SAME sequence of translate/rotate/scale steps and then map
//! the SAME points, so the only difference is the representation:
//!   - matrix: `AffineMatrix2D::compose`, then `apply` per point.
//!   - rotor:  `Rotor::compose` (which round-trips through a 16-component multivector geometric
//!     product), then `to_affine_matrix` once, then the same `apply` per point. `Rotor` has no
//!     point-application of its own, so converting is not a handicap imposed by this bench — it is
//!     the only way to use a rotor on a point.
//!
//! Read the result as instructions via callgrind, not as wall time: `perf_event_paranoid=4` makes
//! perf unusable on this host, and callgrind's Ir counts are deterministic and load-independent,
//! which matters on a shared box.

// Match the production allocator so the comparison is not distorted by libc's malloc, consistent
// with the other benches in this crate.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Shared with the other fm-layout benches; included by path because benches are separate binaries
// with no common crate module.
#[path = "bench_identity.rs"]
mod bench_identity;

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use fm_core::cga::{AffineMatrix2D, Rotor};
use std::hint::black_box;

/// The transform steps both arms compose, in order. Chosen to mix all three kinds rather than
/// repeat one, because a pure-translation chain would flatter the rotor (its translation rotor is
/// sparse) and a pure-rotation chain would flatter it further still.
fn steps(count: usize) -> Vec<(f64, f64, f64)> {
    (0..count)
        .map(|index| {
            let n = index as f64;
            (
                n.mul_add(0.37, 1.0),
                n.mul_add(0.11, 0.5),
                n.mul_add(0.02, 1.01),
            )
        })
        .collect()
}

fn compose_matrix(steps: &[(f64, f64, f64)]) -> AffineMatrix2D {
    let mut accumulated = AffineMatrix2D::identity();
    for &(dx, angle, scale) in steps {
        accumulated = accumulated.compose(AffineMatrix2D::translation(dx, dx));
        accumulated = accumulated.compose(AffineMatrix2D::rotation(angle));
        accumulated = accumulated.compose(AffineMatrix2D::scale(scale));
    }
    accumulated
}

fn compose_rotor(steps: &[(f64, f64, f64)]) -> Rotor {
    let mut accumulated = Rotor::identity();
    for &(dx, angle, scale) in steps {
        accumulated = accumulated.compose(Rotor::translation(dx, dx));
        accumulated = accumulated.compose(Rotor::rotation(angle));
        accumulated = accumulated.compose(Rotor::scale(scale));
    }
    accumulated
}

fn sample_points(count: usize) -> Vec<(f64, f64)> {
    (0..count)
        .map(|index| {
            let n = index as f64;
            (n.mul_add(1.7, -400.0), n.mul_add(-0.9, 250.0))
        })
        .collect()
}

fn bench_compose(c: &mut Criterion) {
    bench_identity::report_self_identity();
    let mut group = c.benchmark_group("transform_compose");

    for depth in [4_usize, 16, 64] {
        let steps = steps(depth);

        group.bench_with_input(BenchmarkId::new("matrix", depth), &steps, |b, steps| {
            b.iter(|| black_box(compose_matrix(black_box(steps))));
        });

        group.bench_with_input(BenchmarkId::new("rotor", depth), &steps, |b, steps| {
            b.iter(|| black_box(compose_rotor(black_box(steps))));
        });
    }

    group.finish();
}

fn bench_compose_and_apply(c: &mut Criterion) {
    bench_identity::report_self_identity();
    let mut group = c.benchmark_group("transform_compose_and_apply");

    let steps = steps(16);
    for point_count in [64_usize, 1_024] {
        let points = sample_points(point_count);

        group.bench_with_input(
            BenchmarkId::new("matrix", point_count),
            &points,
            |b, points| {
                b.iter(|| {
                    let transform = compose_matrix(black_box(&steps));
                    let mut sink = 0.0_f64;
                    for &(x, y) in points {
                        let (tx, ty) = transform.apply(x, y);
                        sink += tx + ty;
                    }
                    black_box(sink)
                });
            },
        );

        group.bench_with_input(
            BenchmarkId::new("rotor", point_count),
            &points,
            |b, points| {
                b.iter(|| {
                    // The rotor must become a matrix before it can touch a point at all.
                    let transform = compose_rotor(black_box(&steps)).to_affine_matrix();
                    let mut sink = 0.0_f64;
                    for &(x, y) in points {
                        let (tx, ty) = transform.apply(x, y);
                        sink += tx + ty;
                    }
                    black_box(sink)
                });
            },
        );
    }

    group.finish();
}

criterion_group!(benches, bench_compose, bench_compose_and_apply);
criterion_main!(benches);
