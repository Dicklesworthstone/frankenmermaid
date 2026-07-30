/**
 * Cross-engine rendered-output equivalence for the head-to-head harness (`bd-evx6`).
 *
 * The harness already proves (a) both engines consumed byte-identical input, (b) each engine is
 * self-deterministic, and (c) the pooled Rust arms are byte-identical to the scalar arm. None of
 * that answers the only question that makes a speedup meaningful: **did we render the same
 * diagram?** A renderer that silently drops an edge or mislays a subgraph is faster and wrong.
 *
 * ## What equivalence means here, precisely
 *
 * It is *not* byte equality and it cannot be. The two engines emit deliberately different SVG:
 * mermaid carries labels in `<foreignObject><div><span>` HTML, we emit `<text>`; its classes are
 * `node default`, ours are `fm-node fm-node-shape-rect`; its geometry comes from dagre, ours from
 * our own layout. Pixel equality is likewise unavailable — the two use different fonts, paddings
 * and stroke widths, so a rasterized perceptual diff would report a large distance for two
 * perfectly correct renders. It would measure styling, not correctness.
 *
 * So this module compares **engine-neutral structural content**, extracted from both engines'
 * bytes by ONE extractor. Using one extractor for both sides is the point: a per-engine extractor
 * pair can drift into agreeing by construction.
 *
 * Two tiers, and every report states which tier covered which diagram — an unclaimed invariant is
 * never presented as a checked one.
 *
 * **Tier 1 — rendered text multiset (all five syntax families, both engines).**
 * Every visible text run in the document, carrier-agnostic: `<text>`, `<tspan>`, and the HTML
 * inside `<foreignObject>` all reduce to the same leaf-text scan. Compared as a *multiset*, so a
 * duplicate label dropped from one of ten identical nodes is still caught. This is what detects a
 * dropped node, a dropped edge label, a truncated ER attribute block, a lost sequence message.
 * Accessibility text (`<title>`/`<desc>`) is extracted separately and never mixed into the visible
 * multiset, because the engines' a11y policies legitimately differ.
 *
 * **Tier 2 — geometric topology (node/edge families: flowchart, state).**
 * mermaid records each link's endpoints in `data-id="L_<src>_<dst>_<n>"`; we emit only a positional
 * `fm-edge-<i>`. Rather than change our output bytes to add endpoint attributes — which would
 * invalidate every pinned checksum in the ledger and alter the artifact being measured — topology
 * is *reconstructed geometrically for both engines by the same code*: take each edge path's first
 * and last point from its `d` attribute, resolve each to the nearest node anchor, and emit the
 * derived `src>dst` multiset. That multiset is then checked twice: engine against engine, and both
 * engines against the **input-derived ground truth**, since the harness generates the corpus and
 * therefore knows the true edge list. Engine-vs-spec is a stronger claim than engine-vs-engine:
 * it cannot be satisfied by two renderers being wrong in the same way.
 *
 * Geometric reconstruction is what catches a *mislaid* subgraph as well as a dropped edge: moving
 * a cluster's nodes without moving its edges makes endpoints resolve to the wrong anchors.
 *
 * Tier 2 is deliberately not claimed for sequence, class or ER. Their DOM is not a node/edge model
 * with recoverable per-edge paths in both engines, so those families carry Tier 1 only and say so.
 */

// ---------------------------------------------------------------- text extraction

const ENTITIES = new Map([
  ['amp', '&'], ['lt', '<'], ['gt', '>'], ['quot', '"'], ['apos', "'"], ['nbsp', ' '],
]);

/** Decode the XML entity forms either engine can emit. Unknown entities are left verbatim. */
export function decodeEntities(text) {
  return text.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z]+);/g, (whole, body) => {
    if (body[0] === '#') {
      const code = body[1] === 'x' || body[1] === 'X'
        ? Number.parseInt(body.slice(2), 16)
        : Number.parseInt(body.slice(1), 10);
      return Number.isFinite(code) && code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : whole;
    }
    return ENTITIES.get(body) ?? whole;
  });
}

/** Collapse runs of whitespace, including the newlines mermaid's HTML labels carry. */
const normalizeText = (text) => decodeEntities(text).replace(/\s+/g, ' ').trim();

/**
 * Split rendered text into comparable tokens.
 *
 * The engines legitimately *segment* text differently: for a state transition we emit one run
 * `S10: event9` where mermaid emits `S10` and `event9` as separate runs. Comparing exact runs would
 * therefore report a difference for two renders carrying identical content. Comparing the token
 * multiset is invariant to segmentation while still strictly accounting for every word, so a
 * dropped label still removes its tokens and is still caught.
 */
export function tokenize(text) {
  return normalizeText(text)
    .split(/[^\p{L}\p{N}_+*.:()<>|/-]+/u)
    .map((t) => t.replace(/^[.:|-]+|[.:|-]+$/g, ''))
    .filter((t) => t.length > 0);
}

/**
 * Remove element bodies whose text is not rendered diagram content. `<style>` matters most:
 * mermaid inlines a multi-kilobyte CSS block and we inline our own theme, and CSS text between
 * `>` and `<` would otherwise flood the visible multiset with selector noise.
 */
function stripNonContent(svg) {
  return svg
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, '<style/>')
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, '<script/>')
    .replace(/<!--[\s\S]*?-->/g, '');
}

/**
 * Pull the bodies of one element name out of a document, returning them in document order.
 * Used for the a11y channel, which must stay out of the visible multiset.
 */
function elementBodies(svg, name) {
  const out = [];
  const re = new RegExp(`<${name}\\b[^>]*>([\\s\\S]*?)</${name}>`, 'gi');
  for (let m = re.exec(svg); m !== null; m = re.exec(svg)) out.push(m[1]);
  return out;
}

