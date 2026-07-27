#!/usr/bin/env node
// Ledger preflight — makes a null-free REJECT hard to write, not merely discouraged.
//
// Fleet broadcast 2 (2026-07-26): ledger integrity DECAYS. frankensqlite audited once, then
// institutionalized the check with a mechanically enforced preflight, and sits at 1.7% void.
// Repos that audited once and stopped sit at 25-91%. This is frankenmermaid's enforcement.
//
// Two modes, both exit non-zero to block:
//
//   --lever "<text>" --surface "<text>"    BEFORE mutating source. Greps the negative-evidence
//                                          ledger for a prior dated REJECT covering this mechanism
//                                          and target surface. exit 2 = BLOCKED.
//
//   --lint [--base <git-ref>] [--staged]   BEFORE committing. Every REJECT or KEEP row ADDED or
//                                          MODIFIED relative to <git-ref>, across both split
//                                          ledgers, must satisfy its evidence contract. REJECT
//                                          needs same-invocation A/A or a counted mechanism; KEEP
//                                          needs the process-self-reported executing ELF SHA-256.
//                                          exit 1 = an inadmissible row.
//
// Evidence markers are deliberately explicit. Natural-language regexes confused retry predicates,
// source hashes, structural arguments, and unrelated "null" phases with actual evidence during the
// resurrection audit. New rows must use one of the markers documented in AGENTS.md.

import { execFileSync } from 'node:child_process';
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const NEGATIVE_LEDGER = {
  path: 'docs/NEGATIVE_EVIDENCE.md',
  heading: '### ',
};
const PERFORMANCE_LEDGER = {
  path: 'docs/PERF_LEDGER.md',
  heading: '## ',
};
const LEDGERS = [NEGATIVE_LEDGER, PERFORMANCE_LEDGER];

// --- mandatory evidence markers ---------------------------------------------------------------

const NULL_MARKER = '**A/A null control (same invocation):**';
const COUNTED_MARKER = '**Counted mechanism:**';
const ELF_MARKER = '**Executing ELF SHA-256 (self-reported by process):**';
const COUNTED_METRIC = /\b(instructions?|cycles?|syscalls?|allocations?|faults?)\b/i;
const MEASURED_VALUE =
  /(?:\d[\d,]*(?:\.\d+)?(?:%|x|ns|us|µs|ms|s)?|unchanged|identical|flat|no (?:measurable |material )?(?:work|change|difference))/i;
const NULL_VALUE = /\b(?:ratio|median|p50|CI|delta|samples?|baseline)\b[^.\n]*\d/i;
const ELF_VALUE = /\b[a-f0-9]{64}\b/;

const REJECT_TITLE =
  /\b(REJECT|REJECTED|NO-SHIP|NOSHIP|REVERT|REVERTED|NEGATIVE|ZERO-GAIN|~0-GAIN|WASH|ABANDON|DEAD|INVALID|VOID|SUB-BAR)\b/i;
const KEEP_TITLE = /^\s*(WIN|KEPT|KEEP|LANDED|VERIFIED|SHIPPED|✅|🟢)/i;
const REJECT_VERDICT =
  /(?:^|\n)[^\n]{0,24}\b(?:Verdict|Decision|Disposition)\b[^\n]{0,32}\b(?:REJECT|NO-SHIP|REVERT|WASH)\b/im;
const KEEP_VERDICT =
  /(?:^|\n)[^\n]{0,24}\b(?:Verdict|Decision|Disposition)\b[^\n]{0,32}\b(?:KEEP|KEPT|LANDED|SHIP)\b/im;

/** Split one ledger into its configured verdict-entry headings. */
function entries(text, ledger = NEGATIVE_LEDGER) {
  const lines = text.split('\n');
  const idx = lines
    .map((line, i) => (line.startsWith(ledger.heading) ? i : -1))
    .filter((i) => i >= 0);
  return idx.map((s, k) => {
    const e = k + 1 < idx.length ? idx[k + 1] : lines.length;
    return {
      ledger: ledger.path,
      line: s + 1,
      title: lines[s].slice(ledger.heading.length).trim(),
      body: lines.slice(s, e).join('\n'),
    };
  });
}

