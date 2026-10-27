# Decision Contract: Conformal Geometric Algebra (Section 12.11)

## Graveyard Score: 2.5 / Tier: C

## Hypothesis

Replacing the axis-aligned orthogonal coordinate system and rectilinear edge routing with Conformal Geometric Algebra (CGA) representations will improve code clarity for geometric operations (rotations, reflections, intersections) and enable non-rectilinear layouts (curved edges, radial layouts) while maintaining performance within 2x of the current matrix-based approach.

## Current Baseline

**File:** `crates/fm-layout/src/lib.rs`

**Geometric primitives:**
```rust
pub struct LayoutPoint { pub x: f32, pub y: f32 }
pub struct LayoutRect { pub x: f32, pub y: f32, pub width: f32, pub height: f32 }
```

**Coordinate assignment** (line ~972):
- Treats layout as 2D rectilinear grid
- `primary_offsets`: cumulative sum of rank_span + spacing
- `secondary_cursor`: incremented by node extent + spacing
- Direction-aware axis mapping: (x,y) or (y,x) based on graph direction
- Single-pass, O(|nodes| x |ranks|)

**Edge routing** (line ~1271):
- Anchor point selection: edge-parallel to rank direction (top/bottom or left/right)
- 4-point polyline routing: source -> mid-x/y -> target
- Collinear point simplification with epsilon threshold
- O(|edges| x polyline_length)

**Metrics available:**
- `LayoutStats.total_edge_length`: sum of all polyline segment lengths
- `LayoutStats.reversed_edge_total_length`: sum of reversed edge lengths
- `polyline_length()`: helper computing Euclidean segment sum

**No current curved edge support. No rotation/reflection transforms. No radial layout mode.**

## Acceptance Criteria (Adopt)

- [ ] Code clarity measurably improved: geometric operations (rotation, reflection, intersection) expressible in fewer LOC than current ad-hoc coordinate math
- [ ] Performance within 2x of current matrix operations on rendering hot path
- [ ] Enables at least one new layout capability not possible with current system (e.g., curved edges, radial layout, or conformal mapping)
- [ ] All existing layout tests pass (coordinate outputs equivalent within epsilon)
- [ ] WASM compilation succeeds with no performance regression > 3x
- [ ] Implementation <= 800 LOC (CGA wrapper + integration)
- [ ] No new `unsafe` code required

## Rejection Criteria (Reject)

- [ ] Performance > 3x slower than current matrix operations on edge routing
- [ ] CGA crate does not compile for `wasm32-unknown-unknown`
- [ ] Code is not demonstrably clearer: reviewers find CGA notation harder to read
- [ ] Implementation exceeds 1200 LOC
- [ ] Coordinate outputs differ by > 1 pixel from current system (for rectilinear mode)
- [ ] Learning curve too steep: team cannot maintain CGA code without specialist knowledge

## Evaluation Protocol

1. **Baseline measurement**
   - Profile coordinate_assignment() and route_edge_points() separately
   - Record wall-clock time and output coordinates for benchmark corpus
   - Count LOC for geometric operations (rotations, intersections, bounding boxes)
   - Document which operations are "awkward" in current coordinate system

2. **Implementation**
   - Wrap CGA operations behind trait abstraction (allow fallback to Euclidean)
   - Implement LayoutPoint as CGA conformal point (e1, e2, e_inf, e_0)
   - Implement edge routing using CGA line representations
   - Add curved edge routing as proof-of-capability
   - Gate behind LayoutConfig option

3. **Post-measurement**
   - Same corpus, compare coordinate outputs (should be equivalent for rectilinear)
   - Measure performance of CGA path vs Euclidean path
   - Count LOC for equivalent geometric operations
   - Conduct readability review with at least one non-CGA-expert

4. **Statistical comparison**
   - Report performance ratio (CGA / Euclidean) per operation type
   - Report LOC reduction for geometric utilities
   - Qualitative: readability survey (at least 2 reviewers)

5. **Decision**
   - Adopt if performance within 2x AND at least one new capability demonstrated
   - Hybrid if adopt for rendering only (not layout core)
   - Reject if performance > 3x or readability concerns from reviewers

## Benchmark Corpus

- **Small rectilinear:** 10-node flowchart (baseline equivalence check)
- **Medium rectilinear:** 100-node layered DAG (performance stress)
- **Curved layout:** 20-node graph requiring curved edges (capability test)
- **Rotation test:** Same graph laid out in 4 directions (LR, RL, TB, BT)
- **Mixed:** Graph with both straight and curved edge requirements

## Timeline

- Baseline + LOC audit: Completed (2026-08-04)
- CGA prototype: Completed (`fm-core::cga`, `fm-layout::cga_routing`, `fm-render-svg::cga_transform`)
- Evaluation + review: Completed (`crates/fm-layout/benches/cga_transforms.rs`, commit `630a4f34`, `bd-9mqa`)
- Decision: **RATIFIED — Hybrid Adoption** (2026-08-05 / 2026-10-14)

## Measured Benchmark Results (`crates/fm-layout/benches/cga_transforms.rs`, `630a4f34`)

Criterion per-iteration comparison in the same invocation (non-overlapping confidence intervals):

| Scenario | 2D Affine Matrix | CGA Rotor (16-component) | Ratio (CGA / Matrix) |
|---|---|---|---|
| Compose x4 | 61.2 ns | 2.13 µs | 34.8x |
| Compose x16 | 235.9 ns | 9.34 µs | 39.6x |
| Compose x64 | 1.01 µs | 38.49 µs | 38.1x |
| Compose + Apply (1024 points) | 973 ns | 9.94 µs | 10.2x |

## Ratified Decision Contract

1. **Use CGA / Euclidean primitives (`fm-core::cga`) for:**
   - Analytic intersection queries (`CgaLineSegment::intersect_segment`, `CgaRect::intersects_segment`, `CgaCircle::intersect_segment`)
   - Obstacle-aware orthogonal edge routing (`crates/fm-layout/src/cga_routing.rs`, `detour_blocked_orthogonal_route`)
   - Rigid rotor transform composition (`CgaTransformStack`, `try_render_transform_to_cga`) with one-time affine extraction (`to_affine_matrix` / `to_svg_transform`)
2. **Use direct 2D affine matrices for:**
   - Anisotropic scaling, skew transforms, and hot batch point loops (`N > 1`)
3. **Do NOT implement 5D conformal `meet()` / `join()` multivector products for hot paths:**
   - Direct conformal inner-product distance (`d^2 = -2 P·Q`) exhibited catastrophic cancellation at coordinates ~90 units from the origin (`681cdf4a`), whereas analytic Euclidean predicates on `CgaPoint`/`CgaLineSegment`/`CgaRect` are exact, branch-predictable, and 10–40x faster.

## Reviewers

Project maintainer (ubuntu)

