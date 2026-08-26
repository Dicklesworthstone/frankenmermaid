/* @ts-self-types="./frankenmermaid.d.ts" */

export class Diagram {
    static __wrap(ptr) {
        const obj = Object.create(Diagram.prototype);
        obj.__wbg_ptr = ptr;
        DiagramFinalization.register(obj, obj.__wbg_ptr, obj);
        return obj;
    }
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        DiagramFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_diagram_free(ptr, 0);
    }
    destroy() {
        wasm.diagram_destroy(this.__wbg_ptr);
    }
    /**
     * Creates a renderer for an `OffscreenCanvas` transferred to a worker.
     *
     * The offscreen 2D context implements the same CanvasRenderingContext2D
     * method surface used by `Canvas2dContext`; it is stored structurally so
     * the renderer can share the normal Canvas2D path without main-thread DOM
     * access. Event registration remains unavailable because an offscreen
     * canvas is not an `EventTarget`.
     * @param {OffscreenCanvas} canvas
     * @param {any | null} [config]
     * @returns {Diagram}
     */
    static fromOffscreenCanvas(canvas, config) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.diagram_fromOffscreenCanvas(retptr, addHeapObject(canvas), isLikeNone(config) ? 0 : addHeapObject(config));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return Diagram.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Return the nearest rendered edge index within a canvas-space tolerance.
     *
     * The query uses CGA point-to-segment distance over the latest render's edge paths, excludes
     * bundled non-rendered paths, and returns `None` for invalid coordinates or tolerance.
     * @param {number} x
     * @param {number} y
     * @param {number} max_distance
     * @returns {number | undefined}
     */
    hitTestEdge(x, y, max_distance) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.diagram_hitTestEdge(retptr, this.__wbg_ptr, x, y, max_distance);
            var r0 = getDataViewMemory0().getFloat64(retptr + 8 * 0, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            return r0 === Number.MAX_SAFE_INTEGER ? undefined : r0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Return the laid-out node below a canvas-space pointer, if any.
     *
     * The query uses CGA rectangle containment against the latest render's layout, so it never
     * reparses or relayouts the diagram. Non-finite coordinates and calls before the first render
     * return `None`.
     * @param {number} x
     * @param {number} y
     * @returns {string | undefined}
     */
    hitTestNode(x, y) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.diagram_hitTestNode(retptr, this.__wbg_ptr, x, y);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeObject(r2);
            }
            let v1;
            if (r0 !== 0) {
                v1 = getStringFromWasm0(r0, r1).slice();
                wasm.__wbindgen_export4(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {HTMLCanvasElement} canvas
     * @param {any | null} [config]
     */
    constructor(canvas, config) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            wasm.diagram_new(retptr, addHeapObject(canvas), isLikeNone(config) ? 0 : addHeapObject(config));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            this.__wbg_ptr = r0;
            DiagramFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} event
     * @param {Function} callback
     */
    on(event, callback) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(event, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_on(retptr, this.__wbg_ptr, ptr0, len0, addBorrowedObject(callback));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            heap[stack_pointer++] = undefined;
        }
    }
    /**
     * @param {string} input
     * @param {any | null} [config]
     * @returns {any}
     */
    render(input, config) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_render(retptr, this.__wbg_ptr, ptr0, len0, isLikeNone(config) ? 0 : addHeapObject(config));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeObject(r1);
            }
            return takeObject(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} theme
     */
    setTheme(theme) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
            const ptr0 = passStringToWasm0(theme, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_setTheme(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeObject(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
}
if (Symbol.dispose) Diagram.prototype[Symbol.dispose] = Diagram.prototype.free;

/**
 * Acquire the browser's `GPUCanvasContext` for a canvas.
 *
 * `web-sys` keeps the WebGPU DOM types behind unstable API flags, so this intentionally returns
 * the context as `JsValue`. The owner of the device passes receives the browser-native context
 * without a duplicate, unstable Rust binding layer.
 * @param {HTMLCanvasElement} canvas
 * @returns {any}
 */
export function acquireWebGpuCanvasContext(canvas) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.acquireWebGpuCanvasContext(retptr, addHeapObject(canvas));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} input
 * @param {string} element_id
 * @param {string} replacement
 * @returns {any}
 */
export function applyLensEdit(input, element_id, replacement) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(replacement, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.applyLensEdit(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Delete an element addressed by the lens, and return the post-delete snapshot with it.
 *
 * The companion to `applyParseLensEdit` for the case a replacement cannot express: an empty
 * replacement leaves the element's indentation and line terminator behind, stranding a blank line
 * per removed node. The returned snapshot is re-derived from the shortened source, because every
 * element id and span after the deletion has moved.
 * @param {string} input
 * @param {string} element_id
 * @returns {any}
 */
export function applyParseLensDelete(input, element_id) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        wasm.applyParseLensDelete(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} input
 * @param {string} element_id
 * @param {string} replacement
 * @returns {any}
 */
export function applyParseLensEdit(input, element_id, replacement) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(replacement, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.applyParseLensEdit(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Insert a line after the line holding an element, matching that line's indentation and the
 * document's line ending, and return the post-insert snapshot with it.
 * @param {string} input
 * @param {string} element_id
 * @param {string} text
 * @returns {any}
 */
export function applyParseLensInsertLineAfter(input, element_id, text) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        const ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len1 = WASM_VECTOR_LEN;
        const ptr2 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len2 = WASM_VECTOR_LEN;
        wasm.applyParseLensInsertLineAfter(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Choose a render target from probed host capabilities (bd-2u0.6 item 3).
 *
 * JSON text in and out, exactly like [`worker_handle_message_js`], so the same call works from the
 * main thread, from inside the worker, and from a native test — and so the DECISION HAS ONE
 * IMPLEMENTATION. A host that re-derived the ladder in JavaScript would drift from this one, and
 * the drift would show up only in degraded environments, which are precisely the ones nobody
 * tests in.
 * @param {string} capabilities_json
 * @returns {string}
 */
export function chooseCanvasTarget(capabilities_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(capabilities_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.chooseCanvasTarget(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeObject(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export4(deferred3_0, deferred3_1, 1);
    }
}

/**
 * @param {string} input
 * @returns {string}
 */
export function describeDiagram(input) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.describeDiagram(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeObject(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export4(deferred3_0, deferred3_1, 1);
    }
}

/**
 * @param {string} input
 * @returns {any}
 */
export function detectType(input) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.detectType(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} input
 * @returns {any}
 */
export function diagramLens(input) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.diagramLens(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

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
 * @param {string} input
 * @param {any | null} [config]
 * @returns {any}
 */
export function hitRegions(input, config) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.hitRegions(retptr, ptr0, len0, isLikeNone(config) ? 0 : addHeapObject(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {any | null} [config]
 */
export function init(config) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        wasm.init(retptr, isLikeNone(config) ? 0 : addHeapObject(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        if (r1) {
            throw takeObject(r0);
        }
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} input
 * @returns {any}
 */
export function parse(input) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.parse(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} input
 * @returns {any}
 */
export function parseLens(input) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.parseLens(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Prepare the WebGPU primitive plan for a diagram.
 *
 * This is the WASM-side half of the WebGPU renderer: it runs the same parse and typography-aware
 * layout path as the SVG backend, then delegates primitive extraction to `fm-render-canvas`.
 * Device creation, buffer uploads and draw submission stay with the WebGPU device pass so this API
 * cannot grow a second renderer in JavaScript.
 * @param {string} input
 * @param {any | null} [config]
 * @returns {any}
 */
export function planWebGpu(input, config) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.planWebGpu(retptr, ptr0, len0, isLikeNone(config) ? 0 : addHeapObject(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Render a diagram AND its deck manifest from one parse + one layout (epic bd-z7g6k).
 *
 * A strict superset of `renderSvg`: identical SVG bytes for identical input/config at
 * nominal pressure tier, plus `manifest` (or `null` with a structured warning when the
 * source has no deck, the family is unsupported, or no slide resolves).
 * @param {string} input
 * @param {any | null} [config]
 * @returns {any}
 */
export function renderDeck(input, config) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.renderDeck(retptr, ptr0, len0, isLikeNone(config) ? 0 : addHeapObject(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeObject(r1);
        }
        return takeObject(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {string} input
 * @param {any | null} [config]
 * @returns {string}
 */
export function renderSvg(input, config) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.renderSvg(retptr, ptr0, len0, isLikeNone(config) ? 0 : addHeapObject(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeObject(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export4(deferred3_0, deferred3_1, 1);
    }
}

/**
 * The worker entry point: hand it a [`WorkerRenderMessage`] as JSON, get a
 * [`WorkerRenderResponse`] as JSON, or `null` when the message needs no reply (a cancel, or an id
 * that is not the live request).
 *
 * JSON text on both sides on purpose — a worker script can forward these straight through
 * `postMessage` with no `JsValue` dependency, which is what makes the same payload usable from the
 * main thread, a dedicated worker, and a native test.
 * @param {string} message_json
 * @returns {string | undefined}
 */
export function workerHandleMessage(message_json) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passStringToWasm0(message_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.workerHandleMessage(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        if (r3) {
            throw takeObject(r2);
        }
        let v2;
        if (r0 !== 0) {
            v2 = getStringFromWasm0(r0, r1).slice();
            wasm.__wbindgen_export4(r0, r1 * 1, 1);
        }
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_92b29b0548f8b746: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_Number_9a4e0ecb0fa16705: function(arg0) {
            const ret = Number(getObject(arg0));
            return ret;
        },
        __wbg_String_8564e559799eccda: function(arg0, arg1) {
            const ret = String(getObject(arg1));
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_boolean_get_fa956cfa2d1bd751: function(arg0) {
            const v = getObject(arg0);
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_in_aca499c5de7ff5e5: function(arg0, arg1) {
            const ret = getObject(arg0) in getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_is_null_ea9085d691f535d3: function(arg0) {
            const ret = getObject(arg0) === null;
            return ret;
        },
        __wbg___wbindgen_is_object_a27215656b807791: function(arg0) {
            const val = getObject(arg0);
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_ea5e6cc2e4141dfe: function(arg0) {
            const ret = typeof(getObject(arg0)) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_c05833b95a3cf397: function(arg0) {
            const ret = getObject(arg0) === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_db4c3b15f63fc170: function(arg0, arg1) {
            const ret = getObject(arg0) == getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_number_get_394265ed1e1b84ee: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_b0ca35b86a603356: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_344f42d3211c4765: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_addEventListener_d85450ee1320c989: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            getObject(arg0).addEventListener(getStringFromWasm0(arg1, arg2), getObject(arg3));
        }, arguments); },
        __wbg_arcTo_6d4ffb0b356f8a23: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            getObject(arg0).arcTo(arg1, arg2, arg3, arg4, arg5);
        }, arguments); },
        __wbg_arc_61372d0a8f0a988c: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            getObject(arg0).arc(arg1, arg2, arg3, arg4, arg5);
        }, arguments); },
        __wbg_beginPath_ca2dfce389ff20d2: function(arg0) {
            getObject(arg0).beginPath();
        },
        __wbg_bezierCurveTo_cb0279ca0ba5b76f: function(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            getObject(arg0).bezierCurveTo(arg1, arg2, arg3, arg4, arg5, arg6);
        },
        __wbg_clearRect_520d2bbc2437bfaa: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).clearRect(arg1, arg2, arg3, arg4);
        },
        __wbg_closePath_0e752092e41e1e22: function(arg0) {
            getObject(arg0).closePath();
        },
        __wbg_entries_015dc610cd81ede0: function(arg0) {
            const ret = Object.entries(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_fillRect_97b1f503e30148c3: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).fillRect(arg1, arg2, arg3, arg4);
        },
        __wbg_fillText_e462ba58cec15054: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).fillText(getStringFromWasm0(arg1, arg2), arg3, arg4);
        }, arguments); },
        __wbg_fill_7e2406c195723006: function(arg0) {
            getObject(arg0).fill();
        },
        __wbg_getContext_e79ddf6a9cb3cc76: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).getContext(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        }, arguments); },
        __wbg_getContext_fd298c901058eb31: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).getContext(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        }, arguments); },
        __wbg_get_507a50627bffa49b: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_get_78f252d074a84d0b: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_get_with_ref_key_6412cf3094599694: function(arg0, arg1) {
            const ret = getObject(arg0)[getObject(arg1)];
            return addHeapObject(ret);
        },
        __wbg_height_6eec812c213259a1: function(arg0) {
            const ret = getObject(arg0).height;
            return ret;
        },
        __wbg_height_f2cc35b336f266f1: function(arg0) {
            const ret = getObject(arg0).height;
            return ret;
        },
        __wbg_instanceof_ArrayBuffer_4480b9e0068a8adb: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_CanvasRenderingContext2d_2284b703b7023dcc: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof CanvasRenderingContext2D;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_309b927aaf7a3fc7: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isSafeInteger_04f36e4056f1b851: function(arg0) {
            const ret = Number.isSafeInteger(getObject(arg0));
            return ret;
        },
        __wbg_length_1f0964f4a5e2c6d8: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_length_370319915dc99107: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_lineTo_1aeefd30328165b5: function(arg0, arg1, arg2) {
            getObject(arg0).lineTo(arg1, arg2);
        },
        __wbg_measureText_c54a480a20a73a31: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).measureText(getStringFromWasm0(arg1, arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_moveTo_2618bed6b5b25622: function(arg0, arg1, arg2) {
            getObject(arg0).moveTo(arg1, arg2);
        },
        __wbg_new_32b398fb48b6d94a: function() {
            const ret = new Array();
            return addHeapObject(ret);
        },
        __wbg_new_7796ffc7ed656783: function() {
            const ret = new Map();
            return addHeapObject(ret);
        },
        __wbg_new_cd45aabdf6073e84: function(arg0) {
            const ret = new Uint8Array(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_new_da52cf8fe3429cb2: function() {
            const ret = new Object();
            return addHeapObject(ret);
        },
        __wbg_now_e7c6795a7f81e10f: function(arg0) {
            const ret = getObject(arg0).now();
            return ret;
        },
        __wbg_performance_3fcf6e32a7e1ed0a: function(arg0) {
            const ret = getObject(arg0).performance;
            return addHeapObject(ret);
        },
        __wbg_prototypesetcall_4770620bbe4688a0: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), getObject(arg2));
        },
        __wbg_push_d2ae3af0c1217ae6: function(arg0, arg1) {
            const ret = getObject(arg0).push(getObject(arg1));
            return ret;
        },
        __wbg_rect_cbf18d19ffd5b10a: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).rect(arg1, arg2, arg3, arg4);
        },
        __wbg_restore_ab535bc88702bcc0: function(arg0) {
            getObject(arg0).restore();
        },
        __wbg_rotate_6a6e81bc63bce7d8: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).rotate(arg1);
        }, arguments); },
        __wbg_save_cd0bc920468bfe2c: function(arg0) {
            getObject(arg0).save();
        },
        __wbg_setLineDash_63ce60143e4d578a: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).setLineDash(getObject(arg1));
        }, arguments); },
        __wbg_setTransform_d3001e44d696c566: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            getObject(arg0).setTransform(arg1, arg2, arg3, arg4, arg5, arg6);
        }, arguments); },
        __wbg_set_575dd786d51585f8: function(arg0, arg1, arg2) {
            const ret = getObject(arg0).set(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        },
        __wbg_set_6be42768c690e380: function(arg0, arg1, arg2) {
            getObject(arg0)[takeObject(arg1)] = takeObject(arg2);
        },
        __wbg_set_8a16b38e4805b298: function(arg0, arg1, arg2) {
            getObject(arg0)[arg1 >>> 0] = takeObject(arg2);
        },
        __wbg_set_fillStyle_4360b989b9352bbb: function(arg0, arg1, arg2) {
            getObject(arg0).fillStyle = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_font_33fee74f2c82cb6f: function(arg0, arg1, arg2) {
            getObject(arg0).font = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_globalAlpha_9b3de2f2aa9958de: function(arg0, arg1) {
            getObject(arg0).globalAlpha = arg1;
        },
        __wbg_set_lineWidth_beb3d05e36f4cc53: function(arg0, arg1) {
            getObject(arg0).lineWidth = arg1;
        },
        __wbg_set_strokeStyle_b390d5f09a6989a8: function(arg0, arg1, arg2) {
            getObject(arg0).strokeStyle = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_textAlign_75f93b22c0415d5d: function(arg0, arg1, arg2) {
            getObject(arg0).textAlign = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_textBaseline_edb08ba62ac0d3ac: function(arg0, arg1, arg2) {
            getObject(arg0).textBaseline = getStringFromWasm0(arg1, arg2);
        },
        __wbg_static_accessor_GLOBAL_4ef717fb391d88b7: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_8d1badc68b5a74f4: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_SELF_146583524fe1469b: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_WINDOW_f2829a2234d7819e: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_strokeRect_74c74060d04c703b: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).strokeRect(arg1, arg2, arg3, arg4);
        },
        __wbg_stroke_cf809e69aae41b03: function(arg0) {
            getObject(arg0).stroke();
        },
        __wbg_translate_d2b84d406c25580d: function() { return handleError(function (arg0, arg1, arg2) {
            getObject(arg0).translate(arg1, arg2);
        }, arguments); },
        __wbg_width_6d9315ecc7140ff6: function(arg0) {
            const ret = getObject(arg0).width;
            return ret;
        },
        __wbg_width_84477c442af415ce: function(arg0) {
            const ret = getObject(arg0).width;
            return ret;
        },
        __wbg_width_f9b3cbe357a34b85: function(arg0) {
            const ret = getObject(arg0).width;
            return ret;
        },
        __wbindgen_cast_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000002: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
        __wbindgen_cast_0000000000000003: function(arg0) {
            // Cast intrinsic for `U64 -> Externref`.
            const ret = BigInt.asUintN(64, arg0);
            return addHeapObject(ret);
        },
        __wbindgen_object_clone_ref: function(arg0) {
            const ret = getObject(arg0);
            return addHeapObject(ret);
        },
        __wbindgen_object_drop_ref: function(arg0) {
            takeObject(arg0);
        },
    };
    return {
        __proto__: null,
        "./frankenmermaid_bg.js": import0,
    };
}

const DiagramFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_diagram_free(ptr, 1));

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
}

