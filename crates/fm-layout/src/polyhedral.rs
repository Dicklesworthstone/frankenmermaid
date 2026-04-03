//! Polyhedral optimization for layout algorithm loop nests.
//!
//! Applies polyhedral model concepts to optimize the cache behavior of
//! crossing minimization and coordinate assignment inner loops. Instead of
//! a full polyhedral compiler, this module provides practical tiling and
//! reordering primitives tuned for Sugiyama layout's access patterns.
//!
//! # Background
//!
//! The crossing minimization loop in Sugiyama layout iterates:
//! ```text
//! for sweep in 0..max_sweeps {        // outer: ~4-20 sweeps
//!     for layer in 0..num_layers {     // middle: ~5-50 layers
//!         for node in layer_nodes {    // inner: ~5-500 nodes per layer
//!             compute_barycenter(node, adjacent_layer)
//!         }
//!     }
//! }
//! ```
//!
//! The inner loop accesses both the current layer's nodes and the adjacent
//! layer's positions. Tiling the layer dimension ensures both layers fit
//! in cache simultaneously.
//!
//! # Techniques
//!
//! 1. **Iteration space tiling**: Process layers in tiles of size T (chosen
//!    to fit two adjacent layers in L1 cache). Within each tile, complete
//!    all sweeps before moving to the next tile.
//!
//! 2. **Strip-mining**: Split the node dimension into strips of size S for
//!    SIMD-friendly access patterns (though Rust auto-vectorization handles
//!    most cases).
//!
//! 3. **Loop interchange**: For coordinate assignment, swap the sweep and
//!    layer loops to improve data reuse when node positions are updated.
//!
//! # References
//!
//! - Bastoul, "Code Generation in the Polyhedral Model" (PhD thesis, 2004)
//! - Bondhugula et al., "A Practical Automatic Polyhedral Parallelizer" (PLDI 2008)

/// Configuration for tiled iteration.
#[derive(Debug, Clone, Copy)]
pub struct TileConfig {
    /// Tile size for the layer dimension. Should be chosen so that
    /// two adjacent layers fit in L1 cache (~32KB / node_size_bytes).
    /// Default: 8 layers per tile.
    pub layer_tile_size: usize,
    /// Strip size for the node dimension. Default: 64 (cache line aligned).
    pub node_strip_size: usize,
}

impl Default for TileConfig {
    fn default() -> Self {
        Self {
            layer_tile_size: 8,
            node_strip_size: 64,
        }
    }
}

/// Compute an appropriate tile size based on cache parameters.
///
/// # Arguments
/// * `avg_nodes_per_layer` - Average number of nodes per layer.
/// * `node_size_bytes` - Size of per-node data in bytes (position, barycenter, etc.).
/// * `cache_size_bytes` - Target cache size (e.g., 32768 for L1).
#[must_use]
pub fn auto_tile_config(
    avg_nodes_per_layer: usize,
    node_size_bytes: usize,
    cache_size_bytes: usize,
) -> TileConfig {
    let bytes_per_layer = avg_nodes_per_layer * node_size_bytes;
    // We need two adjacent layers to fit in cache (current + neighbor).
    let max_layers = cache_size_bytes
        .checked_div(2 * bytes_per_layer)
        .unwrap_or(8);
    let layer_tile = max_layers.clamp(2, 32);

    // Strip size: target 1-2 cache lines worth of nodes.
    let cache_line = 64_usize; // bytes
    let nodes_per_line = cache_line.checked_div(node_size_bytes).unwrap_or(8);
    let node_strip = nodes_per_line.clamp(4, 128);

    TileConfig {
        layer_tile_size: layer_tile,
        node_strip_size: node_strip,
    }
}

/// A tiled iteration schedule for crossing minimization sweeps.
///
/// Instead of iterating `for layer in 0..n`, iterate over tiles:
/// ```text
/// for tile_start in (0..num_layers).step_by(tile_size) {
///     for sweep in 0..max_sweeps {
///         for layer in tile_start..min(tile_start + tile_size, num_layers) {
///             process_layer(layer)
///         }
///     }
/// }
/// ```
///
/// This ensures that adjacent layers stay in cache across sweeps.
#[derive(Debug, Clone)]
pub struct TiledSchedule {
    /// Layer tile boundaries: each entry is (tile_start, tile_end).
    pub layer_tiles: Vec<(usize, usize)>,
    /// Number of sweeps per tile.
    pub sweeps_per_tile: usize,
    /// Total number of layers.
    pub num_layers: usize,
    /// Configuration used.
    pub config: TileConfig,
}

