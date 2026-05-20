# Known Conformance Divergences

This document tracks intentional divergences between frankenmermaid and upstream mermaid-js/FrankenTUI behavior.

## Divergence Policy

- **ACCEPTED**: Intentional design decision, documented and tracked
- **INVESTIGATING**: Under review, may be fixed or accepted
- **WILL-FIX**: Bug that will be corrected
- **WONTFIX**: Upstream behavior is incorrect or undesirable

## Active Divergences

### DISC-001: Kanban node rendering
- **Reference:** FrankenTUI renders kanban items as visible nodes with labels
- **Our impl:** Kanban items parsed as clusters, nodes not rendered in SVG
- **Impact:** SVG output contains no visible task cards, only metadata
- **Resolution:** INVESTIGATING - Layout produces clusters but render path incomplete
- **Tests affected:** `kanban_basic`
- **Review date:** 2026-04-15

### DISC-002: Requirement element type/docref
- **Reference:** mermaid-js supports `type:` and `docref:` in element blocks
- **Our impl:** Parser emits warnings for these properties
- **Impact:** Properties are not parsed into IR
- **Resolution:** ACCEPTED - Low priority, core requirement/element linking works
- **Tests affected:** `requirement_basic`
- **Review date:** 2026-04-15
- **Warning count:** 2 (expected in test)

### DISC-003: GitGraph branch names in SVG
- **Reference:** mermaid-js renders branch names (main, develop) as labels
- **Our impl:** Renders commit nodes with IDs (commit_1, commit_2) and merge labels
- **Impact:** Branch names not visible in SVG, only in node classes
- **Resolution:** ACCEPTED - Branch affiliation is in classes (`fm-node-user-git-branch-0`)
- **Tests affected:** `gitgraph_basic`
- **Review date:** 2026-04-15

### DISC-004: Class member rendering
- **Reference:** mermaid-js renders class members (methods, attributes) inside class boxes
- **Our impl:** Class box contains only class name, members in separate compartments
- **Impact:** SVG layout differs; members may appear differently
- **Resolution:** ACCEPTED - Compartment rendering is correct, visual difference acceptable
- **Tests affected:** `class_inheritance`
- **Review date:** 2026-04-15

### DISC-005: Journey score display
- **Reference:** mermaid-js shows numerical scores (1-5) next to tasks
- **Our impl:** Score encoded in CSS classes (`journey-score-5`)
- **Impact:** Score not visible as text, but styling can reflect it
- **Resolution:** ACCEPTED - Score information preserved in DOM, styling-based display
- **Tests affected:** `journey_user`
- **Review date:** 2026-04-15

## Resolved Divergences

*None yet - all current divergences are under active tracking*

## How to Document a New Divergence

When you discover a behavioral difference:

1. Assign next sequential ID (DISC-NNN)
2. Document:
   - **Reference:** What the reference implementation does
   - **Our impl:** What frankenmermaid does
   - **Impact:** User-visible effect
   - **Resolution:** ACCEPTED/INVESTIGATING/WILL-FIX/WONTFIX
   - **Tests affected:** Which conformance tests are impacted
   - **Review date:** When this was last reviewed

3. If ACCEPTED: Use appropriate test expectations (warnings, svg_contains)
4. If WILL-FIX: Create a bead issue to track the fix
5. Update this document as status changes

## Divergence Statistics

| Status | Count |
|--------|-------|
| ACCEPTED | 4 |
| INVESTIGATING | 1 |
| WILL-FIX | 0 |
| WONTFIX | 0 |
| **Total** | **5** |

## Last Updated

2026-04-15 - Initial documentation of known divergences
