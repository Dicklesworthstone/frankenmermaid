// Deterministic corpus generators for the frankenmermaid <-> mermaid-js head-to-head.
//
// The corpus is *generated*, not committed: every generator here is a pure function of its
// parameters, and `pins.json` records the SHA-256 of each generated input. `run.mjs` verifies
// those hashes on every run, so a silent change to a generator fails the harness instead of
// quietly moving the baseline.
//
// `flowchart` and `wide` reproduce `crates/fm-cli/benches/pipeline_bench.rs`'s `gen_flowchart`
// and `gen_wide` byte for byte, so harness numbers stay comparable with every criterion
// number recorded in evidence/ledger/mermaid-js-head-to-head.toml.

import { createHash } from 'node:crypto';

function flowchart(n) {
  const lines = ['flowchart LR'];
  for (let i = 0; i < n; i++) lines.push(`  N${i}[Node ${i}]`);
  for (let i = 0; i < n - 1; i++) lines.push(`  N${i}-->N${i + 1}`);
  return lines.join('\n');
}

// Mirrors gen_wide(layers, width) in pipeline_bench.rs: layers*width nodes, 2*width*(layers-1) edges.
function wide(layers, width) {
  const lines = ['flowchart TD'];
  for (let layer = 0; layer < layers; layer++) {
    for (let w = 0; w < width; w++) lines.push(`  N${layer}_${w}[L${layer} W${w}]`);
  }
  for (let layer = 0; layer < layers - 1; layer++) {
    for (let w = 0; w < width; w++) {
      lines.push(`  N${layer}_${w}-->N${layer + 1}_${w}`);
      lines.push(`  N${layer}_${w}-->N${layer + 1}_${(w + 1) % width}`);
    }
  }
  return lines.join('\n');
}

// Strongly-connected-component-heavy digraph: rings of `ring` nodes, each ring fully cyclic,
// chained forward to the next ring. Exercises cycle removal + crossing minimization.
function cyclic(n, ring = 5) {
  const lines = ['flowchart TD'];
  for (let i = 0; i < n; i++) lines.push(`  C${i}[C${i}]`);
  for (let i = 0; i < n; i++) {
    const ringStart = Math.floor(i / ring) * ring;
    const next = ringStart + ((i - ringStart + 1) % ring);
    if (next < n) lines.push(`  C${i}-->C${next}`);
    if (i + ring < n) lines.push(`  C${i}-->C${i + ring}`);
  }
  return lines.join('\n');
}

// Dense DAG: every node points at the next `fanout` nodes. No cycles, high edge density.
function denseDag(n, fanout = 4) {
  const lines = ['flowchart LR'];
  for (let i = 0; i < n; i++) lines.push(`  D${i}[D${i}]`);
  for (let i = 0; i < n; i++) {
    for (let k = 1; k <= fanout; k++) if (i + k < n) lines.push(`  D${i}-->D${i + k}`);
  }
  return lines.join('\n');
}

function sequence(n) {
  const lines = ['sequenceDiagram'];
  for (let i = 0; i < n; i++) lines.push(`  participant P${i}`);
  for (let i = 0; i < n - 1; i++) {
    lines.push(`  P${i}->>P${i + 1}: request ${i}`);
    lines.push(`  P${i + 1}-->>P${i}: response ${i}`);
  }
  return lines.join('\n');
}

function classDiagram(n) {
  const lines = ['classDiagram'];
  for (let i = 0; i < n; i++) {
    lines.push(`  class C${i} {`);
    lines.push(`    +int field${i}`);
    lines.push(`    +method${i}() bool`);
    lines.push('  }');
  }
  for (let i = 0; i < n - 1; i++) lines.push(`  C${i} <|-- C${i + 1}`);
  return lines.join('\n');
}

function stateDiagram(n) {
  const lines = ['stateDiagram-v2'];
  lines.push('  [*] --> S0');
  for (let i = 0; i < n - 1; i++) lines.push(`  S${i} --> S${i + 1}: event${i}`);
  lines.push(`  S${n - 1} --> [*]`);
  return lines.join('\n');
}

function erDiagram(n) {
  const lines = ['erDiagram'];
  for (let i = 0; i < n - 1; i++) lines.push(`  E${i} ||--o{ E${i + 1} : has`);
  return lines.join('\n');
}

/**
 * An editing session: the successive full documents a live preview would re-render as a user types.
 * This is the workload a mermaid user actually generates -- an editor calls `mermaid.render()` on
 * every keystroke, because mermaid has no incremental path. Returns `revisions + 1` documents.
 *
 * The edits cycle through the three things people actually do: append a node and wire it up, rename
 * a label, and add an edge between existing nodes.
 */
