//! Topological persistence for stable layout features.
//!
//! Applies persistent homology to 2D node positions to identify stable topological
//! features (clusters and loops) that should be preserved across layout edits.
//! Features with high persistence (long-lived in the filtration) represent robust
//! structural aspects of the layout, while low-persistence features are noise.
//!
//! # Algorithm Overview
//!
//! 1. **Filtration**: Build a Vietoris-Rips filtration on node positions. Start with
//!    all nodes as isolated points. As the distance threshold ε increases, connect
//!    nodes when their pairwise distance drops below ε.
//!
//! 2. **Persistence**: Track the birth and death of topological features:
//!    - β₀ (connected components): born when a node appears, die when merged with another.
//!    - β₁ (loops): born when a cycle forms, die when filled in.
//!
//! 3. **Stability metric**: Compare persistence diagrams before and after an edit using
//!    the bottleneck or Wasserstein distance. Large distance = disruptive edit.
//!
//! # References
//!
//! - Edelsbrunner, Letscher & Zomorodian, "Topological Persistence and Simplification" (2002)
//! - Cohen-Steiner, Edelsbrunner & Harer, "Stability of Persistence Diagrams" (2007)

use std::cmp::Ordering;

use tracing::{debug, trace};

/// A point in the persistence diagram: a feature that is born at `birth` and dies at `death`.
///
/// Features on the diagonal (birth ≈ death) are noise.
/// Features far from the diagonal (high persistence) are stable structural features.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PersistencePoint {
    /// The filtration value at which this feature appears.
    pub birth: f64,
    /// The filtration value at which this feature disappears.
    /// `f64::INFINITY` for features that never die (essential features).
    pub death: f64,
    /// The topological dimension: 0 = connected component, 1 = loop.
    pub dimension: usize,
}

impl PersistencePoint {
    /// The persistence (lifetime) of this feature.
    #[must_use]
    pub fn persistence(&self) -> f64 {
        if self.death.is_infinite() {
            f64::INFINITY
        } else {
            self.death - self.birth
        }
    }

    /// Whether this is a finite (non-essential) feature.
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.death.is_finite()
    }
}

/// A persistence diagram: a multiset of (birth, death) points.
#[derive(Debug, Clone, PartialEq)]
pub struct PersistenceDiagram {
    pub points: Vec<PersistencePoint>,
}

impl PersistenceDiagram {
    /// Create a new empty diagram.
    #[must_use]
    pub fn new() -> Self {
        Self { points: Vec::new() }
    }

    /// Filter to only points of a given dimension.
    #[must_use]
    pub fn dimension(&self, dim: usize) -> Vec<PersistencePoint> {
        self.points
            .iter()
            .filter(|p| p.dimension == dim)
            .copied()
            .collect()
    }

    /// Get all finite points (excluding essential features).
    #[must_use]
    pub fn finite_points(&self) -> Vec<PersistencePoint> {
        self.points
            .iter()
            .filter(|p| p.is_finite())
            .copied()
            .collect()
    }

    /// Get points with persistence above a threshold.
    #[must_use]
    pub fn stable_features(&self, min_persistence: f64) -> Vec<PersistencePoint> {
        self.points
            .iter()
            .filter(|p| p.persistence() > min_persistence)
            .copied()
            .collect()
    }

    /// The number of features born (Betti numbers at infinity = essential features).
    #[must_use]
    pub fn betti_numbers(&self) -> (usize, usize) {
        let b0 = self
            .points
            .iter()
            .filter(|p| p.dimension == 0 && !p.is_finite())
            .count();
        let b1 = self
            .points
            .iter()
            .filter(|p| p.dimension == 1 && !p.is_finite())
            .count();
        (b0, b1)
    }
}

impl Default for PersistenceDiagram {
    fn default() -> Self {
        Self::new()
    }
}

/// An edge in the Vietoris-Rips complex, with its filtration value (distance).
#[derive(Debug, Clone, Copy)]
struct FiltrationEdge {
    u: usize,
    v: usize,
    distance: f64,
}

/// A triangle in the Vietoris-Rips complex, with its filtration value.
#[derive(Debug, Clone, Copy)]
struct FiltrationTriangle {
    u: usize,
    v: usize,
    w: usize,
    /// The filtration value is the maximum edge length in the triangle.
    filtration: f64,
}

