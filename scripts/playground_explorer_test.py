#!/usr/bin/env python3
"""Chromium integration tests for the production playground and SVG selection.

Run: python scripts/playground_explorer_test.py
Rendering/authoring transports are fixtures, not a rebuilt Rust/WASM engine.
The page, SVG selection implementation, DOM geometry, pointer capture and input
are real. Modules load from Blobs with only import URLs/import.meta.url substituted;
no network or development server is needed. Set CHROMIUM to override.

Run the actual showcase/WASM regression for GitHub issue #14 separately:
  python scripts/playground_explorer_test.py --showcase-issue14
This serves the repository and uses the unchanged reporter source, both production
showcase entrypoints, real CSS, real WASM and browser input. CDN access is required
for the showcase's Tailwind stylesheet and fonts; --tailwind-script may supply an
unchanged cached copy of https://cdn.tailwindcss.com when browser CDN access fails.
--resource-cache maps exact CDN URLs to path/content_type/sha256 in JSON;
paths resolve beside the manifest. Cached production fonts must actually load.
--serve-root can point to a built distribution or an older checkout; --output-dir
saves SVGs, screenshots and results.
"""
from pathlib import Path
import argparse
from collections import Counter
import functools
import hashlib
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import json
import math
import os
import re
import shutil
import threading
from urllib.parse import urlencode
import xml.etree.ElementTree as ET

from playwright.sync_api import sync_playwright

ROOT = Path(__file__).resolve().parents[1]
SVG = '''<svg xmlns="http://www.w3.org/2000/svg" width="540" height="320" viewBox="0 0 540 320">
<g class="fm-node" data-id="A" id="fm-node-a-0"><rect x="30" y="35" width="100" height="50" fill="#ddd"/><text x="40" y="65">Alpha</text></g>
<a href="#activated"><g class="fm-node" data-id="B" id="fm-node-b-1"><rect x="260" y="35" width="100" height="50" fill="#ddd"/><text x="270" y="65">Beta</text></g></a>
<g class="fm-node" data-id="C" id="fm-node-c-2"><rect x="30" y="175" width="100" height="50" fill="#ddd"/><text x="40" y="205">Gamma</text></g>
</svg>'''
BOOT = '''
window.workers = []; window.workerMessages = []; window.editorDisposed = 0;
class FixtureWorker {
  constructor() { this.messages = []; this.terminated = false; workers.push(this); }
  postMessage(message) {
    this.messages.push(message); workerMessages.push(message);
    if (message.kind === 'init') {
      this.init = message;
      queueMicrotask(() => this.emit({kind:'ready', requested:'auto',
        target: message.capabilities.canvasTransferred ? 'offscreenInWorker' : 'svgInWorker'}));
    }
    if (message.kind === 'render') this.latest = message;
  }
  emit(message) { this.onmessage?.({data:message}); }
  terminate() { this.terminated = true; }
}
window.Worker = FixtureWorker;
window.finish = (svg = window.testSvg, requestId = workers.at(-1).latest.requestId, worker = workers.at(-1)) =>
  worker.emit({kind:'completed',requestId,svg,nodeCount:3,edgeCount:0});
window.typeSource = source => { const el = document.querySelector('#src'); el.value = source; el.dispatchEvent(new Event('input')); };
window.ids = () => [...document.querySelectorAll('#out [data-fm-selection-overlay] [data-node-id]')].map(el => el.dataset.nodeId);
window.equal = (a,b) => { if (JSON.stringify(a) !== JSON.stringify(b)) throw Error(JSON.stringify(a)+' != '+JSON.stringify(b)); };
window.expect = (value, message = 'assertion failed') => { if (!value) throw Error(message); };
window.center = id => {
  const node = [...document.querySelectorAll('#out .fm-node')].find(el => el.dataset.id === id);
  const r = node.getBoundingClientRect(); return {x:r.x+r.width/2,y:r.y+r.height/2};
};
window.camera = () => { const r = document.querySelector('#out svg').viewBox.baseVal;
  return {x:r.x,y:r.y,width:r.width,height:r.height}; };
window.near = (a,b,tolerance=.002) => expect(Math.abs(a-b)<=tolerance, a+' not near '+b);
window.at = (x,y) => new DOMPoint(x,y).matrixTransform(document.querySelector('#out svg').getScreenCTM().inverse());
window.frames = () => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)));
'''
STUBS = {
    '/web/mermaid-compat.mjs': '''export function createWorkerMermaid() { return {}; }
export function createMermaid() { return {}; } export class MermaidError extends Error {}''',
    '/web/fm-document.js': '''export function mountDocumentWorkspace(options) {
window.openTestDocument = source => { options.sourceEl.value = source; options.onDocumentChange(); options.onChange(); };
return {sourceChanged(){},dispose(){},saveArtifact(){}}; }''',
    '/web/fm-share.js': 'export function mountShareControls(){return {sourceChanged(){},dispose(){}};}',
    '/web/fm-image-export.js': 'export function mountImageExport(){return {sourceChanged(){},dispose(){}};}',
    '/web/fm-config-editor.js': 'export function mountConfigurationEditor(){return {open(){},observeSource(){},sourceChanged(){},dispose(){}};}',
    '/web/fm-deck-editor.js': 'export function mountDeckEditor(){return {open(){},sourceChanged(){},documentChanged(){},dispose(){}};}',
    '/web/fm-source-editor.js': '''export function mountSourceEditor(options) {
options.panelEl.hidden = false;
return {render(source){window.editorSource=source;},dispose(){window.editorDisposed++;}}; }
export function createSourceEditorBackend(){ return {renderSource(){},cancel(){},dispose(){}}; }''',
    '/pkg/frankenmermaid.js': '''export default async function(){}
export function workerHandleMessage(text){const request=JSON.parse(text); return JSON.stringify({kind:'completed',requestId:request.requestId,svg:globalThis.testSvg});}''',
}


# Unchanged source attached to https://github.com/Dicklesworthstone/frankenmermaid/issues/14.
ISSUE14_FIXTURE = ROOT / 'crates/fm-render-svg/tests/fixtures/issue14_omnichannel.mmd'
ISSUE14_SHA256 = '3258bf1dd770ee1d7c9715069f4dc463bcd64faa119646645afe2792ee177b4c'


def issue14_expected_labels(source):
    """Read labels from the reporter's quoted, one-statement-per-line fixture.

    This deliberately does not use FrankenMermaid's parser as the expected result:
    a parser regression must not silently change the label-preservation oracle.
    The pinned source hash guards the fixture-specific extraction assumptions.
    """
    normalize = lambda text: ' '.join(re.sub(r'<br\s*/?>', ' ', text).split())
    nodes, clusters, edges = {}, [], []
    edge_count = 0
    for line in source.splitlines():
        if line.lstrip().startswith('%%'):
            continue
        if line.lstrip().startswith('subgraph '):
            clusters.extend(normalize(label) for label in re.findall(r'\["([^"]+)"\]', line))
            continue
        for identifier, label in re.findall(r'\b([A-Z][A-Z0-9]*)[\[{]"([^"]+)"[\]}]', line):
            assert identifier not in nodes, f'fixture repeats a node definition: {identifier}'
            nodes[identifier] = normalize(label)
        edges.extend(normalize(label) for label in re.findall(r'\|([^|]+)\|', line))
        edge_count += len(re.findall(r'-->|-\.->', line))
    assert len(source.splitlines()) == 76, 'the original issue #14 has 76 lines'
    assert (len(nodes), edge_count, len(clusters)) == (24, 43, 2)
    assert len(edges) == 24, 'the original issue #14 has 24 labeled edges'
    return {'nodes': nodes, 'edges': edges, 'clusters': clusters, 'edge_count': edge_count}


