// Load-immune A/B for the head-to-head example: instructions, arms interleaved in ONE invocation,
// with an A/A null built from a byte-identical COPY of the baseline.
//
// WHY INSTRUCTIONS AND NOT WALL. This host is at saturation (window_check: 64/64 CPUs over the 20%
// ceiling, idle 0.0-0.1%, runq ~130). Wall time here measures the co-tenants, not the change; the
// repo's own ledger records byte-identical code moving 26.0 ms -> 17.3 ms at load 30-44 while
// instructions held A/A at 0.999-1.001 across the same window. Instructions are a per-process
// count, so they are immune to who else is running. They are BLIND to cache and to ISA changes,
// which is why this harness is only valid for pure work-removal levers.
//
// THREE ARMS, NOT TWO. The A/A null is a copy of the baseline binary at a different path, run in
// the same interleaved sweep. Without it a small A/B has nothing to be small relative to.
//
// GUARDS, each of which has caught a real false result somewhere in this project's history:
//   * every arm's ELF hash is SELF-REPORTED by the process (stdout line 1) and checked against the
//     on-disk file, because a hash taken next to the run proves nothing about which binary ran;
//   * baseline and candidate hashes must DIFFER, or the sweep is measuring the baseline twice;
//   * the null's hash must EQUAL the baseline's, or it is not a null;
//   * instruction counts must be large enough to be real work, not a binary that exited early;
//   * every arm's reported output_sha256 is collected, so a "win" that changed the output is
//     visible as a changed hash rather than as a faster number.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';

const args = new Map();
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i].replace(/^--/, ''), process.argv[i + 1]);
const baseBin = args.get('base');
const candBin = args.get('cand');
const nullBin = args.get('null');
const corpus = args.get('corpus');
const rounds = Number(args.get('rounds') ?? 7);
const profile = args.get('profile') ?? 'default';
if (!baseBin || !candBin || !nullBin || !corpus) {
  console.error(
    'usage: ab_instr.mjs --base <elf> --cand <elf> --null <elf> --corpus <json>\n' +
      '                    [--rounds N] [--profile default|lean]\n' +
      '                    [--expect-base <rev>] [--expect-cand <rev>]   <- strongly preferred',
  );
  process.exit(2);
}

const fileSha = (p) => createHash('sha256').update(fs.readFileSync(p)).digest('hex');

/** One measured run: instructions from perf, plus what the process said about itself. */
function run(bin) {
  const perfOut = `${bin}.perf.txt`;
  const stdout = execFileSync(
    'perf',
    ['stat', '-e', 'instructions', '-x', ',', '-o', perfOut, bin, corpus],
    {
      env: {
        ...process.env,
        FM_H2H_FORCE_PROFILE: profile,
        FM_H2H_THREADS: '1',
        FM_H2H_MODE: 'render',
      },
      maxBuffer: 1 << 28,
      encoding: 'utf8',
    },
  );
  const perf = fs.readFileSync(perfOut, 'utf8');
  const row = perf.split('\n').find((line) => line.includes('instructions'));
  if (!row) throw new Error(`perf reported no instructions row for ${bin}:\n${perf}`);
  const instructions = Number(row.split(',')[0]);
  if (!Number.isFinite(instructions)) throw new Error(`unparsable perf row: ${row}`);

  const records = stdout.split('\n').filter(Boolean).map((line) => JSON.parse(line));
  const binary = records.find((r) => r.record === 'binary');
  const items = records.filter((r) => r.status === 'ok');
  if (!binary) throw new Error(`${bin} emitted no binary provenance record`);
  return {
    instructions,
    selfElf: binary.elf_sha256,
    // The revision the ELF was BUILT from, as it reports itself. Checked against --expect-* below.
    buildRevision: binary.build_git_revision ?? '',
    outputs: items.map((r) => `${r.id}:${r.output_sha256 ?? r.parse_result_sha256 ?? 'none'}`).join(' '),
    items: items.length,
  };
}

