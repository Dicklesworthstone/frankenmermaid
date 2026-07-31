#!/usr/bin/env node
/**
 * Cross-engine rendered-output equivalence phase (`bd-evx6`).
 *
 * The head-to-head harness measures how much faster we are. This phase answers the question that
 * makes the measurement mean anything: **did we render the same diagram mermaid rendered?** A
 * renderer that drops an edge or a class member is faster and wrong, and every speedup published
 * without this check is a claim about two possibly-different outputs.
 *
 * It is deliberately a separate, UNTIMED phase. Dumping 500 SVGs per engine inside the timed region
 * would measure the harness's file I/O, so instead each engine renders once with dumping enabled and
 * the checker proves the bytes it inspected are the measured bytes: the concatenation of the dumped
 * revisions must hash to the same `output_sha256` that the engine reported for its timed rounds.
 * Both engines are already gated as self-deterministic, so that hash equality is what closes the
 * loop -- without it, "we checked the output" would only mean "we checked some output".
 *
 * Usage:
 *   node scripts/headtohead/equivalence.mjs --fm-bin target/release/examples/headtohead \
 *     --only ci_batch_500 [--out <dir>] [--keep-dumps]
 *
 * Exit codes: `0` every diagram equivalent · `1` an engine errored or the dump/hash linkage broke ·
 * `2` invalid arguments · `3` corpus drift · `7` the equivalence gate failed (a diagram diverged, or
 * a family that claims a Tier 2 semantic invariant could not have it decided).
 *
 * See `svg_equivalence.mjs` for exactly which invariants are checked and which are not.
 */

import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { cpus, loadavg, platform, release, tmpdir, totalmem } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { CORPUS, generateAll } from './corpus.mjs';
import { TIER2_FAMILIES, compareDiagram, summarize } from './svg_equivalence.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..', '..');
const PINS = JSON.parse(readFileSync(join(HERE, 'pins.json'), 'utf8'));

const EXIT_ENGINE_ERROR = 1;
const EXIT_BAD_ARGS = 2;
const EXIT_CORPUS_DRIFT = 3;
const EXIT_GATE_FAILED = 7;

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : fallback;
}
const has = (name) => process.argv.includes(`--${name}`);
const log = (...a) => console.error('[equiv]', ...a);
const sha256 = (text) => createHash('sha256').update(text, 'utf8').digest('hex');

function sh(cmd, args) {
  const res = spawnSync(cmd, args, { encoding: 'utf8' });
  return res.status === 0 ? (res.stdout ?? '').trim() : null;
}

/**
 * Which mermaid syntax family a revision is, read from its own source header. The family decides
 * whether Tier 2 semantic invariants are claimed, so it is derived from the input rather than
 * guessed from the output -- an engine must not get to choose how strictly it is checked.
 */
export function familyOf(source) {
  const header = source.trimStart().split('\n', 1)[0].trim().toLowerCase();
  if (/^(flowchart|graph)\b/.test(header)) return 'flowchart';
  if (/^sequencediagram\b/.test(header)) return 'sequence';
  if (/^classdiagram\b/.test(header)) return 'class';
  if (/^statediagram/.test(header)) return 'state';
  if (/^erdiagram\b/.test(header)) return 'er';
  if (/^architecture/.test(header)) return 'architecture';
  if (/^mindmap\b/.test(header)) return 'mindmap';
  if (/^journey\b/.test(header)) return 'journey';
  if (/^gantt\b/.test(header)) return 'gantt';
  if (/^pie\b/.test(header)) return 'pie';
  return 'unknown';
}

// ---------------------------------------------------------------- host provenance

function readText(path) {
  try {
    return readFileSync(path, 'utf8').trim();
  } catch {
    return null;
  }
}

function sortedUnique(values) {
  return [...new Set(values.filter((value) => typeof value === 'string' && value.length > 0))].sort();
}

