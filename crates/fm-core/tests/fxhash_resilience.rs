//! FxHash Collision and DoS Resistance Tests (bd-1s1g.7)
//!
//! Validates that FxHash-based NodeMap/EdgeMap are resilient to crafted
//! hash collisions and degenerate performance patterns.
//!
//! Threat model: NodeId/EdgeId values are generated internally by the parser,
//! not from user input. HashDoS is NOT a realistic threat for this use case.

use fm_core::{EdgeMap, IrNodeId, NodeMap, NodeSet};
use rustc_hash::FxHasher;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::time::Instant;

// ============================================================================
// FxHash Analysis Utilities
// ============================================================================

/// Compute FxHash of a usize value (matching what FxHashMap uses internally).
fn fx_hash_usize(value: usize) -> u64 {
    let mut hasher = FxHasher::default();
    value.hash(&mut hasher);
    hasher.finish()
}

/// Compute SipHash (default Rust hasher) for comparison.
fn sip_hash_usize(value: usize) -> u64 {
    let mut hasher = DefaultHasher::new();
    value.hash(&mut hasher);
    hasher.finish()
}

// ============================================================================
// Test 1: Sequential ID Distribution
// ============================================================================

/// Verify that sequential IDs don't create clustering under FxHash.
///
/// FxHash uses multiply-shift hashing which should distribute sequential
/// integers uniformly across buckets.
#[test]
fn sequential_ids_distribute_uniformly() {
    const N: usize = 10_000;
    const BUCKETS: usize = 1024;

    let mut bucket_counts = vec![0usize; BUCKETS];

    for i in 0..N {
        let hash = fx_hash_usize(i);
        let bucket = (hash as usize) % BUCKETS;
        bucket_counts[bucket] += 1;
    }

    // Expected: ~N/BUCKETS per bucket = ~9.77
    let expected = N as f64 / BUCKETS as f64;
    let variance: f64 = bucket_counts
        .iter()
        .map(|&c| (c as f64 - expected).powi(2))
        .sum::<f64>()
        / BUCKETS as f64;
    let stddev = variance.sqrt();

    // Chi-squared test threshold: stddev should be reasonable
    // For uniform distribution, stddev ≈ sqrt(expected * (1 - 1/BUCKETS)) ≈ 3.1
    // Allow up to 2x theoretical stddev for safety margin
    assert!(
        stddev < expected,
        "Sequential IDs show excessive clustering: stddev={stddev:.2}, expected avg={expected:.2}"
    );

    // No bucket should have > 3x expected count (pathological clustering)
    let max_bucket = *bucket_counts.iter().max().unwrap();
    assert!(
        (max_bucket as f64) < expected * 3.0,
        "Bucket with {max_bucket} items exceeds 3x expected ({expected:.0})"
    );
}

// ============================================================================
// Test 2: NodeMap Performance Under Load
// ============================================================================

/// Verify that NodeMap operations remain fast under high load.
#[test]
fn nodemap_performance_under_load() {
    const N: usize = 100_000;

    let start = Instant::now();

    // Insert N items
    let mut map: NodeMap<usize> = NodeMap::default();
    for i in 0..N {
        map.insert(IrNodeId(i), i * 2);
    }

    let insert_time = start.elapsed();

    // Lookup all items
    let lookup_start = Instant::now();
    for i in 0..N {
        assert_eq!(map.get(&IrNodeId(i)), Some(&(i * 2)));
    }
    let lookup_time = lookup_start.elapsed();

    // NO wall-clock deadline (bd-wadw). The substance of this test is the 10,000 lookups asserted
    // above: the map is CORRECT at a size where hashing behaviour matters. An `insert_time < 1s`
    // bound asserts nothing about that — only about how busy the host is — and under
    // `cargo test --workspace`, with dozens of test binaries on every core, it reds the suite.
    //
    // Unlike bd-u6z4's sibling fix there is no relative property to substitute: that test had two
    // arms measured back-to-back, so contention slowed both together and the RATIO survived. This
    // one has a single arm, and inventing a second would be writing a different test. A genuine
    // NodeMap performance guard belongs in a bench under this repo's A/A-null discipline, where a
    // loaded host is controlled for rather than assumed away.
    println!("NodeMap {N} inserts: {insert_time:?}, {N} lookups: {lookup_time:?}");
}

