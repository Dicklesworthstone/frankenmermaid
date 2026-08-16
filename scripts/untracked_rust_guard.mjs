#!/usr/bin/env node
// Repo guard: untracked .rs under a directory cargo compiles.
//
// Adapted from franken_networkx's tests/python/test_no_untracked_rust_in_cargo_dirs.py after
// frankenlibc reported the failure mode across the fleet.
//
// cargo compiles every `.rs` under a crate's `tests/` as its own test target — no crate here sets
// `autotests = false` — so a throwaway probe left there is built on every `cargo test`. If it stops
// compiling it aborts the whole run, and if it is gitignored it is invisible to `git status`,
// `git log`, `git blame` and review, so the usual "what changed" reflexes find nothing. frankenlibc
// hit exactly this: 273 such files, one broken since June.
//
// The inverse is quieter still: an ignored file under `src/` that someone later references with a
// `mod` declaration builds on that machine and fails for everyone else and in CI, because the file
// was never committed.
//
// The guard is about UNTRACKED, not gitignored specifically. Ignoring is what hides a file; tracking
// is what makes it real. The report says which offenders are ignored, because that is the part a
// human cannot see any other way.
//
//   node scripts/untracked_rust_guard.mjs             scan; exit 1 on a new offender
//   node scripts/untracked_rust_guard.mjs --self-test prove the detector and the allowlist rule

import { execFileSync } from 'node:child_process';
import { existsSync, readdirSync, statSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');

// Already present when this guard was written. They are other agents' reproductions, they are
// currently INERT, and disk is at 96%, so this does not demand deletion — it exists to stop the
// SECOND one appearing unnoticed, which is how frankenlibc reached 273. The list may only shrink;
// `known_offenders_are_still_untracked` enforces that.
const KNOWN_UNTRACKED = new Set([
  'crates/fm-cli/tests/repro_scaling_bug.rs',
]);

const git = (...args) => {
  try {
    // `stdio: pipe` on stderr as well: `ls-files --error-unmatch` writes "did not match any
    // file(s) known to git" for every untracked path, which is the NORMAL case here and would
    // otherwise bury a real failure in CI logs.
    return {
      code: 0,
      out: execFileSync('git', args, { cwd: REPO, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }),
    };
  } catch (err) {
    return { code: err.status ?? 1, out: String(err.stdout ?? '') };
  }
};

/** Every directory whose .rs files cargo compiles: each crate's src/, tests/, benches/, plus root. */
function cargoSourceDirs() {
  const dirs = [];
  const crates = join(REPO, 'crates');
  if (existsSync(crates)) {
    for (const crate of readdirSync(crates)) {
      if (!existsSync(join(crates, crate, 'Cargo.toml'))) continue;
      for (const sub of ['src', 'tests', 'benches']) {
        const candidate = join(crates, crate, sub);
        if (existsSync(candidate) && statSync(candidate).isDirectory()) dirs.push(candidate);
      }
    }
  }
  for (const sub of ['tests', 'benches']) {
    const candidate = join(REPO, sub);
    if (existsSync(candidate) && statSync(candidate).isDirectory()) dirs.push(candidate);
  }
  return dirs;
}

function* rustFiles(dir) {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    if (entry.name === 'target') continue;
    const full = join(dir, entry.name);
    if (entry.isDirectory()) yield* rustFiles(full);
    else if (entry.name.endsWith('.rs')) yield full;
  }
}

export function untrackedRustFiles() {
  const offenders = [];
  for (const dir of cargoSourceDirs()) {
    for (const file of rustFiles(dir)) {
      const rel = relative(REPO, file).split('\\').join('/');
      if (git('ls-files', '--error-unmatch', rel).code === 0) continue;
      offenders.push({ path: rel, ignored: git('check-ignore', '-q', rel).code === 0 });
    }
  }
  return offenders.sort((a, b) => a.path.localeCompare(b.path));
}

function selfTest() {
  const cases = [];
  const record = (name, ok, detail) => {
    cases.push(name);
    if (!ok) throw new Error(`untracked-rust-guard self-test failed: ${name} (${JSON.stringify(detail)})`);
  };

  const dirs = cargoSourceDirs().map((d) => relative(REPO, d));
  record('scans every crate tests directory', dirs.includes('crates/fm-cli/tests'), dirs.slice(0, 6));
  record('scans crate src directories', dirs.includes('crates/fm-layout/src'), dirs.slice(0, 6));
  record('scans the workspace tests directory', dirs.includes('tests'), dirs.slice(0, 6));

  // The detector must actually find the known offender — otherwise the allowlist below is a
  // rubber stamp over a scan that sees nothing.
  const found = untrackedRustFiles().map((o) => o.path);
  record('detects the known untracked file',
    found.includes('crates/fm-cli/tests/repro_scaling_bug.rs'), found);
  record('reports the known offender as gitignored',
    untrackedRustFiles().find((o) => o.path === 'crates/fm-cli/tests/repro_scaling_bug.rs')?.ignored === true,
    found);

  // The allowlist may only shrink: an entry that is no longer untracked must be removed, so the
  // list cannot quietly become a place where new files hide.
  const stale = [...KNOWN_UNTRACKED].filter((p) => !found.includes(p));
  record('known_offenders_are_still_untracked', stale.length === 0, stale);

  console.log(JSON.stringify({ ok: true, cases: cases.length, scanned_dirs: dirs.length }));
}

function scan() {
  const offenders = untrackedRustFiles().filter((o) => !KNOWN_UNTRACKED.has(o.path));
  if (offenders.length > 0) {
    console.error(
      'untracked .rs files live in directories cargo compiles.\n' +
      'cargo builds every .rs under a crate tests/ as its own test target, so one that stops\n' +
      'compiling aborts the entire run; an ignored file under src/ that gains a `mod` declaration\n' +
      'breaks every checkout but yours. Commit them, move them outside the crate, or delete them:',
    );
    for (const { path, ignored } of offenders) {
      console.error(`  ${path}${ignored ? '   [GITIGNORED — invisible to review]' : ''}`);
    }
    process.exit(1);
  }
  console.log(JSON.stringify({ scanned: 'ok', allowlisted: KNOWN_UNTRACKED.size }));
}

if (process.argv.includes('--self-test')) selfTest();
else scan();