/**
 * Every rendered text run in the document, carrier-agnostic.
 *
 * The scan is deliberately generic — any non-empty run between a `>` and the next `<` — rather
 * than a per-element-name walk. That single rule covers SVG `<text>`/`<tspan>` and the HTML
 * `<span>`/`<div>`/`<p>` inside a `<foreignObject>` at once, and because only *leaf* elements have
 * text directly between their tags, nesting cannot double-count: mermaid's
 * `<span class="nodeLabel"><p>Node 0</p></span>` yields `Node 0` exactly once.
 */
export function visibleTextRuns(svg) {
  let body = stripNonContent(svg);
  // a11y text is real output but is not diagram content, and the engines' a11y policies differ by
  // design (see the lean/`<const A11Y>` family). Extract it, then remove it from the visible scan.
  const accessible = [];
  for (const name of ['title', 'desc']) {
    for (const raw of elementBodies(body, name)) {
      const text = normalizeText(raw.replace(/<[^>]*>/g, ' '));
      if (text) accessible.push(text);
    }
    body = body.replace(new RegExp(`<${name}\\b[^>]*>[\\s\\S]*?</${name}>`, 'gi'), `<${name}/>`);
  }

  const visible = [];
  const re = />([^<>]+)</g;
  for (let m = re.exec(body); m !== null; m = re.exec(body)) {
    const text = normalizeText(m[1]);
    if (text) visible.push(text);
  }
  return { visible, accessible };
}

// ---------------------------------------------------------------- multiset comparison

/** Multiset as a plain object, so it serializes into the artifact as-is. */
export function multiset(values) {
  const counts = new Map();
  for (const v of values) counts.set(v, (counts.get(v) ?? 0) + 1);
  return counts;
}

/**
 * Compare two multisets. Returns every disagreement, capped for artifact size but with the true
 * total retained — a truncated diff must never read as a small one.
 */
export function diffMultisets(left, right, limit = 12) {
  const keys = new Set([...left.keys(), ...right.keys()]);
  const differences = [];
  for (const key of [...keys].sort()) {
    const l = left.get(key) ?? 0;
    const r = right.get(key) ?? 0;
    if (l !== r) differences.push({ value: key.length > 120 ? `${key.slice(0, 117)}...` : key, left: l, right: r });
  }
  return {
    equal: differences.length === 0,
    difference_count: differences.length,
    differences: differences.slice(0, limit),
    truncated: differences.length > limit,
  };
}

/**
 * What `required` contains that `available` does not, as counts. This is the containment test the
 * text gate uses: a non-zero result means content present in one render is missing from the other.
 */
export function shortfall(required, available, limit = 12) {
  const missing = [];
  let total = 0;
  for (const [key, need] of [...required.entries()].sort()) {
    const have = available.get(key) ?? 0;
    if (have < need) {
      total += need - have;
      missing.push({ value: key.length > 120 ? `${key.slice(0, 117)}...` : key, required: need, present: have });
    }
  }
  return {
    total_missing: total,
    distinct_missing: missing.length,
    missing: missing.slice(0, limit),
    truncated: missing.length > limit,
  };
}

// ---------------------------------------------------------------- geometry

/**
 * Absolute coordinate pairs in the order they appear in a path `d` attribute.
 *
 * Both engines emit absolute commands for edge paths (mermaid via d3's line generator, we via our
 * own writer), so an absolute-only reader is sufficient and, more importantly, honest: a relative
 * command would be *skipped* rather than silently misread, and `pathEndpoints` reports how many
 * commands it understood so a caller can refuse a path it could not fully parse.
 */
export function pathPoints(d) {
  const points = [];
  let unsupported = 0;
  // Split into command letter + argument run.
  const re = /([A-Za-z])([^A-Za-z]*)/g;
  for (let m = re.exec(d); m !== null; m = re.exec(d)) {
    const cmd = m[1];
    const nums = (m[2].match(/-?\d*\.?\d+(?:[eE][-+]?\d+)?/g) ?? []).map(Number);
    switch (cmd) {
      case 'M': case 'L': case 'T':
        for (let i = 0; i + 1 < nums.length; i += 2) points.push([nums[i], nums[i + 1]]);
        break;
      case 'C':
        // Only the on-curve endpoint of each cubic matters; control points are not on the path.
        for (let i = 0; i + 5 < nums.length; i += 6) points.push([nums[i + 4], nums[i + 5]]);
        break;
      case 'Q': case 'S':
        for (let i = 0; i + 3 < nums.length; i += 4) points.push([nums[i + 2], nums[i + 3]]);
        break;
      case 'A':
        for (let i = 0; i + 6 < nums.length; i += 7) points.push([nums[i + 5], nums[i + 6]]);
        break;
      case 'Z': case 'z':
        break;
      default:
        unsupported += 1;
    }
  }
  return { points, unsupported };
}

/** First and last on-path point, or null when the path could not be fully understood. */
export function pathEndpoints(d) {
  const { points, unsupported } = pathPoints(d);
  if (unsupported > 0 || points.length < 2) return null;
  return { start: points[0], end: points[points.length - 1] };
}

const distance = (a, b) => Math.hypot(a[0] - b[0], a[1] - b[1]);

/**
 * Resolve a point to the nearest node anchor, requiring an unambiguous winner.
 *
 * `ratio` guards against a coin-flip: when the two closest anchors are within `ratio` of each
 * other the assignment is not evidence, and we return `null` so the caller degrades that diagram
 * to Tier 1 rather than reporting a topology that a rounding difference could have flipped.
 */