ISSUE14_SNAPSHOT = r'''() => {
  const svg = document.querySelector('#fm-svg svg');
  if (!svg) throw Error('No rendered studio SVG');
  const normalize = value => value.replace(/\s+/g, ' ').trim();
  const visibleText = text => {
    const leaves = [...text.querySelectorAll('tspan')].filter(span => !span.querySelector('tspan'));
    return normalize(leaves.length ? leaves.map(span => span.textContent).join(' ') : text.textContent);
  };
  const painted = element => {
    const box = element.getBBox();
    if (!Number.isFinite(box.width) || !Number.isFinite(box.height) || box.width <= 0 || box.height <= 0) return false;
    const textStyle = getComputedStyle(element);
    const noFill = ['none', 'transparent', 'rgba(0, 0, 0, 0)'].includes(textStyle.fill) || Number(textStyle.fillOpacity) === 0;
    const noStroke = textStyle.stroke === 'none' || Number(textStyle.strokeOpacity) === 0;
    if (noFill && noStroke) return false;
    for (let current = element; current; current = current.parentElement) {
      const css = getComputedStyle(current);
      if (css.display === 'none' || css.visibility !== 'visible' || Number(css.opacity) === 0) return false;
    }
    return true;
  };
  const textRows = selector => [...svg.querySelectorAll(selector)].map(element => ({
    text: visibleText(element), painted: painted(element)
  }));
  const nodes = [...svg.querySelectorAll('.fm-node[data-id]')].map(node => ({
    id: node.dataset.id,
    text: [...node.querySelectorAll('text')].map(visibleText).join(' '),
    painted: node.querySelectorAll('text').length > 0 && [...node.querySelectorAll('text')].every(painted)
  }));
  const matrix = new DOMMatrixReadOnly(getComputedStyle(document.querySelector('#studio-viewport')).transform);
  const rect = element => {
    const box = element.getBoundingClientRect();
    return {left: box.left, top: box.top, right: box.right, bottom: box.bottom};
  };
  return {
    tier: svg.dataset.detailTier, labelDetail: svg.dataset.labelDetail, zoom: matrix.a,
    zoomLabel: document.querySelector('#studio-zoom-level').textContent,
    nodes, edges: textRows('.fm-edge-labeled text.edge-label'), clusters: textRows('.fm-cluster-label'),
    edgeCount: svg.querySelectorAll('g[data-fm-edge-id]').length,
    declaredNodes: Number(svg.dataset.nodes), declaredEdges: Number(svg.dataset.edges),
    source: document.querySelector('#diagram-input').value,
    svgBox: rect(svg), stageBox: rect(document.querySelector('#studio-pan-container'))
  };
}'''


def issue14_label_errors(snapshot, expected, source):
    errors = []
    if snapshot['source'] != source:
        errors.append('editor source changed during rendering/zoom')
    if (snapshot['declaredNodes'], snapshot['declaredEdges'], snapshot['edgeCount']) != (24, 43, 43):
        errors.append(f"node/edge counts changed: {snapshot['declaredNodes']}/{snapshot['declaredEdges']}/{snapshot['edgeCount']}")
    actual_nodes = {node['id']: node['text'] for node in snapshot['nodes']}
    if len(snapshot['nodes']) != 24 or actual_nodes != expected['nodes']:
        for identifier in sorted(expected['nodes'].keys() | actual_nodes.keys()):
            if actual_nodes.get(identifier) != expected['nodes'].get(identifier):
                errors.append(f"node {identifier}: {actual_nodes.get(identifier)!r} != {expected['nodes'].get(identifier)!r}")
        if len(snapshot['nodes']) != 24:
            errors.append(f"rendered {len(snapshot['nodes'])} nodes instead of 24")
    for kind in ('edges', 'clusters'):
        actual = Counter(row['text'] for row in snapshot[kind])
        wanted = Counter(expected[kind])
        if actual != wanted:
            errors.append(f'{kind}: missing={dict(wanted - actual)}, unexpected={dict(actual - wanted)}')
    for kind in ('nodes', 'edges', 'clusters'):
        hidden = [row['text'] for row in snapshot[kind] if not row['painted']]
        if hidden:
            errors.append(f'{kind} have unpainted text: {hidden}')
    return errors


def issue14_fit_errors(snapshot):
    svg, stage = snapshot['svgBox'], snapshot['stageBox']
    if svg['left'] < stage['left'] - 2 or svg['top'] < stage['top'] - 2 or svg['right'] > stage['right'] + 2 or svg['bottom'] > stage['bottom'] + 2:
        return [f'Fit leaves the diagram outside its stage: svg={svg}, stage={stage}']
    return []


