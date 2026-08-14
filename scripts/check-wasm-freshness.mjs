#!/usr/bin/env node
// Detect a stale published WASM package (bd-jj8d).
//
// `pkg/` is a committed build artifact that is ALSO gitignored — the root .gitignore lists `pkg/`,
// and wasm-pack regenerates a `pkg/.gitignore` containing `*`. Git therefore refuses to `add` it
// normally, `git status` never reports it dirty, and nothing reads it. It has drifted twice: ~2
// months before 2026-07-24, then 79 engine commits before 2026-08-08, at which point the published
// engine still rendered kanban as a single column and sized packet fields by label text.
//
// WHY THIS IS A CONTENT CHECK, NOT A TIMESTAMP ONE. Comparing the artifact's commit date against
// the engine crates' would fire on comment-only and test-only changes, which do not alter the
// rendered output. That produces false alarms, and the response to a false alarm is either an
// unnecessary 1.3 MB binary commit or learning to ignore the check — both worse than no check.
// Asserting BEHAVIOUR fires exactly when the artifact has regressed something the engine has fixed.
//
// WHAT IT DOES NOT CLAIM. This does not prove the artifact is byte-current with HEAD; it proves it
// still exhibits the properties listed below. Staleness that changes nothing observable here is not
// detected, and that is the intended trade: those are the cases where a rebuild buys nothing.
// Each assertion names the bead whose fix it pins, so the list grows as future fixes land.
//
// Usage: node scripts/check-wasm-freshness.mjs
// Exit 0 = the published package carries these fixes. Exit 1 = it is stale. Exit 2 = cannot run.

import { readFileSync, existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');
const PKG_JS = join(ROOT, 'pkg', 'frankenmermaid.js');
const PKG_WASM = join(ROOT, 'pkg', 'frankenmermaid_bg.wasm');

if (!existsSync(PKG_JS) || !existsSync(PKG_WASM)) {
  console.error(`cannot run: no published package at ${join(ROOT, 'pkg')}`);
  console.error('build it with ./build-wasm.sh, or this repo no longer vendors the artifact.');
  process.exit(2);
}

let renderSvg;
try {
  const packageModule = await import(PKG_JS);
  packageModule.initSync({ module: readFileSync(PKG_WASM) });
  ({ renderSvg } = packageModule);
} catch (err) {
  console.error(`cannot run: failed to load the published package: ${err.message}`);
  process.exit(2);
}

/** Left edge of every `<rect>` inside a node group whose id matches `idPattern`. */
function nodeRectXs(svg, idPattern) {
  const re = new RegExp(`<g id="fm-node-${idPattern}"[\\s\\S]{0,400}?<rect x="([\\d.]+)"`, 'g');
  return [...svg.matchAll(re)].map((m) => Number(m[1]));
}

/** Width of every `<rect>` inside a node group whose id matches `idPattern`. */
function nodeRectWidths(svg, idPattern) {
  const re = new RegExp(
    `<g id="fm-node-${idPattern}"[\\s\\S]{0,400}?<rect x="[\\d.]+" y="[\\d.]+" width="([\\d.]+)"`,
    'g',
  );
  return [...svg.matchAll(re)].map((m) => Number(m[1]));
}

const checks = [
  {
    bead: 'bd-eg44',
    what: 'kanban lanes are columns, not one stacked pile',
    run() {
      const svg = renderSvg('kanban\n  Todo\n    Task A\n    Task B\n  Doing\n    Task C\n    Task D\n');
      const xs = nodeRectXs(svg, 'task-[a-z]-\\d+');
      const columns = new Set(xs);
      if (xs.length < 4) return `read ${xs.length} cards, expected 4`;
      if (columns.size < 2) return `all ${xs.length} cards share x=${xs[0]}`;
      return null;
    },
  },
  {
    bead: 'bd-51tz',
    what: 'packet field width is its bit count, not its label length',
    run() {
      // Two 16-bit fields with very different label lengths must render identically wide.
      const svg = renderSvg('packet-beta\n0-15: "A"\n16-31: "An extremely long field name indeed"\n');
      const widths = nodeRectWidths(svg, 'pkt-field-\\d+-\\d+');
      if (widths.length < 2) return `read ${widths.length} fields, expected 2`;
      if (Math.abs(widths[0] - widths[1]) > 0.5) {
        return `two 16-bit fields render ${widths[0]} and ${widths[1]} wide`;
      }
      return null;
    },
  },
  {
    bead: 'bd-f3tc',
    what: "a requirement's declared id: and text: reach the output",
    run() {
      const svg = renderSvg(
        'requirementDiagram\n  requirement R {\n    id: REQ-001\n    text: Users must authenticate\n  }\n',
      );
      const missing = ['REQ-001', 'Users must authenticate'].filter((v) => !svg.includes(v));
      return missing.length ? `absent from the render: ${missing.join(', ')}` : null;
    },
  },
  {
    bead: 'bd-9w54',
    what: 'a composite state is drawn once, as its container',
    run() {
      const svg = renderSvg(
        'stateDiagram-v2\n  [*] --> Idle\n  state Processing {\n    Validating --> Computing\n  }\n  Idle --> Processing\n',
      );
      const drawn = [...svg.matchAll(/data-id="Processing"/g)].length;
      return drawn === 0 ? null : `composite state also drawn as a plain node (${drawn} occurrence(s))`;
    },
  },
];

let stale = 0;
for (const check of checks) {
  let failure;
  try {
    failure = check.run();
  } catch (err) {
    failure = `check threw: ${err.message}`;
  }
  if (failure) {
    stale += 1;
    console.error(`STALE  ${check.bead}  ${check.what}\n         ${failure}`);
  } else {
    console.log(`ok     ${check.bead}  ${check.what}`);
  }
}

if (stale > 0) {
  console.error(
    `\n${stale} of ${checks.length} checks failed: the published package in pkg/ predates fixes ` +
      'already landed in this repo.\nRegenerate it with ./build-wasm.sh and commit the result ' +
      '(the files are tracked but gitignored, so they need `git add -f` by explicit path).',
  );
  process.exit(1);
}

console.log(`\nall ${checks.length} checks passed: pkg/ carries the fixes it is pinned against.`);