function boostState() {
  const cpufreqBoost = readText('/sys/devices/system/cpu/cpufreq/boost');
  if (cpufreqBoost === '0' || cpufreqBoost === '1') {
    return {
      source: '/sys/devices/system/cpu/cpufreq/boost',
      enabled: cpufreqBoost === '1',
      raw: cpufreqBoost,
    };
  }
  const intelNoTurbo = readText('/sys/devices/system/cpu/intel_pstate/no_turbo');
  if (intelNoTurbo === '0' || intelNoTurbo === '1') {
    return {
      source: '/sys/devices/system/cpu/intel_pstate/no_turbo',
      enabled: intelNoTurbo === '0',
      raw: intelNoTurbo,
    };
  }
  return { source: null, enabled: null, raw: null };
}

function cpuPowerPolicy() {
  if (platform() !== 'linux') {
    return {
      kind: 'unsupported',
      platform: platform(),
      complete: false,
      drivers: [],
      governors: [],
      energy_performance_preferences: [],
      boost: { source: null, enabled: null, raw: null },
      policies: [],
    };
  }

  const base = '/sys/devices/system/cpu/cpufreq';
  let policyNames = [];
  try {
    policyNames = readdirSync(base)
      .filter((name) => /^policy\d+$/.test(name))
      .sort((left, right) => Number(left.slice(6)) - Number(right.slice(6)));
  } catch {
    // The completeness bit below makes missing cpufreq provenance fail closed.
  }
  const policies = policyNames.map((policy) => {
    const root = join(base, policy);
    return {
      policy,
      affected_cpus: readText(join(root, 'affected_cpus'))
        ?? readText(join(root, 'related_cpus')),
      driver: readText(join(root, 'scaling_driver')),
      governor: readText(join(root, 'scaling_governor')),
      energy_performance_preference: readText(join(root, 'energy_performance_preference')),
      scaling_min_khz: readText(join(root, 'scaling_min_freq')),
      scaling_max_khz: readText(join(root, 'scaling_max_freq')),
    };
  });
  const drivers = sortedUnique(policies.map((policy) => policy.driver));
  const governors = sortedUnique(policies.map((policy) => policy.governor));
  const energyPerformancePreferences = sortedUnique(
    policies.map((policy) => policy.energy_performance_preference),
  );
  return {
    kind: 'linux_cpufreq',
    platform: 'linux',
    complete: policies.length > 0
      && policies.every((policy) => policy.driver !== null && policy.governor !== null),
    drivers,
    governors,
    energy_performance_preferences: energyPerformancePreferences,
    boost: boostState(),
    policies,
  };
}

function cpuIsa() {
  const architecture = process.arch;
  const machine = sh('uname', ['-m']) ?? architecture;
  const featureLine = platform() === 'linux'
    ? readText('/proc/cpuinfo')
      ?.split('\n')
      .find((line) => /^(?:flags|Features)\s*:/.test(line))
    : null;
  const flags = sortedUnique(
    featureLine
      ? featureLine.slice(featureLine.indexOf(':') + 1).trim().toLowerCase().split(/\s+/)
      : [],
  );
  const featureSet = new Set(flags);
  return {
    architecture,
    machine,
    source: platform() === 'linux' ? '/proc/cpuinfo' : null,
    complete: flags.length > 0,
    flags,
    capabilities: {
      avx2: featureSet.has('avx2'),
      fma: featureSet.has('fma'),
      bmi2: featureSet.has('bmi2'),
      vaes: featureSet.has('vaes'),
      any_avx512: flags.some((flag) => flag.startsWith('avx512')),
      neon_or_asimd: featureSet.has('neon') || featureSet.has('asimd'),
    },
  };
}

/** Host identity on the artifact, so a verdict is never separable from the machine that produced it. */
function hostFingerprint() {
  const cpu = cpus();
  return {
    captured_at: new Date().toISOString(),
    host_identity: sh('hostnamectl', ['--static']) ?? sh('hostname', []) ?? 'unknown',
    git_rev: sh('git', ['-C', REPO, 'rev-parse', 'HEAD']),
    git_dirty: (sh('git', ['-C', REPO, 'status', '--porcelain', '--untracked-files=no']) ?? '').length > 0,
    kernel: release(),
    cpu_model: cpu[0]?.model ?? 'unknown',
    logical_threads: cpu.length,
    total_mem_gb: Number((totalmem() / 2 ** 30).toFixed(1)),
    loadavg_1m: loadavg()[0],
    node: process.version,
    cpu_power_policy: cpuPowerPolicy(),
    isa: cpuIsa(),
    // Equivalence is a byte-content check, so load does not threaten its validity the way it
    // threatens a timing claim. Recorded anyway: provenance is cheap, reconstruction is not.
    load_affects_verdict: false,
  };
}

