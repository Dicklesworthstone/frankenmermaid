// Host/protocol tests: execute the real worker, substituting only the WASM boundary.
// These prove transaction/cancellation/UTF-8 behavior, not Mermaid grammar correctness.
// Run: node --test web/fm-source-batch.test.cjs
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");
const { test } = require("node:test");

const SOURCE = "flowchart TB\r\n%% preserve this comment\r\nA[old]\r\nB[two]\r\n";
const bytes = (text) => Buffer.byteLength(text, "utf8");
const plain = (value) => JSON.parse(JSON.stringify(value));

function snapshot(input) {
  const bindings = [...input.matchAll(/^([A-Z])\[([^\r\n]*)\]/gm)].map((match) => ({
    // Deliberately change every element ID when source length changes. The next batch step
    // must use a freshly rebound ID, not blindly replay the original request's IDs.
    elementId: `node-${match[1]}-${input.length}`,
    sourceId: match[1], kind: "node", snippet: match[0],
    textRange: { startByte: bytes(input.slice(0, match.index)),
      endByte: bytes(input.slice(0, match.index + match[0].length)) },
  }));
  return { bindings, parsed: { warnings: ["fixture diagnostic"], ir: { shouldNotCrossWorker: true } } };
}

function host(options = {}) {
  const replies = [];
  const calls = [];
  const parses = [];
  const jobs = [];
  let instance;
  const module = {
    chooseCanvasTarget: () => JSON.stringify({ target: "svgInWorker" }),
    workerHandleMessage: (json) => {
      const message = JSON.parse(json);
      return JSON.stringify({ kind: "completed", requestId: message.requestId, svg: "<svg/>" });
    },
    renderSvg: () => "<svg/>",
    parseLens(input) {
      parses.push(input);
      return options.initialSnapshot ? options.initialSnapshot(input) : snapshot(input);
    },
    applyParseLensEdit(input, elementId, replacement) {
      calls.push({ input, elementId, replacement });
      const binding = snapshot(input).bindings.find((entry) => entry.elementId === elementId);
      assert.ok(binding, "worker must use a current engine binding");
      const start = Buffer.from(input).subarray(0, binding.textRange.startByte).toString().length;
      const end = Buffer.from(input).subarray(0, binding.textRange.endByte).toString().length;
      const updatedSource = input.slice(0, start) + replacement + input.slice(end);
      const response = { result: { updatedSource, elementId, replacement,
        previousSnippet: binding.snippet, replacedRange: binding.textRange },
      snapshot: snapshot(updatedSource) };
      options.onEdit?.(instance, calls.length, response);
      return options.editResponse ? options.editResponse(response, calls.length) : response;
    },
    ...options.exports,
  };
  const context = { setTimeout, clearTimeout,
    __import: async () => options.importPromise ? await options.importPromise : module,
    self: { postMessage: (message) => replies.push(plain(message)) } };
  const file = process.env.FM_WORKER_SOURCE || path.join(__dirname, "fm-render.worker.js");
  const code = fs.readFileSync(file, "utf8").replaceAll("import(", "globalThis.__import(");
  vm.runInNewContext(code, context, { filename: file });
  instance = {
    module, replies, calls, parses,
    send(message) {
      const job = context.self.onmessage({ data: message });
      jobs.push(job);
      return job;
    },
    async finish() {
      // Hooks can add new requests while earlier jobs are awaiting cooperative yields.
      for (let index = 0; index < jobs.length; index += 1) await jobs[index];
    },
  };
  return instance;
}

function batch(input = SOURCE, replacements = { A: "A[first]", B: "B[second]" }, requestId = 1) {
  const bindings = snapshot(input).bindings;
  return { kind: "sourceBatch", requestId, input,
    edits: Object.entries(replacements).map(([id, replacement]) => ({
      elementId: bindings.find((entry) => entry.sourceId === id).elementId, replacement,
    })) };
}

function onlySuccess(worker) {
  assert.equal(worker.replies.length, 1, JSON.stringify(worker.replies));
  assert.equal(worker.replies[0].kind, "sourceBatchEdited", JSON.stringify(worker.replies));
  return worker.replies[0].response;
}

