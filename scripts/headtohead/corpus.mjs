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
 * A software-architecture diagram: `groups` subgraphs of `perGroup` services, chained inside each
 * group and cross-linked between adjacent groups.
 *
 * This is the shape architecture diagrams actually have -- people draw them with `subgraph`, not as
 * a flat node list -- and it is a different layout problem from `flowchart`/`wide`: the cluster
 * boundaries constrain node placement and force the router around obstacles the flat generators
 * never produce. At thousands of nodes it is also the regime where mermaid-js stops finishing.
 */
function architecture(groups, perGroup) {
  const lines = ['flowchart TB'];
  for (let g = 0; g < groups; g++) {
    lines.push(`  subgraph G${g}[Group ${g}]`);
    for (let i = 0; i < perGroup; i++) lines.push(`    G${g}N${i}[Svc ${g}.${i}]`);
    for (let i = 0; i < perGroup - 1; i++) lines.push(`    G${g}N${i}-->G${g}N${i + 1}`);
    lines.push('  end');
  }
  for (let g = 0; g < groups - 1; g++) lines.push(`  G${g}N${perGroup - 1}-->G${g + 1}N0`);
  return lines.join('\n');
}

/**
 * A database-schema ER diagram: `entities` tables each carrying an attribute block, chained by
 * relationships.
 *
 * The pinned `er` generator emits relationships only. A real schema diagram is dominated by
 * attribute rows, which makes it text-measurement-bound rather than graph-bound -- a different cost
 * profile, and the one an ER user actually pays.
 */
function erSchema(entities, attrs) {
  const lines = ['erDiagram'];
  for (let i = 0; i < entities - 1; i++) lines.push(`  E${i} ||--o{ E${i + 1} : has`);
  for (let i = 0; i < entities; i++) {
    lines.push(`  E${i} {`);
    lines.push('    int id PK');
    for (let a = 0; a < attrs - 1; a++) lines.push(`    string field${a}`);
    lines.push('  }');
  }
  return lines.join('\n');
}

/**
 * A documentation build: every diagram one docs page -- or one CI job -- renders in a batch.
 *
 * Returns one document per diagram, so the existing trace machinery times the whole batch as a
 * unit, which is what a docs pipeline actually costs. The per-diagram number this yields is the one
 * a CI owner budgets against. Sizes vary per copy so the batch is not N renders of one cached shape.
 */
function docBuild(copies) {
  const out = [];
  for (let c = 0; c < copies; c++) {
    out.push(flowchart(12 + (c % 7)));
    out.push(sequence(6 + (c % 5)));
    out.push(classDiagram(8 + (c % 4)));
    out.push(stateDiagram(10 + (c % 6)));
    out.push(erDiagram(9 + (c % 3)));
  }
  return out;
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

// ---------------------------------------------------------------------------------------------
// PHASE 2 -- realistic end-to-end workloads.
//
// The generators above produce uniform synthetic input: every label is `Node 123`, every diagram in
// a batch is the same size, and the type mix is a round-robin. Real documentation is none of those,
// and the differences are not cosmetic:
//
//   - Real labels contain `&`, `<`, `>`, apostrophes and non-ASCII. BOTH engines carry escape paths
//     that a corpus of `Node 123` never exercises, so a synthetic batch measures a path the user
//     does not take. This is the integration cost a microbench structurally hides.
//   - Real diagram sizes are strongly right-skewed: mostly 4-12 nodes with a long tail.
//   - Real type mix is flowchart-dominated, not spread evenly across five types.
//
// Deterministic throughout -- a seeded PRNG, never Math.random -- so the corpus stays SHA-256
// pinnable and a generator edit still fails the drift check rather than moving a baseline quietly.

/** mulberry32: small, fast, deterministic. Seeded per document so every item is reproducible. */
function rng(seed) {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let x = Math.imul(a ^ (a >>> 15), 1 | a);
    x = (x + Math.imul(x ^ (x >>> 7), 61 | x)) ^ x;
    return ((x ^ (x >>> 14)) >>> 0) / 4294967296;
  };
}