function editTrace(n, revisions) {
  const nodes = [];
  const edges = [];
  for (let i = 0; i < n; i++) nodes.push(`  N${i}[Node ${i}]`);
  for (let i = 0; i < n - 1; i++) edges.push(`  N${i}-->N${i + 1}`);
  const document = () => ['flowchart LR', ...nodes, ...edges].join('\n');

  const texts = [document()];
  for (let r = 0; r < revisions; r++) {
    switch (r % 3) {
      case 0: {
        // Next free index -- only every third revision appends, so `n + r` would skip ids.
        const i = nodes.length;
        nodes.push(`  N${i}[Node ${i}]`);
        edges.push(`  N${i - 1}-->N${i}`);
        break;
      }
      case 1: {
        const i = r % nodes.length;
        nodes[i] = `  N${i}[Renamed ${r}]`;
        break;
      }
      default: {
        const a = r % n;
        const b = (r * 7 + 3) % n;
        if (a !== b) edges.push(`  N${a}-->N${b}`);
      }
    }
    texts.push(document());
  }
  return texts;
}

// Every generator returns an array of documents. A single-shot item is a one-revision trace, which
// keeps one code path in both engines -- and keeps single-item hashes identical to before traces
// existed, since joining a one-element array yields the element itself.
const GENERATORS = {
  flowchart: (p) => [flowchart(p.n)],
  wide: (p) => [wide(p.layers, p.width)],
  cyclic: (p) => [cyclic(p.n, p.ring)],
  dense_dag: (p) => [denseDag(p.n, p.fanout)],
  sequence: (p) => [sequence(p.n)],
  class: (p) => [classDiagram(p.n)],
  state: (p) => [stateDiagram(p.n)],
  er: (p) => [erDiagram(p.n)],
  edit_trace: (p) => editTrace(p.n, p.revisions),
};

/** Separator used to hash a multi-revision trace as one input. Must match `headtohead.rs`. */
export const REVISION_SEP = '\n%%--revision--%%\n';

// The fixed corpus. `reps_*` are per-engine iteration counts: mermaid is ~3 orders of
// magnitude slower, so it gets fewer reps on the heavy items to keep a run under ~2 minutes.
// `warmup_*` iterations are executed and discarded before timing starts.
export const CORPUS = [
  { id: 'flowchart_small_10',   gen: 'flowchart', params: { n: 10 },                 reps_js: 20, warmup_js: 3, reps_rs: 200, warmup_rs: 20 },
  { id: 'flowchart_medium_100', gen: 'flowchart', params: { n: 100 },                reps_js: 15, warmup_js: 3, reps_rs: 100, warmup_rs: 10 },
  { id: 'flowchart_large_500',  gen: 'flowchart', params: { n: 500 },                reps_js: 7,  warmup_js: 2, reps_rs: 50,  warmup_rs: 5 },
  { id: 'wide_8x16',            gen: 'wide',      params: { layers: 8, width: 16 },  reps_js: 12, warmup_js: 2, reps_rs: 80,  warmup_rs: 8 },
  { id: 'wide_12x24',           gen: 'wide',      params: { layers: 12, width: 24 }, reps_js: 7,  warmup_js: 2, reps_rs: 50,  warmup_rs: 5 },
  { id: 'wide_16x32',           gen: 'wide',      params: { layers: 16, width: 32 }, reps_js: 5,  warmup_js: 1, reps_rs: 30,  warmup_rs: 3 },
  { id: 'dense_dag_200',        gen: 'dense_dag', params: { n: 200, fanout: 4 },     reps_js: 7,  warmup_js: 2, reps_rs: 50,  warmup_rs: 5 },
  { id: 'cyclic_scc_100',       gen: 'cyclic',    params: { n: 100, ring: 5 },       reps_js: 12, warmup_js: 2, reps_rs: 80,  warmup_rs: 8 },
  { id: 'sequence_20',          gen: 'sequence',  params: { n: 20 },                 reps_js: 15, warmup_js: 3, reps_rs: 100, warmup_rs: 10 },
  { id: 'class_50',             gen: 'class',     params: { n: 50 },                 reps_js: 15, warmup_js: 3, reps_rs: 100, warmup_rs: 10 },
  { id: 'state_40',             gen: 'state',     params: { n: 40 },                 reps_js: 15, warmup_js: 3, reps_rs: 100, warmup_rs: 10 },
  { id: 'er_40',                gen: 'er',        params: { n: 40 },                 reps_js: 15, warmup_js: 3, reps_rs: 100, warmup_rs: 10 },
  // A live-preview editing session: 21 successive full documents. One "iteration" renders all 21,
  // which is what an editor does as the user types -- mermaid has no incremental path.
  { id: 'edit_trace_60x20',     gen: 'edit_trace', params: { n: 60, revisions: 20 }, reps_js: 3,  warmup_js: 1, reps_rs: 30,  warmup_rs: 3 },
];

export function sha256(text) {
  return createHash('sha256').update(text, 'utf8').digest('hex');
}

/** All documents for a corpus item, in order. Single-shot items yield a one-element array. */
export function generate(item) {
  const gen = GENERATORS[item.gen];
  if (!gen) throw new Error(`unknown generator: ${item.gen}`);
  return gen(item.params);
}

/** Generate every corpus input and return `{id -> {texts, sha256, bytes}}`. */
export function generateAll() {
  const out = new Map();
  for (const item of CORPUS) {
    const texts = generate(item);
    const joined = texts.join(REVISION_SEP);
    out.set(item.id, { texts, sha256: sha256(joined), bytes: Buffer.byteLength(joined, 'utf8') });
  }
  return out;
}
