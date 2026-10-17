//! Adapton-style self-adjusting computation framework.
//!
//! This module provides demand-driven incremental computation for the layout pipeline.
//! When inputs change, only the affected computations are re-evaluated.
//!
//! # Core Concepts
//!
//! - **`LayoutDcg`**: A typed Demanded Computation Graph for layout phases
//! - **Dirty tracking**: Mark computations as dirty when inputs change
//! - **Cache hits**: Return cached results when inputs haven't changed
//!
//! # Example
//!
//! ```ignore
//! let dcg = LayoutDcg::new();
//!
//! // Set input fingerprint
//! dcg.set_ir_fingerprint(hash_of_ir);
//!
//! // Check if metrics need recompute
//! if dcg.metrics_dirty() {
//!     let metrics = compute_graph_metrics(&ir);
//!     dcg.set_metrics(metrics);
//! }
//!
//! // Get cached metrics (returns None if dirty)
//! if let Some(metrics) = dcg.get_metrics() {
//!     // Use cached value
//! }
//! ```
//!
//! # Design
//!
//! This is a simplified Adapton implementation optimized for layout workloads:
//! - Single-threaded (layout is inherently sequential)
//! - Coarse-grained invalidation (whole-phase granularity)
//! - Known type set (no dynamic type erasure)
//!
//! Future work can add finer-grained tracking (per-node, per-rank).

use std::cell::RefCell;

/// A typed DCG for layout phases.
///
/// Uses explicit dirty flags and caching for each layout phase:
/// 1. Graph metrics (node/edge counts, topology properties)
/// 2. Rank assignments (node positions in layered layout)
/// 3. Node orderings (within-rank ordering for crossing minimization)
/// 4. Final layout (complete positioned diagram)
///
/// The dependency structure is:
/// ```text
/// IR fingerprint ─┬─> graph_metrics ─┬─> ranks ─> orderings ─> layout
///                 │                   │
/// Config fingerprint ─────────────────┴─────────────────────────────┘
/// ```
#[derive(Debug, Clone)]
pub struct LayoutDcg {
    /// Input: the diagram IR fingerprint.
    ir_fingerprint: RefCell<u64>,
    /// Input: the layout configuration fingerprint.
    config_fingerprint: RefCell<u64>,
    /// Cached: graph metrics.
    graph_metrics: RefCell<Option<crate::GraphMetrics>>,
    /// Cached: rank assignments (node index -> rank).
    ranks: RefCell<Option<Vec<usize>>>,
    /// Cached: node orderings per rank.
    orderings: RefCell<Option<Vec<Vec<usize>>>>,
    /// Cached: final layout.
    layout: RefCell<Option<crate::DiagramLayout>>,
    /// Dirty flags for each cached value.
    dirty: RefCell<DirtyFlags>,
    /// Statistics for profiling.
    stats: RefCell<LayoutDcgStats>,
}

impl PartialEq for LayoutDcg {
    fn eq(&self, other: &Self) -> bool {
        *self.ir_fingerprint.borrow() == *other.ir_fingerprint.borrow()
            && *self.config_fingerprint.borrow() == *other.config_fingerprint.borrow()
            && *self.dirty.borrow() == *other.dirty.borrow()
    }
}

/// Dirty flags for each layout phase.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct DirtyFlags {
    /// Graph metrics need recompute.
    graph_metrics: bool,
    /// Rank assignments need recompute.
    ranks: bool,
    /// Node orderings need recompute.
    orderings: bool,
    /// Final layout needs recompute.
    layout: bool,
}

/// Statistics for the layout DCG.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LayoutDcgStats {
    /// Number of input changes (IR or config).
    pub input_changes: usize,
    /// Number of graph metrics cache hits.
    pub metrics_hits: usize,
    /// Number of graph metrics recomputes.
    pub metrics_recomputes: usize,
    /// Number of rank cache hits.
    pub rank_hits: usize,
    /// Number of rank recomputes.
    pub rank_recomputes: usize,
    /// Number of ordering cache hits.
    pub ordering_hits: usize,
    /// Number of ordering recomputes.
    pub ordering_recomputes: usize,
    /// Number of layout cache hits.
    pub layout_hits: usize,
    /// Number of layout recomputes.
    pub layout_recomputes: usize,
}

