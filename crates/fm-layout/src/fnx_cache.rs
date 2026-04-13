//! Deterministic cache for FNX analysis results.
//!
//! This module provides hash-keyed memoization for fnx analysis to reduce
//! repeated work on identical inputs. Cache keys are computed from:
//! - Input graph structure (nodes, edges, topology)
//! - Analysis mode and configuration
//!
//! # Determinism Guarantees
//!
//! - Same input + config always produces same cache key (via FNV-1a hash)
//! - Cache invalidation is triggered by any config change
//! - All cached results are cloneable and immutable
//!
//! # Memory Management
//!
//! - Configurable maximum entry count
//! - LRU eviction when limit is reached
//! - Memory usage tracking via entry count

use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};

use fm_core::{IrEndpoint, MermaidDiagramIr};

use crate::fnx_adapter::ProjectionConfig;
use crate::fnx_cycle_scorer::CriticalityScoringResults;
use crate::fnx_diagnostics::FnxAnalysisResults;

// ============================================================================
// Cache Key
// ============================================================================

/// FNV-1a hash state for deterministic hashing.
struct FnvHasher {
    state: u64,
}

impl FnvHasher {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    fn new() -> Self {
        Self {
            state: Self::FNV_OFFSET_BASIS,
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

impl Hasher for FnvHasher {
    fn write(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u64::from(*byte);
            self.state = self.state.wrapping_mul(Self::FNV_PRIME);
        }
    }

    fn finish(&self) -> u64 {
        self.state
    }
}

/// Unique cache key computed from IR structure and analysis config.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CacheKey(u64);

impl CacheKey {
    /// Compute cache key from IR and projection config.
    #[must_use]
    pub fn from_ir_and_config(ir: &MermaidDiagramIr, config: &ProjectionConfig) -> Self {
        let mut hasher = FnvHasher::new();

        // Hash diagram type
        std::mem::discriminant(&ir.diagram_type).hash(&mut hasher);

        // Hash node count and IDs
        ir.nodes.len().hash(&mut hasher);
        for node in &ir.nodes {
            node.id.hash(&mut hasher);
        }

        // Hash edge topology (source, target pairs)
        ir.edges.len().hash(&mut hasher);
        for edge in &ir.edges {
            hash_endpoint(&edge.from, &mut hasher);
            hash_endpoint(&edge.to, &mut hasher);
        }

        // Hash config settings
        std::mem::discriminant(&config.directed_policy).hash(&mut hasher);
        config.penalty_factor.to_bits().hash(&mut hasher);
        config.preserve_self_loops.hash(&mut hasher);
        config.collapse_parallel_edges.hash(&mut hasher);

        Self(hasher.finish())
    }

    /// Get the raw hash value.
    #[must_use]
    pub const fn as_u64(&self) -> u64 {
        self.0
    }
}

fn hash_endpoint<H: Hasher>(endpoint: &IrEndpoint, hasher: &mut H) {
    match endpoint {
        IrEndpoint::Node(id) => {
            0u8.hash(hasher);
            id.0.hash(hasher);
        }
        IrEndpoint::Port(id) => {
            1u8.hash(hasher);
            id.0.hash(hasher);
        }
        IrEndpoint::Unresolved => {
            2u8.hash(hasher);
        }
    }
}

// ============================================================================
// Cached Results
// ============================================================================

/// Cached analysis results.
#[derive(Debug, Clone)]
pub struct CachedAnalysis {
    /// Structural diagnostics.
    pub diagnostics: Option<FnxAnalysisResults>,
    /// Edge criticality scores.
    pub criticality: Option<CriticalityScoringResults>,
    /// Timestamp (monotonic counter) when entry was last accessed.
    pub last_access: u64,
}

// ============================================================================
// Analysis Cache
// ============================================================================

/// Statistics for cache performance monitoring.
#[derive(Debug, Clone, Default)]
pub struct CacheStats {
    /// Total cache hits.
    pub hits: u64,
    /// Total cache misses.
    pub misses: u64,
    /// Total entries stored.
    pub entries: usize,
    /// Total evictions performed.
    pub evictions: u64,
}

impl CacheStats {
    /// Compute hit rate as a fraction (0.0 to 1.0).
    #[must_use]
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

/// Configuration for the analysis cache.
#[derive(Debug, Clone)]
pub struct CacheConfig {
    /// Maximum number of entries before eviction.
    pub max_entries: usize,
    /// Whether caching is enabled.
    pub enabled: bool,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            max_entries: 64,
            enabled: true,
        }
    }
}

/// Deterministic LRU cache for FNX analysis results.
#[derive(Debug)]
pub struct AnalysisCache {
    /// Cached entries keyed by deterministic hash.
    entries: BTreeMap<CacheKey, CachedAnalysis>,
    /// Monotonic access counter for LRU ordering.
    access_counter: AtomicU64,
    /// Cache configuration.
    config: CacheConfig,
    /// Performance statistics.
    stats: CacheStats,
}

impl Default for AnalysisCache {
    fn default() -> Self {
        Self::new(CacheConfig::default())
    }
}

impl AnalysisCache {
    /// Create a new cache with the given configuration.
    #[must_use]
    pub fn new(config: CacheConfig) -> Self {
        Self {
            entries: BTreeMap::new(),
            access_counter: AtomicU64::new(0),
            config,
            stats: CacheStats::default(),
        }
    }