ISSUE14_LABEL_GEOMETRY = r'''() => {
  const svg = document.querySelector('#fm-svg svg');
  const inverse = svg.getScreenCTM().inverse();
  const boundsInSvg = (element, bounds) => {
    const matrix = inverse.multiply(element.getScreenCTM());
    const corners = [new DOMPoint(bounds.x, bounds.y), new DOMPoint(bounds.x + bounds.width, bounds.y),
      new DOMPoint(bounds.x, bounds.y + bounds.height), new DOMPoint(bounds.x + bounds.width, bounds.y + bounds.height)].map(point => point.matrixTransform(matrix));
    return {left: Math.min(...corners.map(point => point.x)), top: Math.min(...corners.map(point => point.y)),
      right: Math.max(...corners.map(point => point.x)), bottom: Math.max(...corners.map(point => point.y))};
  };
  const box = element => boundsInSvg(element, element.getBBox());
  const intersection = (a, b) => ({left: Math.max(a.left, b.left), top: Math.max(a.top, b.top),
    right: Math.min(a.right, b.right), bottom: Math.min(a.bottom, b.bottom)});
  const fillsMeet = (shapes, area) => {
    if (area.right - area.left <= 0.5 || area.bottom - area.top <= 0.5) return false;
    const matrices = shapes.map(shape => shape.getScreenCTM().inverse().multiply(svg.getScreenCTM()));
    const xs = [area.left + 0.25, (area.left + area.right) / 2, area.right - 0.25];
    const ys = [area.top + 0.25, (area.top + area.bottom) / 2, area.bottom - 0.25];
    return xs.some(x => ys.some(y => shapes.every((shape, index) =>
      shape.isPointInFill(new DOMPoint(x, y).matrixTransform(matrices[index])))));
  };
  const nodeShapes = new Map(), edgeTexts = new Map(), edgeBackgrounds = new Map();
  const nodes = [...svg.querySelectorAll('.fm-node[data-id]')].map(node => {
    const shape = node.querySelector('rect, path, polygon, circle, ellipse');
    nodeShapes.set(node.dataset.id, shape);
    const labels = [...node.querySelectorAll('text')].map(box);
    let diamondTextInside = null;
    if (['D', 'Q'].includes(node.dataset.id)) {
      // The two decision nodes have sloped sides. An axis-aligned bounding box
      // alone cannot detect a long label extending through a diamond's border.
      const intoShape = shape.getScreenCTM().inverse().multiply(svg.getScreenCTM());
      diamondTextInside = labels.every(label => {
        const insetX = Math.min(2, (label.right - label.left) / 2);
        const insetY = Math.min(2, (label.bottom - label.top) / 2);
        return [[label.left + insetX, label.top + insetY], [label.right - insetX, label.top + insetY],
          [label.left + insetX, label.bottom - insetY], [label.right - insetX, label.bottom - insetY]]
          .every(([x, y]) => shape.isPointInFill(new DOMPoint(x, y).matrixTransform(intoShape)));
      });
    }
    return {id: node.dataset.id, owner: box(shape), labels, diamondTextInside};
  });
  const edges = [...svg.querySelectorAll('.fm-edge-labeled')].map(edge => {
    const texts = [...edge.querySelectorAll('text.edge-label')];
    edgeTexts.set(edge.dataset.fmEdgeId, texts);
    const background = edge.querySelector('rect');
    edgeBackgrounds.set(edge.dataset.fmEdgeId, background);
    return {id: edge.dataset.fmEdgeId, owner: box(background), labels: texts.map(box)};
  });
  const clusterRects = [...svg.querySelectorAll('rect.fm-cluster')];
  const clusters = [...svg.querySelectorAll('.fm-cluster-label')].map((label, index) => ({
    id: clusterRects[index].id, owner: box(clusterRects[index]), labels: [box(label)]
  }));
  const view = svg.viewBox.baseVal;
  const frame = {left: view.x, top: view.y, right: view.x + view.width, bottom: view.y + view.height};
  const overlaps = [];
  for (const edge of edges) for (const [labelIndex, label] of edge.labels.entries()) for (const node of nodes) {
    const width = Math.min(label.right, node.owner.right) - Math.max(label.left, node.owner.left);
    const height = Math.min(label.bottom, node.owner.bottom) - Math.max(label.top, node.owner.top);
    if (width <= 2 || height <= 2) continue;
    const shape = nodeShapes.get(node.id), text = edgeTexts.get(edge.id)[labelIndex];
    const intoShape = shape.getScreenCTM().inverse().multiply(text.getScreenCTM());
    const fillsAfterText = Boolean(text.compareDocumentPosition(shape) & Node.DOCUMENT_POSITION_FOLLOWING)
      && !['none', 'transparent', 'rgba(0, 0, 0, 0)'].includes(getComputedStyle(shape).fill);
    const coveredCharacters = [];
    // Inspect actual glyph centers and paint order so the two diamond AABBs
    // and shapes behind labels do not create false occlusion failures.
    for (let index = 0; index < text.getNumberOfChars(); index++) {
      if (/\s/.test(text.textContent[index])) continue;
      const glyph = text.getExtentOfChar(index);
      const center = new DOMPoint(glyph.x + glyph.width / 2, glyph.y + glyph.height / 2).matrixTransform(intoShape);
      if (fillsAfterText && shape.isPointInFill(center)) coveredCharacters.push(text.textContent[index]);
    }
    overlaps.push({edge: edge.id, node: node.id, width, height, coveredCharacters: coveredCharacters.join('')});
  }
  const backgroundOverlaps = [];
  for (const edge of edges) for (const node of nodes) {
    const left = Math.max(edge.owner.left, node.owner.left), top = Math.max(edge.owner.top, node.owner.top);
    const right = Math.min(edge.owner.right, node.owner.right), bottom = Math.min(edge.owner.bottom, node.owner.bottom);
    const width = right - left, height = bottom - top;
    if (width <= 0.5 || height <= 0.5) continue;
    const shape = nodeShapes.get(node.id), background = edgeBackgrounds.get(edge.id);
    const css = getComputedStyle(shape);
    const fillsAfterBackground = Boolean(background.compareDocumentPosition(shape) & Node.DOCUMENT_POSITION_FOLLOWING)
      && !['none', 'transparent', 'rgba(0, 0, 0, 0)'].includes(css.fill)
      && Number(css.fillOpacity) > 0 && Number(css.opacity) > 0;
    const intoShape = shape.getScreenCTM().inverse().multiply(svg.getScreenCTM());
    // Background clearance prevents partial glyph clipping; actual fill
    // sampling avoids diamond-AABB false positives.
    const xs = [left + 0.25, (left + right) / 2, right - 0.25];
    const ys = [top + 0.25, (top + bottom) / 2, bottom - 0.25];
    const obscured = fillsAfterBackground && xs.some(x => ys.some(y =>
      shape.isPointInFill(new DOMPoint(x, y).matrixTransform(intoShape))));
    backgroundOverlaps.push({edge: edge.id, node: node.id, width, height, obscured});
  }
  const labelOverlaps = [];
  for (const [index, earlier] of edges.entries()) for (const later of edges.slice(index + 1)) {
    const earlierPlate = edgeBackgrounds.get(earlier.id), laterPlate = edgeBackgrounds.get(later.id);
    const css = getComputedStyle(laterPlate);
    if (['none', 'transparent', 'rgba(0, 0, 0, 0)'].includes(css.fill) || Number(css.fillOpacity) === 0 || Number(css.opacity) === 0) continue;
    const area = intersection(earlier.owner, later.owner);
    const platesTouch = fillsMeet([earlierPlate, laterPlate], area);
    const coveredCharacters = [];
    for (const text of edgeTexts.get(earlier.id)) {
      if (!(text.compareDocumentPosition(laterPlate) & Node.DOCUMENT_POSITION_FOLLOWING)) continue;
      if (!fillsMeet([laterPlate], intersection(box(text), later.owner))) continue;
      // Test each character's full extent, not just its center: a later opaque
      // label plate must not hide part of another label's glyphs.
      for (let character = 0; character < text.getNumberOfChars(); character++) {
        if (/\s/.test(text.textContent[character])) continue;
        const glyph = boundsInSvg(text, text.getExtentOfChar(character));
        if (fillsMeet([laterPlate], intersection(glyph, later.owner))) coveredCharacters.push(text.textContent[character]);
      }
    }
    if (platesTouch || coveredCharacters.length) labelOverlaps.push({edge: earlier.id, laterEdge: later.id,
      width: area.right - area.left, height: area.bottom - area.top, platesTouch,
      coveredCharacters: coveredCharacters.join('')});
  }
  return {frame, nodes, edges, clusters, edgeNodeOverlaps: overlaps,
    edgeBackgroundOverlaps: backgroundOverlaps, edgeLabelOverlaps: labelOverlaps};
}'''


def issue14_geometry_errors(geometry):
    errors = []

    def contains(outer, inner):
        # SVG user units keep this tolerance invariant under CSS pan/zoom. Two
        # units allow fractional font bounds, without hiding a clipped text row.
        return (inner['left'] >= outer['left'] - 2 and inner['top'] >= outer['top'] - 2
                and inner['right'] <= outer['right'] + 2 and inner['bottom'] <= outer['bottom'] + 2)

    for kind in ('nodes', 'edges', 'clusters'):
        for item in geometry[kind]:
            if item.get('diamondTextInside') is False:
                errors.append(f"node {item['id']} text crosses a sloped diamond border")
            for label in item['labels']:
                if not contains(item['owner'], label):
                    errors.append(f"{kind} {item['id']} text exceeds its shape: text={label}, shape={item['owner']}")
                if not contains(geometry['frame'], label):
                    errors.append(f"{kind} {item['id']} text exceeds the SVG viewBox: {label}")
    for overlap in geometry['edgeNodeOverlaps']:
        if overlap['coveredCharacters']:
            errors.append(f"edge {overlap['edge']} label characters {overlap['coveredCharacters']!r} are obscured by node {overlap['node']}")
    for overlap in geometry['edgeBackgroundOverlaps']:
        if overlap['obscured']:
            errors.append(f"edge {overlap['edge']} padded label background is obscured by node {overlap['node']}")
    for overlap in geometry['edgeLabelOverlaps']:
        if overlap['coveredCharacters']:
            errors.append(f"edge {overlap['edge']} label characters {overlap['coveredCharacters']!r} are obscured by edge {overlap['laterEdge']} label background")
        if overlap['platesTouch']:
            errors.append(f"edge {overlap['edge']} and {overlap['laterEdge']} padded label backgrounds overlap")
    return errors


