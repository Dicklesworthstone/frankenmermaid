#!/usr/bin/env node
// Choose the cores for both arms and print them as JSON.
//
// Exists so a measurement driver that is NOT run.mjs -- abba_render.py, say -- can pin both arms by
// the SAME rule rather than reimplementing core selection. Two implementations of "which core" is
// exactly how the two arms end up under different clock regimes, which is the defect bd-hmfi records:
// our arm was pinned to the 1429 MHz floor while the incumbent ran unpinned on boosted cores.
//
// Reads nothing but /proc/stat and cpufreq, runs no benchmark, and prints one JSON object.
import { readFileSync } from 'node:fs';
import { selectPinnedCpu, selectPinnedCpuSet } from './cpu_selection.mjs';

function snapshot() {
  const out = new Map();
  for (const line of readFileSync('/proc/stat', 'utf8').split('\n')) {
    if (!/^cpu\d/.test(line)) continue;
    const fields = line.trim().split(/\s+/);
    const cpu = Number(fields[0].slice(3));
    const total = fields.slice(1).reduce((a, b) => a + Number(b), 0);
    out.set(cpu, { total, idle: Number(fields[4]) });
  }
  return out;
}

function mhz(cpu) {
  try {
    const khz = Number.parseInt(
      readFileSync(`/sys/devices/system/cpu/cpu${cpu}/cpufreq/scaling_cur_freq`, 'utf8'),
      10,
    );
    return Number.isFinite(khz) ? Math.round(khz / 1000) : null;
  } catch {
    return null;
  }
}

const before = snapshot();
const deadline = Date.now() + 300;
while (Date.now() < deadline) {
  // Busy-wait rather than sleep: this is a 300ms sampling interval, not a wait loop, and it must not
  // yield the very core it is measuring.
}
const after = snapshot();

const records = [];
for (const [cpu, first] of before) {
  const next = after.get(cpu);
  if (!next) continue;
  const dTotal = Math.max(1, next.total - first.total);
  records.push({
    cpu,
    busy: 1 - (next.idle - first.idle) / dTotal,
    mhz: mhz(cpu),
  });
}

const size = Number(process.argv[2] ?? 8);
const single = selectPinnedCpu(records);
const set = selectPinnedCpuSet(records, Number.isInteger(size) && size > 0 ? size : 8);
const clocks = records.map((r) => r.mhz).filter((m) => typeof m === 'number');

console.log(
  JSON.stringify({
    fm_cpu: single.chosen.cpu,
    fm_mhz: single.chosen.mhz ?? null,
    fm_busy_pct: Number((single.chosen.busy * 100).toFixed(1)),
    fm_rule: single.rule,
    incumbent_cpus: set.cpus,
    incumbent_min_mhz: set.min_mhz,
    incumbent_starved: set.starved,
    incumbent_rule: set.rule,
    band_size: single.band_size,
    host_min_mhz: clocks.length ? Math.min(...clocks) : null,
    host_max_mhz: clocks.length ? Math.max(...clocks) : null,
    host_spread: clocks.length ? Number((Math.max(...clocks) / Math.min(...clocks)).toFixed(3)) : null,
    busy_cpus_over_20pct: records.filter((r) => r.busy >= 0.2).length,
    total_cpus: records.length,
  }),
);
