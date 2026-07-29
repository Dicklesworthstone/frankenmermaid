// Driver for the pinned frankenmermaid <-> mermaid-js head-to-head (bead bd-1buv.1).
//
//   node scripts/headtohead/run.mjs --fm-bin <path/to/release/examples/headtohead>
//
// Responsibilities:
//   1. Generate the fixed corpus and verify every input against the SHA-256 pins in pins.json.
//   2. Capture an environment fingerprint (git rev, toolchain, browser, CPU, load).
//   3. Run both engines over byte-identical inputs with warmup discipline.
//   4. Join the results, gate ratios against same-invocation A/A median CIs, and compute ratios.
//   5. Emit JSONL events plus a summary that evidence/ledger can ingest.
//
// A mermaid render that fails is an explicit comparator failure: the run exits non-zero and the
// item is reported with `status: "error"`, never dropped from the table.

import { execFileSync, spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { cpus, loadavg, release, tmpdir, totalmem } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CORPUS, generateAll } from './corpus.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..', '..');
const PINS_PATH = join(HERE, 'pins.json');
const PINS = JSON.parse(readFileSync(PINS_PATH, 'utf8'));

const MIN_CLAIM_RATIO = 1.01;
const THREAD_SWEEP_MIN_SAMPLE_NS = 50_000_000;
const THREAD_SWEEP_CALIBRATION_TARGET_NS = 75_000_000;

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : fallback;
}
const has = (name) => process.argv.includes(`--${name}`);

/** Busy fraction of every CPU over `ms`, from /proc/stat. */
function cpuBusy(ms) {
  const snap = () =>
    readFileSync('/proc/stat', 'utf8')
      .split('\n')
      .filter((l) => /^cpu\d/.test(l))
      .map((l) => {
        const p = l.trim().split(/\s+/);
        const n = p.slice(1, 9).map(Number);
        return { cpu: Number(p[0].slice(3)), idle: n[3] + n[4], total: n.reduce((a, b) => a + b, 0) };
      });
  const a = snap();
  const until = Date.now() + ms;
  while (Date.now() < until) { /* busy-wait: we need wall time, not an event loop turn */ }
  const b = snap();
  return a.map((x, i) => ({ cpu: x.cpu, busy: 1 - (b[i].idle - x.idle) / Math.max(1, b[i].total - x.total) }));
}

/**
 * Pick the least-busy CPU. Pinning the (single-threaded) frankenmermaid runner to one quiet core
 * removes migration jitter. It is also the *conservative* choice for the comparison: mermaid keeps
 * the whole machine and all of Chromium's threads, we take one core.
 */
function pickIdleCpu() {
  const busy = cpuBusy(300).sort((a, b) => a.busy - b.busy);
  return { cpu: busy[0].cpu, busy_pct: Number((busy[0].busy * 100).toFixed(1)) };
}

