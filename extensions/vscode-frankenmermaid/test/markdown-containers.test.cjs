"use strict";

const assert = require("node:assert/strict");
const test = require("node:test");
const {
  extractMermaidBlocks, createRenderSnapshot, checkedSourceBindings, checkedDiagnostics,
  sourceSelectionTargets, planSourceEdit, planSourceBatch, MAX_PREVIEW_DIAGRAMS,
} = require("../preview-contract.cjs");

function documentFor(source, eol = 1) {
  return { fileName: "/work/architecture.md", languageId: "markdown", version: 7, eol,
    getText: () => source };
}
const sources = text => extractMermaidBlocks(documentFor(text)).map(block => block.source);

for (const [name, text, expected] of [
  ["quote fence", "> ```mermaid\n> flowchart LR\n> A-->B\n> ```\nprose", ["flowchart LR\nA-->B"]],
  ["nested quotes with optional spaces", ">> ~~~mermaid\n> >flowchart LR\n>> A-->B\n> > ~~~", ["flowchart LR\nA-->B"]],
  ["bullet item opener", "- ```mermaid\n  flowchart LR\n  A-->B\n  ```\n- prose", ["flowchart LR\nA-->B"]],
  ["ordered item after paragraph", "12) Architecture\n\n    ~~~mermaid\n    flowchart LR\n    A-->B\n    ~~~", ["flowchart LR\nA-->B"]],
  ["list inside quote", "> 1. Plan\n>\n>    ```mermaid\n>    flowchart LR\n>    A-->B\n>    ```", ["flowchart LR\nA-->B"]],
  ["quote inside list", "- > ```mermaid\n  > flowchart LR\n  > A-->B\n  > ```", ["flowchart LR\nA-->B"]],
  ["nested list", "- Plan\n  - Steps\n\n    ```mermaid\n    flowchart LR\n      A-->B\n    ```", ["flowchart LR\n  A-->B"]],
  ["sibling items keep separate diagrams", "- ~~~mermaid\n  A\n  ~~~\n- ~~~mermaid\n  B\n  ~~~", ["A", "B"]],
  ["dedent terminates unclosed list fence", "- ```mermaid\n  A\nprose\n```mermaid\nB\n```", ["A", "B"]],
  ["quote boundary terminates unclosed fence", "> ~~~mermaid\n> A\nprose\n~~~mermaid\nB\n~~~", ["A", "B"]],
  ["partial quote boundary reprocesses the line", "> > ~~~mermaid\n> > A\n> ~~~mermaid\n> B\n> ~~~", ["A", "B"]],
  ["non-Mermaid container fences suppress examples", "> - ````text\n>   ```mermaid\n>   A\n>   ```\n>   ````\n> ~~~mermaid\n> B\n> ~~~", ["B"]],
  ["body container-looking text is literal", "- ~~~~mermaid\n  > not a quote\n  - not a list\n  ~~~\n  ~~~~", ["> not a quote\n- not a list\n~~~"]],
  ["blank lines inside list code", "- ~~~mermaid\n  A\n\n  B\n  ~~~", ["A\n\nB"]],
  ["indented code inside list is not a fence", "- text\n\n      ```mermaid\n      A\n      ```", []],
  ["list marker in indented code is not a container", "    - ```mermaid\n      A\n      ```", []],
  ["list padding beyond four columns starts indented code", "-     ```mermaid\n      A\n      ```", []],
  ["ordered marker cannot interrupt paragraph except one", "prose\n2. ```mermaid\n   A\n   ```", []],
  ["one may interrupt a paragraph", "prose\n1. ```mermaid\n   A\n   ```", ["A"]],
  ["invalid backtick info never opens a fence", "> ```mermaid `bad`\n> A", []],
  ["top-level longer fences remain literal", "````text\n```mermaid\nA\n```\n````\n~~~mermaid\nB\n~~~", ["B"]],
  ["container fence indentation is removed only up to opener", "> -   ```mermaid\n>       A\n>     B\n>     ```", ["  A\nB"]],
]) {
  test(`Markdown containers: ${name}`, () => assert.deepEqual(sources(text), expected));
}

