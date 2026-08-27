//! Graph-deck manifest builder (epic bd-z7g6k, bd-giulf).
//!
//! Turns `ir.deck` (raw slide definitions) plus the deterministic layout into a
//! [`DeckManifest`]: member sets, reveal steps, and camera rectangles in SVG viewBox space,
//! keyed by the same stable element ids the SVG renderer emits. Two pure phases with a
//! deliberate seam:
//!
//! 1. [`resolve_scenes`] — renderer-agnostic, LAYOUT space: selector resolution, edge policy,
//!    step assignment, tight bounds. Mechanically movable to fm-layout if the terminal or
//!    canvas players ever want it.
//! 2. [`project_manifest`] — viewBox projection through the shared [`svg_frame`] math, so the
//!    manifest and the rendered SVG cannot disagree about coordinates.
//!
//! Determinism is contractual (the manifest is golden-tested): `BTreeMap`/`BTreeSet`
//! collections, index-sorted output lists, two-decimal coordinate rounding, and no inputs
//! that vary by build feature.

use std::collections::{BTreeMap, BTreeSet};

use fm_core::{
    DECK_MANIFEST_SCHEMA_VERSION, DeckEdgeEndpoints, DeckManifest, DeckManifestCluster,
    DeckManifestEdge, DeckManifestNode, DeckManifestOptions, DeckManifestOverview,
    DeckManifestSlide, DeckManifestStep, DeckRect, IrDeck, IrDeckEdgePolicy, IrDeckReveal,
    MermaidDiagramIr, deck_manifest_supported, mermaid_cluster_element_id, mermaid_edge_element_id,
    mermaid_node_element_id,
};
use fm_layout::{DiagramLayout, LayoutRect};

use crate::{SvgBackend, SvgFrame, SvgRenderConfig, render_svg_with_layout, svg_frame};

/// One member node of a resolved scene, in layout space.
#[derive(Debug, Clone, PartialEq)]
struct SceneNode {
    ir_index: usize,
    source_id: String,
    rank: usize,
    step: usize,
    tooltip: Option<String>,
    bounds: LayoutRect,
}

/// One included edge of a resolved scene.
#[derive(Debug, Clone, PartialEq)]
struct SceneEdge {
    edge_index: usize,
    step: usize,
    touching: bool,
    /// Polyline points, kept only for the camera union (induced edges only).
    points: Vec<(f32, f32)>,
}

/// One included cluster of a resolved scene.
#[derive(Debug, Clone, PartialEq)]
struct SceneCluster {
    cluster_index: usize,
    step: usize,
    camera_contained: bool,
    bounds: LayoutRect,
}

/// A slide resolved against the diagram, still in layout space.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ResolvedScene {
    id: String,
    title: String,
    caption: String,
    fit_margin: f32,
    zoom_max: f32,
    nodes: Vec<SceneNode>,
    edges: Vec<SceneEdge>,
    clusters: Vec<SceneCluster>,
    /// Tight camera union: member node boxes ∪ fully-contained cluster boxes ∪ induced-edge
    /// points. Touching edges and partially-contained clusters render (half-dim) but never
    /// steer the camera — one distant touching endpoint must not zoom a slide to overview.
    bounds: LayoutRect,
    max_step: usize,
}

/// Wave size for the degenerate-rank auto-reveal fallback: when every member shares one rank
/// (force-directed layouts assign rank 0 to all nodes — ER and architecture families), members
/// chunk into waves of this many per step. One keypress ≈ one thought-group; tunable without
/// any schema change.
const AUTO_WAVE_SIZE: usize = 5;