function onlyFailure(worker, reason) {
  assert.equal(worker.replies.length, 1, JSON.stringify(worker.replies));
  assert.equal(worker.replies[0].kind, "failed", JSON.stringify(worker.replies));
  assert.match(worker.replies[0].reason, reason);
  assert.equal(worker.replies[0].response, undefined, "no partially updated source may escape");
}

test("batch applies right-to-left, rebinds changed IDs, and emits one final source", async () => {
  const worker = host();
  const request = batch();
  await worker.send(request);
  const response = onlySuccess(worker);
  assert.equal(response.result.updatedSource, SOURCE.replace("A[old]", "A[first]").replace("B[two]", "B[second]"));
  assert.deepEqual(worker.calls.map((call) => call.replacement), ["B[second]", "A[first]"]);
  assert.notEqual(worker.calls[1].elementId, request.edits[0].elementId);
  assert.deepEqual(response.result.changes.map((change) => change.elementId), request.edits.map((edit) => edit.elementId));
  assert.equal(response.result.changes[0].appliedElementId, worker.calls[1].elementId);
  assert.deepEqual(response.snapshot.parsed, { warnings: ["fixture diagnostic"] });
  assert.deepEqual(response.snapshot.bindings, snapshot(response.result.updatedSource).bindings);
  assert.ok(response.result.changes.every((change) => !("updatedSource" in change)));
});

test("Unicode, emoji, combining marks, CRLF and comments remain byte-exact outside edits", async () => {
  const input = "%% café 🦀\r\nflowchart TB\r\nA[é👩‍💻]\r\nB[é]\r\n%% tail\r\n";
  const worker = host();
  await worker.send(batch(input, { A: "A[日本語]", B: "B[😀 café]" }));
  const result = onlySuccess(worker).result;
  assert.equal(result.updatedSource, input.replace("A[é👩‍💻]", "A[日本語]").replace("B[é]", "B[😀 café]"));
  assert.equal(result.changes[0].replacedRange.startByte, bytes(input.slice(0, input.indexOf("A["))));
});

test("empty and all-no-op batches preserve exact source without invoking edits", async () => {
  for (const replacements of [{}, { A: "A[old]", B: "B[two]" }]) {
    const worker = host();
    await worker.send(batch(SOURCE, replacements));
    const response = onlySuccess(worker);
    assert.equal(response.result.updatedSource, SOURCE);
    assert.deepEqual(response.result.changes, []);
    assert.equal(worker.calls.length, 0);
  }
});

test("duplicate requested IDs fail before parsing or applying any edit", async () => {
  const worker = host();
  const request = batch();
  request.edits.push({ ...request.edits[0] });
  await worker.send(request);
  onlyFailure(worker, /Duplicate sourceBatch/);
  assert.equal(worker.parses.length, 0);
  assert.equal(worker.calls.length, 0);
});

test("missing bindings fail before any edit", async () => {
  const worker = host();
  const request = batch();
  request.edits[0].elementId = "missing";
  await worker.send(request);
  onlyFailure(worker, /No editable source span/);
  assert.equal(worker.calls.length, 0);
});

test("shared statement spans and containment are rejected before any edit", async () => {
  for (const contained of [false, true]) {
    const worker = host({ initialSnapshot(input) {
      const value = snapshot(input);
      const original = value.bindings[0];
      value.bindings.push({ ...original, elementId: "shared-edge", kind: "edge",
        snippet: contained ? "old" : original.snippet,
        textRange: contained ? { startByte: original.textRange.startByte + 2,
          endByte: original.textRange.endByte - 1 } : original.textRange });
      return value;
    } });
    const request = batch(SOURCE, { A: "A[new]" });
    request.edits.push({ elementId: "shared-edge", replacement: "edge" });
    await worker.send(request);
    onlyFailure(worker, /Overlapping source spans/);
    assert.equal(worker.calls.length, 0);
  }
});