// ============================================================================
// Test 3: Mixed Insert/Delete (Tombstone Resilience)
// ============================================================================

/// Test that alternating insert/delete doesn't cause performance degradation.
///
/// Swiss Tables handle tombstones by triggering rehash when tombstone density
/// gets too high.
#[test]
fn tombstone_resilience() {
    const ROUNDS: usize = 10;
    const BATCH_SIZE: usize = 1000;
    const HALF: usize = ROUNDS / 2;

    let mut map: NodeMap<usize> = NodeMap::default();
    let mut times = Vec::with_capacity(ROUNDS);

    for round in 0..ROUNDS {
        let start = Instant::now();

        // Insert batch
        let base = round * BATCH_SIZE;
        for i in 0..BATCH_SIZE {
            map.insert(IrNodeId(base + i), i);
        }

        // Delete half of previous batch (if exists)
        if round > 0 {
            let prev_base = (round - 1) * BATCH_SIZE;
            for i in 0..(BATCH_SIZE / 2) {
                map.remove(&IrNodeId(prev_base + i));
            }
        }

        times.push(start.elapsed());
    }

    // The load-independent half of this test, and the only part that speaks to CORRECTNESS
    // (bd-aou7). Tombstone handling that loses a live key or resurrects a removed one fails here
    // no matter how busy the host is, which no timing bound can claim. Each round r < ROUNDS-1 had
    // its first half removed by round r+1; the final round keeps its whole batch.
    for round in 0..ROUNDS {
        let base = round * BATCH_SIZE;
        let removed = if round + 1 < ROUNDS {
            BATCH_SIZE / 2
        } else {
            0
        };
        for i in 0..removed {
            assert!(
                !map.contains_key(&IrNodeId(base + i)),
                "round {round} key {i} was removed and must not be resurrected by a rehash"
            );
        }
        for i in removed..BATCH_SIZE {
            assert_eq!(
                map.get(&IrNodeId(base + i)),
                Some(&i),
                "round {round} key {i} is live and must survive tombstone compaction"
            );
        }
    }
    assert_eq!(
        map.len(),
        ROUNDS * BATCH_SIZE - (ROUNDS - 1) * (BATCH_SIZE / 2),
        "surviving key count must match the insert/remove schedule exactly"
    );

    // Compare the MINIMUM of the early rounds against the MINIMUM of the late rounds, rather than
    // every round against times[0] (bd-aou7).
    //
    // Tombstone accumulation is a SYSTEMATIC cost: it inflates every late round. A scheduling stall
    // is a ONE-ROUND event. Contention can only ADD time to a measurement and never remove it, so a
    // minimum over a group converges on the uncontended cost as long as ONE round in that group
    // runs clean. That separates the two.
    //
    // The old form divided by times[0], a single sample chosen for its position, and that failed in
    // BOTH directions. Measured round times on an idle worker were
    // [490, 670, 410, 419, 405, 1052, 411, 386, 400, 406]us: round 0 is not the fastest round (it
    // carries the map's initial growth, 490 against a 386 minimum), and round 5 already stalls to
    // 2.7x the minimum with no load at all. Headroom to the 20x bound was ~9ms of stall on a single
    // round, which is ordinary under `cargo test --workspace`. Worse, a stall in round 0 INFLATES
    // the denominator, so every later ratio is divided by an inflated baseline and the assertion
    // passes regardless of what the map does — it could stop checking anything without saying so.
    // That is the same class as bd-u6z4 and bd-wadw, both closed in this file.
    //
    // This deliberately no longer fails on one anomalously slow round. A single spike is not
    // tombstone accumulation, it is the false positive; the correctness assertions above are what
    // now cover a genuinely broken round.
    let early = times[..HALF].iter().min().copied().expect("HALF > 0");
    let late = times[HALF..].iter().min().copied().expect("HALF < ROUNDS");
    println!(
        "tombstone rounds (us): {:?}, early min {}us, late min {}us",
        times.iter().map(|t| t.as_micros()).collect::<Vec<_>>(),
        early.as_micros(),
        late.as_micros()
    );
    let baseline = early.as_micros().max(1);
    assert!(
        late.as_micros() < baseline * 20,
        "late rounds degraded systematically: early min {early:?} vs late min {late:?} \
         (tombstone accumulation?)"
    );
}

