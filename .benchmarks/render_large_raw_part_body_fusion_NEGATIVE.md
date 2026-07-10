# Large Raw-Part Body Fusion - REJECTED 2026-07-10

Agent: cod_fm

## Lever

Preserve the existing large native parallel edge/node chunk rendering, but avoid wrapping the
prebuilt `Vec<String>` chunks as `Element::raw_svg_parts` children. The candidate added a large
no-interleaving fast path that called `SvgDocument::to_string_with_body` and pushed the already-built
raw chunks directly into the body position.

This is distinct from the 2026-07-05 rejected large `to_string_with_body` attempt, which rendered the
large chunks inside the body closure and lost the parallel/raw-parts shape.

## Measurement

Command shape:

```bash
AGENT_NAME=cod_fm \
CARGO_TARGET_DIR=/data/projects/frankenmermaid/.rch-targets/cod_fm_large_render_candidate \
RCH_REQUIRE_REMOTE=1 \
RCH_QUEUE_WHEN_BUSY=1 \
rch exec -- cargo bench --profile release -p frankenmermaid-cli --bench pipeline_bench -- \
  large_wide_stages/render/40x80 \
  --warm-up-time 1 --measurement-time 2 --sample-size 10 --noplot
```

Same-worker RCH worker: `vmi1227854`.

| Variant | Criterion interval |
| --- | --- |
| Baseline current main | [2.8529 ms, 3.0663 ms, 3.3002 ms] |
| Candidate | [3.5472 ms, 3.7810 ms, 4.0753 ms] |

Criterion reported change: **+22.654%**, p=0.01. Performance regressed.

## Verdict

Rejected and code reverted.

Skipping the raw `Element` wrapper does not remove the dominant fragment-to-final contiguous `String`
copy. The candidate only adds branch/helper/closure plumbing around the same copy.

Do not retry this pre-rendered-chunks-through-`to_string_with_body` shape. A future attempt needs a
different output contract, such as a segmented/rope SVG result or caller-provided writer that avoids
requiring one final contiguous `String`.