function sh(cmd, args, opts = {}) {
  try {
    return execFileSync(cmd, args, { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'], ...opts }).trim();
  } catch {
    return null;
  }
}

function fingerprint() {
  const cpu = cpus();
  const trackedStatus =
    sh('git', ['-C', REPO, 'status', '--porcelain', '--untracked-files=no']) ?? '';
  const allStatus =
    sh('git', ['-C', REPO, 'status', '--porcelain', '--untracked-files=normal']) ?? '';
  return {
    captured_at: new Date().toISOString(),
    git_rev: sh('git', ['-C', REPO, 'rev-parse', 'HEAD']),
    git_dirty: trackedStatus.length > 0,
    git_dirty_scope: 'tracked_files_only',
    git_untracked_count: allStatus
      .split('\n')
      .filter((line) => line.startsWith('??')).length,
    rustc: sh('rustc', ['--version']),
    cargo_profile: 'release (opt-level=3 for fm-core/parser/layout/render-svg, lto=fat, codegen-units=1)',
    rustflags: '-C target-cpu=x86-64-v2 (.cargo/config.toml)',
    node: process.version,
    chromium: sh(PINS.chromium.binary, ['--version'])?.split('\n').pop() ?? 'unknown',
    kernel: release(),
    cpu_model: cpu[0]?.model ?? 'unknown',
    cpu_count: cpu.length,
    total_mem_gb: Number((totalmem() / 2 ** 30).toFixed(1)),
    loadavg_1m: loadavg()[0],
    // Recorded because a loaded box inflates both engines; the ratio survives, absolute ms do not.
    load_warning: loadavg()[0] > cpu.length / 4 ? 'elevated load; absolute timings are not comparable across runs' : null,
  };
}

function pct(p, xs) {
  const s = [...xs].sort((a, b) => a - b);
  return s[Math.min(s.length - 1, Math.max(0, Math.ceil((p / 100) * s.length) - 1))];
}

function validElfSelfReport(record) {
  return (
    record?.record === 'binary' &&
    /^[0-9a-f]{64}$/.test(record.elf_sha256) &&
    Number.isSafeInteger(record.elf_bytes) &&
    record.elf_bytes > 0
  );
}

/**
 * Decide a cross-runtime ratio against the more conservative of the two engines' in-process A/A
 * floors. The runtimes cannot share one binary, so each measures its own identical arm twice inside
 * one invocation; the headline claim must clear both bootstrap median CIs by a 2x margin.
 */
function medianCiGate(claimRatio, controls) {
  const complete =
    Number.isFinite(claimRatio) &&
    claimRatio > 0 &&
    controls.length > 0 &&
    controls.every(
      (control) =>
        control?.sufficient === true &&
        Number.isFinite(control.half_width) &&
        Number.isFinite(control.ci95_lo) &&
        Number.isFinite(control.ci95_hi),
    );
  if (!complete) {
    return {
      verdict: 'fail',
      rule: 'null_ci95_2x_margin',
      cv_gate: 'never',
      reason: 'missing or insufficient same-invocation A/A null control',
      claim_ratio: claimRatio,
      claim_magnitude: null,
      null_radius: null,
      min_decidable_2x: null,
    };
  }
  const claimMagnitude = Math.max(claimRatio, 1 / claimRatio);
  const nullRadius = Math.max(...controls.map((control) => control.half_width));
  const minDecidable = Math.max(MIN_CLAIM_RATIO, 1 + 2 * nullRadius);
  return {
    verdict: claimMagnitude >= minDecidable ? 'pass' : 'fail',
    rule: 'null_ci95_2x_margin',
    cv_gate: 'never',
    reason: claimMagnitude >= minDecidable ? null : 'claim does not clear 2x the A/A median-CI radius',
    claim_ratio: claimRatio,
    claim_magnitude: claimMagnitude,
    null_radius: nullRadius,
    min_decidable_2x: minDecidable,
  };
}

/**
 * Guard a sequential cross-runtime comparison against host drift by measuring the same Rust ELF
 * on both sides of the Chromium phase. The two Rust observations must agree within their own A/A
 * median-CI floor; the slower observation is always the denominator used for the public ratio.
 */
function fmBracket(before, after) {
  const controls = [before?.null_control, after?.null_control];
  const complete =
    before?.status === 'ok' &&
    after?.status === 'ok' &&
    Number.isFinite(before?.pipeline_ns?.p50) &&
    before.pipeline_ns.p50 > 0 &&
    Number.isFinite(after?.pipeline_ns?.p50) &&
    after.pipeline_ns.p50 > 0 &&
    controls.every(
      (control) =>
        control?.sufficient === true &&
        Number.isFinite(control.half_width) &&
        Number.isFinite(control.ci95_lo) &&
        Number.isFinite(control.ci95_hi),
    );
  if (!complete) {
    return {
      verdict: 'fail',
      rule: 'rust_pre_post_drift_inside_aa_median_ci_floor',
      cv_gate: 'never',
      reason: 'missing or insufficient bracketed Rust A/A measurement',
      before_p50_ns: before?.pipeline_ns?.p50 ?? null,
      after_p50_ns: after?.pipeline_ns?.p50 ?? null,
      selected: null,
      drift_ratio: null,
      drift_magnitude: null,
      max_decidable_2x: null,
    };
  }

  const beforeP50 = before.pipeline_ns.p50;
  const afterP50 = after.pipeline_ns.p50;
  const driftRatio = afterP50 / beforeP50;
  const driftMagnitude = Math.max(driftRatio, 1 / driftRatio);
  const nullRadius = Math.max(...controls.map((control) => control.half_width));
  const maxDecidable = Math.max(MIN_CLAIM_RATIO, 1 + 2 * nullRadius);
  const selected = beforeP50 >= afterP50 ? 'before' : 'after';
  return {
    verdict: driftMagnitude <= maxDecidable ? 'pass' : 'fail',
    rule: 'rust_pre_post_drift_inside_aa_median_ci_floor',
    cv_gate: 'never',
    reason: driftMagnitude <= maxDecidable ? null : 'Rust phase drift clears its A/A median-CI floor',
    before_p50_ns: beforeP50,
    after_p50_ns: afterP50,
    selected,
    drift_ratio: driftRatio,
    drift_magnitude: driftMagnitude,
    null_radius: nullRadius,
    max_decidable_2x: maxDecidable,
  };
}

if (has('self-test')) {
  const perfect = { sufficient: true, n: 9, ci95_lo: 1, ci95_hi: 1, half_width: 0 };
  const noisy = { sufficient: true, n: 9, ci95_lo: 0.98, ci95_hi: 1.02, half_width: 0.02 };
  const cases = [
    [1.009, [perfect, perfect], 'fail'],
    [1.01, [perfect, perfect], 'pass'],
    [1.039, [perfect, noisy], 'fail'],
    [1.04, [perfect, noisy], 'pass'],
    [2, [perfect, null], 'fail'],
  ];
  for (const [ratio, controls, want] of cases) {
    const got = medianCiGate(ratio, controls).verdict;
    if (got !== want) throw new Error(`median-CI gate regression: ratio=${ratio} want=${want} got=${got}`);
  }
  const validElf = { record: 'binary', elf_sha256: 'a'.repeat(64), elf_bytes: 1 };
  if (!validElfSelfReport(validElf) || validElfSelfReport({ ...validElf, elf_sha256: 'unavailable' })) {
    throw new Error('executing-ELF self-report validation regression');
  }
  const bracketRecord = (p50, nullControl = perfect) => ({
    status: 'ok',
    pipeline_ns: { p50 },
    null_control: nullControl,
  });
  const bracketCases = [
    [bracketRecord(100), bracketRecord(101), 'pass'],
    [bracketRecord(100), bracketRecord(104, noisy), 'pass'],
    [bracketRecord(100), bracketRecord(105, noisy), 'fail'],
    [bracketRecord(100), bracketRecord(101, null), 'fail'],
  ];
  for (const [before, after, want] of bracketCases) {
    const got = fmBracket(before, after).verdict;
    if (got !== want) throw new Error(`Rust bracket gate regression: want=${want} got=${got}`);
  }
  console.log(JSON.stringify({
    self_test: 'ok',
    median_ci_cases: cases.length,
    bracket_cases: bracketCases.length,
    elf_self_report_gate: 'required',
    rust_pre_post_bracket_gate: 'required',
    cv_gate: 'never',
  }));
  process.exit(0);
}

// ---------------------------------------------------------------- corpus

const corpus = generateAll();

if (has('update-pins')) {
  PINS.corpus_sha256 = Object.fromEntries([...corpus].map(([id, v]) => [id, v.sha256]));
  writeFileSync(PINS_PATH, `${JSON.stringify(PINS, null, 2)}\n`);
  console.error(`[run] wrote ${corpus.size} corpus hashes to pins.json`);
  process.exit(0);
}

const pinned = PINS.corpus_sha256 ?? {};
const drift = [];
for (const [id, v] of corpus) {
  if (!pinned[id]) drift.push(`${id}: not pinned`);
  else if (pinned[id] !== v.sha256) drift.push(`${id}: pinned ${pinned[id].slice(0, 12)} != generated ${v.sha256.slice(0, 12)}`);
}
if (drift.length > 0) {
  console.error('[run] corpus drift detected -- the baseline would move silently:');
  for (const d of drift) console.error(`       ${d}`);
  console.error('[run] if the change is intentional, re-pin with: node scripts/headtohead/run.mjs --update-pins');
  process.exit(3);
}

const only = arg('only');
const repsScale = Number(arg('reps-scale', '1'));
// Scales the per-item mermaid wall budget (see `js_budget_ms` in corpus.mjs). A DNF is only a claim
// about mermaid if the budget was generous, so smoke runs must say so by shrinking it explicitly.
const budgetScale = Number(arg('js-budget-scale', '1'));
const outDir = arg('out', join(REPO, '.benchmarks', 'headtohead'));
mkdirSync(outDir, { recursive: true });

// `--only` takes one id or a comma-separated list, so a class of items can be re-run without
// re-running the whole corpus (the XL items cost minutes each).
const onlyIds = only ? new Set(only.split(',').map((s) => s.trim())) : null;
const items = CORPUS.filter((i) => !onlyIds || onlyIds.has(i.id));
if (onlyIds && items.length === 0) {
  console.error(`[run] --only ${only} matched no corpus item`);
  process.exit(2);
}

function positiveIntList(raw) {
  if (raw === null) return [];
  const values = raw.split(',').map((value) => Number(value.trim()));
  if (
    values.length === 0 ||
    values.some((value) => !Number.isSafeInteger(value) || value < 1) ||
    new Set(values).size !== values.length
  ) {
    console.error(`[run] --thread-sweep must be a unique comma-separated list of positive integers, got ${raw}`);
    process.exit(2);
  }
  return values;
}

const threadSweep = positiveIntList(arg('thread-sweep'));
if (threadSweep.length > 0) {
  if (items.length !== 1) {
    console.error('[run] --thread-sweep requires --only to select exactly one corpus item');
    process.exit(2);
  }
  if (!threadSweep.includes(1)) {
    console.error('[run] --thread-sweep must include 1 for the scalar byte-identity reference');
    process.exit(2);
  }
}

const corpusJson = items.map((i) => ({
  id: i.id,
  texts: corpus.get(i.id).texts,
  reps: Math.max(1, Math.round(i.reps_rs * repsScale)),
  warmup: Math.max(1, Math.round(i.warmup_rs * repsScale)),
}));
// Generated input, not evidence: keep it out of the repo.
const corpusPath = join(tmpdir(), `fm-h2h-corpus-${process.pid}.json`);
writeFileSync(corpusPath, JSON.stringify(corpusJson));

// ---------------------------------------------------------------- run both engines

function runJsonl(label, cmd, args, extraEnv = {}) {
  console.error(`[run] ${label}: ${cmd} ${args.join(' ')}`);
  const res = spawnSync(cmd, args, {
    encoding: 'utf8',
    maxBuffer: 256 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'inherit'],
    env: { ...process.env, ...extraEnv },
  });
  const records = (res.stdout ?? '')
    .split('\n')
    .filter((l) => l.trim().startsWith('{'))
    .map((l) => JSON.parse(l));
  return { records, code: res.status ?? -1 };
}

const fmBin = arg('fm-bin');
if (!fmBin) {
  console.error('[run] --fm-bin <path> is required (build: cargo build --release -p frankenmermaid-cli --example headtohead)');
  process.exit(2);
}

const env = fingerprint();
console.error(`[run] rev=${env.git_rev?.slice(0, 8)}${env.git_dirty ? '-dirty' : ''} load1=${env.loadavg_1m.toFixed(2)} cpus=${env.cpu_count}`);
if (threadSweep.some((threads) => threads > env.cpu_count)) {
  console.error(
    `[run] --thread-sweep requests ${Math.max(...threadSweep)} threads, but this host reports only ${env.cpu_count} logical CPUs`,
  );
  process.exit(2);
}
env.thread_sweep = threadSweep.length > 0
  ? {
      threads: threadSweep,
      local_machine_required: true,
      scalar_reference_threads: 1,
      parallel_executor: 'rayon_persistent_pool',
      incumbent_executor: 'single_page_main_thread',
      min_sample_ns: THREAD_SWEEP_MIN_SAMPLE_NS,
      calibration_target_ns: THREAD_SWEEP_CALIBRATION_TARGET_NS,
    }
  : null;

// CPU pinning for the frankenmermaid runner only (Chromium is multi-process; pinning it would be
// unfair to mermaid, and we would rather understate our margin than overstate it).
const pinArg = arg('pin-cpu', 'auto');
if (threadSweep.length > 0 && pinArg !== 'off') {
  console.error('[run] --thread-sweep requires --pin-cpu off; a one-CPU affinity mask invalidates scaling evidence');
  process.exit(2);
}
let pin = null;
if (pinArg !== 'off') {
  pin = pinArg === 'auto' ? pickIdleCpu() : { cpu: Number(pinArg), busy_pct: null };
  console.error(`[run] pinning frankenmermaid to cpu${pin.cpu}${pin.busy_pct === null ? '' : ` (busy ${pin.busy_pct}%)`}`);
}
env.pinned_cpu = pin;

const [fmCmd, fmArgs] = pin ? ['taskset', ['-c', String(pin.cpu), fmBin, corpusPath]] : [fmBin, [corpusPath]];
// ARM-ASYMMETRY GUARD (trap 3). The two engines are separate runtimes -- a Rust process and
// Chromium -- so they cannot be interleaved inside one measured routine the way a same-binary A/B
// can be. They run in sequence, which means host load drifting between the two phases biases the
// ratio directly, and not symmetrically: frankenfs measured its C arm degrading ~3x harder than its
// own under load, which inflated the ratio in its favour. Neither engine's internal A/A can see
// this, because each null is measured entirely inside its own phase.
//
// So sample machine busyness across each phase and report the asymmetry. A run whose phases saw
// materially different load is not a clean comparison regardless of what the per-engine nulls say.
const phaseLoad = [];

/** Aggregate busy/total jiffies across all CPUs, from /proc/stat. Passive: no busy-wait. */
function cpuTotals() {
  const line = readFileSync('/proc/stat', 'utf8').split('\n').find((l) => /^cpu\s/.test(l));
  const n = line.trim().split(/\s+/).slice(1, 9).map(Number);
  const total = n.reduce((a, b) => a + b, 0);
  const idle = n[3] + n[4];
  return { total, idle };
}

/**
 * Measure machine busyness ACROSS the phase, not at its endpoints.
 *
 * The first version of this guard compared `loadavg()[0]` before and after each phase. That is the
 * wrong instrument here: our arm takes ~19 s and mermaid's ~841 s, and a 1-minute load average
 * sampled around a 19 s phase describes the minute *preceding* it. On a box that is quieting down
 * -- which it was, 21.8 -> 6.6 over the run -- that alone manufactures an apparent asymmetry.
 * /proc/stat deltas are exact over whatever interval they span, so they compare like with like
 * regardless of how differently long the two phases are.
 */
function timedPhase(label, fn) {
  const c0 = cpuTotals();
  const t0 = Date.now();
  const out = fn();
  const seconds = (Date.now() - t0) / 1000;
  const c1 = cpuTotals();
  const dTotal = Math.max(1, c1.total - c0.total);
  const busy = 1 - (c1.idle - c0.idle) / dTotal;
  phaseLoad.push({ phase: label, seconds, busy_fraction: Number(busy.toFixed(4)) });
  return out;
}

function runFrankenmermaidPhase(prefix, sweepOrder = threadSweep) {
  if (threadSweep.length === 0) {
    return timedPhase(prefix, () => runJsonl(prefix, fmCmd, fmArgs));
  }
  const records = [];
  let code = 0;
  for (const threads of sweepOrder) {
    const phase = `${prefix}-t${threads}`;
    const result = timedPhase(
      phase,
      () =>
        runJsonl(phase, fmCmd, fmArgs, {
          FM_H2H_THREADS: String(threads),
          FM_H2H_MIN_SAMPLE_NS: String(THREAD_SWEEP_MIN_SAMPLE_NS),
        }),
    );
    records.push(
      ...result.records.map((record) => ({
        ...record,
        harness_phase: phase,
      })),
    );
    if (result.code !== 0) code = result.code;
  }
  return { records, code };
}

const fmBefore = runFrankenmermaidPhase('frankenmermaid-before');
// The measured binary hashes itself and prints that as its first record. A sha computed by a shell
// step next to the run proves nothing about which ELF executed -- rch builds into an opaque
// per-worker target dir, and agents have edited crates mid-benchmark in this fleet.
const binaryRecordsBefore = fmBefore.records.filter((record) => record.record === 'binary');
const binaryRecordBefore = binaryRecordsBefore[0];
env.fm_elf_sha256 = binaryRecordBefore?.elf_sha256 ?? 'not reported';
env.fm_elf_bytes = binaryRecordBefore?.elf_bytes ?? null;
console.error(`[run] fm elf sha256=${String(env.fm_elf_sha256).slice(0, 16)} (${env.fm_elf_bytes} bytes)`);
const expectedBinaryReports = threadSweep.length > 0 ? threadSweep.length : 1;
const beforeReportedThreads = new Set(
  binaryRecordsBefore.map((record) => record.worker_threads),
);
const elfSelfReportBeforeValid =
  binaryRecordsBefore.length === expectedBinaryReports &&
  (threadSweep.length === 0 ||
    (beforeReportedThreads.size === threadSweep.length &&
      threadSweep.every((threads) => beforeReportedThreads.has(threads)))) &&
  binaryRecordsBefore.every(
    (record) =>
      validElfSelfReport(record) &&
      record.elf_sha256 === binaryRecordBefore?.elf_sha256 &&
      record.elf_bytes === binaryRecordBefore?.elf_bytes &&
      (threadSweep.length === 0 ||
        record.min_sample_ns === THREAD_SWEEP_MIN_SAMPLE_NS) &&
      (threadSweep.length === 0 ||
        record.calibration_target_ns === THREAD_SWEEP_CALIBRATION_TARGET_NS) &&
      (threadSweep.length === 0 || threadSweep.includes(record.worker_threads)),
  );
if (!elfSelfReportBeforeValid) {
  console.error('[run] INVALID: every frankenmermaid sweep arm must self-report the same executing ELF');
}

const mjsArgs = [join(HERE, 'mermaid_bench.mjs')];
if (only) mjsArgs.push('--only', only);
if (repsScale !== 1) mjsArgs.push('--reps-scale', String(repsScale));
if (budgetScale !== 1) mjsArgs.push('--js-budget-scale', String(budgetScale));
const mjs = has('skip-mermaid')
  ? { records: [], code: 0 }
  : timedPhase('mermaid-js', () => runJsonl('mermaid-js', process.execPath, mjsArgs));
const fmAfter = has('skip-mermaid')
  ? fmBefore
  // Mirror the before sweep around the incumbent phase. With the same order on both sides, t1
  // would be far before Chromium but immediately after its process teardown, while t64 would have
  // the opposite placement. Reversing makes each width's two observations equally placed around
  // the comparator and removes that avoidable bracket asymmetry without weakening any gate.
  : runFrankenmermaidPhase('frankenmermaid-after', [...threadSweep].reverse());
const binaryRecordsAfter = fmAfter.records.filter((record) => record.record === 'binary');
const binaryRecordAfter = binaryRecordsAfter[0];
const afterReportedThreads = new Set(
  binaryRecordsAfter.map((record) => record.worker_threads),
);
const elfSelfReportAfterValid =
  binaryRecordsAfter.length === expectedBinaryReports &&
  (threadSweep.length === 0 ||
    (afterReportedThreads.size === threadSweep.length &&
      threadSweep.every((threads) => afterReportedThreads.has(threads)))) &&
  binaryRecordsAfter.every(
    (record) =>
      validElfSelfReport(record) &&
      record.elf_sha256 === binaryRecordBefore?.elf_sha256 &&
      record.elf_bytes === binaryRecordBefore?.elf_bytes &&
      (threadSweep.length === 0 ||
        record.min_sample_ns === THREAD_SWEEP_MIN_SAMPLE_NS) &&
      (threadSweep.length === 0 ||
        record.calibration_target_ns === THREAD_SWEEP_CALIBRATION_TARGET_NS) &&
      (threadSweep.length === 0 || threadSweep.includes(record.worker_threads)),
  );
const sameElf =
  elfSelfReportBeforeValid &&
  elfSelfReportAfterValid &&
  binaryRecordBefore.elf_sha256 === binaryRecordAfter.elf_sha256 &&
  binaryRecordBefore.elf_bytes === binaryRecordAfter.elf_bytes;
env.fm_bracket_elf_sha256 = binaryRecordAfter?.elf_sha256 ?? 'not reported';
env.fm_bracket_elf_bytes = binaryRecordAfter?.elf_bytes ?? null;
env.fm_bracket_same_elf = sameElf;
if (!sameElf) {
  console.error('[run] INVALID: Rust before/after arms did not self-report the same executing ELF');
}

/**
 * Global CPU busy is useful provenance, but it cannot be a numeric gate here: the phases have very
 * different durations and include the engines' own CPU consumption. The blocking host-drift check
 * is the same-ELF Rust-before/Rust-after bracket above.
 */
function armAsymmetry() {
  const phases = phaseLoad;
  if (phases.length < 2) return null;
  const lo = Math.min(...phases.map((phase) => phase.busy_fraction));
  const hi = Math.max(...phases.map((phase) => phase.busy_fraction));
  const ratio = lo > 0 ? hi / lo : null;
  const heavier = phases.reduce((a, b) => (a.busy_fraction >= b.busy_fraction ? a : b));
  return {
    phases: phaseLoad,
    busy_ratio: ratio,
    heavier_phase: heavier.phase,
    verdict: 'provenance_only',
    gate: 'never',
    rule: 'global phase CPU busy is provenance; numeric gate uses bracketed Rust A/A',
  };
}

// ---------------------------------------------------------------- join + gate

const byId = (recs) => new Map(recs.map((r) => [r.id, r]));
const fmKey = (id, threads) =>
  threadSweep.length > 0 ? `${id}@t${threads}` : id;
const byFmKey = (recs) =>
  new Map(
    recs
      .filter((record) => record.record !== 'binary')
      .map((record) => [fmKey(record.id, record.worker_threads), record]),
  );
const fmBeforeById = byFmKey(fmBefore.records);
const fmAfterById = byFmKey(fmAfter.records);
const mjsById = byId(mjs.records);

function phaseSweepIdentity(records) {
  if (threadSweep.length === 0) return null;
  const checks = [];
  for (const item of items) {
    const scalar = records.find(
      (record) =>
        record.id === item.id &&
        record.worker_threads === 1 &&
        record.status === 'ok',
    );
    for (const threads of threadSweep) {
      const record = records.find(
        (candidate) =>
          candidate.id === item.id &&
          candidate.worker_threads === threads &&
          candidate.status === 'ok',
      );
      checks.push({
        id: item.id,
        threads,
        present: Boolean(record),
        input_matches_scalar: Boolean(
          scalar && record && record.input_sha256 === scalar.input_sha256,
        ),
        default_output_matches_scalar: Boolean(
          scalar && record && record.output_sha256 === scalar.output_sha256,
        ),
        lean_output_matches_scalar: Boolean(
          scalar && record && record.output_sha256_lean === scalar.output_sha256_lean,
        ),
      });
    }
  }
  const verdict = checks.every(
    (check) =>
      check.present &&
      check.input_matches_scalar &&
      check.default_output_matches_scalar &&
      check.lean_output_matches_scalar,
  )
    ? 'pass'
    : 'fail';
  return {
    verdict,
    rule: 'every pooled thread count must match the scalar input/default/lean SHA-256',
    scalar_threads: 1,
    checks,
  };
}

const sweepOutputIdentity = threadSweep.length > 0
  ? {
      before: phaseSweepIdentity(fmBefore.records),
      after: phaseSweepIdentity(fmAfter.records),
    }
  : null;
if (sweepOutputIdentity) {
  sweepOutputIdentity.verdict =
    sweepOutputIdentity.before.verdict === 'pass' &&
    sweepOutputIdentity.after.verdict === 'pass'
      ? 'pass'
      : 'fail';
}

const measurements = items.flatMap((item) =>
  (threadSweep.length > 0 ? threadSweep : [null]).map((threads) => ({ item, threads })),
);
const rows = [];
let hardFail = !sameElf || sweepOutputIdentity?.verdict === 'fail';

for (const { item, threads } of measurements) {
  const key = fmKey(item.id, threads);
  const fBefore = fmBeforeById.get(key);
  const fAfter = fmAfterById.get(key);
  const m = mjsById.get(item.id);
  const row = {
    id: item.id,
    fm_worker_threads: threads ?? fBefore?.worker_threads ?? 1,
  };

  if (!fBefore || fBefore.status !== 'ok' || !fAfter || fAfter.status !== 'ok') {
    hardFail = true;
    rows.push({
      ...row,
      status: 'error',
      engine: 'frankenmermaid',
      error: fBefore?.error ?? fAfter?.error ?? 'missing before/after result',
    });
    continue;
  }
  if (
    threadSweep.length > 0 &&
    [fBefore, fAfter].some(
      (record) =>
        record.min_sample_ns !== THREAD_SWEEP_MIN_SAMPLE_NS ||
        record.calibration_target_ns !== THREAD_SWEEP_CALIBRATION_TARGET_NS ||
        !Number.isSafeInteger(record.batch) ||
        record.batch < 1 ||
        record.batch * record.pipeline_ns.p50 < THREAD_SWEEP_MIN_SAMPLE_NS,
    )
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'sample_floor_violation',
      error:
        `every Rust bracket arm must integrate for at least ` +
        `${THREAD_SWEEP_MIN_SAMPLE_NS} ns at its p50`,
    });
    continue;
  }
  if (
    fBefore.input_sha256 !== fAfter.input_sha256 ||
    fBefore.output_sha256 !== fAfter.output_sha256 ||
    fBefore.output_sha256_lean !== fAfter.output_sha256_lean
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'rust_bracket_mismatch',
      error: 'Rust before/after arms did not produce byte-identical input and outputs',
    });
    continue;
  }
  const bracket = fmBracket(fBefore, fAfter);
  const f = bracket.selected === 'after' ? fAfter : fBefore;
  row.fm_bracket = bracket;
  row.fm_execution_model = f.execution_model ?? 'scalar';
  row.fm_available_parallelism = f.available_parallelism ?? null;
  row.fm_min_sample_ns = f.min_sample_ns ?? null;
  row.fm_calibration_target_ns = f.calibration_target_ns ?? null;
  row.fm_batch = f.batch;
  row.fm_integrated_sample_ns = f.batch * f.pipeline_ns.p50;
  row.fm_output_sha256 = f.output_sha256;
  row.fm_output_sha256_lean = f.output_sha256_lean;
  row.class = item.class ?? 'single';
  // For a doc build the batch total is the size that means anything; for everything else it is the
  // largest single diagram. Both are recorded either way.
  row.nodes = row.class === 'doc_build' ? (f.nodes_total ?? f.nodes) : f.nodes;
  row.edges = row.class === 'doc_build' ? (f.edges_total ?? f.edges) : f.edges;
  row.nodes_max = f.nodes;
  row.edges_max = f.edges;
  row.nodes_total = f.nodes_total ?? f.nodes;
  row.edges_total = f.edges_total ?? f.edges;
  row.revisions = f.revisions;
  row.fm_p50_ns = f.pipeline_ns.p50;
  row.fm_min_ns = f.pipeline_ns.min;
  row.fm_cv_pct = f.cv_pct;
  row.fm_mad_pct = f.mad_pct;
  row.fm_null_control = f.null_control ?? null;
  row.fm_profile_ab = f.profile_ab ?? null;
  row.fm_bytes = f.output_bytes;
  row.fm_bytes_lean = f.output_bytes_lean;
  row.fm_lean_p50_ns = f.pipeline_lean_ns.p50;
  row.fm_documents_per_second = (f.revisions * 1e9) / f.pipeline_ns.p50;
  // Recorded because it is currently > 1: the lean output profile is smaller but *slower*, since
  // A11yConfig::none() drops off the streaming fast path onto the per-element Element builder.
  row.lean_slowdown = f.pipeline_lean_ns.p50 / f.pipeline_ns.p50;

  if (has('skip-mermaid')) {
    rows.push({ ...row, status: 'fm_only' });
    continue;
  }
  // A did-not-finish is a *result*: mermaid was given a wall budget at this size and did not
  // produce a render inside it. We record the budget and derive a lower bound on the speedup; we
  // never invent a time for mermaid, and DNF rows stay out of the ratio aggregate below because a
  // bound and a point estimate do not belong in the same median.
  if (m && m.status === 'dnf') {
    row.status = 'comparator_dnf';
    row.mjs_dnf_kind = m.kind ?? 'timeout';
    row.mjs_budget_ms = m.budget_ms;
    row.mjs_elapsed_ms = m.elapsed_ms;
    row.mjs_dnf_phase = m.phase;
    row.error = m.error;
    // Only a timeout bounds the ratio: mermaid was still working when the budget expired, so its
    // render costs at least the budget. A `failed` DNF bounds nothing -- mermaid raised, and waiting
    // longer would not have produced an SVG. Reporting a bound there would be inventing a number.
    row.speedup_lower_bound = row.mjs_dnf_kind === 'timeout'
      ? (m.budget_ms * 1e6) / f.pipeline_ns.p50
      : null;
    // A DNF is a completion result, not a timing-ratio claim. There is no comparator median and
    // therefore no ratio for a median-CI gate to decide.
    row.median_ci_gate = {
      verdict: 'not_applicable',
      rule: 'dnf_has_no_point_ratio',
      cv_gate: 'never',
    };
    rows.push(row);
    continue;
  }
  if (!m || m.status !== 'ok') {
    hardFail = true;
    rows.push({ ...row, status: 'comparator_error', error: m?.error ?? 'no result' });
    continue;
  }
  if (f.input_sha256 !== m.input_sha256) {
    hardFail = true;
    rows.push({ ...row, status: 'input_mismatch', error: `fm ${f.input_sha256.slice(0, 12)} != mjs ${m.input_sha256.slice(0, 12)}` });
    continue;
  }
  if (
    threadSweep.length > 0 &&
    (m.worker_threads !== 1 || m.execution_model !== 'single_page_main_thread')
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'comparator_execution_model_mismatch',
      error:
        `thread sweep requires mermaid-js worker_threads=1 and ` +
        `execution_model=single_page_main_thread; got ${m.worker_threads}/` +
        `${m.execution_model}`,
    });
    continue;
  }

  row.mjs_p50_ns = m.render_ns.p50;
  row.mjs_min_ns = m.render_ns.min;
  row.mjs_cv_pct = m.cv_pct;
  row.mjs_mad_pct = m.mad_pct;
  row.mjs_null_control = m.null_control ?? null;
  row.mjs_worker_threads = m.worker_threads ?? 1;
  row.mjs_execution_model = m.execution_model ?? 'single_page_main_thread';
  row.mjs_bytes = m.output_bytes;
  row.mjs_documents_per_second = (m.revisions * 1e9) / m.render_ns.p50;
  row.speedup = m.render_ns.p50 / f.pipeline_ns.p50;
  // Noise is one-sided, so the min-vs-min ratio is the estimate least contaminated by preemption.
  // If it disagrees with the p50 ratio, the run was noisy and the claim is not robust.
  row.speedup_min = m.render_ns.min / f.pipeline_ns.min;
  row.speedup_lean = m.render_ns.p50 / f.pipeline_lean_ns.p50;
  row.bytes_ratio = m.output_bytes / f.output_bytes;
  row.bytes_ratio_lean = m.output_bytes / f.output_bytes_lean;
  if (f.revisions > 1) {
    // For an editing session the number that matters is the cost of one keystroke's re-render,
    // not the cost of the whole trace.
    row.fm_ns_per_revision = f.pipeline_ns.p50 / f.revisions;
    row.mjs_ns_per_revision = m.render_ns.p50 / m.revisions;
  }
  // CV and MAD remain provenance only. The only blocking statistical decision is whether the
  // cross-runtime median ratio clears both same-invocation null-CI floors.
  row.median_ci_gate = medianCiGate(
    row.speedup,
    [fBefore.null_control, fAfter.null_control, m.null_control],
  );
  row.status = 'ok';
  rows.push(row);
}