/// Phase 1: resolve every slide against the diagram. `None` when the input can produce no
/// scenes at all: no deck, an unsupported family, or zero slides surviving resolution.
/// Unknown selectors and empty slides were already warned at parse time — resolution here is
/// silent about them by design (never warn twice).
pub(crate) fn resolve_scenes(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
) -> Option<Vec<ResolvedScene>> {
    let deck = ir.deck.as_deref()?;
    if !deck_manifest_supported(ir.diagram_type) || deck.slides.is_empty() {
        return None;
    }

    // Views built once per manifest.
    let node_index_by_id: BTreeMap<&str, usize> = ir
        .nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.id.as_str(), index))
        .collect();
    let mut subgraph_members: BTreeMap<&str, BTreeSet<usize>> = BTreeMap::new();
    for subgraph in &ir.graph.subgraphs {
        let members = ir
            .graph
            .subgraph_members_recursive(subgraph.id)
            .into_iter()
            .map(|node_id| node_id.0)
            .filter(|index| *index < ir.nodes.len());
        subgraph_members
            .entry(subgraph.key.as_str())
            .or_default()
            .extend(members);
    }
    // Only nodes with layout geometry can appear on a slide — a member the layout never
    // placed cannot be framed or dimmed.
    let layout_box_by_node: BTreeMap<usize, &fm_layout::LayoutNodeBox> = layout
        .nodes
        .iter()
        .map(|node_box| (node_box.node_index, node_box))
        .collect();

    let resolve_selector = |selector: &str| -> BTreeSet<usize> {
        if selector == "*" {
            return (0..ir.nodes.len()).collect();
        }
        if let Some(key) = selector.strip_prefix("subgraph:") {
            return subgraph_members.get(key).cloned().unwrap_or_default();
        }
        node_index_by_id
            .get(selector)
            .map(|index| BTreeSet::from([*index]))
            .unwrap_or_default()
    };

    let mut scenes = Vec::with_capacity(deck.slides.len());
    for slide in &deck.slides {
        let mut members: BTreeSet<usize> = BTreeSet::new();
        for selector in &slide.nodes {
            members.extend(resolve_selector(selector));
        }
        members.retain(|index| layout_box_by_node.contains_key(index));
        if members.is_empty() {
            continue; // warned at parse time; silently dropped here
        }

        // Node reveal steps (raw), then dense-renumbered so steps 1..=max are all non-empty
        // even when an authored group matched nothing.
        let raw_steps: BTreeMap<usize, usize> = match &slide.reveal {
            IrDeckReveal::None => members.iter().map(|index| (*index, 0)).collect(),
            IrDeckReveal::Groups(groups) => {
                let mut assigned: BTreeMap<usize, usize> = BTreeMap::new();
                for (group_number, group) in groups.iter().enumerate() {
                    let step = group_number + 1;
                    for selector in group {
                        for index in resolve_selector(selector).intersection(&members) {
                            // Lowest group wins (warned at parse time on overlap).
                            assigned.entry(*index).or_insert(step);
                        }
                    }
                }
                members
                    .iter()
                    .map(|index| (*index, assigned.get(index).copied().unwrap_or(0)))
                    .collect()
            }
            IrDeckReveal::Auto => {
                let mut ordered: Vec<(usize, usize)> = members
                    .iter()
                    .map(|index| (layout_box_by_node[index].rank, *index))
                    .collect();
                ordered.sort_unstable();
                let distinct_ranks: BTreeSet<usize> =
                    ordered.iter().map(|(rank, _)| *rank).collect();
                if distinct_ranks.len() >= 2 {
                    // Rank waves: the first rank is step 0, each later rank one keypress.
                    let step_of_rank: BTreeMap<usize, usize> = distinct_ranks
                        .iter()
                        .enumerate()
                        .map(|(position, rank)| (*rank, position))
                        .collect();
                    ordered
                        .into_iter()
                        .map(|(rank, index)| (index, step_of_rank[&rank]))
                        .collect()
                } else {
                    // Degenerate ranks (force layout hardcodes 0 for every node): chunk the
                    // (rank, node_index)-sorted members into AUTO_WAVE_SIZE waves.
                    ordered
                        .into_iter()
                        .enumerate()
                        .map(|(position, (_, index))| (index, position / AUTO_WAVE_SIZE))
                        .collect()
                }
            }
        };
        let node_steps = dense_renumber(&raw_steps);
        let max_step = node_steps.values().copied().max().unwrap_or(0);

        // Edges by policy. Port-rooted endpoints (ER attributes, class members) resolve to
        // their parent node; self-loops are induced iff their single endpoint is a member.
        let mut edges = Vec::new();
        if slide.edges != IrDeckEdgePolicy::None {
            for edge_path in &layout.edges {
                let Some(edge) = ir.edges.get(edge_path.edge_index) else {
                    continue;
                };
                let from = ir.resolve_endpoint_node(edge.from).map(|id| id.0);
                let to = ir.resolve_endpoint_node(edge.to).map(|id| id.0);
                let from_member = from.is_some_and(|index| members.contains(&index));
                let to_member = to.is_some_and(|index| members.contains(&index));
                let include = match slide.edges {
                    IrDeckEdgePolicy::Induced => from_member && to_member,
                    IrDeckEdgePolicy::Touching => from_member || to_member,
                    IrDeckEdgePolicy::None => false,
                };
                if !include {
                    continue;
                }
                let touching = !(from_member && to_member);
                // Edge step = max of its member endpoints' steps (the on-slide endpoint for
                // touching edges).
                let step = [from, to]
                    .into_iter()
                    .flatten()
                    .filter_map(|index| node_steps.get(&index).copied())
                    .max()
                    .unwrap_or(0);
                edges.push(SceneEdge {
                    edge_index: edge_path.edge_index,
                    step,
                    touching,
                    points: edge_path
                        .points
                        .iter()
                        .map(|point| (point.x, point.y))
                        .collect(),
                });
            }
            edges.sort_by_key(|edge| edge.edge_index);
        }

        // Clusters: included with >=1 member on the slide (the chrome anchors a partially
        // shown region); only FULLY contained ones steer the camera. Step = min of in-slide
        // member steps, so a cluster box appears with its first member.
        let mut clusters = Vec::new();
        for cluster_box in &layout.clusters {
            let Some(cluster) = ir.clusters.get(cluster_box.cluster_index) else {
                continue;
            };
            let member_indexes: Vec<usize> =
                cluster.members.iter().map(|node_id| node_id.0).collect();
            if member_indexes.is_empty() {
                continue;
            }
            let on_slide: Vec<usize> = member_indexes
                .iter()
                .copied()
                .filter(|index| members.contains(index))
                .collect();
            if on_slide.is_empty() {
                continue;
            }
            let camera_contained = member_indexes
                .iter()
                .all(|index| members.contains(index) || !layout_box_by_node.contains_key(index));
            let step = on_slide
                .iter()
                .filter_map(|index| node_steps.get(index).copied())
                .min()
                .unwrap_or(0);
            clusters.push(SceneCluster {
                cluster_index: cluster_box.cluster_index,
                step,
                camera_contained,
                bounds: cluster_box.bounds,
            });
        }
        clusters.sort_by_key(|cluster| cluster.cluster_index);

        // Tight camera union.
        let mut union = BoundsUnion::default();
        for index in &members {
            union.add_rect(layout_box_by_node[index].bounds);
        }
        for cluster in clusters.iter().filter(|cluster| cluster.camera_contained) {
            union.add_rect(cluster.bounds);
        }
        for edge in edges.iter().filter(|edge| !edge.touching) {
            for (x, y) in &edge.points {
                union.add_point(*x, *y);
            }
        }
        let Some(bounds) = union.finish() else {
            continue;
        };

        let nodes = members
            .iter()
            .map(|index| {
                let node = &ir.nodes[*index];
                let tooltip = deck
                    .tips
                    .get(&node.id)
                    .cloned()
                    .or_else(|| node.tooltip().map(str::to_string));
                SceneNode {
                    ir_index: *index,
                    source_id: node.id.clone(),
                    rank: layout_box_by_node[index].rank,
                    step: node_steps.get(index).copied().unwrap_or(0),
                    tooltip,
                    bounds: layout_box_by_node[index].bounds,
                }
            })
            .collect();

        scenes.push(ResolvedScene {
            id: slide.id.clone(),
            title: slide.title.clone(),
            caption: slide.caption.clone(),
            fit_margin: slide.fit_margin.unwrap_or(deck.options.fit_margin),
            zoom_max: slide.zoom_max.unwrap_or(deck.options.zoom_max),
            nodes,
            edges,
            clusters,
            bounds,
            max_step,
        });
    }

    if scenes.is_empty() {
        None
    } else {
        Some(scenes)
    }
}