def issue14_export_errors(svg_code, expected):
    root = ET.fromstring(svg_code)
    normalize = lambda value: ' '.join(value.split())

    def label_text(element):
        spans = [node for node in element.iter() if node.tag.endswith('}tspan')]
        return normalize(' '.join(''.join(node.itertext()) for node in spans) if spans else ''.join(element.itertext()))

    nodes, edges, clusters = {}, [], []
    for element in root.iter():
        classes = element.get('class', '').split()
        if 'fm-node' in classes:
            nodes[element.get('data-id')] = ' '.join(label_text(node) for node in element.iter() if node.tag.endswith('}text'))
        if 'edge-label' in classes and element.tag.endswith('}text'):
            edges.append(label_text(element))
        if 'fm-cluster-label' in classes:
            clusters.append(label_text(element))
    errors = []
    for kind, actual in (('nodes', nodes), ('edges', Counter(edges)), ('clusters', Counter(clusters))):
        wanted = expected[kind] if kind == 'nodes' else Counter(expected[kind])
        if actual != wanted:
            errors.append(f'exported {kind} lost or changed authored labels')
    return errors


ISSUE14_MAXIMIZED_SNAPSHOT = r'''(editor = false) => {
  const rect = element => {
    const box = element.getBoundingClientRect();
    return {left: box.left, top: box.top, right: box.right, bottom: box.bottom, width: box.width, height: box.height};
  };
  const selectors = ['#pg-root', '#pg-shell', '.pg-panes', '#studio-theme-strip'];
  selectors.push(...(editor ? ['.pane-editor', '.editor-wrapper', '.editor-textarea-container', '#diagram-input'] : ['.pane-visualizer', '#view-svg-container', '#studio-pan-container']));
  const controls = ['#pg-maximize'];
  controls.push(...(editor ? ['#mobile-render-btn'] : ['#studio-zoom-in', '#studio-zoom-out', '#studio-zoom-reset', '#studio-fullscreen-btn']));
  if (innerWidth < 1024) controls.push('#mobile-tab-editor', '#mobile-tab-preview');
  return {
    viewport: {width: innerWidth, height: innerHeight}, editor,
    maximized: document.body.classList.contains('pg-maximized'),
    pressed: document.querySelector('#pg-maximize').getAttribute('aria-pressed'),
    boxes: Object.fromEntries(selectors.map(selector => [selector, rect(document.querySelector(selector))])),
    controls: Object.fromEntries(controls.map(selector => {
      const element = document.querySelector(selector), box = rect(element);
      const hit = document.elementFromPoint(box.left + box.width / 2, box.top + box.height / 2);
      return [selector, {...box, reachable: !!hit && (element === hit || element.contains(hit))}];
    }))
  };
}'''


def issue14_maximized_errors(snapshot):
    errors = []
    width, height = snapshot['viewport']['width'], snapshot['viewport']['height']
    if not snapshot['maximized'] or snapshot['pressed'] != 'true':
        errors.append('maximized state must be reflected in the button and body')
    for selector, box in (snapshot['boxes'] | snapshot['controls']).items():
        if box['width'] <= 0 or box['height'] <= 0:
            errors.append(f'{selector} has no usable area: {box}')
        if box['left'] < -2 or box['top'] < -2 or box['right'] > width + 2 or box['bottom'] > height + 2:
            errors.append(f'{selector} extends outside {width}x{height}: {box}')
        if 'reachable' in box and not box['reachable']:
            errors.append(f'{selector} is clipped or covered')
    root = snapshot['boxes']['#pg-root']
    if abs(root['width'] - width) > 2 or abs(root['height'] - height) > 2:
        errors.append(f'studio does not fill viewport: {root}')
    stage = snapshot['boxes']['#diagram-input' if snapshot['editor'] else '#studio-pan-container']
    if stage['height'] < 120:
        errors.append(f'studio has less than 120px of usable height: {stage}')
    return errors


