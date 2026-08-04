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
import { mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { cpus, hostname, loadavg, platform, release, tmpdir, totalmem } from 'node:os';
import { join, dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { CORPUS, generateAll } from './corpus.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(HERE, '..', '..');
const PINS_PATH = join(HERE, 'pins.json');
const PINS = JSON.parse(readFileSync(PINS_PATH, 'utf8'));

const MIN_CLAIM_RATIO = 1.01;
const MIN_EFFECT_SAMPLES = 9;
const EFFECT_BOOTSTRAP_RESAMPLES = 10_000;
// Clause 3 of the corrected A/A null gate: max |null median - 1|. Bounds arm-order bias without
// coupling the verdict to the null's precision.
const NULL_MEDIAN_MAX_BIAS = 0.02;
const THREAD_SWEEP_MIN_SAMPLE_NS = 50_000_000;
const THREAD_SWEEP_CALIBRATION_TARGET_NS = 75_000_000;
// `bd-ap4v` retry predicate. The first public-parser incumbent attempt integrated a 74.420081 ms
// Rust sample and failed corrected null clause 3 (Rust-before median 0.959104, 4.090% from 1.0).
// A scalar parse job is short enough that arm-order bias lands inside the sample, so the retry is
// admitted only at a predeclared 250 ms floor -- five times the sweep floor -- which is declared
// here, stamped into the artifact, and enforced against every effect AND null arm. Raising the
// floor is not a retry for a friendlier null: it changes the measured quantity, so the row is
// re-adjudicated once and a second null failure closes the environment rather than looping.
// The Rust runner derives its calibration target as floor + ceil(floor/2); this mirror must track
// that single rule exactly, or the gate rejects every record it is handed.
const PARSE_MIN_SAMPLE_NS = 250_000_000;
const PARSE_CALIBRATION_TARGET_NS =
  PARSE_MIN_SAMPLE_NS + Math.ceil(PARSE_MIN_SAMPLE_NS / 2);
const HOST_WIDE_MAX_BUSY_FRACTION = 0.20;
const HOST_WIDE_QUIET_SAMPLE_MS = 1_000;
const HOST_WIDE_QUIET_MAX_ATTEMPTS = 900;

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : fallback;
}
const has = (name) => process.argv.includes(`--${name}`);
const measurementMode = arg('mode', 'render');
if (!['render', 'parse'].includes(measurementMode)) {
  console.error(`[run] --mode must be render or parse, got ${JSON.stringify(measurementMode)}`);
  process.exit(2);
}
// One source of truth for the integration floor, so the value handed to the Rust runner, the value
// stamped into the artifact, and the value the gate re-checks can never drift apart.
const minSampleNs = () =>
  measurementMode === 'parse' ? PARSE_MIN_SAMPLE_NS : THREAD_SWEEP_MIN_SAMPLE_NS;
const calibrationTargetNs = () =>
  measurementMode === 'parse' ? PARSE_CALIBRATION_TARGET_NS : THREAD_SWEEP_CALIBRATION_TARGET_NS;

function cpuTimeSnapshot() {
  return cpus().map((record, cpu) => ({
    cpu,
    idle: record.times.idle,
    total: Object.values(record.times).reduce((sum, value) => sum + value, 0),
  }));
}

function cpuBusyFromSnapshots(before, after) {
  const afterByCpu = new Map(after.map((record) => [record.cpu, record]));
  return before.map((record) => {
    const next = afterByCpu.get(record.cpu);
    if (!next) throw new Error(`cpu${record.cpu} disappeared during busy sampling`);
    const total = Math.max(1, next.total - record.total);
    const idle = Math.max(0, next.idle - record.idle);
    return {
      cpu: record.cpu,
      busy: Math.min(1, Math.max(0, 1 - idle / total)),
    };
  });
}

/** Busy fraction of every logical CPU over an idle synchronous sampling window. */
function cpuBusy(ms) {
  const before = cpuTimeSnapshot();
  const waitCell = new Int32Array(new SharedArrayBuffer(Int32Array.BYTES_PER_ELEMENT));
  Atomics.wait(waitCell, 0, 0, ms);
  return cpuBusyFromSnapshots(before, cpuTimeSnapshot());
}

function classifyHostWideQuiescence(busyRecords, allowedCpus, limit) {
  const byCpu = new Map(busyRecords.map((record) => [record.cpu, record.busy]));
  const missingCpus = allowedCpus.filter((cpu) => !byCpu.has(cpu));
  const measuredRaw = allowedCpus
    .filter((cpu) => byCpu.has(cpu))
    .map((cpu) => ({ cpu, busy_fraction: byCpu.get(cpu) }));
  const measured = measuredRaw.map(({ cpu, busy_fraction }) => ({
    cpu,
    busy_fraction: Number(busy_fraction.toFixed(6)),
  }));
  const busyCpus = measuredRaw
    .filter((record) => record.busy_fraction > limit)
    .sort((left, right) => right.busy_fraction - left.busy_fraction)
    .map(({ cpu, busy_fraction }) => ({
      cpu,
      busy_fraction: Number(busy_fraction.toFixed(6)),
    }));
  return {
    verdict: missingCpus.length === 0 && busyCpus.length === 0 ? 'clear' : 'blocked',
    allowed_cpu_count: allowedCpus.length,
    sampled_cpu_count: measured.length,
    maximum_busy_fraction: limit,
    observed_max_busy_fraction: measuredRaw.length > 0
      ? Number(Math.max(...measuredRaw.map((record) => record.busy_fraction)).toFixed(6))
      : null,
    missing_cpus: missingCpus,
    busy_cpus_above_limit: busyCpus,
    per_cpu_busy_fraction: measured,
  };
}

function validExclusiveHostClaim(value) {
  return (
    typeof value === 'string' &&
    /^trj-booking:[1-9]\d*$/.test(value)
  );
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

function readText(path) {
  try {
    return readFileSync(path, 'utf8').trim();
  } catch {
    return null;
  }
}

function parseCpuList(raw) {
  if (!raw) return [];
  const cpus = new Set();
  for (const part of raw.split(',').map((value) => value.trim()).filter(Boolean)) {
    const match = /^(\d+)(?:-(\d+))?$/.exec(part);
    const start = Number(match?.[1]);
    const end = Number(match?.[2] ?? match?.[1]);
    if (
      !match ||
      !Number.isSafeInteger(start) ||
      !Number.isSafeInteger(end) ||
      end < start
    ) {
      throw new Error(`invalid CPU list component ${JSON.stringify(part)}`);
    }
    for (let cpu = start; cpu <= end; cpu += 1) cpus.add(cpu);
  }
  return [...cpus].sort((a, b) => a - b);
}

function parseCpuSet(raw) {
  if (!raw) return [];
  return parseCpuList(raw.trim().split(/[\s,]+/).join(','));
}

function sortedUniqueStrings(values) {
  return [...new Set(values.filter((value) => typeof value === 'string' && value.length > 0))]
    .sort();
}

function sortedUniqueNumbers(values) {
  return [...new Set(values.filter((value) => Number.isSafeInteger(value)))]
    .sort((left, right) => left - right);
}

function readInteger(path) {
  const raw = readText(path);
  if (raw === null) return null;
  const value = Number(raw);
  return Number.isSafeInteger(value) ? value : null;
}

function hostTopology(cpuRecords) {
  const online = parseCpuList(readText('/sys/devices/system/cpu/online'));
  const logicalThreads = online.length || cpuRecords.length;
  const physicalCoreKeys = new Set();
  for (const cpu of online) {
    const packageId = readText(`/sys/devices/system/cpu/cpu${cpu}/topology/physical_package_id`);
    const coreId = readText(`/sys/devices/system/cpu/cpu${cpu}/topology/core_id`);
    if (packageId !== null && coreId !== null) physicalCoreKeys.add(`${packageId}:${coreId}`);
  }
  const sysctlPhysical = Number(sh('sysctl', ['-n', 'hw.physicalcpu']));
  const physicalCores = physicalCoreKeys.size ||
    (Number.isSafeInteger(sysctlPhysical) && sysctlPhysical > 0 ? sysctlPhysical : logicalThreads);
  let numaNodes = 1;
  try {
    const nodes = readdirSync('/sys/devices/system/node').filter((name) => /^node\d+$/.test(name));
    if (nodes.length > 0) numaNodes = nodes.length;
  } catch {
    // Apple Silicon exposes unified memory rather than Linux NUMA sysfs; one visible node is the
    // portable topology contract until a platform API reports otherwise.
  }
  const status = readText('/proc/self/status');
  const statusValue = (key) =>
    status?.split('\n').find((line) => line.startsWith(key))?.slice(key.length).trim() ?? null;
  const affinityList = parseCpuList(statusValue('Cpus_allowed_list:'));
  return {
    host_identity: hostname(),
    physical_cores: physicalCores,
    logical_threads: logicalThreads,
    online_cpus: online.length > 0
      ? online
      : Array.from({ length: logicalThreads }, (_, cpu) => cpu),
    threads_per_core: physicalCores > 0 ? logicalThreads / physicalCores : null,
    ram_bytes: totalmem(),
    numa_nodes: numaNodes,
    affinity_mask: statusValue('Cpus_allowed:') ?? 'all-visible',
    affinity_cpus: affinityList.length > 0
      ? affinityList
      : Array.from({ length: logicalThreads }, (_, cpu) => cpu),
    affinity_source: affinityList.length > 0 ? 'linux_proc_status' : `${platform()}_visible_cpu_fallback`,
  };
}

function linuxBoostState() {
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

function linuxCpuPowerPolicy() {
  const base = '/sys/devices/system/cpu/cpufreq';
  const onlineCpus = parseCpuList(readText('/sys/devices/system/cpu/online'));
  let policyNames = [];
  try {
    policyNames = readdirSync(base)
      .filter((name) => /^policy\d+$/.test(name))
      .sort((left, right) => Number(left.slice(6)) - Number(right.slice(6)));
  } catch {
    // Missing cpufreq sysfs is an invalid cross-engine baseline, represented below rather than
    // silently guessed from the CPU model.
  }
  const policies = policyNames.map((policy) => {
    const root = join(base, policy);
    const affectedRaw =
      readText(join(root, 'affected_cpus')) ??
      readText(join(root, 'related_cpus'));
    return {
      policy,
      affected_cpus: parseCpuSet(affectedRaw),
      driver: readText(join(root, 'scaling_driver')),
      governor: readText(join(root, 'scaling_governor')),
      energy_performance_preference: readText(join(root, 'energy_performance_preference')),
      scaling_min_khz: readInteger(join(root, 'scaling_min_freq')),
      scaling_max_khz: readInteger(join(root, 'scaling_max_freq')),
      hardware_min_khz: readInteger(join(root, 'cpuinfo_min_freq')),
      hardware_max_khz: readInteger(join(root, 'cpuinfo_max_freq')),
    };
  });
  const coveredCpus = [...new Set(policies.flatMap((policy) => policy.affected_cpus))]
    .sort((left, right) => left - right);
  const drivers = sortedUniqueStrings(policies.map((policy) => policy.driver));
  const governors = sortedUniqueStrings(policies.map((policy) => policy.governor));
  const energyPerformancePreferences = sortedUniqueStrings(
    policies.map((policy) => policy.energy_performance_preference),
  );
  const eppPresenceConsistent =
    energyPerformancePreferences.length === 0 ||
    policies.every((policy) => typeof policy.energy_performance_preference === 'string');
  const coverageComplete =
    onlineCpus.length > 0 &&
    onlineCpus.every((cpu) => coveredCpus.includes(cpu));
  const consistent =
    policies.length > 0 &&
    drivers.length === 1 &&
    governors.length === 1 &&
    energyPerformancePreferences.length <= 1 &&
    eppPresenceConsistent;
  return {
    kind: 'linux_cpufreq',
    platform: 'linux',
    policy_count: policies.length,
    online_cpus: onlineCpus,
    covered_online_cpus: coveredCpus,
    coverage_complete: coverageComplete,
    consistent,
    drivers,
    governors,
    energy_performance_preferences: energyPerformancePreferences,
    boost: linuxBoostState(),
    policies,
  };
}

function darwinCpuPowerPolicy() {
  const powerSettings = sh('pmset', ['-g', 'custom']);
  return {
    kind: 'darwin_platform_managed',
    platform: 'darwin',
    policy_count: 1,
    coverage_complete: true,
    consistent: typeof powerSettings === 'string' && powerSettings.length > 0,
    drivers: ['darwin-platform-managed'],
    governors: ['platform-managed'],
    energy_performance_preferences: [],
    boost: { source: 'darwin-platform-managed', enabled: null, raw: null },
    power_settings: powerSettings,
    policies: [],
  };
}

function cpuPowerPolicy() {
  if (platform() === 'linux') return linuxCpuPowerPolicy();
  if (platform() === 'darwin') return darwinCpuPowerPolicy();
  return {
    kind: 'unsupported',
    platform: platform(),
    policy_count: 0,
    coverage_complete: false,
    consistent: false,
    drivers: [],
    governors: [],
    energy_performance_preferences: [],
    boost: { source: null, enabled: null, raw: null },
    policies: [],
  };
}

function cpuIsa() {
  const architecture = process.arch;
  const machine = sh('uname', ['-m']) ?? architecture;
  let flags = [];
  let source = null;
  if (platform() === 'linux') {
    const featureLine = readText('/proc/cpuinfo')
      ?.split('\n')
      .find((line) => /^(?:flags|Features)\s*:/.test(line));
    flags = featureLine
      ? featureLine.slice(featureLine.indexOf(':') + 1).trim().split(/\s+/)
      : [];
    source = '/proc/cpuinfo';
  } else if (platform() === 'darwin') {
    const allSysctls = sh('sysctl', ['-a']) ?? '';
    const optionalFeatures = allSysctls
      .split('\n')
      .filter((line) => /^hw\.optional\.[^:]+:\s*1\s*$/.test(line))
      .map((line) => line.slice(0, line.indexOf(':')));
    const x86Features = [
      sh('sysctl', ['-n', 'machdep.cpu.features']),
      sh('sysctl', ['-n', 'machdep.cpu.leaf7_features']),
    ].flatMap((value) => value?.split(/\s+/) ?? []);
    flags = [...optionalFeatures, ...x86Features];
    source = 'sysctl hw.optional + machdep.cpu features';
  }
  flags = sortedUniqueStrings(flags.map((flag) => flag.toLowerCase()));
  const featureSet = new Set(flags);
  return {
    architecture,
    machine,
    source,
    flags,
    capabilities: {
      avx2: featureSet.has('avx2'),
      fma: featureSet.has('fma'),
      bmi2: featureSet.has('bmi2'),
      vaes: featureSet.has('vaes'),
      any_avx512: flags.some((flag) => flag.startsWith('avx512')),
      neon_or_asimd:
        featureSet.has('neon') ||
        featureSet.has('asimd') ||
        flags.some((flag) => flag.includes('hw.optional.neon')),
    },
  };
}

function sameArray(left, right) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function validCpuPowerPolicy(record) {
  if (record?.kind === 'darwin_platform_managed') {
    return (
      record.platform === 'darwin' &&
      record.policy_count === 1 &&
      record.coverage_complete === true &&
      record.consistent === true &&
      sameArray(record.drivers, ['darwin-platform-managed']) &&
      sameArray(record.governors, ['platform-managed']) &&
      typeof record.power_settings === 'string' &&
      record.power_settings.length > 0
    );
  }
  if (record?.kind !== 'linux_cpufreq' || record.platform !== 'linux') return false;
  if (!Array.isArray(record.policies) || record.policies.length === 0) return false;
  if (
    !record.boost ||
    !(
      typeof record.boost.enabled === 'boolean' ||
      record.boost.enabled === null
    ) ||
    !(
      typeof record.boost.source === 'string' ||
      record.boost.source === null
    )
  ) {
    return false;
  }
  if (
    record.policies.some(
      (policy) =>
        typeof policy.policy !== 'string' ||
        !Array.isArray(policy.affected_cpus) ||
        policy.affected_cpus.length === 0 ||
        typeof policy.driver !== 'string' ||
        policy.driver.length === 0 ||
        typeof policy.governor !== 'string' ||
        policy.governor.length === 0,
    )
  ) {
    return false;
  }
  const drivers = sortedUniqueStrings(record.policies.map((policy) => policy.driver));
  const governors = sortedUniqueStrings(record.policies.map((policy) => policy.governor));
  const energyPerformancePreferences = sortedUniqueStrings(
    record.policies.map((policy) => policy.energy_performance_preference),
  );
  const coveredCpus = [...new Set(record.policies.flatMap((policy) => policy.affected_cpus))]
    .sort((left, right) => left - right);
  const coverageComplete =
    Array.isArray(record.online_cpus) &&
    record.online_cpus.length > 0 &&
    record.online_cpus.every((cpu) => coveredCpus.includes(cpu));
  const eppPresenceConsistent =
    energyPerformancePreferences.length === 0 ||
    record.policies.every(
      (policy) => typeof policy.energy_performance_preference === 'string',
    );
  const consistent =
    drivers.length === 1 &&
    governors.length === 1 &&
    energyPerformancePreferences.length <= 1 &&
    eppPresenceConsistent;
  return (
    record.policy_count === record.policies.length &&
    sameArray(record.drivers, drivers) &&
    sameArray(record.governors, governors) &&
    sameArray(record.energy_performance_preferences, energyPerformancePreferences) &&
    sameArray(record.covered_online_cpus, coveredCpus) &&
    record.coverage_complete === coverageComplete &&
    record.consistent === consistent &&
    coverageComplete &&
    consistent
  );
}

function validCpuIsa(record) {
  return (
    typeof record?.architecture === 'string' &&
    record.architecture.length > 0 &&
    typeof record.machine === 'string' &&
    record.machine.length > 0 &&
    typeof record.source === 'string' &&
    record.source.length > 0 &&
    Array.isArray(record.flags) &&
    record.flags.length > 0 &&
    record.flags.every((flag) => typeof flag === 'string' && flag.length > 0)
  );
}

function powerPolicyComparable(record) {
  return {
    kind: record.kind,
    platform: record.platform,
    policy_count: record.policy_count,
    online_cpus: record.online_cpus,
    covered_online_cpus: record.covered_online_cpus,
    coverage_complete: record.coverage_complete,
    consistent: record.consistent,
    drivers: record.drivers,
    governors: record.governors,
    energy_performance_preferences: record.energy_performance_preferences,
    boost: record.boost,
    power_settings: record.power_settings,
    policies: record.policies,
  };
}

function sameCpuPowerPolicy(left, right) {
  return (
    validCpuPowerPolicy(left) &&
    validCpuPowerPolicy(right) &&
    JSON.stringify(powerPolicyComparable(left)) ===
      JSON.stringify(powerPolicyComparable(right))
  );
}

function powerPolicySummary(record) {
  return {
    kind: record.kind,
    platform: record.platform,
    policy_count: record.policy_count,
    coverage_complete: record.coverage_complete,
    consistent: record.consistent,
    drivers: record.drivers,
    governors: record.governors,
    energy_performance_preferences: record.energy_performance_preferences,
    boost: record.boost,
    power_settings: record.power_settings ?? null,
    scaling_min_khz: sortedUniqueNumbers(
      (record.policies ?? []).map((policy) => policy.scaling_min_khz),
    ),
    scaling_max_khz: sortedUniqueNumbers(
      (record.policies ?? []).map((policy) => policy.scaling_max_khz),
    ),
  };
}

function fingerprint() {
  const cpu = cpus();
  const topology = hostTopology(cpu);
  const powerPolicy = cpuPowerPolicy();
  const isa = cpuIsa();
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
    rustflags: process.arch === 'x64'
      ? '-C target-cpu=x86-64-v2 (.cargo/config.toml)'
      : 'workspace target defaults (the x86-64-v2 override does not apply)',
    node: process.version,
    chromium: sh(PINS.chromium.binary, ['--version'])?.split('\n').pop() ?? 'unknown',
    kernel: release(),
    host_identity: topology.host_identity,
    cpu_model: cpu[0]?.model ?? 'unknown',
    isa,
    power_policy: powerPolicy,
    cpu_count: topology.logical_threads,
    physical_cores: topology.physical_cores,
    logical_threads: topology.logical_threads,
    online_cpus: topology.online_cpus,
    threads_per_core: topology.threads_per_core,
    ram_bytes: topology.ram_bytes,
    numa_nodes: topology.numa_nodes,
    affinity_mask: topology.affinity_mask,
    affinity_cpus: topology.affinity_cpus,
    affinity_source: topology.affinity_source,
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

function sampleMedian(samples) {
  const values = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(values.length / 2);
  return values.length % 2 === 0
    ? (values[mid - 1] + values[mid]) / 2
    : values[mid];
}

/**
 * Independent percentile-bootstrap CI for a ratio of whole-job medians.
 *
 * The engines are separate runtimes and cannot be paired inside one timing closure. The enclosing
 * invocation instead brackets Chromium with the same Rust ELF and gives every engine its own A/A
 * control. Given those guarded raw whole-job samples, independently resampling the two medians is
 * the direct effect CI; per-diagram means never enter this calculation.
 */
function bootstrapMedianRatioCi(numeratorSamples, denominatorSamples) {
  const numerator = Array.isArray(numeratorSamples)
    ? numeratorSamples.filter((value) => Number.isFinite(value) && value > 0)
    : [];
  const denominator = Array.isArray(denominatorSamples)
    ? denominatorSamples.filter((value) => Number.isFinite(value) && value > 0)
    : [];
  const sufficient =
    numerator.length >= MIN_EFFECT_SAMPLES &&
    denominator.length >= MIN_EFFECT_SAMPLES;
  const estimate = numerator.length > 0 && denominator.length > 0
    ? sampleMedian(numerator) / sampleMedian(denominator)
    : null;
  if (!sufficient) {
    return {
      kind: 'independent_bootstrap_ratio_of_whole_job_medians',
      sufficient: false,
      numerator_n: numerator.length,
      denominator_n: denominator.length,
      minimum_samples_per_engine: MIN_EFFECT_SAMPLES,
      resamples: 0,
      estimate,
      ci95_lo: null,
      ci95_hi: null,
      reason: 'insufficient raw whole-job samples for the cross-runtime effect CI',
    };
  }

  let state = 0x9e3779b9;
  const next = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return state >>> 0;
  };
  const ratios = new Array(EFFECT_BOOTSTRAP_RESAMPLES);
  const numeratorResample = new Array(numerator.length);
  const denominatorResample = new Array(denominator.length);
  for (let iteration = 0; iteration < EFFECT_BOOTSTRAP_RESAMPLES; iteration++) {
    for (let i = 0; i < numeratorResample.length; i++) {
      numeratorResample[i] = numerator[next() % numerator.length];
    }
    for (let i = 0; i < denominatorResample.length; i++) {
      denominatorResample[i] = denominator[next() % denominator.length];
    }
    ratios[iteration] =
      sampleMedian(numeratorResample) / sampleMedian(denominatorResample);
  }
  ratios.sort((a, b) => a - b);
  const tail = Math.floor(EFFECT_BOOTSTRAP_RESAMPLES / 40);
  const ci95Lo = ratios[tail];
  const ci95Hi = ratios[EFFECT_BOOTSTRAP_RESAMPLES - 1 - tail];
  return {
    kind: 'independent_bootstrap_ratio_of_whole_job_medians',
    sufficient: true,
    numerator_engine: 'mermaid-js',
    denominator_engine: 'frankenmermaid',
    numerator_n: numerator.length,
    denominator_n: denominator.length,
    minimum_samples_per_engine: MIN_EFFECT_SAMPLES,
    resamples: EFFECT_BOOTSTRAP_RESAMPLES,
    estimate,
    ci95_lo: ci95Lo,
    ci95_hi: ci95Hi,
    excludes_1: ci95Lo > 1 || ci95Hi < 1,
    unit: 'whole_job_wall_time',
  };
}

function validElfSelfReport(record) {
  return (
    record?.record === 'binary' &&
    /^[0-9a-f]{64}$/.test(record.elf_sha256) &&
    Number.isSafeInteger(record.elf_bytes) &&
    record.elf_bytes > 0
  );
}

const validSha256 = (value) => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);

function validRchBuildProvenance(builder, base, cleanOverlay) {
  return (
    typeof builder === 'string' &&
    builder.trim().length > 0 &&
    typeof base === 'string' &&
    /^[0-9a-f]{40}$/.test(base) &&
    cleanOverlay === true
  );
}

function validHostTopology(record) {
  return (
    typeof record?.host_identity === 'string' &&
    record.host_identity.length > 0 &&
    Number.isSafeInteger(record.physical_cores) &&
    record.physical_cores > 0 &&
    Number.isSafeInteger(record.logical_threads) &&
    record.logical_threads >= record.physical_cores &&
    Number.isSafeInteger(record.ram_bytes) &&
    record.ram_bytes > 0 &&
    Number.isSafeInteger(record.numa_nodes) &&
    record.numa_nodes > 0 &&
    typeof record.affinity_mask === 'string' &&
    record.affinity_mask.length > 0 &&
    Array.isArray(record.affinity_cpus) &&
    record.affinity_cpus.length > 0 &&
    record.affinity_cpus.every((cpu) => Number.isSafeInteger(cpu) && cpu >= 0)
  );
}

function validRustThreadProvenance(record, requestedThreads) {
  const observed = record?.thread_count_actually_used;
  return (
    record?.worker_threads === requestedThreads &&
    record.thread_count_requested === requestedThreads &&
    Number.isSafeInteger(observed) &&
    observed >= 1 &&
    observed <= requestedThreads &&
    record.thread_probe?.method ===
      'instrumented_caller_worker_union_over_one_exact_workload' &&
    record.thread_probe?.caller_workers_observed === observed &&
    record.thread_probe?.probe_batch === 1 &&
    record.thread_probe?.inside_timed_region === false &&
    record.oversubscribed === (requestedThreads > record.available_parallelism) &&
    Array.isArray(record.affinity_cpus) &&
    record.affinity_cpus.length > 0 &&
    typeof record.affinity_source === 'string'
  );
}

function validIncumbentThreadProvenance(record) {
  return (
    record?.worker_threads === 1 &&
    record.thread_count_requested === 1 &&
    record.thread_count_actually_used === 1 &&
    record.thread_probe?.method === 'single_cdp_page_main_execution_context' &&
    record.thread_probe?.caller_workers_observed === 1 &&
    record.execution_model === 'single_page_main_thread' &&
    typeof record.chromium_binary === 'string' &&
    record.chromium_binary.startsWith('/') &&
    typeof record.chromium_version === 'string' &&
    record.chromium_version.length > 0
  );
}

function validParseThreadProvenance(record) {
  return (
    record?.measurement_boundary === 'public_parse_validate' &&
    record.worker_threads === 1 &&
    record.thread_count_requested === 1 &&
    record.thread_count_actually_used === 1 &&
    record.thread_probe?.method ===
      'instrumented_calling_thread_id_union_over_exact_parse_workload' &&
    record.thread_probe?.caller_workers_observed === 1 &&
    record.thread_probe?.probe_batch === record.batch &&
    record.thread_probe?.inside_timed_region === false &&
    record.execution_model === 'single_calling_thread' &&
    Array.isArray(record.affinity_cpus) &&
    record.affinity_cpus.length > 0
  );
}

function medianNumber(values) {
  if (!Array.isArray(values) || values.length === 0) return null;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0
    ? (sorted[middle - 1] + sorted[middle]) / 2
    : sorted[middle];
}

function validParseSampleFloor(record, requireBatchVectors = false) {
  const effectSamples = record?.effect_integrated_samples_ns;
  const nullSamples = record?.null_integrated_samples_ns;
  const timing = record?.parse_ns;
  const nullControl = record?.null_control;
  const floor = record?.min_sample_ns;
  const durationsValid =
    floor === minSampleNs() &&
    record?.calibration_target_ns === calibrationTargetNs() &&
    Array.isArray(effectSamples) &&
    effectSamples.length === timing?.samples?.length &&
    effectSamples.length > 0 &&
    Array.isArray(nullSamples) &&
    nullSamples.length === 2 * (nullControl?.n ?? -1) &&
    [...effectSamples, ...nullSamples].every(
      (sample) => Number.isSafeInteger(sample) && sample >= floor,
    );
  if (!durationsValid || !requireBatchVectors) return durationsValid;
  return (
    Array.isArray(record.effect_batches) &&
    record.effect_batches.length === effectSamples.length &&
    Array.isArray(record.null_batches) &&
    record.null_batches.length === nullSamples.length &&
    [...record.effect_batches, ...record.null_batches].every(
      (batch) => Number.isSafeInteger(batch) && batch >= 1,
    )
  );
}

function comparatorDnfLowerBound(record, rustP50Ns) {
  return (
    record?.status === 'dnf' &&
    record.kind === 'timeout' &&
    record.phase === 'probe' &&
    Number.isFinite(record.budget_ms) &&
    record.budget_ms > 0 &&
    Number.isFinite(rustP50Ns) &&
    rustP50Ns > 0
  )
    ? (record.budget_ms * 1e6) / rustP50Ns
    : null;
}

function timingStats(record) {
  return measurementMode === 'parse' ? record?.parse_ns : record?.pipeline_ns;
}

function incumbentTimingStats(record) {
  return measurementMode === 'parse' ? record?.parse_ns : record?.render_ns;
}

function nativeResultSha256(record) {
  return measurementMode === 'parse' ? record?.parse_result_sha256 : record?.output_sha256;
}

function nativeResultBytes(record) {
  return measurementMode === 'parse' ? record?.parse_result_bytes : record?.output_bytes;
}

const PARSE_TYPE_NORMALIZATION = new Map([
  ['flowchart', 'flowchart'],
  ['flowchart-v2', 'flowchart'],
  ['sequence', 'sequence'],
  ['class', 'class'],
  ['state', 'state'],
  ['stateDiagram', 'state'],
  ['stateDiagram-v2', 'state'],
  ['er', 'er'],
]);

function normalizeParseTypes(values) {
  if (!Array.isArray(values)) return { valid: false, normalized: null, unknown: ['<missing>'] };
  const unknown = values.filter((value) => !PARSE_TYPE_NORMALIZATION.has(value));
  return {
    valid: unknown.length === 0,
    normalized: unknown.length === 0
      ? values.map((value) => PARSE_TYPE_NORMALIZATION.get(value))
      : null,
    unknown: [...new Set(unknown)],
  };
}

function semanticWorkGate(frankenmermaid, incumbent) {
  const fmCount = frankenmermaid?.revisions;
  const incumbentCount = incumbent?.revisions;
  const fmInput = frankenmermaid?.input_sha256;
  const incumbentInput = incumbent?.input_sha256;
  const expectedBoundary =
    measurementMode === 'parse' ? 'public_parse_validate' : 'parse_layout_render_svg';
  const fmTypes = normalizeParseTypes(frankenmermaid?.parse_diagram_types_ordered);
  const incumbentTypes = normalizeParseTypes(incumbent?.parse_diagram_types_ordered);
  const parseSemantics =
    measurementMode !== 'parse' ||
    incumbent?.status !== 'ok' ||
    (
      fmTypes.valid &&
      incumbentTypes.valid &&
      JSON.stringify(fmTypes.normalized) === JSON.stringify(incumbentTypes.normalized) &&
      frankenmermaid?.parse_recovery_revisions === 0 &&
      frankenmermaid?.parse_warning_revisions === 0 &&
      frankenmermaid?.parse_unsupported_revisions === 0 &&
      incumbent?.parse_nonempty_config_revisions === 0 &&
      incumbent?.parse_deterministic_output === true
    );
  const accepted =
    measurementMode !== 'parse' ||
    incumbent?.status !== 'ok' ||
    (
      frankenmermaid?.parse_accepted_revisions === fmCount &&
      incumbent?.parse_accepted_revisions === incumbentCount
    );
  const equal =
    Number.isSafeInteger(fmCount) &&
    fmCount > 0 &&
    Number.isSafeInteger(incumbentCount) &&
    incumbentCount === fmCount &&
    validSha256(fmInput) &&
    incumbentInput === fmInput &&
    frankenmermaid?.measurement_boundary === expectedBoundary &&
    incumbent?.measurement_boundary === expectedBoundary &&
    accepted &&
    parseSemantics;
  return {
    verdict: equal ? 'equal' : 'mismatch',
    unit: measurementMode === 'parse' ? 'diagram_parse' : 'diagram_render',
    measurement_boundary: expectedBoundary,
    frankenmermaid_requested_count: fmCount ?? null,
    incumbent_requested_count: incumbentCount ?? null,
    frankenmermaid_accepted_count: frankenmermaid?.parse_accepted_revisions ?? null,
    incumbent_accepted_count: incumbent?.parse_accepted_revisions ?? null,
    frankenmermaid_recovery_count: frankenmermaid?.parse_recovery_revisions ?? null,
    frankenmermaid_warning_count: frankenmermaid?.parse_warning_revisions ?? null,
    frankenmermaid_unsupported_count: frankenmermaid?.parse_unsupported_revisions ?? null,
    incumbent_nonempty_config_count: incumbent?.parse_nonempty_config_revisions ?? null,
    incumbent_deterministic_output: incumbent?.parse_deterministic_output ?? null,
    frankenmermaid_normalized_types: fmTypes.normalized,
    incumbent_normalized_types: incumbentTypes.normalized,
    unknown_frankenmermaid_types: fmTypes.unknown,
    unknown_incumbent_types: incumbentTypes.unknown,
    frankenmermaid_input_sha256: fmInput ?? null,
    incumbent_input_sha256: incumbentInput ?? null,
    rule:
      'both engines must execute the same declared boundary over the same positive diagram count '
      + 'and byte-identical input; parse mode additionally requires every revision accepted',
  };
}

/**
 * Decide a cross-runtime ratio against the more conservative of the two engines' in-process A/A
 * floors. The runtimes cannot share one binary, so each measures its own identical arm twice inside
 * one invocation; the headline claim must clear both bootstrap median CIs by a 2x margin.
 */
/**
 * Corrected A/A null gate.
 *
 * A row is decidable when all three hold:
 *   1. the effect CI excludes 1.0        -- only where an effect CI exists (see below)
 *   2. the effect deviation exceeds 2x the larger null radius
 *   3. EVERY null MEDIAN is within `NULL_MEDIAN_MAX_BIAS` of 1.0
 *
 * Clause 3 is the substantive addition, and it is a TIGHTENING. It bounds arm-order bias without
 * coupling the verdict to how precise the null is. Previously a biased null only inflated the radius
 * in clause 2, which raises the bar -- but at a 16,000x effect a raised bar is meaningless, so a null
 * saying "the two identical arms disagreed by 12%" could not stop the row. That is a statement about
 * the measurement environment being unfit, and it now blocks on its own terms.
 *
 * What deliberately did NOT change: `nullRadius` stays `max(|ci95_hi - 1|, |ci95_lo - 1|)`, the
 * distance from 1.0 to the FARTHER endpoint, not the CI's own half-width `(hi - lo) / 2`. The two
 * differ whenever a null is off-centre, and ours is always the larger -- for a null of
 * [1.011, 1.044] ours is 0.044 against 0.0165. Substituting the narrower reading in the name of
 * "adopting the rule" would have LOOSENED clause 2 while claiming to tighten the gate.
 *
 * Cross-runtime rows with enough raw samples use an independent bootstrap ratio-of-medians CI.
 * Older or explicitly budgeted rows that do not carry enough samples report clause 1 as
 * `not_computable`; a workload declaring `effectCiRequired` fails closed instead.
 *
 * Null CIs stay in every record as telemetry. They are not a veto: a null whose CI excludes 1.0 has
 * never been able to block a row here, which is the fleet-wide defect this harness never had.
 */
function medianCiGate(claimRatio, controls, effectCi = null, effectCiRequired = false) {
  const complete =
    Number.isFinite(claimRatio) &&
    claimRatio > 0 &&
    controls.length > 0 &&
    controls.every(
      (control) =>
        control?.sufficient === true &&
        Number.isFinite(control.half_width) &&
        Number.isFinite(control.ci95_lo) &&
        Number.isFinite(control.ci95_hi) &&
        // Required by clause 3; a null that cannot report its median cannot be shown unbiased.
        Number.isFinite(control.median),
    );
  const rule = 'effect_ci_excludes_1_and_2x_null_radius_and_null_median_within_2pct';
  if (!complete) {
    return {
      verdict: 'fail',
      rule,
      cv_gate: 'never',
      reason: 'missing or insufficient same-invocation A/A null control',
      claim_ratio: claimRatio,
      claim_magnitude: null,
      null_radius: null,
      min_decidable_2x: null,
      null_median_max_bias: NULL_MEDIAN_MAX_BIAS,
      clauses: null,
    };
  }
  const claimMagnitude = Math.max(claimRatio, 1 / claimRatio);
  const nullRadius = Math.max(...controls.map((control) => control.half_width));
  const minDecidable = Math.max(MIN_CLAIM_RATIO, 1 + 2 * nullRadius);

  const biases = controls.map((control) => Math.abs(control.median - 1));
  const worstBias = Math.max(...biases);

  const effectCiAvailable = effectCi?.sufficient !== false
    && Number.isFinite(effectCi?.ci95_lo)
    && Number.isFinite(effectCi?.ci95_hi);
  const effectCiExcludesOne = effectCiAvailable
    ? effectCi.ci95_lo > 1 || effectCi.ci95_hi < 1
    : null;
  const clause2 = claimMagnitude >= minDecidable;
  // "Within 2%" is inclusive, so the comparison needs a tolerance: |1.02 - 1| evaluates to
  // 0.020000000000000018 in binary floating point, which would make a null sitting exactly on the
  // stated boundary fail the stated rule. Without this, two repos implementing the same contract
  // can disagree about whether a boundary row is decidable.
  const clause3 = worstBias <= NULL_MEDIAN_MAX_BIAS * (1 + 1e-9);
  const clause1 = effectCiExcludesOne === true || (!effectCiRequired && effectCiExcludesOne === null);
  const pass = clause1 && clause2 && clause3;

  const reasons = [];
  if (effectCiExcludesOne === false) reasons.push('effect CI includes 1.0');
  if (effectCiRequired && effectCiExcludesOne === null) {
    reasons.push('required cross-runtime effect CI is missing or insufficient');
  }
  if (!clause2) reasons.push('claim does not clear 2x the A/A median-CI radius');
  if (!clause3) {
    reasons.push(
      `A/A null median bias ${(worstBias * 100).toFixed(3)}% exceeds `
      + `${(NULL_MEDIAN_MAX_BIAS * 100).toFixed(0)}% (arm-order asymmetry; the measurement `
      + 'environment was unfit, which says nothing about the effect being real)',
    );
  }

  return {
    verdict: pass ? 'pass' : 'fail',
    rule,
    cv_gate: 'never',
    reason: reasons.length === 0 ? null : reasons.join('; '),
    claim_ratio: claimRatio,
    claim_magnitude: claimMagnitude,
    null_radius: nullRadius,
    min_decidable_2x: minDecidable,
    null_median_max_bias: NULL_MEDIAN_MAX_BIAS,
    clauses: {
      effect_ci_excludes_1: effectCiExcludesOne === null ? 'not_computable' : effectCiExcludesOne,
      effect_ci_required: effectCiRequired,
      effect_ci_note: effectCiExcludesOne === null
        ? 'raw whole-job samples were not sufficient for an independent bootstrap ratio-of-medians CI'
        : null,
      effect_clears_2x_null_radius: clause2,
      null_medians_within_2pct: clause3,
      null_medians: controls.map((control) => control.median),
      null_median_biases_pct: biases.map((bias) => Number((bias * 100).toFixed(4))),
      worst_null_median_bias_pct: Number((worstBias * 100).toFixed(4)),
      // Retained as telemetry only -- never a veto. See the fleet-wide straddle defect.
      null_ci95: controls.map((control) => [control.ci95_lo, control.ci95_hi]),
      null_ci_straddles_1: controls.map((control) => control.ci95_lo <= 1 && control.ci95_hi >= 1),
    },
  };
}

/**
 * Guard a sequential cross-runtime comparison against host drift by measuring the same Rust ELF
 * on both sides of the Chromium phase. The two Rust observations must agree within their own A/A
 * median-CI floor; the slower observation is always the denominator used for the public ratio.
 */
function fmBracket(before, after) {
  const beforeTiming = timingStats(before);
  const afterTiming = timingStats(after);
  const controls = [before?.null_control, after?.null_control];
  const complete =
    before?.status === 'ok' &&
    after?.status === 'ok' &&
    Number.isFinite(beforeTiming?.p50) &&
    beforeTiming.p50 > 0 &&
    Number.isFinite(afterTiming?.p50) &&
    afterTiming.p50 > 0 &&
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
      before_p50_ns: beforeTiming?.p50 ?? null,
      after_p50_ns: afterTiming?.p50 ?? null,
      selected: null,
      drift_ratio: null,
      drift_magnitude: null,
      max_decidable_2x: null,
    };
  }

  const beforeP50 = beforeTiming.p50;
  const afterP50 = afterTiming.p50;
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
  const perfect = { sufficient: true, n: 9, median: 1, ci95_lo: 1, ci95_hi: 1, half_width: 0 };
  const noisy = { sufficient: true, n: 9, median: 1, ci95_lo: 0.98, ci95_hi: 1.02, half_width: 0.02 };
  // Clause 3 fixtures. `biased` has a TIGHT CI that excludes 1.0 -- under the fleet-wide straddle
  // defect this would have vetoed; here it must be judged on its median instead.
  const biased = { sufficient: true, n: 9, median: 1.05, ci95_lo: 1.048, ci95_hi: 1.052, half_width: 0.052 };
  const straddleFree = { sufficient: true, n: 9, median: 1.001, ci95_lo: 1.0005, ci95_hi: 1.0015, half_width: 0.0015 };
  const cases = [
    [1.009, [perfect, perfect], 'fail'],
    [1.01, [perfect, perfect], 'pass'],
    [1.039, [perfect, noisy], 'fail'],
    [1.04, [perfect, noisy], 'pass'],
    [2, [perfect, null], 'fail'],
    // Clause 3 blocks a huge effect when an A/A null shows 5% arm-order asymmetry. Under the old
    // gate this passed: the bias only raised the bar to 1.104, which 1000x cleared trivially.
    [1000, [perfect, biased], 'fail'],
    // A null whose CI EXCLUDES 1.0 but whose median is within 2% must still be decidable. This is
    // the fleet-wide defect's exact input condition, asserted as a non-veto.
    [1000, [perfect, straddleFree], 'pass'],
    // Clause 3 is boundary-exact at 2%.
    [1000, [perfect, { ...perfect, median: 1.02 }], 'pass'],
    [1000, [perfect, { ...perfect, median: 1.0201 }], 'fail'],
    [1000, [perfect, { ...perfect, median: 0.98 }], 'pass'],
    [1000, [perfect, { ...perfect, median: 0.9799 }], 'fail'],
    // A null that cannot report a median cannot be shown unbiased.
    [1000, [perfect, { ...perfect, median: null }], 'fail'],
  ];
  for (const [ratio, controls, want] of cases) {
    const got = medianCiGate(ratio, controls).verdict;
    if (got !== want) throw new Error(`median-CI gate regression: ratio=${ratio} want=${want} got=${got}`);
  }
  // Clause 1 is scored only where an effect CI exists, and is never silently counted as satisfied.
  const noEffectCi = medianCiGate(1000, [perfect, perfect]);
  if (noEffectCi.clauses.effect_ci_excludes_1 !== 'not_computable') {
    throw new Error('cross-runtime rows must report clause 1 as not_computable');
  }
  if (medianCiGate(1000, [perfect, perfect], { ci95_lo: 0.9, ci95_hi: 1.1 }).verdict !== 'fail') {
    throw new Error('an effect CI that includes 1.0 must fail clause 1');
  }
  if (medianCiGate(1000, [perfect, perfect], { ci95_lo: 900, ci95_hi: 1100 }).verdict !== 'pass') {
    throw new Error('an effect CI that excludes 1.0 must satisfy clause 1');
  }
  const exactEffect = bootstrapMedianRatioCi(Array(9).fill(200), Array(9).fill(100));
  if (
    !exactEffect.sufficient ||
    exactEffect.estimate !== 2 ||
    exactEffect.ci95_lo !== 2 ||
    exactEffect.ci95_hi !== 2 ||
    medianCiGate(2, [perfect, perfect], exactEffect, true).verdict !== 'pass'
  ) {
    throw new Error(`whole-job effect-CI regression: ${JSON.stringify(exactEffect)}`);
  }
  const insufficientEffect = bootstrapMedianRatioCi([200], Array(9).fill(100));
  if (
    insufficientEffect.sufficient ||
    medianCiGate(2, [perfect, perfect], insufficientEffect, true).verdict !== 'fail'
  ) {
    throw new Error(`required effect-CI sample floor regression: ${JSON.stringify(insufficientEffect)}`);
  }
  // The radius must stay the distance from 1.0 to the FARTHER endpoint, never the CI's own
  // half-width; substituting the latter would loosen clause 2 for every off-centre null.
  if (medianCiGate(1.05, [{ ...straddleFree, median: 1.001 }]).null_radius !== 0.0015) {
    throw new Error('null radius must be max|ci bound - 1|, not (hi-lo)/2');
  }
  const validElf = { record: 'binary', elf_sha256: 'a'.repeat(64), elf_bytes: 1 };
  if (!validElfSelfReport(validElf) || validElfSelfReport({ ...validElf, elf_sha256: 'unavailable' })) {
    throw new Error('executing-ELF self-report validation regression');
  }
  const topology = {
    host_identity: 'threadripperje',
    physical_cores: 64,
    logical_threads: 128,
    ram_bytes: 536_069_869_568,
    numa_nodes: 1,
    affinity_mask: 'ff',
    affinity_cpus: [0, 1, 2, 3],
  };
  if (!validHostTopology(topology) || validHostTopology({ ...topology, physical_cores: null })) {
    throw new Error('host-topology provenance validation regression');
  }
  const liveTopology = {
    ...hostTopology(cpus()),
    cpu_model: cpus()[0]?.model ?? 'unknown',
  };
  if (!validHostTopology(liveTopology)) {
    throw new Error(`live host-topology provenance is incomplete: ${JSON.stringify(liveTopology)}`);
  }
  const powerPolicy = {
    kind: 'linux_cpufreq',
    platform: 'linux',
    policy_count: 2,
    online_cpus: [0, 1],
    covered_online_cpus: [0, 1],
    coverage_complete: true,
    consistent: true,
    drivers: ['amd-pstate-epp'],
    governors: ['powersave'],
    energy_performance_preferences: ['balance_performance'],
    boost: { source: '/sys/devices/system/cpu/cpufreq/boost', enabled: true, raw: '1' },
    policies: [0, 1].map((cpu) => ({
      policy: `policy${cpu}`,
      affected_cpus: [cpu],
      driver: 'amd-pstate-epp',
      governor: 'powersave',
      energy_performance_preference: 'balance_performance',
      scaling_min_khz: 400_000,
      scaling_max_khz: 4_500_000,
      hardware_min_khz: 400_000,
      hardware_max_khz: 4_500_000,
    })),
  };
  const mixedGovernorPolicy = {
    ...powerPolicy,
    policies: powerPolicy.policies.map((policy, index) =>
      index === 0 ? policy : { ...policy, governor: 'performance' }),
  };
  const incompletePolicy = {
    ...powerPolicy,
    online_cpus: [0, 1, 2],
  };
  if (
    !validCpuPowerPolicy(powerPolicy) ||
    validCpuPowerPolicy(mixedGovernorPolicy) ||
    validCpuPowerPolicy(incompletePolicy) ||
    !sameCpuPowerPolicy(powerPolicy, structuredClone(powerPolicy)) ||
    sameCpuPowerPolicy(powerPolicy, {
      ...powerPolicy,
      boost: { ...powerPolicy.boost, enabled: false, raw: '0' },
    })
  ) {
    throw new Error('CPU power-policy provenance validation regression');
  }
  const isa = {
    architecture: 'x64',
    machine: 'x86_64',
    source: '/proc/cpuinfo',
    flags: ['avx2', 'bmi2', 'fma', 'vaes'],
    capabilities: {
      avx2: true,
      fma: true,
      bmi2: true,
      vaes: true,
      any_avx512: false,
      neon_or_asimd: false,
    },
  };
  if (!validCpuIsa(isa) || validCpuIsa({ ...isa, flags: [] })) {
    throw new Error('CPU ISA provenance validation regression');
  }
  const livePowerPolicy = cpuPowerPolicy();
  const liveIsa = cpuIsa();
  if (!validCpuPowerPolicy(livePowerPolicy) || !validCpuIsa(liveIsa)) {
    throw new Error(
      `live CPU power/ISA provenance is incomplete: ` +
      `${JSON.stringify({ power_policy: livePowerPolicy, isa: liveIsa })}`,
    );
  }
  const liveSchedulerTotals = cpuTotals();
  if (
    !Number.isFinite(liveSchedulerTotals.total) ||
    !Number.isFinite(liveSchedulerTotals.idle) ||
    liveSchedulerTotals.total <= 0 ||
    liveSchedulerTotals.idle < 0 ||
    liveSchedulerTotals.idle > liveSchedulerTotals.total
  ) {
    throw new Error(`live scheduler-time provenance is invalid: ${JSON.stringify(liveSchedulerTotals)}`);
  }
  if (JSON.stringify(parseCpuList('0-3,8,10-11,8')) !== JSON.stringify([0, 1, 2, 3, 8, 10, 11])) {
    throw new Error('CPU-list parser regression');
  }
  if (JSON.stringify(parseCpuSet('0 4-5 8')) !== JSON.stringify([0, 4, 5, 8])) {
    throw new Error('CPU-set parser regression');
  }
  for (const invalid of ['2-1', '1-2-3', 'cpu7']) {
    try {
      parseCpuList(invalid);
      throw new Error(`CPU-list parser accepted ${JSON.stringify(invalid)}`);
    } catch (error) {
      if (String(error).includes('accepted')) throw error;
    }
  }
  const busySample = cpuBusyFromSnapshots(
    [
      { cpu: 0, idle: 100, total: 200 },
      { cpu: 1, idle: 100, total: 200 },
    ],
    [
      { cpu: 0, idle: 180, total: 300 },
      { cpu: 1, idle: 170, total: 300 },
    ],
  );
  const quietHost = classifyHostWideQuiescence(
    busySample,
    [0],
    HOST_WIDE_MAX_BUSY_FRACTION,
  );
  const busyHost = classifyHostWideQuiescence(
    busySample,
    [0, 1],
    HOST_WIDE_MAX_BUSY_FRACTION,
  );
  const incompleteHost = classifyHostWideQuiescence(
    busySample,
    [0, 1, 2],
    HOST_WIDE_MAX_BUSY_FRACTION,
  );
  if (
    quietHost.verdict !== 'clear' ||
    busyHost.verdict !== 'blocked' ||
    incompleteHost.verdict !== 'blocked'
  ) {
    throw new Error('host-wide exclusivity classification regression');
  }
  if (
    !validExclusiveHostClaim('trj-booking:1234') ||
    validExclusiveHostClaim('other-host:1234') ||
    validExclusiveHostClaim('trj-booking') ||
    validExclusiveHostClaim('trj-booking:0')
  ) {
    throw new Error('exclusive-host claim validation regression');
  }
  const validThreads = {
    worker_threads: 4,
    thread_count_requested: 4,
    thread_count_actually_used: 3,
    available_parallelism: 4,
    oversubscribed: false,
    batch: 7,
    affinity_cpus: [0, 1, 2, 3],
    affinity_source: 'linux_proc_status',
    thread_probe: {
      method: 'instrumented_caller_worker_union_over_one_exact_workload',
      caller_workers_observed: 3,
      probe_batch: 1,
      inside_timed_region: false,
    },
  };
  if (
    !validRustThreadProvenance(validThreads, 4) ||
    !validRustThreadProvenance({
      ...validThreads,
      worker_threads: 8,
      thread_count_requested: 8,
      thread_count_actually_used: 7,
      oversubscribed: true,
      thread_probe: { ...validThreads.thread_probe, caller_workers_observed: 7 },
    }, 8) ||
    validRustThreadProvenance({ ...validThreads, thread_count_actually_used: null }, 4)
  ) {
    throw new Error('actual-thread provenance validation regression');
  }
  const validIncumbentThreads = {
    worker_threads: 1,
    thread_count_requested: 1,
    thread_count_actually_used: 1,
    execution_model: 'single_page_main_thread',
    chromium_binary: '/usr/bin/google-chrome',
    chromium_version: 'Chrome/151.0.0.0',
    thread_probe: {
      method: 'single_cdp_page_main_execution_context',
      caller_workers_observed: 1,
    },
  };
  if (
    !validIncumbentThreadProvenance(validIncumbentThreads) ||
    validIncumbentThreadProvenance({ ...validIncumbentThreads, thread_count_actually_used: null })
  ) {
    throw new Error('incumbent actual-thread provenance validation regression');
  }
  const validParseThreads = {
    measurement_boundary: 'public_parse_validate',
    worker_threads: 1,
    thread_count_requested: 1,
    thread_count_actually_used: 1,
    execution_model: 'single_calling_thread',
    batch: 4,
    affinity_cpus: [0],
    thread_probe: {
      method: 'instrumented_calling_thread_id_union_over_exact_parse_workload',
      caller_workers_observed: 1,
      probe_batch: 4,
      inside_timed_region: false,
    },
  };
  if (
    !validParseThreadProvenance(validParseThreads) ||
    validParseThreadProvenance({ ...validParseThreads, thread_count_actually_used: null })
  ) {
    throw new Error('parse actual-thread provenance validation regression');
  }
  if (
    !validRchBuildProvenance('hz1', 'a'.repeat(40), true) ||
    validRchBuildProvenance('', 'a'.repeat(40), true) ||
    validRchBuildProvenance('hz1', 'a'.repeat(39), true) ||
    validRchBuildProvenance('hz1', 'a'.repeat(40), false)
  ) {
    throw new Error('RCH exact-base clean-overlay provenance validation regression');
  }
  const semanticRecord = {
    status: 'ok',
    revisions: 2_000,
    input_sha256: 'a'.repeat(64),
    measurement_boundary:
      measurementMode === 'parse' ? 'public_parse_validate' : 'parse_layout_render_svg',
    parse_accepted_revisions: 2_000,
    parse_recovery_revisions: 0,
    parse_warning_revisions: 0,
    parse_unsupported_revisions: 0,
    parse_nonempty_config_revisions: 0,
    parse_deterministic_output: true,
    parse_diagram_types_ordered: Array.from({ length: 2_000 }, () =>
      measurementMode === 'parse' ? 'flowchart' : null),
  };
  if (
    semanticWorkGate(semanticRecord, semanticRecord).verdict !== 'equal' ||
    semanticWorkGate(semanticRecord, { ...semanticRecord, revisions: 1_999 }).verdict !==
      'mismatch' ||
    semanticWorkGate(semanticRecord, { ...semanticRecord, input_sha256: 'b'.repeat(64) }).verdict !==
      'mismatch'
  ) {
    throw new Error('semantic work-count validation regression');
  }
  if (measurementMode === 'parse') {
    const mismatchedTypes = {
      ...semanticRecord,
      parse_diagram_types_ordered: [
        ...semanticRecord.parse_diagram_types_ordered.slice(0, -1),
        'stateDiagram',
      ],
    };
    if (
      semanticWorkGate(semanticRecord, mismatchedTypes).verdict !== 'mismatch' ||
      semanticWorkGate(
        { ...semanticRecord, parse_recovery_revisions: 1 },
        semanticRecord,
      ).verdict !== 'mismatch' ||
      semanticWorkGate(
        semanticRecord,
        { ...semanticRecord, parse_nonempty_config_revisions: 1 },
      ).verdict !== 'mismatch' ||
      semanticWorkGate(
        semanticRecord,
        { ...semanticRecord, parse_deterministic_output: false },
      ).verdict !== 'mismatch'
    ) {
      throw new Error('parse semantic-equivalence validation regression');
    }
    const floorRecord = {
      min_sample_ns: PARSE_MIN_SAMPLE_NS,
      calibration_target_ns: PARSE_CALIBRATION_TARGET_NS,
      parse_ns: { samples: [10, 11] },
      null_control: { n: 2 },
      effect_integrated_samples_ns: [
        PARSE_MIN_SAMPLE_NS,
        PARSE_MIN_SAMPLE_NS + 1,
      ],
      null_integrated_samples_ns: Array.from(
        { length: 4 },
        () => PARSE_MIN_SAMPLE_NS,
      ),
      effect_batches: [4, 4],
      null_batches: [4, 4, 4, 4],
    };
    if (
      !validParseSampleFloor(floorRecord, true) ||
      validParseSampleFloor(
        {
          ...floorRecord,
          null_integrated_samples_ns: [
            PARSE_MIN_SAMPLE_NS - 1,
            ...floorRecord.null_integrated_samples_ns.slice(1),
          ],
        },
        true,
      ) ||
      validParseSampleFloor({ ...floorRecord, null_batches: [4] }, true)
    ) {
      throw new Error('parse every-arm sample-floor validation regression');
    }
    // The 250 ms parse floor must be the value actually enforced: a record carrying the old
    // 50 ms sweep floor has to be refused in parse mode, or the predicate is decorative.
    if (
      validParseSampleFloor(
        {
          ...floorRecord,
          min_sample_ns: THREAD_SWEEP_MIN_SAMPLE_NS,
          calibration_target_ns: THREAD_SWEEP_CALIBRATION_TARGET_NS,
        },
        true,
      )
    ) {
      throw new Error('parse floor must reject the 50 ms thread-sweep floor');
    }
  }
  if (
    comparatorDnfLowerBound(
      { status: 'dnf', kind: 'timeout', phase: 'probe', budget_ms: 100 },
      10_000_000,
    ) !== 10 ||
    comparatorDnfLowerBound(
      { status: 'dnf', kind: 'timeout', phase: 'timed', budget_ms: 100 },
      10_000_000,
    ) !== null ||
    comparatorDnfLowerBound(
      { status: 'dnf', kind: 'failed', phase: 'probe', budget_ms: 100 },
      10_000_000,
    ) !== null
  ) {
    throw new Error('DNF exact-one-job lower-bound validation regression');
  }
  const bracketRecord = (p50, nullControl = perfect) => ({
    status: 'ok',
    ...(measurementMode === 'parse' ? { parse_ns: { p50 } } : { pipeline_ns: { p50 } }),
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
    host_topology_gate: 'required',
    live_topology: {
      host_identity: liveTopology.host_identity,
      physical_cores: liveTopology.physical_cores,
      logical_threads: liveTopology.logical_threads,
      numa_nodes: liveTopology.numa_nodes,
      affinity_logical_cpus: liveTopology.affinity_cpus.length,
    },
    actual_thread_probe_gate: 'required',
    whole_job_effect_ci_gate: {
      method: 'independent_bootstrap_ratio_of_whole_job_medians',
      minimum_samples_per_engine: MIN_EFFECT_SAMPLES,
      resamples: EFFECT_BOOTSTRAP_RESAMPLES,
      required_when_declared_by_corpus: true,
    },
    semantic_work_gate: 'required',
    cpu_power_policy_gate: {
      required: true,
      live: powerPolicySummary(livePowerPolicy),
    },
    cpu_isa_gate: {
      required: true,
      live: liveIsa,
    },
    host_wide_exclusivity_gate: {
      claim_reference: 'required',
      all_affinity_cpus: 'required',
      maximum_busy_fraction: HOST_WIDE_MAX_BUSY_FRACTION,
      sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
      maximum_admission_attempts: HOST_WIDE_QUIET_MAX_ATTEMPTS,
      before_every_measured_phase: true,
    },
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
const exclusiveHostClaim = arg('exclusive-host-claim');
const allowOversubscription = has('allow-oversubscription');
const fmBuilder = arg('fm-builder');
const fmBuildBase = arg('fm-build-base');
const fmBuildCleanOverlay = has('fm-build-clean-overlay');
// A speedup over a render that dropped content is not a speedup. Every measured row must be backed
// by a passing cross-engine equivalence verdict (see equivalence.mjs) produced from the SAME input,
// the SAME Rust ELF and the SAME mermaid bundle. `--allow-unverified-output` still permits a run --
// performance work on a known content gap is legitimate -- but it is recorded in the artifact and
// stamped on every affected row, so no number can later be quoted without its admission.
const allowUnverifiedOutput = has('allow-unverified-output');
const equivalenceDir = arg('equivalence-dir', join(REPO, '.benchmarks', 'headtohead', 'equivalence'));
if (measurementMode === 'parse' && allowUnverifiedOutput) {
  console.error('[run] --mode parse rejects --allow-unverified-output; linked semantic equivalence is mandatory');
  process.exit(2);
}
if (measurementMode === 'parse' && threadSweep.length > 0) {
  console.error('[run] --mode parse currently supports only the scalar public parser boundary');
  process.exit(2);
}
// `bd-ap4v` retry predicate: the parse row is re-adjudicated only under the same dedicated-host
// booking and the same full-host admission check the thread-sweep driver enforces. The first
// attempt ran without either, and its Rust-before null drifted 4.090% from 1.0.
if (measurementMode === 'parse' && !validExclusiveHostClaim(exclusiveHostClaim)) {
  console.error(
    '[run] --mode parse requires --exclusive-host-claim trj-booking:<Agent-Mail-CLAIM-message-id>',
  );
  process.exit(2);
}
if (
  measurementMode === 'parse' &&
  !validRchBuildProvenance(fmBuilder, fmBuildBase, fmBuildCleanOverlay)
) {
  console.error(
    '[run] --mode parse requires --fm-builder <rch-worker-id>, --fm-build-base <40-hex commit>, '
      + 'and --fm-build-clean-overlay',
  );
  process.exit(2);
}
if (threadSweep.length > 0) {
  if (items.length !== 1) {
    console.error('[run] --thread-sweep requires --only to select exactly one corpus item');
    process.exit(2);
  }
  if (!threadSweep.includes(1)) {
    console.error('[run] --thread-sweep must include 1 for the scalar byte-identity reference');
    process.exit(2);
  }
  if (!validExclusiveHostClaim(exclusiveHostClaim)) {
    console.error(
      '[run] --thread-sweep requires --exclusive-host-claim trj-booking:<Agent-Mail-CLAIM-message-id>',
    );
    process.exit(2);
  }
  if (typeof fmBuilder !== 'string' || fmBuilder.trim().length === 0) {
    console.error('[run] --thread-sweep requires --fm-builder <rch-worker-id> for executable provenance');
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
env.measurement_mode = measurementMode;
env.measurement_boundary =
  measurementMode === 'parse' ? 'public_parse_validate' : 'parse_layout_render_svg';
console.error(`[run] rev=${env.git_rev?.slice(0, 8)}${env.git_dirty ? '-dirty' : ''} load1=${env.loadavg_1m.toFixed(2)} cpus=${env.cpu_count}`);
const powerPolicy = powerPolicySummary(env.power_policy);
console.error(
  `[run] power driver=${powerPolicy.drivers.join(',') || 'missing'} ` +
  `governor=${powerPolicy.governors.join(',') || 'missing'} ` +
  `epp=${powerPolicy.energy_performance_preferences.join(',') || 'not-exposed'} ` +
  `boost=${powerPolicy.boost.enabled ?? 'unknown'} ` +
  `isa=${env.isa.machine}/${env.isa.architecture} flags=${env.isa.flags.length}`,
);
const powerAndIsaRequired = !has('skip-mermaid') || threadSweep.length > 0;
const powerAndIsaValid =
  validCpuPowerPolicy(env.power_policy) &&
  validCpuIsa(env.isa);
if (
  powerAndIsaRequired &&
  !powerAndIsaValid
) {
  console.error(
    '[run] INVALID: cross-engine evidence requires complete, internally consistent CPU governor and ISA provenance',
  );
  process.exit(2);
}
if ((threadSweep.length > 0 || measurementMode === 'parse') && !validHostTopology(env)) {
  console.error('[run] INVALID: measured parse rows and thread sweeps require host identity, physical/logical topology, RAM, NUMA, and affinity provenance');
  process.exit(2);
}
const maximumRequestedThreads = threadSweep.length > 0 ? Math.max(...threadSweep) : 1;
if (maximumRequestedThreads > env.cpu_count && !allowOversubscription) {
  console.error(
    `[run] --thread-sweep requests ${maximumRequestedThreads} threads on ${env.cpu_count} logical CPUs; `
      + 'pass --allow-oversubscription to measure and label that execution model explicitly',
  );
  process.exit(2);
}
if (maximumRequestedThreads > env.affinity_cpus.length && !allowOversubscription) {
  console.error(
    `[run] --thread-sweep requests ${maximumRequestedThreads} threads, but this process affinity exposes only `
      + `${env.affinity_cpus.length} logical CPUs; pass --allow-oversubscription to measure and label it`,
  );
  process.exit(2);
}
if (threadSweep.length > 0 && env.affinity_cpus.length !== env.logical_threads) {
  console.error(
    `[run] host-wide thread sweeps require the complete host cpuset; affinity exposes ${env.affinity_cpus.length} of ${env.logical_threads} logical CPUs`,
  );
  process.exit(2);
}
// The admission check samples this process's affinity set. If the driver itself were confined to a
// subset, "full-host quiescence" would only ever assert quiet on that subset -- so parse mode
// requires the whole cpuset for the DRIVER. The measured Rust child is still pinned via taskset
// below; that narrows the child, not the set this driver admits against.
if (measurementMode === 'parse' && env.affinity_cpus.length !== env.logical_threads) {
  console.error(
    `[run] --mode parse requires the complete host cpuset for the admission check; affinity exposes ${env.affinity_cpus.length} of ${env.logical_threads} logical CPUs`,
  );
  process.exit(2);
}
env.thread_sweep = threadSweep.length > 0
  ? {
      threads: threadSweep,
      local_machine_required: true,
      exclusive_host_claim: exclusiveHostClaim,
      host_wide_quiescence_required: true,
      host_wide_maximum_busy_fraction: HOST_WIDE_MAX_BUSY_FRACTION,
      host_wide_sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
      host_wide_maximum_admission_attempts: HOST_WIDE_QUIET_MAX_ATTEMPTS,
      cpu_power_policy_required: true,
      cpu_isa_provenance_required: true,
      scalar_reference_threads: 1,
      parallel_executor: 'rayon_persistent_pool',
      incumbent_executor: 'single_page_main_thread',
      min_sample_ns: THREAD_SWEEP_MIN_SAMPLE_NS,
      calibration_target_ns: THREAD_SWEEP_CALIBRATION_TARGET_NS,
      oversubscription_opt_in: allowOversubscription,
      maximum_requested_threads: maximumRequestedThreads,
      host_logical_threads: env.logical_threads,
      maximum_requested_to_logical_ratio: maximumRequestedThreads / env.logical_threads,
    }
  : null;
// Stamped so the predeclared floor and the admission contract travel with the numbers. A reader
// can tell a 250 ms-floor parse row from the superseded 50 ms one without consulting the ledger.
env.parse_admission = measurementMode === 'parse'
  ? {
      boundary: 'public_parse_validate',
      exclusive_host_claim: exclusiveHostClaim,
      host_wide_quiescence_required: true,
      host_wide_maximum_busy_fraction: HOST_WIDE_MAX_BUSY_FRACTION,
      host_wide_sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
      host_wide_maximum_admission_attempts: HOST_WIDE_QUIET_MAX_ATTEMPTS,
      complete_host_cpuset_required: true,
      cpu_power_policy_required: true,
      cpu_isa_provenance_required: true,
      min_sample_ns: PARSE_MIN_SAMPLE_NS,
      calibration_target_ns: PARSE_CALIBRATION_TARGET_NS,
      predeclared_floor_source: 'bd-ap4v retry predicate',
      superseded_min_sample_ns: THREAD_SWEEP_MIN_SAMPLE_NS,
      null_median_max_bias: NULL_MEDIAN_MAX_BIAS,
      incumbent_executor: 'single_page_main_thread',
    }
  : null;
env.fm_builder = fmBuilder;
env.fm_build_base = fmBuildBase;
env.fm_build_clean_overlay = fmBuildCleanOverlay;

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
// The sweep additionally re-checks the CPU power policy and samples every affinity CPU while the
// host is idle immediately before each measured phase. Any policy drift or CPU above the fixed 20%
// ceiling blocks the invocation before more evidence is produced. Across-phase aggregate busyness
// remains report-only because it includes the engines' own work.
const phaseLoad = [];
const hostWideQuiescenceChecks = [];

function requireHostWideQuiescence(label) {
  for (let attempt = 1; attempt <= HOST_WIDE_QUIET_MAX_ATTEMPTS; attempt++) {
    const livePowerPolicy = cpuPowerPolicy();
    const livePowerPolicyValid = validCpuPowerPolicy(livePowerPolicy);
    const powerPolicyMatchesBaseline =
      livePowerPolicyValid &&
      sameCpuPowerPolicy(env.power_policy, livePowerPolicy);
    const cpuClassification = classifyHostWideQuiescence(
      cpuBusy(HOST_WIDE_QUIET_SAMPLE_MS),
      env.affinity_cpus,
      HOST_WIDE_MAX_BUSY_FRACTION,
    );
    const record = {
      ...cpuClassification,
      phase: label,
      attempt,
      observed_at: new Date().toISOString(),
      sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
      exclusive_host_claim: exclusiveHostClaim,
      requirement: 'all affinity CPUs must remain at or below the fixed busy-fraction ceiling',
      power_policy_valid: livePowerPolicyValid,
      power_policy_matches_baseline: powerPolicyMatchesBaseline,
      power_policy: powerPolicySummary(livePowerPolicy),
      verdict:
        cpuClassification.verdict === 'clear' &&
        powerPolicyMatchesBaseline
          ? 'clear'
          : 'blocked',
    };
    hostWideQuiescenceChecks.push(record);
    if (record.verdict === 'clear') {
      console.error(
        `[run] host-wide exclusivity clear before ${label}: ` +
        `${record.allowed_cpu_count} CPUs, max ${(record.observed_max_busy_fraction * 100).toFixed(1)}%` +
        (attempt === 1 ? '' : ` after ${attempt} admission samples`),
      );
      return;
    }
    const busy = record.busy_cpus_above_limit
      .slice(0, 12)
      .map(({ cpu, busy_fraction }) => `cpu${cpu}=${(busy_fraction * 100).toFixed(1)}%`)
      .join(',');
    console.error(
      `[run] host-wide exclusivity waiting before ${label} ` +
      `(attempt ${attempt}/${HOST_WIDE_QUIET_MAX_ATTEMPTS}): ` +
      `missing=[${record.missing_cpus.join(',')}] busy=[${busy}] ` +
      `power-policy=${livePowerPolicyValid ? (powerPolicyMatchesBaseline ? 'match' : 'changed') : 'invalid'} ` +
      `(limit ${(HOST_WIDE_MAX_BUSY_FRACTION * 100).toFixed(1)}%)`,
    );
  }
  console.error(
    `[run] HOST-WIDE EXCLUSIVITY BLOCKED before ${label}: no clear sample in ` +
    `${HOST_WIDE_QUIET_MAX_ATTEMPTS * HOST_WIDE_QUIET_SAMPLE_MS}ms`,
  );
  process.exit(6);
}

/** Aggregate scheduler time across all CPUs. Passive: no busy-wait. */
function cpuTotals() {
  return cpuTimeSnapshot().reduce(
    (totals, record) => ({
      total: totals.total + record.total,
      idle: totals.idle + record.idle,
    }),
    { total: 0, idle: 0 },
  );
}

/**
 * Measure machine busyness ACROSS the phase, not at its endpoints.
 *
 * The first version of this guard compared `loadavg()[0]` before and after each phase. That is the
 * wrong instrument here: our arm takes ~19 s and mermaid's ~841 s, and a 1-minute load average
 * sampled around a 19 s phase describes the minute *preceding* it. On a box that is quieting down
 * -- which it was, 21.8 -> 6.6 over the run -- that alone manufactures an apparent asymmetry.
 * `os.cpus()` scheduler-time deltas cover the exact interval on Linux and macOS, so they compare
 * like with like regardless of how differently long the two phases are. This remains provenance
 * only; exclusive sweeps additionally block on the idle full-host sample immediately before every
 * phase.
 */
function timedPhase(label, fn) {
  if (threadSweep.length > 0 || measurementMode === 'parse') requireHostWideQuiescence(label);
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
    return timedPhase(prefix, () =>
      runJsonl(prefix, fmCmd, fmArgs, {
        FM_H2H_MODE: measurementMode,
        FM_H2H_THREAD_PROBE: measurementMode === 'parse' ? '1' : '0',
        ...(measurementMode === 'parse'
          ? { FM_H2H_MIN_SAMPLE_NS: String(PARSE_MIN_SAMPLE_NS) }
          : {}),
      }));
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
          FM_H2H_THREAD_PROBE: '1',
          FM_H2H_MODE: measurementMode,
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
      record.measurement_boundary === env.measurement_boundary &&
      record.elf_sha256 === binaryRecordBefore?.elf_sha256 &&
      record.elf_bytes === binaryRecordBefore?.elf_bytes &&
      (threadSweep.length === 0 ||
        (record.thread_probe_required === true &&
          record.thread_count_requested === record.worker_threads &&
          record.affinity_mask === env.affinity_mask &&
          JSON.stringify(record.affinity_cpus) === JSON.stringify(env.affinity_cpus))) &&
      (threadSweep.length === 0 ||
        record.min_sample_ns === THREAD_SWEEP_MIN_SAMPLE_NS) &&
      (threadSweep.length === 0 ||
        record.calibration_target_ns === THREAD_SWEEP_CALIBRATION_TARGET_NS) &&
      (measurementMode !== 'parse' ||
        (
          record.min_sample_ns === PARSE_MIN_SAMPLE_NS &&
          record.calibration_target_ns === PARSE_CALIBRATION_TARGET_NS
        )) &&
      (threadSweep.length === 0 || threadSweep.includes(record.worker_threads)),
  );
if (!elfSelfReportBeforeValid) {
  console.error('[run] INVALID: every frankenmermaid sweep arm must self-report the same executing ELF');
}

const mjsArgs = [join(HERE, 'mermaid_bench.mjs')];
mjsArgs.push('--mode', measurementMode);
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
      record.measurement_boundary === env.measurement_boundary &&
      record.elf_sha256 === binaryRecordBefore?.elf_sha256 &&
      record.elf_bytes === binaryRecordBefore?.elf_bytes &&
      (threadSweep.length === 0 ||
        (record.thread_probe_required === true &&
          record.thread_count_requested === record.worker_threads &&
          record.affinity_mask === env.affinity_mask &&
          JSON.stringify(record.affinity_cpus) === JSON.stringify(env.affinity_cpus))) &&
      (threadSweep.length === 0 ||
        record.min_sample_ns === THREAD_SWEEP_MIN_SAMPLE_NS) &&
      (threadSweep.length === 0 ||
        record.calibration_target_ns === THREAD_SWEEP_CALIBRATION_TARGET_NS) &&
      (measurementMode !== 'parse' ||
        (
          record.min_sample_ns === PARSE_MIN_SAMPLE_NS &&
          record.calibration_target_ns === PARSE_CALIBRATION_TARGET_NS
        )) &&
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

// ---------------------------------------------------------------- output equivalence

/**
 * Newest equivalence artifact that actually applies to this item, or a reason it does not.
 *
 * "Applies" is deliberately strict on all three things that can move rendered content: the input
 * bytes, the Rust ELF that rendered them, and the mermaid bundle it was compared against. A stale
 * artifact from a different binary is worse than none, because it reads as verification.
 */
function findEquivalenceVerdict(id, inputSha, elfSha, bundleSha) {
  let names;
  try {
    names = readdirSync(equivalenceDir).filter((n) => n.startsWith('equivalence-') && n.endsWith('.json'));
  } catch {
    return { status: 'no_artifact_directory', directory: equivalenceDir };
  }
  // Newest first by the embedded timestamp, NOT by filename: the name is
  // `equivalence-<gitrev>-<ts>.json`, so a plain sort orders by git hash, which is meaningless.
  const byNewest = names
    .map((name) => ({ name, ts: Number(/-(\d+)\.json$/.exec(name)?.[1] ?? 0) }))
    .sort((a, b) => b.ts - a.ts)
    .map((entry) => entry.name);
  const candidates = [];
  for (const name of byNewest) {
    let artifact;
    try {
      artifact = JSON.parse(readFileSync(join(equivalenceDir, name), 'utf8'));
    } catch {
      continue;
    }
    const row = (artifact.rows ?? []).find((r) => r.id === id);
    if (!row) continue;
    const mismatch = [];
    if (row.input_sha256 !== inputSha) mismatch.push('input_sha256');
    if (elfSha && artifact.provenance?.fm_elf_sha256 !== elfSha) mismatch.push('fm_elf_sha256');
    if (bundleSha && artifact.pins?.bundle_sha256 !== bundleSha) mismatch.push('mermaid_bundle_sha256');
    if (mismatch.length > 0) {
      candidates.push({ artifact: name, mismatch });
      continue;
    }
    if (row.status !== 'compared') {
      return { status: 'not_comparable', artifact: name, reason: row.status, note: row.note ?? null };
    }
    const e = row.equivalence;
    return {
      status: e.verdict === 'pass' ? 'verified' : 'divergent',
      artifact: name,
      method: e.method,
      claim: e.claim,
      diagrams: e.diagrams,
      equivalent: e.equivalent,
      divergent: e.divergent,
      unverified: e.unverified,
      by_family: e.by_family,
      divergent_families: Object.entries(e.by_family)
        .filter(([, v]) => v.divergent > 0 || v.unverified > 0)
        .map(([f, v]) => `${f}:${v.divergent}divergent/${v.unverified}unverified/${v.diagrams}`),
    };
  }
  return { status: 'no_matching_artifact', directory: equivalenceDir, rejected: candidates.slice(0, 4) };
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

function rowHostWideExclusivity(threads) {
  if (threadSweep.length === 0) return null;
  const phases = has('skip-mermaid')
    ? [`frankenmermaid-before-t${threads}`]
    : [
        `frankenmermaid-before-t${threads}`,
        'mermaid-js',
        `frankenmermaid-after-t${threads}`,
      ];
  const checks = phases.map((phase) => {
    const check = hostWideQuiescenceChecks.findLast((candidate) => candidate.phase === phase);
    return {
      phase,
      verdict: check?.verdict ?? 'missing',
      observed_at: check?.observed_at ?? null,
      observed_max_busy_fraction: check?.observed_max_busy_fraction ?? null,
      power_policy_valid: check?.power_policy_valid ?? false,
      power_policy_matches_baseline: check?.power_policy_matches_baseline ?? false,
      power_policy: check?.power_policy ?? null,
    };
  });
  return {
    verdict: checks.every((check) => check.verdict === 'clear') ? 'clear' : 'blocked',
    exclusive_host_claim: exclusiveHostClaim,
    maximum_busy_fraction: HOST_WIDE_MAX_BUSY_FRACTION,
    sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
    maximum_admission_attempts: HOST_WIDE_QUIET_MAX_ATTEMPTS,
    complete_host_cpuset: env.affinity_cpus.length === env.logical_threads,
    baseline_power_policy: powerPolicySummary(env.power_policy),
    phase_checks: checks,
  };
}

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
    fm_worker_threads_requested: threads ?? fBefore?.thread_count_requested ?? 1,
    fm_builder: fmBuilder,
    fm_build_base: fmBuildBase,
    fm_build_clean_overlay: fmBuildCleanOverlay,
    fm_build: {
      tool: 'rch exec',
      worker: fmBuilder,
      base: fmBuildBase,
      clean_overlay: fmBuildCleanOverlay,
    },
    host_wide_exclusivity: rowHostWideExclusivity(threads),
    host: {
      identity: env.host_identity,
      cpu_model: env.cpu_model,
      isa: env.isa,
      power_policy: powerPolicySummary(env.power_policy),
      physical_cores: env.physical_cores,
      logical_threads: env.logical_threads,
      ram_bytes: env.ram_bytes,
      numa_nodes: env.numa_nodes,
    },
    engine_sha256: {
      frankenmermaid_elf: binaryRecordBefore?.elf_sha256 ?? null,
      mermaid_js_bundle: m?.bundle_sha256 ?? null,
    },
  };

  if (
    threadSweep.length > 0 &&
    row.host_wide_exclusivity?.verdict !== 'clear'
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'host_wide_exclusivity_invalid',
      error:
        'every measured sweep phase requires clear full-host quiescence and the unchanged baseline power policy',
    });
    continue;
  }
  if (
    measurementMode === 'parse' &&
    [fBefore, fAfter].some((record) => !validParseSampleFloor(record))
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'sample_floor_violation',
      error:
        `every Rust parse effect and A/A arm must integrate for at least `
        + `${PARSE_MIN_SAMPLE_NS} ns`,
    });
    continue;
  }
  if (
    measurementMode === 'parse' &&
    (!validParseThreadProvenance(fBefore) || !validParseThreadProvenance(fAfter))
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'rust_thread_provenance_invalid',
      error:
        'parse rows require an observed single calling thread over the exact parse workload',
    });
    continue;
  }
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
    (
      !validRustThreadProvenance(fBefore, threads) ||
      !validRustThreadProvenance(fAfter, threads) ||
      fBefore.affinity_mask !== fAfter.affinity_mask ||
      fBefore.affinity_mask !== env.affinity_mask ||
      JSON.stringify(fBefore.affinity_cpus) !== JSON.stringify(fAfter.affinity_cpus) ||
      JSON.stringify(fBefore.affinity_cpus) !== JSON.stringify(env.affinity_cpus)
    )
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'rust_thread_provenance_invalid',
      error:
        'every Rust bracket arm must report requested and actual operation threads plus the exact inherited affinity',
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
    nativeResultSha256(fBefore) !== nativeResultSha256(fAfter) ||
    (
      measurementMode === 'render' &&
      fBefore.output_sha256_lean !== fAfter.output_sha256_lean
    )
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
  const fTiming = timingStats(f);
  row.fm_bracket = bracket;
  row.fm_execution_model = f.execution_model ?? 'scalar';
  row.fm_available_parallelism = f.available_parallelism ?? null;
  row.fm_oversubscribed = f.oversubscribed === true;
  row.fm_requested_to_available_ratio =
    row.fm_worker_threads_requested / Math.max(1, row.fm_available_parallelism ?? 1);
  row.fm_worker_threads_actually_used = f.thread_count_actually_used ?? null;
  row.fm_worker_threads_actually_used_before = fBefore.thread_count_actually_used ?? null;
  row.fm_worker_threads_actually_used_after = fAfter.thread_count_actually_used ?? null;
  row.fm_worker_threads_observed_match =
    fBefore.thread_count_actually_used === fAfter.thread_count_actually_used;
  row.fm_thread_probe = f.thread_probe ?? null;
  row.affinity = {
    mask: f.affinity_mask ?? env.affinity_mask,
    cpus: f.affinity_cpus ?? env.affinity_cpus,
    source: f.affinity_source ?? env.affinity_source,
  };
  row.fm_elf_sha256 = binaryRecordBefore.elf_sha256;
  row.fm_elf_bytes = binaryRecordBefore.elf_bytes;
  row.fm_min_sample_ns = f.min_sample_ns ?? null;
  row.fm_calibration_target_ns = f.calibration_target_ns ?? null;
  row.fm_batch = f.batch;
  row.fm_integrated_sample_ns = medianNumber(f.effect_integrated_samples_ns);
  row.fm_effect_integrated_samples_ns = f.effect_integrated_samples_ns ?? null;
  row.fm_null_integrated_samples_ns = f.null_integrated_samples_ns ?? null;
  row.fm_output_sha256 = nativeResultSha256(f);
  row.fm_native_result_sha256 = nativeResultSha256(f);
  row.fm_output_sha256_lean = f.output_sha256_lean ?? null;
  row.measurement_mode = measurementMode;
  row.measurement_boundary = f.measurement_boundary;
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
  row.fm_p50_ns = fTiming.p50;
  row.fm_min_ns = fTiming.min;
  row.fm_cv_pct = f.cv_pct;
  row.fm_mad_pct = f.mad_pct;
  row.fm_null_control = f.null_control ?? null;
  row.fm_profile_ab = f.profile_ab ?? null;
  row.fm_bytes = nativeResultBytes(f);
  row.fm_bytes_lean = f.output_bytes_lean ?? null;
  row.fm_lean_p50_ns = f.pipeline_lean_ns?.p50 ?? null;
  row.fm_documents_per_second = (f.revisions * 1e9) / fTiming.p50;
  // Recorded because it is currently > 1: the lean output profile is smaller but *slower*, since
  // A11yConfig::none() drops off the streaming fast path onto the per-element Element builder.
  row.lean_slowdown = measurementMode === 'render'
    ? f.pipeline_lean_ns.p50 / f.pipeline_ns.p50
    : null;

  if (has('skip-mermaid')) {
    rows.push({ ...row, status: 'fm_only' });
    continue;
  }
  if (
    (threadSweep.length > 0 || measurementMode === 'parse') &&
    m &&
    (
      !validIncumbentThreadProvenance(m) ||
      !validSha256(m.bundle_sha256) ||
      m.bundle_sha256 !== PINS.mermaid.sha256
    )
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'comparator_execution_model_mismatch',
      error:
        'cross-engine row requires the pinned mermaid-js bundle and an observed single CDP main-thread execution context',
    });
    continue;
  }
  if (m) {
    row.mjs_worker_threads = m.worker_threads ?? 1;
    row.mjs_worker_threads_requested = m.thread_count_requested ?? 1;
    row.mjs_worker_threads_actually_used = m.thread_count_actually_used ?? null;
    row.mjs_execution_model = m.execution_model ?? 'single_page_main_thread';
    row.mjs_bundle_sha256 = m.bundle_sha256 ?? null;
    row.mjs_chromium_binary = m.chromium_binary ?? null;
    row.mjs_chromium_version = m.chromium_version ?? null;
    row.semantic_work = semanticWorkGate(f, m);
    if (row.semantic_work.verdict !== 'equal') {
      hardFail = true;
      rows.push({
        ...row,
        status: 'semantic_work_mismatch',
        error:
          'frankenmermaid and mermaid-js must execute the same boundary over the same accepted, '
            + 'byte-identical diagram set',
      });
      continue;
    }
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
    // Only the dedicated one-render probe can bound one job. A timed-phase deadline covers
    // calibration, warmup, A/A and effect samples together, so dividing that whole item budget by
    // one Rust job would invent a per-job bound. A raised failure likewise bounds nothing.
    row.speedup_lower_bound = comparatorDnfLowerBound(m, fTiming.p50);
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
  const mTiming = incumbentTimingStats(m);
  if (!mTiming) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'comparator_boundary_mismatch',
      error: `mermaid-js did not report ${measurementMode} timing samples`,
    });
    continue;
  }
  if (
    measurementMode === 'parse' &&
    !validParseSampleFloor(m, true)
  ) {
    hardFail = true;
    rows.push({
      ...row,
      status: 'sample_floor_violation',
      error:
        `every mermaid-js parse effect and A/A arm must integrate for at least `
        + `${PARSE_MIN_SAMPLE_NS} ns`,
    });
    continue;
  }
  row.mjs_p50_ns = mTiming.p50;
  row.mjs_min_ns = mTiming.min;
  row.mjs_cv_pct = m.cv_pct;
  row.mjs_mad_pct = m.mad_pct;
  row.mjs_null_control = m.null_control ?? null;
  row.mjs_batch = m.batch ?? 1;
  row.mjs_min_sample_ns = m.min_sample_ns ?? null;
  row.mjs_calibration_target_ns = m.calibration_target_ns ?? null;
  row.mjs_integrated_sample_ns = medianNumber(m.effect_integrated_samples_ns);
  row.mjs_effect_integrated_samples_ns = m.effect_integrated_samples_ns ?? null;
  row.mjs_null_integrated_samples_ns = m.null_integrated_samples_ns ?? null;
  row.mjs_effect_batches = m.effect_batches ?? null;
  row.mjs_null_batches = m.null_batches ?? null;
  row.mjs_bytes = nativeResultBytes(m);
  row.mjs_native_result_sha256 = nativeResultSha256(m);
  row.mjs_documents_per_second = (m.revisions * 1e9) / mTiming.p50;
  row.measurement_unit =
    measurementMode === 'parse' ? 'whole_parse_job_wall_time' : 'whole_job_wall_time';
  row.speedup = mTiming.p50 / fTiming.p50;
  row.effect_ci_required =
    measurementMode === 'parse' || item.effect_ci_required === true;
  row.effect_ci = bootstrapMedianRatioCi(
    mTiming.samples,
    fTiming.samples,
  );
  // Noise is one-sided, so the min-vs-min ratio is the estimate least contaminated by preemption.
  // If it disagrees with the p50 ratio, the run was noisy and the claim is not robust.
  row.speedup_min = mTiming.min / fTiming.min;
  row.speedup_lean =
    measurementMode === 'render' ? mTiming.p50 / f.pipeline_lean_ns.p50 : null;
  row.bytes_ratio = measurementMode === 'render' ? m.output_bytes / f.output_bytes : null;
  row.bytes_ratio_lean =
    measurementMode === 'render' ? m.output_bytes / f.output_bytes_lean : null;
  if (f.revisions > 1 && row.class !== 'doc_build') {
    // For an editing session the number that matters is the cost of one keystroke's re-render,
    // not the cost of the whole trace.
    row.fm_ns_per_revision = fTiming.p50 / f.revisions;
    row.mjs_ns_per_revision = mTiming.p50 / m.revisions;
  }
  // CV and MAD remain provenance only. The only blocking statistical decision is whether the
  // cross-runtime median ratio clears both same-invocation null-CI floors.
  row.median_ci_gate = medianCiGate(
    row.speedup,
    [fBefore.null_control, fAfter.null_control, m.null_control],
    row.effect_ci,
    row.effect_ci_required,
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
      row.fm_parallel_efficiency_requested =
        row.fm_scaling_vs_1t / Math.max(1, row.fm_worker_threads);
      row.fm_parallel_efficiency_observed =
        row.fm_scaling_vs_1t / Math.max(1, row.fm_worker_threads_actually_used ?? 1);
      row.fm_parallel_efficiency = row.fm_parallel_efficiency_requested;
    }
  }
}