/// Dense-renumber raw step assignments so steps 1..=max are contiguous and each is held by at
/// least one node (an authored reveal group that matched nothing must not leave a hole the
/// runtime would advance through invisibly).
fn dense_renumber(raw: &BTreeMap<usize, usize>) -> BTreeMap<usize, usize> {
    let used: BTreeSet<usize> = raw.values().copied().filter(|step| *step > 0).collect();
    let remap: BTreeMap<usize, usize> = used
        .iter()
        .enumerate()
        .map(|(position, step)| (*step, position + 1))
        .collect();
    raw.iter()
        .map(|(index, step)| (*index, if *step == 0 { 0 } else { remap[step] }))
        .collect()
}

/// Accumulates a tight bounding rectangle over rects and points.
#[derive(Debug, Default)]
struct BoundsUnion {
    min_x: Option<f32>,
    min_y: Option<f32>,
    max_x: Option<f32>,
    max_y: Option<f32>,
}

impl BoundsUnion {
    fn add_point(&mut self, x: f32, y: f32) {
        self.min_x = Some(self.min_x.map_or(x, |current| current.min(x)));
        self.min_y = Some(self.min_y.map_or(y, |current| current.min(y)));
        self.max_x = Some(self.max_x.map_or(x, |current| current.max(x)));
        self.max_y = Some(self.max_y.map_or(y, |current| current.max(y)));
    }

