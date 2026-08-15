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
//   --lint [--base <git-ref>] [--staged]   BEFORE committing. Every REJECT or performance-result
//                                          row ADDED or MODIFIED relative to <git-ref>, across both
//                                          split ledgers, must satisfy its evidence contract.
//                                          REJECT needs same-invocation A/A or a counted mechanism.
//                                          Every kept result needs the process-self-reported
//                                          executing ELF SHA-256 and an explicit result class.
//                                          An incumbent-win additionally needs a measured,
//                                          same-invocation mermaid-js arm and its A/A null.
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
const RESULT_CLASS_MARKER = '**Campaign result class:**';
const INCUMBENT_MARKER = '**Legacy incumbent arm (same invocation):**';
const MAINTENANCE_SELF_SPEEDUP = 'maintenance-self-speedup';
const INCUMBENT_WIN = 'incumbent-win';
// A result the previous two classes cannot express: the incumbent did not COMPLETE on the input, so
// there is no comparator time and therefore no ratio. Before this class existed the only ways to
// record such a result were to omit the classification entirely (leaving the strongest capability
// evidence this project produces unbanked) or to invent a ratio for an engine that produced
// nothing, which is proof-class inflation. This class demands MORE than a maintenance row -- the
// pinned incumbent artifact, the shared invocation, a named failure class per fixture, and our own
// verified output -- and it FORBIDS a measured_ratio, so it can never be used to launder a
// competitive number. A gate change that suddenly produces wins is a loosening; this one produces
// no ratios at all.
const INCUMBENT_DNF = 'incumbent-dnf';
// Where the arms actually ran. Adopted 2026-08-15 after a fleet-wide finding: frankenscipy measured
// the SAME cubic splu cell on two different rch workers and got 1.2693x on one and 0.0093x on the
// other -- a 13.6x swing -- with BOTH A/A nulls PASSING. The null only controls within-invocation
// noise; it is blind to between-worker differences in CPU model, cache, memory bandwidth and
// contention. So a passing null does not make a cross-worker comparison valid, and a row that does
// not name its worker cannot be compared to any other row. Required on EVERY kept row, including
// maintenance-self-speedup, because a self A/B is corrupted by split arms exactly as badly as a
// competitive one.
const HOST_MARKER = '**Measurement host (observed, both arms):**';
const HOST_THREADS = /\bthreads=\d+\b/i;
const HOST_GOVERNOR = /\bgovernor=[a-z0-9][a-z0-9._-]*\b/i;
const HOST_ISA = /\bisa=[a-z0-9][a-z0-9._+-]*\b/i;
const HOST_WORKER_ALL = /\bworker=([a-z0-9][a-z0-9._-]*)/gi;
// The local-measurement spelling of the same fact, adopted from frankenfs's form
// (`RCH_WORKER=<id>` or `same_host=<hostname>`). This repo's head-to-head driver runs BOTH arms on
// this box rather than on an rch worker, and a row that measured locally has no rch worker id to
// give. Without this spelling the honest local row would either be blocked or tempted to invent a
// `worker=` value, which is worse than no field at all. Either spelling answers "where", and the
// multi-value refusal below counts them together: `worker=hz2 same_host=csd` is two machines.
const HOST_SAME_ALL = /\bsame_host=([a-z0-9][a-z0-9._-]*)/gi;
// WHICH HARNESS produced the number. Adopted 2026-08-15 alongside `worker=`, after a second
// fleet-wide finding that is independent of the first: frankenlibc measured malloc/free on the SAME
// worker (hz2) through two separately-sanctioned harnesses and got 5.9459x and 12.385414x -- a ~2x
// spread -- with BOTH A/A nulls passing in tolerance. So a passing null does not certify that a
// harness measures what its row says it measures, and two rows from different harnesses are no more
// comparable than two rows from different workers. This repo has more than one: the head-to-head
// driver (`scripts/headtohead/run.mjs`), the `perf stat` instruction A/B used for maintenance rows,
// and the per-crate criterion benches.
//
// If two harnesses here ever disagree on the same primitive, the DISAGREEMENT is the finding and
// both numbers get banked with their harness named. Picking the friendlier one is the failure mode
// this field exists to make impossible to do silently.
const HOST_HARNESS = /\bharness=[a-z0-9][a-z0-9._/-]*\b/i;
// Retro-flag for rows banked BEFORE the provenance gate existed. It does not excuse a row from
// naming where it ran -- it removes the row from the comparable set, which is a strictly worse
// outcome for the row and therefore not a way around the gate. See `measurementHostEvidence`.
const SCOPED_MARKER = '**Measurement provenance:**';
const SCOPED_VALUE = /\bWORKER-SCOPED\b/;
const SCOPED_BACKLOG = /\bpre-gate-backlog\b/i;
const SCOPED_AUDIT = /\bbd-[a-z0-9]+\b/i;
const COUNTED_METRIC = /\b(instructions?|cycles?|syscalls?|allocations?|faults?)\b/i;
const MEASURED_VALUE =
  /(?:\d[\d,]*(?:\.\d+)?(?:%|x|ns|us|µs|ms|s)?|unchanged|identical|flat|no (?:measurable |material )?(?:work|change|difference))/i;
