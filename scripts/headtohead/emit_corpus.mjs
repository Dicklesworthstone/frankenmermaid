// Emit ONE head-to-head corpus item as the JSON the `headtohead` example consumes.
//
// run.mjs builds this shape inline and then enforces its admission gates before it will run the
// binary. On a saturated host those gates refuse (correctly), but the corpus itself is pure
// deterministic generation with no timing in it, so it can be materialised on its own and fed to a
// load-immune instruction-counted A/B.
//
// Shape must stay `{id, texts, reps, warmup}` — `texts`, plural. An earlier session lost hours to a
// corpus whose container field had moved from `text` to `texts` while the payload hash was
// unchanged, so the input preflight passed on a corpus the binary could not read.
import { generateAll } from './corpus.mjs';
import fs from 'node:fs';

const id = process.argv[2];
const reps = Number(process.argv[3] ?? 20);
const warmup = Number(process.argv[4] ?? 2);
const out = process.argv[5];
// generateAll(), not generate(): run.mjs reads `corpus.get(id).texts` off exactly this map, and its
// sha256 is over REVISION_SEP-joined text. Reproducing the harness's own accessor is what makes the
// input hash below comparable to a harness row instead of a second, differently-joined corpus.
const built = generateAll().get(id);
if (!built) {
  console.error(`no corpus item ${id}`);
  process.exit(2);
}
const { texts } = built;
fs.writeFileSync(out, JSON.stringify([{ id, texts, reps, warmup }]));
console.log(`${id}: ${texts.length} documents, ${built.bytes} bytes`);
console.log(`joined_input_sha256=${built.sha256}`);
console.log(`reps=${reps} warmup=${warmup} -> ${out}`);