// ---------------------------------------------------------------- arguments and corpus

const fmBin = arg('fm-bin');
if (!fmBin) {
  log('--fm-bin <path> is required (build strict-remote; see README)');
  process.exit(EXIT_BAD_ARGS);
}
const only = arg('only');
if (!only) {
  log('--only <corpus_id>[,<corpus_id>...] is required');
  process.exit(EXIT_BAD_ARGS);
}
const outDir = arg('out', join(REPO, '.benchmarks', 'headtohead', 'equivalence'));
mkdirSync(outDir, { recursive: true });

const corpus = generateAll();
const pinned = PINS.corpus_sha256 ?? {};
const drift = [];
for (const [id, v] of corpus) {
  if (pinned[id] && pinned[id] !== v.sha256) drift.push(`${id}: pinned != generated`);
}
if (drift.length > 0) {
  log('corpus drift detected -- an equivalence verdict about drifted input is meaningless:');
  for (const d of drift) log(`  ${d}`);
  process.exit(EXIT_CORPUS_DRIFT);
}

const onlyIds = new Set(only.split(',').map((s) => s.trim()));
const items = CORPUS.filter((i) => onlyIds.has(i.id));
if (items.length === 0) {
  log(`--only ${only} matched no corpus item`);
  process.exit(EXIT_BAD_ARGS);
}

const env = hostFingerprint();
if (!env.cpu_power_policy.complete || !env.isa.complete) {
  log('host provenance incomplete: equivalence evidence requires observed governor and ISA data');
  log(JSON.stringify({
    cpu_power_policy: env.cpu_power_policy,
    isa: env.isa,
  }));
  process.exit(EXIT_ENGINE_ERROR);
}
log(`host=${env.host_identity} rev=${env.git_rev?.slice(0, 8)}${env.git_dirty ? '-dirty' : ''} load1=${env.loadavg_1m.toFixed(2)}`);

// One render each is all this phase needs; reps are a timing device and this phase does not time.
const corpusJson = items.map((i) => ({
  id: i.id,
  texts: corpus.get(i.id).texts,
  reps: 1,
  warmup: 1,
}));
const corpusPath = join(tmpdir(), `fm-h2h-equiv-corpus-${process.pid}.json`);
writeFileSync(corpusPath, JSON.stringify(corpusJson));

const dumpRoot = mkdtempSync(join(tmpdir(), 'fm-h2h-equiv-'));
const fmDump = join(dumpRoot, 'fm');
const jsDump = join(dumpRoot, 'js');
mkdirSync(fmDump, { recursive: true });
mkdirSync(jsDump, { recursive: true });

function runJsonl(label, cmd, args, extraEnv = {}) {
  log(`${label}: ${cmd} ${args.join(' ')}`);
  const res = spawnSync(cmd, args, {
    encoding: 'utf8',
    maxBuffer: 512 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'inherit'],
    env: { ...process.env, ...extraEnv },
  });
  const records = (res.stdout ?? '')
    .split('\n')
    .filter((l) => l.trim().startsWith('{'))
    .map((l) => JSON.parse(l));
  return { records, code: res.status ?? -1 };
}

// ---------------------------------------------------------------- render both engines

const fmRun = runJsonl('frankenmermaid', fmBin, [corpusPath, fmDump], {
  FM_H2H_DUMP_ALL: '1',
  // Scalar: this phase checks content, and the sweep already proves pooled output is byte-identical
  // to the scalar arm, so checking scalar transfers to every width.
  FM_H2H_THREADS: '1',
  // Requested width is configuration, not evidence. Observe the caller threads that execute this
  // exact workload outside the measured samples so every equivalence row carries the actual count.
  FM_H2H_THREAD_PROBE: '1',
});
if (fmRun.code !== 0) {
  log(`frankenmermaid exited ${fmRun.code}`);
  process.exit(EXIT_ENGINE_ERROR);
}