export function nearestAnchor(point, anchors, ratio = 0.75) {
  let best = null;
  let bestD = Infinity;
  let secondD = Infinity;
  for (const [id, center] of anchors) {
    const d = distance(point, center);
    if (d < bestD) { secondD = bestD; bestD = d; best = id; }
    else if (d < secondD) { secondD = d; }
  }
  if (best === null) return null;
  if (Number.isFinite(secondD) && bestD > secondD * ratio) return null;
  return best;
}

// ---------------------------------------------------------------- node identity
//
// Neither engine exposes the author's node id verbatim, and they mangle it differently:
//
//   mermaid   <diagramSlug>_r<n>_<n>-<kind>-<userId>-<counter>   e.g. class50_r14_0-classId-C0-2350
//   ours      fm-node-<lowercased userId>-<counter>              e.g. fm-node-c0-0
//
// The trailing counters are each engine's own element numbering and carry no shared meaning
// (mermaid's class ids start at 2350 here, ours at 0), so they are stripped rather than compared.
// Our writer lowercases, which is lossy, so the comparison is case-insensitive on both sides --
// stated here because it is a real, if small, weakening of the check.

const MERMAID_KINDS = 'flowchart|classId|entity|state|note|actor|subGraph|cluster';

/** Recover the author-facing node id from one engine's mangled element id. */
export function canonicalNodeId(engine, rawId) {
  if (typeof rawId !== 'string' || rawId.length === 0) return null;
  let id = rawId;
  if (engine === 'mermaid-js') {
    const m = new RegExp(`^.*?-(?:${MERMAID_KINDS})-(.+)$`).exec(id);
    if (m) id = m[1];
  } else {
    const m = /^fm-node-(.+)$/.exec(id);
    if (m) id = m[1];
  }
  // Strip the engine's own trailing element counter.
  id = id.replace(/-\d+$/, '');
  return id.toLowerCase();
}

/**
 * Pseudo-nodes each engine invents for the same construct under different names -- a state
 * diagram's implicit start marker is `root_start` to mermaid and `state-start` to us. They are real
 * output, but their *names* are not a shared contract, so they are compared by count, not identity.
 *
 * Known limitation: an author node literally named `start` or `end` is classified as synthetic and
 * therefore checked by count rather than by id. That is the deliberate direction to err in -- failing
 * to map the pseudo-nodes produces a false failure on every state diagram, while this costs id-level
 * strictness on two specific names. No corpus item uses them.
 */
export function isSyntheticNode(id) {
  return /^(root[_-])?(state[_-])?(start|end)$/.test(id) || /^root[_-]/.test(id);
}

// ---------------------------------------------------------------- engine adapters
//
// The adapters do ONE thing: say where this engine's nodes and edges are in the DOM. Everything
// downstream -- text scanning, multiset comparison, geometric resolution -- is shared code applied
// identically to both sides.

const ATTR = (tag, name) => {
  const m = new RegExp(`\\b${name}="([^"]*)"`).exec(tag);
  return m ? m[1] : null;
};

/** Every `<g ...>` open tag with its byte offset, so a group's subtree can be sliced out. */
function openGroups(svg) {
  const out = [];
  const re = /<g\b[^>]*>/g;
  for (let m = re.exec(svg); m !== null; m = re.exec(svg)) out.push({ tag: m[0], at: m.index, end: re.lastIndex });
  return out;
}

/** The substring of `svg` spanned by the `<g>` opening at `from`, honouring nesting. */
function groupSubtree(svg, from, openEnd) {
  let depth = 1;
  const re = /<\/?g\b[^>]*>/g;
  re.lastIndex = openEnd;
  for (let m = re.exec(svg); m !== null; m = re.exec(svg)) {
    if (m[0][1] === '/') { depth -= 1; if (depth === 0) return svg.slice(from, re.lastIndex); }
    else if (!m[0].endsWith('/>')) depth += 1;
  }
  return svg.slice(from);
}

/**
 * Combined translate offset of a `transform` chain. Both engines position node groups with
 * `translate(...)`, which is where a node's anchor actually comes from.
 */
function translateOf(tag) {
  const transform = ATTR(tag, 'transform');
  if (!transform) return null;
  let x = 0;
  let y = 0;
  let seen = false;
  const re = /translate\(\s*(-?[\d.eE+]+)(?:\s*[, ]\s*(-?[\d.eE+]+))?\s*\)/g;
  for (let m = re.exec(transform); m !== null; m = re.exec(transform)) {
    x += Number(m[1]);
    y += Number(m[2] ?? 0);
    seen = true;
  }
  return seen ? [x, y] : null;
}

/**
 * mermaid: node groups carry `data-id="<userId>"` (and an `id` of
 * `<diagramId>-flowchart-<userId>-<n>`); links carry `data-id="L_<src>_<dst>_<n>"`.
 */
function mermaidStructure(svg) {
  const nodes = new Map();
  for (const g of openGroups(svg)) {
    const cls = ATTR(g.tag, 'class') ?? '';
    if (!/\bnode\b/.test(cls) || /\bnodeLabel\b/.test(cls)) continue;
    const at = translateOf(g.tag);
    const raw = ATTR(g.tag, 'data-id') ?? ATTR(g.tag, 'id');
    const id = canonicalNodeId('mermaid-js', raw);
    if (!id || !at) continue;
    // A repeated id is not a node model we can anchor against; drop to Tier 1 by leaving it out.
    if (!nodes.has(id)) nodes.set(id, at);
  }

  const edges = [];
  const re = /<path\b[^>]*>/g;
  for (let m = re.exec(svg); m !== null; m = re.exec(svg)) {
    const cls = ATTR(m[0], 'class') ?? '';
    if (!/flowchart-link|transition|\bedge-thickness/.test(cls)) continue;
    const d = ATTR(m[0], 'd');
    if (!d) continue;
    const dataId = ATTR(m[0], 'data-id');
    edges.push({ d, declared: dataId });
  }
  return { nodes, edges };
}

