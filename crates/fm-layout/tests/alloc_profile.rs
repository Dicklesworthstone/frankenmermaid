//! Where do `layout_diagram`'s allocations actually go? (bd-oarm)
//!
//! bd-oarm proposes a layout-scoped bump arena for "the ~50% of layout allocs CSR cannot reach",
//! and states a size histogram from an earlier ad-hoc run. That histogram is the whole basis for
//! aiming the arena, and the bead itself records that CSR kept coming back ~0-gain because it was
//! aimed at the wrong bucket. So this re-derives the histogram in-tree, reproducibly, before any
//! refactor — rather than trusting a number nobody can re-run.
//!
//! COUNTS, NOT TIME. A counting `GlobalAlloc` is load-immune: it works on a busy host, which matters
//! because the head-to-head quiescence gate has been refusing all day. Time would not be measurable
//! here at all. It also means the arena's effect is gated on a COUNTED mechanism, which is what this
//! repo's ledger contract wants for a work-removal change.
//!
//! `System` rather than the shipped mimalloc: this counts allocation REQUESTS at the program
//! boundary, and that count is identical whichever allocator services them. Absolute wall time under
//! `System` would not be comparable to production, which is exactly why this file reports no time.
//!
//! A test target is its own crate root, so it may carry `unsafe` even though `fm-layout`'s library
//! is `#![forbid(unsafe_code)]`. Nothing here is linked into the library.
//!
//! Run: `cargo test -p fm-layout --test alloc_profile -- --ignored --nocapture`

use std::alloc::{GlobalAlloc, Layout, System};
use std::backtrace::Backtrace;
use std::cell::Cell;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use fm_core::{
    ArrowType, DiagramType, IrEdge, IrEndpoint, IrLabel, IrNode, IrNodeId, MermaidDiagramIr, Span,
};
use fm_layout::layout_diagram;

/// Size buckets chosen to match the ones bd-oarm names, so the numbers are directly comparable to
/// the claim being tested: 8-15 B (small per-node `Vec` buffers), 32-39 B and 64 B (the buckets the
/// bead argues are per-element structs rather than containers).
const BUCKETS: [(usize, usize); 6] = [(0, 7), (8, 15), (16, 31), (32, 39), (40, 63), (64, 64)];

static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static ALLOC_BYTES: AtomicUsize = AtomicUsize::new(0);
static BUCKET_HITS: [AtomicUsize; BUCKETS.len() + 1] = [
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
    AtomicUsize::new(0),
];
/// Counting is off until a measurement window opens, so the harness's own startup is not counted.
static COUNTING: AtomicUsize = AtomicUsize::new(0);

fn record(size: usize) {
    if COUNTING.load(Ordering::Relaxed) == 0 {
        return;
    }
    // The backtrace machinery allocates heavily, and those allocations reach this function too.
    // Counting them inflated `dense_dag_200` from 401 to 53,528 on the first capture run — the
    // instrument measuring itself. The capture guard has to suppress COUNTING as well as recursion,
    // or every histogram taken with capture enabled is nonsense.
    if IN_CAPTURE.with(Cell::get) {
        return;
    }
    ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
    ALLOC_BYTES.fetch_add(size, Ordering::Relaxed);
    let index = BUCKETS
        .iter()
        .position(|&(lo, hi)| size >= lo && size <= hi)
        .unwrap_or(BUCKETS.len());
    BUCKET_HITS[index].fetch_add(1, Ordering::Relaxed);
}

// ---------------------------------------------------------------- backtrace capture (bd-cs09)
//
// Capturing a backtrace INSIDE a GlobalAlloc allocates — the capture itself, the symbol strings, the
// Vec push, the Mutex — so without a guard this recurses until the stack dies. The guard is a
// thread-local `Cell` with a `const` initialiser specifically so the TLS slot itself never lazily
// allocates on first touch, which would reintroduce the recursion it exists to prevent.
//
// Off by default: `CAPTURE_SIZE` is 0 unless a profile opts in, so the ordinary counting path pays
// nothing but one relaxed load.