test("container line maps retain exact original UTF-16 positions and CRLF", () => {
  const text = "# Architecture\r\n> - ```mermaid\r\n>   flowchart LR\r\n>   A[😀 café]-->B\r\n>   ```\r\nUnrelated prose\r\n";
  const snapshot = createRenderSnapshot(documentFor(text, 2), 4, "architecture.md");
  const [block] = snapshot.blocks;
  assert.equal(block.source, "flowchart LR\nA[😀 café]-->B");
  assert.equal(block.startLine, 2);
  assert.deepEqual(block.lineMap, [
    { line: 2, indent: 4, text: "flowchart LR" },
    { line: 3, indent: 4, text: "A[😀 café]-->B" },
  ]);
  const snippet = "A[😀 café]";
  const startByte = Buffer.byteLength("flowchart LR\n");
  const bindings = checkedSourceBindings(block, [{ elementId: "fm-node-a-0", snippet,
    textRange: { startByte, endByte: startByte + Buffer.byteLength(snippet) } }]);
  assert.deepEqual(bindings.get("fm-node-a-0").range,
    { start: { line: 3, character: 4 }, end: { line: 3, character: 14 } });
  const diagnostic = checkedDiagnostics(block, [{ message: "example", severity: "warning",
    span: { start: { line: 2 }, end: { line: 2 } } }])[0];
  assert.deepEqual(diagnostic.range, { start: { line: 3, character: 4 }, end: { line: 3, character: 18 } });
  const reports = new Map([[0, { bindings }]]);
  assert.deepEqual(sourceSelectionTargets(reports, { start: { line: 3, character: 6 }, end: { line: 3, character: 6 } }),
    [{ diagramId: 0, elementId: "fm-node-a-0" }]);
  assert.equal(snapshot.message.diagrams[0].lineMap, undefined);
});

test("existing top-level maps remain unchanged", () => {
  const [block] = extractMermaidBlocks(documentFor("  ```mermaid\n  flowchart LR\n   A-->B\n  ```"));
  assert.equal(block.source, "flowchart LR\n A-->B");
  assert.deepEqual(block.lineMap, [
    { line: 1, indent: 2, text: "flowchart LR" },
    { line: 2, indent: 2, text: " A-->B" },
  ]);
});

test("nested inputs enforce diagram and nesting limits without silent truncation", () => {
  const diagram = "> ```mermaid\n> A\n> ```\n";
  assert.equal(sources(diagram.repeat(MAX_PREVIEW_DIAGRAMS)).length, MAX_PREVIEW_DIAGRAMS);
  assert.throws(() => sources(diagram.repeat(MAX_PREVIEW_DIAGRAMS + 1)), /at most 64/u);
  assert.throws(() => sources("> ".repeat(129) + "```mermaid\n"), /nesting/u);
});

function editFixture(text, fragments, eol = 1) {
  const document = documentFor(text, eol);
  const snapshot = createRenderSnapshot(document, 9, "architecture.md");
  const reports = new Map(snapshot.blocks.map((block, diagramId) => {
    const bindings = fragments.filter(fragment => fragment.diagramId === diagramId).map(({ elementId, snippet }) => {
      const offset = block.source.indexOf(snippet);
      assert.notEqual(offset, -1, "test fragment must exist in the extracted engine source");
      const startByte = Buffer.byteLength(block.source.slice(0, offset));
      return { elementId, snippet, textRange: { startByte, endByte: startByte + Buffer.byteLength(snippet) } };
    });
    return [diagramId, { bindings: checkedSourceBindings(block, bindings) }];
  }));
  // Boundary fixture for an engine receipt, not a replacement for a native ParseLens test.
  function replacement(diagramId, elementId, text) {
    const binding = reports.get(diagramId).bindings.get(elementId);
    const bytes = Buffer.from(snapshot.blocks[diagramId].source);
    return { diagramId, elementId, replacement: text,
      result: { elementId, previousSnippet: binding.snippet, replacement: text,
        replacedRange: { startByte: binding.startByte, endByte: binding.endByte },
        updatedSource: Buffer.concat([bytes.subarray(0, binding.startByte), Buffer.from(text), bytes.subarray(binding.endByte)]).toString() } };
  }
  return { document, snapshot, reports, replacement };
}