const isKeepRow = (e) => KEEP_TITLE.test(e.title) || KEEP_VERDICT.test(e.body);
const isRejectRow = (e) =>
  !isKeepRow(e) && (REJECT_TITLE.test(e.title) || REJECT_VERDICT.test(e.body));

function markerParagraph(body, marker) {
  const start = body.indexOf(marker);
  if (start < 0) return '';
  const rest = body.slice(start + marker.length);
  const end = rest.search(/\n\s*\n|\n-\s+\*\*[^*]+:\*\*/);
  return (end < 0 ? rest : rest.slice(0, end)).trim();
}

function rejectEvidence(body) {
  const nullEvidence = markerParagraph(body, NULL_MARKER);
  if (nullEvidence && NULL_VALUE.test(nullEvidence))
    return { ok: true, why: 'same-invocation A/A null control recorded' };

  const countedEvidence = markerParagraph(body, COUNTED_MARKER);
  if (
    countedEvidence &&
    COUNTED_METRIC.test(countedEvidence) &&
    MEASURED_VALUE.test(countedEvidence)
  )
    return { ok: true, why: 'counted mechanism recorded' };

  return { ok: false, why: null };
}

function keepEvidence(body) {
  const elfEvidence = markerParagraph(body, ELF_MARKER);
  return ELF_VALUE.test(elfEvidence);
}

function addedEntries(before, after, ledger = NEGATIVE_LEDGER) {
  const remaining = new Map();
  for (const e of entries(before, ledger)) {
    const key = e.body.trimEnd();
    remaining.set(key, (remaining.get(key) ?? 0) + 1);
  }
  const added = [];
  for (const e of entries(after, ledger)) {
    const key = e.body.trimEnd();
    const count = remaining.get(key) ?? 0;
    if (count > 0) remaining.set(key, count - 1);
    else added.push(e);
  }
  return added;
}

function retryPredicate(body) {
  const flat = body.replace(/\s+/g, ' ');
  const predicate = flat.match(
    /\b(?:if retried|retry (?:only )?(?:if|when|predicate|condition)?|do[- ]not[- ]retry|reopen only|unblock(?:ed)?(?: if| when)?)[^.!?]*(?:[.!?]|$)/i,
  );
  return predicate ? predicate[0].trim().slice(0, 700) : null;
}