impl TiledSchedule {
    /// Create a tiled schedule for crossing minimization.
    ///
    /// # Arguments
    /// * `num_layers` - Total number of layers in the Sugiyama layout.
    /// * `max_sweeps` - Maximum number of barycenter sweeps.
    /// * `config` - Tile configuration.
    #[must_use]
    pub fn new(num_layers: usize, max_sweeps: usize, config: TileConfig) -> Self {
        let tile_size = config.layer_tile_size.max(1);
        let mut tiles = Vec::new();
        let mut start = 0;
        while start < num_layers {
            let end = (start + tile_size).min(num_layers);
            tiles.push((start, end));
            start = end;
        }

        Self {
            layer_tiles: tiles,
            sweeps_per_tile: max_sweeps,
            num_layers,
            config,
        }
    }

    /// Iterate over the schedule, yielding (sweep, layer) pairs in tiled order.
    ///
    /// The caller processes layers in this order for optimal cache behavior.
    #[must_use]
    pub fn iterate(&self) -> Vec<(usize, usize)> {
        let mut schedule = Vec::new();
        for &(tile_start, tile_end) in &self.layer_tiles {
            for sweep in 0..self.sweeps_per_tile {
                // Alternate direction for odd sweeps (like barycenter).
                if sweep % 2 == 0 {
                    for layer in tile_start..tile_end {
                        schedule.push((sweep, layer));
                    }
                } else {
                    for layer in (tile_start..tile_end).rev() {
                        schedule.push((sweep, layer));
                    }
                }
            }
        }
        schedule
    }

    /// Number of (sweep, layer) iterations in the schedule.
    #[must_use]
    pub fn total_iterations(&self) -> usize {
        self.layer_tiles
            .iter()
            .map(|&(start, end)| (end - start) * self.sweeps_per_tile)
            .sum()
    }
}

/// Strip-mine the node dimension for SIMD-friendly access.
///
/// Returns strip boundaries `(strip_start, strip_end)` for iterating over
/// nodes in cache-line-aligned chunks.
#[must_use]
pub fn strip_mine(num_nodes: usize, strip_size: usize) -> Vec<(usize, usize)> {
    let strip = strip_size.max(1);
    let mut strips = Vec::new();
    let mut start = 0;
    while start < num_nodes {
        let end = (start + strip).min(num_nodes);
        strips.push((start, end));
        start = end;
    }
    strips
}

/// Analyze whether loop interchange is profitable for coordinate assignment.
///
/// In coordinate assignment, the default loop order is:
/// ```text
/// for sweep in 0..max_sweeps {
///     for layer in 0..num_layers {
///         for node in layer_nodes { update_position(node) }
///     }
/// }
/// ```
///
/// Interchange to `layer → sweep → node` is profitable when:
/// - Number of sweeps is small (< 10)
/// - Layers are large (> 100 nodes)
/// - Node position arrays are contiguous per-layer
///
/// Returns `true` if interchange is recommended.
#[must_use]
pub fn should_interchange(
    num_layers: usize,
    avg_nodes_per_layer: usize,
    max_sweeps: usize,
) -> bool {
    // Interchange is profitable when the inner dimension (nodes) is large
    // relative to the outer dimension (sweeps), and we can keep a single
    // layer's data in cache across all sweeps.
    max_sweeps <= 10 && avg_nodes_per_layer > 100 && num_layers > 3
}