/// Union-Find (disjoint set) data structure for tracking connected components.
struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
    size: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
            rank: vec![0; n],
            size: vec![1; n],
        }
    }

    fn find(&mut self, mut x: usize) -> usize {
        while self.parent[x] != x {
            self.parent[x] = self.parent[self.parent[x]]; // path halving
            x = self.parent[x];
        }
        x
    }

    fn union(&mut self, x: usize, y: usize) -> bool {
        let rx = self.find(x);
        let ry = self.find(y);
        if rx == ry {
            return false;
        }
        // Union by rank; break ties by smaller index (determinism).
        match self.rank[rx].cmp(&self.rank[ry]) {
            Ordering::Less => {
                self.parent[rx] = ry;
                self.size[ry] += self.size[rx];
            }
            Ordering::Greater => {
                self.parent[ry] = rx;
                self.size[rx] += self.size[ry];
            }
            Ordering::Equal => {
                // Deterministic tie-break: smaller index becomes root.
                if rx < ry {
                    self.parent[ry] = rx;
                    self.size[rx] += self.size[ry];
                    self.rank[rx] += 1;
                } else {
                    self.parent[rx] = ry;
                    self.size[ry] += self.size[rx];
                    self.rank[ry] += 1;
                }
            }
        }
        true
    }

    fn connected(&mut self, x: usize, y: usize) -> bool {
        self.find(x) == self.find(y)
    }
}

/// Compute the Euclidean distance between two 2D points.
fn euclidean_distance(p1: (f64, f64), p2: (f64, f64)) -> f64 {
    let dx = p1.0 - p2.0;
    let dy = p1.1 - p2.1;
    (dx * dx + dy * dy).sqrt()
}