test("multiline fragment edits restore quote/list prefixes and preserve surrounding prose", () => {
  const text = "# Plan\r\n> - ```mermaid\r\n>   flowchart LR\r\n>   A[old]-->B\r\n>   ```\r\nUntouched prose 😀\r\n";
  const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A[old]" }], 2);
  const plan = planSourceEdit(f.document, f.snapshot, f.reports, f.replacement(0, "a", "A[new]\nC[extra]"));
  assert.equal(plan.after, text.replace("A[old]", "A[new]\r\n>   C[extra]"));
  assert.deepEqual(plan.range, { start: { line: 3, character: 4 }, end: { line: 3, character: 10 } });
  assert.equal(plan.newText, "A[new]\r\n>   C[extra]");
});

test("one batch safely edits separate nested diagrams", () => {
  const text = "> ~~~mermaid\n> A[old]\n> ~~~\n\n1. ~~~mermaid\n   B[old]\n   ~~~\nprose\n";
  const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A[old]" },
    { diagramId: 1, elementId: "b", snippet: "B[old]" }]);
  const plan = planSourceBatch(f.document, f.snapshot, f.reports, { edits: [
    f.replacement(0, "a", "A[new]\nC[extra]"), f.replacement(1, "b", "B[new]\nD[extra]"),
  ] });
  assert.equal(plan.after, text.replace("A[old]", "A[new]\n> C[extra]").replace("B[old]", "B[new]\n   D[extra]"));
  assert.equal(plan.edits.length, 2);
});

test("nested editing still rejects fence escape and forged engine ranges", () => {
  const f = editFixture("> ~~~mermaid\n> A\n> ~~~\nprotected prose\n", [{ diagramId: 0, elementId: "a", snippet: "A" }]);
  assert.throws(() => planSourceEdit(f.document, f.snapshot, f.reports, f.replacement(0, "a", "A\n~~~\nescape")), /fence/u);
  const forged = f.replacement(0, "a", "B");
  forged.result.replacedRange.startByte += 1;
  assert.throws(() => planSourceEdit(f.document, f.snapshot, f.reports, forged), /receipt/u);
  assert.equal(f.document.getText(), "> ~~~mermaid\n> A\n> ~~~\nprotected prose\n");
});

for (const [name, opening, closing] of [
  ["raw preformatted HTML", "<pre class='example'>", "</pre>"],
  ["HTML comments", "<!--", "-->"],
  ["processing instructions", "<?example", "?>"],
  ["declarations", "<!DOCTYPE", ">"],
  ["CDATA", "<![CDATA[", "]]>"],
  ["block tags", "<details>", "</details>\n"],
  ["complete custom tags", "<diagram-demo data-name='x > y' disabled>", "</diagram-demo>\n"],
]) {
  for (const prefix of ["", "> ", ">   "]) {
    test(`literal ${name} under ${JSON.stringify(prefix)} never becomes a diagram`, () => {
      const nested = prefix === ">   " ? "> - context\n>\n" : "";
      const literal = [opening, "~~~mermaid", "not a live diagram", "~~~", ...closing.split("\n")];
      const text = nested + literal.map(line => prefix + line).join("\n") + "\n" +
        ["~~~mermaid", "A", "~~~"].map(line => prefix + line).join("\n");
      assert.deepEqual(sources(text), ["A"]);
    });
  }
}

test("raw HTML ends at its container boundary without swallowing a sibling", () => {
  assert.deepEqual(sources("> <pre>\n> ~~~mermaid\n> literal\n~~~mermaid\nA\n~~~"), ["A"]);
  assert.deepEqual(sources("- <!--\n  literal\n~~~mermaid\nA\n~~~"), ["A"]);
});