    /// Check if caching is enabled.
    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get current statistics.
    #[must_use]
    pub fn stats(&self) -> &CacheStats {
        &self.stats
    }

    /// Look up cached diagnostics.
    pub fn get_diagnostics(
        &mut self,
        ir: &MermaidDiagramIr,
        config: &ProjectionConfig,
    ) -> Option<FnxAnalysisResults> {
        if !self.config.enabled {
            return None;
        }

        let key = CacheKey::from_ir_and_config(ir, config);
        if let Some(entry) = self.get_and_touch(key)
            && let Some(result) = entry.diagnostics.clone()
        {
            self.stats.hits += 1;
            return Some(result);
        }
        None
    }

    /// Look up cached criticality scores.
    pub fn get_criticality(
        &mut self,
        ir: &MermaidDiagramIr,
        config: &ProjectionConfig,
    ) -> Option<CriticalityScoringResults> {
        if !self.config.enabled {
            return None;
        }

        let key = CacheKey::from_ir_and_config(ir, config);
        if let Some(entry) = self.get_and_touch(key)
            && let Some(result) = entry.criticality.clone()
        {
            self.stats.hits += 1;
            return Some(result);
        }
        None
    }

    /// Store diagnostics in cache.
    pub fn put_diagnostics(
        &mut self,
        ir: &MermaidDiagramIr,
        config: &ProjectionConfig,
        diagnostics: FnxAnalysisResults,
    ) {
        if !self.config.enabled {
            return;
        }

        let key = CacheKey::from_ir_and_config(ir, config);
        self.stats.misses += 1;
        self.put(key, |entry| {
            entry.diagnostics = Some(diagnostics);
        });
    }

    /// Store criticality scores in cache.
    pub fn put_criticality(
        &mut self,
        ir: &MermaidDiagramIr,
        config: &ProjectionConfig,
        criticality: CriticalityScoringResults,
    ) {
        if !self.config.enabled {
            return;
        }

        let key = CacheKey::from_ir_and_config(ir, config);
        self.stats.misses += 1;
        self.put(key, |entry| {
            entry.criticality = Some(criticality);
        });
    }

    /// Clear all cached entries.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.stats.entries = 0;
    }