if (threadSweep.length > 0) {
  for (const item of items) {
    const scalar = rows.find(
      (row) =>
        row.id === item.id &&
        row.fm_worker_threads === 1 &&
        (row.status === 'ok' || row.status === 'fm_only'),
    );
    if (!scalar) {
      hardFail = true;
      continue;
    }
    for (const row of rows.filter((candidate) => candidate.id === item.id)) {
      row.fm_scaling_vs_1t = scalar.fm_p50_ns / row.fm_p50_ns;
      row.fm_parallel_efficiency =
        row.fm_scaling_vs_1t / Math.max(1, row.fm_worker_threads);
    }
  }
}

const ok = rows.filter((r) => r.status === 'ok');
const dnf = rows.filter((r) => r.status === 'comparator_dnf');
const speedups = ok.map((r) => r.speedup);
const speedupsMin = ok.map((r) => r.speedup_min);
const rowLabel = (row) =>
  threadSweep.length > 0 ? `${row.id}@t${row.fm_worker_threads}` : row.id;
const measurementOrder = phaseLoad.map((phase) => phase.phase);
const summary = {
  schema: 'frankenmermaid.headtohead.v2',
  env,
  pins: { mermaid: PINS.mermaid.version, bundle_url: PINS.mermaid.url, security_level: PINS.mermaid.security_level },
  corpus_items: items.length,
  measurement_rows: rows.length,
  ok_items: ok.length,
  // Items where mermaid produced no render inside its wall budget. Reported separately from
  // `speedup` on purpose: these carry lower bounds, not measured ratios.
  dnf_items: dnf.length,
  dnf: dnf.map((r) => ({
    id: r.id, budget_ms: r.mjs_budget_ms, phase: r.mjs_dnf_phase,
    fm_p50_ns: r.fm_p50_ns, speedup_lower_bound: r.speedup_lower_bound, error: r.error,
  })),
  median_ci_gate_rule: 'claim magnitude >= max(1.01, 1 + 2 * max(per-engine A/A CI radius))',
  measurement_order: measurementOrder,
  fm_bracket_gate_rule: 'Rust pre/post drift magnitude <= max(1.01, 1 + 2 * Rust A/A CI radius)',
  fm_bracket_gate_failures: ok
    .filter((r) => r.fm_bracket.verdict === 'fail')
    .map(rowLabel),
  thread_sweep: threadSweep.length > 0
    ? {
        threads: threadSweep,
        scalar_output_identity: sweepOutputIdentity,
        comparison_scope:
          `same ${corpusJson[0].texts.length}-document ${items[0].id} workload; ` +
          'frankenmermaid caller pool vs mermaid-js single-page main-thread API',
        incumbent: {
          name: 'mermaid-js',
          version: PINS.mermaid.version,
          worker_threads: 1,
          execution_model: 'single_page_main_thread',
        },
        corpus_aggregate: false,
      }
    : null,
  arm_asymmetry: armAsymmetry(),
  cv_gate: 'never',
  median_ci_gate_failures: ok
    .filter((r) => r.median_ci_gate.verdict === 'fail')
    .map(rowLabel),
  speedup: speedups.length
    ? { min: Math.min(...speedups), median: pct(50, speedups), max: Math.max(...speedups) }
    : null,
  speedup_min_estimator: speedupsMin.length
    ? { min: Math.min(...speedupsMin), median: pct(50, speedupsMin), max: Math.max(...speedupsMin) }
    : null,
  rows,
};

