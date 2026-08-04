# bd-1buv.2 — large-flowchart node-metadata micro sweep — NEGATIVE

Date: 2026-07-24
Agent: cod (`MagentaGull`)

## Scope and method

This continuation stayed inside the measured parse-layout-SVG frontier and used the pinned
`scripts/headtohead` `flowchart_large_500` corpus (500 nodes, 499 edges, 15,060 input bytes).
The starting production-equivalent profile was the 5,642-sample, zero-loss profile recorded in
`.benchmarks/bd_1buv_2_parse_layout_svg_floor_NEGATIVE.md`. It put `render_nodes_serial` at 3.02%
self, `write_common_node_fragment_into::<true>` at 2.33%,
`write_mermaid_node_element_id_into` at 1.04%, and
`simple_node_user_class_suffix` at 0.55%; `lookup_centrality_tier` had no separately measurable
self share.

Every candidate was built fail-closed through RCH from immutable `HEAD` plus only the explicitly
reserved overlay. Baseline and candidate ELF files were copied to the same worker and pinned to
one CPU. The final comparison worker was `hz2`, CPU14. The immutable baseline SHA-256 was
`5be73a4c163214bdd3cd8eb6a05313292ae2173b1333e12e71a5957429b3b908`.
All admissible pinned rows emitted exactly 343,946 SVG bytes.

## REJECT 1 — fuse element-id sanitization, accent hashing, and data-id escape detection

The candidate computed the existing FNV-1a accent hash and XML-attribute escape flag in the DOM-id
sanitizer's raw-byte pass, then skipped the redundant clean `data-id` scan. An exact oracle covered
the old/new DOM id, FNV hash, and XML metacharacter predicate.

Render-only Criterion showed a small direction, but the decisive immutable
`scripts/headtohead` A/B had stable round dispersion (baseline p50 CV about 0.6%, candidate about
0.8%) and paired medians of 362/366, 362/360, 359/359, and 365/360 microseconds. The paired median
gain was about 0.3%, wholly inside the baseline null band of 359–365 microseconds. REJECT; source
was restored.

Retry only if a profile attributes at least 3% of the full pinned pipeline to repeated raw node-id
scans, or the pinned corpus adopts materially longer node ids; require a null-adjusted gain above
3%.

## REJECT 2 — explicit empty centrality-map guard

The pinned flowchart has no node-centrality extensions, so the candidate returned `None` from
`lookup_centrality_tier` before calling `HashMap::get`. Admissible pinned pairs were
359/363, 368/372, and 370/368 microseconds (baseline/candidate): +1.1%, +1.1%, and -0.5%, a median
regression of about 1.1%. REJECT; source was restored.

Retry only if a future standard-library `HashMap::get` implementation no longer short-circuits an
empty table, or a profile gives this helper a measurable >=3% full-pipeline share.

## REJECT 3 — borrowed empty user-class suffix

The candidate changed `simple_node_user_class_suffix` to return `Cow<str>` and borrowed `""` for
the overwhelmingly common classless node, while preserving the owned styled-node path. The
admissible pinned pairs were 363/356 and 361/360 microseconds. Candidate values remained inside
the baseline null band (360–372 microseconds); the approximately 1.1% paired direction is below
the predeclared 3% null-adjusted floor. REJECT; source was restored.

Retry only if class-suffix handling rises above 3% of the full pinned profile or a representation
change removes actual allocation rather than only moving an empty `String`.

## Verdict

Three consecutive fresh candidates were rejected. No production source change ships. The only
retained changes are this NEGATIVE ledger and its summary row in `docs/NEGATIVE_EVIDENCE.md`.