const ok = rows.filter((r) => r.status === 'ok');
const dnf = rows.filter((r) => r.status === 'comparator_dnf');

// Attach the cross-engine content verdict to every measured row. A DNF row is exempt: mermaid
// produced nothing to compare against, which is already reported as a bound rather than a ratio.
const elfShaForEquivalence = /^[0-9a-f]{64}$/.test(String(env.fm_elf_sha256)) ? env.fm_elf_sha256 : null;
for (const row of ok) {
  const verdict = findEquivalenceVerdict(
    row.id,
    corpus.get(row.id)?.sha256,
    elfShaForEquivalence,
    PINS.mermaid.sha256,
  );
  row.output_equivalence = verdict;
  row.content_verified = verdict.status === 'verified';
  if (!row.content_verified) row.content_unverified_admitted = allowUnverifiedOutput;
}
const equivalenceFailures = ok
  .filter((r) => !r.content_verified)
  .map((r) => `${threadSweep.length > 0 ? `${r.id}@t${r.fm_worker_threads}` : r.id}:${r.output_equivalence.status}`);
const speedups = ok.map((r) => r.speedup);
const speedupsMin = ok.map((r) => r.speedup_min);
const speedupAggregate = speedups.length
  ? { min: Math.min(...speedups), median: pct(50, speedups), max: Math.max(...speedups) }
  : null;
