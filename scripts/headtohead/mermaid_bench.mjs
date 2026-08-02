// mermaid-js side of the head-to-head. Zero npm dependencies.
//
// Why no puppeteer / no `npm install mermaid`: AGENTS.md forbids ad-hoc package installs, and a
// node_modules tree is not a provenance pin. Instead we fetch the *exact* pinned `mermaid.min.js`
// bundle recorded in pins.json, verify its SHA-256, and drive a system Chromium over the DevTools
// Protocol using Node's built-in `WebSocket` and `fetch`. The bundle is the same artifact a browser
// user would load from the CDN.
//
// `mmdc` (@mermaid-js/mermaid-cli) is deliberately not used: in 11.15.0 its bundled dist/index.html
// is an 81-byte stub and the CLI cannot render at all.
//
// Emits one JSON object per corpus item on stdout. The default mode times `mermaid.render()`; the
// explicit `--mode parse` boundary times `mermaid.parse()` for equal-work parser comparisons. A
// rejected parse or a render that throws/produces mermaid's error SVG is never a silent win.
//
// DID-NOT-FINISH. The XL corpus items reach sizes where mermaid may not complete at all. That is a
// result, not a harness failure, so items carrying `dnf_allowed` report `status: "dnf"` with the
// wall budget attached instead of failing the run. A DNF yields a *lower bound* on the speedup and
// is never mixed into the ratio aggregate: we say "mermaid did not finish inside B seconds", which
// is a claim about mermaid, not a number we made up for it.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  accessSync,
  constants,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { homedir } from 'node:os';
import { dirname, isAbsolute, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Script } from 'node:vm';
import { CORPUS, REVISION_SEP, generate, sha256 } from './corpus.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const PINS = JSON.parse(readFileSync(join(HERE, 'pins.json'), 'utf8'));
const MIN_NULL_ROUNDS = 9;
const ISOLATED_NULL_ROUNDS = 10;
const ISOLATED_SAMPLE_MIN_DOCUMENTS = 100;
const BOOTSTRAP_RESAMPLES = 2_000;
// `bd-ap4v` retry predicate: the predeclared parse floor rises 50 -> 250 ms for BOTH engines.
// run.mjs re-checks these exact values on the mermaid-js record, so the two files must move
// together; the calibration target keeps the runner's floor + ceil(floor/2) rule.
const PARSE_MIN_SAMPLE_MS = 250;
const PARSE_CALIBRATION_TARGET_MS =
  PARSE_MIN_SAMPLE_MS + Math.ceil(PARSE_MIN_SAMPLE_MS / 2);

function effectRepsForMode(reps, parseOnly) {
  return parseOnly ? Math.max(MIN_NULL_ROUNDS, reps) : reps;
}

// Bundle cache. Read by node, never by the browser, so a hidden dir is fine here.
const CACHE = join(homedir(), '.cache', 'fm-headtohead');

// Chromium's profile is another matter: snap's `home` interface denies access to *hidden*
// directories under $HOME (`~/.cache/...` => "Failed to create SingletonLock: Permission denied"),
// so the profile must live in the snap's own writable area. Each run gets a fresh mkdtemp profile;
// nothing is ever deleted (AGENTS.md rule 1), the dirs are small and live outside the repo.
const SNAP_COMMON = join(homedir(), 'snap', 'chromium', 'common');
const PROFILE_ROOT = existsSync(SNAP_COMMON) ? SNAP_COMMON : homedir();

function arg(name, fallback = null) {
  const i = process.argv.indexOf(`--${name}`);
  return i >= 0 && i + 1 < process.argv.length ? process.argv[i + 1] : fallback;
}
const has = (name) => process.argv.includes(`--${name}`);
const log = (...a) => console.error('[mermaid]', ...a);

function isExecutable(path) {
  try {
    accessSync(path, constants.X_OK);
    return true;
  } catch {
    return false;
  }
}

function resolveChromiumBinary(override, pinned, executable = isExecutable) {
  const binary = override || pinned;
  if (
    typeof binary === 'string' &&
    binary.length > 0 &&
    isAbsolute(binary) &&
    executable(binary)
  ) {
    return binary;
  }

  const source = override ? 'FM_CHROMIUM_BIN' : 'pins.json chromium.binary';
  throw new Error(
    `${source} is not an executable absolute path: ${binary || '<missing>'}; ` +
    'set FM_CHROMIUM_BIN to the absolute path of Chrome or Chromium',
  );
}

// ---------------------------------------------------------------- pinned bundle

async function bundle() {
  const { version, url, sha256: want } = PINS.mermaid;
  mkdirSync(CACHE, { recursive: true });
  const cached = join(CACHE, `mermaid-${version}.min.js`);
  let text;
  if (existsSync(cached)) {
    text = readFileSync(cached, 'utf8');
  } else {
    log(`fetching pinned bundle ${url}`);
    const res = await fetch(url, { redirect: 'follow' });
    if (!res.ok) throw new Error(`bundle fetch failed: HTTP ${res.status} ${url}`);
    text = await res.text();
    writeFileSync(cached, text);
  }
  const got = createHash('sha256').update(text, 'utf8').digest('hex');
  if (has('pin')) {
    console.error(`mermaid ${version} sha256 = ${got}`);
    process.exit(0);
  }
  if (want && got !== want) {
    throw new Error(
      `pinned bundle SHA-256 mismatch for mermaid ${version}\n  want ${want}\n  got  ${got}\n` +
      `Refusing to benchmark against an unpinned bundle. Inspect or move ${cached}, then re-run.`,
    );
  }
  return { text, version, url, sha256: got };
}

// ---------------------------------------------------------------- minimal CDP client

class Cdp {
  #ws; #next = 1; #pending = new Map();

  #failPending(error) {
    for (const pending of this.#pending.values()) pending.reject(error);
    this.#pending.clear();
  }

  static async attach(wsUrl) {
    const ws = new WebSocket(wsUrl);
    await new Promise((res, rej) => {
      ws.addEventListener('open', res, { once: true });
      ws.addEventListener('error', () => rej(new Error(`cdp connect failed: ${wsUrl}`)), { once: true });
    });
    const c = new Cdp();
    c.#ws = ws;
    ws.addEventListener('message', (ev) => {
      const msg = JSON.parse(ev.data);
      if (!('id' in msg)) {
        if (msg.method === 'Inspector.targetCrashed' || msg.method === 'Target.targetCrashed') {
          c.#failPending(new Error(`chromium target crashed during benchmark (${msg.method})`));
        }
        return;
      }
      const p = c.#pending.get(msg.id);
      if (!p) return;
      c.#pending.delete(msg.id);
      if (msg.error) p.reject(new Error(`${msg.error.message} (cdp ${msg.error.code})`));
      else p.resolve(msg.result);
    });
    ws.addEventListener('close', () => {
      c.#failPending(new Error('chromium devtools connection closed during benchmark'));
    });
    ws.addEventListener('error', () => {
      c.#failPending(new Error('chromium devtools connection failed during benchmark'));
    });
    return c;
  }

  send(method, params = {}, sessionId) {
    const id = this.#next++;
    const payload = { id, method, params };
    if (sessionId) payload.sessionId = sessionId;
    return new Promise((resolve, reject) => {
      this.#pending.set(id, { resolve, reject });
      this.#ws.send(JSON.stringify(payload));
    });
  }

