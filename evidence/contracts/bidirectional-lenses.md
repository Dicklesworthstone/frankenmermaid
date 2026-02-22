# Decision Contract: Bidirectional Lenses for Diagram/Text Sync (Section 6.2)

## Graveyard Score: 4.0 / Tier: B

## Hypothesis

Implementing bidirectional lenses (get/put pairs satisfying GetPut and PutGet laws) between Mermaid source text and the diagram IR will enable round-trip editing: users can modify the rendered diagram (move a node, edit a label) and have those changes reflected back in the source text, and vice versa. This transforms the pipeline from one-way (text -> IR -> render) to bidirectional (text <-> IR <-> render).

## Current Baseline

**File:** `crates/fm-parser/src/lib.rs`, `crates/fm-parser/src/mermaid_parser.rs`

**Current pipeline:** One-way only.
```
Mermaid Text -> Chumsky Parser -> IrBuilder -> MermaidDiagramIr -> Renderers
```

**Parser characteristics:**
- Chumsky-based declarative parser combinator (version 0.10)
- Recovery-first: malformed input produces best-effort IR + warnings
- Detection methods: ExactKeyword > FuzzyKeyword > ContentHeuristic > DotFormat > Fallback
- Parser confidence tracked as f32 (0.0-1.0)
- No reverse generation capability

**Span preservation:**
- `Span { start: Position, end: Position }` with line/col/byte offsets (fm-core)
- Each `IrNode` has `span_primary` and `span_all` fields
- Spans are captured during parsing but not used for reverse mapping
- No span-to-source reconstruction logic exists

**IR structure (fm-core):**
- `MermaidDiagramIr` holds nodes, edges, clusters, labels, constraints
- Labels stored in `labels: Vec<IrLabel>` with `IrLabelId` references
- Node shapes, edge arrow types, line types all preserved in IR
- Direction (LR/RL/TB/BT) stored at IR level

**No roundtrip tests exist. No reverse code generation. No lens crate in dependencies.**

## Acceptance Criteria (Adopt)

- [ ] GetPut law holds: `put(get(source), source) == source` for 100% of benchmark corpus
- [ ] PutGet law holds: `get(put(view, source)) == view` for 100% of benchmark corpus
- [ ] Round-trip latency < 200ms on 500-node diagram
- [ ] Supported operations: node label edit, node addition, node deletion, edge modification
- [ ] Source formatting preserved: whitespace, comments, indentation maintained through round-trip
- [ ] At least flowchart and sequence diagram types supported for round-trip
- [ ] Implementation <= 800 LOC (lens infrastructure + flowchart/sequence support)
- [ ] Existing parser behavior unchanged (additive changes only)

## Rejection Criteria (Reject)

- [ ] GetPut or PutGet law violation on any corpus input (data loss)
- [ ] Round-trip latency > 500ms on 500-node diagram
- [ ] Source formatting destroyed: comments lost, indentation changed, whitespace collapsed
- [ ] Only works for trivial edits (single character changes)
- [ ] Requires fundamental parser rewrite (> 500 LOC changes to existing parser)
- [ ] Lens infrastructure exceeds 1200 LOC
- [ ] Cannot handle recovery/error state in source (lens fails on malformed input)

## Evaluation Protocol

1. **Baseline measurement**
   - Catalog all diagram types and their source syntax
   - Count source-to-IR information loss for each type (what is discarded during parsing)
   - Identify which formatting details are not captured in IR (whitespace, comments, etc.)
   - Create golden-file test suite: source -> IR -> regenerated source comparison
   - Measure parser round-trip: parse -> unparse -> reparse -> compare IR equality

2. **Implementation**
   - Phase 1: Implement source regeneration (IR -> Mermaid text) for flowcharts
     - Use Span information to reconstruct original text where possible
     - Fall back to canonical generation where spans are unavailable
   - Phase 2: Implement lens interface with GetPut/PutGet law checking
     - `get: MermaidText -> MermaidDiagramIr` (existing parser)
     - `put: (MermaidDiagramIr, MermaidText) -> MermaidText` (new: merge IR changes back)
   - Phase 3: Extend to sequence diagrams as second supported type
   - Gate behind feature flag; existing pipeline unchanged

3. **Post-measurement**
   - Run law verification on full benchmark corpus
   - Measure round-trip timing
   - Check source formatting preservation quality (diff original vs round-tripped)

4. **Statistical comparison**
   - Report law satisfaction rate per diagram type
   - Report formatting preservation score (% of source lines unchanged)
   - Report timing: parse time vs put time vs total round-trip

5. **Decision**
   - Adopt if both laws hold at 100% on supported types and formatting mostly preserved
   - Hybrid if only subset of edit operations supported (e.g., label edits only)
   - Defer if parser needs significant restructuring to capture enough source info
   - Reject if fundamental information loss in parsing makes round-trip impossible

## Benchmark Corpus

- **Minimal flowchart:** 3 nodes, 2 edges (law verification)
- **Formatted flowchart:** 10 nodes with comments, varied indentation (formatting preservation)
- **Complex flowchart:** 50 nodes, subgraphs, styled nodes (feature coverage)
- **Sequence diagram:** 5 participants, 10 messages (second type support)
- **Malformed input:** Source with syntax errors + recovery (robustness test)
- **Large diagram:** 500 nodes (performance test)
- **Edit scenarios:** Node label change, node addition, edge deletion (operation coverage)

## Timeline

- Source analysis + information loss audit: TBD
- Phase 1 (source regeneration): TBD (estimate: 3-4 sessions)
- Phase 2 (lens laws): TBD (estimate: 2-3 sessions)
- Phase 3 (sequence diagram): TBD (estimate: 2 sessions)
- Evaluation: TBD
- Decision: TBD

## Reviewers

Project maintainer (ubuntu)

## Notes

- The Span infrastructure already exists in fm-core and is captured during parsing
- Key challenge: Chumsky's recovery mode may discard source information that prevents clean round-trip
- Lens laws are strict: if parsing is lossy (discards comments), put must still preserve them
- Strategy: Store original source alongside IR; lens `put` patches original source rather than regenerating
- This approach ("patching lens") is more robust than full regeneration
- Alternative: Use tree-sitter for incremental parsing with concrete syntax tree (preserves all source)
- Reference: Foster et al. "Combinators for Bidirectional Tree Transformations" (2007)
- Reference: Bohannon et al. "Boomerang: Resourceful Lenses for String Data" (2008)