// `mermaid_bench.mjs` reads repetition counts from the source corpus rather than the temporary
// one-render JSON used by the Rust executable. Scale every selected item down far enough that its
// rounded count is one; equivalence needs one deterministic render, not the timing harness's nine
// effect/null samples.
const equivalenceRepsScale = 1 / Math.max(...items.map((item) => item.reps_js));
const jsArgs = [
  '--only', [...onlyIds].join(','),
  '--reps-scale', String(equivalenceRepsScale),
  '--render-once',
  '--dump-svg', jsDump,
  '--dump-all-revisions',
];
const jsRun = runJsonl('mermaid-js', 'node', [join(HERE, 'mermaid_bench.mjs'), ...jsArgs]);
if (jsRun.code !== 0) {
  log(`mermaid_bench exited ${jsRun.code}`);
  process.exit(EXIT_ENGINE_ERROR);
}

const binaryRecord = fmRun.records.find((r) => r.record === 'binary');
const fmById = new Map(fmRun.records.filter((r) => r.id && r.record !== 'binary').map((r) => [r.id, r]));
const jsById = new Map(jsRun.records.filter((r) => r.id).map((r) => [r.id, r]));

// ---------------------------------------------------------------- compare

function readRevisions(dir, id, suffix) {
  const prefix = `${id}.rev`;
  const names = readdirSync(dir)
    .filter((n) => n.startsWith(prefix) && n.endsWith(suffix))
    .sort();
  return names.map((n) => readFileSync(join(dir, n), 'utf8'));
}

const rows = [];
let gateFailed = false;

for (const item of items) {
  const texts = corpus.get(item.id).texts;
  const fmRecord = fmById.get(item.id);
  const jsRecord = jsById.get(item.id);

  if (!fmRecord || fmRecord.status !== 'ok') {
    log(`FAIL ${item.id}: frankenmermaid status=${fmRecord?.status ?? 'missing'}`);
    process.exit(EXIT_ENGINE_ERROR);
  }
  // A comparator that cannot render cannot be checked for equivalence, and that is a legitimate
  // outcome for the XL/DNF tier -- reported, not silently passed.
  if (!jsRecord || jsRecord.status !== 'ok') {
    rows.push({
      id: item.id,
      status: 'incumbent_did_not_render',
      incumbent_status: jsRecord?.status ?? 'missing',
      incumbent_detail: jsRecord?.dnf ?? jsRecord?.error ?? null,
      equivalence: null,
      note: 'no equivalence claim is possible for an item mermaid-js did not render',
    });
    log(`SKIP ${item.id}: mermaid-js status=${jsRecord?.status ?? 'missing'} -- no comparison possible`);
    continue;
  }

  const execution = {
    frankenmermaid: {
      requested_worker_threads: fmRecord.thread_count_requested,
      actual_observed_worker_threads: fmRecord.thread_count_actually_used,
      execution_model: fmRecord.execution_model,
    },
    mermaid_js: {
      requested_worker_threads: jsRecord.thread_count_requested,
      actual_observed_worker_threads: jsRecord.thread_count_actually_used,
      execution_model: jsRecord.execution_model,
    },
  };
  const threadProvenanceComplete = [execution.frankenmermaid, execution.mermaid_js].every(
    (record) => Number.isSafeInteger(record.requested_worker_threads)
      && record.requested_worker_threads > 0
      && Number.isSafeInteger(record.actual_observed_worker_threads)
      && record.actual_observed_worker_threads > 0
      && record.actual_observed_worker_threads === record.requested_worker_threads
      && typeof record.execution_model === 'string'
      && record.execution_model.length > 0,
  );
  if (!threadProvenanceComplete) {
    log(`FAIL ${item.id}: actual observed worker-thread provenance is incomplete`);
    log(`  ${JSON.stringify(execution)}`);
    process.exit(EXIT_ENGINE_ERROR);
  }

  const fmSvgs = readRevisions(fmDump, item.id, '.default.svg');
  const jsSvgs = readRevisions(jsDump, item.id, '.mermaid.svg');

  // LINKAGE PROOF. Without this the phase would only show that *some* render agrees. Each engine
  // reports `output_sha256` over its timed rounds' concatenated revisions; the dumps must reproduce
  // it exactly, which -- given each engine's own determinism gate -- makes these the measured bytes.
  const linkage = {
    fm_revisions_dumped: fmSvgs.length,
    js_revisions_dumped: jsSvgs.length,
    revisions_expected: texts.length,
    fm_dump_sha256: sha256(fmSvgs.join('')),
    fm_reported_sha256: fmRecord.output_sha256,
    js_dump_sha256: sha256(jsSvgs.join('')),
    js_reported_sha256: jsRecord.output_sha256,
  };
  linkage.fm_matches_measured = linkage.fm_dump_sha256 === linkage.fm_reported_sha256;
  linkage.js_matches_measured = linkage.js_dump_sha256 === linkage.js_reported_sha256;
  linkage.counts_match = fmSvgs.length === texts.length && jsSvgs.length === texts.length;

  if (!linkage.counts_match || !linkage.fm_matches_measured || !linkage.js_matches_measured) {
    log(`FAIL ${item.id}: dumped SVGs are not provably the measured render`);
    log(`  ${JSON.stringify(linkage)}`);
    process.exit(EXIT_ENGINE_ERROR);
  }

  const results = [];
  for (let i = 0; i < texts.length; i += 1) {
    results.push(compareDiagram({
      index: i,
      family: familyOf(texts[i]),
      fmSvg: fmSvgs[i],
      jsSvg: jsSvgs[i],
      source: texts[i],
    }));
  }

  const equivalence = summarize(results);
  if (equivalence.verdict !== 'pass') gateFailed = true;
  rows.push({
    id: item.id,
    status: 'compared',
    revisions: texts.length,
    input_sha256: corpus.get(item.id).sha256,
    // Both engines' agreement on the input hash is already gated by run.mjs; repeated here so this
    // artifact stands alone as evidence.
    fm_input_sha256: fmRecord.input_sha256,
    js_input_sha256: jsRecord.input_sha256,
    inputs_identical: fmRecord.input_sha256 === jsRecord.input_sha256,
    execution,
    linkage,
    equivalence,
  });

  const fams = Object.entries(equivalence.by_family)
    .map(([f, v]) => `${f}=${v.diagrams - v.divergent - v.unverified}/${v.diagrams}`)
    .join(' ');
  log(`${equivalence.verdict === 'pass' ? 'PASS' : 'FAIL'} ${item.id}: `
    + `${equivalence.equivalent}/${equivalence.diagrams} equivalent, ${equivalence.divergent} divergent, `
    + `${equivalence.unverified} unverified | ${fams}`);
}