/**
 * frankenmermaid: node groups are `class="fm-node ..."` with `id="<userId>"`; edge groups are
 * `class="fm-edge"` with `id="fm-edge-<i>"` and carry their geometry on an inner `<path>`.
 */
function frankenStructure(svg) {
  const nodes = new Map();
  const edges = [];
  for (const g of openGroups(svg)) {
    const cls = ATTR(g.tag, 'class') ?? '';
    if (/\bfm-node\b/.test(cls)) {
      const id = canonicalNodeId('frankenmermaid', ATTR(g.tag, 'id'));
      if (!id) continue;
      const at = translateOf(g.tag);
      if (at) { if (!nodes.has(id)) nodes.set(id, at); continue; }
      // No transform: derive the anchor from the group's own shape/text geometry.
      const subtree = groupSubtree(svg, g.at, g.end);
      const anchor = shapeAnchor(subtree);
      if (anchor && !nodes.has(id)) nodes.set(id, anchor);
      continue;
    }
    if (/\bfm-edge\b/.test(cls) && ATTR(g.tag, 'data-fm-edge-id') !== null) {
      const subtree = groupSubtree(svg, g.at, g.end);
      const d = /<path\b[^>]*\bd="([^"]*)"/.exec(subtree)?.[1];
      if (d) edges.push({ d, declared: null });
    }
  }
  return { nodes, edges };
}

/** Centre of the first `<rect>`/`<circle>`/`<ellipse>`/`<polygon>` in a node subtree. */
function shapeAnchor(subtree) {
  const rect = /<rect\b[^>]*>/.exec(subtree)?.[0];
  if (rect) {
    const x = Number(ATTR(rect, 'x'));
    const y = Number(ATTR(rect, 'y'));
    const w = Number(ATTR(rect, 'width'));
    const h = Number(ATTR(rect, 'height'));
    if ([x, y, w, h].every(Number.isFinite)) return [x + w / 2, y + h / 2];
  }
  for (const name of ['circle', 'ellipse']) {
    const tag = new RegExp(`<${name}\\b[^>]*>`).exec(subtree)?.[0];
    if (tag) {
      const cx = Number(ATTR(tag, 'cx'));
      const cy = Number(ATTR(tag, 'cy'));
      if (Number.isFinite(cx) && Number.isFinite(cy)) return [cx, cy];
    }
  }
  const poly = /<polygon\b[^>]*\bpoints="([^"]*)"/.exec(subtree)?.[1];
  if (poly) {
    const nums = (poly.match(/-?\d*\.?\d+/g) ?? []).map(Number);
    if (nums.length >= 4) {
      let sx = 0;
      let sy = 0;
      let n = 0;
      for (let i = 0; i + 1 < nums.length; i += 2) { sx += nums[i]; sy += nums[i + 1]; n += 1; }
      return [sx / n, sy / n];
    }
  }
  const text = /<text\b[^>]*>/.exec(subtree)?.[0];
  if (text) {
    const x = Number(ATTR(text, 'x'));
    const y = Number(ATTR(text, 'y'));
    if (Number.isFinite(x) && Number.isFinite(y)) return [x, y];
  }
  return null;
}

const ADAPTERS = { 'mermaid-js': mermaidStructure, frankenmermaid: frankenStructure };

/** Collapse each engine's own pseudo-node name so a shared topology can be stated. */
const pseudo = (id) => (isSyntheticNode(id) ? '#pseudo' : id);

// ---------------------------------------------------------------- signatures

/**
 * The engine-neutral structural signature of one rendered SVG.
 *
 * `topology` is present only when this document's own geometry supports an unambiguous
 * reconstruction; `topology_status` always records why it is or is not available, so a report can
 * never quietly present "no topology extracted" as "topology agreed".
 */
export function signature(svg, engine) {
  const adapter = ADAPTERS[engine];
  if (!adapter) throw new Error(`unknown engine for equivalence extraction: ${engine}`);
  const { visible, accessible } = visibleTextRuns(svg);
  const { nodes, edges } = adapter(svg);

  const anchors = [...nodes.entries()];
  const derived = [];
  let unresolved = 0;
  let unparsed = 0;
  for (const edge of edges) {
    const ends = pathEndpoints(edge.d);
    if (!ends) { unparsed += 1; continue; }
    const from = nearestAnchor(ends.start, anchors);
    const to = nearestAnchor(ends.end, anchors);
    if (from === null || to === null) { unresolved += 1; continue; }
    derived.push(`${pseudo(from)}>${pseudo(to)}`);
  }

  const ids = [...nodes.keys()];
  const resolvable = edges.length > 0 && anchors.length > 0 && unresolved === 0 && unparsed === 0;
  return {
    engine,
    bytes: svg.length,
    visible_text: visible,
    visible_tokens: visible.flatMap(tokenize),
    accessible_text_count: accessible.length,
    node_ids: ids.filter((id) => !isSyntheticNode(id)).sort(),
    synthetic_node_count: ids.filter(isSyntheticNode).length,
    node_count: nodes.size,
    edge_element_count: edges.length,
    // mermaid states endpoints in `data-id`; retained as provenance for the geometric result.
    declared_edges: edges.map((e) => e.declared).filter(Boolean).sort(),
    topology: resolvable ? derived.slice().sort() : null,
    topology_status: resolvable
      ? 'geometric'
      : edges.length === 0
        ? 'no_edge_elements'
        : anchors.length === 0
          ? 'no_node_anchors'
          : `ambiguous(unresolved=${unresolved},unparsed=${unparsed})`,
  };
}

// ---------------------------------------------------------------- ground truth from input

