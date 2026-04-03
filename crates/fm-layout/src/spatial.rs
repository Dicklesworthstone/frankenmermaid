//! Spatial indexing for fast nearest-node queries.
//!
//! Provides two spatial index implementations for O(1) expected-time cursor-to-node
//! proximity detection:
//!
//! 1. **Grid spatial hash** — Simple, fast, and sufficient for most diagrams (<5000 nodes).
//!    Divides the viewport into cells of width `w`. Queries check the cell containing the
//!    query point plus its 8 neighbors.
//!
//! 2. **Locality-Sensitive Hashing (LSH)** — For very large diagrams (5000+ nodes).
//!    Uses random projections (p-stable distributions) to hash nearby points to the same
//!    bucket with high probability.
//!
//! # Usage
//!
//! ```rust
//! use fm_layout::spatial::{GridSpatialIndex, SpatialIndex};
//!
//! let mut index = GridSpatialIndex::new(50.0); // 50px cell width
//! index.insert(0, (100.0, 200.0));
//! index.insert(1, (105.0, 195.0));
//!
//! let nearest = index.nearest((102.0, 198.0), 20.0);
//! assert_eq!(nearest, Some(0));
//! // Returns Some(0) — node 0 is closest within radius 20
//! ```
//!
//! # References
//!
//! - Indyk & Motwani, "Approximate Nearest Neighbors" (STOC 1998)
//! - Datar et al., "Locality-Sensitive Hashing Scheme Based on p-Stable Distributions" (SCG 2004)

use std::collections::BTreeMap;

use tracing::trace;

/// A positioned node entry: (node_id, x, y).
type NodeEntry = (usize, f64, f64);

/// Cell key for the grid spatial hash.
type CellKey = (i64, i64);

/// Trait for spatial index implementations.
pub trait SpatialIndex {
    /// Insert a node at the given position.
    fn insert(&mut self, node_id: usize, position: (f64, f64));

    /// Find the nearest node within `radius` of `point`.
    /// Returns `None` if no node is within range.
    fn nearest(&self, point: (f64, f64), radius: f64) -> Option<usize>;

    /// Find all nodes within `radius` of `point`.
    fn within_radius(&self, point: (f64, f64), radius: f64) -> Vec<usize>;

    /// Number of indexed nodes.
    fn len(&self) -> usize;

    /// Whether the index is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear all entries.
    fn clear(&mut self);
}

// ---------------------------------------------------------------------------
// Grid Spatial Hash
// ---------------------------------------------------------------------------

/// A grid-based spatial hash for fast nearest-node queries.
///
/// Divides 2D space into square cells of side `cell_size`. Each cell stores
/// node IDs whose positions fall within it. Queries check the cell containing
/// the query point plus its 8 neighbors (9 cells total).
///
/// Expected query time: O(k) where k = average nodes per 9-cell neighborhood.
/// For well-distributed layouts this is typically O(1).
#[derive(Debug, Clone)]
pub struct GridSpatialIndex {
    cell_size: f64,
    inv_cell_size: f64,
    /// Maps (cell_x, cell_y) → list of (node_id, x, y).
    cells: BTreeMap<CellKey, Vec<NodeEntry>>,
    count: usize,
}

impl GridSpatialIndex {
    /// Create a new grid spatial index with the given cell size.
    ///
    /// `cell_size` should be roughly the expected query radius. For diagram
    /// layouts, typical values are 50–200 pixels.
    #[must_use]
    pub fn new(cell_size: f64) -> Self {
        let cell_size = cell_size.max(1.0);
        Self {
            cell_size,
            inv_cell_size: 1.0 / cell_size,
            cells: BTreeMap::new(),
            count: 0,
        }
    }

    /// Create a grid index pre-populated with nodes.
    #[must_use]
    pub fn from_positions(positions: &[(usize, f64, f64)], cell_size: f64) -> Self {
        let mut index = Self::new(cell_size);
        for &(id, x, y) in positions {
            index.insert(id, (x, y));
        }
        index
    }