// Label fragments of the kind that actually appear in architecture and process diagrams. The
// ampersand / apostrophe / angle-bracket / accented entries are deliberate and are ~25% of this pool:
// they are the escaping realism the synthetic corpus lacks.
const LABEL_WORDS = [
  'User', 'Client', 'API Gateway', 'Auth Service', 'Session Store', 'Database', 'Read Replica',
  'Cache', 'Message Queue', 'Worker', 'Scheduler', 'Load Balancer', 'CDN', 'Object Store',
  'Validate input', 'Normalize payload', 'Check permissions', 'Emit audit event', 'Persist record',
  'Rate limit (429)', 'Retry & backoff', 'Fan out', 'Aggregate results', 'Render response',
  'Parse <config>', 'Diff & merge', 'Sign & upload', 'Rollback on failure',
  'Café latency', 'naïve retry', 'Ingestion', 'Überprüfung', 'Résumé job', "User's session",
];

const pick = (r, xs) => xs[Math.floor(r() * xs.length) % xs.length];

/** Right-skewed size: mostly small, occasional large, which is how docs corpora are shaped. */
function skewedSize(r, min, max) {
  const u = Math.max(1e-9, r());
  return min + Math.floor((max - min) * u ** 2.4);
}

function realisticLabel(r, i) {
  const base = pick(r, LABEL_WORDS);
  // ~30% carry a qualifier, which is how real diagrams disambiguate repeated nodes.
  return r() < 0.3 ? `${base} ${i}` : base;
}

function docFlowchart(r, n) {
  const lines = [`flowchart ${pick(r, ['LR', 'TD', 'TB'])}`];
  for (let i = 0; i < n; i++) lines.push(`  N${i}["${realisticLabel(r, i)}"]`);
  for (let i = 0; i < n - 1; i++) {
    // Real flowcharts branch; they are not one chain.
    const from = r() < 0.75 ? i : Math.floor(r() * (i + 1));
    if (r() < 0.22) lines.push(`  N${from}-->|"${pick(r, LABEL_WORDS)}"|N${i + 1}`);
    else lines.push(`  N${from}-->N${i + 1}`);
  }
  return lines.join('\n');
}

function docSequence(r, n) {
  const lines = ['sequenceDiagram'];
  const p = Math.max(2, Math.min(6, Math.ceil(n / 3)));
  for (let i = 0; i < p; i++) lines.push(`  participant P${i} as ${pick(r, LABEL_WORDS)}`);
  for (let i = 0; i < n; i++) {
    const a = i % p;
    const b = (i + 1 + Math.floor(r() * (p - 1))) % p;
    lines.push(`  P${a}${r() < 0.25 ? '-->>' : '->>'}P${b}: ${realisticLabel(r, i)}`);
  }
  return lines.join('\n');
}

function docClass(r, n) {
  const lines = ['classDiagram'];
  for (let i = 0; i < n; i++) {
    lines.push(`  class C${i} {`);
    const fields = 1 + Math.floor(r() * 4);
    for (let f = 0; f < fields; f++) {
      lines.push(`    +${pick(r, ['int', 'string', 'bool', 'float'])} field${f}`);
    }
    lines.push(`    +method${i}() ${pick(r, ['bool', 'void', 'string'])}`);
    lines.push('  }');
  }
  for (let i = 0; i < n - 1; i++) {
    lines.push(`  C${i} ${pick(r, ['<|--', '*--', 'o--', '-->'])} C${i + 1}`);
  }
  return lines.join('\n');
}

function docState(r, n) {
  const lines = ['stateDiagram-v2', '  [*] --> S0'];
  for (let i = 0; i < n - 1; i++) lines.push(`  S${i} --> S${i + 1}: ${pick(r, LABEL_WORDS)}`);
  lines.push(`  S${n - 1} --> [*]`);
  return lines.join('\n');
}