// ---------------------------------------------------------------- artifact

const stamp = Date.now();
const rev = env.git_rev?.slice(0, 8) ?? 'nogit';
const artifactPath = join(outDir, `equivalence-${rev}-${stamp}.json`);
const artifact = {
  schema: 'frankenmermaid.headtohead.equivalence.v1',
  env,
  method: {
    kind: 'svg_structural',
    rasterized_perceptual_diff: false,
    why_not_raster: 'the engines use different fonts, paddings and stroke widths, so a pixel diff '
      + 'would report a large distance for two correct renders -- it measures styling, not content',
    why_not_byte_equality: 'the engines emit deliberately different SVG (HTML foreignObject labels '
      + 'vs <text>, different class vocabularies, different layout engines)',
    tier1: 'rendered-text token multiset, containment-gated: every token mermaid-js renders must be '
      + 'present in ours. Applies to every syntax family. Rendering MORE than mermaid is reported, '
      + 'not failed.',
    tier2: 'rendered-path edge topology compared cross-engine AND against input-derived ground '
      + 'truth for flowchart/state, plus class relationship kind and marker-owning endpoint '
      + 'compared cross-engine AND against input-derived ground truth. Referenced marker bodies '
      + 'must encode the right diamond geometry/fill, while inheritance additionally requires a '
      + 'hollow triangle facing away from the path. Frankenmermaid endpoints are reconstructed '
      + 'geometrically; mermaid-js uses the same geometry when unambiguous and uniquely resolved '
      + 'per-path data-id endpoints otherwise. '
      + `Claimed for: ${[...TIER2_FAMILIES].join(', ')}.`,
    extractor: 'single shared implementation applied to both engines (svg_equivalence.mjs); a '
      + 'per-engine extractor pair could agree by construction',
    self_test: 'svg_equivalence.mjs --self-test: 16 mutation controls (including dropped or '
      + 'rewired edges, displaced nodes, swapped class relationship kinds, wrong owning endpoint, '
      + 'unknown markers, invalid diamond bodies, inward or filled inheritance triangles, and '
      + 'cardinality/label drift) '
      + 'and 4 negative controls (including benign path ordering and extra content)',
    undecidable_is_not_a_pass: true,
  },
  pins: { mermaid: PINS.mermaid.version, bundle_sha256: jsRun.records[0]?.bundle_sha256 ?? null },
  provenance: {
    fm_elf_sha256: binaryRecord?.elf_sha256 ?? null,
    fm_elf_bytes: binaryRecord?.elf_bytes ?? null,
    fm_execution_model: binaryRecord?.execution_model ?? null,
    chromium_binary: jsRun.records[0]?.chromium_binary ?? null,
    chromium_version: jsRun.records[0]?.chromium_version ?? null,
    mermaid_bundle_url: jsRun.records[0]?.bundle_url ?? null,
  },
  rows,
  verdict: gateFailed ? 'fail' : 'pass',
};
writeFileSync(artifactPath, `${JSON.stringify(artifact, null, 2)}\n`);
log(`artifact ${artifactPath}`);