    /// Map a coordinate to its cell index.
    fn cell_coord(&self, x: f64, y: f64) -> (i64, i64) {
        let cx = (x * self.inv_cell_size).floor() as i64;
        let cy = (y * self.inv_cell_size).floor() as i64;
        (cx, cy)
    }

    /// Iterate over the 9 cells surrounding (and including) the cell at (cx, cy).
    fn neighborhood(cx: i64, cy: i64) -> impl Iterator<Item = (i64, i64)> {
        (-1..=1).flat_map(move |dx| (-1..=1).map(move |dy| (cx + dx, cy + dy)))
    }
}

impl SpatialIndex for GridSpatialIndex {
    fn insert(&mut self, node_id: usize, position: (f64, f64)) {
        let cell = self.cell_coord(position.0, position.1);
        self.cells
            .entry(cell)
            .or_default()
            .push((node_id, position.0, position.1));
        self.count += 1;
    }

    fn nearest(&self, point: (f64, f64), radius: f64) -> Option<usize> {
        let (cx, cy) = self.cell_coord(point.0, point.1);
        let r2 = radius * radius;

        let mut best_id = None;
        let mut best_dist2 = r2;

        for (nx, ny) in Self::neighborhood(cx, cy) {
            if let Some(nodes) = self.cells.get(&(nx, ny)) {
                for &(id, x, y) in nodes {
                    let dx = x - point.0;
                    let dy = y - point.1;
                    let dist2 = dx * dx + dy * dy;
                    if dist2 < best_dist2 {
                        best_dist2 = dist2;
                        best_id = Some(id);
                    }
                }
            }
        }

        // If radius > cell_size, we might miss nodes in farther cells.
        // For correctness, also check cells within ceil(radius/cell_size) distance.
        if radius > self.cell_size {
            let extra_range = (radius * self.inv_cell_size).ceil() as i64;
            for dx in -extra_range..=extra_range {
                for dy in -extra_range..=extra_range {
                    if dx.abs() <= 1 && dy.abs() <= 1 {
                        continue; // Already checked in neighborhood
                    }
                    if let Some(nodes) = self.cells.get(&(cx + dx, cy + dy)) {
                        for &(id, x, y) in nodes {
                            let ddx = x - point.0;
                            let ddy = y - point.1;
                            let dist2 = ddx * ddx + ddy * ddy;
                            if dist2 < best_dist2 {
                                best_dist2 = dist2;
                                best_id = Some(id);
                            }
                        }
                    }
                }
            }
        }

        best_id
    }

    fn within_radius(&self, point: (f64, f64), radius: f64) -> Vec<usize> {
        let (cx, cy) = self.cell_coord(point.0, point.1);
        let r2 = radius * radius;
        let cell_range = (radius * self.inv_cell_size).ceil() as i64;
        let cell_range = cell_range.max(1);

        let mut result = Vec::new();

        for dx in -cell_range..=cell_range {
            for dy in -cell_range..=cell_range {
                if let Some(nodes) = self.cells.get(&(cx + dx, cy + dy)) {
                    for &(id, x, y) in nodes {
                        let ddx = x - point.0;
                        let ddy = y - point.1;
                        if ddx * ddx + ddy * ddy <= r2 {
                            result.push(id);
                        }
                    }
                }
            }
        }

        result
    }

    fn len(&self) -> usize {
        self.count
    }

    fn clear(&mut self) {
        self.cells.clear();
        self.count = 0;
    }
}

// ---------------------------------------------------------------------------
// Locality-Sensitive Hashing (LSH)
// ---------------------------------------------------------------------------

/// A single LSH hash function: h(p) = floor((a·p + b) / w)
/// where a is a random direction vector and b is a random offset.
#[derive(Debug, Clone, Copy)]
struct LshHashFunction {
    ax: f64,
    ay: f64,
    b: f64,
    w: f64,
}

impl LshHashFunction {
    /// Evaluate the hash function on a 2D point.
    fn hash(&self, x: f64, y: f64) -> i64 {
        let dot = self.ax * x + self.ay * y;
        ((dot + self.b) / self.w).floor() as i64
    }
}

