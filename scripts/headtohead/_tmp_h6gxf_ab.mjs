// bd-h6gxf: instruction-counted, interleaved A/B for a lever that CHANGES OUTPUT ON PURPOSE.
//
// `ab_instr.mjs` refuses when any arm's output hash moves, and it is right to: a "win" that changed
// the picture is not a win. This lever's whole point is that six of schema_catalog_25's twenty-five
// revisions stop being laid out by the algorithm a wrong cost model forced on them, so that refusal
// would fire on the intended effect and hide the measurement behind it.
//
// The gate is therefore not weakened, it is SHARPENED: instead of "nothing may change", this asserts
// EXACTLY WHICH revisions change. An unmeasured revision moving is still a hard failure, and so is
// an expected one failing to move — the second is what a build that never picked up the change
// looks like, and it passes a "nothing changed" check trivially.
//
// Per-revision bytes come from FM_H2H_DUMP_ALL, whose concatenation the engine hashes into the
// `output_sha256` it reports for its timed rounds; that linkage is what makes the inspected bytes
// the measured bytes rather than a second, differently produced render.
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

const args = new Map();
for (let i = 2; i < process.argv.length; i += 2) args.set(process.argv[i].replace(/^--/, ''), process.argv[i + 1]);
const baseBin = args.get('base');
const candBin = args.get('cand');
const nullBin = args.get('null');
const corpus = args.get('corpus');
const dumpRoot = args.get('dump-root');
const rounds = Number(args.get('rounds') ?? 7);
const expectMoved = (args.get('expect-moved') ?? '').split(',').filter(Boolean).map(Number);
if (!baseBin || !candBin || !nullBin || !corpus || !dumpRoot) {
  console.error('usage: --base <elf> --cand <elf> --null <elf> --corpus <json> --dump-root <dir>\n' +
                '       [--rounds N] [--expect-moved 0,4,9,...] [--expect-base <rev>] [--expect-cand <rev>]');
  process.exit(2);
}

const fileSha = (p) => createHash('sha256').update(fs.readFileSync(p)).digest('hex');

function run(bin, { dumpDir = null } = {}) {
  const perfOut = `${bin}.h6gxf.perf.txt`;
  const env = { ...process.env, FM_H2H_FORCE_PROFILE: 'default', FM_H2H_THREADS: '1', FM_H2H_MODE: 'render' };
  // The dump directory is the example's SECOND POSITIONAL argument, not an env var; FM_H2H_DUMP_ALL
  // only widens an already-enabled dump from the last revision to all of them.
  if (dumpDir) env.FM_H2H_DUMP_ALL = '1';
  const argv = ['stat', '-e', 'instructions', '-x', ',', '-o', perfOut, bin, corpus];
  if (dumpDir) argv.push(dumpDir);
  const stdout = execFileSync('perf', argv, { env, maxBuffer: 1 << 28, encoding: 'utf8' });
  const row = fs.readFileSync(perfOut, 'utf8').split('\n').find((l) => l.includes('instructions'));
  if (!row) throw new Error(`perf reported no instructions row for ${bin}`);
  const instructions = Number(row.split(',')[0]);
  if (!Number.isFinite(instructions)) throw new Error(`unparsable perf row: ${row}`);
  const records = stdout.split('\n').filter(Boolean).map((l) => JSON.parse(l));
  const binary = records.find((r) => r.record === 'binary');
  const items = records.filter((r) => r.status === 'ok');
  if (!binary) throw new Error(`${bin} emitted no binary provenance record`);
  return {
    instructions,
    selfElf: binary.elf_sha256,
    buildRevision: binary.build_git_revision ?? '',
    joint: items.map((r) => `${r.id}:${r.output_sha256 ?? 'none'}`).join(' '),
    items: items.length,
  };
}

// ---- provenance ------------------------------------------------------------------------------
const disk = { base: fileSha(baseBin), cand: fileSha(candBin), null: fileSha(nullBin) };
for (const arm of ['base', 'cand', 'null']) console.log(`on-disk  ${arm} ${disk[arm]}`);
if (disk.base === disk.cand) { console.error('REFUSED: base and cand are the same ELF'); process.exit(3); }
if (disk.base !== disk.null) { console.error('REFUSED: the null arm is not a byte-identical copy of the baseline'); process.exit(3); }