if (!has('keep-dumps')) rmSync(dumpRoot, { recursive: true, force: true });
else log(`dumps kept at ${dumpRoot}`);

// ---------------------------------------------------------------- report

console.log('');
console.log(`equivalence  host=${env.host_identity}  mermaid@${PINS.mermaid.version}  rev=${rev}`);
console.log('method: SVG structural (text-token containment + rendered-path topology + class relationship semantics). Not a pixel diff.');
console.log('');
console.log('item                  diagrams  equiv  diverg  unver  verdict');
for (const row of rows) {
  if (row.status !== 'compared') {
    console.log(`${row.id.padEnd(21)} ${String(row.revisions ?? '-').padStart(8)}  ${'-'.padStart(5)}  ${'-'.padStart(6)}  ${'-'.padStart(5)}  ${row.status}`);
    continue;
  }
  const e = row.equivalence;
  console.log(
    `${row.id.padEnd(21)} ${String(e.diagrams).padStart(8)}  ${String(e.equivalent).padStart(5)}  `
    + `${String(e.divergent).padStart(6)}  ${String(e.unverified).padStart(5)}  ${e.verdict.toUpperCase()}`,
  );
  for (const [family, v] of Object.entries(e.by_family)) {
    const ok = v.diagrams - v.divergent - v.unverified;
    console.log(`  ${family.padEnd(14)} ${String(ok).padStart(4)}/${String(v.diagrams).padEnd(4)} equivalent`
      + `  tier2_decided=${v.tier2}`
      + (v.divergent ? `  DIVERGENT=${v.divergent}` : '')
      + (v.unverified ? `  UNVERIFIED=${v.unverified}` : ''));
  }
  for (const sample of e.divergent_samples) {
    const failing = sample.checks.filter((c) => c.pass === false && c.decided && c.gating !== false);
    console.log(`  first divergence: revision ${sample.index} (${sample.family})`);
    for (const c of failing) {
      const d = c.detail ?? {};
      const what = d.total_missing !== undefined
        ? `${d.total_missing} tokens missing from our render (${d.distinct_missing} distinct)`
        : `${d.difference_count} differences`;
      console.log(`    ${c.invariant}: ${what}`);
      for (const m of (d.missing ?? d.differences ?? []).slice(0, 6)) {
        console.log(`      ${JSON.stringify(m)}`);
      }
    }
  }
}
console.log('');
console.log(`verdict: ${artifact.verdict.toUpperCase()}`);
if (gateFailed) {
  console.log('a divergent or unverified diagram means a speedup for that item compares two');
  console.log('different renders; it is not a win until the divergence is explained or fixed.');
}
process.exit(gateFailed ? EXIT_GATE_FAILED : 0);
