# PARKED, UNCOMPILED: architecture-beta direction-aware placement (bd-zce4)

**⚠️ THIS CODE HAS NEVER BEEN COMPILED.** It was written during a host throttle with builds
forbidden (load 652, 78% iowait, 677 blocked processes). Treat every signature as a claim to verify,
not a fact. It is parked rather than committed live precisely so an uncompiled lever cannot turn
main red.

The acceptance gate already exists and is executable:
`crates/fm-render-svg/tests/architecture_placement.rs` — one control that passes today, and two
`#[ignore]`d gates verified to fail (`a.x=0 and b.x=0`). Un-ignoring both is how bd-zce4 closes.

## What is actually wrong

`a:R --> L:b` means **b sits to the RIGHT of a**. R/L/T/B are a PLACEMENT grammar, which mermaid
models per edge as `sourceDir`/`targetDir` (`ArchitectureDirection`, pinned 11.15.0 bundle).

Measured: architecture-beta falls through `select_general_graph_algorithm_with_config` and lands on
**Sugiyama**, which stacks the services vertically — `edge pts=[(53.83, 98.75), (53.83, 218.75)]`,
same x, top to bottom. `parse_architecture_endpoint` detects the side token via
`is_architecture_side_token` and then **discards it**, returning only the id.

## Two traps that killed the ORIGINAL plan in the bead

1. The bead first proposed anchoring endpoints via `clip_to_shape_border`. Both of that function's
   call sites are in `force_build_edge_paths`, and architecture uses **Sugiyama** — the change would
   never have executed. Even if it had, anchoring on the correct sides of two vertically stacked
   boxes still draws a vertical diagram.
2. It also proposed `IrEndpoint::Port`. There are **372** direct matches on `IrEndpoint::Node`
   across layout and the three renderers against **3** uses of `resolved_node_id`, so Port endpoints
   would be unresolvable almost everywhere.

Hence: carry the sides on `IrEdgeExtras` (where cardinalities already live) and give the diagram
type its own placement pass. Do not touch endpoints.

## Part 1 — `crates/fm-core/src/lib.rs`

Mirror the cardinality fields exactly, including the serde attributes:

```rust
    /// Declared source side for an architecture-beta edge (`"L" | "R" | "T" | "B"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_side: Option<Box<str>>,
    /// Declared target side for an architecture-beta edge.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_side: Option<Box<str>>,
```

and the two accessors beside `source_cardinality()`:

```rust
    /// Declared source side, if any.
    pub fn source_side(&self) -> Option<&str> {
        self.extras.as_ref().and_then(|e| e.source_side.as_deref())
    }
    /// Declared target side, if any.
    pub fn target_side(&self) -> Option<&str> {
        self.extras.as_ref().and_then(|e| e.target_side.as_deref())
    }
```

## Part 2 — `crates/fm-parser`

`parse_architecture_endpoint` currently returns `Option<String>` and drops the side. Return it:

```rust
fn parse_architecture_endpoint(raw: &str) -> Option<(String, Option<&'static str>)> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some((lhs, rhs)) = trimmed.split_once(':') {
        let left = lhs.trim();
        let right = rhs.trim();
        // The side may be written on either side of the colon: `a:R` and `L:b` are both legal, and
        // which one carries the token depends on whether the endpoint is the source or the target.
        if let Some(side) = architecture_side_token(left) {
            return clean_label(Some(right)).map(|id| (id, Some(side)));
        }
        if let Some(side) = architecture_side_token(right) {
            return clean_label(Some(left)).map(|id| (id, Some(side)));
        }
    }
    clean_label(Some(trimmed)).map(|id| (id, None))
}

/// Returns the canonical `'static` token so the side can be stored without another allocation.
fn architecture_side_token(token: &str) -> Option<&'static str> {
    match token {
        "L" => Some("L"),
        "R" => Some("R"),
        "T" => Some("T"),
        "B" => Some("B"),
        _ => None,
    }
}
```

`is_architecture_side_token` becomes `architecture_side_token(..).is_some()` — check for other
callers before deleting it.

`parse_architecture_edge` widens its tuple to carry both sides, and **must swap them with the
endpoints** in the `reverse` arm (`<--`), or a reversed edge gets its sides crossed:

```rust
            return if reverse {
                Some((to, from, arrow, to_side, from_side))
            } else {
                Some((from, to, arrow, from_side, to_side))
            };
```

New builder setter, in `ir_builder.rs` beside `set_last_edge_cardinality`. **The
`mark_reusable_prefix_edge_dirty` call is not optional** — every other edge mutator does it, and
omitting it lets incremental reuse serve a pre-edit edge:

```rust
    /// Set architecture-beta placement sides on the most recently pushed edge.
    pub(crate) fn set_last_edge_architecture_sides(
        &mut self,
        source: Option<&str>,
        target: Option<&str>,
    ) {
        if source.is_none() && target.is_none() {
            return;
        }
        if let Some(edge_index) = self.ir.edges.len().checked_sub(1) {
            self.mark_reusable_prefix_edge_dirty(edge_index);
        }
        if let Some(edge) = self.ir.edges.last_mut() {
            if let Some(s) = source {
                edge.extras_mut().source_side = Some(Box::from(s));
            }
            if let Some(t) = target {
                edge.extras_mut().target_side = Some(Box::from(t));
            }
        }
    }