test("type-seven HTML cannot interrupt a paragraph; inline HTML remains ordinary text", () => {
  assert.deepEqual(sources("paragraph\n<span>\n~~~mermaid\nA\n~~~"), ["A"]);
  assert.deepEqual(sources("<span> inline\n~~~mermaid\nA\n~~~"), ["A"]);
  assert.deepEqual(sources("<span>\n~~~mermaid\nA\n~~~"), []);
});

test("HTML block syntax inside Mermaid fences remains native input", () => {
  assert.deepEqual(sources("> ~~~mermaid\n> <pre>\n> <!--\n> A[<b>label</b>]\n> ~~~"), ["<pre>\n<!--\nA[<b>label</b>]"]);
});

test("invalid and incomplete custom tags do not hide subsequent diagrams", () => {
  for (const start of ["<x invalid= >", "<x a='unterminated>", "<x a='v'b='w'>", "</x attr>", "<x/ >"]) {
    assert.deepEqual(sources(start + "\n~~~mermaid\nA\n~~~"), ["A"], start);
  }
});

function structuralReceipt(fixture, operation, elementId, text, from, to, replacement, diagramId = 0) {
  const source = fixture.snapshot.blocks[diagramId].source;
  return { diagramId, elementId, operation, replacement: text, result: {
    elementId, replacedRange: { startByte: Buffer.byteLength(source.slice(0, from)),
      endByte: Buffer.byteLength(source.slice(0, to)) },
    previousSnippet: source.slice(from, to), replacement,
    updatedSource: source.slice(0, from) + replacement + source.slice(to),
  } };
}

function assertNativeSplice(plan) {
  const starts = [0];
  for (const match of plan.before.matchAll(/\r\n|\n|\r/gu)) starts.push(match.index + match[0].length);
  const from = starts[plan.range.start.line] + plan.range.start.character;
  const to = starts[plan.range.end.line] + plan.range.end.character;
  assert.equal(plan.before.slice(0, from) + plan.newText + plan.before.slice(to), plan.after);
}

for (const [opening, prefix] of [["> ~~~mermaid", "> "], ["- ~~~mermaid", "  "],
  ["> 12) ~~~mermaid", ">     "], ["- > ~~~mermaid", "  > "]]) {
  for (const eol of ["\n", "\r\n"]) {
    test(`structural insert/delete preserves ${JSON.stringify(opening)} with ${JSON.stringify(eol)}`, () => {
      const text = ["# Before", opening, prefix + "A", prefix + "B", prefix + "~~~", "After 😀"].join(eol);
      const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A" },
        { diagramId: 0, elementId: "b", snippet: "B" }], eol === "\r\n" ? 2 : 1);
      for (const [receipt, expected] of [
        [structuralReceipt(f, "delete", "a", "", 0, 2, ""), text.replace(prefix + "A" + eol, "")],
        [structuralReceipt(f, "delete", "b", "", 2, 3, ""), text.replace(prefix + "B" + eol, "")],
        [structuralReceipt(f, "insert-after", "a", "X\nY", 2, 2, "X\nY\n"),
          text.replace(prefix + "B", prefix + "X" + eol + prefix + "Y" + eol + prefix + "B")],
        [structuralReceipt(f, "insert-after", "b", "X", 3, 3, "\nX\n"),
          text.replace(prefix + "B", prefix + "B" + eol + prefix + "X")],
      ]) {
        const plan = planSourceEdit(f.document, f.snapshot, f.reports, receipt);
        assert.equal(plan.after, expected);
        assertNativeSplice(plan);
      }
    });
  }
}