const NULL_VALUE = /\b(?:ratio|median|p50|CI|delta|samples?|baseline)\b[^.\n]*\d/i;
const ELF_VALUE = /\b[a-f0-9]{64}\b/;
const INCUMBENT_NAME = /\bname=mermaid-js\b/;
const INCUMBENT_VERSION = /\bversion=[a-z0-9][a-z0-9.+_-]*\b/i;
const INCUMBENT_ARTIFACT = /\bartifact_sha256=[a-f0-9]{64}\b/;
const INCUMBENT_INVOCATION = /\binvocation_id=[a-z0-9][a-z0-9._:-]*\b/i;
const INCUMBENT_RATIO = /\bmeasured_ratio=\d+(?:\.\d+)?x\b/i;

const REJECT_TITLE =
  /\b(REJECT|REJECTED|NO-SHIP|NOSHIP|REVERT|REVERTED|NEGATIVE|ZERO-GAIN|~0-GAIN|WASH|ABANDON|DEAD|INVALID|VOID|SUB-BAR)\b/i;
const KEEP_TITLE = /^\s*(WIN|KEPT|KEEP|LANDED|VERIFIED|SHIPPED|✅|🟢)/i;
const RESULT_TITLE =
  /^\s*(?:MAINTENANCE\s+SELF[- ]SPEEDUP|SELF[- ]SPEEDUP|CAMPAIGN\s+WIN|INCUMBENT\s+WIN)\b/i;
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

const isKeepRow = (e) =>
  KEEP_TITLE.test(e.title) ||
  RESULT_TITLE.test(e.title) ||
  KEEP_VERDICT.test(e.body) ||
  resultClass(e.body) !== null;
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

/**
 * Evidence that both arms were observed on ONE named machine.
 *
 * Two distinct `worker=` values in the same marker is a refusal, not a warning: that is a row
 * declaring in its own words that its arms landed on different hosts, which is the exact shape the
 * fleet measured a 13.6x swing across. The fields are the ones that differ between workers and
 * change a ratio without changing the code: which box, how many threads were observed on it, what
 * the scaling governor was doing, which ISA level the executable was allowed to use, and — since
 * 2026-08-15 — which HARNESS produced the number.
 *
 * ONE ALTERNATIVE IS ACCEPTED, for the backlog only. A row banked before this gate existed may
 * instead declare itself WORKER-SCOPED. That is not an exemption: it takes the row OUT of the
 * comparable set, and `incumbent-win` is refused outright, so the flag can never carry a
 * competitive claim. A new row gains nothing by reaching for it — it would be trading a comparable
 * result for a non-comparable one — which is what makes it safe to accept at all. The alternative
 * exists because the audit that found these rows (bd-kcy4) established that their measurement host
 * is UNKNOWABLE: the artifacts of that era carry no host, worker, thread, affinity, governor or ISA
 * field, so demanding those four values would leave the truth unrecordable and the rows silently
 * trusted, which is worse than saying plainly that they are not comparable.
 */
