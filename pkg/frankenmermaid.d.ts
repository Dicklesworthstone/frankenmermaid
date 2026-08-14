/* tslint:disable */
/* eslint-disable */

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

export function describeDiagram(input: string): string;

export function detectType(input: string): any;

export function diagramLens(input: string): any;

export function init(config?: any | null): void;

export function parse(input: string): any;

export function parseLens(input: string): any;

export function renderSvg(input: string, config?: any | null): string;

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
    readonly applyLensEdit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly applyParseLensDelete: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly applyParseLensEdit: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
    readonly applyParseLensInsertLineAfter: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => void;
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
    readonly init: (a: number, b: number) => void;
    readonly parse: (a: number, b: number, c: number) => void;
    readonly parseLens: (a: number, b: number, c: number) => void;
    readonly renderSvg: (a: number, b: number, c: number, d: number) => void;
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
