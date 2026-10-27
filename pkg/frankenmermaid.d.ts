/* tslint:disable */
/* eslint-disable */

/** A rectangle in SVG viewBox space (deck manifest schema 1.0.0). */
export interface DeckRect { x: number; y: number; width: number; height: number; }

export interface DeckManifestOptions {
    fitMargin: number; zoomMax: number; dimOpacity: number; autoAdvanceMs: number;
}

export interface DeckManifestNode {
    index: number; sourceId: string; elementId: string; step: number; tooltip?: string;
}

export interface DeckManifestEdge {
    index: number; elementId: string; step: number; touching: boolean;
}

export interface DeckManifestCluster {
    index: number; elementId: string; step: number; cameraContained: boolean;
}

export interface DeckManifestStep { step: number; elementIds: string[]; }

export interface DeckManifestSlide {
    id: string; title: string; caption?: string; bounds: DeckRect;
    fitMargin: number; zoomMax: number;
    nodes: DeckManifestNode[]; edges?: DeckManifestEdge[]; clusters?: DeckManifestCluster[];
    maxStep: number; steps?: DeckManifestStep[];
}

export interface DeckManifestOverview {
    enabled: boolean; title: string; caption?: string; tour: boolean;
}

/** One edge's endpoint node element ids — the live-edge join for morphing runtimes (1.1.0). */
export interface DeckEdgeEndpoints { fromElementId: string; toElementId: string; }

/** The renderer-agnostic presentation contract; additive-only within 1.x. */
export interface DeckManifest {
    schemaVersion: string; generator: string; diagramType: string; title?: string;
    viewBox: DeckRect; options: DeckManifestOptions; slides: DeckManifestSlide[];
    overview: DeckManifestOverview; nodeSlideIndex?: Record<string, string[]>;
    /** Home rect per laid-out node (viewBox space), whole diagram (1.1.0). */
    nodeGeometry?: Record<string, DeckRect>;
    /** Edge elementId -> endpoint node element ids (1.1.0). */
    edgeEndpoints?: Record<string, DeckEdgeEndpoints>;
}

/** renderDeck() result: svg + manifest (null when no deck) + structured deck diagnostics. */
export interface WasmDeckOutput {
    svg: string; manifest?: DeckManifest; warnings: unknown[];
}



/** Strict initialization-config validation result (schema 1.0.0). */
export interface MermaidConfigValidation {
    schemaVersion: "1.0.0";
    errors: Array<{ field: string; value: string; message: string }>;
}

/** Return the JSON Schema used by validateConfig. */
export function configSchema(): string;
/** Validate config JSON and return MermaidConfigValidation encoded as JSON. */
export function validateConfig(configJson: string): string;
/** Validate Mermaid init/constraints directives and return MermaidConfigValidation encoded as JSON. */
export function validateInitDirectives(source: string): string;



/** selectionState(): IDs are author IDs, not layout indices. Absent optionals are nullish. */
export interface DiagramSelectionState {
    selectedNodeIds: string[];
    primaryNodeId?: string | null;
    activeGesture?: string | null;
    frameAvailable: boolean;
}
export type DiagramSelectionMode = "replace" | "add" | "remove" | "toggle";
export type DiagramSelectionContainment = "intersects" | "contains";