    /// Get entry and update access time.
    fn get_and_touch(&mut self, key: CacheKey) -> Option<&CachedAnalysis> {
        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_access = self.access_counter.fetch_add(1, Ordering::Relaxed);
            Some(entry)
        } else {
            None
        }
    }

    /// Insert or update entry, evicting LRU if needed.
    fn put<F>(&mut self, key: CacheKey, updater: F)
    where
        F: FnOnce(&mut CachedAnalysis),
    {
        // Evict if at capacity
        while self.entries.len() >= self.config.max_entries {
            self.evict_lru();
        }

        let access = self.access_counter.fetch_add(1, Ordering::Relaxed);
        let entry = self.entries.entry(key).or_insert_with(|| {
            self.stats.entries += 1;
            CachedAnalysis {
                diagnostics: None,
                criticality: None,
                last_access: access,
            }
        });
        entry.last_access = access;
        updater(entry);
    }

    /// Evict the least recently used entry.
    fn evict_lru(&mut self) {
        if self.entries.is_empty() {
            return;
        }

        // Find entry with lowest last_access
        let lru_key = self
            .entries
            .iter()
            .min_by_key(|(_, entry)| entry.last_access)
            .map(|(key, _)| *key);

        if let Some(key) = lru_key {
            self.entries.remove(&key);
            self.stats.evictions += 1;
            self.stats.entries = self.stats.entries.saturating_sub(1);
        }
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use fm_core::{DiagramType, IrEdge, IrNode, IrNodeId, NodeShape};

    fn make_test_ir(nodes: usize, edges: usize) -> MermaidDiagramIr {
        let nodes_vec: Vec<IrNode> = (0..nodes)
            .map(|i| IrNode {
                id: format!("N{i}"),
                shape: NodeShape::Rect,
                ..Default::default()
            })
            .collect();

        let edges_vec: Vec<IrEdge> = (0..edges)
            .map(|i| IrEdge {
                from: IrEndpoint::Node(IrNodeId(i % nodes)),
                to: IrEndpoint::Node(IrNodeId((i + 1) % nodes)),
                ..Default::default()
            })
            .collect();

        MermaidDiagramIr {
            diagram_type: DiagramType::Flowchart,
            nodes: nodes_vec,
            edges: edges_vec,
            ..Default::default()
        }
    }

    #[test]
    fn cache_key_deterministic() {
        let ir = make_test_ir(5, 4);
        let config = ProjectionConfig::default();

        let key1 = CacheKey::from_ir_and_config(&ir, &config);
        let key2 = CacheKey::from_ir_and_config(&ir, &config);

        assert_eq!(key1, key2, "same input should produce same key");
    }

    #[test]
    fn cache_key_differs_on_topology_change() {
        let ir1 = make_test_ir(5, 4);
        let ir2 = make_test_ir(5, 5); // extra edge
        let config = ProjectionConfig::default();

        let key1 = CacheKey::from_ir_and_config(&ir1, &config);
        let key2 = CacheKey::from_ir_and_config(&ir2, &config);

        assert_ne!(key1, key2, "different topology should produce different key");
    }

    #[test]
    fn cache_key_differs_on_config_change() {
        let ir = make_test_ir(5, 4);
        let config1 = ProjectionConfig::default();
        let mut config2 = ProjectionConfig::default();
        config2.penalty_factor = 0.75; // Different from default 0.5

        let key1 = CacheKey::from_ir_and_config(&ir, &config1);
        let key2 = CacheKey::from_ir_and_config(&ir, &config2);

        assert_ne!(key1, key2, "different config should produce different key");
    }

    #[test]
    fn cache_stores_and_retrieves() {
        let mut cache = AnalysisCache::default();
        let ir = make_test_ir(3, 2);
        let config = ProjectionConfig::default();

        // Initially empty
        assert!(cache.get_diagnostics(&ir, &config).is_none());

        // Store
        let results = FnxAnalysisResults::default();
        cache.put_diagnostics(&ir, &config, results.clone());

        // Retrieve
        let cached = cache.get_diagnostics(&ir, &config);
        assert!(cached.is_some());
    }

    #[test]
    fn cache_evicts_lru() {
        let config = CacheConfig {
            max_entries: 2,
            enabled: true,
        };
        let mut cache = AnalysisCache::new(config);
        let proj_config = ProjectionConfig::default();

        let ir1 = make_test_ir(1, 0);
        let ir2 = make_test_ir(2, 1);
        let ir3 = make_test_ir(3, 2);

        cache.put_diagnostics(&ir1, &proj_config, FnxAnalysisResults::default());
        cache.put_diagnostics(&ir2, &proj_config, FnxAnalysisResults::default());

        // Access ir1 to make it recently used
        let _ = cache.get_diagnostics(&ir1, &proj_config);

        // Add ir3 - should evict ir2 (least recently used)
        cache.put_diagnostics(&ir3, &proj_config, FnxAnalysisResults::default());

        assert!(cache.get_diagnostics(&ir1, &proj_config).is_some(), "ir1 should remain");
        assert!(cache.get_diagnostics(&ir2, &proj_config).is_none(), "ir2 should be evicted");
        assert!(cache.get_diagnostics(&ir3, &proj_config).is_some(), "ir3 should be present");
    }

    #[test]
    fn cache_stats_track_hits_misses() {
        let mut cache = AnalysisCache::default();
        let ir = make_test_ir(3, 2);
        let config = ProjectionConfig::default();

        // Miss on first access
        let _ = cache.get_diagnostics(&ir, &config);
        cache.put_diagnostics(&ir, &config, FnxAnalysisResults::default());

        // Hit on second access
        let _ = cache.get_diagnostics(&ir, &config);
        let _ = cache.get_diagnostics(&ir, &config);

        assert_eq!(cache.stats().misses, 1);
        assert_eq!(cache.stats().hits, 2);
        assert!(cache.stats().hit_rate() > 0.6);
    }

    #[test]
    fn disabled_cache_returns_none() {
        let config = CacheConfig {
            max_entries: 10,
            enabled: false,
        };
        let mut cache = AnalysisCache::new(config);
        let ir = make_test_ir(3, 2);
        let proj_config = ProjectionConfig::default();

        cache.put_diagnostics(&ir, &proj_config, FnxAnalysisResults::default());
        assert!(cache.get_diagnostics(&ir, &proj_config).is_none());
    }
}