const speedupMinAggregate = speedupsMin.length
  ? { min: Math.min(...speedupsMin), median: pct(50, speedupsMin), max: Math.max(...speedupsMin) }
  : null;
const rowLabel = (row) =>
  threadSweep.length > 0 ? `${row.id}@t${row.fm_worker_threads}` : row.id;
const measurementOrder = phaseLoad.map((phase) => phase.phase);
const expectedHostWidePhases = threadSweep.length === 0
  ? []
  : has('skip-mermaid')
    ? threadSweep.map((threads) => `frankenmermaid-before-t${threads}`)
    : [
        ...threadSweep.map((threads) => `frankenmermaid-before-t${threads}`),
        'mermaid-js',
        ...threadSweep.map((threads) => `frankenmermaid-after-t${threads}`),
      ];
const finalHostWideChecks = expectedHostWidePhases.map((phase) =>
  hostWideQuiescenceChecks.findLast((candidate) => candidate.phase === phase));
const summary = {
  schema: measurementMode === 'parse'
    ? 'frankenmermaid.headtohead.parse.v1'
    : 'frankenmermaid.headtohead.v2',
  env,
  measurement_mode: measurementMode,
  measurement_boundary:
    measurementMode === 'parse' ? 'public_parse_validate' : 'parse_layout_render_svg',
  pins: {
    mermaid: PINS.mermaid.version,
    bundle_url: PINS.mermaid.url,
    bundle_sha256: PINS.mermaid.sha256,
    security_level: PINS.mermaid.security_level,
  },
  environment_provenance_gate: {
    verdict: powerAndIsaValid ? 'pass' : (powerAndIsaRequired ? 'fail' : 'not_required'),
    required: powerAndIsaRequired,
    rule:
      'cross-engine evidence requires complete ISA flags and one internally consistent full-CPU power policy',
    isa: env.isa,
    power_policy: powerPolicySummary(env.power_policy),
  },
  corpus_items: items.length,
  measurement_rows: rows.length,
  ok_items: ok.length,
  // Items where mermaid did not complete the selected public API inside its wall budget.
  // `speedup` on purpose: these carry lower bounds, not measured ratios.
  dnf_items: dnf.length,
  dnf: dnf.map((r) => ({
    id: r.id, budget_ms: r.mjs_budget_ms, phase: r.mjs_dnf_phase,
    fm_p50_ns: r.fm_p50_ns, speedup_lower_bound: r.speedup_lower_bound, error: r.error,
  })),
  // Rendered structural equivalence is the common semantic oracle for both modes. In parse mode it
  // proves the native parse results feed equivalent user-visible output without forcing either
  // engine to serialize into the other's internal AST representation.
  output_equivalence_gate: {
    verdict: equivalenceFailures.length === 0
      ? 'pass'
      : (allowUnverifiedOutput ? 'admitted_unverified' : 'fail'),
    rule: 'every measured row needs a passing cross-engine equivalence verdict from the same input, '
      + 'Rust ELF and mermaid bundle (scripts/headtohead/equivalence.mjs)',
    method: (measurementMode === 'parse' ? 'linked_parse_semantics_via_' : '')
      + 'svg_structural (rendered-text token containment + rendered-path edge topology '
      + 'cross-checked against input truth); not byte equality, not a rasterized perceptual diff',
    allow_unverified_output: allowUnverifiedOutput,
    failures: equivalenceFailures,
    // Spelled out so a row produced under the override cannot be quoted as verified.
    caveat: equivalenceFailures.length > 0 && allowUnverifiedOutput
      ? 'these rows compare renders that are NOT known to carry the same content; the ratio is a '
        + 'performance observation, not a like-for-like speedup claim'
      : null,
  },
  median_ci_gate_rule: 'effect CI excludes 1.0 when required; claim magnitude >= max(1.01, '
    + '1 + 2 * max(per-engine A/A CI radius)); every null median stays within 2% of 1.0',
  measurement_order: measurementOrder,
  fm_bracket_gate_rule: 'Rust pre/post drift magnitude <= max(1.01, 1 + 2 * Rust A/A CI radius)',
  fm_bracket_gate_failures: ok
    .filter((r) => r.fm_bracket.verdict === 'fail')
    .map(rowLabel),
  thread_sweep: threadSweep.length > 0
    ? {
        threads: threadSweep,
        requested_threads: threadSweep,
        host_wide_exclusivity: {
          verdict:
            finalHostWideChecks.length === expectedHostWidePhases.length &&
            finalHostWideChecks.every((check) => check?.verdict === 'clear')
              ? 'clear'
              : 'blocked',
          exclusive_host_claim: exclusiveHostClaim,
          claim_reference_format: 'trj-booking:<Agent-Mail-CLAIM-message-id>',
          complete_host_cpuset: env.affinity_cpus.length === env.logical_threads,
          maximum_busy_fraction: HOST_WIDE_MAX_BUSY_FRACTION,
          sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
          maximum_admission_attempts: HOST_WIDE_QUIET_MAX_ATTEMPTS,
          checked_before_every_measured_phase: true,
          expected_phase_count: expectedHostWidePhases.length,
          sample_attempt_count: hostWideQuiescenceChecks.length,
          final_phase_checks: finalHostWideChecks,
          checks: hostWideQuiescenceChecks,
        },
        actual_observed_threads: rows
          .filter((row) => Number.isSafeInteger(row.fm_worker_threads_actually_used))
          .map((row) => ({
            requested: row.fm_worker_threads_requested,
            available_parallelism: row.fm_available_parallelism,
            oversubscribed: row.fm_oversubscribed,
            before: row.fm_worker_threads_actually_used_before,
            after: row.fm_worker_threads_actually_used_after,
            selected: row.fm_worker_threads_actually_used,
          })),
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
  speedup: measurementMode === 'render' ? speedupAggregate : null,
  parse_speedup: measurementMode === 'parse' ? speedupAggregate : null,
  speedup_min_estimator: measurementMode === 'render' ? speedupMinAggregate : null,
  parse_speedup_min_estimator: measurementMode === 'parse' ? speedupMinAggregate : null,
  rows,
};
if (
  summary.thread_sweep &&
  summary.thread_sweep.host_wide_exclusivity.verdict !== 'clear'
) {
  hardFail = true;
}

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
    const bounded = Number.isFinite(r.speedup_lower_bound);
    console.log(
      `${pad(displayId, 22)}${lpad(r.nodes, 6)}${lpad(r.edges, 7)}${lpad(ms(r.fm_p50_ns), 12)}` +
      `${lpad(bounded ? `DNF>${(r.mjs_budget_ms / 1000).toFixed(0)}s` : (timedOut ? 'TIMEOUT' : 'CANNOT'), 12)}` +
      `${lpad(bounded ? `>${r.speedup_lower_bound.toFixed(0)}x` : 'n/a', 10)}` +
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
  const bytesRatio = Number.isFinite(r.bytes_ratio) ? `${r.bytes_ratio.toFixed(2)}x` : '-';
  const leanBytesRatio =
    Number.isFinite(r.bytes_ratio_lean) ? `${r.bytes_ratio_lean.toFixed(2)}x` : '-';
  console.log(
    pad(displayId, 22) + lpad(r.nodes, 6) + lpad(r.edges, 7) + lpad(ms(r.fm_p50_ns), 12) + lpad(ms(r.mjs_p50_ns), 12) +
    lpad(`${r.speedup.toFixed(0)}x`, 10) + lpad(`${r.speedup_min.toFixed(0)}x`, 10) + lpad(r.fm_mad_pct.toFixed(1), 9) +
    lpad(bytesRatio, 9) + lpad(leanBytesRatio, 8) +
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
      `  requested t${String(r.fm_worker_threads_requested).padStart(3)}  ` +
      `observed ${String(r.fm_worker_threads_actually_used).padStart(3)}  ` +
      `${r.fm_oversubscribed ? `(oversubscribed on ${r.fm_available_parallelism})  ` : ''}` +
      `${ms(r.fm_p50_ns)} ms  ` +
      `${r.fm_scaling_vs_1t.toFixed(2)}x vs t1  ` +
      `${(r.fm_parallel_efficiency_observed * 100).toFixed(1)}% observed efficiency  ` +
      versusIncumbent,
    );
  }
  console.log(
    `  scalar/parallel SVG identity: ${summary.thread_sweep.scalar_output_identity.verdict}`,
  );
  console.log(
    `  host-wide exclusivity: ${summary.thread_sweep.host_wide_exclusivity.verdict} ` +
    `(${summary.thread_sweep.host_wide_exclusivity.checks.length} pre-phase checks, ` +
    `claim ${summary.thread_sweep.host_wide_exclusivity.exclusive_host_claim})`,
  );
  console.log('');
}
const displayedSpeedup = summary.speedup ?? summary.parse_speedup;
const displayedMinSpeedup =
  summary.speedup_min_estimator ?? summary.parse_speedup_min_estimator;