/**
 * The true node and edge sets of one corpus revision, read from the mermaid source the harness
 * generated. Only the flat `flowchart`/`graph` and `stateDiagram` node/edge forms are decoded;
 * anything else returns `null` and the diagram carries Tier 1 only.
 *
 * This is what makes the check engine-vs-spec rather than only engine-vs-engine.
 */
export function groundTruth(text) {
  const header = text.trimStart().split('\n', 1)[0].trim();
  const isFlow = /^(flowchart|graph)\b/.test(header);
  if (!isFlow) return null;

  const nodes = new Set();
  const edges = [];
  // A hyphen is legal inside a mermaid node id, but a greedy `[\w-]*` swallows the `--` of the
  // following arrow and yields ids like `b--`. Allow `-` only when it does not begin an arrow.
  const NODE = '[A-Za-z_](?:\\w|-(?![->]))*';
  // `A[Label]`, `A(Label)`, `A{Label}` and bare `A`, joined by an arrow with an optional
  // `|edge label|`. Only the two-endpoint form is decoded; chained `A-->B-->C` is expanded.
  const linkRe = new RegExp(
    `(${NODE})\\s*(?:\\[[^\\]]*\\]|\\([^)]*\\)|\\{[^}]*\\})?\\s*` +
    '(?:-{2,3}>|-{2,3}|={2,3}>|-\\.->)\\s*' +
    `(?:\\|[^|]*\\|\\s*)?(${NODE})`,
    'g',
  );
  for (const rawLine of text.split('\n')) {
    const line = rawLine.trim();
    if (!line || line.startsWith('%%')) continue;
    if (/^(flowchart|graph|subgraph|end|classDef|class|style|linkStyle|click)\b/.test(line)) {
      const decl = new RegExp(`^(${NODE})\\s*(?:\\[[^\\]]*\\]|\\([^)]*\\)|\\{[^}]*\\})`).exec(line);
      if (decl) nodes.add(decl[1]);
      continue;
    }
    let matched = false;
    linkRe.lastIndex = 0;
    for (let m = linkRe.exec(line); m !== null; m = linkRe.exec(line)) {
      nodes.add(m[1]);
      nodes.add(m[2]);
      edges.push(`${m[1]}>${m[2]}`);
      matched = true;
      // Support `A-->B-->C` by resuming at the target.
      linkRe.lastIndex = Math.max(linkRe.lastIndex - m[2].length, m.index + 1);
    }
    if (matched) continue;
    const decl = new RegExp(`^(${NODE})\\s*(?:\\[[^\\]]*\\]|\\([^)]*\\)|\\{[^}]*\\})?\\s*$`).exec(line);
    if (decl) nodes.add(decl[1]);
  }
  if (nodes.size === 0) return null;
  // Lowercased to match the canonical node identity the extractor can recover from both engines.
  return {
    node_ids: [...nodes].map((n) => n.toLowerCase()).sort(),
    edges: edges.map((e) => e.toLowerCase()).sort(),
  };
}

// ---------------------------------------------------------------- verdict

/**
 * Compare one diagram across engines. `truth` may be null; the verdict records which invariants
 * were actually decided rather than assuming an absent one passed.
 */
/**
 * Families whose DOM is a node/edge model in BOTH engines, and for which Tier 2 is therefore
 * claimed. For these, a topology that cannot be decided is NOT a pass: it is `unverified`.
 *
 * This distinction is load-bearing. Displacing a node far from its edges does not produce a *wrong*
 * topology, it produces an *ambiguous* one -- every endpoint resolves to a coin-flip and the check
 * goes undecided. Collapsing that into "equivalent" would hand a renderer a way to evade the gate
 * by degrading its own geometry, so the third verdict exists and the harness gate refuses it.
 */
export const TIER2_FAMILIES = new Set(['flowchart', 'state']);