/// Only allocations of exactly this size are traced. 0 disables capture.
static CAPTURE_SIZE: AtomicUsize = AtomicUsize::new(0);
/// Stop after this many, because the interesting thing is which site repeats, not all 600 of them.
const CAPTURE_LIMIT: usize = 4;
static CAPTURED: Mutex<Vec<String>> = Mutex::new(Vec::new());

thread_local! {
    static IN_CAPTURE: Cell<bool> = const { Cell::new(false) };
}

fn maybe_capture(size: usize) {
    let want = CAPTURE_SIZE.load(Ordering::Relaxed);
    if want == 0 || size != want {
        return;
    }
    IN_CAPTURE.with(|guard| {
        if guard.get() {
            return;
        }
        guard.set(true);
        // Everything in here allocates; the guard makes those allocations invisible to this path.
        if let Ok(mut captured) = CAPTURED.lock()
            && captured.len() < CAPTURE_LIMIT
        {
            captured.push(format!("{}", Backtrace::force_capture()));
        }
        guard.set(false);
    });
}

struct CountingAllocator;

// SAFETY: every method forwards to `System` with the same layout it was handed, and the only added
// work is relaxed atomic counting, which cannot unwind or allocate.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        record(layout.size());
        maybe_capture(layout.size());
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if COUNTING.load(Ordering::Relaxed) != 0 {
            REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAllocator = CountingAllocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AllocProfile {
    allocs: usize,
    reallocs: usize,
    bytes: usize,
    buckets: [usize; BUCKETS.len() + 1],
}

/// Serialises measurement windows.
///
/// Every counter here is a process-global, and libtest runs tests on concurrent THREADS in one
/// process — so two profiling windows open at once corrupt each other. That is not hypothetical: the
/// first run with both instruments enabled reported `chain_100` at 377 allocations with 31 at 64 B,
/// against its true 130 and 1, because `layout_allocation_profile` and
/// `dense_graph_64_byte_allocation_sites` were interleaving. `#[ignore]` keeps these off the normal
/// suite; it does nothing about each other.
static PROFILE_LOCK: Mutex<()> = Mutex::new(());

/// Count every allocation `body` makes, with the whole window held under [`PROFILE_LOCK`].
fn profile<T>(body: impl FnOnce() -> T) -> (T, AllocProfile) {
    let _window = PROFILE_LOCK
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    ALLOC_CALLS.store(0, Ordering::Relaxed);
    REALLOC_CALLS.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
    for hits in &BUCKET_HITS {
        hits.store(0, Ordering::Relaxed);
    }
    COUNTING.store(1, Ordering::Relaxed);
    let value = body();
    COUNTING.store(0, Ordering::Relaxed);
    let mut buckets = [0_usize; BUCKETS.len() + 1];
    for (slot, hits) in buckets.iter_mut().zip(BUCKET_HITS.iter()) {
        *slot = hits.load(Ordering::Relaxed);
    }
    (
        value,
        AllocProfile {
            allocs: ALLOC_CALLS.load(Ordering::Relaxed),
            reallocs: REALLOC_CALLS.load(Ordering::Relaxed),
            bytes: ALLOC_BYTES.load(Ordering::Relaxed),
            buckets,
        },
    )
}

fn chain_ir(node_count: usize) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for index in 0..node_count {
        ir.nodes.push(IrNode {
            id: format!("N{index}"),
            ..IrNode::default()
        });
        ir.labels.push(IrLabel {
            text: format!("Node {index}"),
            span: Span::default(),
        });
    }
    for index in 0..node_count.saturating_sub(1) {
        ir.edges.push(IrEdge {
            from: IrEndpoint::Node(IrNodeId(index)),
            to: IrEndpoint::Node(IrNodeId(index + 1)),
            arrow: ArrowType::Arrow,
            ..IrEdge::default()
        });
    }
    ir
}