def run_showcase_issue14(serve_root, output_dir=None, tailwind_script=None, resource_cache=None):
    source_bytes = ISSUE14_FIXTURE.read_bytes()
    source_hash = hashlib.sha256(source_bytes).hexdigest()
    assert source_hash == ISSUE14_SHA256, 'preserve the issue #14 source exactly as reported'
    source = source_bytes.decode('utf-8')
    expected = issue14_expected_labels(source)
    tailwind_bytes = tailwind_script.read_bytes() if tailwind_script else None
    cached_resources = {}
    if resource_cache:
        for url, entry in json.loads(resource_cache.read_text(encoding='utf-8')).items():
            data = (resource_cache.parent / entry['path']).read_bytes()
            digest = hashlib.sha256(data).hexdigest()
            assert digest == entry['sha256'], f'cached resource changed: {url}'
            cached_resources[url] = {'body': data, 'content_type': entry['content_type'], 'sha256': digest}
    cached_fonts = any(url.startswith('https://fonts.googleapis.com/') for url in cached_resources)
    if output_dir:
        output_dir.mkdir(parents=True, exist_ok=True)

    class QuietHandler(SimpleHTTPRequestHandler):
        def log_message(self, *_args):
            pass

    server = ThreadingHTTPServer(('127.0.0.1', 0), functools.partial(QuietHandler, directory=str(serve_root)))
    thread = threading.Thread(target=server.serve_forever, daemon=True)
    thread.start()
    base_url = f'http://127.0.0.1:{server.server_port}'
    records = []

    def record(name, errors, **evidence):
        row = {'scenario_id': name, 'passed': not errors, 'errors': errors, **evidence}
        records.append(row)
        print(json.dumps(row, ensure_ascii=False), flush=True)

    try:
        with sync_playwright() as pw:
            browser = pw.chromium.launch(headless=True,
                executable_path=os.environ.get('CHROMIUM') or shutil.which('chromium'), args=['--no-sandbox'])
            for route in ('index.html', 'frankenmermaid_demo_showcase.html'):
                page_errors = []
                resource_errors = []
                page = browser.new_page(viewport={'width': 1466, 'height': 904}, reduced_motion='reduce',
                    user_agent='frankenmermaid-agent (+https://github.com/Dicklesworthstone/frankenmermaid)')
                page.set_default_timeout(10000)
                page.on('pageerror', lambda error: page_errors.append(str(error)))
                page.on('requestfailed', lambda request: resource_errors.append({'url': request.url, 'failure': request.failure}))
                if tailwind_bytes is not None:
                    page.route('https://cdn.tailwindcss.com/', lambda request: request.fulfill(body=tailwind_bytes, content_type='text/javascript'))
                if cached_resources:
                    def fulfill_cached(request):
                        resource = cached_resources[request.request.url]
                        request.fulfill(body=resource['body'], content_type=resource['content_type'],
                            headers={'Access-Control-Allow-Origin': '*'})
                    page.route(re.compile('^(?:' + '|'.join(re.escape(url) for url in cached_resources) + ')$'), fulfill_cached)
                phase = 'load showcase'
                try:
                    # The public source query restores the unchanged report even when an
                    # older checkout's source editor is clipped by the sizing defect.
                    query = urlencode({'source': source})
                    page.goto(f'{base_url}/{route}?{query}#playground', wait_until='networkidle', timeout=60000)
                    phase = 'wait for real WASM rendering'
                    page.wait_for_function("Boolean(window.frankenRuntime && document.querySelector('#fm-svg svg'))")
                    # Do not accept geometry without the production utility stylesheet.
                    phase = 'wait for production Tailwind layout'
                    page.wait_for_function("getComputedStyle(document.querySelector('.pg-panes')).display === 'grid'")
                    page.evaluate('document.fonts.ready')
                    phase = 'wait for the reported diagram'
                    page.wait_for_function("document.querySelector('#fm-svg svg[data-nodes=\"24\"] .fm-node[data-id=\"LEARN\"]')")
                    # Load every label's face/weight and Czech characters;
                    # an earlier fonts.ready can precede the inserted diagram.
                    page.evaluate('''async () => {
                      await Promise.all([...document.querySelectorAll('#fm-svg svg text')].map(text => {
                        const css = getComputedStyle(text);
                        return document.fonts.load(`${css.fontStyle} ${css.fontWeight} ${css.fontSize} ${css.fontFamily}`, text.textContent);
                      }));
                      await document.fonts.ready;
                    }''')
                    page.evaluate('() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve)))')
                    if cached_fonts:
                        fonts = page.evaluate("[...document.fonts].map(font => ({family: font.family, style: font.style, weight: font.weight, status: font.status}))")
                        missing = [family for family in ('Inter', 'JetBrains Mono') if not any(
                            font['family'].strip('\"\'') == family and font['status'] == 'loaded' for font in fonts)]
                        record(f'{route}/production-fonts', [f'production font did not load: {family}' for family in missing], fonts=fonts)
                    phase = 'exercise labels, zoom and maximize'

                    def capture_labels(name):
                        snapshot = page.evaluate(ISSUE14_SNAPSHOT)
                        geometry = page.evaluate(ISSUE14_LABEL_GEOMETRY)
                        errors = issue14_label_errors(snapshot, expected, source) + issue14_geometry_errors(geometry)
                        record(f'{route}/{name}', errors, snapshot=snapshot, geometry=geometry)
                        if output_dir:
                            (output_dir / f'{route}-{name}.svg').write_text(page.locator('#fm-svg svg').evaluate('(element) => element.outerHTML'), encoding='utf-8')
                        return snapshot

                    fitted = capture_labels('fit-labels')
                    record(f'{route}/fit-bounds', issue14_fit_errors(fitted))
                    def zoom_to(target):
                        # Use real buttons and native wheel input for both zooms
                        # reported in the issue, without private camera helpers.
                        zoom = page.evaluate(ISSUE14_SNAPSHOT)['zoom']
                        for _ in range(24):
                            if target * 0.96 <= zoom <= target * 1.04:
                                break
                            page.click('#studio-zoom-in' if zoom < target else '#studio-zoom-out')
                            page.evaluate('() => new Promise(resolve => requestAnimationFrame(resolve))')
                            zoom = page.evaluate(ISSUE14_SNAPSHOT)['zoom']
                        for _ in range(8):
                            if abs(zoom - target) < 0.0049:
                                break
                            pan = page.locator('#studio-pan-container').bounding_box()
                            viewport = page.viewport_size
                            left, top = max(0, pan['x']), max(0, pan['y'])
                            right = min(viewport['width'], pan['x'] + pan['width'])
                            bottom = min(viewport['height'], pan['y'] + pan['height'])
                            assert right > left and bottom > top, 'zoom stage has no visible area'
                            page.mouse.move((left + right) / 2, (top + bottom) / 2)
                            page.mouse.wheel(0, max(-30, min(30, -math.log(target / zoom) / 0.0006)))
                            page.wait_for_function("prior => Math.abs(new DOMMatrixReadOnly(getComputedStyle(document.querySelector('#studio-viewport')).transform).a - prior) > 0.00001", arg=zoom)
                            zoom = page.evaluate(ISSUE14_SNAPSHOT)['zoom']

                    for target in (0.31, 0.95):
                        percent = round(target * 100)
                        zoom_to(target)
                        zoomed = capture_labels(f'zoom-{percent}-labels')
                        at_reported_zoom = abs(zoomed['zoom'] - target) < 0.01 and zoomed['zoomLabel'].strip() == f'{percent}%'
                        record(f'{route}/zoom-{percent}-controls', [] if at_reported_zoom else [
                            f"zoom controls reached {zoomed['zoom']} ({zoomed['zoomLabel']}), expected {percent}%"])

                    with page.expect_download() as download_info:
                        page.click('#btn-export-svg')
                    download = download_info.value
                    exported = Path(download.path()).read_text(encoding='utf-8')
                    record(f'{route}/svg-export-labels', issue14_export_errors(exported, expected))
                    if output_dir:
                        (output_dir / f'{route}-export.svg').write_text(exported, encoding='utf-8')

                    page.click('#pg-maximize')
                    page.wait_for_function("document.body.classList.contains('pg-maximized')")
                    for width, height in ((1466, 904), (1100, 650), (390, 844), (390, 650), (844, 390)):
                        page.set_viewport_size({'width': width, 'height': height})
                        if width < 1024:
                            page.click('#mobile-tab-preview')
                        # The production resize handler debounces 120 ms. Wait for the
                        # CSS transition and scheduled fit, not a replaced layout rule.
                        page.wait_for_timeout(350)
                        snapshot = page.evaluate(ISSUE14_MAXIMIZED_SNAPSHOT)
                        name = f'maximized-{width}x{height}'
                        record(f'{route}/{name}', issue14_maximized_errors(snapshot), snapshot=snapshot)
                        fitted = capture_labels(f'{name}-labels')
                        record(f'{route}/{name}-fit-bounds', issue14_fit_errors(fitted))
                        if output_dir:
                            page.screenshot(path=str(output_dir / f'{route}-{name}.png'), animations='disabled')
                        if width < 1024:
                            page.click('#mobile-tab-editor')
                            page.wait_for_timeout(200)
                            editor = page.evaluate(ISSUE14_MAXIMIZED_SNAPSHOT, True)
                            record(f'{route}/{name}-editor', issue14_maximized_errors(editor), snapshot=editor)
                            if output_dir:
                                page.screenshot(path=str(output_dir / f'{route}-{name}-editor.png'), animations='disabled')
                            page.click('#mobile-render-btn')
                            page.wait_for_function("getComputedStyle(document.querySelector('.pane-visualizer')).display !== 'none'")
                            page.wait_for_timeout(150)
                            capture_labels(f'{name}-editor-return-labels')
                    page.keyboard.press('Escape')
                    page.wait_for_function("!document.body.classList.contains('pg-maximized')")
                    restore_errors = []
                    if page.get_attribute('#pg-maximize', 'aria-pressed') != 'false':
                        restore_errors.append('Escape did not restore aria-pressed=false')
                    record(f'{route}/restore', restore_errors)
                except Exception as error:
                    state = page.evaluate('''() => ({runtime: !!window.frankenRuntime,
                      grid: getComputedStyle(document.querySelector('.pg-panes')).display,
                      input: document.querySelector('#diagram-input').value.slice(0, 120),
                      nodeCount: document.querySelector('#fm-svg svg')?.dataset.nodes,
                      nodeIds: [...document.querySelectorAll('#fm-svg .fm-node')].map(node => node.dataset.id)})''')
                    record(f'{route}/browser-run', [f'{phase}: {error}'], resource_errors=resource_errors, state=state)
                    if output_dir:
                        try:
                            page.screenshot(path=str(output_dir / f'{route}-failure.png'), animations='disabled')
                        except Exception as screenshot_error:
                            record(f'{route}/failure-screenshot', [str(screenshot_error)])
                finally:
                    # External font/CDN failures are evidence, not engine exceptions:
                    # normal browser fallback fonts must preserve readable labels too.
                    record(f'{route}/uncaught-errors', page_errors, resource_errors=resource_errors,
                        fonts=page.evaluate("[...document.fonts].map(font => ({family: font.family, style: font.style, weight: font.weight, status: font.status}))"))
                    page.close()
            browser.close()
    finally:
        server.shutdown()
        server.server_close()
        thread.join(timeout=2)
    summary = {
        'checks': len(records), 'failed': [row['scenario_id'] for row in records if not row['passed']],
        'fixture_sha256': source_hash,
        'wasm_sha256': hashlib.sha256((serve_root / 'pkg/frankenmermaid_bg.wasm').read_bytes()).hexdigest(),
        'showcase_sha256': hashlib.sha256((serve_root / 'index.html').read_bytes()).hexdigest(),
        'tailwind_sha256': hashlib.sha256(tailwind_bytes).hexdigest() if tailwind_bytes is not None else 'live CDN',
        'resource_cache': {url: {key: value for key, value in resource.items() if key != 'body'}
            for url, resource in cached_resources.items()},
        'results': records,
    }
    if output_dir:
        (output_dir / 'issue14-results.json').write_text(json.dumps(summary, ensure_ascii=False, indent=2), encoding='utf-8')
    print(json.dumps({key: value for key, value in summary.items() if key != 'results'}), flush=True)
    return bool(summary['failed'])