function docEr(r, n) {
  const lines = ['erDiagram'];
  for (let i = 0; i < n - 1; i++) {
    lines.push(`  E${i} ${pick(r, ['||--o{', '||--||', '}o--o{'])} E${i + 1} : ${pick(r, ['has', 'owns', 'refers'])}`);
  }
  for (let i = 0; i < n; i++) {
    lines.push(`  E${i} {`);
    lines.push('    int id PK');
    const attrs = 1 + Math.floor(r() * 5);
    for (let a = 0; a < attrs; a++) lines.push(`    string field${a}`);
    lines.push('  }');
  }
  return lines.join('\n');
}

/**
 * A documentation site build: every mermaid block across a docs repo, rendered in one job.
 *
 * This is a job a user actually runs -- `mkdocs build`, a Docusaurus production build, a CI docs
 * check. Flowchart-dominated the way real documentation is, sizes right-skewed, labels carrying the
 * characters that force escaping.
 */
function docsSite(count, seed) {
  const out = [];
  for (let d = 0; d < count; d++) {
    const r = rng(seed + d * 7919);
    const roll = r();
    if (roll < 0.58) out.push(docFlowchart(r, skewedSize(r, 4, 60)));
    else if (roll < 0.74) out.push(docSequence(r, skewedSize(r, 3, 30)));
    else if (roll < 0.83) out.push(docClass(r, skewedSize(r, 3, 20)));
    else if (roll < 0.91) out.push(docState(r, skewedSize(r, 3, 24)));
    else out.push(docEr(r, skewedSize(r, 3, 18)));
  }
  return out;
}

/**
 * An equivalence-decidable CI documentation build.
 *
 * This keeps the realistic docs generator's right-skewed sizes, labels, and occasional edge labels,
 * but restricts the job to flat left-to-right process flowcharts. The shared SVG checker can prove
 * every rendered path structurally against input-derived topology. Branching flowcharts are
 * excluded because mermaid-js 11.15.0 places some path endpoints equidistant from adjacent node
 * anchors, making geometry alone honestly undecidable; state is excluded
 * because the two renderers currently disagree on transition labels containing punctuation. Class
 * remains excluded while bd-4isi is open (member rows are dropped), and sequence/ER do not expose
 * enough common geometry to prove that no unlabeled edge disappeared.
 */
function equivalenceDecidableDocs(count, seed) {
  const out = [];
  for (let d = 0; d < count; d++) {
    const r = rng(seed + d * 7919);
    const n = skewedSize(r, 4, 60);
    const lines = ['flowchart LR'];
    for (let i = 0; i < n; i++) lines.push(`  N${i}["${realisticLabel(r, i)}"]`);
    for (let i = 0; i < n - 1; i++) {
      if (r() < 0.22) lines.push(`  N${i}-->|"${pick(r, LABEL_WORDS)}"|N${i + 1}`);
      else lines.push(`  N${i}-->N${i + 1}`);
    }
    out.push(lines.join('\n'));
  }
  return out;
}

/**
 * A related-diagram build whose pages embed one byte-identical platform subgraph.
 *
 * Documentation sets routinely repeat the same deployment, authentication, or ingestion block
 * and attach a page-specific tail. Mermaid-js reparses that shared block for every diagram. This
 * corpus keeps every diagram distinct while making the repeated work explicit and pinnable.
 */
function sharedSubgraphDocs(count, sharedNodes, seed, divergentBlocks = false) {
  const sharedRandom = rng(seed);
  const prefix = ['flowchart LR', '  subgraph Shared["Shared ingestion platform"]'];
  for (let i = 0; i < sharedNodes; i++) {
    prefix.push(`    S${i}["${realisticLabel(sharedRandom, i)}"]`);
  }
  for (let i = 0; i < sharedNodes - 1; i++) prefix.push(`    S${i}-->S${i + 1}`);
  prefix.push('  end');
  const shared = prefix.join('\n');

  const out = [];
  for (let d = 0; d < count; d++) {
    const documentRandom = rng(seed + (d + 1) * 104729);
    const uniqueNodes = 4 + (d % 9);
    const suffix = [];
    if (divergentBlocks) suffix.push(`  subgraph Tenant${d}["Tenant ${d}"]`);
    for (let i = 0; i < uniqueNodes; i++) {
      const indent = divergentBlocks ? '    ' : '  ';
      suffix.push(`${indent}D${i}["${realisticLabel(documentRandom, d * 16 + i)}"]`);
    }
    if (!divergentBlocks) suffix.push(`  S${sharedNodes - 1}-->D0`);
    for (let i = 0; i < uniqueNodes - 1; i++) {
      const indent = divergentBlocks ? '    ' : '  ';
      suffix.push(`${indent}D${i}-->D${i + 1}`);
    }
    if (divergentBlocks) {
      suffix.push('  end');
      suffix.push(`  S${sharedNodes - 1}-->D0`);
    }
    out.push([shared, ...suffix].join('\n'));
  }
  return out;
}