/// Locality-Sensitive Hashing spatial index for approximate nearest-node queries.
///
/// Uses `num_tables` independent hash tables, each with `num_hashes` hash functions
/// concatenated. A query checks all tables and returns the nearest candidate.
///
/// # Parameters
/// - `bucket_width`: controls sensitivity. Smaller = more precise but more tables needed.
/// - `num_hashes`: hash functions per table (higher = fewer false positives, more false negatives).
/// - `num_tables`: independent hash tables (higher = fewer false negatives, more memory).
#[derive(Debug, Clone)]
pub struct LshSpatialIndex {
    /// Hash function tables.
    tables: Vec<LshTable>,
    /// All positions for distance computation during candidate verification.
    positions: Vec<(usize, f64, f64)>,
    count: usize,
}

/// A single LSH hash table with its hash functions and buckets.
#[derive(Debug, Clone)]
struct LshTable {
    hash_functions: Vec<LshHashFunction>,
    buckets: BTreeMap<Vec<i64>, Vec<usize>>, // composite hash → position indices
}

impl LshTable {
    fn composite_hash(&self, x: f64, y: f64) -> Vec<i64> {
        self.hash_functions.iter().map(|h| h.hash(x, y)).collect()
    }
}

/// Configuration for LSH spatial index.
#[derive(Debug, Clone, Copy)]
pub struct LshConfig {
    /// Bucket width (controls hash sensitivity). Default: 100.0
    pub bucket_width: f64,
    /// Number of hash functions per table. Default: 4
    pub num_hashes: usize,
    /// Number of independent hash tables. Default: 8
    pub num_tables: usize,
    /// Deterministic seed for reproducible hash functions. Default: 42
    pub seed: u64,
}

impl Default for LshConfig {
    fn default() -> Self {
        Self {
            bucket_width: 100.0,
            num_hashes: 4,
            num_tables: 8,
            seed: 42,
        }
    }
}

/// Simple deterministic pseudo-random number generator (xorshift64).
/// Used to generate LSH hash function parameters reproducibly.
struct DetRng {
    state: u64,
}