impl Default for LayoutDcg {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutDcg {
    /// Create a new layout DCG.
    #[must_use]
    pub fn new() -> Self {
        Self {
            ir_fingerprint: RefCell::new(0),
            config_fingerprint: RefCell::new(0),
            graph_metrics: RefCell::new(None),
            ranks: RefCell::new(None),
            orderings: RefCell::new(None),
            layout: RefCell::new(None),
            dirty: RefCell::new(DirtyFlags {
                graph_metrics: true,
                ranks: true,
                orderings: true,
                layout: true,
            }),
            stats: RefCell::new(LayoutDcgStats::default()),
        }
    }

    /// Set the IR fingerprint, invalidating downstream caches.
    ///
    /// This is the primary input that triggers recomputation.
    /// Any change to the IR invalidates all cached phases.
    pub fn set_ir_fingerprint(&self, fingerprint: u64) {
        let old = *self.ir_fingerprint.borrow();
        if old == fingerprint {
            return;
        }

        *self.ir_fingerprint.borrow_mut() = fingerprint;
        self.stats.borrow_mut().input_changes += 1;

        // Invalidate all downstream caches
        let mut dirty = self.dirty.borrow_mut();
        dirty.graph_metrics = true;
        dirty.ranks = true;
        dirty.orderings = true;
        dirty.layout = true;
    }

    /// Get the current IR fingerprint.
    #[must_use]
    pub fn ir_fingerprint(&self) -> u64 {
        *self.ir_fingerprint.borrow()
    }

    /// Set the config fingerprint, invalidating downstream caches.
    ///
    /// Config affects orderings and layout but not graph metrics or ranks
    /// (topology-derived values don't depend on rendering config).
    pub fn set_config_fingerprint(&self, fingerprint: u64) {
        let old = *self.config_fingerprint.borrow();
        if old == fingerprint {
            return;
        }

        *self.config_fingerprint.borrow_mut() = fingerprint;
        self.stats.borrow_mut().input_changes += 1;

        // Config affects orderings and layout but not graph metrics or ranks
        let mut dirty = self.dirty.borrow_mut();
        dirty.orderings = true;
        dirty.layout = true;
    }

    /// Get the current config fingerprint.
    #[must_use]
    pub fn config_fingerprint(&self) -> u64 {
        *self.config_fingerprint.borrow()
    }

    // --- Graph Metrics ---

    /// Check if graph metrics are dirty (need recompute).
    #[must_use]
    pub fn metrics_dirty(&self) -> bool {
        self.dirty.borrow().graph_metrics
    }

    /// Record that graph metrics were computed.
    pub fn set_metrics(&self, metrics: crate::GraphMetrics) {
        *self.graph_metrics.borrow_mut() = Some(metrics);
        self.dirty.borrow_mut().graph_metrics = false;
        self.stats.borrow_mut().metrics_recomputes += 1;
    }

    /// Get cached graph metrics if clean, otherwise `None`.
    #[must_use]
    pub fn get_metrics(&self) -> Option<crate::GraphMetrics> {
        if self.metrics_dirty() {
            return None;
        }
        self.stats.borrow_mut().metrics_hits += 1;
        *self.graph_metrics.borrow()
    }

    // --- Rank Assignments ---

    /// Check if ranks are dirty (need recompute).
    #[must_use]
    pub fn ranks_dirty(&self) -> bool {
        self.dirty.borrow().ranks
    }

    /// Record that ranks were computed.
    pub fn set_ranks(&self, ranks: Vec<usize>) {
        *self.ranks.borrow_mut() = Some(ranks);
        self.dirty.borrow_mut().ranks = false;
        self.stats.borrow_mut().rank_recomputes += 1;
    }

    /// Get cached ranks if clean, otherwise `None`.
    #[must_use]
    pub fn get_ranks(&self) -> Option<Vec<usize>> {
        if self.ranks_dirty() {
            return None;
        }
        self.stats.borrow_mut().rank_hits += 1;
        self.ranks.borrow().clone()
    }