/**
 * A live editing session as it actually happens: a user TYPES a label one character at a time.
 *
 * `edit_trace` models structural edits -- append a node, add an edge. Real editing is dominated by
 * keystrokes inside a label, which re-render a document nearly identical to the previous one. That
 * is a different workload: many tiny deltas at high frequency.
 */
function typingTrace(baseNodes, phrase, seed) {
  const r = rng(seed);
  const nodes = [];
  for (let i = 0; i < baseNodes; i++) nodes.push(`  N${i}["${realisticLabel(r, i)}"]`);
  const edges = [];
  for (let i = 0; i < baseNodes - 1; i++) edges.push(`  N${i}-->N${i + 1}`);
  const target = Math.floor(baseNodes / 2);
  const texts = [];
  for (let k = 1; k <= phrase.length; k++) {
    nodes[target] = `  N${target}["${phrase.slice(0, k)}"]`;
    texts.push(['flowchart LR', ...nodes, ...edges].join('\n'));
  }
  return texts;
}

const DOMAIN_NAMES = [
  'Identity & Access', 'Customer Experience', 'Billing', 'Data Platform', 'Observability',
  'Fulfillment', 'Search', 'Messaging', 'Developer Platform', 'Risk & Compliance',
  'Analytics', 'Content Delivery',
];

const EDGE_LABELS = ['HTTPS', 'gRPC', 'events', 'reads', 'writes', 'publishes', 'subscribes'];

/**
 * One monorepo service map exported for an architecture review.
 *
 * Real service graphs are hub-heavy rather than regular: gateways, event buses, identity and data
 * services collect much more degree than leaf workers. The `r() ** 2.8` endpoint choice produces
 * that power-law-like skew while every service still has a route into the graph. Domain assignment
 * is also skewed, so the subgraphs are not uniformly sized.
 */
function monorepoArchitecture(serviceCount, domainCount, seed) {
  const r = rng(seed);
  const domains = Array.from({ length: domainCount }, () => []);
  for (let i = 0; i < serviceCount; i++) {
    const domain = i < domainCount
      ? i
      : Math.min(domainCount - 1, Math.floor(domainCount * r() ** 1.7));
    domains[domain].push(i);
  }

  const lines = ['flowchart LR'];
  for (let d = 0; d < domains.length; d++) {
    lines.push(`  subgraph D${d}["${DOMAIN_NAMES[d % DOMAIN_NAMES.length]}"]`);
    for (const i of domains[d]) lines.push(`    S${i}["${realisticLabel(r, i)}"]`);
    lines.push('  end');
  }

  // Every non-root service depends on one earlier service, biased strongly toward a small set of
  // hubs. Additional cross-domain links model shared platforms and event streams.
  for (let i = 1; i < serviceCount; i++) {
    const hub = Math.min(i - 1, Math.floor(i * r() ** 2.8));
    lines.push(`  S${hub} -->|"${pick(r, EDGE_LABELS)}"| S${i}`);
  }
  for (let i = 0; i < Math.floor(serviceCount * 0.55); i++) {
    const from = Math.floor(r() * serviceCount);
    const to = Math.floor(serviceCount * r() ** 2.8);
    if (from !== to) lines.push(`  S${from} -.->|"${pick(r, EDGE_LABELS)}"| S${to}`);
  }
  return [lines.join('\n')];
}