export function compareDiagram({ index, family, fmSvg, jsSvg, source }) {
  const fm = signature(fmSvg, 'frankenmermaid');
  const js = signature(jsSvg, 'mermaid-js');
  const truth = source ? groundTruth(source) : null;

  const checks = [];

  // GATING, and deliberately one-directional. The failure this whole module exists to catch is
  // *content loss on our side*: a label, member or message that mermaid renders and we do not.
  // Rendering strictly more than mermaid -- we draw ER relationship cardinalities (`0..*`, `1`)
  // that 11.15.0 omits -- is a feature difference, not a correctness defect, and must not fail the
  // run. So the gate asserts mermaid's token multiset is contained in ours, and the symmetric
  // difference is reported separately as provenance.
  const missing = shortfall(multiset(js.visible_tokens), multiset(fm.visible_tokens));
  checks.push({
    invariant: 'rendered_text_no_loss',
    tier: 1,
    decided: true,
    pass: missing.total_missing === 0,
    detail: {
      ...missing,
      js_token_count: js.visible_tokens.length,
      fm_token_count: fm.visible_tokens.length,
    },
  });

  // Provenance only: exact run-for-run equality is not expected (the engines segment text
  // differently and we render extra content), so this never gates.
  const exact = diffMultisets(multiset(fm.visible_text), multiset(js.visible_text));
  checks.push({
    invariant: 'rendered_text_exact_runs',
    tier: 1,
    decided: false,
    gating: false,
    pass: exact.equal,
    detail: exact,
  });

  const nodeIds = diffMultisets(multiset(fm.node_ids), multiset(js.node_ids));
  checks.push({
    invariant: 'node_id_set',
    tier: 1,
    decided: fm.node_ids.length > 0 && js.node_ids.length > 0,
    pass: nodeIds.equal,
    detail: {
      ...nodeIds,
      fm_node_count: fm.node_count,
      js_node_count: js.node_count,
      fm_synthetic: fm.synthetic_node_count,
      js_synthetic: js.synthetic_node_count,
    },
  });

  const bothTopo = fm.topology !== null && js.topology !== null;
  const topo = bothTopo ? diffMultisets(multiset(fm.topology), multiset(js.topology)) : null;
  checks.push({
    invariant: 'edge_topology_cross_engine',
    tier: 2,
    decided: bothTopo,
    pass: bothTopo ? topo.equal : null,
    detail: {
      fm_status: fm.topology_status,
      js_status: js.topology_status,
      fm_edge_elements: fm.edge_element_count,
      js_edge_elements: js.edge_element_count,
      ...(topo ?? {}),
    },
  });

  // mermaid states its own endpoints; when it does, its geometric reconstruction must agree with
  // them. This is the extractor's own null control: it proves the geometry code is not inventing
  // a topology that merely happens to match on both sides.
  const declaredPairs = js.declared_edges
    .map((id) => /^L_(.+)_(.+?)_\d+$/.exec(id))
    .filter(Boolean)
    .map((m) => `${pseudo(m[1].toLowerCase())}>${pseudo(m[2].toLowerCase())}`)
    .sort();
  const declaredDecidable = js.topology !== null && declaredPairs.length === js.declared_edges.length
    && declaredPairs.length > 0;
  const declared = declaredDecidable ? diffMultisets(multiset(js.topology), multiset(declaredPairs)) : null;
  checks.push({
    invariant: 'incumbent_geometry_matches_declared_ids',
    tier: 2,
    decided: declaredDecidable,
    pass: declaredDecidable ? declared.equal : null,
    detail: declared ?? { declared_ids: js.declared_edges.length },
  });

  for (const [engine, sig] of [['frankenmermaid', fm], ['mermaid-js', js]]) {
    const decidable = truth !== null && sig.topology !== null;
    const against = decidable ? diffMultisets(multiset(sig.topology), multiset(truth.edges)) : null;
    checks.push({
      invariant: `edge_topology_vs_input__${engine}`,
      tier: 2,
      decided: decidable,
      pass: decidable ? against.equal : null,
      detail: against ?? { reason: truth === null ? 'input_not_decodable' : sig.topology_status },
    });
  }

  // `decided` is the set of invariants this diagram's own output could actually settle. An
  // invariant that could not be decided is reported as such and never counted as a pass.
  const decided = checks.filter((c) => c.decided && c.gating !== false);
  const failed = decided.filter((c) => c.pass === false);

  // Tier 2 is claimed for this family but its own output could not settle it: report `unverified`
  // rather than letting an undecidable check read as agreement.
  const tier2Missing = TIER2_FAMILIES.has(family)
    && !decided.some((c) => c.invariant === 'edge_topology_cross_engine');
  const verdict = failed.length > 0 ? 'divergent' : tier2Missing ? 'unverified' : 'equivalent';

  return {
    index,
    family,
    verdict,
    unverified_reason: verdict === 'unverified'
      ? `tier2 claimed for ${family} but topology undecidable (fm=${fm.topology_status}, js=${js.topology_status})`
      : undefined,
    tiers_decided: [...new Set(decided.map((c) => c.tier))].sort(),
    checks_decided: decided.length,
    checks_failed: failed.length,
    // Full detail only for failures; a 500-diagram batch would otherwise bury the finding.
    checks: failed.length === 0
      ? checks.map((c) => ({ invariant: c.invariant, tier: c.tier, gating: c.gating !== false, decided: c.decided, pass: c.pass }))
      : checks,
    fm: { bytes: fm.bytes, node_count: fm.node_count, edge_element_count: fm.edge_element_count, topology_status: fm.topology_status },
    js: { bytes: js.bytes, node_count: js.node_count, edge_element_count: js.edge_element_count, topology_status: js.topology_status },
  };
}

/** Roll per-diagram verdicts into the record the harness gates on. */
export function summarize(results) {
  const byInvariant = new Map();
  for (const r of results) {
    for (const c of r.checks) {
      const row = byInvariant.get(c.invariant)
        ?? { gating: c.gating !== false, decided: 0, passed: 0, failed: 0, undecided: 0 };
      if (c.decided === false) row.undecided += 1;
      else if (c.pass === true) { row.decided += 1; row.passed += 1; }
      else if (c.pass === false) { row.decided += 1; row.failed += 1; }
      else row.undecided += 1;
      byInvariant.set(c.invariant, row);
    }
  }
  const divergent = results.filter((r) => r.verdict === 'divergent');
  const unverified = results.filter((r) => r.verdict === 'unverified');
  const families = new Map();
  for (const r of results) {
    const row = families.get(r.family) ?? { diagrams: 0, divergent: 0, unverified: 0, tier2: 0 };
    row.diagrams += 1;
    if (r.verdict === 'divergent') row.divergent += 1;
    if (r.verdict === 'unverified') row.unverified += 1;
    if (r.tiers_decided.includes(2)) row.tier2 += 1;
    families.set(r.family, row);
  }
  return {
    method: 'svg_structural',
    rasterized_perceptual_diff: false,
    // Stated explicitly so a reader never has to infer the strength of the claim.
    claim: 'engine-neutral structural equivalence: rendered-text multiset (all families) plus '
      + 'geometrically reconstructed edge topology cross-engine and against input-derived ground '
      + 'truth (node/edge families only). Not byte equality; not a pixel diff.',
    diagrams: results.length,
    equivalent: results.length - divergent.length - unverified.length,
    divergent: divergent.length,
    // Claimed-but-undecidable is a gate failure, not a pass. See TIER2_FAMILIES.
    unverified: unverified.length,
    verdict: divergent.length === 0 && unverified.length === 0 ? 'pass' : 'fail',
    tier2_diagrams: results.filter((r) => r.tiers_decided.includes(2)).length,
    by_invariant: Object.fromEntries(byInvariant),
    by_family: Object.fromEntries(families),
    divergent_samples: divergent.slice(0, 5),
    unverified_samples: unverified.slice(0, 5).map((r) => ({ index: r.index, family: r.family, reason: r.unverified_reason })),
  };
}