    fn add_rect(&mut self, rect: LayoutRect) {
        self.add_point(rect.x, rect.y);
        self.add_point(rect.x + rect.width, rect.y + rect.height);
    }

    fn finish(self) -> Option<LayoutRect> {
        Some(LayoutRect {
            x: self.min_x?,
            y: self.min_y?,
            width: self.max_x? - self.min_x?,
            height: self.max_y? - self.min_y?,
        })
    }
}

/// Round to two decimals — the manifest's coordinate precision (mirrors the layout-checksum
/// canonicalization idea; shorter JSON matters for the embedded showcase/CLI payloads, and
/// fixed precision keeps `serde_json` float formatting bit-stable across toolchains).
fn round2(value: f32) -> f32 {
    (value * 100.0).round() / 100.0
}

/// Phase 2: project resolved scenes into viewBox space and assemble the serde manifest.
pub(crate) fn project_manifest(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    scenes: Vec<ResolvedScene>,
    frame: &SvgFrame,
) -> DeckManifest {
    let deck = ir.deck.as_deref().cloned().unwrap_or_else(IrDeck::default);
    let project_rect = |rect: LayoutRect| DeckRect {
        x: round2(rect.x + frame.offset_x),
        y: round2(rect.y + frame.offset_y),
        width: round2(rect.width),
        height: round2(rect.height),
    };

    // Whole-diagram geometry joins (schema 1.1.0): a morphing runtime animates EVERY node —
    // off-slide members glide out past the camera window — and redraws each edge between its
    // endpoints' live positions, so it needs home rects and endpoint ids for the full graph,
    // not just slide members.
    let mut node_geometry: BTreeMap<String, DeckRect> = BTreeMap::new();
    let mut element_id_by_node: BTreeMap<usize, String> = BTreeMap::new();
    for node_box in &layout.nodes {
        let Some(node) = ir.nodes.get(node_box.node_index) else {
            continue;
        };
        let element_id = mermaid_node_element_id(&node.id, node_box.node_index);
        node_geometry.insert(element_id.clone(), project_rect(node_box.bounds));
        element_id_by_node.insert(node_box.node_index, element_id);
    }
    let mut edge_endpoints: BTreeMap<String, DeckEdgeEndpoints> = BTreeMap::new();
    for (edge_index, edge) in ir.edges.iter().enumerate() {
        let endpoint = |end| {
            ir.resolve_endpoint_node(end)
                .and_then(|node_id| element_id_by_node.get(&node_id.0))
        };
        if let (Some(from), Some(to)) = (endpoint(edge.from), endpoint(edge.to)) {
            edge_endpoints.insert(
                mermaid_edge_element_id(edge_index),
                DeckEdgeEndpoints {
                    from_element_id: from.clone(),
                    to_element_id: to.clone(),
                },
            );
        }
    }

    let mut node_slide_index: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut slides = Vec::with_capacity(scenes.len());
    for scene in scenes {
        let mut manifest_nodes = Vec::with_capacity(scene.nodes.len());
        for node in &scene.nodes {
            let element_id = mermaid_node_element_id(&node.source_id, node.ir_index);
            node_slide_index
                .entry(element_id.clone())
                .or_default()
                .push(scene.id.clone());
            manifest_nodes.push(DeckManifestNode {
                index: node.ir_index,
                source_id: node.source_id.clone(),
                element_id,
                step: node.step,
                tooltip: node.tooltip.clone(),
            });
        }
        let manifest_edges: Vec<DeckManifestEdge> = scene
            .edges
            .iter()
            .map(|edge| DeckManifestEdge {
                index: edge.edge_index,
                element_id: mermaid_edge_element_id(edge.edge_index),
                step: edge.step,
                touching: edge.touching,
            })
            .collect();
        let manifest_clusters: Vec<DeckManifestCluster> = scene
            .clusters
            .iter()
            .map(|cluster| DeckManifestCluster {
                index: cluster.cluster_index,
                element_id: mermaid_cluster_element_id(cluster.cluster_index),
                step: cluster.step,
                camera_contained: cluster.camera_contained,
            })
            .collect();

        // Per-step reveal lists in the exact stagger order runtimes replay verbatim:
        // nodes by (rank, node index), then edges by index, then clusters by index.
        let mut steps = Vec::with_capacity(scene.max_step);
        for step in 1..=scene.max_step {
            let mut element_ids = Vec::new();
            let mut step_nodes: Vec<&SceneNode> = scene
                .nodes
                .iter()
                .filter(|node| node.step == step)
                .collect();
            step_nodes.sort_by_key(|node| (node.rank, node.ir_index));
            element_ids.extend(
                step_nodes
                    .iter()
                    .map(|node| mermaid_node_element_id(&node.source_id, node.ir_index)),
            );
            element_ids.extend(
                scene
                    .edges
                    .iter()
                    .filter(|edge| edge.step == step)
                    .map(|edge| mermaid_edge_element_id(edge.edge_index)),
            );
            element_ids.extend(
                scene
                    .clusters
                    .iter()
                    .filter(|cluster| cluster.step == step)
                    .map(|cluster| mermaid_cluster_element_id(cluster.cluster_index)),
            );
            steps.push(DeckManifestStep { step, element_ids });
        }

        slides.push(DeckManifestSlide {
            id: scene.id,
            title: scene.title,
            caption: scene.caption,
            bounds: project_rect(scene.bounds),
            fit_margin: round2(scene.fit_margin),
            zoom_max: round2(scene.zoom_max),
            nodes: manifest_nodes,
            edges: manifest_edges,
            clusters: manifest_clusters,
            max_step: scene.max_step,
            steps,
        });
    }

    DeckManifest {
        schema_version: DECK_MANIFEST_SCHEMA_VERSION.to_string(),
        generator: "frankenmermaid".to_string(),
        diagram_type: ir.diagram_type,
        title: deck.title.clone(),
        view_box: DeckRect {
            x: 0.0,
            y: 0.0,
            width: round2(frame.viewbox_width),
            height: round2(frame.viewbox_height),
        },
        options: DeckManifestOptions {
            fit_margin: round2(deck.options.fit_margin),
            zoom_max: round2(deck.options.zoom_max),
            dim_opacity: round2(deck.options.dim_opacity),
            auto_advance_ms: deck.options.auto_advance_ms,
        },
        slides,
        overview: DeckManifestOverview {
            enabled: deck.overview.enabled,
            title: deck.overview.title.clone(),
            caption: deck.overview.caption.clone(),
            tour: deck.overview.tour,
        },
        node_slide_index,
        node_geometry,
        edge_endpoints,
    }
}