def main():
    failures, page_errors = [], []
    checks = 0
    with sync_playwright() as pw:
        browser = pw.chromium.launch(headless=True,
            executable_path=os.environ.get('CHROMIUM') or shutil.which('chromium'), args=['--no-sandbox'])
        page = browser.new_page(viewport={'width': 1440, 'height': 1100})
        page.set_default_timeout(4000)
        page.on('pageerror', lambda error: page_errors.append(str(error)))

        html = (ROOT / 'web/playground.html').read_text()
        markup, separator, script = html.partition('<script type="module">')
        assert separator and script.endswith('</script>\n'), 'Expected the production inline module'
        page.set_content(markup)
        page.evaluate(BOOT + '\nwindow.testSvg=' + json.dumps(SVG) + ';')
        page.evaluate('''async ({source, selection, stubs}) => {
          const urls = {};
          for (const [path, code] of Object.entries(stubs)) {
            const name = path.startsWith('/web/') ? './' + path.slice(5) : '..' + path;
            urls[name] = URL.createObjectURL(new Blob([code], {type:'text/javascript'}));
          }
          const rewrite = code => {
            for (const [name, url] of Object.entries(urls)) code = code.replaceAll(JSON.stringify(name), JSON.stringify(url));
            return code.replaceAll('import.meta.url', JSON.stringify('https://frankenmermaid.invalid/web/playground.html'));
          };
          urls['./mermaid.mjs'] = URL.createObjectURL(new Blob([rewrite(selection)], {type:'text/javascript'}));
          const entry = URL.createObjectURL(new Blob([rewrite(source)], {type:'text/javascript'}));
          await import(entry);
        }''', {'source': script.removesuffix('</script>\n'),
               'selection': (ROOT/'web/mermaid.mjs').read_text(), 'stubs': STUBS})
        page.wait_for_function('workers[0]?.latest')

        def check(name, operation):
            nonlocal checks
            checks += 1
            try:
                operation() if callable(operation) else page.evaluate('() => {' + operation + '}')
                print(json.dumps({'scenario_id': name, 'passed': True}))
            except Exception as error:
                failures.append(name)
                print(json.dumps({'scenario_id': name, 'passed': False, 'error': str(error)}))

        def open_explorer(svg=SVG):
            if page.get_attribute('#explore', 'aria-expanded') == 'true':
                page.click('#explore')
            previous_workers = page.evaluate('workers.length')
            page.click('#explore')
            page.wait_for_function('(count) => workers.length > count && workers.at(-1)?.latest && !workers.at(-1).terminated && !workers.at(-1).init.capabilities.offscreenCanvas', arg=previous_workers)
            page.evaluate('(svg) => finish(svg)', svg)
            page.wait_for_function('!document.querySelector("#explorer-controls").disabled')

        def click_node(id, modifiers=None):
            p = page.evaluate('(id) => center(id)', id)
            for modifier in modifiers or []:
                page.keyboard.down(modifier)
            try:
                page.mouse.click(p['x'], p['y'])
            finally:
                for modifier in modifiers or []:
                    page.keyboard.up(modifier)

        check('explorer-mounts-svg-worker', lambda: (
            open_explorer(), page.evaluate('''expect(workers[0].terminated); expect(!workers.at(-1).init.capabilities.canvasTransferred);
expect(!document.querySelector('#out').inert); expect(document.querySelectorAll('#explorer-results option').length===3);''')))
        check('real-click-selects-without-activation', lambda: (
            click_node('B'), page.evaluate("equal(ids(),['B']); expect(location.hash===''); expect(document.querySelector('#explorer-selection').textContent==='1 nodes selected: B');")))
        check('real-modifier-multiselect', lambda: (
            click_node('A', ['Shift']), page.evaluate("equal(ids(),['A','B'])")))
        check('keyboard-navigation-updates-inspector', lambda: (
            page.keyboard.press('ArrowDown'), page.evaluate("equal(ids(),['C']); expect(document.querySelector('#explorer-selection').textContent.includes('C'))")))
        check('search-label-and-enter-select', lambda: (
            page.fill('#explorer-search', 'bEtA'), page.press('#explorer-search', 'Enter'),
            page.evaluate("equal(ids(),['B']); expect(document.querySelectorAll('#explorer-results option').length===1)")))
        check('source-edit-suspends-old-selection', '''
window.oldRoot=document.querySelector('#out svg'); typeSource('changed source');
expect(document.querySelector('#out').inert); expect(document.querySelector('#explorer-controls').disabled);
equal(ids(),[]); expect(document.querySelector('#explorer-message').textContent.includes('not interactive'));
''')
        check('rerender-retains-authored-id', '''
finish(testSvg.replace('fm-node-b-1','entirely-different-generated-id').replace('Beta','Beta updated'));
equal(ids(),['B']); expect(document.querySelector('#out svg')!==oldRoot);
expect(!oldRoot.querySelector('[data-fm-selection-overlay]')); expect(!oldRoot.hasAttribute('tabindex'));
expect(!document.querySelector('#out').inert);
''')
        check('obsolete-reply-never-replaces-current-preview', '''
const w=workers.at(-1); typeSource('older'); const old=w.latest.requestId;
typeSource('newer'); finish(); const root=document.querySelector('#out svg'); finish('<svg/>',old);
expect(document.querySelector('#out svg')===root); equal(ids(),['B']);
''')
        check('failure-keeps-stale-preview-inert', '''
typeSource('bad input'); workers.at(-1).emit({kind:'failed',requestId:workers.at(-1).latest.requestId,reason:'parse failed'});
expect(document.querySelector('#out').inert); expect(document.querySelector('#explorer-controls').disabled);
expect(document.querySelector('#explorer-message').textContent.includes('failed'));
''')
        check('recovery-restores-selection', "typeSource('repaired'); finish(); equal(ids(),['B']); expect(!document.querySelector('#out').inert);")
        check('new-document-does-not-inherit-equal-ids', "openTestDocument('another file'); finish(); equal(ids(),[]);")
        check('removed-node-is-forgotten', lambda: (
            page.fill('#explorer-search','Alpha'), page.press('#explorer-search','Enter'),
            page.evaluate("typeSource('remove A'); finish(testSvg.replace('data-id=\"A\"','data-id=\"new-node\"')); equal(ids(),[]);")))
        check('empty-chart-is-explicit', '''
typeSource('pie'); finish('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 300 200"><circle cx="80" cy="80" r="30"/></svg>');
expect(document.querySelector('#explorer-message').textContent.includes('no addressable')); equal(ids(),[]);
expect(!document.querySelector('#out').inert);
''')
        check('duplicate-ids-fail-closed', '''
typeSource('duplicate'); finish(testSvg.replace('data-id="C"','data-id="A"'));
expect(document.querySelector('#out').inert); expect(document.querySelector('#explorer-message').textContent.includes('Ambiguous'));
''')
        check('hostile-label-is-data-in-search-ui', '''
typeSource('escaped labels'); finish(testSvg.replace('Alpha','&lt;img src=x onerror=alert(1)&gt;'));
const input=document.querySelector('#explorer-search'); input.value='img'; input.dispatchEvent(new Event('input'));
expect(document.querySelector('#explorer-results').textContent.includes('<img')); expect(!document.querySelector('#explorer img'));
''')
        check('large-search-is-bounded-but-counted', '''
const nodes=Array.from({length:260},(_,i)=>`<g class="fm-node" data-id="N${i}"><rect x="${i}" y="0" width="2" height="2"/></g>`).join('');
typeSource('large'); finish(`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 400 300">${nodes}</svg>`);
const input=document.querySelector('#explorer-search'); input.value=''; input.dispatchEvent(new Event('input'));
expect(document.querySelectorAll('#explorer-results option').length===200);
expect(document.querySelector('#explorer-search-count').textContent.includes('260 matching'));
''')
        check('close-unbinds-and-restores-root', lambda: (
            page.click('#explore'), page.evaluate("expect(!document.querySelector('#out').inert); expect(document.querySelector('#explorer').hidden); expect(!document.querySelector('#out svg').hasAttribute('tabindex'));")))
        check('reopen-is-fresh-and-does-not-double-bind', lambda: (
            open_explorer(), click_node('A'), page.evaluate("equal(ids(),['A']); expect(document.querySelectorAll('[data-fm-selection-overlay]').length===1)")))
        check('source-editor-takes-exclusive-ownership', lambda: (
            page.click('#edit-source'), page.wait_for_function('window.editorSource !== undefined'),
            page.evaluate("expect(document.querySelector('#explorer').hidden); expect(!document.querySelector('#out').inert); expect(workers.at(-1).terminated);")))
        check('explorer-can-replace-source-editor', lambda: (
            open_explorer(), page.evaluate("expect(editorDisposed===1); expect(document.querySelector('#source-editor').hidden); expect(!document.querySelector('#edit-source').disabled);")))
        check('zoom-buttons-do-not-reparse-or-change-source', lambda: (
            page.evaluate("window.beforeCamera=camera(); window.messageCount=workerMessages.length; window.beforeSource=document.querySelector('#src').value;"),
            page.click('#explorer-zoom-in'), page.evaluate("near(camera().width,beforeCamera.width/1.25); expect(workerMessages.length===messageCount); expect(document.querySelector('#src').value===beforeSource);"),
            page.click('#explorer-zoom-out'), page.evaluate("near(camera().width,beforeCamera.width);")))

        def wheel_anchor():
            page.click('#explorer-fit')
            p = page.evaluate("center('B')")
            page.evaluate('(p) => { window.anchor=at(p.x,p.y); window.oldWidth=camera().width; }', p)
            page.mouse.move(p['x'], p['y'])
            page.mouse.wheel(0, -140)
            page.wait_for_function('camera().width < oldWidth')
            page.evaluate('(p) => {const now=at(p.x,p.y); near(now.x,anchor.x); near(now.y,anchor.y);}', p)

        check('wheel-zoom-keeps-pointer-anchor', wheel_anchor)
        check('zoom-extremes-are-bounded-and-finite', '''
const root=document.querySelector('#out svg'), r=root.getBoundingClientRect();
for (let i=0;i<30;i++) root.dispatchEvent(new WheelEvent('wheel',{deltaY:-600,clientX:r.x+r.width/2,clientY:r.y+r.height/2,bubbles:true,cancelable:true}));
expect(document.querySelector('#explorer-zoom').textContent==='51200% of fit'); expect(Object.values(camera()).every(Number.isFinite));
for (let i=0;i<30;i++) root.dispatchEvent(new WheelEvent('wheel',{deltaY:600,clientX:r.x+r.width/2,clientY:r.y+r.height/2,bubbles:true,cancelable:true}));
expect(document.querySelector('#explorer-zoom').textContent==='5% of fit'); expect(Object.values(camera()).every(Number.isFinite));
''')
        check('fit-diagram-restores-home', lambda: (
            page.click('#explorer-fit'), page.evaluate("expect(document.querySelector('#explorer-zoom').textContent==='100% of fit');")))
        check('fit-selection-frames-selected-node', lambda: (
            click_node('A'), page.click('#explorer-fit-selection'), page.evaluate('''
equal(ids(),['A']); const view=camera(); expect(view.x<=30 && view.x+view.width>=130);
expect(view.y<=35 && view.y+view.height>=85); expect(view.width<540);
const overlay=document.querySelector('[data-node-id="A"]').getBoundingClientRect();
const node=document.querySelector('.fm-node[data-id="A"]').getBoundingClientRect();
near(overlay.x,node.x); near(overlay.y,node.y); near(overlay.width,node.width);
''')))
        check('fit-empty-selection-is-explicit', lambda: (
            page.click('#explorer-clear'), page.click('#explorer-fit-selection'),
            page.evaluate("expect(document.querySelector('#explorer-message').textContent.includes('Select one or more')); expect(Object.values(camera()).every(Number.isFinite));")))

        def pan_camera(button='middle', dx=40, dy=25):
            page.click('#explorer-fit')
            click_node('A')
            p = page.evaluate("center('B')")
            before = page.evaluate("center('A')")
            page.mouse.move(p['x'], p['y'])
            page.mouse.down(button=button)
            page.mouse.move(p['x']+dx, p['y']+dy)
            page.mouse.up(button=button)
            page.evaluate('([before,dx,dy]) => {const now=center("A"); near(now.x,before.x+dx); near(now.y,before.y+dy); equal(ids(),["A"]);}', [before,dx,dy])

        check('middle-pan-preserves-selection', pan_camera)

        def left_pan():
            page.select_option('#explorer-tool','pan')
            page.click('#explorer-fit')
            before = page.evaluate("center('A')")
            p = page.evaluate("center('B')")
            page.mouse.move(p['x'],p['y'])
            page.mouse.down()
            page.mouse.move(p['x']-45,p['y']+20)
            page.mouse.up()
            page.evaluate('(before) => {const now=center("A"); near(now.x,before.x-45); near(now.y,before.y+20); equal(ids(),["A"]);}', before)

        check('pan-tool-left-drag-is-camera-only', left_pan)
        check('pan-keyboard-moves-without-changing-selection', lambda: (
            page.evaluate("window.oldPoint=center('A'); document.querySelector('#out svg').focus();"),
            page.keyboard.press('ArrowRight'), page.evaluate("near(center('A').x,oldPoint.x-50); equal(ids(),['A']);")))
        check('keyboard-fit-and-zoom', lambda: (
            page.keyboard.press('0'), page.evaluate("expect(document.querySelector('#explorer-zoom').textContent==='100% of fit'); window.oldWidth=camera().width;"),
            page.keyboard.press('+'), page.evaluate("near(camera().width,oldWidth/1.25);"),
            page.keyboard.press('f'), page.evaluate("expect(camera().width<540); equal(ids(),['A']);")))

        def pinch():
            page.click('#explorer-fit')
            r = page.locator('#out svg').bounding_box()
            x, y = r['x']+r['width']/2, r['y']+r['height']/2
            session = page.context.new_cdp_session(page)
            point = lambda id, px: {'id': id, 'x': px, 'y': y, 'radiusX': 1, 'radiusY': 1, 'force': 1}
            page.evaluate('([x,y])=>{window.pinchAnchor=at(x,y);window.pinchWidth=camera().width;}',[x,y])
            session.send('Input.dispatchTouchEvent',{'type':'touchStart','touchPoints':[point(1,x-40)]})
            session.send('Input.dispatchTouchEvent',{'type':'touchStart','touchPoints':[point(1,x-40),point(2,x+40)]})
            session.send('Input.dispatchTouchEvent',{'type':'touchMove','touchPoints':[point(1,x-80),point(2,x+80)]})
            session.send('Input.dispatchTouchEvent',{'type':'touchEnd','touchPoints':[]})
            page.evaluate('([x,y])=>{near(camera().width,pinchWidth/2);const p=at(x,y);near(p.x,pinchAnchor.x);near(p.y,pinchAnchor.y);equal(ids(),["A"]);}',[x,y])
            session.detach()

        check('real-two-touch-pinch-keeps-midpoint-anchor', pinch)
        check('camera-and-selection-survive-source-refresh', '''
window.beforeCamera=camera(); typeSource('same geometry, new source'); finish(testSvg.replace('Alpha','Alpha revised'));
const now=camera(); for(const key of Object.keys(now)) near(now[key],beforeCamera[key]); equal(ids(),['A']);
''')
        check('document-switch-resets-camera', '''
openTestDocument('new document'); finish(); expect(document.querySelector('#explorer-zoom').textContent==='100% of fit'); equal(ids(),[]);
''')

        def box_after_zoom():
            page.select_option('#explorer-tool','box')
            page.click('#explorer-fit')
            page.click('#explorer-zoom-in')
            r = page.locator('#out .fm-node[data-id="C"]').bounding_box()
            page.mouse.move(r['x']-5,r['y']-5)
            page.mouse.down()
            page.mouse.move(r['x']+r['width']+5,r['y']+r['height']+5)
            page.mouse.up()
            page.evaluate("equal(ids(),['C']);")

        check('box-selection-uses-camera-transformed-geometry', box_after_zoom)

        def pan_revoked_by_source():
            page.select_option('#explorer-tool','pan')
            p = page.evaluate("center('C')")
            page.mouse.move(p['x'],p['y'])
            page.mouse.down()
            page.mouse.move(p['x']+15,p['y']+10)
            page.evaluate("typeSource('typing cancels pan'); window.pendingCamera=camera(); expect(document.querySelector('#out').inert);")
            page.mouse.move(p['x']+75,p['y']+50)
            page.mouse.up()
            page.evaluate("for(const key of Object.keys(camera())) near(camera()[key],pendingCamera[key]); finish(); equal(ids(),['C']);")

        check('source-edit-revokes-pan-capture', pan_revoked_by_source)
        check('nonfinite-wheel-input-does-not-corrupt-view', '''
const root=document.querySelector('#out svg'), before=camera();
// Native WheelEventInit rejects Infinity; a forged event exercises the defensive boundary.
const event=new WheelEvent('wheel',{bubbles:true,cancelable:true});
Object.defineProperty(event,'deltaY',{value:Infinity}); root.dispatchEvent(event); equal(camera(),before);
''')

        def resizing():
            page.evaluate("window.preResize=camera();")
            page.set_viewport_size({'width':1200,'height':950})
            page.evaluate('frames()')
            page.evaluate('''const now=camera(); near(now.x+now.width/2,preResize.x+preResize.width/2);
near(now.y+now.height/2,preResize.y+preResize.height/2); equal(ids(),['C']);''')

        check('resize-preserves-camera-center-and-selection', resizing)
        check('closing-restores-original-viewbox', lambda: (
            page.click('#explore'), page.evaluate("expect(document.querySelector('#out svg').getAttribute('viewBox')==='0 0 540 320'); expect(!document.querySelector('#out').hasAttribute('data-exploring'));"),
            open_explorer()))
        check('pagehide-cleans-up-interaction-and-worker', '''
const root=document.querySelector('#out svg'); window.dispatchEvent(new PageTransitionEvent('pagehide',{persisted:false}));
expect(workers.at(-1).terminated); expect(!root.querySelector('[data-fm-selection-overlay]'));
expect(!root.hasAttribute('tabindex'));
''')
        browser.close()
    if page_errors:
        failures.append('uncaught-browser-errors')
        print(json.dumps({'page_errors': page_errors}))
    print(json.dumps({'checks': checks, 'failed': failures,
        'playground_sha256': hashlib.sha256((ROOT/'web/playground.html').read_bytes()).hexdigest()}))
    return bool(failures)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--showcase-issue14', action='store_true', help='exercise the real showcase and WASM with the reporter source')
    parser.add_argument('--serve-root', type=Path, default=ROOT, help='repository or distribution to serve for --showcase-issue14')
    parser.add_argument('--output-dir', type=Path, help='save issue #14 regression evidence here')
    parser.add_argument('--tailwind-script', type=Path, help='unchanged cached Tailwind CDN script for --showcase-issue14')
    parser.add_argument('--resource-cache', type=Path, help='JSON manifest of exact CDN URLs and cached path/content_type/sha256 for --showcase-issue14')
    args = parser.parse_args()
    if not args.showcase_issue14 and (args.output_dir or args.serve_root != ROOT or args.tailwind_script or args.resource_cache):
        parser.error('--serve-root, --output-dir, --tailwind-script and --resource-cache require --showcase-issue14')
    raise SystemExit(run_showcase_issue14(args.serve_root.resolve(), args.output_dir, args.tailwind_script, args.resource_cache) if args.showcase_issue14 else main())