// ---------------------------------------------------------------- self-test
//
// A gate that has never been observed to fail is not evidence. These are MUTATION controls: each
// takes a pair that compares as equivalent, introduces exactly one of the defects this module
// claims to catch, and asserts the verdict flips. Without them, "500/500 equivalent" could equally
// mean "the comparator cannot see anything".

/** A minimal flowchart pair in each engine's real dialect: three nodes, two edges. */
function fixturePair() {
  const js = `<svg id="d"><style>.node{fill:#fff}</style><g class="root"><g class="nodes">`
    + `<g class="node default" id="d_r1_0-flowchart-A-0" transform="translate(50, 20)">`
    + `<foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel"><p>Alpha</p></span></div></foreignObject></g>`
    + `<g class="node default" id="d_r1_0-flowchart-B-1" transform="translate(150, 20)">`
    + `<foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel"><p>Beta</p></span></div></foreignObject></g>`
    + `<g class="node default" id="d_r1_0-flowchart-C-2" transform="translate(250, 20)">`
    + `<foreignObject><div xmlns="http://www.w3.org/1999/xhtml"><span class="nodeLabel"><p>Gamma</p></span></div></foreignObject></g>`
    + `</g><g class="edgePaths">`
    + `<path d="M55,20L145,20" class="flowchart-link" data-id="L_A_B_0"/>`
    + `<path d="M155,20L245,20" class="flowchart-link" data-id="L_B_C_0"/>`
    + `</g></g></svg>`;
  const fm = `<svg><style>.fm-node{fill:#fff}</style>`
    + `<g id="fm-node-a-0" class="fm-node fm-node-shape-rect" transform="translate(50, 20)"><text x="0" y="0">Alpha</text></g>`
    + `<g id="fm-node-b-1" class="fm-node fm-node-shape-rect" transform="translate(150, 20)"><text x="0" y="0">Beta</text></g>`
    + `<g id="fm-node-c-2" class="fm-node fm-node-shape-rect" transform="translate(250, 20)"><text x="0" y="0">Gamma</text></g>`
    + `<g id="fm-edge-0" class="fm-edge" data-fm-edge-id="0"><path d="M55,20L145,20"/></g>`
    + `<g id="fm-edge-1" class="fm-edge" data-fm-edge-id="1"><path d="M155,20L245,20"/></g>`
    + `</svg>`;
  const source = 'flowchart LR\n  A[Alpha]\n  B[Beta]\n  C[Gamma]\n  A-->B\n  B-->C\n';
  return { js, fm, source };
}

const failedInvariants = (result) =>
  result.checks.filter((c) => c.pass === false && c.decided && c.gating !== false).map((c) => c.invariant);

