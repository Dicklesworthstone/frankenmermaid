// Digest the wasm32 render output for the bd-1s1g.6 cross-target determinism comparison.
//
// WHY THIS EXISTS: the bead asks for output compared ACROSS x86_64, aarch64 and wasm32, and every
// determinism test in this project until now ran on ONE target and said so. This host has no qemu
// and no wasmtime, but `pkg/` is a real wasm32-unknown-unknown build of fm-wasm and node can drive
// it, so wasm32 is reachable without either.
//
// The Rust half is `x86_64_and_wasm32_render_the_same_bytes` in crates/fm-wasm/src/lib.rs, which
// asserts the digests this script prints. Run this, then that test: agreement means the two targets
// produced identical bytes for the same input.
//
// ⚠️ REBUILD THE BUNDLE FIRST. `pkg/` is a committed artifact and has drifted months behind source
// before (see the size-budget notes in build-wasm.sh). A digest taken from a stale bundle compares
// today's Rust against whenever pkg/ was last generated, which is a different question and looks
// exactly like a pass or a failure depending on luck:
//
//     env -u CARGO_TARGET_DIR bash build-wasm.sh
//
// ⚠️ BOTH SIDES MUST CALL THE SAME ENTRY POINT. This calls `renderSvg(source)` with no config
// argument; the Rust half calls `render_svg_js(source, None)`. They are the same function. Do NOT
// compare against `render()`, which reads the runtime config directly instead of merging an
// override and a theme -- digesting one against the other reports a FUNCTION difference as a
// cross-target divergence.
//
// FNV-1a rather than any built-in hash, matching the Rust side byte for byte: node's and Rust's
// default hashers are unrelated, and Rust's is not stable across versions or targets, so a
// "cross-target golden" built on either would differ for reasons that have nothing to do with
// floating point.
import * as fm from '../pkg/frankenmermaid.js';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const here = dirname(fileURLToPath(import.meta.url));
fm.initSync({ module: readFileSync(join(here, '..', 'pkg', 'frankenmermaid_bg.wasm')) });

// Assembled rather than written literally so the source of this file contains no bare arrow
// sequences: the fleet's command guard reads `--` followed by `>` in a shell argument as a
// redirect, which has blocked commands that merely quoted a diagram.
const ARROW = '-' + '-' + '>';

// The same four fixtures as crates/fm-cli/tests/layout_fp_determinism.rs, so a divergence found
// here can be traced with the coordinate digests that file already pins.
const DIAGRAMS = [
  ['flowchart', `flowchart TD\n  a[Alpha] ${ARROW} b[Beta]\n  b ${ARROW} c[Gamma]\n  c -.${ARROW} a\n  b ${ARROW} d[Delta]\n`],
  ['sequence', 'sequenceDiagram\n  participant A\n  participant B\n  A-' + '>>B: hello\n  B--' + '>>A: reply\n'],
  ['class', 'classDiagram\n  class Alpha {\n    +String name\n    +run()\n  }\n  Alpha <|-- Beta\n'],
  ['state', `stateDiagram-v2\n  [*] ${ARROW} Idle\n  Idle ${ARROW} Busy: start\n  Busy ${ARROW} Idle: done\n`],
];

function fnv1a(bytes) {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = (1n << 64n) - 1n;
  for (const b of bytes) {
    hash ^= BigInt(b);
    hash = (hash * prime) & mask;
  }
  return hash;
}

let vacuous = false;
for (const [name, source] of DIAGRAMS) {
  const svg = fm.renderSvg(source);
  const bytes = new TextEncoder().encode(String(svg));
  // A bundle that renders nothing would agree with a Rust side that also rendered nothing, and the
  // comparison would pass while proving neither target does anything.
  if (bytes.length < 1000) {
    console.error(`${name}: rendered only ${bytes.length} bytes -- the comparison would be vacuous`);
    vacuous = true;
  }
  console.log(`${name} bytes=${bytes.length} digest=0x${fnv1a(bytes).toString(16).padStart(16, '0')}`);
}
process.exit(vacuous ? 1 : 0);