const arg = (n, d = null) => {
  const i = process.argv.indexOf(`--${n}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : d;
};
const has = (n) => process.argv.includes(`--${n}`);

const missingLedgers = LEDGERS.filter((ledger) => !existsSync(join(REPO, ledger.path)));
if (missingLedgers.length > 0) {
  console.error(`[preflight] missing ${missingLedgers.map((ledger) => ledger.path).join(', ')}`);
  process.exit(3);
}

// ---------------------------------------------------------------- mode: --self-test
if (has('self-test')) {
  const hash = 'a'.repeat(64);
  const perfKeepWithoutElf = entries(
    '## KEEP: parsed from the performance ledger\n\n**Verdict:** KEEP.\n',
    PERFORMANCE_LEDGER,
  )[0];
  const perfKeepWithElf = entries(
    `## KEEP: exact marker\n\n${ELF_MARKER} \`${hash}\`\n\n**Verdict:** KEEP.\n`,
    PERFORMANCE_LEDGER,
  )[0];
  const cases = [
    ['structural prose is not counted evidence', !rejectEvidence('**Root cause:** no work.').ok],
    ['ceiling prose is not counted evidence', !rejectEvidence('Amdahl ceiling: 1%.').ok],
    [
      'retry-only A/A prose is not evidence',
      !rejectEvidence('Retry only when same-invocation A/A ratio is below 1.01.').ok,
    ],
    [
      'empty A/A marker is rejected',
      !rejectEvidence(`${NULL_MARKER} required before retry.`).ok,
    ],
    [
      'measured A/A marker is accepted',
      rejectEvidence(
        `${NULL_MARKER} baseline/null median ratio 1.0012x, CI [0.999, 1.003].`,
      ).ok,
    ],
    [
      'counted marker is accepted',
      rejectEvidence(`${COUNTED_MARKER} instructions 12,004 -> 12,004 (unchanged).`).ok,
    ],
    ['source SHA is not an ELF self-report', !keepEvidence(`source SHA-256 ${hash}`)],
    [
      'uppercase SHA is rejected by the lowercase contract',
      !keepEvidence(`${ELF_MARKER} \`${hash.toUpperCase()}\``),
    ],
    ['self-reported executing ELF is accepted', keepEvidence(`${ELF_MARKER} \`${hash}\``)],
    [
      'PERF_LEDGER KEEP headings are parsed as KEEP rows',
      perfKeepWithoutElf?.ledger === PERFORMANCE_LEDGER.path && isKeepRow(perfKeepWithoutElf),
    ],
    [
      'PERF_LEDGER KEEP without an ELF marker is rejected',
      !keepEvidence(perfKeepWithoutElf?.body ?? ''),
    ],
    [
      'PERF_LEDGER KEEP with an exact ELF marker is accepted',
      isKeepRow(perfKeepWithElf) && keepEvidence(perfKeepWithElf.body),
    ],
    [
      'modified PERF_LEDGER KEEP appears in the lint delta',
      addedEntries(
        '## KEEP: exact marker\n\nold body\n',
        `## KEEP: exact marker\n\n${ELF_MARKER} \`${hash}\`\n`,
        PERFORMANCE_LEDGER,
      ).length === 1,
    ],
  ];
  const failed = cases.filter(([, ok]) => !ok);
  for (const [name, ok] of cases) console.log(`[self-test] ${ok ? 'ok' : 'FAIL'}  ${name}`);
  process.exit(failed.length === 0 ? 0 : 1);
}

// ---------------------------------------------------------------- mode: --lever
if (has('lever')) {
  const lever = arg('lever', '');
  const surface = arg('surface', arg('frame'));
  if (!surface) {
    console.error('[preflight] --lever requires --surface "<target file/function/benchmark>"');
    process.exit(3);
  }
  const terms = [
    ...new Set(
      [surface, ...surface.split(/\s+/), ...lever.split(/\s+/).filter((w) => w.length >= 5)]
        .filter(Boolean)
        .map((t) => t.toLowerCase().replace(/[^a-z0-9_:<>-]/g, ''))
        .filter(Boolean),
    ),
  ];
  if (terms.length === 0) {
    console.error('[preflight] --lever and --surface need searchable descriptions');
    process.exit(3);
  }
  const rows = entries(
    readFileSync(join(REPO, NEGATIVE_LEDGER.path), 'utf8'),
    NEGATIVE_LEDGER,
  ).filter(isRejectRow);
  const ranked = rows
    .map((e) => {
      const hay = e.body.toLowerCase();
      const matched = terms.filter((t) => hay.includes(t));
      const surfaceHit = hay.includes(surface.toLowerCase());
      return { e, matched, score: matched.length + (surfaceHit ? 8 : 0), surfaceHit };
    })
    .sort((a, b) => b.score - a.score);
  const exactSurfaceHits = ranked.filter((h) => h.surfaceHit);
  const hits = (exactSurfaceHits.length > 0
    ? exactSurfaceHits
    : ranked.filter((h) => h.matched.length >= 2)
  ).slice(0, 8);

  if (hits.length === 0) {
    console.log(`[preflight] OK — no prior REJECT row matches (${rows.length} reject rows scanned).`);
    console.log('[preflight] Reminder: your own REJECT will need an A/A null or a counted mechanism.');
    process.exit(0);
  }
  console.error(`[preflight] BLOCKED — ${hits.length} prior REJECT row(s) cover this mechanism:\n`);
  for (const h of hits) {
    console.error(`  ${h.e.ledger}:${h.e.line}`);
    console.error(`    ${h.e.title}`);
    console.error(`    matched: ${h.matched.join(', ')}`);
    const retry = retryPredicate(h.e.body);
    if (retry) console.error(`    retry predicate: ${retry}`);
    else console.error('    retry predicate: none recorded');
    console.error('');
  }
  console.error('[preflight] Satisfy the retry predicate and say so in your row, or pick another lever.');
  process.exit(2);
}