export function selfTest() {
  const cases = [];
  const record = (name, ok, detail) => {
    cases.push({ name, ok, detail });
    if (!ok) throw new Error(`equivalence self-test failed: ${name} (${JSON.stringify(detail)})`);
  };

  const { js, fm, source } = fixturePair();
  const base = compareDiagram({ index: 0, family: 'flowchart', fmSvg: fm, jsSvg: js, source });
  record('baseline_pair_is_equivalent', base.verdict === 'equivalent',
    { verdict: base.verdict, failed: failedInvariants(base) });
  record('baseline_decides_tier2', base.tiers_decided.includes(2), base.tiers_decided);
  // The extractor's own geometry must agree with mermaid's declared endpoints, or a topology
  // "agreement" downstream proves nothing.
  record('baseline_geometry_matches_declared_ids',
    base.checks.some((c) => c.invariant === 'incumbent_geometry_matches_declared_ids' && c.pass === true),
    base.checks.map((c) => [c.invariant, c.pass]));

  // MUTATION 1 -- we drop a node's label. The text gate must catch it.
  const droppedLabel = fm.replace('<text x="0" y="0">Beta</text>', '');
  const m1 = compareDiagram({ index: 0, family: 'flowchart', fmSvg: droppedLabel, jsSvg: js, source });
  record('dropped_label_is_divergent',
    m1.verdict === 'divergent' && failedInvariants(m1).includes('rendered_text_no_loss'),
    { verdict: m1.verdict, failed: failedInvariants(m1) });

  // MUTATION 2 -- we drop an entire edge. Topology must catch it, cross-engine and vs input.
  const droppedEdge = fm.replace('<g id="fm-edge-1" class="fm-edge" data-fm-edge-id="1"><path d="M155,20L245,20"/></g>', '');
  const m2 = compareDiagram({ index: 0, family: 'flowchart', fmSvg: droppedEdge, jsSvg: js, source });
  record('dropped_edge_is_divergent',
    m2.verdict === 'divergent'
    && failedInvariants(m2).includes('edge_topology_cross_engine')
    && failedInvariants(m2).includes('edge_topology_vs_input__frankenmermaid'),
    { verdict: m2.verdict, failed: failedInvariants(m2) });

  // MUTATION 3 -- we keep the edge count but rewire it (A->C instead of B->C). A count-only check
  // would pass this; geometric endpoint resolution must not.
  const rewired = fm.replace('<path d="M155,20L245,20"/>', '<path d="M55,20L245,20"/>');
  const m3 = compareDiagram({ index: 0, family: 'flowchart', fmSvg: rewired, jsSvg: js, source });
  record('rewired_edge_is_divergent',
    m3.verdict === 'divergent' && failedInvariants(m3).includes('edge_topology_cross_engine'),
    { verdict: m3.verdict, failed: failedInvariants(m3) });

  // MUTATION 4 -- a node is moved far from its edges ("mislaid subgraph"). Endpoints then resolve
  // to the wrong anchor.
  const mislaid = fm.replace('<g id="fm-node-b-1" class="fm-node fm-node-shape-rect" transform="translate(150, 20)">',
    '<g id="fm-node-b-1" class="fm-node fm-node-shape-rect" transform="translate(9000, 9000)">');
  const m4 = compareDiagram({ index: 0, family: 'flowchart', fmSvg: mislaid, jsSvg: js, source });
  // Displacement makes topology ambiguous rather than wrong, so the honest outcome is a refusal to
  // certify -- but it must NOT be "equivalent".
  record('mislaid_node_is_not_certified', m4.verdict !== 'equivalent',
    { verdict: m4.verdict, failed: failedInvariants(m4), reason: m4.unverified_reason });

  // NEGATIVE CONTROL -- rendering strictly MORE than mermaid is a feature difference, not loss, and
  // must not fail. This is the ER cardinality case (`0..*`, `1`) seen in the real corpus.
  const extra = fm.replace('</svg>', '<text x="1" y="1">0..*</text><text x="2" y="2">1</text></svg>');
  const n1 = compareDiagram({ index: 0, family: 'flowchart', fmSvg: extra, jsSvg: js, source });
  record('extra_content_is_not_a_failure', n1.verdict === 'equivalent',
    { verdict: n1.verdict, failed: failedInvariants(n1) });

  // NEGATIVE CONTROL -- differing text SEGMENTATION must not fail. We emit one run where mermaid
  // emits two; the token multiset is unchanged.
  const merged = fm.replace('<text x="0" y="0">Beta</text>', '<text x="0" y="0">Beta extra-word</text>');
  const jsSplit = js.replace('<p>Beta</p>', '<p>Beta</p><p>extra-word</p>');
  const n2 = compareDiagram({ index: 0, family: 'flowchart', fmSvg: merged, jsSvg: jsSplit, source });
  record('segmentation_difference_is_not_a_failure', n2.verdict === 'equivalent',
    { verdict: n2.verdict, failed: failedInvariants(n2) });

  // Unit-level invariants the above depend on.
  record('entities_decode', decodeEntities('a&lt;b&amp;c&#65;&#x42;') === 'a<b&cAB', decodeEntities('a&lt;b&amp;c&#65;&#x42;'));
  record('style_text_excluded', !visibleTextRuns('<svg><style>.a{fill:red}</style><text>X</text></svg>').visible.join('|').includes('fill'),
    visibleTextRuns('<svg><style>.a{fill:red}</style><text>X</text></svg>').visible);
  record('a11y_text_separated', (() => {
    const r = visibleTextRuns('<svg><title>Chart</title><text>X</text></svg>');
    return r.visible.length === 1 && r.visible[0] === 'X' && r.accessible.length === 1;
  })(), visibleTextRuns('<svg><title>Chart</title><text>X</text></svg>'));
  record('nested_html_label_counted_once',
    visibleTextRuns('<svg><span class="nodeLabel"><p>Node 0</p></span></svg>').visible.join('|') === 'Node 0',
    visibleTextRuns('<svg><span class="nodeLabel"><p>Node 0</p></span></svg>').visible);
  record('cubic_endpoint_only', (() => {
    const p = pathPoints('M0,0C10,10 20,20 30,30');
    return p.points.length === 2 && p.points[1][0] === 30 && p.points[1][1] === 30;
  })(), pathPoints('M0,0C10,10 20,20 30,30'));
  record('relative_command_refuses', pathEndpoints('M0,0l10,10') === null, pathEndpoints('M0,0l10,10'));
  record('ambiguous_anchor_refuses',
    nearestAnchor([10, 0], [['a', [0, 0]], ['b', [20, 0]]]) === null,
    nearestAnchor([10, 0], [['a', [0, 0]], ['b', [20, 0]]]));
  record('canonical_ids_agree',
    canonicalNodeId('mermaid-js', 'class50_r14_0-classId-C0-2350') === 'c0'
    && canonicalNodeId('frankenmermaid', 'fm-node-c0-0') === 'c0'
    && canonicalNodeId('mermaid-js', 'er40_r14_0-entity-E7-7') === 'e7'
    && canonicalNodeId('frankenmermaid', 'fm-node-e7-7') === 'e7',
    [canonicalNodeId('mermaid-js', 'class50_r14_0-classId-C0-2350'), canonicalNodeId('frankenmermaid', 'fm-node-c0-0')]);
  record('ground_truth_reads_chained_edges', (() => {
    const t = groundTruth('flowchart LR\n  A-->B-->C\n');
    return t !== null && t.edges.join(',') === 'a>b,b>c';
  })(), groundTruth('flowchart LR\n  A-->B-->C\n'));
  record('ground_truth_reads_edge_labels', (() => {
    const t = groundTruth('flowchart LR\n  A-->|goes to|B\n');
    return t !== null && t.edges.join(',') === 'a>b';
  })(), groundTruth('flowchart LR\n  A-->|goes to|B\n'));
  record('non_flow_source_is_not_decodable', groundTruth('classDiagram\n  class C0\n') === null, null);

  return { ok: true, cases: cases.length, mutation_controls: 4, negative_controls: 2 };
}

// Run as a script: `node scripts/headtohead/svg_equivalence.mjs --self-test`
if (process.argv[1] && process.argv[1].endsWith('svg_equivalence.mjs') && process.argv.includes('--self-test')) {
  console.log(JSON.stringify(selfTest()));
}