const FIELD_NAMES = [
  'external_id', 'created_at', 'updated_at', 'display_name', 'status', 'owner_id',
  'region', 'version', 'payload', 'checksum', 'expires_at', 'retry_count',
];

function catalogSchema(r, schemaIndex, entityCount) {
  const lines = ['erDiagram'];
  const entity = (i) => `S${schemaIndex}_E${i}`;
  for (let i = 0; i < entityCount; i++) {
    lines.push(`  ${entity(i)} {`);
    lines.push('    uuid id PK');
    const fields = 1 + Math.floor(r() ** 1.8 * 8);
    for (let f = 0; f < fields; f++) {
      lines.push(`    ${pick(r, ['string', 'int', 'boolean', 'timestamp', 'json'])} ${pick(r, FIELD_NAMES)}_${f}`);
    }
    lines.push('  }');
  }
  for (let i = 1; i < entityCount; i++) {
    const parent = Math.min(i - 1, Math.floor(i * r() ** 2.5));
    lines.push(`  ${entity(parent)} ||--o{ ${entity(i)} : ${pick(r, ['contains', 'owns', 'references'])}`);
  }
  return lines.join('\n');
}

/**
 * A database documentation publish: render every bounded-context schema in one catalog job.
 *
 * Schema sizes are right-skewed and relationship endpoints prefer hubs, matching the common shape
 * of identity/account/order tables with many small peripheral tables. Attribute counts and types
 * vary per entity rather than repeating one synthetic block.
 */