// ============================================================================
// Test 4: FxHash vs SipHash Comparison
// ============================================================================

/// Document the performance difference between FxHash and SipHash.
///
/// This test demonstrates why we use FxHash for internal IDs:
/// the security overhead of SipHash is unnecessary when IDs aren't
/// user-controlled.
#[test]
fn fxhash_faster_than_siphash() {
    const N: usize = 1_000_000;

    // Time FxHash
    let fx_start = Instant::now();
    let mut fx_sum: u64 = 0;
    for i in 0..N {
        fx_sum = fx_sum.wrapping_add(fx_hash_usize(i));
    }
    let fx_time = fx_start.elapsed();

    // Time SipHash
    let sip_start = Instant::now();
    let mut sip_sum: u64 = 0;
    for i in 0..N {
        sip_sum = sip_sum.wrapping_add(sip_hash_usize(i));
    }
    let sip_time = sip_start.elapsed();

    // Prevent optimization
    assert_ne!(fx_sum, 0);
    assert_ne!(sip_sum, 0);

    let fx_ns = fx_time.as_nanos();
    let sip_ns = sip_time.as_nanos();

    // Assert the RELATIVE property this test is named for, and nothing absolute (bd-u6z4).
    //
    // This used to assert `fx_time < 1s` and `sip_time < 1s` — an absolute wall-clock deadline,
    // which is a property of how busy the host is, not of the code. Under `cargo test --workspace`
    // the machine runs dozens of test binaries across every core, and two million hash operations
    // exceeding a second there is ordinary; it red the suite four times in one session while
    // passing every isolated re-run. Raising the budget would have kept a host-speed assertion and
    // merely made it fire less often.
    //
    // A ratio between two arms measured back-to-back in the same process is the shape that
    // SURVIVES contention, because scheduling pressure slows both arms together. It is also the
    // property the test's name claims and its old comment promised ("we only assert it's not
    // slower") but which was never actually asserted — `fx_ns` and `sip_ns` were computed and then
    // used only in the `println!` below.
    //
    // The bound is deliberately weak in one direction only: FxHash must not be SLOWER than
    // SipHash. The real ratio is 5-10x, so a genuine regression that inverted them still fails,
    // while ordinary scheduling jitter between two adjacent loops cannot.
    assert!(
        fx_ns <= sip_ns,
        "FxHash must not be slower than SipHash over the same workload: FxHash {fx_time:?} vs \
         SipHash {sip_time:?}"
    );

    // Log the ratio for documentation purposes
    println!(
        "FxHash: {:?}, SipHash: {:?}, ratio: {:.2}x",
        fx_time,
        sip_time,
        sip_ns as f64 / fx_ns.max(1) as f64
    );
}

// ============================================================================
// Test 5: Graceful Degradation Under Crafted Collisions
// ============================================================================