export class Diagram {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Begin a transactional box/lasso preview. Returns a decimal token or undefined if no
     * usable draw is available, another gesture owns the session, or tokens are exhausted.
     */
    beginSelectionGesture(mode?: string | null, policy?: string | null): string | undefined;
    /**
     * Roll back the matching preview. True means selection changed and should be redrawn.
     */
    cancelSelectionGesture(token: string): boolean;
    /**
     * Clear selection and revoke the current drag. A delayed cancellation cannot restore it.
     */
    clearSelection(): boolean;
    /**
     * Commit the latest accepted preview. True means committed, not selection-changed.
     */
    commitSelectionGesture(token: string): boolean;
    destroy(): void;
    /**
     * Creates a renderer for an `OffscreenCanvas` transferred to a worker.
     *
     * The offscreen 2D context implements the same CanvasRenderingContext2D
     * method surface used by `Canvas2dContext`; it is stored structurally so
     * the renderer can share the normal Canvas2D path without main-thread DOM
     * access. Event registration remains unavailable because an offscreen
     * canvas is not an `EventTarget`.
     */
    static fromOffscreenCanvas(canvas: OffscreenCanvas, config?: any | null): Diagram;
    /**
     * Return the nearest rendered edge within a backing-store-pixel tolerance.
     * Shares the draw viewport with node picking and excludes bundled-away paths.
     */
    hitTestEdge(x: number, y: number, max_distance: number): number | undefined;
    /**
     * Inspect the topmost node's interaction metadata without following a URL or calling JS.
     */
    hitTestInteraction(x: number, y: number): any;
    /**
     * Return the topmost node below a backing-store-pixel pointer, if any.
     * Uses the actual draw viewport, including fit, padding and origin normalization.
     * Resizing invalidates picking until redraw; this query never parses or lays out source.
     */
    hitTestNode(x: number, y: number): string | undefined;
    /**
     * Revoke picking and roll back an in-progress drag after an external canvas clear/reset.
     * Call even when assigning the SAME width/height: that clears pixels without a size change.
     * Geometry and committed selection remain available to redraw().
     */
    invalidateFrame(): void;
    /**
     * Move the primary keyboard selection, optionally extending the existing selected set.
     */
    navigateSelection(direction: string, extend?: boolean | null): boolean;
    constructor(canvas: HTMLCanvasElement, config?: any | null);
    on(event: string, callback: Function): void;
    /**
     * Preview a marquee from its pointer-down selection, not the previous preview.
     * Captured pointers may be outside the canvas. Wrong/retired tokens have no side effects.
     */
    previewSelectionBox(token: string, start_x: number, start_y: number, end_x: number, end_y: number): boolean;
    /**
     * Preview an even-odd lasso from interleaved backing-store x/y coordinates.
     * Length is bounded BEFORE copying out of JS memory. Invalid input throws without changing
     * selection; polygon degeneracy is handled by the native selector as an unchanged preview.
     */
    previewSelectionLasso(token: string, values: Float64Array): boolean;
    /**
     * Redraw retained geometry and current selection. No parse or layout is performed.
     * Returns false before the first render or while the backing store has zero size.
     */
    redraw(): boolean;
    render(input: string, config?: any | null): any;
    /**
     * Modifier click in backing-store pixels. Convert CSS coordinates once in the host.
     */
    selectAt(x: number, y: number, mode?: string | null): boolean;
    /**
     * Select a placed author ID. An unknown ID leaves selection and active gestures unchanged.
     */
    selectNode(id: string, mode?: string | null): boolean;
    /**
     * Return the currently selected node ID from the interactive [`fm_render_canvas::CanvasSession`] (`bd-g7sx9`).
     */
    selectedNodeId(): string | undefined;
    /**
     * Stable author IDs, primary keyboard anchor, active token and usable-frame availability.
     */
    selectionState(): any;
    setTheme(theme: string): void;
}

/**
 * Acquire the browser's `GPUCanvasContext` for a canvas.
 *
 * `web-sys` keeps the WebGPU DOM types behind unstable API flags, so this intentionally returns
 * the context as `JsValue`. The owner of the device passes receives the browser-native context
 * without a duplicate, unstable Rust binding layer.
 */
export function acquireWebGpuCanvasContext(canvas: HTMLCanvasElement): any;

/**
 * Apply a complete edited IR through the semantic family lens and return fresh source bindings.
 *
 * Unlike source-span replacement, this validates graph meaning before returning an edit. The
 * `edited_ir` object has the same shape as `parseLens(input).parsed.ir`.
 */
export function applyIrLensEdit(input: string, edited_ir: any): any;

export function applyLensEdit(input: string, element_id: string, replacement: string): any;

/**
 * Delete an element addressed by the lens, and return the post-delete snapshot with it.
 *
 * The companion to `applyParseLensEdit` for the case a replacement cannot express: an empty
 * replacement leaves the element's indentation and line terminator behind, stranding a blank line
 * per removed node. The returned snapshot is re-derived from the shortened source, because every
 * element id and span after the deletion has moved.
 */
export function applyParseLensDelete(input: string, element_id: string): any;

export function applyParseLensEdit(input: string, element_id: string, replacement: string): any;

/**
 * Insert a line after the line holding an element, matching that line's indentation and the
 * document's line ending, and return the post-insert snapshot with it.
 */
export function applyParseLensInsertLineAfter(input: string, element_id: string, text: string): any;

/**
 * Choose a render target from probed host capabilities (bd-2u0.6 item 3).
 *
 * JSON text in and out, exactly like [`worker_handle_message_js`], so the same call works from the
 * main thread, from inside the worker, and from a native test — and so the DECISION HAS ONE
 * IMPLEMENTATION. A host that re-derived the ladder in JavaScript would drift from this one, and
 * the drift would show up only in degraded environments, which are precisely the ones nobody
 * tests in.
 */
export function chooseCanvasTarget(capabilities_json: string): string;

/**
 * Return the canonical, versioned Mermaid initialization-config schema.
 *
 * Keeping this export alongside the native CLI endpoint means browser tooling can validate
 * configuration before it asks the renderer to initialize.
 */
export function configSchema(): string;

export function describeDiagram(input: string): string;

export function detectType(input: string): any;

export function diagramLens(input: string): any;

/**
 * The clickable areas of a diagram, for a host driving a canvas or WebGPU surface.
 *
 * SVG carries `click` in the document itself — a `title=` and, in link mode, an `<a href>` — so a
 * browser resolves a pointer against it with no help from us. A raster surface has no element to
 * hang an attribute on, so before this the whole `click` family was unreachable from every
 * non-SVG browser path: `renderWebGpuToRgba` returns pixels, and pixels cannot tell a host that
 * the box at (x, y) carries a URL.
 *
 * The host owns the pointer. This returns WHERE each interactive node landed and WHAT the author
 * attached to it, once per render; hit-testing a point against those rectangles is a few lines of
 * JS and does not need to cross the wasm boundary on every `mousemove`.
 *
 * Only nodes that actually carry an interaction are returned — a region per node would report the
 * whole diagram as clickable and push the filtering back onto the caller.
 *
 * # Errors
 * Returns a JS error when the runtime config cannot be resolved.
 */