  close() { this.#ws.close(); }
}

async function launchChromium() {
  const bin = resolveChromiumBinary(process.env.FM_CHROMIUM_BIN, PINS.chromium.binary);
  const profile = mkdtempSync(join(PROFILE_ROOT, 'fm-h2h-profile-'));
  const port = 9500 + Math.floor(Math.random() * 400);
  const proc = spawn(bin, [
    '--headless=new',
    `--remote-debugging-port=${port}`,
    `--user-data-dir=${profile}`,
    '--no-sandbox', '--disable-gpu', '--disable-dev-shm-usage',
    '--no-first-run', '--no-default-browser-check', '--disable-extensions',
    '--disable-background-networking', '--disable-sync', '--metrics-recording-only',
    '--mute-audio', '--hide-scrollbars',
    'about:blank',
  ], { stdio: ['ignore', 'ignore', 'ignore'] });
  let spawnError = null;
  proc.once('error', (error) => { spawnError = error; });

  const deadline = Date.now() + 30_000;
  for (;;) {
    if (spawnError) throw new Error(`chromium failed to start from ${bin}: ${spawnError.message}`);
    if (Date.now() > deadline) { proc.kill('SIGKILL'); throw new Error('chromium did not expose a devtools port within 30s'); }
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (res.ok) {
        const info = await res.json();
        return { proc, port, info, bin, cdp: await Cdp.attach(info.webSocketDebuggerUrl) };
      }
    } catch { /* not up yet */ }
    await new Promise((r) => setTimeout(r, 120));
  }
}

// ---------------------------------------------------------------- deadlines

/** Thrown when an in-page evaluation outlives its wall budget. Distinguished from a render error. */
class Deadline extends Error {}

/**
 * Race `promise` against `ms`.
 *
 * mermaid's layout work is synchronous JavaScript, so it holds the page's main thread and cannot be
 * interrupted from outside: once we time one out, that page is wedged for good and the caller must
 * relaunch the browser. That is the honest cost of asking "does it finish?", and it is why the
 * budget is generous -- a DNF must mean mermaid did not finish, never that the harness was impatient.
 */
function withDeadline(promise, ms) {
  if (!Number.isFinite(ms)) return promise;
  if (ms <= 0) {
    promise.catch(() => {});
    return Promise.reject(new Deadline('item wall budget already exhausted'));
  }
  let timer;
  return Promise.race([
    promise.finally(() => clearTimeout(timer)),
    new Promise((_, reject) => {
      timer = setTimeout(() => reject(new Deadline(`exceeded the ${Math.round(ms / 1000)} s budget`)), ms);
    }),
  ]);
}

// ---------------------------------------------------------------- statistics

function stats(samples) {
  const xs = [...samples].sort((a, b) => a - b);
  const n = xs.length;
  const pct = (p) => xs[Math.min(n - 1, Math.max(0, Math.ceil((p / 100) * n) - 1))];
  const mean = xs.reduce((a, b) => a + b, 0) / n;
  const variance = n > 1 ? xs.reduce((a, b) => a + (b - mean) ** 2, 0) / (n - 1) : 0;
  const sd = Math.sqrt(variance);
  const mid = Math.floor(n / 2);
  const median = n % 2 === 0 ? (xs[mid - 1] + xs[mid]) / 2 : xs[mid];
  // See the `mad_pct` doc comment in crates/fm-cli/examples/headtohead.rs: timing noise is
  // one-sided, so MAD measures dispersion of the uncontaminated regime while sd does not.
  const devs = xs.map((x) => Math.abs(x - median)).sort((a, b) => a - b);
  const mad = devs[Math.max(0, Math.ceil(n / 2) - 1)];
  return {
    n,
    samples: xs,
    min: xs[0],
    p50: median,
    // With few reps a p95/p99 is just the max wearing a hat. Report only when the rank is real.
    p95: n >= 20 ? pct(95) : null,
    p99: n >= 100 ? pct(99) : null,
    max: xs[n - 1],
    mean,
    sd,
    cv_pct: mean > 0 ? (sd / mean) * 100 : 0,
    mad_pct: median > 0 ? (mad / median) * 100 : 0,
  };
}

function median(samples) {
  const xs = [...samples].sort((a, b) => a - b);
  const mid = Math.floor(xs.length / 2);
  return xs.length % 2 === 0 ? (xs[mid - 1] + xs[mid]) / 2 : xs[mid];
}

/** Deterministic percentile-bootstrap 95% CI on the median. */
function bootstrapMedianCi(ratios) {
  let state = 0x4f6cdd1d;
  const next = () => {
    state ^= state << 13;
    state ^= state >>> 17;
    state ^= state << 5;
    return state >>> 0;
  };
  const medians = [];
  for (let i = 0; i < BOOTSTRAP_RESAMPLES; i++) {
    const sample = [];
    for (let k = 0; k < ratios.length; k++) sample.push(ratios[next() % ratios.length]);
    medians.push(median(sample));
  }
  medians.sort((a, b) => a - b);
  const tail = Math.floor(BOOTSTRAP_RESAMPLES / 40);
  return [medians[tail], medians[BOOTSTRAP_RESAMPLES - 1 - tail]];
}

function nullControl(ratios, checksumBytes) {
  if (ratios.length === 0) {
    return {
      n: 0,
      sufficient: false,
      median: null,
      ci95_lo: null,
      ci95_hi: null,
      half_width: null,
      min_decidable_2x: null,
      cv_pct: null,
      mad_pct: null,
      cv_gate: 'never',
      checksum_bytes: checksumBytes,
    };
  }
  const ratioMedian = median(ratios);
  const mean = ratios.reduce((a, b) => a + b, 0) / ratios.length;
  const variance = ratios.length > 1
    ? ratios.reduce((a, b) => a + (b - mean) ** 2, 0) / (ratios.length - 1)
    : 0;
  const sd = Math.sqrt(variance);
  const mad = median(ratios.map((x) => Math.abs(x - ratioMedian)));
  const [ci95Lo, ci95Hi] = bootstrapMedianCi(ratios);
  const halfWidth = Math.max(Math.abs(ci95Hi - 1), Math.abs(ci95Lo - 1));
  return {
    n: ratios.length,
    sufficient: ratios.length >= MIN_NULL_ROUNDS,
    median: ratioMedian,
    ci95_lo: ci95Lo,
    ci95_hi: ci95Hi,
    half_width: halfWidth,
    min_decidable_2x: Math.max(1.01, 1 + 2 * halfWidth),
    cv_pct: Number((mean === 0 ? 0 : (sd / mean) * 100).toFixed(2)),
    mad_pct: Number((ratioMedian === 0 ? 0 : (mad / ratioMedian) * 100).toFixed(2)),
    cv_gate: 'never',
    checksum_bytes: checksumBytes,
  };
}

/**
 * Long edit/CI traces cannot safely repeat every null arm in one Chromium page. mermaid retains
 * enough process state across hundreds of renders that an A/A prelude can turn the eventual real
 * sample into the nineteenth consecutive editing session -- or kill the renderer before it gets
 * there. Each sample therefore gets fresh browser state once an item reaches 100 documents. The
 * browser processes remain children of this one harness invocation, and every arm still runs the
 * runtime identity check against the pinned bundle.
 */
function isolatesSampleState(documentCount) {
  return documentCount >= ISOLATED_SAMPLE_MIN_DOCUMENTS;
}

/**
 * Pick the most expensive-looking document for the preflight probe.
 *
 * The original extended corpus grows monotonically, so its final revision is also its largest.
 * Realistic docs/catalog batches are right-skewed and shuffled; their final document may be the
 * smallest in the job. Probe the largest UTF-8 input instead, or budget sizing can overcommit null
 * rounds and manufacture a timeout before the real sample.
 */
