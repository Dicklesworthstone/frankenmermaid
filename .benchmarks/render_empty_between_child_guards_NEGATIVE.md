# Large Render Empty Between-Child Guards - Negative Evidence

Date: 2026-07-10
Agent: cod_fm
Lane: large-diagram render double-copy

## Attempt

Tried a non-body-fusion shape after the closed large `to_string_with_body` and raw-part body-fusion regressions:

- precompute `has_bundle_count_labels` once and skip the slow-path bundle-label loop when false;
- skip class-cardinality emission outside `DiagramType::Class`;
- leave `Element::raw_svg_parts` and the final `SvgDocument::to_string_with_capacity` path unchanged.

This was meant to remove empty between-edge-and-node passes from the large slow path without reusing the rejected
`to_string_with_body` chunk path.

## Profile

Focused render-loop profile on `large_wide_stages/render/40x80`, release profile with symbols preserved only for
attribution:

- `perf stat` current-main short Criterion run: 33.92B cycles, 44.11B instructions, 351.1M cache misses,
  24.11% cache-miss rate.
- corrected `perf record -- ... --bench --profile-time 10` captured 23K samples.
- top named user-space hotspot: `__memmove_avx_unaligned_erms`, 14.82%.
- attempted target: `write_class_cardinality_labels_into`, 0.49%.
- flamegraph generated from captured perf data at `/tmp/cod_fm_large_render_renderloop.svg`.

## A/B Measurement

Exact row:

```bash
cargo bench -p frankenmermaid-cli --bench pipeline_bench --profile release -- \
  large_wide_stages/render/40x80 \
  --warm-up-time 1 --measurement-time 5 --sample-size 30 --noplot --discard-baseline
```

ORIG was built from a `git archive HEAD` snapshot at `/tmp/frankenmermaid-orig-aafe1c1-1783650284`.
CANDIDATE was built from the edited checkout. Both were measured on the same machine.

| Profile | ORIG | Candidate | Verdict |
|---|---:|---:|---|
| release + debuginfo symbols | [2.1054, 2.1414, 2.1770] ms | [2.0891, 2.1433, 2.1947] ms | flat |
| stripped release | [2.1797, 2.2372, 2.3072] ms | [2.1551, 2.2561, 2.3753] ms | +0.85% median slower |

## Verdict

Rejected and reverted. The empty pass exists but is too small and noisy to clear the keep gate. It does not remove the
dominant raw-chunk-to-final contiguous `String` copy.

Do not retry this exact empty between-child guard shape unless a future profile shows the bundle/class empty passes as
a top-5 render-loop hotspot or the insertion model changes.
