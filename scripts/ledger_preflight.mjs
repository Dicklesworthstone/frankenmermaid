#!/usr/bin/env node
// Ledger preflight — makes a null-free REJECT hard to write, not merely discouraged.
//
// Fleet broadcast 2 (2026-07-26): ledger integrity DECAYS. frankensqlite audited once, then
// institutionalized the check with a mechanically enforced preflight, and sits at 1.7% void.
// Repos that audited once and stopped sit at 25-91%. This is frankenmermaid's enforcement.
//
// Two modes, both exit non-zero to block:
//
//   --lever "<text>" [--frame <symbol>]   BEFORE mutating source. Greps the negative-evidence
//                                          ledger for a prior dated REJECT covering this mechanism.
//                                          exit 2 = BLOCKED (a matching REJECT exists).
//
//   --lint [--base <git-ref>]              BEFORE committing. Every REJECT row ADDED relative to
//                                          <git-ref> must carry an A/A null control or a counted
//                                          mechanism. exit 1 = a row would be unfalsifiable.
//
// The --lint predicates are deliberately the SAME ones docs/LEDGER_RESURRECTION.md section 7 audits
// with, so the gate and the audit agree by construction: a row this gate admits is a row that audit
// classifies VALID-*, and a row it rejects is one that audit would classify VOID-NONULL.

import { execFileSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const LEDGER = join(REPO, 'docs', 'NEGATIVE_EVIDENCE.md');

// --- the shared predicates (keep in lockstep with docs/LEDGER_RESURRECTION.md section 7) ---------

/** An A/A null control was actually recorded. */
const NULL_CTRL =
  /(A\/B\/null|A\/A\b|null[- ]control|null arm|noop floor|null floor|null median|null delta|paired\(base, ?base\)|null CV|null-adjusted|noop null|null cv)/i;
/** A COUNTED mechanism: instructions / cycles / syscalls / allocations / faults. */
const MECHANISM =
  /(instructions?\b|\binstr\b|perf stat|cycles\b|syscall|allocations? (count|unchanged|identical)|page[- ]fault|branch[- ]miss|retired|counted)/i;
const MECH_UNCHANGED =
  /(unchanged|identical|flat|no (measurable |material )?(change|difference)|same instruction|instruction[- ]identical|0\.0[0-9]?% instr|no work (was )?removed)/i;
/** A structural refutation: the row proves no work was removed by argument (see section 7.4). */
const STRUCTURAL =
  /\*\*(mechanism|why (it'?s )?~?0|why ~0|root cause|why the profile lied|why it looked|why not a free win|why it'?s (below|fragile)|lesson|mechanism trade-off)/i;
/** A stated ceiling bounds the claim without needing a null. */
const CEILING = /(ceiling|Amdahl|upper bound|theoretical (max|limit)|at (its |the )?floor)/i;

const REJECT_TITLE =
  /\b(REJECT|REJECTED|NO-SHIP|NOSHIP|REVERT|REVERTED|NEGATIVE|ZERO-GAIN|~0-GAIN|WASH|ABANDON|DEAD|INVALID|VOID|SUB-BAR)\b/i;
const KEEP_TITLE = /^\s*(WIN|KEPT|KEEP|LANDED|VERIFIED|SHIPPED|✅|🟢)/i;

/** Split a ledger into `### ` entries. */
function entries(text) {
  const lines = text.split('\n');
  const idx = lines.map((l, i) => (l.startsWith('### ') ? i : -1)).filter((i) => i >= 0);
  return idx.map((s, k) => {
    const e = k + 1 < idx.length ? idx[k + 1] : lines.length;
    return { line: s + 1, title: lines[s].slice(4).trim(), body: lines.slice(s, e).join('\n') };
  });
}

const isRejectRow = (e) => !KEEP_TITLE.test(e.title) && REJECT_TITLE.test(e.title);

/** Would section 7 classify this row VALID-*? */
function isFalsifiable(body) {
  if (NULL_CTRL.test(body)) return { ok: true, why: 'A/A null control recorded' };
  if (MECHANISM.test(body) && MECH_UNCHANGED.test(body))
    return { ok: true, why: 'counted mechanism recorded (work shown unchanged)' };
  if (STRUCTURAL.test(body)) return { ok: true, why: 'structural refutation paragraph' };
  if (CEILING.test(body)) return { ok: true, why: 'stated ceiling bounds the claim' };
  return { ok: false, why: null };
}

const arg = (n, d = null) => {
  const i = process.argv.indexOf(`--${n}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : d;
};
const has = (n) => process.argv.includes(`--${n}`);

if (!existsSync(LEDGER)) {
  console.error(`[preflight] missing ${LEDGER}`);
  process.exit(3);
}

// ---------------------------------------------------------------- mode: --lever
if (has('lever')) {
  const lever = arg('lever', '');
  const frame = arg('frame');
  const terms = [frame, ...lever.split(/\s+/).filter((w) => w.length >= 5)]
    .filter(Boolean)
    .map((t) => t.toLowerCase().replace(/[^a-z0-9_:<>-]/g, ''))
    .filter(Boolean);
  if (terms.length === 0) {
    console.error('[preflight] --lever needs a description, and --frame <symbol> is strongly advised');
    process.exit(3);
  }
  const rows = entries(readFileSync(LEDGER, 'utf8')).filter(isRejectRow);
  const hits = rows
    .map((e) => {
      const hay = e.body.toLowerCase();
      const matched = terms.filter((t) => hay.includes(t));
      return { e, matched, score: matched.length + (frame && hay.includes(frame.toLowerCase()) ? 5 : 0) };
    })
    .filter((h) => (frame ? h.score >= 5 : h.matched.length >= 2))
    .sort((a, b) => b.score - a.score)
    .slice(0, 8);

  if (hits.length === 0) {
    console.log(`[preflight] OK — no prior REJECT row matches (${rows.length} reject rows scanned).`);
    console.log('[preflight] Reminder: your own REJECT will need an A/A null or a counted mechanism.');
    process.exit(0);
  }
  console.error(`[preflight] BLOCKED — ${hits.length} prior REJECT row(s) cover this mechanism:\n`);
  for (const h of hits) {
    console.error(`  docs/NEGATIVE_EVIDENCE.md:${h.e.line}`);
    console.error(`    ${h.e.title}`);
    console.error(`    matched: ${h.matched.join(', ')}`);
    const retry = h.e.body.match(/[Rr]etry (only )?(if|predicate|condition)[^\n]{0,220}/);
    if (retry) console.error(`    retry predicate: ${retry[0].trim()}`);
    console.error('');
  }
  console.error('[preflight] Satisfy the retry predicate and say so in your row, or pick another lever.');
  process.exit(2);
}

// ---------------------------------------------------------------- mode: --lint
if (has('lint')) {
  const base = arg('base', 'origin/main');
  let before = '';
  try {
    before = execFileSync('git', ['-C', REPO, 'show', `${base}:docs/NEGATIVE_EVIDENCE.md`], {
      encoding: 'utf8', maxBuffer: 64 * 1024 * 1024,
    });
  } catch {
    console.error(`[preflight] cannot read docs/NEGATIVE_EVIDENCE.md at ${base}; linting the whole file`);
  }
  const oldTitles = new Set(entries(before).map((e) => e.title));
  const added = entries(readFileSync(LEDGER, 'utf8'))
    .filter(isRejectRow)
    .filter((e) => !oldTitles.has(e.title));

  if (added.length === 0) {
    console.log(`[preflight] OK — no REJECT rows added vs ${base}.`);
    process.exit(0);
  }
  const bad = [];
  for (const e of added) {
    const v = isFalsifiable(e.body);
    if (v.ok) console.log(`[preflight] ok    L${e.line}  ${v.why}\n              ${e.title.slice(0, 96)}`);
    else bad.push(e);
  }
  if (bad.length === 0) {
    console.log(`\n[preflight] OK — all ${added.length} new REJECT row(s) are falsifiable.`);
    process.exit(0);
  }
  console.error(`\n[preflight] BLOCKED — ${bad.length} new REJECT row(s) record neither an A/A null,`);
  console.error('            a counted mechanism, a structural refutation, nor a ceiling.');
  console.error('            As written they cannot distinguish the lever from the harness — this is');
  console.error('            the VOID-NONULL class that is 167 of this ledger\'s 250 reject rows.\n');
  for (const e of bad) {
    console.error(`  docs/NEGATIVE_EVIDENCE.md:${e.line}`);
    console.error(`    ${e.title}\n`);
  }
  console.error('  Add ONE of:');
  console.error('    - an A/A null control from the same invocation (campaign section 2.2), or');
  console.error('    - a counted mechanism: instructions/cycles/syscalls/allocations shown unchanged');
  console.error('      (a null cannot change the fact that no work was removed), or');
  console.error('    - a **Mechanism** / **Why ~0** paragraph proving no work was removed, or');
  console.error('    - a computed ceiling that bounds the claim.');
  process.exit(1);
}

console.error(`usage:
  node scripts/ledger_preflight.mjs --lever "<description>" [--frame <symbol>]
  node scripts/ledger_preflight.mjs --lint [--base <git-ref>]`);
process.exit(3);