const stamp = `${env.git_rev?.slice(0, 8) ?? 'nogit'}-${Date.now()}`;
const jsonlPath = join(outDir, `run-${stamp}.jsonl`);
const tagPhase = (records, phase) =>
  records.map((record) => ({
    ...record,
    harness_phase: record.harness_phase ?? phase,
  }));
const events = has('skip-mermaid')
  ? tagPhase(fmBefore.records, 'frankenmermaid-before')
  : [
      ...tagPhase(fmBefore.records, 'frankenmermaid-before'),
      ...tagPhase(mjs.records, 'mermaid-js'),
      ...tagPhase(fmAfter.records, 'frankenmermaid-after'),
    ];
writeFileSync(jsonlPath, events.map((r) => JSON.stringify(r)).join('\n') + '\n');
writeFileSync(join(outDir, `summary-${stamp}.json`), `${JSON.stringify(summary, null, 2)}\n`);

// ---------------------------------------------------------------- report

const ms = (ns) => (ns / 1e6).toFixed(3);
const pad = (s, n) => String(s).padEnd(n);
const lpad = (s, n) => String(s).padStart(n);

console.log('');
console.log(`corpus=${items.length}  mermaid=${PINS.mermaid.version} (securityLevel=${PINS.mermaid.security_level})  rev=${env.git_rev?.slice(0, 8)}`);
console.log('');
console.log(`${pad('item', 22)}${lpad('nodes', 6)}${lpad('edges', 7)}${lpad('fm p50 ms', 12)}${lpad('mermaid ms', 12)}${lpad('speedup', 10)}${lpad('(by min)', 10)}${lpad('fm mad%', 9)}${lpad('bytes x', 9)}${lpad('lean x', 8)}  gate`);
console.log('-'.repeat(116));
for (const r of rows) {
  const displayId = rowLabel(r);
  if (r.status === 'comparator_dnf') {
    const timedOut = r.mjs_dnf_kind === 'timeout';
    console.log(
      `${pad(displayId, 22)}${lpad(r.nodes, 6)}${lpad(r.edges, 7)}${lpad(ms(r.fm_p50_ns), 12)}` +
      `${lpad(timedOut ? `DNF>${(r.mjs_budget_ms / 1000).toFixed(0)}s` : 'CANNOT', 12)}` +
      `${lpad(timedOut ? `>${r.speedup_lower_bound.toFixed(0)}x` : 'n/a', 10)}` +
      `${lpad('-', 10)}${lpad(r.fm_mad_pct.toFixed(1), 9)}${lpad('-', 9)}${lpad('-', 8)}  n/a`,
    );
    continue;
  }
  if (r.status === 'fm_only') {
    console.log(
      `${pad(displayId, 22)}${lpad(r.nodes, 6)}${lpad(r.edges, 7)}` +
      `${lpad(ms(r.fm_p50_ns), 12)}${lpad('-', 12)}${lpad('-', 10)}` +
      `${lpad('-', 10)}${lpad(r.fm_mad_pct.toFixed(1), 9)}` +
      `${lpad('-', 9)}${lpad('-', 8)}  fm-only`,
    );
    continue;
  }
  if (r.status !== 'ok') {
    console.log(`${pad(displayId, 22)}  ${r.status.toUpperCase()}: ${r.error ?? ''}`);
    continue;
  }
  console.log(
    pad(displayId, 22) + lpad(r.nodes, 6) + lpad(r.edges, 7) + lpad(ms(r.fm_p50_ns), 12) + lpad(ms(r.mjs_p50_ns), 12) +
    lpad(`${r.speedup.toFixed(0)}x`, 10) + lpad(`${r.speedup_min.toFixed(0)}x`, 10) + lpad(r.fm_mad_pct.toFixed(1), 9) +
    lpad(`${r.bytes_ratio.toFixed(2)}x`, 9) + lpad(`${r.bytes_ratio_lean.toFixed(2)}x`, 8) +
    `  ${r.median_ci_gate.verdict}/${r.fm_bracket.verdict}`,
  );
}
console.log('');
if (summary.thread_sweep) {
  console.log('frankenmermaid caller-thread scaling (one persistent pool; scalar hash identity required):');
  for (const r of rows.filter(
    (row) => row.status === 'ok' || row.status === 'fm_only',
  )) {
    const versusIncumbent = Number.isFinite(r.speedup)
      ? `${r.speedup.toFixed(0)}x vs mermaid-js`
      : 'incumbent skipped';
    console.log(
      `  t${String(r.fm_worker_threads).padStart(2)}  ${ms(r.fm_p50_ns)} ms  ` +
      `${r.fm_scaling_vs_1t.toFixed(2)}x vs t1  ` +
      `${(r.fm_parallel_efficiency * 100).toFixed(1)}% efficiency  ` +
      versusIncumbent,
    );
  }
  console.log(
    `  scalar/parallel SVG identity: ${summary.thread_sweep.scalar_output_identity.verdict}`,
  );
  console.log('');
}
if (summary.speedup) {
  console.log(`speedup vs mermaid ${PINS.mermaid.version} (p50):  min ${summary.speedup.min.toFixed(0)}x  median ${summary.speedup.median.toFixed(0)}x  max ${summary.speedup.max.toFixed(0)}x`);
  console.log(`speedup vs mermaid ${PINS.mermaid.version} (min):  min ${summary.speedup_min_estimator.min.toFixed(0)}x  median ${summary.speedup_min_estimator.median.toFixed(0)}x  max ${summary.speedup_min_estimator.max.toFixed(0)}x`);
}
for (const r of ok.filter((x) => x.revisions > 1)) {
  const unit = r.class === 'doc_build' ? 'diagram' : 're-render';
  const what = r.class === 'doc_build'
    ? `${r.revisions} diagrams in one batch`
    : `${r.revisions} revisions`;
  const why = r.class === 'doc_build'
    ? 'a docs build or CI job pays this per diagram.'
    : 'a live preview redraws on every keystroke.';
  console.log(`${r.class} ${r.id}: ${what} -- per ${unit} frankenmermaid ${ms(r.fm_ns_per_revision)} ms vs mermaid ${ms(r.mjs_ns_per_revision)} ms (${why})`);
}
if (dnf.length) {
  console.log('');
  console.log(`DID NOT FINISH -- mermaid ${PINS.mermaid.version} produced no render on ${dnf.length} item(s):`);
  for (const r of dnf) {
    console.log(`  ${pad(r.id, 22)} ${r.nodes} nodes / ${r.edges} edges -- ${r.mjs_dnf_phase} phase, budget ${(r.mjs_budget_ms / 1000).toFixed(0)}s`);
    if (r.mjs_dnf_kind === 'timeout') {
      console.log(`  ${' '.repeat(22)} still working when the budget expired (${r.error}).`);
      console.log(`  ${' '.repeat(22)} frankenmermaid: ${ms(r.fm_p50_ns)} ms, so the speedup is at least ${r.speedup_lower_bound.toFixed(0)}x -- a bound, not a measurement.`);
    } else {
      console.log(`  ${' '.repeat(22)} FAILED after ${(r.mjs_elapsed_ms / 1000).toFixed(1)}s: ${r.error}`);
      console.log(`  ${' '.repeat(22)} frankenmermaid: ${ms(r.fm_p50_ns)} ms. mermaid does not render this input at any budget, so there is no ratio to state.`);
    }
  }
}
const leanSlow = ok.filter((r) => r.lean_slowdown > 1.05);
if (leanSlow.length) {
  const worst = leanSlow.reduce((a, b) => (b.lean_slowdown > a.lean_slowdown ? b : a));
  console.log(`note: the lean output profile is smaller but SLOWER on ${leanSlow.length}/${ok.length} items (worst ${worst.id}: ${worst.lean_slowdown.toFixed(2)}x) -- A11yConfig::none() falls off the streaming fast path.`);
}
if (summary.median_ci_gate_failures.length) {
  console.log(`MEDIAN-CI GATE FAIL: ${summary.median_ci_gate_failures.join(', ')}`);
}
if (summary.fm_bracket_gate_failures.length) {
  console.log(`RUST BRACKET GATE FAIL: ${summary.fm_bracket_gate_failures.join(', ')}`);
}
const asym = summary.arm_asymmetry;
if (asym) {
  console.log('');
  console.log(`PHASE-LOAD PROVENANCE (${asym.rule}): CPU-busy ratio ${asym.busy_ratio?.toFixed(2)}, heavier on ${asym.heavier_phase}.`);
}
console.log(`\nevents:  ${jsonlPath}`);
console.log(`summary: ${join(outDir, `summary-${stamp}.json`)}`);

if (hardFail || fmBefore.code !== 0 || mjs.code !== 0 || fmAfter.code !== 0) {
  console.error('\n[run] FAILED: at least one engine reported an error (see rows above)');
  process.exit(1);
}
if (summary.median_ci_gate_failures.length) process.exit(4);
if (summary.fm_bracket_gate_failures.length) process.exit(5);