// ---------------------------------------------------------------- mode: --lint
if (has('lint')) {
  const staged = has('staged');
  const base = arg('base', staged ? 'HEAD' : 'origin/main');
  const added = [];
  for (const ledger of LEDGERS) {
    let before = '';
    try {
      before = execFileSync('git', ['-C', REPO, 'show', `${base}:${ledger.path}`], {
        encoding: 'utf8',
        maxBuffer: 64 * 1024 * 1024,
      });
    } catch {
      console.error(`[preflight] cannot read ${ledger.path} at ${base}; linting the whole file`);
    }
    let current;
    if (staged) {
      try {
        current = execFileSync('git', ['-C', REPO, 'show', `:${ledger.path}`], {
          encoding: 'utf8',
          maxBuffer: 64 * 1024 * 1024,
        });
      } catch {
        console.error(`[preflight] cannot read staged ${ledger.path}`);
        process.exit(3);
      }
    } else {
      current = readFileSync(join(REPO, ledger.path), 'utf8');
    }
    added.push(
      ...addedEntries(before, current, ledger).filter((e) => isRejectRow(e) || isKeepRow(e)),
    );
  }

  if (added.length === 0) {
    console.log(
      `[preflight] OK — no REJECT or KEEP rows added across ${LEDGERS.map((ledger) => ledger.path).join(', ')} vs ${base}${staged ? ' in the index' : ''}.`,
    );
    process.exit(0);
  }
  const bad = [];
  for (const e of added) {
    if (isKeepRow(e)) {
      if (keepEvidence(e.body))
        console.log(
          `[preflight] ok    ${e.ledger}:L${e.line}  executing ELF SHA-256 self-report recorded\n              ${e.title.slice(0, 96)}`,
        );
      else bad.push({ e, kind: 'KEEP', why: `missing ${ELF_MARKER}` });
      continue;
    }
    const verdict = rejectEvidence(e.body);
    if (verdict.ok)
      console.log(
        `[preflight] ok    ${e.ledger}:L${e.line}  ${verdict.why}\n              ${e.title.slice(0, 96)}`,
      );
    else
      bad.push({
        e,
        kind: 'REJECT',
        why: `missing ${NULL_MARKER} or ${COUNTED_MARKER}`,
      });
  }
  if (bad.length === 0) {
    console.log(`\n[preflight] OK — all ${added.length} new ledger verdict row(s) satisfy the contract.`);
    process.exit(0);
  }
  console.error(`\n[preflight] BLOCKED — ${bad.length} new ledger verdict row(s) violate the contract.\n`);
  for (const { e, kind, why } of bad) {
    console.error(`  ${kind} at ${e.ledger}:${e.line}`);
    console.error(`    ${e.title}`);
    console.error(`    ${why}\n`);
  }
  console.error('  REJECT rows need measured evidence under at least one of:');
  console.error(`    ${NULL_MARKER}`);
  console.error(`    ${COUNTED_MARKER}`);
  console.error('  Structural prose and ceilings do not satisfy this gate.');
  console.error(`  KEEP rows need: ${ELF_MARKER} <64 lowercase hex characters>`);
  process.exit(1);
}

console.error(`usage:
  node scripts/ledger_preflight.mjs --lever "<description>" --surface "<file/function/bench>"
  node scripts/ledger_preflight.mjs --lint [--base <git-ref>] [--staged]
  node scripts/ledger_preflight.mjs --self-test`);
process.exit(3);