function largestInput(texts) {
  let text = texts[0];
  let bytes = Buffer.byteLength(text, 'utf8');
  for (let i = 1; i < texts.length; i++) {
    const candidateBytes = Buffer.byteLength(texts[i], 'utf8');
    if (candidateBytes > bytes) {
      text = texts[i];
      bytes = candidateBytes;
    }
  }
  return { text, bytes };
}

// ---------------------------------------------------------------- in-page benchmark

// Runs inside chromium. One timed sample renders every revision of the item in order (a single-shot
// item has exactly one revision), which is what a live preview does as the user edits. Returns the
// timings plus every SVG so the driver can validate each one and sum the bytes.
const PAGE_BENCH = `async ({ texts, reps, warmup, nullReps, tag, jobBatch }) => {
  const m = window.mermaid;
  const out = {
    times: [],
    svgs: [],
    nullRatios: [],
    nullChecksumBytes: 0,
    nullOutputValid: true,
    error: null,
  };
  const plausibleSvg = (svg) =>
    typeof svg === 'string' &&
    svg.includes('<svg') &&
    svg.includes('</svg>') &&
    !svg.includes('aria-roledescription="error"') &&
    !/Syntax error in text/i.test(svg);
  const renderAll = async (suffix) => {
    let svgs = [];
    for (let job = 0; job < jobBatch; job++) {
      const completed = [];
      for (let k = 0; k < texts.length; k++) {
        const r = await m.render(tag + suffix + '_j' + job + '_' + k, texts[k]);
        if (!plausibleSvg(r.svg)) {
          throw new Error('invalid SVG in logical job ' + job + ', revision ' + k);
        }
        completed.push(r.svg);
      }
      svgs = completed;
    }
    return svgs;
  };
  const timed = async (suffix) => {
    const t0 = performance.now();
    const svgs = await renderAll(suffix);
    return { ms: performance.now() - t0, svgs };
  };
  try {
    for (let i = 0; i < warmup; i++) {
      await renderAll('_w' + i);
    }
    for (let i = 0; i < nullReps; i++) {
      let a;
      let b;
      if (i % 2 === 0) {
        a = await timed('_null_' + i + '_a');
        b = await timed('_null_' + i + '_b');
      } else {
        b = await timed('_null_' + i + '_b');
        a = await timed('_null_' + i + '_a');
      }
      out.nullRatios.push(a.ms / Math.max(Number.EPSILON, b.ms));
      out.nullOutputValid =
        out.nullOutputValid &&
        a.svgs.every(plausibleSvg) &&
        b.svgs.every(plausibleSvg);
      out.nullChecksumBytes +=
        a.svgs.reduce((sum, svg) => sum + svg.length, 0) +
        b.svgs.reduce((sum, svg) => sum + svg.length, 0);
    }
    for (let i = 0; i < reps; i++) {
      const measured = await timed('_r' + i);
      out.times.push(measured.ms);
      out.svgs = measured.svgs;
    }
  } catch (e) {
    out.error = String((e && e.message) || e);
  }
  return out;
}`;

// Parse-only equal-work boundary. The timer covers only the public `mermaid.parse()` calls for the
// complete job. Result normalization happens after `performance.now()` so neither engine is charged
// for serializing its native parse representation. A linked full-render equivalence artifact in the
// top-level driver proves the two native representations carry equivalent user-visible semantics.
const PAGE_PARSE_BENCH = `async ({
  texts, reps, warmup, nullReps, minSampleMs, calibrationTargetMs,
}) => {
  const m = window.mermaid;
  const out = {
    times: [],
    signatures: [],
    parseRecords: [],
    nullRatios: [],
    nullChecksumBytes: 0,
    nullOutputValid: true,
    effectIntegratedSampleMs: [],
    nullIntegratedSampleMs: [],
    effectBatches: [],
    nullBatches: [],
    deterministicOutput: true,
    batch: 1,
    error: null,
  };
  const normalize = (parsed) => {
    const config =
      parsed && typeof parsed === 'object' && parsed.config && typeof parsed.config === 'object'
        ? parsed.config
        : null;
    return {
      accepted: parsed !== false,
      diagramType: parsed && typeof parsed === 'object' ? (parsed.diagramType ?? null) : null,
      configNonempty: config !== null && Object.keys(config).length > 0,
    };
  };
  const parseAll = async () => {
    const parsed = [];
    for (const text of texts) parsed.push(await m.parse(text));
    return parsed;
  };
  const inspect = (parsed) => ({
    records: parsed.map(normalize),
    signatures: parsed.map((value) => JSON.stringify(value)),
  });
  const same = (left, right) =>
    left.length === right.length && left.every((value, index) => value === right[index]);
  const integrated = async (batch) => {
    const parsedBatches = new Array(batch);
    const t0 = performance.now();
    for (let i = 0; i < batch; i++) parsedBatches[i] = await parseAll();
    const integratedMs = performance.now() - t0;
    const inspectedBatches = parsedBatches.map(inspect);
    const inspected = inspectedBatches[inspectedBatches.length - 1];
    return {
      ms: integratedMs / batch,
      integratedMs,
      records: inspected.records,
      signatures: inspected.signatures,
      inspectedBatches,
    };
  };
  try {
    for (let i = 0; i < warmup; i++) await parseAll();
    const reference = inspect(await parseAll());
    out.signatures = reference.signatures;
    out.parseRecords = reference.records;
    const validateDeterminism = (sample, label) => {
      for (const inspected of sample.inspectedBatches ?? [sample]) {
        if (
          !same(inspected.signatures, reference.signatures) ||
          JSON.stringify(inspected.records) !== JSON.stringify(reference.records)
        ) {
          out.deterministicOutput = false;
          throw new Error('nondeterministic mermaid.parse result in ' + label);
        }
      }
    };
    let batch = 1;
    for (let attempt = 0; attempt < 4; attempt++) {
      const calibration = await integrated(batch);
      validateDeterminism(calibration, 'calibration sample ' + (attempt + 1));
      if (calibration.integratedMs >= calibrationTargetMs) break;
      batch = Math.max(
        batch + 1,
        Math.ceil(batch * calibrationTargetMs / Math.max(Number.EPSILON, calibration.integratedMs)),
      );
    }
    out.batch = batch;
    for (let i = 0; i < nullReps; i++) {
      let a;
      let b;
      if (i % 2 === 0) {
        a = await integrated(batch);
        b = await integrated(batch);
      } else {
        b = await integrated(batch);
        a = await integrated(batch);
      }
      validateDeterminism(a, 'A/A arm A sample ' + (i + 1));
      validateDeterminism(b, 'A/A arm B sample ' + (i + 1));
      out.nullRatios.push(a.ms / Math.max(Number.EPSILON, b.ms));
      out.nullIntegratedSampleMs.push(a.integratedMs, b.integratedMs);
      out.nullBatches.push(batch, batch);
      out.nullOutputValid =
        out.nullOutputValid &&
        a.records.every((record) => record.accepted === true) &&
        b.records.every((record) => record.accepted === true);
      out.nullChecksumBytes +=
        a.signatures.reduce((sum, value) => sum + value.length, 0) +
        b.signatures.reduce((sum, value) => sum + value.length, 0);
    }
    for (let i = 0; i < reps; i++) {
      const measured = await integrated(batch);
      validateDeterminism(measured, 'effect sample ' + (i + 1));
      out.times.push(measured.ms);
      out.effectIntegratedSampleMs.push(measured.integratedMs);
      out.effectBatches.push(batch);
    }
    const after = inspect(await parseAll());
    validateDeterminism(after, 'post-measurement check');
    out.sampleFloorValid =
      out.effectIntegratedSampleMs.length > 0 &&
      [...out.effectIntegratedSampleMs, ...out.nullIntegratedSampleMs]
        .every((value) => value >= minSampleMs);
  } catch (e) {
    out.error = String((e && e.message) || e);
  }
  return out;
}`;