test("a later parse invalidating an earlier binding aborts rather than guessing", async () => {
  const worker = host({ editResponse(response) {
    response.snapshot.bindings = response.snapshot.bindings.filter((binding) => binding.sourceId !== "A");
    return response;
  } });
  await worker.send(batch());
  onlyFailure(worker, /Source binding changed/);
  assert.equal(worker.calls.length, 1);
});

test("ambiguous rebindings abort instead of selecting the first candidate", async () => {
  const worker = host({ editResponse(response) {
    const first = response.snapshot.bindings.find((binding) => binding.sourceId === "A");
    response.snapshot.bindings.push({ ...first, elementId: "duplicate-identity" });
    return response;
  } });
  await worker.send(batch());
  onlyFailure(worker, /became ambiguous/);
  assert.equal(worker.calls.length, 1);
});

test("a forged update outside the selected span is never returned", async () => {
  const worker = host({ editResponse(response) {
    response.result.updatedSource += "\nUNREQUESTED";
    return response;
  } });
  await worker.send(batch());
  onlyFailure(worker, /outside its selected span/);
  assert.equal(worker.calls.length, 1);
});

test("incorrect transaction metadata is rejected even if updatedSource is plausible", async () => {
  for (const field of ["elementId", "previousSnippet", "replacement", "replacedRange"]) {
    const worker = host({ editResponse(response) {
      response.result[field] = field === "replacedRange" ? { startByte: 0, endByte: 0 } : "wrong";
      return response;
    } });
    await worker.send(batch());
    onlyFailure(worker, /invalid transaction/);
  }
});

test("malformed UTF-8 ranges and mismatched snippets are rejected", async () => {
  const input = "flowchart TB\nA[é]\nB[ok]\n";
  for (const invalid of ["interior-byte", "out-of-bounds", "snippet", "duplicate-id"]) {
    const worker = host({ initialSnapshot(source) {
      const value = snapshot(source);
      if (invalid === "interior-byte") {
        value.bindings[0].textRange.endByte = bytes(source.slice(0, source.indexOf("é"))) + 1;
      } else if (invalid === "out-of-bounds") {
        value.bindings[0].textRange.endByte = 1e9;
      } else if (invalid === "snippet") {
        value.bindings[0].snippet = "wrong";
      } else {
        value.bindings.push({ ...value.bindings[0] });
      }
      return value;
    } });
    await worker.send(batch(input));
    onlyFailure(worker, /invalid UTF-8|do not match|Duplicate source-map/);
    assert.equal(worker.calls.length, 0);
  }
});

test("fresh bindings are validated before an otherwise successful batch can commit", async () => {
  const worker = host({ editResponse(response) {
    response.snapshot.bindings[0].snippet = "not in source";
    return response;
  } });
  await worker.send(batch());
  onlyFailure(worker, /bindings do not match/);
});

test("unpaired surrogates in source or any replacement fail without editing", async () => {
  for (const badSource of [false, true]) {
    const worker = host();
    const request = batch();
    if (badSource) request.input += "\ud800";
    else request.edits[0].replacement += "\ud800";
    await worker.send(request);
    onlyFailure(worker, /unpaired surrogate/);
    assert.equal(worker.calls.length, 0);
    assert.equal(worker.parses.length, 0);
  }
});

test("cancellation between edits emits no partial result and stops further WASM work", async () => {
  const worker = host({ onEdit(instance, number) {
    if (number === 1) queueMicrotask(() => instance.send({ kind: "cancel", requestId: 1 }));
  } });
  await worker.send(batch());
  await worker.finish();
  assert.deepEqual(worker.replies, [{ kind: "noReply", requestId: 1 }]);
  assert.equal(worker.calls.length, 1);
});

test("a new source render supersedes an in-progress batch without leaking its partial source", async () => {
  const worker = host({ onEdit(instance, number) {
    if (number === 1) queueMicrotask(() => instance.send({ kind: "sourceRender", requestId: 2, input: "flowchart LR\nZ[new]" }));
  } });
  await worker.send(batch());
  await worker.finish();
  assert.deepEqual(worker.replies.map(({ kind, requestId }) => ({ kind, requestId })), [
    { kind: "noReply", requestId: 1 }, { kind: "sourceRendered", requestId: 2 },
  ]);
  assert.equal(worker.calls.length, 1);
});