function addBorrowedObject(obj) {
    if (stack_pointer == 1) throw new Error('out of js stack');
    heap[--stack_pointer] = obj;
    return stack_pointer;
}

function dropObject(idx) {
    if (idx < 1028) return;
    heap[idx] = heap_next;
    heap_next = idx;
}

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    return decodeText(ptr >>> 0, len);
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function getObject(idx) { return heap[idx]; }

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        wasm.__wbindgen_export3(addHeapObject(e));
    }
}

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function isLikeNone(x) {
    return x === undefined || x === null;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

let stack_pointer = 1024;

function takeObject(idx) {
    const ret = getObject(idx);
    dropObject(idx);
    return ret;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(ptr, len) {
    numBytesDecoded += len;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true, fatal: true });
        cachedTextDecoder.decode();
        numBytesDecoded = len;
    }
    return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = module.ok && expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;


    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('frankenmermaid_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };



const CAPABILITY_MATRIX = {"schema_version":"1.0.0","project":"frankenmermaid","status_counts":{"experimental":1,"implemented":35,"partial":2},"claims":[{"id":"diagram-type/flowchart","category":"diagram_type","title":"Support flowchart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/sequence","category":"diagram_type","title":"Support sequence diagrams","status":"partial","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as partial capability"]},{"id":"diagram-type/class","category":"diagram_type","title":"Support class diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/state","category":"diagram_type","title":"Support state diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/er","category":"diagram_type","title":"Support er diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Context","category":"diagram_type","title":"Support C4Context diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Container","category":"diagram_type","title":"Support C4Container diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Component","category":"diagram_type","title":"Support C4Component diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Dynamic","category":"diagram_type","title":"Support C4Dynamic diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Deployment","category":"diagram_type","title":"Support C4Deployment diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/architecture-beta","category":"diagram_type","title":"Support architecture-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/block-beta","category":"diagram_type","title":"Support block-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/gantt","category":"diagram_type","title":"Support gantt diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/timeline","category":"diagram_type","title":"Support timeline diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/journey","category":"diagram_type","title":"Support journey diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/gitGraph","category":"diagram_type","title":"Support gitGraph diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/sankey","category":"diagram_type","title":"Support sankey diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/mindmap","category":"diagram_type","title":"Support mindmap diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/pie","category":"diagram_type","title":"Support pie diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/quadrantChart","category":"diagram_type","title":"Support quadrantChart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/xyChart","category":"diagram_type","title":"Support xyChart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/requirementDiagram","category":"diagram_type","title":"Support requirementDiagram diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/packet-beta","category":"diagram_type","title":"Support packet-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/kanban","category":"diagram_type","title":"Support kanban diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"surface/wasm-render-deck","category":"surface","title":"WASM API renders deck manifests","status":"implemented","advertised_in":["README.md#graph-decks"],"code_paths":["crates/fm-wasm/src/lib.rs::render_deck_js","crates/fm-render-svg/src/deck.rs::render_svg_with_deck"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::render_deck_is_a_strict_superset_of_render_svg","note":"renderDeck().svg byte-equals renderSvg() at nominal pressure tier"},{"kind":"code_path","reference":"scripts/verify_deck_runtime.mjs (renders the showcase demo deck through pkg/)","note":null}],"notes":[]},{"id":"surface/cli-deck","category":"surface","title":"CLI deck command emitting a standalone HTML presentation","status":"implemented","advertised_in":["README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Deck","crates/fm-cli/src/main.rs::cmd_deck","crates/fm-render-svg/src/deck.rs::deck_manifest"],"evidence":[{"kind":"test","reference":"crates/fm-cli/tests/integration_test.rs::deck_subcommand_emits_standalone_html_and_manifest","note":"HTML contains SVG + manifest + runtime; deck/render manifest byte-equality"},{"kind":"code_path","reference":"crates/fm-cli/src/deck_template.html","note":null}],"notes":[]},{"id":"surface/cli-detect","category":"surface","title":"CLI detect command","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Detect","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::detects_flowchart_keyword","note":"Smoke coverage for type detection"},{"kind":"code_path","reference":"crates/fm-cli/src/main.rs::cmd_detect","note":null}],"notes":[]},{"id":"surface/cli-parse","category":"surface","title":"CLI parse command with IR JSON evidence","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Parse","crates/fm-parser/src/lib.rs::parse_evidence_json"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::parse_flowchart_extracts_nodes_edges_and_direction","note":"Validates parse output contains structural IR"}],"notes":[]},{"id":"surface/cli-render-svg","category":"surface","title":"CLI SVG rendering","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Render","crates/fm-render-svg/src/lib.rs::render_svg_with_layout"],"evidence":[{"kind":"test","reference":"crates/fm-render-svg/src/lib.rs::tests::prop_svg_render_is_total_and_counts_match","note":"SVG renderer smoke coverage"}],"notes":[]},{"id":"surface/cli-render-term","category":"surface","title":"CLI terminal rendering","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Render","crates/fm-render-term/src/lib.rs::render_term_with_config"],"evidence":[{"kind":"test","reference":"crates/fm-render-term/src/lib.rs::tests::render_term_produces_output","note":"Terminal renderer smoke coverage"}],"notes":[]},{"id":"surface/cli-validate","category":"surface","title":"CLI validate command with structured diagnostics","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Validate","crates/fm-core/src/lib.rs::StructuredDiagnostic"],"evidence":[{"kind":"test","reference":"crates/fm-cli/src/main.rs::tests::collect_validation_diagnostics_includes_parse_warnings","note":"Validate path emits structured diagnostics"}],"notes":[]},{"id":"surface/cli-capabilities","category":"surface","title":"CLI capability matrix command","status":"implemented","advertised_in":["README.md#command-reference","README.md#runtime-capability-metadata"],"code_paths":["crates/fm-cli/src/main.rs::Command::Capabilities","crates/fm-cli/src/main.rs::cmd_capabilities","crates/fm-core/src/lib.rs::capability_matrix"],"evidence":[{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::capability_matrix_json_matches_checked_in_artifact","note":"CLI command serializes the checked-in capability artifact"},{"kind":"code_path","reference":"crates/fm-cli/src/main.rs::cmd_capabilities","note":null}],"notes":[]},{"id":"surface/wasm-svg","category":"surface","title":"WASM API renders SVG","status":"implemented","advertised_in":["README.md#javascript--wasm-api","README.md#technical-architecture"],"code_paths":["crates/fm-wasm/src/lib.rs::render","crates/fm-wasm/src/lib.rs::render_svg_js","crates/fm-wasm/src/lib.rs::Diagram::render"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::render_returns_svg_and_type","note":"WASM facade smoke coverage"}],"notes":[]},{"id":"surface/wasm-capabilities","category":"surface","title":"WASM API exposes capability matrix metadata","status":"implemented","advertised_in":["README.md#javascript--wasm-api","README.md#runtime-capability-metadata"],"code_paths":["crates/fm-wasm/src/lib.rs::capability_matrix_js","crates/fm-core/src/lib.rs::capability_matrix"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::capability_matrix_js_returns_matrix_payload","note":"WASM surface returns the shared capability matrix"}],"notes":[]},{"id":"surface/canvas","category":"surface","title":"Canvas rendering backend","status":"implemented","advertised_in":["README.md#why-use-frankenmermaid","README.md#technical-architecture"],"code_paths":["crates/fm-render-canvas/src/lib.rs::render_to_canvas","crates/fm-wasm/src/lib.rs::Diagram::render"],"evidence":[{"kind":"test","reference":"crates/fm-render-canvas/src/lib.rs::tests::render_with_mock_context","note":"Canvas backend exercises draw pipeline"}],"notes":[]},{"id":"layout/deterministic","category":"layout","title":"Deterministic layout output","status":"implemented","advertised_in":["README.md#design-philosophy","README.md#faq"],"code_paths":["crates/fm-layout/src/lib.rs::layout_diagram_traced","crates/fm-layout/src/lib.rs::crossing_refinement"],"evidence":[{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::traced_layout_is_deterministic","note":"Checks full traced layout equality across runs"}],"notes":[]},{"id":"parser/recovery","category":"parser","title":"Best-effort parse with warnings instead of hard failure","status":"partial","advertised_in":["README.md#tl-dr","README.md#design-philosophy"],"code_paths":["crates/fm-parser/src/lib.rs::parse","crates/fm-core/src/lib.rs::MermaidWarning"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::empty_input_returns_warning","note":"Current coverage proves warning-based fallback for empty input"}],"notes":["Recovery exists, but README claims are broader than current automated evidence"]},{"id":"runtime/guard-report","category":"runtime","title":"Guard and degradation report types exist in shared IR","status":"experimental","advertised_in":["AGENTS.md#key-design-decisions","README.md#technical-architecture"],"code_paths":["crates/fm-core/src/lib.rs::MermaidGuardReport","crates/fm-core/src/lib.rs::MermaidDegradationPlan"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::MermaidDiagramMeta","note":"Types are threaded into IR metadata but not yet fully activated"}],"notes":["Data model exists; cross-pipeline activation is still an open backlog item"]}]};

function hasKnownSpan(span) {
  if (!span || !span.start || !span.end) {
    return false;
  }

  return Boolean(
    span.start.line || span.start.column || span.start.byte ||
    span.end.line || span.end.column || span.end.byte
  );
}

function sanitizeFragment(raw) {
  let out = "";
  let lastWasDash = false;

  for (const ch of String(raw ?? "")) {
    if ((ch >= "0" && ch <= "9") || (ch >= "A" && ch <= "Z") || (ch >= "a" && ch <= "z")) {
      out += ch.toLowerCase();
      lastWasDash = false;
    } else if (!lastWasDash && out.length > 0) {
      out += "-";
      lastWasDash = true;
    }
  }

  return out.replace(/^-+|-+$/g, "");
}

function nodeElementId(nodeId, index) {
  const fragment = sanitizeFragment(nodeId);
  return fragment ? `fm-node-${fragment}-${index}` : `fm-node-${index}`;
}

function stringifySourceId(value) {
  if (value == null) {
    return undefined;
  }
  if (typeof value === "number" || typeof value === "string") {
    return String(value);
  }
  if (Array.isArray(value) && value.length > 0) {
    return String(value[0]);
  }
  if (typeof value === "object" && 0 in value) {
    return String(value[0]);
  }
  return String(value);
}

export function sourceSpans(input) {
  const parsed = parse(input);
  const ir = parsed && parsed.ir ? parsed.ir : {};
  const records = [];
  const nodes = Array.isArray(ir.nodes) ? ir.nodes : [];
  const edges = Array.isArray(ir.edges) ? ir.edges : [];
  const clusters = Array.isArray(ir.clusters) ? ir.clusters : [];

  nodes.forEach((node, index) => {
    const span = node?.span_primary ?? node?.spanPrimary;
    if (!hasKnownSpan(span)) {
      return;
    }
    const sourceId = typeof node?.id === "string" && node.id.length > 0 ? node.id : undefined;
    records.push({
      kind: "node",
      index,
      id: sourceId,
      elementId: nodeElementId(sourceId ?? "", index),
      span,
    });
  });

  edges.forEach((edge, index) => {
    if (!hasKnownSpan(edge?.span)) {
      return;
    }
    records.push({
      kind: "edge",
      index,
      elementId: `fm-edge-${index}`,
      span: edge.span,
    });
  });

  clusters.forEach((cluster, index) => {
    if (!hasKnownSpan(cluster?.span)) {
      return;
    }
    records.push({
      kind: "cluster",
      index,
      id: stringifySourceId(cluster?.id),
      elementId: `fm-cluster-${index}`,
      span: cluster.span,
    });
  });

  return records;
}

export function capabilityMatrix() {
  return CAPABILITY_MATRIX;
}

