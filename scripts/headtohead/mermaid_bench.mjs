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
// Emits one JSON object per corpus item on stdout. A mermaid render that throws, or that produces
// mermaid's "Syntax error" placeholder SVG, is reported as `status: "error"` and makes the process
// exit non-zero -- a failed comparator render is never a silent win for frankenmermaid.
//
// DID-NOT-FINISH. The XL corpus items reach sizes where mermaid may not complete at all. That is a
// result, not a harness failure, so items carrying `dnf_allowed` report `status: "dnf"` with the
// wall budget attached instead of failing the run. A DNF yields a *lower bound* on the speedup and
// is never mixed into the ratio aggregate: we say "mermaid did not finish inside B seconds", which
// is a claim about mermaid, not a number we made up for it.

import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { homedir } from 'node:os';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { Script } from 'node:vm';
import { CORPUS, REVISION_SEP, generate, sha256 } from './corpus.mjs';

const HERE = dirname(fileURLToPath(import.meta.url));
const PINS = JSON.parse(readFileSync(join(HERE, 'pins.json'), 'utf8'));
const MIN_NULL_ROUNDS = 9;
const BOOTSTRAP_RESAMPLES = 2_000;

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
      if (!('id' in msg)) return; // a CDP event, not a response to one of our commands
      const p = c.#pending.get(msg.id);
      if (!p) return;
      c.#pending.delete(msg.id);
      if (msg.error) p.reject(new Error(`${msg.error.message} (cdp ${msg.error.code})`));
      else p.resolve(msg.result);
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
  const bin = PINS.chromium.binary;
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

  const deadline = Date.now() + 30_000;
  for (;;) {
    if (Date.now() > deadline) { proc.kill('SIGKILL'); throw new Error('chromium did not expose a devtools port within 30s'); }
    try {
      const res = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (res.ok) {
        const info = await res.json();
        return { proc, port, info, cdp: await Cdp.attach(info.webSocketDebuggerUrl) };
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
  if (!Number.isFinite(ms) || ms <= 0) return promise;
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

// ---------------------------------------------------------------- in-page benchmark

// Runs inside chromium. One timed sample renders every revision of the item in order (a single-shot
// item has exactly one revision), which is what a live preview does as the user edits. Returns the
// timings plus every SVG so the driver can validate each one and sum the bytes.
const PAGE_BENCH = `async ({ texts, reps, warmup, nullReps, tag }) => {
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
    const svgs = [];
    for (let k = 0; k < texts.length; k++) {
      const r = await m.render(tag + suffix + '_' + k, texts[k]);
      svgs.push(r.svg);
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

// One untimed render of a single document, reporting its own cost. Run before the timed loop on
// budgeted items so we learn what one render costs *before* committing to `reps` of them -- without
// it, a 19-item corpus either caps reps blindly or spends an hour discovering a single item is slow.
const PAGE_PROBE = `async ({ text, tag }) => {
  const t0 = performance.now();
  const r = await window.mermaid.render(tag + '_probe', text);
  return { ms: performance.now() - t0, bytes: r.svg.length, svg: r.svg };
}`;

if (has('self-test')) {
  if (stats([1, 3]).p50 !== 2) throw new Error('timing median must average the two middle values');
  const perfect = nullControl(Array.from({ length: 41 }, () => 1), 1234);
  if (perfect.ci95_lo !== 1 || perfect.ci95_hi !== 1 || perfect.half_width !== 0) {
    throw new Error(`perfect-null bootstrap regression: ${JSON.stringify(perfect)}`);
  }
  const insufficient = nullControl([1], 7);
  if (insufficient.sufficient || insufficient.n !== 1) {
    throw new Error(`null sample-floor regression: ${JSON.stringify(insufficient)}`);
  }
  new Script(`(${PAGE_BENCH})`);
  new Script(`(${PAGE_PROBE})`);
  console.log(JSON.stringify({
    self_test: 'ok',
    perfect,
    insufficient,
    page_functions_compiled: 2,
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
const repsScale = Number(arg('reps-scale', '1'));
// Scales every item's `js_budget_ms`. A smoke run wants short budgets; a claim run wants the
// declared ones. Recorded on every DNF row, because "did not finish" is only meaningful with the
// budget it did not finish inside.
const budgetScale = Number(arg('js-budget-scale', '1'));
const securityLevel = arg('security-level', PINS.mermaid.security_level);
// Writes each item's final SVG to <dir>/<id>.mermaid.svg. Used to settle output-contract questions
// ("does mermaid emit per-element role/tabindex/<title>?") against the real comparator output.
const dumpSvgDir = arg('dump-svg');
if (dumpSvgDir) mkdirSync(dumpSvgDir, { recursive: true });

const { text: bundleText, version, url, sha256: bundleSha } = await bundle();

const PAGE_HTML =
  '<!DOCTYPE html><html><head><meta charset="utf-8"></head><body><div id="container"></div></body></html>';
const INIT_EXPR = `(() => {
      if (!window.mermaid) return 'window.mermaid missing after bundle eval';
      window.mermaid.initialize(${JSON.stringify({
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
  const { proc, cdp, info } = await launchChromium();
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
  return { proc, cdp, sessionId, info };
}

function killBrowser(b) {
  try { b.cdp.close(); } catch { /* already gone */ }
  try { b.proc.kill('SIGKILL'); } catch { /* already gone */ }
}

let browser = await newBrowser();
log(`browser=${browser.info.Browser} bundle=mermaid@${version}`);

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

let failed = false;
try {
  // Matches run.mjs: one id, or a comma-separated list.
  const onlyIds = only ? new Set(only.split(',').map((s) => s.trim())) : null;
  const items = CORPUS.filter((i) => !onlyIds || onlyIds.has(i.id));
  for (const item of items) {
    const texts = generate(item);
    let reps = Math.max(1, Math.round(item.reps_js * repsScale));
    let nullReps = Math.max(MIN_NULL_ROUNDS, reps);
    // A declared `warmup_js: 0` means zero, not one: on an item that takes minutes per render, a
    // warmup pass doubles the item for no statistical gain. `Math.max(1, ...)` still guards the
    // pinned items, whose warmup must never be scaled away by `--reps-scale`.
    let warmup = item.warmup_js === 0 ? 0 : Math.max(1, Math.round(item.warmup_js * repsScale));
    const budgetMs = item.js_budget_ms ? item.js_budget_ms * budgetScale : null;
    const tag = item.id.replace(/[^a-z0-9]/gi, '');
    const t0 = Date.now();
    const joined = texts.join(REVISION_SEP);
    const record = {
      engine: 'mermaid-js',
      version,
      bundle_url: url,
      bundle_sha256: bundleSha,
      security_level: securityLevel,
      id: item.id,
      revisions: texts.length,
      input_sha256: sha256(joined),
      input_bytes: Buffer.byteLength(joined, 'utf8'),
    };

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
      }));
      // The page is wedged inside mermaid's synchronous layout; nothing short of a new process
      // gets it back.
      killBrowser(browser);
      browser = await newBrowser();
    };

    // Probe phase: one untimed render of the largest revision, so an item that cannot finish is
    // discovered in one render rather than `warmup + reps` of them.
    let probeMs = null;
    if (budgetMs) {
      try {
        const p = await evaluateInPage(PAGE_PROBE, { text: texts[texts.length - 1], tag }, budgetMs);
        if (p.exceptionDetails) throw new Error(p.exceptionDetails.text);
        probeMs = p.result.value.ms;
        const bad = validate(p.result.value.svg);
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
      const perSample = Math.max(1, probeMs * texts.length);
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

    const args = { texts, reps, warmup, nullReps, tag };
    let res;
    try {
      res = await evaluateInPage(PAGE_BENCH, args, budgetMs ? budgetMs - (Date.now() - t0) : null);
    } catch (e) {
      if (!item.dnf_allowed) throw new Error(`${item.id}: ${e.message}`);
      await dnf(
        'timed',
        e instanceof Deadline ? 'timeout' : 'failed',
        e instanceof Deadline ? `mermaid did not finish ${reps} sample(s), ${e.message}` : String(e.message),
      );
      continue;
    }

    if (res.exceptionDetails) throw new Error(`${item.id}: ${res.exceptionDetails.text}`);
    const out = res.result.value;
    // Every revision is validated, not just the last: a trace that silently degrades into mermaid's
    // error placeholder halfway through would otherwise look like a very fast render.
    const err =
      out.error ??
      (out.nullOutputValid ? null : 'A/A null control produced invalid SVG') ??
      out.svgs.map(validate).find(Boolean) ??
      (out.svgs.length === texts.length ? null : 'revision count mismatch');
    record.wall_s = (Date.now() - t0) / 1000;
    if (err) {
      // An in-page failure on a budgeted item is mermaid's own guardrail or an OOM at a size it
      // cannot serve -- a did-not-finish, not a broken harness.
      if (item.dnf_allowed) {
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
    const outputBytes = out.svgs.reduce((a, s) => a + s.length, 0);
    if (dumpSvgDir) writeFileSync(join(dumpSvgDir, `${item.id}.mermaid.svg`), out.svgs[out.svgs.length - 1]);
    console.log(JSON.stringify({
      ...record,
      status: 'ok',
      warmup,
      reps,
      null_reps: nullReps,
      budget_ms: budgetMs,
      probe_ms: probeMs === null ? null : Math.round(probeMs),
      render_ns: Object.fromEntries(
        Object.entries(ms)
          .filter(([k]) => !['cv_pct', 'mad_pct'].includes(k))
          .map(([k, v]) => [k, k === 'n' || v === null ? v : Math.round(v * 1e6)]),
      ),
      cv_pct: Number(ms.cv_pct.toFixed(2)),
      mad_pct: Number(ms.mad_pct.toFixed(2)),
      null_control: nullStats,
      output_bytes: outputBytes,
      output_sha256: sha256(out.svgs.join('')),
    }));
    log(
      `ok   ${item.id}  p50=${ms.p50.toFixed(1)}ms ` +
      `null=${nullStats.median === null ? 'missing' : `${nullStats.median.toFixed(6)} [${nullStats.ci95_lo.toFixed(6)},${nullStats.ci95_hi.toFixed(6)}]`} ` +
      `bytes=${outputBytes}`,
    );
  }
} finally {
  killBrowser(browser);
}

if (failed) { log('one or more comparator renders failed'); process.exit(2); }