test("reinitialization cancels a batch and advertises the additive capability", async () => {
  const worker = host({ onEdit(instance, number) {
    if (number === 1) queueMicrotask(() => instance.send({ kind: "init" }));
  } });
  await worker.send(batch());
  await worker.finish();
  assert.deepEqual(worker.replies.map((message) => message.kind), ["noReply", "ready"]);
  assert.equal(worker.replies[1].sourceBatchEditing, true);
  assert.equal(worker.calls.length, 1);
});

test("cancelled request ID reuse cannot revive the old batch", async () => {
  const worker = host({ onEdit(instance, number) {
    if (number !== 1) return;
    queueMicrotask(() => {
      instance.send({ kind: "cancel", requestId: 1 });
      instance.send(batch("flowchart LR\nA[new document]\n", { A: "A[latest]" }, 1));
    });
  } });
  await worker.send(batch());
  await worker.finish();
  assert.deepEqual(worker.replies.map((message) => message.kind), ["noReply", "sourceBatchEdited"]);
  assert.equal(worker.replies[1].response.result.updatedSource, "flowchart LR\nA[latest]\n");
  assert.equal(worker.calls.length, 2);
});

test("a cold import coalesces obsolete batches before any edit runs", async () => {
  let resolveImport;
  const importPromise = new Promise((resolve) => { resolveImport = resolve; });
  const worker = host({ importPromise });
  const first = worker.send(batch());
  const second = worker.send(batch(SOURCE, { A: "A[latest]" }, 2));
  resolveImport(worker.module);
  await Promise.all([first, second]);
  assert.deepEqual(worker.replies.map((message) => message.kind), ["noReply", "sourceBatchEdited"]);
  assert.equal(worker.replies[1].requestId, 2);
  assert.equal(worker.calls.length, 1);
});

test("invalid batch schemas do not evict a valid pending operation", async () => {
  const worker = host();
  const valid = worker.send(batch(SOURCE, { A: "A[latest]" }));
  const invalid = worker.send({ kind: "sourceBatch", requestId: 2, input: SOURCE, edits: null });
  await Promise.all([valid, invalid]);
  assert.equal(worker.replies.find((message) => message.requestId === 2).kind, "failed");
  assert.equal(worker.replies.find((message) => message.requestId === 1).kind, "sourceBatchEdited");
  assert.equal(worker.calls.length, 1);
});

test("oversized batches are rejected before importing/parsing/editing", async () => {
  const worker = host();
  await worker.send({ kind: "sourceBatch", requestId: 1, input: SOURCE,
    edits: Array.from({ length: 1025 }, (_, index) => ({ elementId: `node-${index}`, replacement: "" })) });
  onlyFailure(worker, /at most 1024/);
  assert.equal(worker.parses.length, 0);
  assert.equal(worker.calls.length, 0);
});

test("older WASM builds fail explicitly instead of falling through to a different operation", async () => {
  const worker = host({ exports: { applyParseLensEdit: undefined } });
  await worker.send(batch());
  onlyFailure(worker, /lacks source-editing APIs/);
});

test("a Rust edit exception after an earlier successful step never publishes a partial batch", async () => {
  const worker = host({ onEdit(_instance, number) {
    if (number === 2) throw new Error("engine rejected second edit");
  } });
  await worker.send(batch());
  onlyFailure(worker, /engine rejected second edit/);
  assert.equal(worker.calls.length, 2);
});

test("empty replacements remove only the selected span, preserving line framing", async () => {
  const worker = host();
  await worker.send(batch(SOURCE, { A: "A[new]", B: "" }));
  const response = onlySuccess(worker);
  assert.equal(response.result.updatedSource, SOURCE.replace("A[old]", "A[new]").replace("B[two]", ""));
  assert.equal(response.result.changes[1].replacement, "");
  assert.ok(response.result.updatedSource.endsWith("\r\n\r\n"));
});
