# Decision Contract: [Concept Name] (Section Reference)

## Graveyard Score: X.X / Tier: A|B|C

## Hypothesis

[What improvement do we expect? How much? On what workloads?]

## Current Baseline

[Description of current implementation. Include specific files, line numbers, data structures, and algorithms. Reference existing metrics from LayoutStats or other instrumented counters.]

## Acceptance Criteria (Adopt)

- [ ] Metric A improved by >= X% on benchmark corpus
- [ ] No metric regressed by > Y%
- [ ] Implementation complexity <= Z LOC
- [ ] Evidence ledger entry complete with measurements
- [ ] Determinism preserved: repeated runs produce identical output
- [ ] All existing tests pass without modification (or with documented changes)

## Rejection Criteria (Reject)

- [ ] Metric A improvement < X% (threshold for "not worth it")
- [ ] Any metric regression > Y%
- [ ] Implementation complexity > Z LOC
- [ ] Solver/algorithm timeout > T ms on target workload
- [ ] Determinism broken: non-reproducible outputs across runs
- [ ] New dependency introduces unsafe code or WASM incompatibility

## Evaluation Protocol

1. **Baseline measurement** (pre-implementation)
   - Run full benchmark corpus through current pipeline
   - Record all LayoutStats fields + wall-clock timings
   - Capture crossing counts, edge lengths, phase iterations
   - Record memory usage (peak RSS) for representative inputs

2. **Implementation**
   - Feature-gated behind `cfg` flag or `LayoutConfig` option
   - Must not break existing API surface (additive changes only)
   - Must compile for both native and `wasm32-unknown-unknown` targets

3. **Post-measurement** (same corpus, same metrics)
   - Run identical benchmark corpus through new pipeline path
   - Record all metrics under same conditions (same machine, same inputs)
   - Run 10 iterations minimum for timing stability

4. **Statistical comparison**
   - Compute mean, median, p95, and stddev for timed metrics
   - Report relative improvement with 95% confidence intervals
   - Flag any metric regressions exceeding noise threshold (> 2%)

5. **Decision: Adopt / Reject / Defer / Hybrid**
   - Adopt: All acceptance criteria met, no rejection criteria triggered
   - Reject: Any rejection criterion triggered
   - Defer: Promising but needs more work (document gaps)
   - Hybrid: Adopt for subset of use cases (document scope)

## Benchmark Corpus

[Specify the test inputs used for evaluation. Must include at minimum:]
- Small graph: 5-10 nodes, 5-15 edges (smoke test)
- Medium graph: 50-100 nodes, 75-200 edges (typical use)
- Large graph: 500-1000 nodes, 1000-3000 edges (stress test)
- Pathological case: graph that exercises the concept's strength
- Regression case: graph that exercises the concept's weakness

## Timeline

- Baseline: [date]
- Implementation: [date range]
- Evaluation: [date]
- Decision: [date]

## Reviewers

[Who reviews and ratifies the decision?]

## Notes

[Any additional context, references, or related decisions.]