export function hitRegions(input: string, config?: any | null): any;

export function init(config?: any | null): void;

export function parse(input: string): any;

export function parseLens(input: string): any;

/**
 * Prepare the WebGPU primitive plan for a diagram.
 *
 * This is the WASM-side half of the WebGPU renderer: it runs the same parse and typography-aware
 * layout path as the SVG backend, then delegates primitive extraction to `fm-render-canvas`.
 * Device creation, buffer uploads and draw submission stay with the WebGPU device pass so this API
 * cannot grow a second renderer in JavaScript.
 */
export function planWebGpu(input: string, config?: any | null): any;

/**
 * Render a diagram AND its deck manifest from one parse + one layout (epic bd-z7g6k).
 *
 * A strict superset of `renderSvg`: identical SVG bytes for identical input/config at
 * nominal pressure tier, plus `manifest` (or `null` with a structured warning when the
 * source has no deck, the family is unsupported, or no slide resolves).
 */
export function renderDeck(input: string, config?: any | null): any;

export function renderSvg(input: string, config?: any | null): string;

/**
 * Strictly validate Mermaid initialization configuration JSON and return a serializable report.
 *
 * Unlike the renderer's compatibility adapter, this rejects unknown nested keys so a typo cannot
 * silently become a default at a browser boundary.
 */
export function validateConfig(config_json: string): string;

/**
 * Strictly validate Mermaid initialization directives in source and return a serializable report.
 */
export function validateInitDirectives(source: string): string;

/**
 * The worker entry point: hand it a [`WorkerRenderMessage`] as JSON, get a
 * [`WorkerRenderResponse`] as JSON, or `null` when the message needs no reply (a cancel, or an id
 * that is not the live request).
 *
 * JSON text on both sides on purpose — a worker script can forward these straight through
 * `postMessage` with no `JsValue` dependency, which is what makes the same payload usable from the
 * main thread, a dedicated worker, and a native test.
 */
export function workerHandleMessage(message_json: string): string | undefined;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_diagram_free: (a: number, b: number) => void;
    readonly acquireWebGpuCanvasContext: (a: number, b: number) => void;
    readonly applyIrLensEdit: (a: number, b: number, c: number, d: number) => void;
    readonly applyLensEdit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly applyParseLensDelete: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly applyParseLensEdit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly applyParseLensInsertLineAfter: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly chooseCanvasTarget: (a: number, b: number, c: number) => void;
    readonly configSchema: (a: number) => void;
    readonly describeDiagram: (a: number, b: number, c: number) => void;
    readonly detectType: (a: number, b: number, c: number) => void;
    readonly diagramLens: (a: number, b: number, c: number) => void;
    readonly diagram_beginSelectionGesture: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly diagram_cancelSelectionGesture: (a: number, b: number, c: number, d: number) => void;
    readonly diagram_clearSelection: (a: number, b: number) => void;
    readonly diagram_commitSelectionGesture: (a: number, b: number, c: number, d: number) => void;
    readonly diagram_destroy: (a: number) => void;
    readonly diagram_fromOffscreenCanvas: (a: number, b: number, c: number) => void;
    readonly diagram_hitTestEdge: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_hitTestInteraction: (a: number, b: number, c: number, d: number) => void;
    readonly diagram_hitTestNode: (a: number, b: number, c: number, d: number) => void;
    readonly diagram_invalidateFrame: (a: number, b: number) => void;
    readonly diagram_navigateSelection: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_new: (a: number, b: number, c: number) => void;
    readonly diagram_on: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_previewSelectionBox: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly diagram_previewSelectionLasso: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_redraw: (a: number, b: number) => void;
    readonly diagram_render: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_selectAt: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly diagram_selectNode: (a: number, b: number, c: number, d: number, e: number, f: number) => void;
    readonly diagram_selectedNodeId: (a: number, b: number) => void;
    readonly diagram_selectionState: (a: number, b: number) => void;
    readonly diagram_setTheme: (a: number, b: number, c: number, d: number) => void;
    readonly hitRegions: (a: number, b: number, c: number, d: number) => void;
    readonly init: (a: number, b: number) => void;
    readonly parse: (a: number, b: number, c: number) => void;
    readonly parseLens: (a: number, b: number, c: number) => void;
    readonly planWebGpu: (a: number, b: number, c: number, d: number) => void;
    readonly renderDeck: (a: number, b: number, c: number, d: number) => void;
    readonly renderSvg: (a: number, b: number, c: number, d: number) => void;
    readonly validateConfig: (a: number, b: number, c: number) => void;
    readonly validateInitDirectives: (a: number, b: number, c: number) => void;
    readonly workerHandleMessage: (a: number, b: number, c: number) => void;
    readonly __wbindgen_export: (a: number, b: number) => number;
    readonly __wbindgen_export2: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export3: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;

export function sourceSpans(input: string): any[];
/**
 * @returns {any}
 */
export function capabilityMatrix(): any;