/// Test that even with worst-case hash collisions, operations complete.
///
/// We craft a set of values that hash to the same bucket mod a small table size.
/// This simulates the worst case for a hash table.
#[test]
fn graceful_degradation_under_collisions() {
    // Find 100 values that all hash to the same bucket (mod 128)
    const TARGET_BUCKET: usize = 42;
    const BUCKET_COUNT: usize = 128;
    const COLLISION_COUNT: usize = 100;

    let mut colliding_values = Vec::with_capacity(COLLISION_COUNT);
    let mut i = 0usize;
    while colliding_values.len() < COLLISION_COUNT {
        let hash = fx_hash_usize(i);
        if (hash as usize) % BUCKET_COUNT == TARGET_BUCKET {
            colliding_values.push(i);
        }
        i += 1;
    }

    // Insert all colliding values into a NodeMap
    let start = Instant::now();
    let mut map: NodeMap<usize> = NodeMap::default();
    for (idx, &val) in colliding_values.iter().enumerate() {
        map.insert(IrNodeId(val), idx);
    }
    let insert_time = start.elapsed();

    // Lookup all values
    let lookup_start = Instant::now();
    for (idx, &val) in colliding_values.iter().enumerate() {
        assert_eq!(map.get(&IrNodeId(val)), Some(&idx));
    }
    let lookup_time = lookup_start.elapsed();

    // NO wall-clock deadline (bd-wadw) — and this was the most fragile of the family, a 100ms
    // budget. What this test proves is in the loop above: every colliding key still resolves to its
    // own value, so degradation under collisions is graceful in the sense that matters. Whether a
    // contended machine takes 3ms or 300ms to do it says nothing about the map.
    println!("colliding inserts: {insert_time:?}, colliding lookups: {lookup_time:?}");
}

// ============================================================================
// Test 6: NodeSet Operations
// ============================================================================

/// Verify NodeSet (FxHashSet<IrNodeId>) works correctly.
#[test]
fn nodeset_basic_operations() {
    let mut set: NodeSet = NodeSet::default();

    // Insert
    for i in 0..1000 {
        assert!(set.insert(IrNodeId(i)));
    }
    assert_eq!(set.len(), 1000);

    // Duplicate insert returns false
    assert!(!set.insert(IrNodeId(500)));
    assert_eq!(set.len(), 1000);

    // Contains
    for i in 0..1000 {
        assert!(set.contains(&IrNodeId(i)));
    }
    assert!(!set.contains(&IrNodeId(1000)));

    // Remove
    assert!(set.remove(&IrNodeId(500)));
    assert!(!set.contains(&IrNodeId(500)));
    assert_eq!(set.len(), 999);
}

// ============================================================================
// Test 7: EdgeMap Operations
// ============================================================================

/// Verify EdgeMap works correctly with edge indices.
#[test]
fn edgemap_basic_operations() {
    let mut map: EdgeMap<String> = EdgeMap::default();

    // Insert
    for i in 0..100 {
        map.insert(i, format!("edge_{i}"));
    }
    assert_eq!(map.len(), 100);

    // Lookup
    assert_eq!(map.get(&50), Some(&"edge_50".to_string()));
    assert_eq!(map.get(&100), None);

    // Update
    map.insert(50, "updated".to_string());
    assert_eq!(map.get(&50), Some(&"updated".to_string()));

    // Remove
    assert_eq!(map.remove(&50), Some("updated".to_string()));
    assert_eq!(map.get(&50), None);
}

// ============================================================================
// Documentation: Threat Model
// ============================================================================

/// Document the threat model for FxHash usage.
///
/// This test exists to ensure the security rationale is captured in code.
#[test]
fn threat_model_documented() {
    // NodeId and EdgeId are assigned internally by the parser:
    // - NodeId: sequential integer assigned during IR construction
    // - EdgeId: index into edges vector
    //
    // Neither is controlled by user input. The parser assigns IDs based on
    // parse order, not based on content that could be crafted by an attacker.
    //
    // Therefore:
    // - HashDoS attacks are NOT possible (attacker cannot choose IDs)
    // - FxHash's non-cryptographic nature is acceptable
    // - The 5-10x performance gain over SipHash is a net win
    //
    // If this changes (e.g., user-provided node IDs), we must revisit.

    // This assertion documents the invariant
    let node_id = IrNodeId(42);
    assert_eq!(
        node_id.0, 42,
        "NodeId is a simple integer wrapper, not user-controlled"
    );
}