impl DetRng {
    fn new(seed: u64) -> Self {
        // xorshift64 has a fixed point at 0 — ensure we never start there.
        let state = seed.wrapping_add(1);
        Self {
            state: if state == 0 { 1 } else { state },
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    /// Generate a f64 in [0, 1).
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / ((1_u64 << 53) as f64)
    }

    /// Generate a standard normal variate using Box-Muller transform.
    fn next_normal(&mut self) -> f64 {
        let u1 = self.next_f64().max(f64::MIN_POSITIVE);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

impl LshSpatialIndex {
    /// Create a new LSH spatial index with default configuration.
    #[must_use]
    pub fn new() -> Self {
        Self::with_config(LshConfig::default())
    }

    /// Create a new LSH spatial index with custom configuration.
    #[must_use]
    pub fn with_config(config: LshConfig) -> Self {
        let mut rng = DetRng::new(config.seed);
        let tables = (0..config.num_tables)
            .map(|_| {
                let hash_functions = (0..config.num_hashes)
                    .map(|_| {
                        // Random direction from standard normal (p-stable for L2)
                        let ax = rng.next_normal();
                        let ay = rng.next_normal();
                        let b = rng.next_f64() * config.bucket_width;
                        LshHashFunction {
                            ax,
                            ay,
                            b,
                            w: config.bucket_width,
                        }
                    })
                    .collect();
                LshTable {
                    hash_functions,
                    buckets: BTreeMap::new(),
                }
            })
            .collect();

        Self {
            tables,
            positions: Vec::new(),
            count: 0,
        }
    }
}

impl Default for LshSpatialIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl SpatialIndex for LshSpatialIndex {
    fn insert(&mut self, node_id: usize, position: (f64, f64)) {
        let pos_idx = self.positions.len();
        self.positions.push((node_id, position.0, position.1));

        for table in &mut self.tables {
            let key = table.composite_hash(position.0, position.1);
            table.buckets.entry(key).or_default().push(pos_idx);
        }

        self.count += 1;
    }

    fn nearest(&self, point: (f64, f64), radius: f64) -> Option<usize> {
        let r2 = radius * radius;
        let mut best_id = None;
        let mut best_dist2 = r2;

        // Collect candidate indices from all tables.
        for table in &self.tables {
            let key = table.composite_hash(point.0, point.1);
            if let Some(indices) = table.buckets.get(&key) {
                for &pos_idx in indices {
                    let (id, x, y) = self.positions[pos_idx];
                    let dx = x - point.0;
                    let dy = y - point.1;
                    let dist2 = dx * dx + dy * dy;
                    if dist2 < best_dist2 {
                        best_dist2 = dist2;
                        best_id = Some(id);
                    }
                }
            }
        }

        best_id
    }

    fn within_radius(&self, point: (f64, f64), radius: f64) -> Vec<usize> {
        let r2 = radius * radius;
        let mut result_set = std::collections::BTreeSet::new();

        for table in &self.tables {
            let key = table.composite_hash(point.0, point.1);
            if let Some(indices) = table.buckets.get(&key) {
                for &pos_idx in indices {
                    let (id, x, y) = self.positions[pos_idx];
                    let dx = x - point.0;
                    let dy = y - point.1;
                    if dx * dx + dy * dy <= r2 {
                        result_set.insert(id);
                    }
                }
            }
        }

        result_set.into_iter().collect()
    }

    fn len(&self) -> usize {
        self.count
    }

    fn clear(&mut self) {
        for table in &mut self.tables {
            table.buckets.clear();
        }
        self.positions.clear();
        self.count = 0;
    }
}

// ---------------------------------------------------------------------------
// Auto-selecting spatial index
// ---------------------------------------------------------------------------

/// Threshold for switching from grid to LSH index.
const LSH_THRESHOLD: usize = 5000;

/// Create an appropriate spatial index for the given number of nodes.
///
/// Returns a `GridSpatialIndex` for small graphs (< 5000 nodes) and
/// an `LshSpatialIndex` for large graphs.
#[must_use]
pub fn create_spatial_index(
    positions: &[(usize, f64, f64)],
    cell_size: f64,
) -> Box<dyn SpatialIndex> {
    if positions.len() < LSH_THRESHOLD {
        trace!(
            node_count = positions.len(),
            strategy = "grid",
            "Creating grid spatial index"
        );
        Box::new(GridSpatialIndex::from_positions(positions, cell_size))
    } else {
        trace!(
            node_count = positions.len(),
            strategy = "lsh",
            "Creating LSH spatial index"
        );
        let config = LshConfig {
            bucket_width: cell_size * 2.0,
            ..Default::default()
        };
        let mut index = LshSpatialIndex::with_config(config);
        for &(id, x, y) in positions {
            index.insert(id, (x, y));
        }
        Box::new(index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // -- Grid Spatial Hash tests --

    #[test]
    fn grid_empty_nearest_returns_none() {
        let index = GridSpatialIndex::new(50.0);
        assert!(index.nearest((0.0, 0.0), 100.0).is_none());
        assert!(index.is_empty());
    }

    #[test]
    fn grid_single_node_found() {
        let mut index = GridSpatialIndex::new(50.0);
        index.insert(0, (100.0, 100.0));

        assert_eq!(index.nearest((100.0, 100.0), 10.0), Some(0));
        assert_eq!(index.nearest((105.0, 95.0), 20.0), Some(0));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn grid_nearest_picks_closest() {
        let mut index = GridSpatialIndex::new(50.0);
        index.insert(0, (0.0, 0.0));
        index.insert(1, (10.0, 0.0));
        index.insert(2, (100.0, 0.0));

        assert_eq!(index.nearest((8.0, 0.0), 50.0), Some(1));
        assert_eq!(index.nearest((90.0, 0.0), 50.0), Some(2));
    }

    #[test]
    fn grid_out_of_radius_not_found() {
        let mut index = GridSpatialIndex::new(50.0);
        index.insert(0, (0.0, 0.0));

        assert!(index.nearest((200.0, 200.0), 10.0).is_none());
    }

    #[test]
    fn grid_within_radius_returns_all() {
        let mut index = GridSpatialIndex::new(50.0);
        index.insert(0, (0.0, 0.0));
        index.insert(1, (5.0, 0.0));
        index.insert(2, (100.0, 100.0));

        let mut result = index.within_radius((0.0, 0.0), 10.0);
        result.sort();
        assert_eq!(result, vec![0, 1]);
    }

    #[test]
    fn grid_negative_coordinates() {
        let mut index = GridSpatialIndex::new(50.0);
        index.insert(0, (-100.0, -200.0));
        index.insert(1, (-95.0, -205.0));

        // Node 0 at (-100,-200) is dist 2.83 from query; node 1 at (-95,-205) is dist 4.24.
        assert_eq!(index.nearest((-98.0, -202.0), 20.0), Some(0));
    }

    #[test]
    fn grid_clear_empties_index() {
        let mut index = GridSpatialIndex::new(50.0);
        index.insert(0, (0.0, 0.0));
        index.insert(1, (10.0, 10.0));
        assert_eq!(index.len(), 2);

        index.clear();
        assert!(index.is_empty());
        assert!(index.nearest((0.0, 0.0), 100.0).is_none());
    }

    #[test]
    fn grid_from_positions() {
        let positions = vec![(0, 0.0, 0.0), (1, 50.0, 50.0), (2, 100.0, 100.0)];
        let index = GridSpatialIndex::from_positions(&positions, 60.0);
        assert_eq!(index.len(), 3);
        assert_eq!(index.nearest((48.0, 48.0), 10.0), Some(1));
    }

    #[test]
    fn grid_large_radius_correctness() {
        // Radius larger than cell_size should still find distant nodes.
        let mut index = GridSpatialIndex::new(10.0);
        index.insert(0, (0.0, 0.0));
        index.insert(1, (50.0, 0.0)); // 5 cells away

        assert_eq!(index.nearest((0.0, 0.0), 100.0), Some(0));
        assert_eq!(index.nearest((45.0, 0.0), 100.0), Some(1));
    }

    #[test]
    fn grid_deterministic() {
        let positions = vec![
            (0, 10.0, 20.0),
            (1, 30.0, 40.0),
            (2, 50.0, 60.0),
            (3, 70.0, 80.0),
        ];

        let i1 = GridSpatialIndex::from_positions(&positions, 50.0);
        let i2 = GridSpatialIndex::from_positions(&positions, 50.0);

        let p = (25.0, 35.0);
        assert_eq!(i1.nearest(p, 100.0), i2.nearest(p, 100.0));
    }

    // -- LSH tests --

    #[test]
    fn lsh_empty_nearest_returns_none() {
        let index = LshSpatialIndex::new();
        assert!(index.nearest((0.0, 0.0), 100.0).is_none());
        assert!(index.is_empty());
    }

    #[test]
    fn lsh_single_node_found() {
        let mut index = LshSpatialIndex::with_config(LshConfig {
            bucket_width: 50.0,
            num_hashes: 2,
            num_tables: 4,
            seed: 42,
        });
        index.insert(0, (100.0, 100.0));

        // The node should be found if query point is in the same bucket.
        // With bucket_width=50 and point at exactly the same location, it should work.
        assert_eq!(index.nearest((100.0, 100.0), 10.0), Some(0));
    }

    #[test]
    fn lsh_nearby_nodes_found() {
        let config = LshConfig {
            bucket_width: 200.0, // large buckets for nearby point detection
            num_hashes: 2,
            num_tables: 8,
            seed: 42,
        };
        let mut index = LshSpatialIndex::with_config(config);
        index.insert(0, (0.0, 0.0));
        index.insert(1, (5.0, 5.0));
        index.insert(2, (1000.0, 1000.0));

        // Nodes 0 and 1 are very close; node 2 is far.
        let result = index.nearest((3.0, 3.0), 20.0);
        // Should find one of the nearby nodes.
        assert!(
            result == Some(0) || result == Some(1),
            "Expected nearby node, got {result:?}"
        );
    }

    #[test]
    fn lsh_deterministic() {
        let config = LshConfig {
            seed: 123,
            ..Default::default()
        };

        let mut i1 = LshSpatialIndex::with_config(config);
        let mut i2 = LshSpatialIndex::with_config(config);

        for id in 0..10 {
            let pos = (id as f64 * 10.0, id as f64 * 15.0);
            i1.insert(id, pos);
            i2.insert(id, pos);
        }

        let p = (45.0, 70.0);
        assert_eq!(
            i1.nearest(p, 100.0),
            i2.nearest(p, 100.0),
            "LSH should be deterministic with same seed"
        );
    }

    #[test]
    fn lsh_within_radius() {
        let config = LshConfig {
            bucket_width: 200.0,
            num_hashes: 2,
            num_tables: 8,
            seed: 42,
        };
        let mut index = LshSpatialIndex::with_config(config);
        index.insert(0, (0.0, 0.0));
        index.insert(1, (5.0, 0.0));
        index.insert(2, (1000.0, 1000.0));

        let result = index.within_radius((0.0, 0.0), 10.0);
        // Should find nodes 0 and 1 but not 2.
        assert!(result.contains(&0), "Node 0 should be within radius");
        assert!(result.contains(&1), "Node 1 should be within radius");
        assert!(!result.contains(&2), "Node 2 should NOT be within radius");
    }

    #[test]
    fn lsh_clear() {
        let mut index = LshSpatialIndex::new();
        index.insert(0, (0.0, 0.0));
        assert_eq!(index.len(), 1);

        index.clear();
        assert!(index.is_empty());
        assert!(index.nearest((0.0, 0.0), 100.0).is_none());
    }

    // -- Auto-selection tests --

    #[test]
    fn auto_select_grid_for_small() {
        let positions: Vec<(usize, f64, f64)> = (0..100)
            .map(|i| (i, i as f64 * 10.0, i as f64 * 5.0))
            .collect();
        let index = create_spatial_index(&positions, 50.0);
        assert_eq!(index.len(), 100);
        // Should find a node
        assert!(index.nearest((50.0, 25.0), 100.0).is_some());
    }

    // -- Accuracy comparison --

    #[test]
    fn grid_matches_brute_force() {
        // Verify grid spatial hash returns the same result as brute force.
        let positions: Vec<(usize, f64, f64)> = vec![
            (0, 10.0, 20.0),
            (1, 50.0, 30.0),
            (2, 80.0, 10.0),
            (3, 25.0, 70.0),
            (4, 90.0, 90.0),
        ];

        let index = GridSpatialIndex::from_positions(&positions, 30.0);

        let query = (45.0, 25.0);
        let radius = 100.0;

        // Brute force
        let brute_nearest = positions
            .iter()
            .filter_map(|&(id, x, y)| {
                let d2 = (x - query.0).powi(2) + (y - query.1).powi(2);
                if d2 <= radius * radius {
                    Some((id, d2))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .map(|(id, _)| id);

        let grid_nearest = index.nearest(query, radius);

        assert_eq!(grid_nearest, brute_nearest, "Grid should match brute force");
    }

    #[test]
    fn grid_within_radius_matches_brute_force() {
        let positions: Vec<(usize, f64, f64)> = vec![
            (0, 0.0, 0.0),
            (1, 3.0, 4.0),   // dist 5
            (2, 6.0, 8.0),   // dist 10
            (3, 30.0, 40.0), // dist 50
        ];

        let index = GridSpatialIndex::from_positions(&positions, 20.0);
        let query = (0.0, 0.0);
        let radius = 12.0;

        let mut grid_result = index.within_radius(query, radius);
        grid_result.sort();

        // Brute force
        let mut brute_result: Vec<usize> = positions
            .iter()
            .filter(|&&(_, x, y)| x * x + y * y <= radius * radius)
            .map(|&(id, _, _)| id)
            .collect();
        brute_result.sort();

        assert_eq!(grid_result, brute_result);
    }
}