/// Compute persistent homology for a set of 2D points.
///
/// Uses the Vietoris-Rips filtration:
/// - β₀ (dimension 0): Connected components tracked via union-find on edges.
/// - β₁ (dimension 1): Loops tracked by detecting triangles that close cycles.
///
/// # Arguments
/// * `points` - 2D coordinates of nodes.
/// * `max_filtration` - Maximum distance threshold. `None` = no limit.
///
/// # Returns
/// A `PersistenceDiagram` with birth/death pairs for all features.
pub fn compute_persistence(
    points: &[(f64, f64)],
    max_filtration: Option<f64>,
) -> PersistenceDiagram {
    let n = points.len();
    let mut diagram = PersistenceDiagram::new();

    if n == 0 {
        return diagram;
    }

    // All nodes are born at filtration 0 as isolated components.
    // We'll track their deaths below.

    // Build all pairwise edges with distances.
    let mut edges: Vec<FiltrationEdge> = Vec::with_capacity(n * (n - 1) / 2);
    for i in 0..n {
        for j in (i + 1)..n {
            let dist = euclidean_distance(points[i], points[j]);
            if max_filtration.is_none_or(|max| dist <= max) {
                edges.push(FiltrationEdge {
                    u: i,
                    v: j,
                    distance: dist,
                });
            }
        }
    }

    // Sort edges by distance (filtration order). Tie-break by (u, v) for determinism.
    edges.sort_by(|a, b| {
        a.distance
            .partial_cmp(&b.distance)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.u.cmp(&b.u))
            .then_with(|| a.v.cmp(&b.v))
    });

    // --- β₀: Connected components via union-find ---
    let mut uf = UnionFind::new(n);
    let mut component_birth: Vec<f64> = vec![0.0; n]; // All born at 0

    // Track which edges have been added (for loop detection).
    let mut added_edges: Vec<(usize, usize, f64)> = Vec::new();

    for edge in &edges {
        let ru = uf.find(edge.u);
        let rv = uf.find(edge.v);

        if ru != rv {
            // Merging two components: the younger one dies.
            // "Younger" = higher birth time. Since all born at 0, use the
            // elder rule: the component with the larger representative survives.
            let (survivor, dying) = if ru < rv { (ru, rv) } else { (rv, ru) };
            let death = edge.distance;
            let birth = component_birth[dying];

            // Only record if persistence > 0 (non-trivial).
            if death > birth {
                diagram.points.push(PersistencePoint {
                    birth,
                    death,
                    dimension: 0,
                });
            }

            uf.union(edge.u, edge.v);
            // Propagate the surviving component's birth to the new root.
            let new_root = uf.find(edge.u);
            component_birth[new_root] = component_birth[survivor];
        }

        added_edges.push((edge.u, edge.v, edge.distance));
    }

    // Record essential H₀ features (components that never die).
    // There's exactly one essential H₀ feature per connected component at the end.
    let mut seen_roots = std::collections::BTreeSet::new();
    for i in 0..n {
        let root = uf.find(i);
        if seen_roots.insert(root) {
            diagram.points.push(PersistencePoint {
                birth: 0.0,
                death: f64::INFINITY,
                dimension: 0,
            });
        }
    }

    // --- β₁: Loops via triangle detection ---
    // For each triangle (u, v, w) in the Vietoris-Rips complex, check if adding
    // its longest edge creates a cycle (i.e., u, v, w were already connected via
    // the shorter two edges). If so, the triangle "fills" the cycle at the
    // filtration value of its longest edge.

    // Build triangles from edge pairs sharing a vertex.
    if n >= 3 {
        // Adjacency: for each node, track which edges connect to it.
        let mut adjacency: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for &(u, v, dist) in &added_edges {
            adjacency[u].push((v, dist));
            adjacency[v].push((u, dist));
        }

        let mut triangles: Vec<FiltrationTriangle> = Vec::new();
        for &(u, v, d_uv) in &added_edges {
            // Find common neighbors of u and v.
            // Use the shorter adjacency list for efficiency.
            let (smaller, larger) = if adjacency[u].len() <= adjacency[v].len() {
                (u, v)
            } else {
                (v, u)
            };

            let larger_neighbors: std::collections::BTreeSet<usize> =
                adjacency[larger].iter().map(|&(n, _)| n).collect();

            for &(w, d_sw) in &adjacency[smaller] {
                if w <= u || w <= v {
                    continue; // Avoid duplicates: require u < v < w ordering check
                }
                if !larger_neighbors.contains(&w) {
                    continue;
                }

                // Found triangle (u, v, w) (or a permutation). Get all three edge distances.
                let d_lw = adjacency[larger]
                    .iter()
                    .find(|&&(n, _)| n == w)
                    .map(|&(_, d)| d)
                    .unwrap_or(f64::INFINITY);

                let filtration = d_uv.max(d_sw).max(d_lw);
                triangles.push(FiltrationTriangle {
                    u,
                    v,
                    w,
                    filtration,
                });
            }
        }

        // Sort triangles by filtration value.
        triangles.sort_by(|a, b| {
            a.filtration
                .partial_cmp(&b.filtration)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.u.cmp(&b.u))
                .then_with(|| a.v.cmp(&b.v))
                .then_with(|| a.w.cmp(&b.w))
        });

        // For H₁: a cycle is born when an edge connects two already-connected nodes,
        // and dies when a triangle fills that cycle.
        // We use a simplified approach: track cycles created by edges (non-tree edges
        // in the spanning forest), then kill them when triangles appear.

        let mut uf_cycle = UnionFind::new(n);
        let mut pending_cycles: Vec<(f64, usize, usize)> = Vec::new(); // (birth, u, v)

        for edge in &edges {
            if uf_cycle.connected(edge.u, edge.v) {
                // This edge creates a cycle — born at this filtration value.
                pending_cycles.push((edge.distance, edge.u, edge.v));
            } else {
                uf_cycle.union(edge.u, edge.v);
            }
        }

        // Match cycles with triangles that fill them.
        // Simple greedy matching: each triangle can kill one cycle.
        // NOTE: This correctly handles 3-cycles but may over-kill larger cycles
        // if a triangle shares two vertices with the non-tree edge. For layout
        // stability purposes this approximation is acceptable.
        let mut killed = vec![false; pending_cycles.len()];
        for tri in &triangles {
            let tri_nodes = [tri.u, tri.v, tri.w];
            // Find a pending cycle involving any two nodes of this triangle.
            for (ci, &(birth, cu, cv)) in pending_cycles.iter().enumerate() {
                if killed[ci] {
                    continue;
                }
                let cycle_nodes = [cu, cv];
                let matches = cycle_nodes.iter().all(|cn| tri_nodes.contains(cn));
                if matches && tri.filtration >= birth {
                    diagram.points.push(PersistencePoint {
                        birth,
                        death: tri.filtration,
                        dimension: 1,
                    });
                    killed[ci] = true;
                    break;
                }
            }
        }

        // Remaining unkilled cycles are essential H₁ features.
        for (ci, &(birth, _, _)) in pending_cycles.iter().enumerate() {
            if !killed[ci] {
                diagram.points.push(PersistencePoint {
                    birth,
                    death: f64::INFINITY,
                    dimension: 1,
                });
            }
        }
    }

    trace!(
        total_points = diagram.points.len(),
        h0_finite = diagram
            .dimension(0)
            .iter()
            .filter(|p| p.is_finite())
            .count(),
        h0_essential = diagram
            .dimension(0)
            .iter()
            .filter(|p| !p.is_finite())
            .count(),
        h1_finite = diagram
            .dimension(1)
            .iter()
            .filter(|p| p.is_finite())
            .count(),
        h1_essential = diagram
            .dimension(1)
            .iter()
            .filter(|p| !p.is_finite())
            .count(),
        "Persistence diagram computed"
    );

    diagram
}

