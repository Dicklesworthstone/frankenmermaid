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



export class Diagram {
    free(): void;
    [Symbol.dispose](): void;
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
     * Return the nearest rendered edge index within a canvas-space tolerance.
     *
     * The query uses CGA point-to-segment distance over the latest render's edge paths, excludes
     * bundled non-rendered paths, and returns `None` for invalid coordinates or tolerance.
     */
    hitTestEdge(x: number, y: number, max_distance: number): number | undefined;
    /**
     * Return the laid-out node below a canvas-space pointer, if any.
     *
     * The query uses CGA rectangle containment against the latest render's layout, so it never
     * reparses or relayouts the diagram. Non-finite coordinates and calls before the first render
     * return `None`.
     */
    hitTestNode(x: number, y: number): string | undefined;
    constructor(canvas: HTMLCanvasElement, config?: any | null);
    on(event: string, callback: Function): void;
    render(input: string, config?: any | null): any;
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
    readonly applyLensEdit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly applyParseLensDelete: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly applyParseLensEdit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly applyParseLensInsertLineAfter: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly chooseCanvasTarget: (a: number, b: number, c: number) => void;
    readonly configSchema: (a: number) => void;
    readonly describeDiagram: (a: number, b: number, c: number) => void;
    readonly detectType: (a: number, b: number, c: number) => void;
    readonly diagramLens: (a: number, b: number, c: number) => void;
    readonly diagram_destroy: (a: number) => void;
    readonly diagram_fromOffscreenCanvas: (a: number, b: number, c: number) => void;
    readonly diagram_hitTestEdge: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_hitTestNode: (a: number, b: number, c: number, d: number) => void;
    readonly diagram_new: (a: number, b: number, c: number) => void;
    readonly diagram_on: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_render: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_setTheme: (a: number, b: number, c: number, d: number) => void;
    readonly hitRegions: (a: number, b: number, c: number, d: number) => void;
    readonly init: (a: number, b: number) => void;
    readonly parse: (a: number, b: number, c: number) => void;
    readonly parseLens: (a: number, b: number, c: number) => void;
    readonly planWebGpu: (a: number, b: number, c: number, d: number) => void;
    readonly renderDeck: (a: number, b: number, c: number, d: number) => void;
    readonly renderSvg: (a: number, b: number, c: number, d: number) => void;
    readonly validateConfig: (a: number, b: number, c: number) => void;
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