function measurementHostEvidence(body, classification = null) {
  const scoped = markerParagraph(body, SCOPED_MARKER);
  if (SCOPED_VALUE.test(scoped)) {
    const missing = [];
    if (!SCOPED_BACKLOG.test(scoped)) {
      missing.push('pre-gate-backlog (this form is for rows banked before the gate, nothing else)');
    }
    if (!SCOPED_AUDIT.test(scoped)) missing.push('the bead id of the audit that established it');
    if (classification === INCUMBENT_WIN) {
      missing.push(
        'a competitive claim can never be worker-scoped; an incumbent-win row must name its host',
      );
    }
    return { ok: missing.length === 0, missing, scoped: true };
  }

  const evidence = markerParagraph(body, HOST_MARKER);
  const missing = [];
  const machines = new Set([
    ...[...evidence.matchAll(HOST_WORKER_ALL)].map((match) => match[1].toLowerCase()),
    ...[...evidence.matchAll(HOST_SAME_ALL)].map((match) => match[1].toLowerCase()),
  ]);
  if (machines.size === 0) {
    missing.push('worker=<rch worker id> or same_host=<hostname that ran both arms>');
  }
  if (!HOST_THREADS.test(evidence)) missing.push('threads=<observed thread count>');
  if (!HOST_GOVERNOR.test(evidence)) missing.push('governor=<scaling governor>');
  if (!HOST_ISA.test(evidence)) missing.push('isa=<ISA level the build targeted>');
  if (!HOST_HARNESS.test(evidence)) missing.push('harness=<which harness produced the number>');

  if (machines.size > 1) {
    missing.push(
      `both arms must be measured on ONE machine in ONE invocation; this row names ${machines.size}: ${[...machines].join(', ')}`,
    );
  }
  return { ok: missing.length === 0, missing, scoped: false };
}

function resultClass(body) {
  const value = markerParagraph(body, RESULT_CLASS_MARKER)
    .replaceAll('`', '')
    .split(/\s/)[0];
  return value === MAINTENANCE_SELF_SPEEDUP || value === INCUMBENT_WIN || value === INCUMBENT_DNF
    ? value
    : null;
}

function incumbentEvidence(body) {
  const evidence = markerParagraph(body, INCUMBENT_MARKER);
  const nullEvidence = markerParagraph(body, NULL_MARKER);
  const missing = [];
  if (!INCUMBENT_NAME.test(evidence)) missing.push('name=mermaid-js');
  if (!INCUMBENT_VERSION.test(evidence)) missing.push('version=<pin>');
  if (!INCUMBENT_ARTIFACT.test(evidence)) missing.push('artifact_sha256=<64 lowercase hex>');
  if (!INCUMBENT_INVOCATION.test(evidence)) missing.push('invocation_id=<shared invocation>');
  if (!INCUMBENT_RATIO.test(evidence)) missing.push('measured_ratio=<number>x');
  if (!NULL_VALUE.test(nullEvidence)) missing.push(NULL_MARKER);
  return { ok: missing.length === 0, missing };
}

const INCUMBENT_OUTCOME = /\boutcome=did_not_complete\b/i;
const INCUMBENT_FAILURE_CLASS = /\bfailure_class=[a-z0-9][a-z0-9._-]*\b/i;

/**
 * Evidence for an `incumbent-dnf` row.
 *
 * Same provenance spine as an incumbent-win -- the pinned artifact and the shared invocation, so
 * the claim names exactly which build of which comparator failed and in which run -- minus the
 * ratio, which does not exist, plus the two things a completion claim actually needs: an explicit
 * `outcome=did_not_complete` and a named `failure_class` so "it broke" is queryable rather than
 * prose. A `measured_ratio` is REFUSED here: an engine that produced no output cannot bound one.
 */