    // --- Node Orderings ---

    /// Check if orderings are dirty (need recompute).
    #[must_use]
    pub fn orderings_dirty(&self) -> bool {
        self.dirty.borrow().orderings
    }

    /// Record that orderings were computed.
    pub fn set_orderings(&self, orderings: Vec<Vec<usize>>) {
        *self.orderings.borrow_mut() = Some(orderings);
        self.dirty.borrow_mut().orderings = false;
        self.stats.borrow_mut().ordering_recomputes += 1;
    }

    /// Get cached orderings if clean, otherwise `None`.
    #[must_use]
    pub fn get_orderings(&self) -> Option<Vec<Vec<usize>>> {
        if self.orderings_dirty() {
            return None;
        }
        self.stats.borrow_mut().ordering_hits += 1;
        self.orderings.borrow().clone()
    }

    // --- Final Layout ---

    /// Check if layout is dirty (need recompute).
    #[must_use]
    pub fn layout_dirty(&self) -> bool {
        self.dirty.borrow().layout
    }

    /// Record that layout was computed.
    pub fn set_layout(&self, layout: crate::DiagramLayout) {
        *self.layout.borrow_mut() = Some(layout);
        self.dirty.borrow_mut().layout = false;
        self.stats.borrow_mut().layout_recomputes += 1;
    }

    /// Get cached layout if clean, otherwise `None`.
    #[must_use]
    pub fn get_layout(&self) -> Option<crate::DiagramLayout> {
        if self.layout_dirty() {
            return None;
        }
        self.stats.borrow_mut().layout_hits += 1;
        self.layout.borrow().clone()
    }

    /// Record a layout cache hit without cloning a stored `DiagramLayout`.
    pub fn record_layout_hit(&self) {
        self.dirty.borrow_mut().layout = false;
        self.stats.borrow_mut().layout_hits += 1;
    }

    /// Record an input change and layout recompute without cloning a `DiagramLayout`.
    pub fn record_layout_recompute(&self) {
        self.dirty.borrow_mut().layout = false;
        let mut stats = self.stats.borrow_mut();
        stats.input_changes += 1;
        stats.layout_recomputes += 1;
    }

    // --- Utilities ---

    /// Get current statistics.
    #[must_use]
    pub fn stats(&self) -> LayoutDcgStats {
        self.stats.borrow().clone()
    }

    /// Reset statistics.
    pub fn reset_stats(&self) {
        *self.stats.borrow_mut() = LayoutDcgStats::default();
    }

    /// Clear all caches and mark everything dirty.
    pub fn invalidate_all(&self) {
        *self.graph_metrics.borrow_mut() = None;
        *self.ranks.borrow_mut() = None;
        *self.orderings.borrow_mut() = None;
        *self.layout.borrow_mut() = None;
        *self.dirty.borrow_mut() = DirtyFlags {
            graph_metrics: true,
            ranks: true,
            orderings: true,
            layout: true,
        };
    }

    /// Check if any cache is dirty.
    #[must_use]
    pub fn any_dirty(&self) -> bool {
        let dirty = self.dirty.borrow();
        dirty.graph_metrics || dirty.ranks || dirty.orderings || dirty.layout
    }

    /// Check if all caches are clean (fully computed).
    #[must_use]
    pub fn fully_cached(&self) -> bool {
        !self.any_dirty()
    }