/// A layered DAG: `layers` ranks of `width` nodes, each fanning out to two below.
///
/// A chain is the WEAKEST possible allocation fixture — one node per rank, nothing for crossing
/// minimisation to do. bd-oarm's target workloads are dense (its worst ratio is an ER catalog), so a
/// chain alone cannot confirm or refute its histogram.
fn wide_ir(layers: usize, width: usize) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for layer in 0..layers {
        for column in 0..width {
            ir.nodes.push(IrNode {
                id: format!("N{layer}_{column}"),
                ..IrNode::default()
            });
            ir.labels.push(IrLabel {
                text: format!("L{layer} W{column}"),
                span: Span::default(),
            });
        }
    }
    let index_of = |layer: usize, column: usize| layer * width + column;
    for layer in 0..layers.saturating_sub(1) {
        for column in 0..width {
            for target in [column, (column + 1) % width] {
                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(index_of(layer, column))),
                    to: IrEndpoint::Node(IrNodeId(index_of(layer + 1, target))),
                    arrow: ArrowType::Arrow,
                    ..IrEdge::default()
                });
            }
        }
    }
    ir
}

/// Every node points at the next `fanout` nodes: no cycles, high edge density, so edge routing has
/// many obstacles to work around. This is the shape closest to the workload bd-oarm names.
fn dense_dag_ir(node_count: usize, fanout: usize) -> MermaidDiagramIr {
    let mut ir = MermaidDiagramIr::empty(DiagramType::Flowchart);
    for index in 0..node_count {
        ir.nodes.push(IrNode {
            id: format!("D{index}"),
            ..IrNode::default()
        });
        ir.labels.push(IrLabel {
            text: format!("D{index}"),
            span: Span::default(),
        });
    }
    for index in 0..node_count {
        for step in 1..=fanout {
            if index + step < node_count {
                ir.edges.push(IrEdge {
                    from: IrEndpoint::Node(IrNodeId(index)),
                    to: IrEndpoint::Node(IrNodeId(index + step)),
                    arrow: ArrowType::Arrow,
                    ..IrEdge::default()
                });
            }
        }
    }
    ir
}

fn report(name: &str, nodes: usize, profile: &AllocProfile) {
    let per_element = if nodes > 0 {
        profile.allocs as f64 / nodes as f64
    } else {
        0.0
    };
    println!("--- {name} (nodes={nodes}) ---");
    println!(
        "  allocs={} reallocs={} bytes={} per_node={per_element:.2}",
        profile.allocs, profile.reallocs, profile.bytes
    );
    for (index, &(lo, hi)) in BUCKETS.iter().enumerate() {
        let share = if profile.allocs > 0 {
            100.0 * profile.buckets[index] as f64 / profile.allocs as f64
        } else {
            0.0
        };
        println!(
            "  {lo:>3}..={hi:<3} B : {:>7}  ({share:.1}%)",
            profile.buckets[index]
        );
    }
    println!(
        "  >{:>2} B      : {:>7}",
        BUCKETS[BUCKETS.len() - 1].1,
        profile.buckets[BUCKETS.len()]
    );
}

/// The instrument has to be shown to work before its numbers mean anything.
#[test]
fn counting_allocator_observes_allocations_and_stays_off_outside_the_window() {
    let (sum, profile) = profile(|| {
        let data: Vec<u64> = (0..1_000).collect();
        data.iter().sum::<u64>()
    });
    assert_eq!(sum, 499_500);
    assert!(
        profile.allocs >= 1,
        "a Vec of 1000 u64 must allocate at least once; the counter saw {}",
        profile.allocs
    );
    assert!(profile.bytes >= 8_000, "bytes={}", profile.bytes);

    // Outside a window the counter must not move, or every profile is polluted by whatever the
    // harness did before it.
    let before = ALLOC_CALLS.load(Ordering::Relaxed);
    let noise: Vec<u64> = (0..10_000).collect();
    std::hint::black_box(&noise);
    assert_eq!(
        ALLOC_CALLS.load(Ordering::Relaxed),
        before,
        "the counter kept counting after the window closed"
    );
}