// One untimed parse + render of a single document, reporting the render's own cost. Parse acceptance
// is kept outside the timed region and proves that a render failure is not merely invalid syntax.
// Run before the timed loop on budgeted items so we learn what one render costs *before* committing
// to `reps` of them -- without it, a large corpus either caps reps blindly or spends an hour
// discovering a single item is slow.
const PAGE_PROBE = `async ({ text, tag }) => {
  const parsed = await window.mermaid.parse(text);
  const t0 = performance.now();
  try {
    const r = await window.mermaid.render(tag + '_probe', text);
    return {
      ms: performance.now() - t0,
      bytes: r.svg.length,
      svg: r.svg,
      parseAccepted: parsed !== false,
      renderError: null,
    };
  } catch (e) {
    return {
      ms: performance.now() - t0,
      bytes: null,
      svg: null,
      parseAccepted: parsed !== false,
      renderError: String((e && e.message) || e),
    };
  }
}`;

if (has('self-test')) {
  const timing = stats([3, 1]);
  if (timing.p50 !== 2) throw new Error('timing median must average the two middle values');
  if (timing.samples.join(',') !== '1,3') throw new Error('timing samples must remain available for the effect CI');
  const perfect = nullControl(Array.from({ length: 41 }, () => 1), 1234);
  if (perfect.ci95_lo !== 1 || perfect.ci95_hi !== 1 || perfect.half_width !== 0) {
    throw new Error(`perfect-null bootstrap regression: ${JSON.stringify(perfect)}`);
  }
  const insufficient = nullControl([1], 7);
  if (insufficient.sufficient || insufficient.n !== 1) {
    throw new Error(`null sample-floor regression: ${JSON.stringify(insufficient)}`);
  }
  if (
    isolatesSampleState(ISOLATED_SAMPLE_MIN_DOCUMENTS - 1) ||
    !isolatesSampleState(ISOLATED_SAMPLE_MIN_DOCUMENTS)
  ) {
    throw new Error('long-trace sample-isolation threshold regression');
  }
  const probe = largestInput(['three', 'ééé']);
  if (probe.text !== 'ééé' || probe.bytes !== 6) {
    throw new Error(`largest-input probe regression: ${JSON.stringify(probe)}`);
  }
  if (
    resolveChromiumBinary('/override/chrome', '/pinned/chromium', (path) => path === '/override/chrome') !==
    '/override/chrome'
  ) {
    throw new Error('chromium override selection regression');
  }
  if (
    resolveChromiumBinary('', '/pinned/chromium', (path) => path === '/pinned/chromium') !==
    '/pinned/chromium'
  ) {
    throw new Error('pinned chromium selection regression');
  }
  let missingChromiumRejected = false;
  try {
    resolveChromiumBinary('/missing/chrome', '/pinned/chromium', () => false);
  } catch (error) {
    missingChromiumRejected =
      String(error.message).includes(
        'FM_CHROMIUM_BIN is not an executable absolute path: /missing/chrome',
      );
  }
  if (!missingChromiumRejected) throw new Error('missing chromium executable was not rejected');
  if (
    effectRepsForMode(2, true) !== MIN_NULL_ROUNDS ||
    effectRepsForMode(12, true) !== 12 ||
    effectRepsForMode(2, false) !== 2
  ) {
    throw new Error('parse effect-sample floor regression');
  }
  new Script(`(${PAGE_BENCH})`);
  new Script(`(${PAGE_PARSE_BENCH})`);
  new Script(`(${PAGE_PROBE})`);
  let parseCalls = 0;
  const parsePage = new Script(`(${PAGE_PARSE_BENCH})`).runInNewContext({
    window: {
      mermaid: {
        parse: async (text) => {
          parseCalls += 1;
          return { diagramType: text };
        },
      },
    },
    performance: globalThis.performance,
  });
  const parsePageResult = await parsePage({
    texts: ['flowchart-v2', 'stateDiagram'],
    reps: 2,
    warmup: 1,
    nullReps: 1,
    minSampleMs: 0,
    calibrationTargetMs: 0,
  });
  if (
    parsePageResult.batch !== 1 ||
    parsePageResult.times.length !== 2 ||
    parsePageResult.nullRatios.length !== 1 ||
    parsePageResult.effectIntegratedSampleMs.length !== 2 ||
    parsePageResult.nullIntegratedSampleMs.length !== 2 ||
    parsePageResult.deterministicOutput !== true ||
    parsePageResult.parseRecords.map((record) => record.diagramType).join(',') !==
      'flowchart-v2,stateDiagram' ||
    parseCalls !== 16
  ) {
    throw new Error(`parse page protocol regression: ${JSON.stringify({ parsePageResult, parseCalls })}`);
  }
  let unstableCalls = 0;
  const unstableParsePage = new Script(`(${PAGE_PARSE_BENCH})`).runInNewContext({
    window: {
      mermaid: {
        parse: async () => {
          unstableCalls += 1;
          return { diagramType: `flowchart-v${unstableCalls}` };
        },
      },
    },
    performance: globalThis.performance,
  });
  const unstableResult = await unstableParsePage({
    texts: ['flowchart'],
    reps: 1,
    warmup: 0,
    nullReps: 0,
    minSampleMs: 0,
    calibrationTargetMs: 0,
  });
  if (
    unstableResult.deterministicOutput !== false ||
    !unstableResult.error?.includes('nondeterministic mermaid.parse result')
  ) {
    throw new Error(`parse determinism mutation escaped: ${JSON.stringify(unstableResult)}`);
  }
  let transientCalls = 0;
  let clockIndex = 0;
  const clock = [0, 1, 2, 4];
  const transientParsePage = new Script(`(${PAGE_PARSE_BENCH})`).runInNewContext({
    window: {
      mermaid: {
        parse: async () => {
          transientCalls += 1;
          return { diagramType: transientCalls === 3 ? 'stateDiagram' : 'flowchart-v2' };
        },
      },
    },
    performance: {
      now: () => clock[clockIndex++] ?? clock[clock.length - 1],
    },
  });
  const transientResult = await transientParsePage({
    texts: ['flowchart'],
    reps: 1,
    warmup: 0,
    nullReps: 0,
    minSampleMs: 0,
    calibrationTargetMs: 2,
  });
  if (
    transientResult.deterministicOutput !== false ||
    !transientResult.error?.includes('nondeterministic mermaid.parse result')
  ) {
    throw new Error(`inner-batch parse mutation escaped: ${JSON.stringify(transientResult)}`);
  }
  console.log(JSON.stringify({
    self_test: 'ok',
    perfect,
    insufficient,
    page_functions_compiled: 3,
    parse_page_calls: parseCalls,
    parse_determinism_mutation: 'rejected',
    inner_batch_determinism_mutation: 'rejected',
    chromium_binary_resolution_cases: 3,
  }));
  process.exit(0);
}