function schemaCatalog(schemaCount, minEntities, maxEntities, seed) {
  const out = [];
  for (let schema = 0; schema < schemaCount; schema++) {
    const r = rng(seed + schema * 104729);
    out.push(catalogSchema(r, schema, skewedSize(r, minEntities, maxEntities)));
  }
  return out;
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
  docs_site: (p) => docsSite(p.count, p.seed),
  equivalence_decidable_docs: (p) => equivalenceDecidableDocs(p.count, p.seed),
  shared_subgraph_docs: (p) => sharedSubgraphDocs(p.count, p.shared_nodes, p.seed),
  shared_subgraph_divergent_docs: (p) =>
    sharedSubgraphDocs(p.count, p.shared_nodes, p.seed, true),
  typing_trace: (p) => typingTrace(p.nodes, p.phrase, p.seed),
  monorepo_architecture: (p) => monorepoArchitecture(p.services, p.domains, p.seed),
  schema_catalog: (p) => schemaCatalog(p.schemas, p.min_entities, p.max_entities, p.seed),
  architecture: (p) => [architecture(p.groups, p.per_group)],
  er_schema: (p) => [erSchema(p.entities, p.attrs)],
  doc_build: (p) => docBuild(p.copies),
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

  // ---------------------------------------------------------------------------------------------
  // Workload classes the 13-item baseline never covered (bd-1buv, cc/STRUCTURAL lane).
  //
  // The items above top out at 500-node flowcharts and 512-node layered DAGs. That is not where
  // mermaid is used and it is not where mermaid hurts. The three classes below are:
  //
  //   XL         diagrams of thousands of nodes -- architecture maps and database schemas. This is
  //              the regime where mermaid-js may not finish at all, which is a stronger result than
  //              any ratio; see `dnf_allowed` and `js_budget_ms` below.
  //   EDIT       a real editing session, hundreds of revisions rather than 21.
  //   DOC_BUILD  every diagram on a docs page / in a CI job, timed as one batch.
  //
  // `js_budget_ms` is a wall budget for the mermaid arm of that item. Exceeding it is recorded as
  // `status: "dnf"` with the budget attached -- an honest "did not finish", never a silent win and
  // never a hard run failure. `dnf_allowed` items also downgrade a comparator *error* (mermaid's own
  // size guardrails, an OOM) to a DNF. The 13 pinned items above set neither field, so a failure
  // there stays a hard failure exactly as before.
  //
  // `warmup_js: 0` on the heaviest items is deliberate: a warmup render doubles an item that may
  // already take minutes, and these samples are long enough that v8 is warm within the first few
  // renders of the timed sample itself.
  { id: 'flowchart_xl_2000',    gen: 'flowchart',    params: { n: 2000 },                    class: 'single',     reps_js: 3, warmup_js: 1, reps_rs: 20, warmup_rs: 3, js_budget_ms: 300_000, dnf_allowed: true },
  { id: 'flowchart_xl_5000',    gen: 'flowchart',    params: { n: 5000 },                    class: 'single',     reps_js: 2, warmup_js: 0, reps_rs: 12, warmup_rs: 2, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'arch_100x50',          gen: 'architecture', params: { groups: 100, per_group: 50 }, class: 'single',     reps_js: 2, warmup_js: 0, reps_rs: 12, warmup_rs: 2, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'er_schema_1000x6',     gen: 'er_schema',    params: { entities: 1000, attrs: 6 },   class: 'single',     reps_js: 3, warmup_js: 1, reps_rs: 20, warmup_rs: 3, js_budget_ms: 900_000, dnf_allowed: true },
  // The top of the range the campaign named (5-10k nodes). mermaid-js already fails at 2,000, so
  // these exist to characterise *our* scaling where the comparator has no curve left to measure --
  // and because every ledger self-time percentage is a claim about a workload, so the frontier at
  // 10k is a different question from the frontier at 500.
  // `reps_rs` is 40, not the 8 the other XL items originally used. Under the v2 harness that means
  // 40 paired A/A and A/B rounds (well above the fail-closed minimum of 9), which narrows the
  // bootstrap median CI for these one-sample-per-arm items. Historical MAD varied from 0.4% to
  // 16.4% across identical inputs; that dispersion is retained as provenance, never as a gate.
  // The workload, hash, and budget remain untouched, and 40 rounds costs roughly 0.4 s per arm.
  { id: 'arch_200x50',          gen: 'architecture', params: { groups: 200, per_group: 50 }, class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'er_schema_2500x8',     gen: 'er_schema',    params: { entities: 2500, attrs: 8 },   class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  // ER endpoints at the campaign's explicitly named 5k-10k *node* range. The 2,500-entity row
  // establishes the comparator's failure boundary; these two remain admitted-but-unmeasured until
  // Lane M grants a quiet window. Their purpose is to expose frankenmermaid's scaling after the
  // comparator curve has ended, without turning a construction-only Lane L turn into a timing claim.
  { id: 'er_schema_5000x8',     gen: 'er_schema',    params: { entities: 5000, attrs: 8 },   class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'er_schema_10000x8',    gen: 'er_schema',    params: { entities: 10000, attrs: 8 },  class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 2400_000, dnf_allowed: true },
  // 201 successive documents: a real editing session, not a 21-keystroke sketch.
  { id: 'edit_trace_200x200',   gen: 'edit_trace',   params: { n: 200, revisions: 200 },     class: 'edit_trace', reps_js: 2, warmup_js: 0, reps_rs: 10, warmup_rs: 2, js_budget_ms: 6000_000, dnf_allowed: true },
  // A sustained live-preview session: 1,001 successive full documents. This is long enough to
  // surface cumulative allocator/cache behavior that a 21- or 201-document trace cannot expose.
  { id: 'edit_trace_500x1000',  gen: 'edit_trace',   params: { n: 500, revisions: 1000 },    class: 'edit_trace', reps_js: 1, warmup_js: 0, reps_rs: 12, warmup_rs: 1, js_budget_ms: 600_000, dnf_allowed: true },
  // 40 diagrams across five types -- one docs page, or one CI batch job.
  { id: 'doc_build_40',         gen: 'doc_build',    params: { copies: 8 },                  class: 'doc_build',  reps_js: 3, warmup_js: 1, reps_rs: 30, warmup_rs: 3, js_budget_ms: 300_000, dnf_allowed: true },
  // A repository-scale CI render: 500 diagrams across the same five syntax families. It measures
  // one whole job, not a per-diagram microbenchmark, and remains unmeasured until a worker window.
  { id: 'ci_batch_500',         gen: 'doc_build',    params: { copies: 100 },                class: 'doc_build',  reps_js: 1, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 1500_000, dnf_allowed: true },

  // ---------------------------------------------------------------------------------------------
  // XL tier for the SEVEN syntax families that never had one.
  //
  // The XL block above reaches thousands of nodes through exactly five generators: `flowchart`,
  // `architecture`, `er_schema`, `edit_trace` and `doc_build`. The other seven generators in this
  // file stop at the pinned baseline sizes -- 20 participants, 40 states, 40 ER entities, 50
  // classes, 100 cyclic nodes, 200 dense-DAG nodes, 512 wide-layout nodes -- so seven of the twelve
  // syntax families this corpus can express were never taken anywhere near the regime where the
  // comparator stops working. Every ratio measured on them describes a size nobody's CI job hits.
  //
  // The claim these items exist to decide is a COMPLETION claim, not a timing one: at this size,
  // does mermaid-js produce a diagram at all? `dnf_allowed` is what makes that answerable -- it
  // downgrades mermaid's own failure (its size guardrails, a stack overflow, an OOM) from a hard
  // run failure to a recorded `status: "dnf"` with `kind: "failed"`, which run.mjs keeps out of the
  // ratio aggregate and out of the cross-engine equivalence gate, because an engine that rendered
  // nothing cannot be compared against and cannot bound a ratio.
  //
  // Sizes are chosen to clear 2,000 nodes in every family, since that is where the comparator has
  // been observed to stop, and to keep each family's own shape: `wide` stays layered, `cyclic`
  // stays SCC-heavy, `dense_dag` keeps fanout 4, `class` keeps its members, `er` keeps its chain.
  // Nothing here is a new generator or a new shape -- same runtime-selected generators, one tier up.
  { id: 'wide_xl_50x50',        gen: 'wide',         params: { layers: 50, width: 50 },      class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'cyclic_scc_xl_2500',   gen: 'cyclic',       params: { n: 2500, ring: 5 },           class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'dense_dag_xl_2000',    gen: 'dense_dag',    params: { n: 2000, fanout: 4 },         class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'sequence_xl_2000',     gen: 'sequence',     params: { n: 2000 },                    class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'class_xl_2000',        gen: 'class',        params: { n: 2000 },                    class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'state_xl_2000',        gen: 'state',        params: { n: 2000 },                    class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },
  { id: 'er_xl_2000',           gen: 'er',           params: { n: 2000 },                    class: 'single',     reps_js: 1, warmup_js: 0, reps_rs: 40, warmup_rs: 3, js_budget_ms: 600_000, dnf_allowed: true },

  // ---------------------------------------------------------------------------------------------
  // PHASE 2 — whole jobs a real user runs, on realistic data. See the generator notes above: these
  // differ from `doc_build`/`edit_trace` in DISTRIBUTION, not just size — flowchart-dominated type
  // mix, right-skewed diagram sizes, and labels that actually contain `&`, `<`, `>`, apostrophes and
  // accented characters, which is the escaping cost a synthetic corpus never charges either engine.
  { id: 'docs_site_50',         gen: 'docs_site',    params: { count: 50, seed: 20260728 },  class: 'doc_build',  reps_js: 3, warmup_js: 1, reps_rs: 30, warmup_rs: 3, js_budget_ms: 900_000,  dnf_allowed: true },
  { id: 'docs_site_200',        gen: 'docs_site',    params: { count: 200, seed: 20260728 }, class: 'doc_build',  reps_js: 2, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 1500_000, dnf_allowed: true },
  // CI render-farm jobs over the same realistic, right-skewed distribution. These are deliberately
  // whole 2,000- and 5,000-diagram invocations rather than a small job multiplied after timing:
  // corpus traversal, output ownership, allocator pressure, and persistent-pool scheduling remain
  // inside the boundary a user pays. They are pinned now and require exclusive-trj certification.
  { id: 'ci_docs_2000',         gen: 'docs_site',    params: { count: 2000, seed: 20260729 }, class: 'doc_build', reps_js: 1, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 3600_000, dnf_allowed: true },
  { id: 'ci_docs_5000',         gen: 'docs_site',    params: { count: 5000, seed: 20260729 }, class: 'doc_build', reps_js: 1, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 9000_000, dnf_allowed: true },
  // 512 independent diagrams in one job, not 512 divided timings. Every diagram belongs to a
  // family whose SVG equivalence can prove rendered edge topology against input ground truth.
  // Nine live incumbent samples make the cross-runtime bootstrap median-ratio CI decidable.
  { id: 'ci_equiv_512',         gen: 'equivalence_decidable_docs', params: { count: 512, seed: 20260730 }, class: 'doc_build', reps_js: 9, null_reps_js: 20, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 7200_000, dnf_allowed: true, effect_ci_required: true },
  // 384 distinct docs pages share one complete 48-node platform subgraph and attach independent
  // tails. The job exposes cross-diagram parser reuse while retaining one full render per page.
  { id: 'ci_shared_subgraph_384', gen: 'shared_subgraph_docs', params: { count: 384, shared_nodes: 48, seed: 20260731 }, class: 'doc_build', reps_js: 9, null_reps_js: 20, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 7200_000, dnf_allowed: true, effect_ci_required: true },
  // The same platform block followed by a distinct complete tenant subgraph. The largest leading
  // prefix differs per document, so only the linear common-boundary planner can reuse the platform.
  { id: 'ci_shared_subgraph_divergent_64', gen: 'shared_subgraph_divergent_docs', params: { count: 64, shared_nodes: 48, seed: 20260731 }, class: 'doc_build', reps_js: 9, null_reps_js: 20, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 900_000, dnf_allowed: true, effect_ci_required: true },
  // 60 keystrokes inside one label: the re-render frequency a live preview actually generates.
  { id: 'typing_trace_60',      gen: 'typing_trace', params: { nodes: 40, phrase: 'Aggregate results from the upstream ingestion workers safely', seed: 20260728 }, class: 'edit_trace', reps_js: 2, warmup_js: 1, reps_rs: 20, warmup_rs: 2, js_budget_ms: 900_000, dnf_allowed: true },
  // One architecture-review export at two monorepo sizes. Degree and domain sizes are deliberately
  // skewed; these are service maps, not regular layered grids.
  { id: 'monorepo_arch_120',    gen: 'monorepo_architecture', params: { services: 120, domains: 8, seed: 20260728 }, class: 'single', reps_js: 3, warmup_js: 1, reps_rs: 30, warmup_rs: 3, js_budget_ms: 900_000, dnf_allowed: true },
  { id: 'monorepo_arch_300',    gen: 'monorepo_architecture', params: { services: 300, domains: 12, seed: 20260728 }, class: 'single', reps_js: 2, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 1200_000, dnf_allowed: true },
  // Twenty-five bounded-context ER diagrams rendered as one database-catalog publish.
  { id: 'schema_catalog_25',    gen: 'schema_catalog', params: { schemas: 25, min_entities: 8, max_entities: 80, seed: 20260728 }, class: 'doc_build', reps_js: 2, warmup_js: 0, reps_rs: 20, warmup_rs: 2, js_budget_ms: 1500_000, dnf_allowed: true },
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

/** Keep the structural-capability tier complete: every formerly sub-XL family stays represented. */
export function assertXlCapabilityFixtures() {
  const expected = new Set([
    'wide_xl_50x50', 'cyclic_scc_xl_2500', 'dense_dag_xl_2000', 'sequence_xl_2000',
    'class_xl_2000', 'state_xl_2000', 'er_xl_2000',
  ]);
  const actual = new Set(CORPUS.map((item) => item.id));
  if ([...expected].some((id) => !actual.has(id))) {
    throw new Error(`XL capability fixtures changed: expected ${[...expected].join(', ')}`);
  }
  for (const item of CORPUS.filter((candidate) => expected.has(candidate.id))) {
    if (item.dnf_allowed !== true || item.js_budget_ms < 600_000) {
      throw new Error(`XL capability fixture ${item.id} must preserve DNF admission and its 600s budget`);
    }
  }
}