/// Build the deck manifest for a diagram, or `None` when there is nothing to build: no deck
/// directive, an unsupported diagram family (no per-node SVG element ids), zero slides that
/// resolve to members, or the Scene backend (whose frame math this module does not model).
#[must_use]
pub fn deck_manifest(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
) -> Option<DeckManifest> {
    if config.backend == SvgBackend::Scene {
        return None;
    }
    let scenes = resolve_scenes(ir, layout)?;
    let frame = svg_frame(ir, layout, config);
    Some(project_manifest(ir, layout, scenes, &frame))
}

/// Render SVG and manifest from one shared layout and one shared frame computation — the
/// pairing that makes SVG/manifest drift structurally impossible. With `backend == Scene` or
/// any other manifest gate, the SVG still renders and the manifest is `None`.
#[must_use]
pub fn render_svg_with_deck(
    ir: &MermaidDiagramIr,
    layout: &DiagramLayout,
    config: &SvgRenderConfig,
) -> (String, Option<DeckManifest>) {
    let svg = render_svg_with_layout(ir, layout, config);
    let manifest = deck_manifest(ir, layout, config);
    (svg, manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fm_layout::layout_diagram;

    fn manifest_for(source: &str) -> Option<DeckManifest> {
        let parsed = fm_parser::parse(source);
        let layout = layout_diagram(&parsed.ir);
        deck_manifest(&parsed.ir, &layout, &SvgRenderConfig::default())
    }

    fn decked_flowchart() -> &'static str {
        "flowchart LR\n%%{deck: {\n  title: 'Tour',\n  tips: { a: 'the start' },\n  slides: [\n    { id: 's1', title: 'Start', nodes: ['a', 'b'], reveal: [['b']] },\n    { id: 's2', nodes: ['subgraph:core'], edges: 'touching' },\n  ],\n}}%%\n  a --> b\n  subgraph core\n    b --> c\n    c --> d\n  end\n"
    }

    /// Schema 1.1.0 morphing joins: every laid-out node has home geometry inside the SVG
    /// viewBox, and every edge's endpoints name node element ids that geometry covers — the
    /// exact invariants the morphing runtime relies on to fly nodes and re-anchor edges.
    #[test]
    fn geometry_and_endpoint_joins_cover_the_whole_graph() {
        let parsed = fm_parser::parse(decked_flowchart());
        let layout = layout_diagram(&parsed.ir);
        let manifest =
            deck_manifest(&parsed.ir, &layout, &SvgRenderConfig::default()).expect("manifest");

        assert_eq!(
            manifest.node_geometry.len(),
            layout.nodes.len(),
            "every laid-out node carries home geometry"
        );
        for (element_id, rect) in &manifest.node_geometry {
            assert!(element_id.starts_with("fm-node-"));
            assert!(rect.width > 0.0 && rect.height > 0.0);
            assert!(
                rect.x >= 0.0
                    && rect.y >= 0.0
                    && rect.x + rect.width <= manifest.view_box.width
                    && rect.y + rect.height <= manifest.view_box.height,
                "node rect {element_id} escapes the viewBox"
            );
        }
        assert_eq!(
            manifest.edge_endpoints.len(),
            parsed.ir.edges.len(),
            "every IR edge in this fixture has two laid-out endpoints"
        );
        for (element_id, endpoints) in &manifest.edge_endpoints {
            assert!(element_id.starts_with("fm-edge-"));
            assert!(
                manifest
                    .node_geometry
                    .contains_key(&endpoints.from_element_id)
                    && manifest
                        .node_geometry
                        .contains_key(&endpoints.to_element_id),
                "edge {element_id} references a node without geometry"
            );
        }
        // Slide members are a subset of the geometry map — the runtime can always find a
        // member's home rect.
        for slide in &manifest.slides {
            for node in &slide.nodes {
                assert!(manifest.node_geometry.contains_key(&node.element_id));
            }
        }
    }

    #[test]
    fn manifest_gates_return_none() {
        // Deckless.
        assert!(manifest_for("flowchart LR\n  a --> b\n").is_none());
        // Unsupported family (pie has no per-node element ids).
        assert!(
            manifest_for(
                "pie\n%%{deck: {slides: [{id: 's', nodes: ['*']}]}}%%\n  \"A\": 10\n  \"B\": 20\n"
            )
            .is_none()
        );
        // All slides resolve empty.
        assert!(
            manifest_for(
                "flowchart LR\n%%{deck: {slides: [{id: 's', nodes: ['ghost']}]}}%%\n  a --> b\n"
            )
            .is_none()
        );
        // Scene backend renders SVG but produces no manifest.
        let parsed = fm_parser::parse(decked_flowchart());
        let layout = layout_diagram(&parsed.ir);
        let config = SvgRenderConfig {
            backend: SvgBackend::Scene,
            ..SvgRenderConfig::default()
        };
        let (svg, manifest) = render_svg_with_deck(&parsed.ir, &layout, &config);
        assert!(svg.starts_with("<svg"));
        assert!(manifest.is_none());
    }

    #[test]
    fn membership_steps_and_join_keys_resolve() {
        let manifest = manifest_for(decked_flowchart()).expect("manifest");
        assert_eq!(manifest.schema_version, DECK_MANIFEST_SCHEMA_VERSION);
        assert_eq!(manifest.title.as_deref(), Some("Tour"));
        assert_eq!(manifest.slides.len(), 2);

        let first = &manifest.slides[0];
        assert_eq!(first.id, "s1");
        let ids: Vec<&str> = first
            .nodes
            .iter()
            .map(|node| node.source_id.as_str())
            .collect();
        assert_eq!(ids, ["a", "b"]);
        // Authored reveal: 'b' at step 1, 'a' at step 0; the a->b edge follows its later
        // endpoint; tooltip merged from deck tips.
        assert_eq!(first.max_step, 1);
        let node_a = &first.nodes[0];
        assert_eq!(node_a.step, 0);
        assert_eq!(node_a.tooltip.as_deref(), Some("the start"));
        let node_b = &first.nodes[1];
        assert_eq!(node_b.step, 1);
        assert_eq!(first.edges.len(), 1);
        assert_eq!(first.edges[0].step, 1);
        assert!(!first.edges[0].touching);
        assert_eq!(first.steps.len(), 1);
        // The `core` cluster contains `b`, whose step is 1 — the cluster box appears with its
        // first member (min-member-step rule), after nodes and edges in the stagger order.
        assert_eq!(first.clusters.len(), 1);
        assert_eq!(first.clusters[0].step, 1);
        assert!(
            !first.clusters[0].camera_contained,
            "only b of {{b,c,d}} is on the slide"
        );
        assert_eq!(
            first.steps[0].element_ids,
            vec![
                node_b.element_id.clone(),
                first.edges[0].element_id.clone(),
                first.clusters[0].element_id.clone(),
            ],
            "stagger order: nodes, then edges, then clusters"
        );

        // Subgraph selector expands to members; touching policy includes the a->b boundary
        // edge... no — a is off-slide and b is on-slide, so b's edges to a are touching.
        let second = &manifest.slides[1];
        let ids: Vec<&str> = second
            .nodes
            .iter()
            .map(|node| node.source_id.as_str())
            .collect();
        assert_eq!(ids, ["b", "c", "d"]);
        assert!(
            second.edges.iter().any(|edge| edge.touching),
            "the a->b edge touches the slide"
        );
        assert!(
            second
                .clusters
                .iter()
                .any(|cluster| cluster.camera_contained),
            "the core subgraph is fully contained"
        );

        // Join-key contract + travel index.
        for slide in &manifest.slides {
            for node in &slide.nodes {
                assert_eq!(
                    node.element_id,
                    mermaid_node_element_id(&node.source_id, node.index)
                );
                assert!(
                    manifest.node_slide_index[&node.element_id].contains(&slide.id),
                    "nodeSlideIndex must map {} to {}",
                    node.element_id,
                    slide.id
                );
            }
        }
    }

    #[test]
    fn camera_bounds_exclude_touching_edges_and_partial_clusters() {
        // Slide selects ONE node inside a large subgraph, with a touching edge whose far
        // endpoint (z) sits diagram-distant. Neither the subgraph box nor the touching edge
        // may steer the camera.
        let source = "flowchart LR\n%%{deck: {slides: [{id: 'tight', nodes: ['m1'], edges: 'touching'}]}}%%\n  subgraph big\n    m1 --> m2\n    m2 --> m3\n    m3 --> m4\n    m4 --> m5\n  end\n  m5 --> z\n  z --> m1\n";
        let parsed = fm_parser::parse(source);
        let layout = layout_diagram(&parsed.ir);
        let config = SvgRenderConfig::default();
        let manifest = deck_manifest(&parsed.ir, &layout, &config).expect("manifest");
        let slide = &manifest.slides[0];
        assert_eq!(slide.nodes.len(), 1);
        assert!(slide.edges.iter().all(|edge| edge.touching));
        assert!(slide.clusters.iter().all(|c| !c.camera_contained));

        // The tight bounds must equal m1's own box (projected), not the whole subgraph.
        let frame = svg_frame(&parsed.ir, &layout, &config);
        let m1_index = slide.nodes[0].index;
        let m1_box = layout
            .nodes
            .iter()
            .find(|node_box| node_box.node_index == m1_index)
            .expect("m1 layout box");
        assert!((slide.bounds.width - m1_box.bounds.width).abs() < 0.01);
        assert!((slide.bounds.height - m1_box.bounds.height).abs() < 0.01);
        assert!((slide.bounds.x - (m1_box.bounds.x + frame.offset_x)).abs() < 0.01);
        // And it must sit inside the viewBox.
        assert!(slide.bounds.x >= manifest.view_box.x - 0.01);
        assert!(
            slide.bounds.x + slide.bounds.width
                <= manifest.view_box.x + manifest.view_box.width + 0.01
        );
    }

    #[test]
    fn auto_reveal_groups_by_rank_with_wave_fallback() {
        // Layered flowchart: distinct ranks become steps (first rank = step 0).
        let manifest = manifest_for(
            "flowchart LR\n%%{deck: {slides: [{id: 's', nodes: ['*'], reveal: 'auto'}]}}%%\n  a --> b\n  b --> c\n  c --> d\n",
        )
        .expect("manifest");
        let slide = &manifest.slides[0];
        assert_eq!(slide.max_step, 3, "four ranks → steps 0..=3");
        let step_of = |id: &str| {
            slide
                .nodes
                .iter()
                .find(|node| node.source_id == id)
                .map(|node| node.step)
                .expect("node present")
        };
        assert_eq!(step_of("a"), 0);
        assert_eq!(step_of("b"), 1);
        assert_eq!(step_of("d"), 3);
        // Every step list non-empty.
        assert_eq!(slide.steps.len(), 3);
        assert!(slide.steps.iter().all(|step| !step.element_ids.is_empty()));

        // Degenerate ranks (an ER diagram lays out force-directed with rank 0 everywhere):
        // wave fallback chunks members instead of collapsing to a single step-0 wave.
        let er = manifest_for(
            "erDiagram\n%%{deck: {slides: [{id: 's', nodes: ['*'], reveal: 'auto'}]}}%%\n  A ||--o{ B : has\n  A ||--o{ C : has\n  B ||--o{ D : has\n  C ||--o{ E : has\n  D ||--o{ F : has\n  E ||--o{ G : has\n",
        )
        .expect("er manifest");
        let slide = &er.slides[0];
        let distinct_steps: BTreeSet<usize> = slide.nodes.iter().map(|node| node.step).collect();
        assert!(
            distinct_steps.len() >= 2,
            "wave fallback must produce reveals on a single-rank layout: {distinct_steps:?}"
        );
        assert!(slide.steps.iter().all(|step| !step.element_ids.is_empty()));
    }

    #[test]
    fn authored_group_holes_are_densely_renumbered() {
        // Group 1 matches nothing ('ghost'); group 2 matches 'b'. Steps must compact to
        // max_step == 1 with no empty step list.
        let manifest = manifest_for(
            "flowchart LR\n%%{deck: {slides: [{id: 's', nodes: ['a', 'b'], reveal: [['ghost'], ['b']]}]}}%%\n  a --> b\n",
        )
        .expect("manifest");
        let slide = &manifest.slides[0];
        assert_eq!(slide.max_step, 1);
        assert_eq!(slide.steps.len(), 1);
        assert!(!slide.steps[0].element_ids.is_empty());
    }

    #[test]
    fn manifest_is_bit_identical_across_runs() {
        let parsed = fm_parser::parse(decked_flowchart());
        let layout = layout_diagram(&parsed.ir);
        let config = SvgRenderConfig::default();
        let first =
            serde_json::to_string(&deck_manifest(&parsed.ir, &layout, &config).expect("manifest"))
                .expect("serialize");
        let second =
            serde_json::to_string(&deck_manifest(&parsed.ir, &layout, &config).expect("manifest"))
                .expect("serialize");
        assert_eq!(first, second);
    }

    /// THE SUPPORTED-FAMILY CONTRACT (plan I.5): `deck_manifest_supported` may list a family
    /// only if its rendered SVG really carries per-node `fm-node-*` element ids — otherwise a
    /// manifest would reference elements that do not exist. One fixture per claimed family.
    #[test]
    fn supported_families_emit_per_node_element_ids() {
        let fixtures: &[(&str, &str)] = &[
            ("flowchart", "flowchart LR\n  a --> b\n"),
            ("class", "classDiagram\n  class Alpha\n  Alpha <|-- Beta\n"),
            (
                "state",
                "stateDiagram-v2\n  [*] --> Idle\n  Idle --> Busy\n",
            ),
            ("er", "erDiagram\n  A ||--o{ B : has\n"),
            (
                "c4",
                "C4Context\n  Person(user, \"User\")\n  System(sys, \"System\")\n",
            ),
            (
                "architecture",
                "architecture-beta\n  service api(cloud)[API]\n  service db(database)[DB]\n  api:R -- L:db\n",
            ),
            (
                "requirement",
                "requirementDiagram\n  requirement r1 {\n    id: 1\n  }\n  element e1 {\n  }\n  e1 - satisfies -> r1\n",
            ),
            ("mindmap", "mindmap\n  root((core))\n    a\n    b\n"),
            (
                "sequence",
                "sequenceDiagram\n  participant A\n  participant B\n  A->>B: hi\n",
            ),
            ("gitGraph", "gitGraph\n  commit\n  branch dev\n  commit\n"),
            ("timeline", "timeline\n  2024 : a\n  2025 : b\n"),
            ("journey", "journey\n  section S\n    task: 5: Me\n"),
            ("kanban", "kanban\n  Todo\n    t1\n"),
            ("blockBeta", "block-beta\n  columns 2\n  a b\n"),
        ];
        for (label, source) in fixtures {
            let parsed = fm_parser::parse(source);
            assert!(
                deck_manifest_supported(parsed.ir.diagram_type),
                "{label}: fixture family {:?} must be in the supported set",
                parsed.ir.diagram_type
            );
            let svg = crate::render_svg(&parsed.ir);
            assert!(
                svg.contains("id=\"fm-node-"),
                "{label}: supported family {:?} emitted no fm-node-* ids — remove it from \
                 deck_manifest_supported or fix the renderer",
                parsed.ir.diagram_type
            );
        }
    }
}
