import fs from 'node:fs';
import { JSDOM } from 'jsdom';
const BUNDLE = '/home/ubuntu/.cache/fm-headtohead/mermaid-11.15.0.min.js';
const dom = new JSDOM('<!DOCTYPE html><html><body><div id="c"></div></body></html>', { runScripts: 'dangerously' });
const w = dom.window;
const s = w.document.createElement('script');
s.textContent = fs.readFileSync(BUNDLE, 'utf8');
w.document.head.appendChild(s);
const mermaid = w.mermaid;
mermaid.initialize({ startOnLoad: false, securityLevel: 'strict' });

const OPS = process.argv.slice(2);
for (const op of OPS) {
  const text = `classDiagram\n    Alpha ${op} Beta\n`;
  let out;
  try {
    await mermaid.parse(text);
    const d = await mermaid.mermaidAPI.getDiagramFromText(text);
    const db = d.db ?? d.getDB();
    const classes = db.getClasses();
    const ids = [...(classes.keys ? classes.keys() : Object.keys(classes))];
    const rels = db.getRelations().map(r => `${r.id1}|${JSON.stringify(r.relation)}|${r.id2}`);
    out = `ids=[${ids.join(',')}] rels=${rels.join(' ; ')}`;
  } catch (e) {
    out = `ERROR ${e.message}`;
  }
  console.log(`${op.padEnd(8)} ${out}`);
}