// ---- provenance, before any measurement -----------------------------------------------------
const disk = { base: fileSha(baseBin), cand: fileSha(candBin), null: fileSha(nullBin) };
console.log(`on-disk  base ${disk.base}`);
console.log(`on-disk  cand ${disk.cand}`);
console.log(`on-disk  null ${disk.null}`);
if (disk.base === disk.cand) {
  console.error('REFUSED: baseline and candidate are the same ELF — the sweep would measure the baseline twice');
  process.exit(3);
}
if (disk.base !== disk.null) {
  console.error('REFUSED: the null arm is not a byte-identical copy of the baseline');
  process.exit(3);
}

// ⚠️ AN ARM CAN BE THE WRONG REVISION AND STILL PASS EVERY CHECK ABOVE. A `git checkout` that
// ABORTS over local modifications leaves the worktree on its previous commit, the build succeeds,
// the two ELF hashes still differ (so the same-binary check is satisfied) and the outputs still
// match (so the divergence check is satisfied) — and the A/B silently measures a pair of revisions
// nobody asked for. That happened here. `--expect-base` / `--expect-cand` close it by asking the
// ELF which revision it was built from; they are optional only so an unrevisioned local build can
// still be compared, and passing them is strongly preferred.
const expected = { base: args.get('expect-base'), cand: args.get('expect-cand') };
function assertRevision(arm, result) {
  const want = expected[arm];
  if (!want) return;
  if (!result.buildRevision) {
    console.error(`REFUSED: ${arm} reports no build revision, so --expect-${arm} cannot be checked`);
    process.exit(3);
  }
  if (!result.buildRevision.startsWith(want)) {
    console.error(
      `REFUSED: ${arm} was built from ${result.buildRevision}, not the expected ${want}`,
    );
    process.exit(3);
  }
}

const series = { base: [], cand: [], null: [] };
let outputs = null;
let selfChecked = false;

for (let round = 0; round < rounds; round += 1) {
  // Alternate arm order every round so any monotone drift in the host lands on both arms equally.
  const order = round % 2 === 0 ? ['base', 'null', 'cand'] : ['cand', 'null', 'base'];
  const binFor = { base: baseBin, cand: candBin, null: nullBin };
  const seen = {};
  for (const arm of order) {
    const result = run(binFor[arm]);
    seen[arm] = result;
    series[arm].push(result.instructions);
    if (!selfChecked) {
      if (result.selfElf !== disk[arm]) {
        console.error(`REFUSED: ${arm} self-reported ${result.selfElf} but the file on disk is ${disk[arm]}`);
        process.exit(3);
      }
      assertRevision(arm === 'null' ? 'base' : arm, result);
      if (result.instructions < 1e8) {
        console.error(`REFUSED: ${arm} retired only ${result.instructions} instructions — that is not this workload`);
        process.exit(3);
      }
      if (result.items === 0) {
        console.error(`REFUSED: ${arm} measured 0 items`);
        process.exit(3);
      }
    }
  }
  selfChecked = true;
  if (outputs === null) outputs = seen.base.outputs;
  for (const arm of order) {
    if (seen[arm].outputs !== outputs) {
      console.error(`OUTPUT DIVERGENCE on ${arm} round ${round}:\n  ${outputs}\n  ${seen[arm].outputs}`);
      process.exit(4);
    }
  }
  const r = (seen.cand.instructions / seen.base.instructions).toFixed(6);
  const n = (seen.null.instructions / seen.base.instructions).toFixed(6);
  console.log(`round ${round} order=${order.join(',')} base=${seen.base.instructions} cand=${seen.cand.instructions} null=${seen.null.instructions}  A/B=${r}  A/A=${n}`);
}

const median = (xs) => {
  const s = [...xs].sort((a, b) => a - b);
  const mid = s.length >> 1;
  return s.length % 2 ? s[mid] : (s[mid - 1] + s[mid]) / 2;
};
const ratios = (a, b) => a.map((x, i) => x / b[i]);
const summarize = (label, xs) => {
  const s = [...xs].sort((a, b) => a - b);
  console.log(`${label}: median ${median(xs).toFixed(6)}  range ${s[0].toFixed(6)}-${s[s.length - 1].toFixed(6)}  n=${xs.length}`);
};

console.log('');
console.log(`base instructions median ${median(series.base)}`);
console.log(`cand instructions median ${median(series.cand)}`);
summarize('A/A null (null/base)', ratios(series.null, series.base));
summarize('A/B      (cand/base)', ratios(series.cand, series.base));
console.log(`outputs identical across every arm and round: ${outputs}`);
