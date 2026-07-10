// Driver for the pinned frankenmermaid <-> mermaid-js head-to-head (bead bd-1buv.1).
//
//   node scripts/headtohead/run.mjs --fm-bin <path/to/release/examples/headtohead>
//
// Responsibilities:
//   1. Generate the fixed corpus and verify every input against the SHA-256 pins in pins.json.
//   2. Capture an environment fingerprint (git rev, toolchain, browser, CPU, load).
//   3. Run both engines over byte-identical inputs with warmup discipline.
//   4. Join the results, apply the coefficient-of-variation gate, and compute ratios.
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

// Dispersion gate. See the `mad_pct` doc comment in crates/fm-cli/examples/headtohead.rs for why
// this gates on median absolute deviation rather than the coefficient of variation: scheduler
// preemption on a shared box adds a one-sided right tail that inflates sd without touching the
// bulk of the distribution. cv_pct is still recorded, just not gated on.
const MAD_GATE_PCT = 5.0;

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
  return {
    captured_at: new Date().toISOString(),
    git_rev: sh('git', ['-C', REPO, 'rev-parse', 'HEAD']),
    git_dirty: (sh('git', ['-C', REPO, 'status', '--porcelain']) ?? '').length > 0,
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
const outDir = arg('out', join(REPO, '.benchmarks', 'headtohead'));
mkdirSync(outDir, { recursive: true });

const items = CORPUS.filter((i) => !only || i.id === only);
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

function runJsonl(label, cmd, args) {
  console.error(`[run] ${label}: ${cmd} ${args.join(' ')}`);
  const res = spawnSync(cmd, args, { encoding: 'utf8', maxBuffer: 256 * 1024 * 1024, stdio: ['ignore', 'pipe', 'inherit'] });
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

// CPU pinning for the frankenmermaid runner only (Chromium is multi-process; pinning it would be
// unfair to mermaid, and we would rather understate our margin than overstate it).
const pinArg = arg('pin-cpu', 'auto');
let pin = null;
if (pinArg !== 'off') {
  pin = pinArg === 'auto' ? pickIdleCpu() : { cpu: Number(pinArg), busy_pct: null };
  console.error(`[run] pinning frankenmermaid to cpu${pin.cpu}${pin.busy_pct === null ? '' : ` (busy ${pin.busy_pct}%)`}`);
}
env.pinned_cpu = pin;

const [fmCmd, fmArgs] = pin ? ['taskset', ['-c', String(pin.cpu), fmBin, corpusPath]] : [fmBin, [corpusPath]];
const fm = runJsonl('frankenmermaid', fmCmd, fmArgs);
const mjsArgs = [join(HERE, 'mermaid_bench.mjs')];
if (only) mjsArgs.push('--only', only);
if (repsScale !== 1) mjsArgs.push('--reps-scale', String(repsScale));
const mjs = has('skip-mermaid') ? { records: [], code: 0 } : runJsonl('mermaid-js', process.execPath, mjsArgs);

// ---------------------------------------------------------------- join + gate

const byId = (recs) => new Map(recs.map((r) => [r.id, r]));
const fmById = byId(fm.records);
const mjsById = byId(mjs.records);

const rows = [];
let hardFail = false;

for (const item of items) {
  const f = fmById.get(item.id);
  const m = mjsById.get(item.id);
  const row = { id: item.id };

  if (!f || f.status !== 'ok') {
    hardFail = true;
    rows.push({ ...row, status: 'error', engine: 'frankenmermaid', error: f?.error ?? 'no result' });
    continue;
  }
  row.nodes = f.nodes;
  row.edges = f.edges;
  row.revisions = f.revisions;
  row.fm_p50_ns = f.pipeline_ns.p50;
  row.fm_min_ns = f.pipeline_ns.min;
  row.fm_cv_pct = f.cv_pct;
  row.fm_mad_pct = f.mad_pct;
  row.fm_bytes = f.output_bytes;
  row.fm_bytes_lean = f.output_bytes_lean;
  row.fm_lean_p50_ns = f.pipeline_lean_ns.p50;
  // Recorded because it is currently > 1: the lean output profile is smaller but *slower*, since
  // A11yConfig::none() drops off the streaming fast path onto the per-element Element builder.
  row.lean_slowdown = f.pipeline_lean_ns.p50 / f.pipeline_ns.p50;

  if (has('skip-mermaid')) {
    rows.push({ ...row, status: 'fm_only' });
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

  row.mjs_p50_ns = m.render_ns.p50;
  row.mjs_min_ns = m.render_ns.min;
  row.mjs_cv_pct = m.cv_pct;
  row.mjs_mad_pct = m.mad_pct;
  row.mjs_bytes = m.output_bytes;
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
  // Blocking on our side; advisory on mermaid's, where a 2.9 s/render item cannot afford enough
  // reps to tighten its dispersion and its variance is dwarfed by a 1000x ratio anyway.
  row.mad_gate = f.mad_pct <= MAD_GATE_PCT ? 'pass' : 'fail';
  row.comparator_mad_gate = m.mad_pct <= MAD_GATE_PCT ? 'pass' : 'warn';
  row.status = 'ok';
  rows.push(row);
}

const ok = rows.filter((r) => r.status === 'ok');
const speedups = ok.map((r) => r.speedup);
const speedupsMin = ok.map((r) => r.speedup_min);
const summary = {
  schema: 'frankenmermaid.headtohead.v1',
  env,
  pins: { mermaid: PINS.mermaid.version, bundle_url: PINS.mermaid.url, security_level: PINS.mermaid.security_level },
  corpus_items: items.length,
  ok_items: ok.length,
  mad_gate_pct: MAD_GATE_PCT,
  mad_gate_failures: ok.filter((r) => r.mad_gate === 'fail').map((r) => r.id),
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
writeFileSync(jsonlPath, [...fm.records, ...mjs.records].map((r) => JSON.stringify(r)).join('\n') + '\n');
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
  if (r.status !== 'ok') {
    console.log(`${pad(r.id, 22)}  ${r.status.toUpperCase()}: ${r.error ?? ''}`);
    continue;
  }
  console.log(
    pad(r.id, 22) + lpad(r.nodes, 6) + lpad(r.edges, 7) + lpad(ms(r.fm_p50_ns), 12) + lpad(ms(r.mjs_p50_ns), 12) +
    lpad(`${r.speedup.toFixed(0)}x`, 10) + lpad(`${r.speedup_min.toFixed(0)}x`, 10) + lpad(r.fm_mad_pct.toFixed(1), 9) +
    lpad(`${r.bytes_ratio.toFixed(2)}x`, 9) + lpad(`${r.bytes_ratio_lean.toFixed(2)}x`, 8) +
    `  ${r.mad_gate}${r.comparator_mad_gate === 'warn' ? ' (cmp noisy)' : ''}`,
  );
}
console.log('');
if (summary.speedup) {
  console.log(`speedup vs mermaid ${PINS.mermaid.version} (p50):  min ${summary.speedup.min.toFixed(0)}x  median ${summary.speedup.median.toFixed(0)}x  max ${summary.speedup.max.toFixed(0)}x`);
  console.log(`speedup vs mermaid ${PINS.mermaid.version} (min):  min ${summary.speedup_min_estimator.min.toFixed(0)}x  median ${summary.speedup_min_estimator.median.toFixed(0)}x  max ${summary.speedup_min_estimator.max.toFixed(0)}x`);
}
for (const r of ok.filter((x) => x.revisions > 1)) {
  console.log(`edit trace ${r.id}: ${r.revisions} revisions -- per re-render frankenmermaid ${ms(r.fm_ns_per_revision)} ms vs mermaid ${ms(r.mjs_ns_per_revision)} ms (a live preview redraws on every keystroke).`);
}
const leanSlow = ok.filter((r) => r.lean_slowdown > 1.05);
if (leanSlow.length) {
  const worst = leanSlow.reduce((a, b) => (b.lean_slowdown > a.lean_slowdown ? b : a));
  console.log(`note: the lean output profile is smaller but SLOWER on ${leanSlow.length}/${ok.length} items (worst ${worst.id}: ${worst.lean_slowdown.toFixed(2)}x) -- A11yConfig::none() falls off the streaming fast path.`);
}
if (summary.mad_gate_failures.length) console.log(`MAD GATE FAIL (fm mad > ${MAD_GATE_PCT}%): ${summary.mad_gate_failures.join(', ')}`);
console.log(`\nevents:  ${jsonlPath}`);
console.log(`summary: ${join(outDir, `summary-${stamp}.json`)}`);

if (hardFail || fm.code !== 0 || mjs.code !== 0) {
  console.error('\n[run] FAILED: at least one engine reported an error (see rows above)');
  process.exit(1);
}
if (summary.mad_gate_failures.length) process.exit(4);