function incumbentDnfEvidence(body) {
  const evidence = markerParagraph(body, INCUMBENT_MARKER);
  const missing = [];
  if (!INCUMBENT_NAME.test(evidence)) missing.push('name=mermaid-js');
  if (!INCUMBENT_VERSION.test(evidence)) missing.push('version=<pin>');
  if (!INCUMBENT_ARTIFACT.test(evidence)) missing.push('artifact_sha256=<64 lowercase hex>');
  if (!INCUMBENT_INVOCATION.test(evidence)) missing.push('invocation_id=<shared invocation>');
  if (!INCUMBENT_OUTCOME.test(evidence)) missing.push('outcome=did_not_complete');
  if (!INCUMBENT_FAILURE_CLASS.test(evidence)) missing.push('failure_class=<observed class>');
  if (INCUMBENT_RATIO.test(evidence)) {
    missing.push('a did-not-complete row must NOT carry measured_ratio');
  }
  return { ok: missing.length === 0, missing };
}

function resultEvidence(body) {
  if (!keepEvidence(body)) return { ok: false, why: `missing ${ELF_MARKER}` };

  const classification = resultClass(body);

  // Host evidence is checked against the classification, because the one alternative form is
  // refused for a competitive claim: an incumbent-win row must name where it ran, always.
  const host = measurementHostEvidence(body, classification);
  if (!host.ok) {
    const marker = host.scoped ? SCOPED_MARKER : HOST_MARKER;
    return { ok: false, why: `incomplete ${marker} ${host.missing.join(', ')}` };
  }

  if (!classification) {
    return {
      ok: false,
      why: `missing ${RESULT_CLASS_MARKER} ${MAINTENANCE_SELF_SPEEDUP}|${INCUMBENT_WIN}`,
    };
  }
  if (classification === MAINTENANCE_SELF_SPEEDUP) {
    return {
      ok: true,
      why: host.scoped
        ? 'maintenance self-speedup, WORKER-SCOPED (not comparable to any other row)'
        : 'maintenance self-speedup (not campaign output)',
    };
  }

  if (classification === INCUMBENT_DNF) {
    const dnf = incumbentDnfEvidence(body);
    if (!dnf.ok) {
      return { ok: false, why: `incomplete ${INCUMBENT_MARKER} ${dnf.missing.join(', ')}` };
    }
    return { ok: true, why: 'same-invocation incumbent did-not-complete' };
  }

  const incumbent = incumbentEvidence(body);
  if (!incumbent.ok) {
    return {
      ok: false,
      why: `incomplete ${INCUMBENT_MARKER} ${incumbent.missing.join(', ')}`,
    };
  }
  return { ok: true, why: 'same-invocation actual-incumbent win' };
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
  const maintenanceWithoutClass = entries(
    `## MAINTENANCE SELF-SPEEDUP: missing class marker\n\n${ELF_MARKER} \`${hash}\`\n`,
    PERFORMANCE_LEDGER,
  )[0];
  const host = `${HOST_MARKER} worker=hz2 threads=64 governor=performance isa=x86-64-v2 harness=headtohead/run.mjs`;
  const selfSpeedup = `${ELF_MARKER} \`${hash}\`

${host}

${RESULT_CLASS_MARKER} ${MAINTENANCE_SELF_SPEEDUP}`;
  const incumbentDnf = `${ELF_MARKER} \`${hash}\`

${host}

${RESULT_CLASS_MARKER} ${INCUMBENT_DNF}

${INCUMBENT_MARKER} name=mermaid-js version=11.15.0 artifact_sha256=${hash} invocation_id=equiv-1 outcome=did_not_complete failure_class=range_error`;
  const incumbentWin = `${selfSpeedup.replace(MAINTENANCE_SELF_SPEEDUP, INCUMBENT_WIN)}

${NULL_MARKER} baseline/null median ratio 1.0012x, CI [0.999, 1.003].

${INCUMBENT_MARKER} name=mermaid-js version=11.15.0 artifact_sha256=${hash} invocation_id=run-42 measured_ratio=871.0x`;
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
      'a result without an explicit class is rejected',
      !resultEvidence(`${ELF_MARKER} \`${hash}\``).ok,
    ],
    [
      'a maintenance title cannot evade the result-class gate',
      isKeepRow(maintenanceWithoutClass) && !resultEvidence(maintenanceWithoutClass.body).ok,
    ],
    [
      'a self-speedup is accepted only as maintenance',
      resultEvidence(selfSpeedup).ok &&
        resultEvidence(selfSpeedup).why === 'maintenance self-speedup (not campaign output)',
    ],
    [
      'an incumbent-win without the actual incumbent arm is rejected',
      !resultEvidence(
        `${selfSpeedup.replace(MAINTENANCE_SELF_SPEEDUP, INCUMBENT_WIN)}

${NULL_MARKER} baseline/null median ratio 1.0x, CI [0.99, 1.01].`,
      ).ok,
    ],
    [
      'a self baseline cannot masquerade as the legacy incumbent',
      !resultEvidence(
        incumbentWin.replace('name=mermaid-js', 'name=self-baseline'),
      ).ok,
    ],
    [
      'an incumbent-win without an A/A null is rejected',
      !resultEvidence(
        incumbentWin.replace(
          `${NULL_MARKER} baseline/null median ratio 1.0012x, CI [0.999, 1.003].\n\n`,
          '',
        ),
      ).ok,
    ],
    [
      'a pinned mermaid-js arm in the same invocation is accepted',
      resultEvidence(incumbentWin).ok &&
        resultEvidence(incumbentWin).why === 'same-invocation actual-incumbent win',
    ],
    [
      'an incumbent-dnf with outcome and failure class is accepted',
      resultEvidence(incumbentDnf).ok &&
        resultEvidence(incumbentDnf).why === 'same-invocation incumbent did-not-complete',
    ],
    [
      'an incumbent-dnf carrying a ratio is refused -- nothing completed to bound one',
      !resultEvidence(
        `${incumbentDnf} measured_ratio=401800000x`,
      ).ok,
    ],
    [
      'an incumbent-dnf without a named failure class is refused',
      !resultEvidence(incumbentDnf.replace(' failure_class=range_error', '')).ok,
    ],
    [
      'an incumbent-dnf without the pinned comparator artifact is refused',
      !resultEvidence(incumbentDnf.replace(`artifact_sha256=${hash}`, 'artifact_sha256=unknown')).ok,
    ],
    [
      'an incumbent-dnf still needs the process-self-reported ELF',
      !resultEvidence(incumbentDnf.replace(`${ELF_MARKER} \`${hash}\`\n\n`, '')).ok,
    ],
    [
      'a kept row without a measurement host is rejected',
      !resultEvidence(selfSpeedup.replace(`${host}\n\n`, '')).ok,
    ],
    [
      'a measurement host missing the observed thread count is rejected',
      !resultEvidence(selfSpeedup.replace(' threads=64', '')).ok,
    ],
    [
      'a measurement host missing the governor is rejected',
      !resultEvidence(selfSpeedup.replace(' governor=performance', '')).ok,
    ],
    [
      'a measurement host missing the ISA level is rejected',
      !resultEvidence(selfSpeedup.replace(' isa=x86-64-v2', '')).ok,
    ],
    [
      'arms named on two different workers are refused even with every other marker present',
      !resultEvidence(
        incumbentWin.replace(
          'worker=hz2 threads=64',
          'worker=hz2 worker=vmi1293453 threads=64',
        ),
      ).ok,
    ],
    [
      'a passing A/A null does not excuse a missing measurement host',
      !resultEvidence(incumbentWin.replace(`${host}\n\n`, '')).ok,
    ],
    // harness= (2026-08-15). frankenlibc got 5.9459x and 12.385414x for the same primitive on the
    // SAME worker through two sanctioned harnesses, both A/A nulls passing. Worker identity alone
    // does not make two rows comparable.
    [
      'a measurement host missing the harness is rejected',
      !resultEvidence(selfSpeedup.replace(' harness=headtohead/run.mjs', '')).ok,
    ],
    [
      'a row naming worker AND harness is admitted',
      resultEvidence(selfSpeedup).ok,
    ],
    // same_host= is the local-measurement spelling of "where", from frankenfs's form. Our
    // head-to-head runs both arms on this box, so it has no rch worker id to give; without this
    // spelling the honest local row is either blocked or tempted to invent a worker= value.
    [
      'same_host= names the machine just as well as worker= for a locally measured row',
      resultEvidence(selfSpeedup.replace('worker=hz2', 'same_host=csd')).ok,
    ],
    [
      'two different same_host values are refused exactly like two workers',
      !resultEvidence(selfSpeedup.replace('worker=hz2', 'same_host=csd same_host=csd2')).ok,
    ],
    [
      'a worker and a different same_host in one row is still two machines',
      !resultEvidence(selfSpeedup.replace('worker=hz2', 'worker=hz2 same_host=csd')).ok,
    ],
    [
      'the same machine named twice in both spellings is not two machines',
      resultEvidence(selfSpeedup.replace('worker=hz2', 'worker=hz2 same_host=hz2')).ok,
    ],
    // The WORKER-SCOPED backlog form. It must DEMOTE, never excuse.
    [
      'a pre-gate backlog row may declare itself worker-scoped instead of naming a host',
      resultEvidence(
        selfSpeedup.replace(
          host,
          `${SCOPED_MARKER} WORKER-SCOPED (pre-gate-backlog, bd-kcy4)`,
        ),
      ).ok,
    ],
    [
      'a worker-scoped row that does not say which audit established it is refused',
      !resultEvidence(
        selfSpeedup.replace(host, `${SCOPED_MARKER} WORKER-SCOPED (pre-gate-backlog)`),
      ).ok,
    ],
    [
      'a worker-scoped row that does not declare itself backlog is refused',
      !resultEvidence(selfSpeedup.replace(host, `${SCOPED_MARKER} WORKER-SCOPED (bd-kcy4)`)).ok,
    ],
    // THE TEETH. Without this the flag would be a way for a competitive claim to skip naming its
    // host, which is exactly the hole the gate exists to close.
    [
      'an incumbent-win can NEVER be worker-scoped, however well formed the flag is',
      !resultEvidence(
        incumbentWin.replace(
          host,
          `${SCOPED_MARKER} WORKER-SCOPED (pre-gate-backlog, bd-kcy4)`,
        ),
      ).ok,
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
      const evidence = resultEvidence(e.body);
      if (evidence.ok)
        console.log(
          `[preflight] ok    ${e.ledger}:L${e.line}  ${evidence.why}; executing ELF SHA-256 self-report recorded\n              ${e.title.slice(0, 96)}`,
        );
      else bad.push({ e, kind: 'RESULT', why: evidence.why });
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
  console.error(`  Every kept result needs: ${ELF_MARKER} <64 lowercase hex characters>`);
  console.error(
    `  Every kept result needs: ${RESULT_CLASS_MARKER} ${MAINTENANCE_SELF_SPEEDUP}|${INCUMBENT_WIN}`,
  );
  console.error(`  ${MAINTENANCE_SELF_SPEEDUP} is maintenance, never campaign output.`);
  console.error(`  ${INCUMBENT_WIN} also needs:`);
  console.error(`    ${NULL_MARKER}`);
  console.error(
    `    ${INCUMBENT_MARKER} name=mermaid-js version=<pin> artifact_sha256=<sha> invocation_id=<id> measured_ratio=<number>x`,
  );
  process.exit(1);
}

console.error(`usage:
  node scripts/ledger_preflight.mjs --lever "<description>" --surface "<file/function/bench>"
  node scripts/ledger_preflight.mjs --lint [--base <git-ref>] [--staged]
  node scripts/ledger_preflight.mjs --self-test`);
process.exit(3);