// ---- blast radius, before any timing ---------------------------------------------------------
const dumpDirs = {};
for (const arm of ['base', 'cand']) {
  dumpDirs[arm] = path.join(dumpRoot, arm);
  fs.rmSync(dumpDirs[arm], { recursive: true, force: true });
  fs.mkdirSync(dumpDirs[arm], { recursive: true });
  const r = run({ base: baseBin, cand: candBin }[arm], { dumpDir: dumpDirs[arm] });
  const want = args.get(`expect-${arm}`);
  if (want && !r.buildRevision.startsWith(want)) {
    console.error(`REFUSED: ${arm} was built from ${r.buildRevision}, not the expected ${want}`);
    process.exit(3);
  }
  if (r.selfElf !== disk[arm]) {
    console.error(`REFUSED: ${arm} self-reported ${r.selfElf} but the file on disk is ${disk[arm]}`);
    process.exit(3);
  }
  console.log(`${arm} revision ${r.buildRevision || '(unstamped)'}  joint ${r.joint}`);
}
const revFiles = (dir) => fs.readdirSync(dir).filter((f) => /\.rev\d+\.default\.svg$/.test(f)).sort();
const baseRevs = revFiles(dumpDirs.base);
const candRevs = revFiles(dumpDirs.cand);
if (baseRevs.length === 0 || baseRevs.join() !== candRevs.join()) {
  console.error(`REFUSED: the two arms dumped different revision sets (${baseRevs.length} vs ${candRevs.length})`);
  process.exit(3);
}
const moved = [];
for (const [i, f] of baseRevs.entries()) {
  const a = fileSha(path.join(dumpDirs.base, f));
  const b = fileSha(path.join(dumpDirs.cand, f));
  if (a !== b) moved.push(i);
}
console.log(`\nblast radius: ${moved.length} of ${baseRevs.length} revisions changed -> [${moved.join(',')}]`);
if (expectMoved.length) {
  const same = moved.length === expectMoved.length && moved.every((v, i) => v === expectMoved[i]);
  if (!same) {
    console.error(`REFUSED: expected exactly [${expectMoved.join(',')}] to move, got [${moved.join(',')}]`);
    process.exit(4);
  }
  console.log(`blast radius matches the selection diff exactly`);
}

// ---- interleaved instruction sweep -----------------------------------------------------------
const series = { base: [], cand: [], null: [] };
const joints = { base: null, cand: null, null: null };
for (let round = 0; round < rounds; round += 1) {
  const order = round % 2 === 0 ? ['base', 'null', 'cand'] : ['cand', 'null', 'base'];
  const binFor = { base: baseBin, cand: candBin, null: nullBin };
  const seen = {};
  for (const arm of order) {
    const r = run(binFor[arm]);
    seen[arm] = r;
    series[arm].push(r.instructions);
    if (r.instructions < 1e8) { console.error(`REFUSED: ${arm} retired only ${r.instructions} instructions`); process.exit(3); }
    if (r.items === 0) { console.error(`REFUSED: ${arm} measured 0 items`); process.exit(3); }
    // Within an arm the joint hash must be STABLE across rounds: that is this harness's
    // determinism check, and it is what the relaxed cross-arm comparison gives up.
    if (joints[arm] === null) joints[arm] = r.joint;
    else if (joints[arm] !== r.joint) { console.error(`REFUSED: ${arm} is non-deterministic across rounds`); process.exit(4); }
  }
  if (joints.base !== joints.null) { console.error('REFUSED: the null arm did not reproduce the baseline output'); process.exit(4); }
  console.log(`round ${round} order=${order.join(',')} base=${seen.base.instructions} cand=${seen.cand.instructions} null=${seen.null.instructions}  A/B=${(seen.cand.instructions / seen.base.instructions).toFixed(6)}  A/A=${(seen.null.instructions / seen.base.instructions).toFixed(6)}`);
}

const median = (xs) => { const s = [...xs].sort((a, b) => a - b); const m = s.length >> 1; return s.length % 2 ? s[m] : (s[m - 1] + s[m]) / 2; };
const ratios = (a, b) => a.map((x, i) => x / b[i]);
const summarize = (label, xs) => { const s = [...xs].sort((a, b) => a - b); console.log(`${label}: median ${median(xs).toFixed(6)}  range ${s[0].toFixed(6)}-${s[s.length - 1].toFixed(6)}  n=${xs.length}`); };
console.log('');
console.log(`base instructions median ${median(series.base)}`);
console.log(`cand instructions median ${median(series.cand)}`);
summarize('A/A null (null/base)', ratios(series.null, series.base));
summarize('A/B      (cand/base)', ratios(series.cand, series.base));