/** mermaid renders a placeholder SVG on parse failure instead of throwing; treat that as an error. */
function validate(svg) {
  if (typeof svg !== 'string' || svg.length === 0) return 'empty output';
  if (!svg.includes('<svg') || !svg.includes('</svg>')) return 'not an svg document';
  if (svg.includes('aria-roledescription="error"')) return 'mermaid rendered its error placeholder';
  if (/Syntax error in text/i.test(svg)) return 'mermaid reported a syntax error';
  return null;
}

// ---------------------------------------------------------------- main

const only = arg('only');
const mode = arg('mode', 'render');
if (!['render', 'parse'].includes(mode)) {
  log(`--mode must be render or parse, got ${JSON.stringify(mode)}`);
  process.exit(2);
}
const parseOnly = mode === 'parse';
const repsScale = Number(arg('reps-scale', '1'));
const jobBatch = Number(arg('job-batch', '1'));
if (!Number.isSafeInteger(jobBatch) || jobBatch < 1) {
  log(`--job-batch must be a positive safe integer, got ${JSON.stringify(jobBatch)}`);
  process.exit(2);
}
const forceSampleIsolation = has('isolate-samples');
// Scales every item's `js_budget_ms`. A smoke run wants short budgets; a claim run wants the
// declared ones. Recorded on every DNF row, because "did not finish" is only meaningful with the
// budget it did not finish inside.
const budgetScale = Number(arg('js-budget-scale', '1'));
const securityLevel = arg('security-level', PINS.mermaid.security_level);
// Writes each item's final SVG to <dir>/<id>.mermaid.svg. Used to settle output-contract questions
// ("does mermaid emit per-element role/tabindex/<title>?") against the real comparator output.
const dumpSvgDir = arg('dump-svg');
// `--dump-svg` alone keeps its original one-file-per-item behaviour (settling output-contract
// questions). `--dump-all-revisions` additionally writes every revision, which is what the
// cross-engine equivalence phase needs for a multi-diagram job.
const dumpAllRevisions = has('dump-all-revisions');
// Equivalence needs exactly one set of rendered bytes, not timing samples. This mode is deliberately
// separate from `--reps-scale`: measurement runs must retain the nine-round A/A floor even when
// their effect sample count is scaled down.
const renderOnce = has('render-once');
if (dumpSvgDir) mkdirSync(dumpSvgDir, { recursive: true });
if (dumpAllRevisions && !dumpSvgDir) {
  log('--dump-all-revisions requires --dump-svg <dir>');
  process.exit(2);
}

const { text: bundleText, version, url, sha256: bundleSha } = await bundle();

const PAGE_HTML =
  '<!DOCTYPE html><html><head><meta charset="utf-8"></head><body><div id="container"></div></body></html>';
// DISPATCH-TRAP GUARD. Verifying the bundle's SHA-256 before injection proves which *file* we
// loaded; it does not prove that the object answering `render()` is that library. franken_networkx
// published a 2.6x win whose baseline was already dispatched to the fast implementation -- genuine
// NetworkX was 1.88x SLOWER. So assert the incumbent's identity at RUNTIME, inside the page:
//   - `mermaid.version` (or the bundle's own reported version) equals the pin, and
//   - `render` is a function that did not come from us -- its source must not be a bound/proxied
//     shim, which is what a dispatched baseline looks like from the caller's side.
// Any mismatch aborts the run rather than producing a number.
const INIT_EXPR = `(() => {
      if (!window.mermaid) return 'window.mermaid missing after bundle eval';
      const m = window.mermaid;
      if (typeof m.render !== 'function') return 'mermaid.render is not a function';
      if (typeof m.parse !== 'function') return 'mermaid.parse is not a function';
      const renderSrc = Function.prototype.toString.call(m.render);
      const parseSrc = Function.prototype.toString.call(m.parse);
      if (window.__fm_probe)
        return 'PROBE render=' + renderSrc.slice(0, 80) + ' parse=' + parseSrc.slice(0, 80)
          + ' ver=' + String(m.version);
      // A dispatched baseline presents as a bound wrapper -- a zero-arg native stub with no body.
      // A genuine bundled implementation, minified arrow or function, carries its own source text.
      if (/^\s*function\s*\(\s*\)\s*\{\s*\[native code\]\s*\}\s*$/.test(renderSrc))
        return 'mermaid.render is a bound/native shim, not the library function';
      if (/^\s*function\s*\(\s*\)\s*\{\s*\[native code\]\s*\}\s*$/.test(parseSrc))
        return 'mermaid.parse is a bound/native shim, not the library function';
      const reported = String(m.version ?? (typeof m.getVersion === 'function' ? m.getVersion() : ''));
      const want = ${JSON.stringify(PINS.mermaid.version)};
      if (reported && reported !== want)
        return 'mermaid reports version ' + reported + ', pinned ' + want;
      window.__fm_incumbent = { version: reported || want, version_reported: Boolean(reported) };
      m.initialize(${JSON.stringify({
        startOnLoad: false,
        securityLevel,
        maxEdges: PINS.mermaid.max_edges,
        maxTextSize: PINS.mermaid.max_text_size,
      })});
      return 'ok';
    })()`;

/**
 * Launch a browser and bring one page up to "mermaid initialized". Factored out because timing an
 * item out wedges its page permanently (mermaid's layout is synchronous), so a DNF has to be
 * followed by a fresh browser before the next item can be measured.
 */
async function newBrowser() {
  const { proc, cdp, info, bin } = await launchChromium();
  const { targetId } = await cdp.send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await cdp.send('Target.attachToTarget', { targetId, flatten: true });
  await cdp.send('Page.enable', {}, sessionId);
  await cdp.send('Runtime.enable', {}, sessionId);

  const { frameTree } = await cdp.send('Page.getFrameTree', {}, sessionId);
  await cdp.send('Page.setDocumentContent', { frameId: frameTree.frame.id, html: PAGE_HTML }, sessionId);

  const inject = await cdp.send('Runtime.evaluate', { expression: bundleText, returnByValue: false }, sessionId);
  if (inject.exceptionDetails) throw new Error(`bundle eval failed: ${inject.exceptionDetails.text}`);

  const init = await cdp.send('Runtime.evaluate', { expression: INIT_EXPR, returnByValue: true }, sessionId);
  switch (init.result.value) {
    case 'ok':
      break;
    default:
      throw new Error(String(init.result.value));
  }
  return { proc, cdp, sessionId, info, bin };
}

function killBrowser(b) {
  try { b.cdp.close(); } catch { /* already gone */ }
  try { b.proc.kill('SIGKILL'); } catch { /* already gone */ }
}

let browser = await newBrowser();
log(`browser=${browser.info.Browser} binary=${browser.bin} bundle=mermaid@${version} mode=${mode}`);

/** Evaluate `fn(args)` in the live page, under an optional wall deadline. */
function evaluateInPage(fn, args, deadlineMs) {
  return withDeadline(
    browser.cdp.send('Runtime.evaluate', {
      expression: `(${fn})(${JSON.stringify(args)})`,
      awaitPromise: true,
      returnByValue: true,
    }, browser.sessionId),
    deadlineMs,
  );
}

/**
 * Evaluate a whole render sample without forcing every SVG through one CDP WebSocket frame.
 *
 * The page still executes the complete Mermaid job once and records its own timer around that
 * unchanged work. Only after the promise resolves do we fetch the returned SVG array in bounded
 * chunks. Large CI jobs otherwise exceed Node's WebSocket frame ceiling before the harness can
 * hash or dump the incumbent output, even though Mermaid completed successfully.
 */