/// Estimate the cache miss reduction from tiling.
///
/// Returns the estimated ratio of cache misses with tiling vs without.
/// A ratio < 1.0 means tiling helps.
///
/// # Arguments
/// * `num_layers` - Total layers.
/// * `avg_nodes` - Average nodes per layer.
/// * `node_bytes` - Bytes per node data.
/// * `cache_bytes` - Cache size.
/// * `sweeps` - Number of sweeps.
#[must_use]
pub fn estimate_miss_ratio(
    num_layers: usize,
    avg_nodes: usize,
    node_bytes: usize,
    cache_bytes: usize,
    sweeps: usize,
) -> f64 {
    let working_set = num_layers * avg_nodes * node_bytes;

    if working_set <= cache_bytes {
        // Everything fits in cache — tiling doesn't help.
        return 1.0;
    }

    // Without tiling: each sweep touches all layers sequentially.
    // With N layers and cache holding C layers, we get ~N/C * sweeps cold misses.
    let layers_in_cache = if avg_nodes * node_bytes > 0 {
        (cache_bytes / (avg_nodes * node_bytes * 2)).max(1) // factor of 2 for adjacent layers
    } else {
        num_layers
    };

    if layers_in_cache >= num_layers {
        return 1.0;
    }

    // Without tiling: each sweep reloads all layers = sweeps * num_layers loads.
    let without = (sweeps * num_layers) as f64;

    // With tiling: each tile of `layers_in_cache` layers is loaded once per tile,
    // and all sweeps for that tile run while it's in cache.
    let num_tiles = num_layers.div_ceil(layers_in_cache);
    let with = (num_tiles * layers_in_cache) as f64; // Each tile loaded once.

    if without > 0.0 { with / without } else { 1.0 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_tile_config() {
        let config = TileConfig::default();
        assert_eq!(config.layer_tile_size, 8);
        assert_eq!(config.node_strip_size, 64);
    }

    #[test]
    fn auto_tile_fits_in_cache() {
        // 50 nodes/layer, 16 bytes/node = 800 bytes/layer.
        // L1 = 32KB = 32768 bytes. Two layers = 1600 bytes.
        // Can fit 32768/1600 = 20 layers.
        let config = auto_tile_config(50, 16, 32768);
        assert!(config.layer_tile_size >= 2);
        assert!(config.layer_tile_size <= 32);
    }

    #[test]
    fn auto_tile_large_layers() {
        // 1000 nodes/layer, 32 bytes/node = 32KB/layer.
        // L1 = 32KB. Two layers = 64KB > cache. Tile = 2 (minimum).
        let config = auto_tile_config(1000, 32, 32768);
        assert_eq!(config.layer_tile_size, 2);
    }

    #[test]
    fn tiled_schedule_covers_all_layers() {
        let schedule = TiledSchedule::new(20, 4, TileConfig::default());

        // All layers should appear in the schedule.
        let mut layers_seen: std::collections::BTreeSet<usize> = std::collections::BTreeSet::new();
        for &(_, layer) in &schedule.iterate() {
            layers_seen.insert(layer);
        }
        let expected: std::collections::BTreeSet<usize> = (0..20).collect();
        assert_eq!(layers_seen, expected);
    }

    #[test]
    fn tiled_schedule_total_iterations() {
        let schedule = TiledSchedule::new(20, 4, TileConfig::default());
        assert_eq!(schedule.total_iterations(), 20 * 4);
    }

    #[test]
    fn tiled_schedule_alternates_direction() {
        let config = TileConfig {
            layer_tile_size: 4,
            node_strip_size: 64,
        };
        let schedule = TiledSchedule::new(4, 2, config);
        let iterations = schedule.iterate();

        // Sweep 0: forward (0,1,2,3)
        // Sweep 1: reverse (3,2,1,0)
        assert_eq!(iterations[0], (0, 0));
        assert_eq!(iterations[1], (0, 1));
        assert_eq!(iterations[2], (0, 2));
        assert_eq!(iterations[3], (0, 3));
        assert_eq!(iterations[4], (1, 3));
        assert_eq!(iterations[5], (1, 2));
        assert_eq!(iterations[6], (1, 1));
        assert_eq!(iterations[7], (1, 0));
    }

    #[test]
    fn strip_mine_basic() {
        let strips = strip_mine(10, 3);
        assert_eq!(strips, vec![(0, 3), (3, 6), (6, 9), (9, 10)]);
    }

    #[test]
    fn strip_mine_exact_fit() {
        let strips = strip_mine(8, 4);
        assert_eq!(strips, vec![(0, 4), (4, 8)]);
    }

    #[test]
    fn strip_mine_empty() {
        let strips = strip_mine(0, 4);
        assert!(strips.is_empty());
    }

    #[test]
    fn should_interchange_large_layers() {
        // Large layers + few sweeps → interchange profitable.
        assert!(should_interchange(10, 200, 4));
    }

    #[test]
    fn should_not_interchange_small_layers() {
        // Small layers → interchange not profitable.
        assert!(!should_interchange(10, 20, 4));
    }

    #[test]
    fn should_not_interchange_many_sweeps() {
        // Many sweeps → default order better.
        assert!(!should_interchange(10, 200, 20));
    }

    #[test]
    fn miss_ratio_small_working_set() {
        // Everything fits in cache → ratio = 1.0 (no benefit).
        let ratio = estimate_miss_ratio(5, 10, 16, 32768, 4);
        assert!((ratio - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn miss_ratio_large_working_set() {
        // Working set much larger than cache → tiling helps.
        let ratio = estimate_miss_ratio(100, 500, 32, 32768, 10);
        assert!(
            ratio < 1.0,
            "Tiling should reduce misses for large working set, got ratio {ratio}"
        );
    }

    #[test]
    fn tiled_schedule_single_layer() {
        let schedule = TiledSchedule::new(1, 4, TileConfig::default());
        assert_eq!(schedule.total_iterations(), 4);
        let iters = schedule.iterate();
        for (i, &(sweep, layer)) in iters.iter().enumerate() {
            assert_eq!(sweep, i);
            assert_eq!(layer, 0);
        }
    }

    #[test]
    fn estimate_deterministic() {
        let r1 = estimate_miss_ratio(50, 100, 16, 32768, 8);
        let r2 = estimate_miss_ratio(50, 100, 16, 32768, 8);
        assert_eq!(r1, r2);
    }
}