/// Compute the bottleneck distance between two persistence diagrams.
///
/// The bottleneck distance is the maximum over all matched pairs of the L∞ distance
/// between their (birth, death) coordinates. Unmatched points are matched to the
/// diagonal (their persistence / 2).
///
/// This uses a greedy approximation for speed. For exact computation, a Hungarian
/// algorithm would be needed, but the greedy approach is sufficient for our
/// stability metric use case.
pub fn bottleneck_distance(d1: &PersistenceDiagram, d2: &PersistenceDiagram) -> f64 {
    bottleneck_distance_for_dimension(d1, d2, 0).max(bottleneck_distance_for_dimension(d1, d2, 1))
}

/// Compute the bottleneck distance for a specific dimension.
fn bottleneck_distance_for_dimension(
    d1: &PersistenceDiagram,
    d2: &PersistenceDiagram,
    dim: usize,
) -> f64 {
    let pts1: Vec<PersistencePoint> = d1
        .points
        .iter()
        .filter(|p| p.dimension == dim && p.is_finite())
        .copied()
        .collect();
    let pts2: Vec<PersistencePoint> = d2
        .points
        .iter()
        .filter(|p| p.dimension == dim && p.is_finite())
        .copied()
        .collect();

    if pts1.is_empty() && pts2.is_empty() {
        return 0.0;
    }

    // Greedy matching: match each point in the larger set to its nearest in the smaller.
    let (larger, smaller) = if pts1.len() >= pts2.len() {
        (&pts1, &pts2)
    } else {
        (&pts2, &pts1)
    };

    let mut max_dist = 0.0_f64;
    let mut used = vec![false; smaller.len()];

    for lp in larger {
        // Cost of matching to diagonal.
        let diag_cost = lp.persistence() / 2.0;

        // Find best match in smaller set.
        let mut best_idx = None;
        let mut best_cost = diag_cost;

        for (si, sp) in smaller.iter().enumerate() {
            if used[si] {
                continue;
            }
            let cost = (lp.birth - sp.birth).abs().max((lp.death - sp.death).abs());
            if cost < best_cost {
                best_cost = cost;
                best_idx = Some(si);
            }
        }

        if let Some(idx) = best_idx {
            used[idx] = true;
        }
        max_dist = max_dist.max(best_cost);
    }

    // Unmatched points in the smaller set are matched to the diagonal.
    for (si, sp) in smaller.iter().enumerate() {
        if !used[si] {
            max_dist = max_dist.max(sp.persistence() / 2.0);
        }
    }

    max_dist
}

/// Compute the p-Wasserstein distance between two persistence diagrams.
///
/// W_p(D₁, D₂) = (Σ cost(matching)^p)^{1/p}
///
/// Uses greedy matching (approximate). Set `p = 1` for L1 Wasserstein, `p = 2` for L2.
pub fn wasserstein_distance(d1: &PersistenceDiagram, d2: &PersistenceDiagram, p: f64) -> f64 {
    wasserstein_distance_for_dimension(d1, d2, 0, p)
        .max(wasserstein_distance_for_dimension(d1, d2, 1, p))
}