test("partial-tab mappings allow exact node edits and whole-line structural patches", () => {
  const text = "> ~~~mermaid\n>\tA[😀]\n> ~~~";
  const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A[😀]" }]);
  assert.equal(f.snapshot.blocks[0].source, "  A[😀]");
  assert.deepEqual(f.snapshot.blocks[0].lineMap[0], { line: 1, indent: 2, text: "  A[😀]", virtualIndent: 2 });
  assert.deepEqual(f.reports.get(0).bindings.get("a").range,
    { start: { line: 1, character: 2 }, end: { line: 1, character: 7 } });
  const replace = planSourceEdit(f.document, f.snapshot, f.reports, f.replacement(0, "a", "B[😀]"));
  assert.equal(replace.after, text.replace("A[😀]", "B[😀]"));
  const remove = planSourceEdit(f.document, f.snapshot, f.reports, structuralReceipt(f, "delete", "a", "", 0, 7, ""));
  assert.equal(remove.after, "> ~~~mermaid\n> ~~~");
  const insert = planSourceEdit(f.document, f.snapshot, f.reports, structuralReceipt(f, "insert-after", "a", "B", 7, 7, "\n  B\n"));
  assert.equal(insert.after, "> ~~~mermaid\n>\tA[😀]\n>   B\n> ~~~");
  for (const plan of [replace, remove, insert]) assertNativeSplice(plan);
  assert.throws(() => checkedSourceBindings(f.snapshot.blocks[0], [{ elementId: "tab-half", snippet: " ",
    textRange: { startByte: 0, endByte: 1 } }]), /document positions/u);
});

test("structural insertion before a partially consumed tab preserves its physical prefix", () => {
  const text = "> ~~~mermaid\n> A\n>\tB\n> ~~~";
  const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A" }]);
  const plan = planSourceEdit(f.document, f.snapshot, f.reports,
    structuralReceipt(f, "insert-after", "a", "X", 2, 2, "X\n"));
  assert.equal(plan.after, "> ~~~mermaid\n> A\n> X\n>\tB\n> ~~~");
  assertNativeSplice(plan);
});

test("ordinary Mermaid file bytes and injected scheduler cancellation stay unchanged", () => {
  const text = "flowchart LR\r\nA[😀]-->B\r\n";
  const f = { languageId: "mermaid", fileName: "x.mmd", getText: () => text };
  assert.equal(extractMermaidBlocks(f)[0].source, text);
  const { DebouncedRenderScheduler } = require("../preview-contract.cjs");
  const cancelled = [];
  const scheduler = new DebouncedRenderScheduler(0, () => 37, id => cancelled.push(id));
  scheduler.schedule(() => {});
  scheduler.cancel();
  assert.deepEqual(cancelled, [37]);
});

test("self-closing raw-tag names are not complete-tag HTML blocks", () => {
  for (const tag of ["pre", "script", "style", "textarea"]) {
    assert.deepEqual(sources(`<${tag}/>\n~~~mermaid\nA\n~~~`), ["A"]);
  }
});

test("a mixed structural batch commits only intended container content", () => {
  const text = "> - ~~~mermaid\n>   A\n>   B\n>   C\n>   ~~~\n\n~~~mermaid\nOther\n~~~";
  const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A" },
    { diagramId: 0, elementId: "b", snippet: "B" }, { diagramId: 0, elementId: "c", snippet: "C" }]);
  const plan = planSourceBatch(f.document, f.snapshot, f.reports, { edits: [
    structuralReceipt(f, "delete", "a", "", 0, 2, ""),
    f.replacement(0, "b", "Beta"),
    structuralReceipt(f, "insert-after", "c", "New", 5, 5, "\nNew\n"),
  ] });
  assert.equal(plan.after, "> - ~~~mermaid\n>   Beta\n>   C\n>   New\n>   ~~~\n\n~~~mermaid\nOther\n~~~");
  assert.equal(plan.edits.length, 3);
});

test("deleting the final node of an unclosed container preserves outside prose", () => {
  const text = "> ~~~mermaid\n> A\nOutside prose\n\n~~~mermaid\nOther\n~~~";
  const f = editFixture(text, [{ diagramId: 0, elementId: "a", snippet: "A" }]);
  const plan = planSourceEdit(f.document, f.snapshot, f.reports,
    structuralReceipt(f, "delete", "a", "", 0, 1, ""));
  assert.equal(plan.after, text.replace("> A\n", ""));
  assertNativeSplice(plan);
});

test("CommonMark declarations accept lowercase ASCII letters as well as uppercase", () => {
  assert.deepEqual(sources("<!doctype\n~~~mermaid\nnot live\n~~~\n>\n~~~mermaid\nA\n~~~"), ["A"]);
});