if (displayedSpeedup) {
  console.log(`${measurementMode} speedup vs mermaid ${PINS.mermaid.version} (p50):  min ${displayedSpeedup.min.toFixed(0)}x  median ${displayedSpeedup.median.toFixed(0)}x  max ${displayedSpeedup.max.toFixed(0)}x`);
  console.log(`${measurementMode} speedup vs mermaid ${PINS.mermaid.version} (min):  min ${displayedMinSpeedup.min.toFixed(0)}x  median ${displayedMinSpeedup.median.toFixed(0)}x  max ${displayedMinSpeedup.max.toFixed(0)}x`);
}
for (const r of ok.filter((x) => x.class === 'doc_build')) {
  console.log(
    `whole job ${r.id}: ${r.revisions} diagrams -- frankenmermaid ${ms(r.fm_p50_ns)} ms vs `
      + `mermaid ${ms(r.mjs_p50_ns)} ms; no per-diagram mean is used for the verdict`,
  );
}
for (const r of ok.filter((x) => x.class !== 'doc_build' && x.revisions > 1)) {
  const unit = measurementMode === 'parse' ? 'parse' : 're-render';
  console.log(
    `${r.class} ${r.id}: ${r.revisions} revisions -- per ${unit} frankenmermaid `
      + `${ms(r.fm_ns_per_revision)} ms vs mermaid ${ms(r.mjs_ns_per_revision)} ms `
      + (measurementMode === 'parse' ? '(equal-work public parser boundary.)' : '(a live preview redraws on every keystroke.)'),
  );
}
if (dnf.length) {
  console.log('');
  console.log(
    `DID NOT FINISH -- mermaid ${PINS.mermaid.version} did not complete ${measurementMode} `
      + `on ${dnf.length} item(s):`,
  );
  for (const r of dnf) {
    console.log(`  ${pad(r.id, 22)} ${r.nodes} nodes / ${r.edges} edges -- ${r.mjs_dnf_phase} phase, budget ${(r.mjs_budget_ms / 1000).toFixed(0)}s`);
    if (r.mjs_dnf_kind === 'timeout') {
      console.log(`  ${' '.repeat(22)} still working when the budget expired (${r.error}).`);
      if (Number.isFinite(r.speedup_lower_bound)) {
        console.log(`  ${' '.repeat(22)} frankenmermaid: ${ms(r.fm_p50_ns)} ms, so the speedup is at least ${r.speedup_lower_bound.toFixed(0)}x -- a bound, not a measurement.`);
      } else {
        console.log(
          `  ${' '.repeat(22)} this deadline covered the multi-phase measurement, not one exact `
            + 'job; no per-job speedup bound is stated.',
        );
      }
    } else {
      console.log(`  ${' '.repeat(22)} FAILED after ${(r.mjs_elapsed_ms / 1000).toFixed(1)}s: ${r.error}`);
      console.log(
        `  ${' '.repeat(22)} frankenmermaid: ${ms(r.fm_p50_ns)} ms. mermaid does not `
          + `${measurementMode} this input at any budget, so there is no ratio to state.`,
      );
    }
  }
}
const leanSlow = ok.filter((r) => r.lean_slowdown > 1.05);
if (leanSlow.length) {
  const worst = leanSlow.reduce((a, b) => (b.lean_slowdown > a.lean_slowdown ? b : a));
  console.log(`note: the lean output profile is smaller but SLOWER on ${leanSlow.length}/${ok.length} items (worst ${worst.id}: ${worst.lean_slowdown.toFixed(2)}x) -- A11yConfig::none() falls off the streaming fast path.`);
}
const equivGate = summary.output_equivalence_gate;
if (equivGate.verdict !== 'pass') {
  console.log('');
  console.log(`OUTPUT EQUIVALENCE GATE ${equivGate.verdict === 'fail' ? 'FAIL' : 'ADMITTED UNVERIFIED'}: `
    + `${equivGate.failures.join(', ')}`);
  for (const r of ok.filter((row) => !row.content_verified)) {
    const e = r.output_equivalence;
    if (e.status === 'divergent') {
      console.log(`  ${r.id}: ${e.equivalent}/${e.diagrams} diagrams equivalent; `
        + `divergent/unverified families: ${e.divergent_families.join(', ')}`);
    } else {
      console.log(`  ${r.id}: ${e.status}${e.reason ? ` (${e.reason})` : ''} -- run scripts/headtohead/equivalence.mjs`);
    }
  }
  console.log(
    `  a ${measurementMode} ratio without linked equivalent rendered semantics is not a like-for-like speedup.`,
  );
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
// Last, so a run that is both slow-verified and content-divergent still reports the statistical
// gates first; content divergence is the more fundamental problem but the least ambiguous to fix.
if (equivGate.verdict === 'fail') process.exit(7);