async function evaluateRenderInPage(fn, args, deadlineMs) {
  const evaluated = await withDeadline(
    browser.cdp.send('Runtime.evaluate', {
      expression: `(${fn})(${JSON.stringify(args)})`,
      awaitPromise: true,
      returnByValue: false,
    }, browser.sessionId),
    deadlineMs,
  );
  const objectId = evaluated.result?.objectId;
  if (evaluated.exceptionDetails || !objectId) return evaluated;

  try {
    const metadata = await browser.cdp.send('Runtime.callFunctionOn', {
      objectId,
      functionDeclaration: `function () {
        const { svgs, ...rest } = this;
        return { ...rest, svgCount: Array.isArray(svgs) ? svgs.length : 0 };
      }`,
      returnByValue: true,
    }, browser.sessionId);
    if (metadata.exceptionDetails) return metadata;

    const value = metadata.result.value;
    const svgs = [];
    for (let offset = 0; offset < value.svgCount; offset += 8) {
      const chunk = await browser.cdp.send('Runtime.callFunctionOn', {
        objectId,
        functionDeclaration:
          'function (start, count) { return this.svgs.slice(start, start + count); }',
        arguments: [{ value: offset }, { value: 8 }],
        returnByValue: true,
      }, browser.sessionId);
      if (chunk.exceptionDetails) return chunk;
      svgs.push(...chunk.result.value);
    }
    delete value.svgCount;
    value.svgs = svgs;
    return { result: { value } };
  } finally {
    await browser.cdp
      .send('Runtime.releaseObject', { objectId }, browser.sessionId)
      .catch(() => {});
  }
}

function remainingBudgetMs(startedAt, budgetMs) {
  if (!Number.isFinite(budgetMs)) return null;
  const remaining = budgetMs - (Date.now() - startedAt);
  if (remaining <= 0) throw new Deadline('item wall budget already exhausted');
  return remaining;
}

async function replaceBrowser() {
  killBrowser(browser);
  browser = await newBrowser();
}

/**
 * Render one complete multi-document sample in fresh Chromium state.
 *
 * Browser launch and bundle injection are outside the timed interval but inside the item wall
 * budget. The returned timing still covers every revision, in order, exactly once.
 */
async function isolatedSample(texts, tag, startedAt, budgetMs) {
  await replaceBrowser();
  const res = await evaluateRenderInPage(
    PAGE_BENCH,
    { texts, reps: 1, warmup: 0, nullReps: 0, tag },
    remainingBudgetMs(startedAt, budgetMs),
  );
  if (res.exceptionDetails) throw new Error(res.exceptionDetails.text);
  const out = res.result.value;
  const err =
    out.error ??
    out.svgs.map(validate).find(Boolean) ??
    (out.svgs.length === texts.length ? null : 'revision count mismatch') ??
    (out.times.length === 1 ? null : 'timing sample count mismatch');
  if (err) throw new Error(err);
  return {
    ms: out.times[0],
    svgs: out.svgs,
    bytes: out.svgs.reduce((sum, svg) => sum + svg.length, 0),
  };
}

/**
 * Host-orchestrated A/A for long traces. Every arm and every real sample starts from fresh browser
 * state, but all are children of this one `mermaid_bench.mjs` invocation. Ten null rounds balance
 * the alternating A-first/B-first order exactly.
 */
async function isolatedBenchmark(texts, reps, nullReps, tag, startedAt, budgetMs, itemId) {
  const nullRatios = [];
  let nullChecksumBytes = 0;
  for (let i = 0; i < nullReps; i++) {
    let a;
    let b;
    if (i % 2 === 0) {
      a = await isolatedSample(texts, `${tag}_null_${i}_a`, startedAt, budgetMs);
      b = await isolatedSample(texts, `${tag}_null_${i}_b`, startedAt, budgetMs);
    } else {
      b = await isolatedSample(texts, `${tag}_null_${i}_b`, startedAt, budgetMs);
      a = await isolatedSample(texts, `${tag}_null_${i}_a`, startedAt, budgetMs);
    }
    nullRatios.push(a.ms / Math.max(Number.EPSILON, b.ms));
    nullChecksumBytes += a.bytes + b.bytes;
    log(`null ${itemId}: ${i + 1}/${nullReps}`);
  }

  const times = [];
  let svgs = [];
  for (let i = 0; i < reps; i++) {
    const measured = await isolatedSample(texts, `${tag}_real_${i}`, startedAt, budgetMs);
    times.push(measured.ms);
    svgs = measured.svgs;
    log(`real ${itemId}: ${i + 1}/${reps}`);
  }
  return {
    times,
    svgs,
    nullRatios,
    nullChecksumBytes,
    nullOutputValid: true,
    error: null,
  };
}

async function isolatedParseSample(texts, startedAt, budgetMs) {
  await replaceBrowser();
  const res = await evaluateInPage(
    PAGE_PARSE_BENCH,
    {
      texts,
      reps: 1,
      warmup: 1,
      nullReps: 0,
      minSampleMs: PARSE_MIN_SAMPLE_MS,
      calibrationTargetMs: PARSE_CALIBRATION_TARGET_MS,
    },
    remainingBudgetMs(startedAt, budgetMs),
  );
  if (res.exceptionDetails) throw new Error(res.exceptionDetails.text);
  const out = res.result.value;
  const err =
    out.error ??
    (out.signatures.length === texts.length ? null : 'parse revision count mismatch') ??
    (out.times.length === 1 ? null : 'parse timing sample count mismatch') ??
    (out.parseRecords.every((record) => record.accepted === true)
      ? null
      : 'mermaid.parse rejected a revision') ??
    (out.sampleFloorValid ? null : 'parse integrated sample missed the floor');
  if (err) throw new Error(err);
  return {
    ms: out.times[0],
    signatures: out.signatures,
    records: out.parseRecords,
    batch: out.batch,
    integratedMs: out.effectIntegratedSampleMs[0],
    bytes: out.signatures.reduce((sum, value) => sum + value.length, 0),
  };
}

async function isolatedParseBenchmark(texts, reps, nullReps, startedAt, budgetMs, itemId) {
  const nullRatios = [];
  const nullIntegratedSampleMs = [];
  const nullBatches = [];
  let nullChecksumBytes = 0;
  let referenceSignatures = null;
  let referenceRecords = null;
  const checkDeterminism = (sample, label) => {
    const signatures = JSON.stringify(sample.signatures);
    const records = JSON.stringify(sample.records);
    referenceSignatures ??= signatures;
    referenceRecords ??= records;
    if (signatures !== referenceSignatures || records !== referenceRecords) {
      throw new Error(`nondeterministic mermaid.parse result across isolated ${label}`);
    }
  };
  for (let i = 0; i < nullReps; i++) {
    let a;
    let b;
    if (i % 2 === 0) {
      a = await isolatedParseSample(texts, startedAt, budgetMs);
      b = await isolatedParseSample(texts, startedAt, budgetMs);
    } else {
      b = await isolatedParseSample(texts, startedAt, budgetMs);
      a = await isolatedParseSample(texts, startedAt, budgetMs);
    }
    checkDeterminism(a, `A/A arm A sample ${i + 1}`);
    checkDeterminism(b, `A/A arm B sample ${i + 1}`);
    nullRatios.push(a.ms / Math.max(Number.EPSILON, b.ms));
    nullIntegratedSampleMs.push(a.integratedMs, b.integratedMs);
    nullBatches.push(a.batch, b.batch);
    nullChecksumBytes += a.bytes + b.bytes;
    log(`parse null ${itemId}: ${i + 1}/${nullReps}`);
  }

  const times = [];
  let signatures = [];
  let records = [];
  let batch = null;
  const effectIntegratedSampleMs = [];
  const effectBatches = [];
  for (let i = 0; i < reps; i++) {
    const measured = await isolatedParseSample(texts, startedAt, budgetMs);
    checkDeterminism(measured, `effect sample ${i + 1}`);
    times.push(measured.ms);
    effectIntegratedSampleMs.push(measured.integratedMs);
    effectBatches.push(measured.batch);
    signatures = measured.signatures;
    records = measured.records;
    batch = measured.batch;
    log(`parse real ${itemId}: ${i + 1}/${reps}`);
  }
  return {
    times,
    signatures,
    parseRecords: records,
    batch,
    effectIntegratedSampleMs,
    nullIntegratedSampleMs,
    effectBatches,
    nullBatches,
    deterministicOutput: true,
    sampleFloorValid:
      effectIntegratedSampleMs.length > 0 &&
      [...effectIntegratedSampleMs, ...nullIntegratedSampleMs]
        .every((value) => value >= PARSE_MIN_SAMPLE_MS),
    nullRatios,
    nullChecksumBytes,
    nullOutputValid: true,
    error: null,
  };
}

