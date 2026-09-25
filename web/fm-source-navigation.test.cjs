"use strict";
// Browser navigation math; these tests do not fixture or claim Rust parser behavior.
const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const loaded = import(`data:text/javascript;base64,${Buffer.from(fs.readFileSync(path.join(__dirname, "fm-source-editor.js"))).toString("base64")}`);
const box = (elementId, x, y, width = 10, height = 10) => ({ elementId, rect: { x, y, width, height } });

test("search combines literal terms across identity, source and rendered labels", async () => {
  const { searchSourceBindings: find } = await loaded;
  const items = [
    { elementId: "fm-node-a-0", sourceId: "auth", kind: "node", snippet: "A[Login]" },
    { elementId: "fm-edge-0", kind: "edge", snippet: "A --> B" },
    { elementId: "fm-cluster-0", kind: "cluster", snippet: "subgraph secure" },
  ];
  assert.deepEqual(find(items, "AUTH login"), [items[0]]);
  assert.deepEqual(find(items, "a -->", "edge"), [items[1]]);
  assert.deepEqual(find(items, "", "cluster"), [items[2]]);
  assert.deepEqual(find(items, "sign in", "all", new Map([[items[0].elementId, "Sign in now"]])), [items[0]]);
  assert.deepEqual(find(items, "[Login]"), [items[0]]);
  assert.deepEqual(find(items, "[.*]"), []);
  assert.deepEqual(find(items, " \n "), items);
});

test("search normalizes composed Unicode and compatibility characters without altering source", async () => {
  const { searchSourceBindings: find } = await loaded;
  const items = [{ elementId: "a", snippet: "Cafe\u0301 世界 🐉 ＡＰＩ", kind: "Node" }];
  assert.deepEqual(find(items, "CAFÉ api 🐉", "node"), items);
  assert.equal(items[0].snippet, "Cafe\u0301 世界 🐉 ＡＰＩ");
  assert.deepEqual(find(items, "absent"), []);
});

test("four directions select screen-space neighbors, not source-order neighbors", async () => {
  const { directionalSourceBinding: next } = await loaded;
  const items = [box("bottom", 0, 50), box("right", 50, 0), box("here", 0, 0), box("left", -50, 0), box("top", 0, -50)];
  for (const [key, id] of [["ArrowRight", "right"], ["ArrowLeft", "left"], ["ArrowUp", "top"], ["ArrowDown", "bottom"]]) {
    assert.equal(next(items, "here", key), id);
  }
});

test("aligned boxes precede diagonals, with nearest aligned candidate first", async () => {
  const { directionalSourceBinding: next } = await loaded;
  const items = [box("here", 0, 0), box("diagonal", 10, 30), box("far", 100, 0), box("near", 60, 4)];
  assert.equal(next(items, "here", "ArrowRight"), "near");
  assert.equal(next(items.slice(0, 3), "here", "ArrowRight"), "far");
});

test("directional ties preserve input order and never wrap behind the origin", async () => {
  const { directionalSourceBinding: next } = await loaded;
  const items = [box("here", 0, 0), box("first", 30, -30), box("second", 30, 30), box("behind", -10, 0)];
  assert.equal(next(items, "here", "ArrowRight"), "first");
  assert.equal(next(items, "behind", "ArrowLeft"), null);
  assert.equal(next([box("same", 0, 0), box("here", 0, 0)], "here", "ArrowRight"), null);
});

test("invalid geometry cannot win navigation; zero-width edges remain navigable", async () => {
  const { directionalSourceBinding: next } = await loaded;
  const items = [box("here", 0, 0), box("nan", NaN, 0), box("negative", 20, 0, -5), box("empty", 20, 0, 0, 0), box("edge", 40, 0, 0, 50)];
  assert.equal(next(items, "here", "ArrowRight"), "edge");
  assert.equal(next(items, "nan", "ArrowRight"), null);
  assert.equal(next(items, "absent", "ArrowRight"), null);
  assert.equal(next(items, "here", "invalid"), null);
});

test("navigation is invariant under positive scale and translation", async () => {
  const { directionalSourceBinding: next } = await loaded;
  const items = [box("here", -20, -10), box("winner", 20, -10), box("diagonal", 10, 50)];
  for (const scale of [0.01, 1, 800]) {
    const transformed = items.map(({ elementId, rect: r }) => box(elementId, r.x * scale + 90, r.y * scale - 35, r.width * scale, r.height * scale));
    assert.equal(next(transformed, "here", "ArrowRight"), "winner");
  }
});