/// Which call site produces the 64 B allocations that are ~40% of dense-graph layout allocs
/// (bd-cs09)?
///
/// The histogram says they are 0.75 per node on `dense_dag` and absent on chain/wide, so something
/// allocates per node only once fanout is high — the shape of a spilled inline buffer, a `Vec` grown
/// to capacity 8, or 8 `usize`s. Reading ruled out the obstacle index, which is already CSR. This
/// prints the stacks.
#[test]
#[ignore = "allocation site instrument; run with --ignored --nocapture"]
fn dense_graph_64_byte_allocation_sites() {
    let ir = dense_dag_ir(200, 4);
    std::hint::black_box(layout_diagram(&ir));

    CAPTURED.lock().expect("capture mutex").clear();
    CAPTURE_SIZE.store(64, Ordering::Relaxed);
    let (layout, profile) = profile(|| layout_diagram(&ir));
    CAPTURE_SIZE.store(0, Ordering::Relaxed);
    std::hint::black_box(&layout);

    let sixty_four = profile.buckets[BUCKETS.len() - 1];
    // TOTALS UNDER CAPTURE ARE NOT A HISTOGRAM. Even with the reentrancy guard suppressing both
    // counting and recursion, symbolising a backtrace leaves residue: this fixture measures 401
    // allocations with capture off and ~4,100 with it on. (Before the guard covered `record` as
    // well, it was 53,528 — the instrument measuring itself.) Use `layout_allocation_profile` for
    // counts and this test only for SITES.
    println!(
        "dense_dag_200 [capture on; total {} is polluted, use layout_allocation_profile for counts]",
        profile.allocs
    );
    println!("  64 B allocations traced: {sixty_four}");
    // The 64 B count IS trustworthy: those allocations are recorded before capture is entered, and
    // the machinery's own allocations are guarded out. It must match the capture-off figure of 150.
    assert!(
        (140..=170).contains(&sixty_four),
        "expected ~150 64 B allocations as measured with capture off, got {sixty_four}; either the \
         fixture changed or the guard stopped suppressing the backtrace machinery"
    );
    let captured = CAPTURED.lock().expect("capture mutex");
    println!("captured {} stack(s):", captured.len());
    for (index, trace) in captured.iter().enumerate() {
        // Workspace frames are the signal; std/backtrace frames are noise. But which allocations
        // land in the first CAPTURE_LIMIT slots shifts between runs, so a run can capture stacks
        // with no workspace frame at all — and a filter that then prints nothing looks like a broken
        // instrument. Fall back to raw frames rather than silence.
        let workspace: Vec<&str> = trace
            .lines()
            .filter(|line| line.contains("fm_layout") || line.contains("fm_core"))
            .take(12)
            .collect();
        println!("--- stack {index} ---");
        if workspace.is_empty() {
            println!("  (no workspace frame; raw head)");
            for line in trace.lines().take(10) {
                println!("  {}", line.trim());
            }
        } else {
            for line in workspace {
                println!("  {}", line.trim());
            }
        }
    }
}

/// The layout allocation histogram bd-oarm is aimed at. `--ignored`, because it is an instrument
/// rather than an assertion: it prints numbers for a human to aim a lever with.
#[test]
#[ignore = "allocation profile instrument; run with --ignored --nocapture"]
fn layout_allocation_profile() {
    let fixtures: Vec<(String, MermaidDiagramIr)> = vec![
        ("chain_100".to_owned(), chain_ir(100)),
        ("chain_1000".to_owned(), chain_ir(1_000)),
        ("wide_8x16".to_owned(), wide_ir(8, 16)),
        ("wide_16x32".to_owned(), wide_ir(16, 32)),
        ("dense_dag_200".to_owned(), dense_dag_ir(200, 4)),
        ("dense_dag_800".to_owned(), dense_dag_ir(800, 4)),
    ];
    for (name, ir) in &fixtures {
        // Warm any lazily-initialised global state so it is not attributed to the measured run.
        std::hint::black_box(layout_diagram(ir));
        let (layout, profile) = profile(|| layout_diagram(ir));
        std::hint::black_box(&layout);
        println!("  edges={}", ir.edges.len());
        report(name, ir.nodes.len(), &profile);
    }
}