/// Compute the Wasserstein distance for a specific dimension.
fn wasserstein_distance_for_dimension(
    d1: &PersistenceDiagram,
    d2: &PersistenceDiagram,
    dim: usize,
    p: f64,
) -> f64 {
    let pts1: Vec<PersistencePoint> = d1
        .points
        .iter()
        .filter(|pt| pt.dimension == dim && pt.is_finite())
        .copied()
        .collect();
    let pts2: Vec<PersistencePoint> = d2
        .points
        .iter()
        .filter(|pt| pt.dimension == dim && pt.is_finite())
        .copied()
        .collect();

    if pts1.is_empty() && pts2.is_empty() {
        return 0.0;
    }

    let (larger, smaller) = if pts1.len() >= pts2.len() {
        (&pts1, &pts2)
    } else {
        (&pts2, &pts1)
    };

    let mut total_cost = 0.0_f64;
    let mut used = vec![false; smaller.len()];

    for lp in larger {
        let diag_cost = lp.persistence() / 2.0;
        let mut best_idx = None;
        let mut best_cost = diag_cost;

        for (si, sp) in smaller.iter().enumerate() {
            if used[si] {
                continue;
            }
            let cost = (lp.birth - sp.birth).abs().max((lp.death - sp.death).abs());
            if cost < best_cost {
                best_cost = cost;
                best_idx = Some(si);
            }
        }

        if let Some(idx) = best_idx {
            used[idx] = true;
        }
        total_cost += best_cost.powf(p);
    }

    for (si, sp) in smaller.iter().enumerate() {
        if !used[si] {
            total_cost += (sp.persistence() / 2.0).powf(p);
        }
    }

    total_cost.powf(1.0 / p)
}

/// Result of a layout stability comparison.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutStabilityResult {
    /// Persistence diagram before the edit.
    pub before: PersistenceDiagram,
    /// Persistence diagram after the edit.
    pub after: PersistenceDiagram,
    /// Bottleneck distance between the diagrams.
    pub bottleneck_distance: f64,
    /// 1-Wasserstein distance between the diagrams.
    pub wasserstein_1: f64,
    /// Whether the layout change is considered disruptive (exceeds threshold).
    pub disruptive: bool,
    /// Number of stable features destroyed by the edit.
    pub features_destroyed: usize,
    /// Number of stable features created by the edit.
    pub features_created: usize,
}

/// Configuration for layout stability analysis.
#[derive(Debug, Clone, Copy)]
pub struct StabilityConfig {
    /// Minimum persistence for a feature to be considered "stable".
    pub stable_threshold: f64,
    /// Maximum bottleneck distance before a change is considered disruptive.
    pub disruption_threshold: f64,
    /// Maximum filtration distance to consider. `None` = no limit.
    pub max_filtration: Option<f64>,
}

impl Default for StabilityConfig {
    fn default() -> Self {
        Self {
            stable_threshold: 50.0,
            disruption_threshold: 100.0,
            max_filtration: None,
        }
    }
}