    /// Get a summary of dirty state for diagnostics.
    #[must_use]
    pub fn dirty_summary(&self) -> String {
        let dirty = self.dirty.borrow();
        let mut parts = Vec::new();
        if dirty.graph_metrics {
            parts.push("metrics");
        }
        if dirty.ranks {
            parts.push("ranks");
        }
        if dirty.orderings {
            parts.push("orderings");
        }
        if dirty.layout {
            parts.push("layout");
        }
        if parts.is_empty() {
            "clean".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Compute a fingerprint for an IR to use with `LayoutDcg::set_ir_fingerprint`.
///
/// The fingerprint captures the topology (nodes and edges) but not
/// styling or labels that don't affect layout.
#[must_use]
pub fn ir_fingerprint(ir: &fm_core::MermaidDiagramIr) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;

    // Hash node count
    hash_u64(&mut hash, ir.nodes.len() as u64);

    // Hash node IDs for stable ordering
    for node in &ir.nodes {
        hash_str(&mut hash, &node.id);
    }

    // Hash edge count and endpoints
    hash_u64(&mut hash, ir.edges.len() as u64);
    for edge in &ir.edges {
        // Hash endpoint discriminants (Unresolved=0, Node=1, Port=2)
        hash_u64(&mut hash, endpoint_discriminant(&edge.from) as u64);
        hash_u64(&mut hash, endpoint_discriminant(&edge.to) as u64);
    }

    // Hash diagram direction (affects layout)
    hash_u64(&mut hash, ir.direction as u64);

    // Hash cluster membership (affects layout regions)
    for cluster in &ir.clusters {
        hash_u64(&mut hash, cluster.members.len() as u64);
    }

    hash
}

fn endpoint_discriminant(endpoint: &fm_core::IrEndpoint) -> u8 {
    match endpoint {
        fm_core::IrEndpoint::Unresolved => 0,
        fm_core::IrEndpoint::Node(_) => 1,
        fm_core::IrEndpoint::Port(_) => 2,
    }
}

fn hash_str(state: &mut u64, s: &str) {
    for byte in s.bytes() {
        *state = state
            .wrapping_mul(0x517c_c1b7_2722_0a95)
            .wrapping_add(u64::from(byte));
    }
}

/// Compute a fingerprint for a config to use with `LayoutDcg::set_config_fingerprint`.
#[must_use]
pub fn config_fingerprint(config: &fm_core::MermaidConfig) -> u64 {
    let mut hash = 0xdead_beef_cafe_babe_u64;

    hash_u64(&mut hash, config.layout_iteration_budget as u64);
    hash_u64(&mut hash, config.route_budget as u64);
    hash_u64(&mut hash, config.max_nodes as u64);
    hash_u64(&mut hash, config.max_edges as u64);
    hash_u64(&mut hash, config.max_label_chars as u64);
    hash_u64(&mut hash, config.glyph_mode as u64);
    hash_u64(&mut hash, config.edge_bundling as u64);

    hash
}

fn hash_u64(state: &mut u64, value: u64) {
    *state = state
        .wrapping_mul(0x517c_c1b7_2722_0a95)
        .wrapping_add(value);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_metrics() -> crate::GraphMetrics {
        crate::GraphMetrics {
            node_count: 10,
            edge_count: 15,
            edge_to_node_ratio: 1.5,
            back_edge_count: 0,
            scc_count: 0,
            max_scc_size: 1,
            root_count: 1,
            is_tree_like: false,
            is_sparse: false,
            is_dense: false,
        }
    }

    fn make_test_layout() -> crate::DiagramLayout {
        crate::DiagramLayout {
            nodes: vec![],
            clusters: vec![],
            cycle_clusters: vec![],
            edges: vec![],
            bounds: crate::LayoutRect {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            },
            stats: crate::LayoutStats {
                node_count: 0,
                edge_count: 0,
                crossing_count: 0,
                crossing_count_before_refinement: 0,
                reversed_edges: 0,
                cycle_count: 0,
                cycle_node_count: 0,
                max_cycle_size: 0,
                collapsed_clusters: 0,
                reversed_edge_total_length: 0.0,
                total_edge_length: 0.0,
                phase_iterations: 0,
            },
            extensions: crate::LayoutExtensions::default(),
            dirty_regions: vec![],
        }
    }

    #[test]
    fn test_initial_state_is_dirty() {
        let dcg = LayoutDcg::new();

        assert!(dcg.metrics_dirty());
        assert!(dcg.ranks_dirty());
        assert!(dcg.orderings_dirty());
        assert!(dcg.layout_dirty());
        assert!(dcg.any_dirty());
        assert!(!dcg.fully_cached());
    }

    #[test]
    fn test_set_metrics_clears_dirty() {
        let dcg = LayoutDcg::new();

        dcg.set_metrics(make_test_metrics());

        assert!(!dcg.metrics_dirty());
        assert!(dcg.ranks_dirty()); // Other phases still dirty
        assert!(dcg.get_metrics().is_some());
    }

    #[test]
    fn test_ir_change_invalidates_all() {
        let dcg = LayoutDcg::new();

        // Populate all caches
        dcg.set_metrics(make_test_metrics());
        dcg.set_ranks(vec![0, 1, 1, 2]);
        dcg.set_orderings(vec![vec![0], vec![1, 2], vec![3]]);

        assert!(!dcg.metrics_dirty());
        assert!(!dcg.ranks_dirty());
        assert!(!dcg.orderings_dirty());

        // Change IR fingerprint
        dcg.set_ir_fingerprint(12345);

        // All should be dirty now
        assert!(dcg.metrics_dirty());
        assert!(dcg.ranks_dirty());
        assert!(dcg.orderings_dirty());
        assert!(dcg.layout_dirty());
    }

    #[test]
    fn test_config_change_preserves_metrics() {
        let dcg = LayoutDcg::new();
        dcg.set_ir_fingerprint(100);

        // Populate caches
        dcg.set_metrics(make_test_metrics());
        dcg.set_ranks(vec![0, 1, 2]);
        dcg.set_orderings(vec![vec![0], vec![1], vec![2]]);

        assert!(!dcg.metrics_dirty());
        assert!(!dcg.ranks_dirty());
        assert!(!dcg.orderings_dirty());

        // Change config
        dcg.set_config_fingerprint(999);

        // Metrics and ranks should still be clean (topology didn't change)
        assert!(!dcg.metrics_dirty());
        // Orderings and layout depend on config
        assert!(dcg.orderings_dirty());
        assert!(dcg.layout_dirty());
    }

    #[test]
    fn test_same_fingerprint_no_invalidation() {
        let dcg = LayoutDcg::new();
        dcg.set_ir_fingerprint(100);
        dcg.set_metrics(make_test_metrics());

        assert!(!dcg.metrics_dirty());

        // Set same fingerprint
        dcg.set_ir_fingerprint(100);

        // Should still be clean
        assert!(!dcg.metrics_dirty());
        assert_eq!(dcg.stats().input_changes, 1); // Only counted once
    }

    #[test]
    fn test_stats_tracking() {
        let dcg = LayoutDcg::new();

        dcg.set_ir_fingerprint(100);
        dcg.set_config_fingerprint(200);
        assert_eq!(dcg.stats().input_changes, 2);

        dcg.set_metrics(make_test_metrics());
        assert_eq!(dcg.stats().metrics_recomputes, 1);

        let _ = dcg.get_metrics();
        assert_eq!(dcg.stats().metrics_hits, 1);

        let _ = dcg.get_metrics();
        assert_eq!(dcg.stats().metrics_hits, 2);
    }

    #[test]
    fn test_dirty_summary() {
        let dcg = LayoutDcg::new();

        assert_eq!(dcg.dirty_summary(), "metrics, ranks, orderings, layout");

        dcg.set_metrics(make_test_metrics());
        dcg.set_ranks(vec![0, 1]);
        dcg.set_orderings(vec![vec![0], vec![1]]);

        assert_eq!(dcg.dirty_summary(), "layout");

        dcg.set_layout(make_test_layout());

        assert_eq!(dcg.dirty_summary(), "clean");
    }

    #[test]
    fn test_invalidate_all() {
        let dcg = LayoutDcg::new();
        dcg.set_metrics(make_test_metrics());
        dcg.set_ranks(vec![0]);
        dcg.set_orderings(vec![vec![0]]);
        dcg.set_layout(make_test_layout());

        assert!(dcg.fully_cached());

        dcg.invalidate_all();

        assert!(!dcg.fully_cached());
        assert!(dcg.metrics_dirty());
        assert!(dcg.get_metrics().is_none());
    }
}
