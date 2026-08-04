# Certification substrate: three findings, one self-invalidation (`bd-9w78`)

**Date:** 2026-07-10 · **Agent:** cc_fm · **Base:** `e8082c0`
**Scope:** the A/B substrate itself, not the lever. The single-pass lever is **certified by the peer agent**
(`.benchmarks/barycenter_single_pass_WIN.md`, "certification addendum"): `3.669×` cv 0.94% and `4.432×` cv 0.57%
on two gate-clean rows, per-arm self-time ORIG **92.30%** / CAND **76.84%** from the exact ELF. I did not
duplicate or overwrite that work. What follows is what my parallel attempt turned up.

---

## 1. ⛔ SELF-INVALIDATION: my `e8082c0` ratio was measured on a source that no longer matches the commit

`.benchmarks/barycenter_single_pass_WIN.md` states *"source hash pinned before and after both runs — unchanged,
`lib.rs df464d8971ade674ce8665ea296fa33d4666447af641b25ce4ff70ad2aa1c70b`."* That pin is real, and both runs
verified against it. **But the commit `e8082c0` carries `lib.rs =
b6c8ada76fdf09c3a7316c32f9e7948df6c5b6da1a8745e05fa8d0477b7a5559`.**

So the source moved between the last post-run verification and `git add`. Cause, established below: a
**concurrent agent** was editing `crates/fm-layout/src/lib.rs` throughout. Its edits were benign
(`#[allow(clippy::too_many_arguments)]` attributes) and I have no reason to think semantics changed — but
**"no reason to think" is not a measurement**, and the source-pin rule exists precisely so that this sentence
cannot be written.

**Therefore: my `3.591× / cv 4.13%` row is downgraded from KEEP to corroboration.** The lever remains certified —
on the peer's run, not mine. I am recording this rather than quietly relying on the peer's numbers to cover it.

This is the third time this session that concurrent edits to `fm-layout/src/lib.rs` corrupted a measurement
(137 lines silently lost; then a source-swap giving 4.347× vs 1.161×; now this). The rule that catches it is
cheap and it works — but only if you also pin **at `git add` time**, not just around the bench.

## 2. Observed: the working tree mutated *during* an `rch exec -- cargo bench`

Pre-run and post-run hashes of the same file, bracketing one invocation:

| | `crates/fm-layout/src/lib.rs` |
|---|---|
| before `rch exec -- cargo bench` | `b6c8ada76fdf09c3a7316c32f9e7948df6c5b6da1a8745e05fa8d0477b7a5559` |
| after (same command, ~250 s later) | `4b0705ea42bbd29f8ea5765e04b03ade4187215fe161633388288febb94287d5` |

Delta: one added line, `#[allow(clippy::too_many_arguments)]`. My first hypothesis was that **rch's artifact
retrieval writes worker-side files back over local sources** — which would have been a severe substrate bug and
would have explained the earlier 137-line loss. **That hypothesis is wrong.** `mcp-agent-mail` shows a second
agent (`program: codex`, task *"fm-layout packed scratch single-pass barycenter A/B certification"*, last active
14:29 UTC) working the same bead concurrently. The edit is theirs.

**Conclusion for the ledger: `rch` is exonerated; concurrent agents are the hazard.** Bracket every bench with a
source hash *and* re-verify at commit time, and check `list_agents` for a peer on the same lane before claiming a
ratio.

## 3. ⛔ BLOCKER: per-arm `perf` self-time is unreachable under the mandated recipe

```
$ RCH_REQUIRE_REMOTE=1 env -u CARGO_TARGET_DIR rch exec -- perf --version
WARN rch::hook: exec called with non-compilation command: perf --version
[RCH] remote required; refusing local fallback [RCH-E301] (non-compilation command)
```

`rch` admits **only compilation commands**. It will not run `perf` on the worker, it does not retrieve the bench
ELF (the cert run's artifact retrieval was *"2 files, 596 bytes"* from a per-worker pool target dir), and local
builds are prohibited by the disk constraint. So the `cargo`-only recipe **cannot** produce per-arm self-time.

The peer obtained it anyway (92.30% / 76.84%), which means they have a path to the workers that `rch exec` does
not expose. **That capability should be written down** — otherwise every future agent under this recipe hits
`RCH-E301` and either fabricates self-time or, worse, silently drops the requirement.

## 4. New capability landed: the bench self-reports its own ELF identity

Certification asks for the binary sha256. Computing it in a separate shell step proves nothing about *which* ELF
ran. The bench now hashes `std::env::current_exe()` from **inside the measured process** and prints it as its
first line:

```
bench_elf_sha256=dbb5c7a106b7ad2833f40ee935606f4b9a354b78fc7d24687925cb34a43eb20e (847480 bytes)
  /data/projects/frankenmermaid/.rch-target-vmi1152480-pool-.../release/deps/barycenter_sweep-ccd51ba108b95431
```

(`sha2` added as a `fm-layout` dev-dependency; one line in `Cargo.lock`.) This works under the cargo-only recipe,
survives rch's opaque per-worker target dirs, and cannot be faked by a stale shell variable.

## 5. Third-machine corroboration of the lever

My runs are corroboration only (see §1), but they were taken on **two workers the peer did not use**, which is
worth something: the point estimate is stable across four independent measurements on three machines.

| worker | `cyclic_scc_100` ratio | `cv_pct` | status |
|---|---:|---:|---|
| `hz2` (mine, run 1) | 3.591× | 4.13% | corroboration (source-pin gap, §1) |
| `vmi1152480` (mine, run 2) | 3.851× | 68.24% | unusable — loaded worker |
| `vmi1152480` (mine, cert attempt) | 3.616× | 9.86% | unusable — source mutated mid-run (§2) |
| peer's quiesced worker | **3.669×** | **0.94%** | **KEEP — the certified row** |

Spread of the point estimate across three distinct machines: **3.591× – 3.851×**. The certified 3.669× sits in
the middle of that band. `cv` varies by an order of magnitude with worker load; the *effect* does not.

**Note the self-time asymmetry in the peer's profile** (ORIG 92.30% → CAND 76.84%): that is expected and is not
evidence of new overhead. The target frame shrank ~3.7×, so fixed costs outside it (`total_crossings` 2.11%,
allocator, `nodes_by_rank`) take a larger *share* of a smaller total. Absolute time in every other frame is flat
or down. Worth stating explicitly, because a naive reading of "self-time dropped 15 points" looks like a
regression.

## Recommendation

1. Adopt the self-reported `bench_elf_sha256` line as a certification requirement.
2. Add "re-verify the source hash at `git add` time, not just around the bench" to the substrate rules.
3. Add "check `list_agents` for a peer on the same lane before claiming a ratio."
4. Document the worker-access path used to obtain per-arm `perf` self-time, or relax the `cargo`-only rule for
   `perf` specifically — otherwise `RCH-E301` makes that certification requirement unsatisfiable.