/// Compare two layouts for topological stability.
///
/// Given node positions before and after an edit, computes persistence diagrams
/// for both and measures how much the topological structure changed.
pub fn compare_layout_stability(
    before_positions: &[(f64, f64)],
    after_positions: &[(f64, f64)],
    config: &StabilityConfig,
) -> LayoutStabilityResult {
    let before = compute_persistence(before_positions, config.max_filtration);
    let after = compute_persistence(after_positions, config.max_filtration);

    let bd = bottleneck_distance(&before, &after);
    let w1 = wasserstein_distance(&before, &after, 1.0);

    let stable_before = before.stable_features(config.stable_threshold);
    let stable_after = after.stable_features(config.stable_threshold);

    // Count features destroyed/created by comparing stable feature counts per dimension.
    let before_h0 = stable_before.iter().filter(|p| p.dimension == 0).count();
    let after_h0 = stable_after.iter().filter(|p| p.dimension == 0).count();
    let before_h1 = stable_before.iter().filter(|p| p.dimension == 1).count();
    let after_h1 = stable_after.iter().filter(|p| p.dimension == 1).count();

    let destroyed = before_h0.saturating_sub(after_h0) + before_h1.saturating_sub(after_h1);
    let created = after_h0.saturating_sub(before_h0) + after_h1.saturating_sub(before_h1);

    let disruptive = bd > config.disruption_threshold;

    debug!(
        bottleneck_distance = bd,
        wasserstein_1 = w1,
        features_destroyed = destroyed,
        features_created = created,
        disruptive,
        "Layout stability comparison"
    );

    LayoutStabilityResult {
        before,
        after,
        bottleneck_distance: bd,
        wasserstein_1: w1,
        disruptive,
        features_destroyed: destroyed,
        features_created: created,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_points_produce_empty_diagram() {
        let diagram = compute_persistence(&[], None);
        assert!(diagram.points.is_empty());
    }

    #[test]
    fn single_point_produces_one_essential_component() {
        let diagram = compute_persistence(&[(0.0, 0.0)], None);
        assert_eq!(diagram.points.len(), 1);
        assert_eq!(diagram.points[0].dimension, 0);
        assert!(!diagram.points[0].is_finite()); // essential
        assert_eq!(diagram.points[0].birth, 0.0);
    }

    #[test]
    fn two_points_produce_one_h0_death_and_one_essential() {
        let diagram = compute_persistence(&[(0.0, 0.0), (3.0, 4.0)], None);

        let h0 = diagram.dimension(0);
        // One finite H₀ (component merge) + one essential H₀.
        let finite_h0: Vec<_> = h0.iter().filter(|p| p.is_finite()).collect();
        let essential_h0: Vec<_> = h0.iter().filter(|p| !p.is_finite()).collect();

        assert_eq!(finite_h0.len(), 1);
        assert_eq!(essential_h0.len(), 1);

        // The two points are distance 5.0 apart.
        let merge = finite_h0[0];
        assert!((merge.death - 5.0).abs() < 1e-10);
        assert!((merge.birth - 0.0).abs() < 1e-10);
    }

    #[test]
    fn three_collinear_points_no_loops() {
        // Collinear points: no loops should form.
        let diagram = compute_persistence(&[(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)], None);

        let h1 = diagram.dimension(1);
        // With 3 collinear points, any cycle formed by edges is immediately
        // killed by a triangle, so H₁ features should be zero-persistence or absent.
        let stable_h1: Vec<_> = h1.iter().filter(|p| p.persistence() > 0.1).collect();
        // Collinear points form triangles that immediately fill any cycle.
        assert!(
            stable_h1.is_empty(),
            "Collinear points should not have stable loops, got {stable_h1:?}"
        );
    }

    #[test]
    fn square_points_produce_loop() {
        // Square with a hole in the middle: should detect H₁ feature.
        // Points at corners of unit square.
        let points = vec![(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)];
        let diagram = compute_persistence(&points, None);

        let h1 = diagram.dimension(1);
        // A square has 4 edges of length 1 and 2 diagonals of length √2 ≈ 1.414.
        // The cycle forms when 4 edges are added (at distance 1.0) but the diagonals
        // haven't been added yet. The cycle dies when the first diagonal (triangle) is added.
        assert!(
            !h1.is_empty(),
            "Square should produce at least one H₁ feature"
        );
    }

    #[test]
    fn persistence_ordering_by_lifetime() {
        // Two clusters at different scales: should have different persistence.
        let points = vec![
            // Tight cluster (small persistence when merging within)
            (0.0, 0.0),
            (0.1, 0.0),
            (0.0, 0.1),
            // Spread cluster (large persistence when merging within)
            (10.0, 0.0),
            (20.0, 0.0),
            (15.0, 8.66), // roughly equilateral triangle with side ~10
        ];

        let diagram = compute_persistence(&points, None);
        let h0_finite = diagram
            .dimension(0)
            .into_iter()
            .filter(|p| p.is_finite())
            .collect::<Vec<_>>();

        // Should have multiple H₀ deaths with varying persistence.
        assert!(
            h0_finite.len() >= 2,
            "Should have multiple component merges, got {}",
            h0_finite.len()
        );

        // The tight cluster merges should have lower persistence than
        // inter-cluster merges.
        let mut persistences: Vec<f64> = h0_finite.iter().map(|p| p.persistence()).collect();
        persistences.sort_by(|a, b| a.partial_cmp(b).unwrap());

        // At least some should be small (tight cluster) and some large (inter-cluster).
        assert!(
            persistences.first().unwrap() < persistences.last().unwrap(),
            "Persistence values should span a range"
        );
    }

    #[test]
    fn bottleneck_distance_identical_diagrams() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (0.5, 0.866)];
        let d1 = compute_persistence(&points, None);
        let d2 = d1.clone();

        let dist = bottleneck_distance(&d1, &d2);
        assert!(
            dist < 1e-10,
            "Bottleneck distance of identical diagrams should be 0, got {dist}"
        );
    }

    #[test]
    fn bottleneck_distance_different_layouts() {
        let before = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)];
        let after = vec![(0.0, 0.0), (1.0, 0.0), (100.0, 0.0)]; // node moved far

        let d1 = compute_persistence(&before, None);
        let d2 = compute_persistence(&after, None);

        let dist = bottleneck_distance(&d1, &d2);
        assert!(
            dist > 1.0,
            "Moving a node far should produce large bottleneck distance, got {dist}"
        );
    }

    #[test]
    fn wasserstein_distance_identical_is_zero() {
        let points = vec![(0.0, 0.0), (3.0, 4.0)];
        let d1 = compute_persistence(&points, None);
        let d2 = d1.clone();

        let dist = wasserstein_distance(&d1, &d2, 1.0);
        assert!(
            dist < 1e-10,
            "Wasserstein distance of identical diagrams should be 0, got {dist}"
        );
    }

    #[test]
    fn stability_comparison_small_edit_not_disruptive() {
        let before = vec![(0.0, 0.0), (10.0, 0.0), (5.0, 8.66)];
        let after = vec![(0.0, 0.0), (10.0, 0.0), (5.1, 8.66)]; // tiny move

        let config = StabilityConfig {
            stable_threshold: 1.0,
            disruption_threshold: 5.0,
            max_filtration: None,
        };
        let result = compare_layout_stability(&before, &after, &config);

        assert!(
            !result.disruptive,
            "Small edit should not be disruptive, bottleneck = {}",
            result.bottleneck_distance
        );
    }

    #[test]
    fn stability_comparison_large_edit_is_disruptive() {
        let before = vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0), (3.0, 0.0), (4.0, 0.0)];
        // Completely scrambled positions
        let after = vec![
            (100.0, 200.0),
            (50.0, 300.0),
            (200.0, 100.0),
            (0.0, 400.0),
            (300.0, 0.0),
        ];

        let config = StabilityConfig {
            stable_threshold: 1.0,
            disruption_threshold: 5.0,
            max_filtration: None,
        };
        let result = compare_layout_stability(&before, &after, &config);

        assert!(
            result.disruptive,
            "Large position scramble should be disruptive, bottleneck = {}",
            result.bottleneck_distance
        );
    }

    #[test]
    fn betti_numbers_single_component() {
        // Connected triangle: one component, no persistent loops.
        let diagram = compute_persistence(&[(0.0, 0.0), (1.0, 0.0), (0.5, 0.866)], None);
        let (b0, _b1) = diagram.betti_numbers();
        assert_eq!(b0, 1, "Connected graph should have β₀ = 1");
    }

    #[test]
    fn stable_features_filter() {
        let mut diagram = PersistenceDiagram::new();
        diagram.points.push(PersistencePoint {
            birth: 0.0,
            death: 0.5,
            dimension: 0,
        }); // low persistence
        diagram.points.push(PersistencePoint {
            birth: 0.0,
            death: 10.0,
            dimension: 0,
        }); // high persistence
        diagram.points.push(PersistencePoint {
            birth: 0.0,
            death: f64::INFINITY,
            dimension: 0,
        }); // essential

        let stable = diagram.stable_features(1.0);
        assert_eq!(stable.len(), 2); // high persistence + essential
    }

    #[test]
    fn max_filtration_limits_edges() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (100.0, 0.0)];

        // With max_filtration = 5.0, the far point shouldn't merge
        let diagram = compute_persistence(&points, Some(5.0));
        let (b0, _) = diagram.betti_numbers();
        assert_eq!(
            b0, 2,
            "With max_filtration=5, should have 2 components (far point isolated)"
        );
    }

    #[test]
    fn persistence_deterministic() {
        let points = vec![(0.0, 0.0), (1.0, 0.0), (0.5, 0.866), (2.0, 1.0), (3.0, 0.5)];

        let d1 = compute_persistence(&points, None);
        let d2 = compute_persistence(&points, None);

        assert_eq!(
            d1, d2,
            "Same input should produce identical persistence diagram"
        );
    }
}
