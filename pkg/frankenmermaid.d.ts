/* tslint:disable */
/* eslint-disable */

export class Diagram {
    free(): void;
    [Symbol.dispose](): void;
    destroy(): void;
    constructor(canvas: HTMLCanvasElement, config?: any | null);
    on(event: string, callback: Function): void;
    render(input: string, config?: any | null): any;
    setTheme(theme: string): void;
}

export function capabilityMatrix(): any;

export function detectType(input: string): any;

export function init(config?: any | null): void;

export function parse(input: string): any;

export function renderSvg(input: string, config?: any | null): string;

export function sourceSpans(input: string): any;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_diagram_free: (a: number, b: number) => void;
    readonly capabilityMatrix: (a: number) => void;
    readonly detectType: (a: number, b: number, c: number) => void;
    readonly diagram_destroy: (a: number) => void;
    readonly diagram_new: (a: number, b: number, c: number) => void;
    readonly diagram_on: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_render: (a: number, b: number, c: number, d: number, e: number) => void;
    readonly diagram_setTheme: (a: number, b: number, c: number, d: number) => void;
    readonly init: (a: number, b: number) => void;
    readonly parse: (a: number, b: number, c: number) => void;
    readonly renderSvg: (a: number, b: number, c: number, d: number) => void;
    readonly sourceSpans: (a: number, b: number, c: number) => void;
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