```

called immediately after the existing `builder.push_edge(from_node, to_node, arrow, None, span);`
at `mermaid_parser.rs:7535`.

## Part 3 — `crates/fm-layout/src/lib.rs`

Add to the specialized dispatch (beside `DiagramType::PacketBeta => LayoutAlgorithm::Packet`):

```rust
        DiagramType::ArchitectureBeta => LayoutAlgorithm::Architecture,
```

**FALL BACK when no edge declares a side**, so existing architecture diagrams written without
directions keep their current appearance and this cannot regress them:

```rust
/// Place architecture-beta services from the DECLARED edge directions.
///
/// `a:R --> L:b` means b sits to the right of a. Ranking these by edge (Sugiyama) throws the
/// grammar away and stacks them vertically, which is bd-zce4.
fn layout_diagram_architecture_traced(ir: &MermaidDiagramIr) -> TracedLayout {
    // No direction anywhere ⇒ nothing to honour; defer rather than impose a worse layout.
    if !ir.edges.iter().any(|e| e.source_side().is_some() || e.target_side().is_some()) {
        return layout_diagram_general_traced(ir); // ← VERIFY THE REAL NAME OF THIS ENTRY POINT
    }

    let node_count = ir.nodes.len();
    let mut cell: Vec<Option<(i32, i32)>> = vec![None; node_count];

    // Seed every component: an edge whose source is unplaced starts a new origin, so a diagram with
    // two disconnected clusters does not collapse both onto (0, 0).
    // Repeat to a fixed point: an edge may be declared before the node it hangs off is placed.
    let mut progressed = true;
    while progressed {
        progressed = false;
        for edge in &ir.edges {
            let (Some(from), Some(to)) = (
                endpoint_node_index(ir, edge.from),
                endpoint_node_index(ir, edge.to),
            ) else {
                continue;
            };
            // The SOURCE side is what positions the target: `a:R` means "leaves a going right".
            let Some(delta) = edge.source_side().and_then(architecture_delta) else {
                continue;
            };
            if cell[from].is_none() && cell[to].is_none() {
                cell[from] = Some((0, 0));
                progressed = true;
            }
            if let (Some(base), None) = (cell[from], cell[to]) {
                cell[to] = Some((base.0 + delta.0, base.1 + delta.1));
                progressed = true;
            }
        }
    }

    // Anything still unplaced (no directed edge touches it) goes in a trailing row rather than
    // being dropped or piled onto an occupied cell.
    let mut spare = 0_i32;
    let max_row = cell.iter().flatten().map(|c| c.1).max().unwrap_or(0);
    for slot in cell.iter_mut() {
        if slot.is_none() {
            *slot = Some((spare, max_row + 1));
            spare += 1;
        }
    }

    // ⚠️ UNRESOLVED, and deliberately left to the implementer: two targets can be sent to the SAME
    // cell (`a:R --> L:b` and `a:R --> L:c`). Options are to fan the collision out along the
    // perpendicular axis, or to treat the second as unplaced. Whichever is chosen needs its own
    // test; do not let it silently overlap two boxes.

    // Grid → pixels, then reuse the normal edge builder.
    // `finalize_specialized_layout` takes (ir, sizes, rank_by_node, order_by_node, centers, trace,
    // ..) — mirror `layout_diagram_gitgraph_traced`, which does exactly this with lane/row indices.
    todo!("convert `cell` to centers via node_sizes + LayoutSpacing, then finalize")
}

/// Grid step for a declared side. Screen coordinates: +y is DOWN, so `T` is negative.
fn architecture_delta(side: &str) -> Option<(i32, i32)> {
    match side {
        "R" => Some((1, 0)),
        "L" => Some((-1, 0)),
        "B" => Some((0, 1)),
        "T" => Some((0, -1)),
        _ => None,
    }
}
```

## Verification checklist before this is trusted

1. It compiles at all. Several names above are asserted from reading, not from a build:
   `layout_diagram_general_traced`, `endpoint_node_index`'s signature, `LayoutAlgorithm::Architecture`
   needing a new variant, and `finalize_specialized_layout`'s exact parameter list.
2. `cargo test -p fm-render-svg --test architecture_placement -- --ignored` must PASS, and both
   `#[ignore]` attributes then come off. The reversed-direction test is the one that catches a
   layout which merely always places the second node to the right.
3. The `feature_parity.rs` KNOWN_GAPS entry for `arch_edge_sides` must be DELETED — that list is
   checked in both directions, so leaving it fails the gate once the sides start mattering.
4. Golden corpus: check whether any `.mmd` fixture is architecture-beta with directions before
   blessing anything, and explain each moved case rather than bulk-blessing.
5. The collision case above needs a decision and a test.
