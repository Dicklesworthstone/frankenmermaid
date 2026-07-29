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
const THREAD_SWEEP_MIN_SAMPLE_NS = 50_000_000;
const THREAD_SWEEP_CALIBRATION_TARGET_NS = 75_000_000;
const HOST_WIDE_MAX_BUSY_FRACTION = 0.20;
const HOST_WIDE_QUIET_SAMPLE_MS = 1_000;

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : fallback;
}
const has = (name) => process.argv.includes(`--${name}`);

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

function validElfSelfReport(record) {
  return (
    record?.record === 'binary' &&
    /^[0-9a-f]{64}$/.test(record.elf_sha256) &&
    Number.isSafeInteger(record.elf_bytes) &&
    record.elf_bytes > 0
  );
}

const validSha256 = (value) => typeof value === 'string' && /^[0-9a-f]{64}$/.test(value);

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
    record.thread_probe?.method === 'instrumented_caller_worker_union_over_exact_workload' &&
    record.thread_probe?.caller_workers_observed === observed &&
    record.thread_probe?.probe_batch === record.batch &&
    record.thread_probe?.inside_timed_region === false &&
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
    record.execution_model === 'single_page_main_thread'
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
    batch: 7,
    affinity_cpus: [0, 1, 2, 3],
    affinity_source: 'linux_proc_status',
    thread_probe: {
      method: 'instrumented_caller_worker_union_over_exact_workload',
      caller_workers_observed: 3,
      probe_batch: 7,
      inside_timed_region: false,
    },
  };
  if (
    !validRustThreadProvenance(validThreads, 4) ||
    validRustThreadProvenance({ ...validThreads, thread_count_actually_used: null }, 4)
  ) {
    throw new Error('actual-thread provenance validation regression');
  }
  const validIncumbentThreads = {
    worker_threads: 1,
    thread_count_requested: 1,
    thread_count_actually_used: 1,
    execution_model: 'single_page_main_thread',
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
    host_topology_gate: 'required',
    live_topology: {
      host_identity: liveTopology.host_identity,
      physical_cores: liveTopology.physical_cores,
      logical_threads: liveTopology.logical_threads,
      numa_nodes: liveTopology.numa_nodes,
      affinity_logical_cpus: liveTopology.affinity_cpus.length,
    },
    actual_thread_probe_gate: 'required',
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
if (threadSweep.length > 0 && !validHostTopology(env)) {
  console.error('[run] INVALID: thread sweeps require host identity, physical/logical topology, RAM, NUMA, and affinity provenance');
  process.exit(2);
}
if (threadSweep.some((threads) => threads > env.cpu_count)) {
  console.error(
    `[run] --thread-sweep requests ${Math.max(...threadSweep)} threads, but this host reports only ${env.cpu_count} logical CPUs`,
  );
  process.exit(2);
}
if (threadSweep.some((threads) => threads > env.affinity_cpus.length)) {
  console.error(
    `[run] --thread-sweep requests ${Math.max(...threadSweep)} threads, but this process affinity exposes only ${env.affinity_cpus.length} logical CPUs`,
  );
  process.exit(2);
}
if (threadSweep.length > 0 && env.affinity_cpus.length !== env.logical_threads) {
  console.error(
    `[run] host-wide thread sweeps require the complete host cpuset; affinity exposes ${env.affinity_cpus.length} of ${env.logical_threads} logical CPUs`,
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
      cpu_power_policy_required: true,
      cpu_isa_provenance_required: true,
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
// The sweep additionally re-checks the CPU power policy and samples every affinity CPU while the
// host is idle immediately before each measured phase. Any policy drift or CPU above the fixed 20%
// ceiling blocks the invocation before more evidence is produced. Across-phase aggregate busyness
// remains report-only because it includes the engines' own work.
const phaseLoad = [];
const hostWideQuiescenceChecks = [];

function requireHostWideQuiescence(label) {
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
  if (record.verdict !== 'clear') {
    const busy = record.busy_cpus_above_limit
      .slice(0, 12)
      .map(({ cpu, busy_fraction }) => `cpu${cpu}=${(busy_fraction * 100).toFixed(1)}%`)
      .join(',');
    console.error(
      `[run] HOST-WIDE EXCLUSIVITY BLOCKED before ${label}: ` +
      `missing=[${record.missing_cpus.join(',')}] busy=[${busy}] ` +
      `power-policy=${livePowerPolicyValid ? (powerPolicyMatchesBaseline ? 'match' : 'changed') : 'invalid'} ` +
      `(limit ${(HOST_WIDE_MAX_BUSY_FRACTION * 100).toFixed(1)}%)`,
    );
    process.exit(6);
  }
  console.error(
    `[run] host-wide exclusivity clear before ${label}: ` +
    `${record.allowed_cpu_count} CPUs, max ${(record.observed_max_busy_fraction * 100).toFixed(1)}%`,
  );
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
  if (threadSweep.length > 0) requireHostWideQuiescence(label);
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
          FM_H2H_THREAD_PROBE: '1',
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
        (record.thread_probe_required === true &&
          record.thread_count_requested === record.worker_threads &&
          record.affinity_mask === env.affinity_mask &&
          JSON.stringify(record.affinity_cpus) === JSON.stringify(env.affinity_cpus))) &&
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
        (record.thread_probe_required === true &&
          record.thread_count_requested === record.worker_threads &&
          record.affinity_mask === env.affinity_mask &&
          JSON.stringify(record.affinity_cpus) === JSON.stringify(env.affinity_cpus))) &&
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
    const check = hostWideQuiescenceChecks.find((candidate) => candidate.phase === phase);
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
  if (
    threadSweep.length > 0 &&
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
        'thread sweep requires the pinned mermaid-js bundle and an observed single CDP main-thread execution context',
    });
    continue;
  }
  if (m) {
    row.mjs_worker_threads = m.worker_threads ?? 1;
    row.mjs_worker_threads_requested = m.thread_count_requested ?? 1;
    row.mjs_worker_threads_actually_used = m.thread_count_actually_used ?? null;
    row.mjs_execution_model = m.execution_model ?? 'single_page_main_thread';
    row.mjs_bundle_sha256 = m.bundle_sha256 ?? null;
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
  row.mjs_p50_ns = m.render_ns.p50;
  row.mjs_min_ns = m.render_ns.min;
  row.mjs_cv_pct = m.cv_pct;
  row.mjs_mad_pct = m.mad_pct;
  row.mjs_null_control = m.null_control ?? null;
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
const speedups = ok.map((r) => r.speedup);
const speedupsMin = ok.map((r) => r.speedup_min);
const rowLabel = (row) =>
  threadSweep.length > 0 ? `${row.id}@t${row.fm_worker_threads}` : row.id;
const measurementOrder = phaseLoad.map((phase) => phase.phase);
const summary = {
  schema: 'frankenmermaid.headtohead.v2',
  env,
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
        requested_threads: threadSweep,
        host_wide_exclusivity: {
          verdict:
            hostWideQuiescenceChecks.length ===
              (has('skip-mermaid') ? threadSweep.length : threadSweep.length * 2 + 1) &&
            hostWideQuiescenceChecks.every((check) => check.verdict === 'clear')
              ? 'clear'
              : 'blocked',
          exclusive_host_claim: exclusiveHostClaim,
          claim_reference_format: 'trj-booking:<Agent-Mail-CLAIM-message-id>',
          complete_host_cpuset: env.affinity_cpus.length === env.logical_threads,
          maximum_busy_fraction: HOST_WIDE_MAX_BUSY_FRACTION,
          sample_ms: HOST_WIDE_QUIET_SAMPLE_MS,
          checked_before_every_measured_phase: true,
          expected_check_count: has('skip-mermaid')
            ? threadSweep.length
            : threadSweep.length * 2 + 1,
          checks: hostWideQuiescenceChecks,
        },
        actual_observed_threads: rows
          .filter((row) => Number.isSafeInteger(row.fm_worker_threads_actually_used))
          .map((row) => ({
            requested: row.fm_worker_threads_requested,
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
  speedup: speedups.length
    ? { min: Math.min(...speedups), median: pct(50, speedups), max: Math.max(...speedups) }
    : null,
  speedup_min_estimator: speedupsMin.length
    ? { min: Math.min(...speedupsMin), median: pct(50, speedupsMin), max: Math.max(...speedupsMin) }
    : null,
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
      `  requested t${String(r.fm_worker_threads_requested).padStart(3)}  ` +
      `observed ${String(r.fm_worker_threads_actually_used).padStart(3)}  ` +
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