let failed = false;
try {
  // Matches run.mjs: one id, or a comma-separated list.
  const onlyIds = only ? new Set(only.split(',').map((s) => s.trim())) : null;
  const items = CORPUS.filter((i) => !onlyIds || onlyIds.has(i.id));
  for (const item of items) {
    const texts = generate(item);
    let reps = Math.max(1, Math.round(item.reps_js * repsScale));
    // A workload may predeclare more A/A pairs than effect samples when a completed retry has
    // exposed opposite-sign null-median movement. This raises precision without changing the
    // effect sample count, input, or verdict rule.
    let nullReps = Math.max(MIN_NULL_ROUNDS, item.null_reps_js ?? reps);
    // `mermaid.parse()` clears the diagram DB and reparses on every call. Keep its page hot so the
    // public parser boundary excludes one-time registration/JIT work; render mode retains the
    // fresh-browser isolation required for stateful long traces.
    const isolateSamples =
      !parseOnly && (forceSampleIsolation || isolatesSampleState(texts.length));
    if (isolateSamples) nullReps = Math.max(ISOLATED_NULL_ROUNDS, nullReps);
    // A declared `warmup_js: 0` means zero, not one: on an item that takes minutes per render, a
    // warmup pass doubles the item for no statistical gain. `Math.max(1, ...)` still guards the
    // pinned items, whose warmup must never be scaled away by `--reps-scale`.
    let warmup = item.warmup_js === 0 ? 0 : Math.max(1, Math.round(item.warmup_js * repsScale));
    if (parseOnly) {
      warmup = Math.max(1, warmup);
      // The parse driver requires an independent cross-runtime effect CI on every row. Many
      // render-era corpora predate that rule and carry only 1-3 expensive render samples; public
      // parse calls are cheap, so raise their effect arm to the same nine-sample minimum as A/A.
      reps = effectRepsForMode(reps, true);
    }
    if (renderOnce) {
      reps = 1;
      nullReps = 0;
      warmup = 0;
    }
    const budgetMs = item.js_budget_ms ? item.js_budget_ms * budgetScale : null;
    const tag = item.id.replace(/[^a-z0-9]/gi, '');
    const t0 = Date.now();
    const joined = texts.join(REVISION_SEP);
    const record = {
      engine: 'mermaid-js',
      version,
      bundle_url: url,
      bundle_sha256: bundleSha,
      chromium_binary: browser.bin,
      chromium_version: browser.info.Browser,
      security_level: securityLevel,
      worker_threads: 1,
      thread_count_requested: 1,
      thread_count_actually_used: 1,
      thread_probe: {
        method: 'single_cdp_page_main_execution_context',
        caller_workers_observed: 1,
        portable_across_isa: true,
        inside_timed_region: false,
      },
      execution_model: 'single_page_main_thread',
      measurement_mode: mode,
      measurement_boundary: parseOnly ? 'public_parse_validate' : 'parse_layout_render_svg',
      job_batch: parseOnly ? 1 : jobBatch,
      render_once: renderOnce,
      id: item.id,
      revisions: texts.length,
      input_sha256: sha256(joined),
      input_bytes: Buffer.byteLength(joined, 'utf8'),
    };
    let probeMs = null;
    let probeInputBytes = null;
    let probeParseAccepted = null;

    /**
     * Record a did-not-finish and re-arm the browser. Only reachable for `dnf_allowed` items.
     *
     * `kind` separates the two ways mermaid fails to produce a render, because they support
     * different claims. `timeout` means it was still working when the budget ran out, which bounds
     * the speedup from below. `failed` means it raised -- a stack overflow, its own size guardrail,
     * an OOM -- and there is no bound to state: at this size mermaid does not render the diagram at
     * all, and no amount of waiting changes that.
     */
    const dnf = async (phase, kind, reason) => {
      const elapsed = Date.now() - t0;
      log(`DNF  ${item.id}: ${phase} ${kind} ${reason} (${(elapsed / 1000).toFixed(1)}s elapsed)`);
      console.log(JSON.stringify({
        ...record,
        status: 'dnf',
        kind,
        phase,
        error: reason,
        budget_ms: budgetMs,
        elapsed_ms: elapsed,
        wall_s: elapsed / 1000,
        probe_parse_accepted: probeParseAccepted,
      }));
      // The page is wedged inside mermaid's synchronous layout; nothing short of a new process
      // gets it back.
      killBrowser(browser);
      browser = await newBrowser();
    };

    // Probe phase: one untimed render of the largest revision, so an item that cannot finish is
    // discovered in one render rather than `warmup + reps` of them.
    if (budgetMs && !parseOnly) {
      try {
        const probe = largestInput(texts);
        probeInputBytes = probe.bytes;
        const p = await evaluateInPage(PAGE_PROBE, { text: probe.text, tag }, budgetMs);
        if (p.exceptionDetails) throw new Error(p.exceptionDetails.text);
        const probeResult = p.result?.value;
        if (!probeResult || typeof probeResult !== 'object') {
          throw new Error('probe returned no structured result');
        }
        probeParseAccepted = probeResult.parseAccepted === true;
        if (!probeParseAccepted) {
          throw new Error('mermaid.parse did not accept the probe input');
        }
        if (probeResult.renderError) {
          throw new Error(`render failed after parse accepted: ${probeResult.renderError}`);
        }
        probeMs = probeResult.ms;
        const bad = validate(probeResult.svg);
        if (bad) throw new Error(bad);
        log(`probe ${item.id}: ${(probeMs / 1000).toFixed(2)}s for the largest revision`);
      } catch (e) {
        if (!item.dnf_allowed) throw new Error(`${item.id}: probe: ${e.message}`);
        await dnf(
          'probe',
          e instanceof Deadline ? 'timeout' : 'failed',
          e instanceof Deadline ? `mermaid did not finish one render, ${e.message}` : String(e.message),
        );
        continue;
      }
      // Reserve two renders per A/A round before sizing the real arm. If the declared budget cannot
      // afford a valid null, still attempt one real sample: it can honestly establish a DNF, but a
      // completed timing remains inconclusive and the top-level median-CI gate will reject it.
      const perSample = Math.max(1, probeMs * texts.length * jobBatch);
      const left = budgetMs - (Date.now() - t0);
      const affordable = Math.floor(left / perSample);
      if (affordable < 2 * nullReps + 1 + warmup) warmup = 0;
      if (affordable < 2 * nullReps + 1) nullReps = MIN_NULL_ROUNDS;
      if (affordable < 2 * MIN_NULL_ROUNDS + 1) {
        nullReps = 0;
        reps = 1;
      } else {
        reps = Math.max(1, Math.min(reps, affordable - warmup - 2 * nullReps));
      }
    }

    const args = {
      texts,
      reps,
      warmup,
      nullReps,
      tag,
      jobBatch,
      minSampleMs: PARSE_MIN_SAMPLE_MS,
      calibrationTargetMs: PARSE_CALIBRATION_TARGET_MS,
    };
    let res;
    let out;
    try {
      if (isolateSamples) {
        out = parseOnly
          ? await isolatedParseBenchmark(texts, reps, nullReps, t0, budgetMs, item.id)
          : await isolatedBenchmark(texts, reps, nullReps, tag, t0, budgetMs, item.id);
      } else {
        res = await evaluateInPage(
          parseOnly ? PAGE_PARSE_BENCH : PAGE_BENCH,
          args,
          budgetMs ? budgetMs - (Date.now() - t0) : null,
        );
      }
    } catch (e) {
      if (!item.dnf_allowed || (parseOnly && !(e instanceof Deadline))) {
        throw new Error(`${item.id}: ${e.message}`);
      }
      await dnf(
        'timed',
        e instanceof Deadline ? 'timeout' : 'failed',
        e instanceof Deadline ? `mermaid did not finish ${reps} sample(s), ${e.message}` : String(e.message),
      );
      continue;
    }

    if (!isolateSamples) {
      if (res.exceptionDetails) throw new Error(`${item.id}: ${res.exceptionDetails.text}`);
      out = res.result.value;
    }
    // Every revision is validated, not just the last. Parse mode requires public-API acceptance;
    const outputs = parseOnly ? out.signatures : out.svgs;
    const err = parseOnly
      ? out.error ??
        (out.deterministicOutput ? null : 'nondeterministic mermaid.parse result') ??
        (out.nullOutputValid ? null : 'A/A null control rejected a parse') ??
        (outputs.length === texts.length ? null : 'parse revision count mismatch') ??
        (out.parseRecords.every((record) => record.accepted === true)
          ? null
          : 'mermaid.parse rejected a revision') ??
        (out.sampleFloorValid ? null : 'parse integrated sample missed the floor')
      : out.error ??
        (out.nullOutputValid ? null : 'A/A null control produced invalid SVG') ??
        outputs.map(validate).find(Boolean) ??
        (outputs.length === texts.length ? null : 'revision count mismatch');
    record.wall_s = (Date.now() - t0) / 1000;
    if (err) {
      // An in-page failure on a budgeted item is mermaid's own guardrail or an OOM at a size it
      // cannot serve -- a did-not-finish, not a broken harness.
      if (item.dnf_allowed && !parseOnly) {
        await dnf('timed', 'failed', err);
        continue;
      }
      failed = true;
      log(`FAIL ${item.id}: ${err}`);
      console.log(JSON.stringify({ ...record, status: 'error', error: err }));
      continue;
    }
    const ms = stats(out.times);
    const nullStats = nullControl(out.nullRatios, out.nullChecksumBytes);
    const outputBytes = outputs.reduce((a, value) => a + value.length, 0);
    if (dumpSvgDir && !parseOnly) {
      writeFileSync(join(dumpSvgDir, `${item.id}.mermaid.svg`), outputs[outputs.length - 1]);
      // Cross-engine equivalence (`bd-evx6`) compares every diagram in the job, not just the
      // last one. A 500-diagram CI batch is a single item with 500 revisions, so the per-revision
      // dump is what makes the check cover the whole batch. `output_sha256` below hashes these same
      // bytes joined, which lets the checker prove it read the measured render.
      if (dumpAllRevisions) {
        for (const [revision, svg] of outputs.entries()) {
          writeFileSync(join(dumpSvgDir, `${item.id}.rev${String(revision).padStart(5, '0')}.mermaid.svg`), svg);
        }
      }
    }
    const durationNs = Object.fromEntries(
      Object.entries(ms)
        .filter(([k]) => !['cv_pct', 'mad_pct'].includes(k))
        .map(([k, v]) => {
          if (k === 'n' || v === null) return [k, v];
          if (k === 'samples') return [k, v.map((sample) => Math.round(sample * 1e6))];
          return [k, Math.round(v * 1e6)];
        }),
    );
    console.log(JSON.stringify({
      ...record,
      status: 'ok',
      warmup,
      reps,
      null_reps: nullReps,
      sample_isolation: isolateSamples ? 'fresh_browser_per_arm' : 'single_browser',
      batch: parseOnly ? out.batch : 1,
      min_sample_ns: parseOnly ? PARSE_MIN_SAMPLE_MS * 1e6 : null,
      calibration_target_ns: parseOnly ? PARSE_CALIBRATION_TARGET_MS * 1e6 : null,
      integrated_sample_ns: parseOnly
        ? Math.round(stats(out.effectIntegratedSampleMs).p50 * 1e6)
        : null,
      effect_integrated_samples_ns: parseOnly
        ? out.effectIntegratedSampleMs.map((sample) => Math.round(sample * 1e6))
        : null,
      null_integrated_samples_ns: parseOnly
        ? out.nullIntegratedSampleMs.map((sample) => Math.round(sample * 1e6))
        : null,
      effect_batches: parseOnly ? out.effectBatches : null,
      null_batches: parseOnly ? out.nullBatches : null,
      parse_deterministic_output: parseOnly ? out.deterministicOutput : null,
      budget_ms: budgetMs,
      probe_ms: probeMs === null ? null : Math.round(probeMs),
      probe_input_bytes: probeInputBytes,
      probe_parse_accepted: parseOnly ? true : probeParseAccepted,
      ...(parseOnly ? { parse_ns: durationNs } : { render_ns: durationNs }),
      cv_pct: Number(ms.cv_pct.toFixed(2)),
      mad_pct: Number(ms.mad_pct.toFixed(2)),
      null_control: nullStats,
      parse_accepted_revisions: parseOnly
        ? out.parseRecords.filter((record) => record.accepted === true).length
        : null,
      parse_diagram_types: parseOnly
        ? [...new Set(out.parseRecords.map((record) => record.diagramType).filter(Boolean))].sort()
        : null,
      parse_diagram_types_ordered: parseOnly
        ? out.parseRecords.map((record) => record.diagramType)
        : null,
      parse_nonempty_config_revisions: parseOnly
        ? out.parseRecords.filter((record) => record.configNonempty).length
        : null,
      ...(parseOnly
        ? {
            parse_result_bytes: outputBytes,
            parse_result_sha256: sha256(outputs.join('')),
          }
        : {
            output_bytes: outputBytes,
            output_sha256: sha256(outputs.join('')),
          }),
    }));
    log(
      `ok   ${item.id}  ${parseOnly ? 'parse-' : ''}p50=${ms.p50.toFixed(1)}ms ` +
      `null=${nullStats.median === null ? 'missing' : `${nullStats.median.toFixed(6)} [${nullStats.ci95_lo.toFixed(6)},${nullStats.ci95_hi.toFixed(6)}]`} ` +
      `bytes=${outputBytes}`,
    );
  }
} finally {
  killBrowser(browser);
}

if (failed) { log('one or more comparator renders failed'); process.exit(2); }
