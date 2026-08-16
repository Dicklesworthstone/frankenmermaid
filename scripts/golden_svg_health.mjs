#!/usr/bin/env node
// Structural health of the committed golden SVGs — the regression net for bd-8zo0.
//
// A golden corpus is a byte snapshot: it pins that the output does not CHANGE, and is blind to the
// output being wrong in a way it has always been wrong. `gantt_basic.svg` referenced
// `marker-end="url(#arrowhead)"` against an empty `<defs>` in every committed revision, so every
// arrowhead silently failed to draw and every byte comparison passed. These four invariants are the
// checks a byte golden cannot make for itself.
//
//   node scripts/golden_svg_health.mjs             scan the committed corpora, exit 1 on a violation
//   node scripts/golden_svg_health.mjs --self-test prove each check fires on a synthetic violation
//
// Deliberately NOT a renderer test: it reads what is committed, so it also catches a golden that was
// re-blessed from a broken build.

import { readdirSync, readFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const REPO = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const CORPORA = [
  join(REPO, 'crates/fm-cli/tests/golden'),
  join(REPO, 'artifacts/regression-harness/latest/golden'),
];

const NUMERIC_ATTRS =
  'x|y|cx|cy|r|rx|ry|width|height|x1|y1|x2|y2|dx|dy|offset|stroke-width|font-size';

/**
 * Every violation one document commits, as `{ check, detail }`.
 *
 * Each check is a distinct failure mode, not a style preference:
 * - `dangling_reference`: the document points at an id it never defines, so the referenced marker,
 *   gradient or clip path simply does not apply. This is the bd-8zo0 defect.
 * - `duplicate_id`: invalid SVG, and `url(#id)` then resolves to whichever element the viewer picks.
 * - `non_finite_number`: a NaN coordinate does not misplace an element, it REMOVES it — the viewer
 *   drops the shape entirely, so content loss reads as a rendering that "looks fine".
 * - `bare_ampersand`: not well-formed XML; a strict consumer refuses the whole document.
 * - `bad_viewbox`: a missing or degenerate viewBox makes the diagram unscalable or invisible.
 */
export function svgHealthViolations(svg) {
  const violations = [];

  const declared = new Set([...svg.matchAll(/\sid="([^"]+)"/g)].map((m) => m[1]));
  const referenced = new Set([
    ...[...svg.matchAll(/url\(#([^)"']+)\)/g)].map((m) => m[1]),
    ...[...svg.matchAll(/(?:xlink:)?href="#([^"]+)"/g)].map((m) => m[1]),
  ]);
  for (const id of referenced) {
    if (!declared.has(id)) violations.push({ check: 'dangling_reference', detail: `url(#${id})` });
  }

  const seen = new Set();
  for (const [, id] of svg.matchAll(/\sid="([^"]+)"/g)) {
    if (seen.has(id)) violations.push({ check: 'duplicate_id', detail: id });
    seen.add(id);
  }

  for (const [, key, value] of svg.matchAll(new RegExp(`\\s(${NUMERIC_ATTRS})="([^"]*)"`, 'g'))) {
    if (/nan|infinity|\binf\b/i.test(value)) {
      violations.push({ check: 'non_finite_number', detail: `${key}="${value}"` });
    }
  }

  const bare = [...svg.matchAll(/&(?!#\d+;|#x[0-9a-fA-F]+;|[a-zA-Z][a-zA-Z0-9]*;)/g)];
  if (bare.length > 0) {
    violations.push({ check: 'bare_ampersand', detail: `${bare.length} occurrence(s)` });
  }

  const viewBox = /viewBox="([^"]*)"/.exec(svg);
  if (!viewBox) {
    violations.push({ check: 'bad_viewbox', detail: 'absent' });
  } else {
    const parts = viewBox[1].trim().split(/\s+/).map(Number);
    if (parts.length !== 4 || parts.some((n) => !Number.isFinite(n)) || parts[2] <= 0 || parts[3] <= 0) {
      violations.push({ check: 'bad_viewbox', detail: viewBox[1] });
    }
  }

  return violations;
}

function selfTest() {
  const cases = [];
  const record = (name, ok, detail) => {
    cases.push(name);
    if (!ok) throw new Error(`golden svg health self-test failed: ${name} (${JSON.stringify(detail)})`);
  };
  const wrap = (body) => `<svg viewBox="0 0 10 10">${body}</svg>`;
  const has = (svg, check) => svgHealthViolations(svg).some((v) => v.check === check);

  // A clean document must produce nothing, or every check below would be meaningless.
  record('a_healthy_document_has_no_violations',
    svgHealthViolations(wrap('<defs><marker id="m"/></defs><path marker-end="url(#m)"/>')).length === 0,
    svgHealthViolations(wrap('<defs><marker id="m"/></defs><path marker-end="url(#m)"/>')));

  // One case per check, each the exact shape it exists to catch.
  record('dangling_marker_reference_is_a_violation',
    has(wrap('<defs></defs><path marker-end="url(#arrowhead)"/>'), 'dangling_reference'), 'bd-8zo0 shape');
  record('dangling_href_reference_is_a_violation',
    has(wrap('<use href="#missing"/>'), 'dangling_reference'), 'href form');
  record('duplicate_id_is_a_violation',
    has(wrap('<rect id="a"/><rect id="a"/>'), 'duplicate_id'), 'two rects share an id');
  record('nan_coordinate_is_a_violation',
    has(wrap('<rect x="NaN" y="1"/>'), 'non_finite_number'), 'NaN x');
  record('infinite_dimension_is_a_violation',
    has(wrap('<rect width="Infinity"/>'), 'non_finite_number'), 'Infinity width');
  record('bare_ampersand_is_a_violation',
    has(wrap('<text>a &amp b</text>'), 'bare_ampersand'), 'unescaped &');
  record('escaped_entities_are_not_violations',
    !has(wrap('<text>a &amp; b &#39; c &#x27; d</text>'), 'bare_ampersand'), 'named, decimal and hex entities');
  record('degenerate_viewbox_is_a_violation',
    svgHealthViolations('<svg viewBox="0 0 0 10"></svg>').some((v) => v.check === 'bad_viewbox'), 'zero width');
  record('absent_viewbox_is_a_violation',
    svgHealthViolations('<svg></svg>').some((v) => v.check === 'bad_viewbox'), 'no viewBox');

  console.log(JSON.stringify({ ok: true, cases: cases.length }));
}

function scan() {
  let scanned = 0;
  const failures = [];
  for (const dir of CORPORA) {
    let names;
    try {
      names = readdirSync(dir).filter((n) => n.endsWith('.svg')).sort();
    } catch {
      continue; // A corpus that is not checked out is not a failure; the other still gates.
    }
    for (const name of names) {
      const path = join(dir, name);
      scanned += 1;
      for (const violation of svgHealthViolations(readFileSync(path, 'utf8'))) {
        failures.push(`${path}: ${violation.check} ${violation.detail}`);
      }
    }
  }
  if (scanned === 0) {
    console.error('[golden-health] no golden SVGs found in either corpus; refusing to report a pass');
    process.exit(2);
  }
  for (const failure of failures) console.error(`[golden-health] ${failure}`);
  console.log(JSON.stringify({ scanned, violations: failures.length }));
  process.exit(failures.length === 0 ? 0 : 1);
}

if (process.argv.includes('--self-test')) selfTest();
else scan();
