/* @ts-self-types="./frankenmermaid.d.ts" */

/**
 * A retained Canvas renderer with initialization defaults captured at construction.
 * Successful per-render overrides become this instance's defaults; later global `init` calls
 * affect new renderers, while source directives apply only to the current diagram.
 */
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
    /**
     * Begin a transactional box/lasso preview. Returns a decimal token or undefined if no
     * usable draw is available, another gesture owns the session, or tokens are exhausted.
     * @param {string | null} [mode]
     * @param {string | null} [policy]
     * @returns {string | undefined}
     */
    beginSelectionGesture(mode, policy) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            let ptr0, len0, ptr1, len1;
            try {
                ptr0 = isLikeNone(mode) ? 0 : passStringToWasm0(mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
                len0 = WASM_VECTOR_LEN;
                ptr1 = isLikeNone(policy) ? 0 : passStringToWasm0(policy, wasm.__wbindgen_export, wasm.__wbindgen_export2);
                len1 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr1) { wasm.__wbindgen_export3(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export3(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.diagram_beginSelectionGesture(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeFromExternrefTable0(r2);
            }
            let v3;
            if (r0 !== 0) {
                v3 = getStringFromWasm0(r0, r1);
                wasm.__wbindgen_export3(r0, r1 * 1, 1);
            }
            return v3;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Roll back the matching preview. True means selection changed and should be redrawn.
     * @param {string} token
     * @returns {boolean}
     */
    cancelSelectionGesture(token) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(token, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_cancelSelectionGesture(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Clear selection and revoke the current drag. A delayed cancellation cannot restore it.
     * @returns {boolean}
     */
    clearSelection() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_clearSelection(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Commit the latest accepted preview. True means committed, not selection-changed.
     * @param {string} token
     * @returns {boolean}
     */
    commitSelectionGesture(token) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(token, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_commitSelectionGesture(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
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
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_fromOffscreenCanvas(retptr, canvas, isLikeNone(config) ? 0 : addToExternrefTable0(config));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return Diagram.__wrap(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Return the nearest rendered edge within a backing-store-pixel tolerance.
     * Shares the draw viewport with node picking and excludes bundled-away paths.
     * @param {number} x
     * @param {number} y
     * @param {number} max_distance
     * @returns {number | undefined}
     */
    hitTestEdge(x, y, max_distance) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_hitTestEdge(retptr, this.__wbg_ptr, x, y, max_distance);
            var r0 = getDataViewMemory0().getFloat64(retptr + 8 * 0, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeFromExternrefTable0(r2);
            }
            return r0 === Number.MAX_SAFE_INTEGER ? undefined : r0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Return the topmost node's link, callback, tooltip and actor-menu metadata, or null.
     * Coordinates are backing-store pixels in the last completed draw's viewport. A resize
     * invalidates the result until redraw. This only queries metadata; the host owns activation.
     * @param {number} x
     * @param {number} y
     * @returns {any}
     */
    hitTestInteraction(x, y) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_hitTestInteraction(retptr, this.__wbg_ptr, x, y);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return takeFromExternrefTable0(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Return the topmost node below a backing-store-pixel pointer, if any.
     * Uses the actual draw viewport, including fit, padding and origin normalization.
     * Resizing invalidates picking until redraw; this query never parses or lays out source.
     * @param {number} x
     * @param {number} y
     * @returns {string | undefined}
     */
    hitTestNode(x, y) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_hitTestNode(retptr, this.__wbg_ptr, x, y);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeFromExternrefTable0(r2);
            }
            let v1;
            if (r0 !== 0) {
                v1 = getStringFromWasm0(r0, r1);
                wasm.__wbindgen_export3(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Revoke picking and roll back an in-progress drag after an external canvas clear/reset.
     * Call even when assigning the SAME width/height: that clears pixels without a size change.
     * Geometry and committed selection remain available to redraw().
     */
    invalidateFrame() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_invalidateFrame(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeFromExternrefTable0(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Move the primary keyboard selection, optionally extending the existing selected set.
     * @param {string} direction
     * @param {boolean | null} [extend]
     * @returns {boolean}
     */
    navigateSelection(direction, extend) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(direction, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_navigateSelection(retptr, this.__wbg_ptr, ptr0, len0, isLikeNone(extend) ? 0xFFFFFF : extend ? 1 : 0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
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
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_new(retptr, canvas, isLikeNone(config) ? 0 : addToExternrefTable0(config));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
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
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(event, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_on(retptr, this.__wbg_ptr, ptr0, len0, callback);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeFromExternrefTable0(r0);
            }
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Preview a marquee from its pointer-down selection, not the previous preview.
     * Captured pointers may be outside the canvas. Wrong/retired tokens have no side effects.
     * @param {string} token
     * @param {number} start_x
     * @param {number} start_y
     * @param {number} end_x
     * @param {number} end_y
     * @returns {boolean}
     */
    previewSelectionBox(token, start_x, start_y, end_x, end_y) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const val2 = +start_x;
            const val3 = +start_y;
            const val4 = +end_x;
            const val5 = +end_y;
            const ptr0 = passStringToWasm0(token, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_previewSelectionBox(retptr, this.__wbg_ptr, ptr0, len0, val2, val3, val4, val5);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Preview an even-odd lasso from interleaved backing-store x/y coordinates.
     * Length is bounded BEFORE copying out of JS memory. Invalid input throws without changing
     * selection; polygon degeneracy is handled by the native selector as an unchanged preview.
     * @param {string} token
     * @param {Float64Array} values
     * @returns {boolean}
     */
    previewSelectionLasso(token, values) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(token, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_previewSelectionLasso(retptr, this.__wbg_ptr, ptr0, len0, values);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Redraw retained geometry and current selection. No parse or layout is performed.
     * Returns false before the first render or while the backing store has zero size.
     * @returns {boolean}
     */
    redraw() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_redraw(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} input
     * @param {any | null} [config]
     * @returns {any}
     */
    render(input, config) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_render(retptr, this.__wbg_ptr, ptr0, len0, isLikeNone(config) ? 0 : addToExternrefTable0(config));
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return takeFromExternrefTable0(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Modifier click in backing-store pixels. Convert CSS coordinates once in the host.
     * @param {number} x
     * @param {number} y
     * @param {string | null} [mode]
     * @returns {boolean}
     */
    selectAt(x, y, mode) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const val2 = +x;
            const val3 = +y;
            var ptr0 = isLikeNone(mode) ? 0 : passStringToWasm0(mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len0 = WASM_VECTOR_LEN;
            wasm.diagram_selectAt(retptr, this.__wbg_ptr, val2, val3, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Select a placed author ID. An unknown ID leaves selection and active gestures unchanged.
     * @param {string} id
     * @param {string | null} [mode]
     * @returns {boolean}
     */
    selectNode(id, mode) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            let ptr0, len0, ptr1, len1;
            try {
                ptr0 = passStringToWasm0(id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
                len0 = WASM_VECTOR_LEN;
                ptr1 = isLikeNone(mode) ? 0 : passStringToWasm0(mode, wasm.__wbindgen_export, wasm.__wbindgen_export2);
                len1 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr1) { wasm.__wbindgen_export3(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export3(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.diagram_selectNode(retptr, this.__wbg_ptr, ptr0, len0, ptr1, len1);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return r0 !== 0;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Return the currently selected node ID from the interactive [`fm_render_canvas::CanvasSession`] (`bd-g7sx9`).
     * @returns {string | undefined}
     */
    selectedNodeId() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_selectedNodeId(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            if (r3) {
                throw takeFromExternrefTable0(r2);
            }
            let v1;
            if (r0 !== 0) {
                v1 = getStringFromWasm0(r0, r1);
                wasm.__wbindgen_export3(r0, r1 * 1, 1);
            }
            return v1;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Stable author IDs, primary keyboard anchor, active token and usable-frame availability.
     * @returns {any}
     */
    selectionState() {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            wasm.diagram_selectionState(retptr, this.__wbg_ptr);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            return takeFromExternrefTable0(r0);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * @param {string} theme
     */
    setTheme(theme) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const ptr0 = passStringToWasm0(theme, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len0 = WASM_VECTOR_LEN;
            wasm.diagram_setTheme(retptr, this.__wbg_ptr, ptr0, len0);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            if (r1) {
                throw takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        wasm.acquireWebGpuCanvasContext(retptr, canvas);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * Apply a complete edited IR through the semantic family lens and return fresh source bindings.
 *
 * Unlike source-span replacement, this validates graph meaning before returning an edit. The
 * `edited_ir` object has the same shape as `parseLens(input).parsed.ir`.
 * @param {string} input
 * @param {any} edited_ir
 * @returns {any}
 */
export function applyIrLensEdit(input, edited_ir) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.applyIrLensEdit(retptr, ptr0, len0, edited_ir);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1, ptr2, len2;
        try {
            ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passStringToWasm0(replacement, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len2 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr2) { wasm.__wbindgen_export3(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_export3(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export3(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.applyLensEdit(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1;
        try {
            ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len1 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr1) { wasm.__wbindgen_export3(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export3(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.applyParseLensDelete(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1, ptr2, len2;
        try {
            ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passStringToWasm0(replacement, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len2 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr2) { wasm.__wbindgen_export3(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_export3(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export3(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.applyParseLensEdit(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1, ptr2, len2;
        try {
            ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(element_id, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passStringToWasm0(text, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            len2 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr2) { wasm.__wbindgen_export3(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_export3(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export3(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.applyParseLensInsertLineAfter(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
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
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Return the canonical, versioned Mermaid initialization-config schema.
 *
 * Keeping this export alongside the native CLI endpoint means browser tooling can validate
 * configuration before it asks the renderer to initialize.
 * @returns {string}
 */
export function configSchema() {
    let deferred1_0;
    let deferred1_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        wasm.configSchema(retptr);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        deferred1_0 = r0;
        deferred1_1 = r1;
        return getStringFromWasm0(r0, r1);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred1_0, deferred1_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
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
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}

/**
 * @param {string} input
 * @returns {any}
 */
export function detectType(input) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.detectType(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.diagramLens(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.hitRegions(retptr, ptr0, len0, isLikeNone(config) ? 0 : addToExternrefTable0(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * @param {any | null} [config]
 */
export function init(config) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        wasm.init(retptr, isLikeNone(config) ? 0 : addToExternrefTable0(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        if (r1) {
            throw takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.parse(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.parseLens(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.planWebGpu(retptr, ptr0, len0, isLikeNone(config) ? 0 : addToExternrefTable0(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.renderDeck(retptr, ptr0, len0, isLikeNone(config) ? 0 : addToExternrefTable0(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        if (r2) {
            throw takeFromExternrefTable0(r1);
        }
        return takeFromExternrefTable0(r0);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(input, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.renderSvg(retptr, ptr0, len0, isLikeNone(config) ? 0 : addToExternrefTable0(config));
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Strictly validate Mermaid initialization configuration JSON and return a serializable report.
 *
 * Unlike the renderer's compatibility adapter, this rejects unknown nested keys so a typo cannot
 * silently become a default at a browser boundary.
 * @param {string} config_json
 * @returns {string}
 */
export function validateConfig(config_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(config_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.validateConfig(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Strictly validate Mermaid initialization directives in source and return a serializable report.
 * @param {string} source
 * @returns {string}
 */
export function validateInitDirectives(source) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(source, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.validateInitDirectives(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export3(deferred3_0, deferred3_1, 1);
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
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(message_json, wasm.__wbindgen_export, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.workerHandleMessage(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        if (r3) {
            throw takeFromExternrefTable0(r2);
        }
        let v2;
        if (r0 !== 0) {
            v2 = getStringFromWasm0(r0, r1);
            wasm.__wbindgen_export3(r0, r1 * 1, 1);
        }
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg_Number_ff192a72a0dcdf6d: function(arg0) {
            const ret = Number(arg0);
            return ret;
        },
        __wbg_String_8564e559799eccda: function(arg0, arg1) {
            const ret = String(arg1);
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_bigint_get_as_i64_11da4b52cd0aedef: function(arg0, arg1) {
            const v = arg1;
            const ret = typeof(v) === 'bigint' ? v : undefined;
            getDataViewMemory0().setBigInt64((arg0 >>> 0) + 8 * 1, isLikeNone(ret) ? BigInt(0) : ret, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_boolean_get_4f1baf9a00be3c74: function(arg0) {
            const v = arg0;
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_in_3f58a9d3b3a7eed2: function(arg0, arg1) {
            const ret = arg0 in arg1;
            return ret;
        },
        __wbg___wbindgen_is_bigint_5fa10641e039d3ef: function(arg0) {
            const ret = typeof(arg0) === 'bigint';
            return ret;
        },
        __wbg___wbindgen_is_function_741ff2bb612836f5: function(arg0) {
            const ret = typeof(arg0) === 'function';
            return ret;
        },
        __wbg___wbindgen_is_null_c22311b1cc228e3a: function(arg0) {
            const ret = arg0 === null;
            return ret;
        },
        __wbg___wbindgen_is_object_8b968c3ab8e96595: function(arg0) {
            const val = arg0;
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_fc456fee5d3317de: function(arg0) {
            const ret = typeof(arg0) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_be5b34249ff1fc15: function(arg0) {
            const ret = arg0 === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_eq_38d7bfbe4cc84039: function(arg0, arg1) {
            const ret = arg0 === arg1;
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_b2f214e2e4b30e36: function(arg0, arg1) {
            const ret = arg0 == arg1;
            return ret;
        },
        __wbg___wbindgen_number_get_934b640d0889af78: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64((arg0 >>> 0) + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_7ce8eee7fd327545: function(arg0, arg1) {
            const obj = arg1;
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_4c88d8bf264fd984: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_addEventListener_847fd86b8c33ec43: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            arg0.addEventListener(getStringFromWasm0(arg1, arg2), arg3);
        }, arguments); },
        __wbg_arcTo_a347b807c468785d: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.arcTo(arg1, arg2, arg3, arg4, arg5);
        }, arguments); },
        __wbg_arc_df3f7eb77e59876f: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            arg0.arc(arg1, arg2, arg3, arg4, arg5);
        }, arguments); },
        __wbg_beginPath_ab690dc0c7efe6dc: function(arg0) {
            arg0.beginPath();
        },
        __wbg_bezierCurveTo_fcaa1adffbe3419a: function(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            arg0.bezierCurveTo(arg1, arg2, arg3, arg4, arg5, arg6);
        },
        __wbg_call_40b008ba32f73991: function() { return handleError(function (arg0, arg1) {
            const ret = arg0.call(arg1);
            return ret;
        }, arguments); },
        __wbg_clearRect_428f96a6ba73b2f4: function(arg0, arg1, arg2, arg3, arg4) {
            arg0.clearRect(arg1, arg2, arg3, arg4);
        },
        __wbg_clip_26239db7c0b67972: function(arg0) {
            arg0.clip();
        },
        __wbg_closePath_e9e11e034c5d75a3: function(arg0) {
            arg0.closePath();
        },
        __wbg_done_57e08d0be6e30c02: function(arg0) {
            const ret = arg0.done;
            return ret;
        },
        __wbg_entries_7bb1459bd9b5c2b9: function(arg0) {
            const ret = Object.entries(arg0);
            return ret;
        },
        __wbg_fillRect_e1f0b9915220348b: function(arg0, arg1, arg2, arg3, arg4) {
            arg0.fillRect(arg1, arg2, arg3, arg4);
        },
        __wbg_fillText_89d353a9ad9f4a1a: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            arg0.fillText(getStringFromWasm0(arg1, arg2), arg3, arg4);
        }, arguments); },
        __wbg_fill_cb36fe21a8d46cd7: function(arg0) {
            arg0.fill();
        },
        __wbg_getContext_1938edb9a972f63d: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.getContext(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_getContext_d93f904b714852ae: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.getContext(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        }, arguments); },
        __wbg_get_2080c49015e35254: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_635eea311fa1a291: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(arg0, arg1);
            return ret;
        }, arguments); },
        __wbg_get_d7176cfb76d71114: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_get_index_4b7ea14f5ecf64df: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_get_unchecked_e6325ad374eff861: function(arg0, arg1) {
            const ret = arg0[arg1 >>> 0];
            return ret;
        },
        __wbg_get_with_ref_key_fc601247060b7204: function(arg0, arg1) {
            const ret = arg0[arg1];
            return ret;
        },
        __wbg_height_a6466ebf29036658: function(arg0) {
            const ret = arg0.height;
            return ret;
        },
        __wbg_height_ac6f1c3f49c13fe2: function(arg0) {
            const ret = arg0.height;
            return ret;
        },
        __wbg_instanceof_ArrayBuffer_12cdaa4161423e6b: function(arg0) {
            let result;
            try {
                result = arg0 instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_CanvasRenderingContext2d_72a60be8aaadecab: function(arg0) {
            let result;
            try {
                result = arg0 instanceof CanvasRenderingContext2D;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Float64Array_4acace5b49aaf791: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Float64Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Map_0b4fc8b62e73f163: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Map;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_d7aeb0dcb3108fea: function(arg0) {
            let result;
            try {
                result = arg0 instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isArray_bcf1376d0d732d08: function(arg0) {
            const ret = Array.isArray(arg0);
            return ret;
        },
        __wbg_isSafeInteger_1a35ba4d31e71d38: function(arg0) {
            const ret = Number.isSafeInteger(arg0);
            return ret;
        },
        __wbg_iterator_e4c5e8f68ada7b58: function() {
            const ret = Symbol.iterator;
            return ret;
        },
        __wbg_length_186163212ffe79c3: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_5a8bb8abe1e54e15: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_length_b03a0fabb109b1e9: function(arg0) {
            const ret = arg0.length;
            return ret;
        },
        __wbg_lineTo_91262b25fb37c461: function(arg0, arg1, arg2) {
            arg0.lineTo(arg1, arg2);
        },
        __wbg_measureText_d831259b682a3f7d: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = arg0.measureText(getStringFromWasm0(arg1, arg2));
            return ret;
        }, arguments); },
        __wbg_moveTo_b4cd3b0b7e1585a5: function(arg0, arg1, arg2) {
            arg0.moveTo(arg1, arg2);
        },
        __wbg_new_13de428334042d26: function(arg0) {
            const ret = new Uint8Array(arg0);
            return ret;
        },
        __wbg_new_37eb45aaf5eeb820: function() {
            const ret = new Array();
            return ret;
        },
        __wbg_new_7075786089b89c5e: function() {
            const ret = new Map();
            return ret;
        },
        __wbg_new_cd4348290afae586: function() {
            const ret = new Object();
            return ret;
        },
        __wbg_next_178c1651c11432b3: function(arg0) {
            const ret = arg0.next;
            return ret;
        },
        __wbg_next_af5ac6a08bb0a717: function() { return handleError(function (arg0) {
            const ret = arg0.next();
            return ret;
        }, arguments); },
        __wbg_now_e7c6795a7f81e10f: function(arg0) {
            const ret = arg0.now();
            return ret;
        },
        __wbg_performance_7e4e4494fdc9d07a: function(arg0) {
            const ret = arg0.performance;
            return ret;
        },
        __wbg_prototypesetcall_13162965d1fe7aa3: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
        },
        __wbg_push_c9f4893fc209f768: function(arg0, arg1) {
            const ret = arg0.push(arg1);
            return ret;
        },
        __wbg_quadraticCurveTo_f9dba59fd1d7d276: function(arg0, arg1, arg2, arg3, arg4) {
            arg0.quadraticCurveTo(arg1, arg2, arg3, arg4);
        },
        __wbg_rect_ab5601e64afdbc33: function(arg0, arg1, arg2, arg3, arg4) {
            arg0.rect(arg1, arg2, arg3, arg4);
        },
        __wbg_resetTransform_63b910188c95e29b: function() { return handleError(function (arg0) {
            arg0.resetTransform();
        }, arguments); },
        __wbg_restore_f322d9dcee2d3472: function(arg0) {
            arg0.restore();
        },
        __wbg_rotate_c33a3db69ad81687: function() { return handleError(function (arg0, arg1) {
            arg0.rotate(arg1);
        }, arguments); },
        __wbg_save_1375b4c2f3596b8b: function(arg0) {
            arg0.save();
        },
        __wbg_setLineDash_01dfd4d9d06df1ea: function() { return handleError(function (arg0, arg1) {
            arg0.setLineDash(arg1);
        }, arguments); },
        __wbg_setTransform_ee8f5d076fcdec0f: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            arg0.setTransform(arg1, arg2, arg3, arg4, arg5, arg6);
        }, arguments); },
        __wbg_set_8282f7e0d2d01336: function(arg0, arg1, arg2) {
            const ret = arg0.set(arg1, arg2);
            return ret;
        },
        __wbg_set_d2d33b52458e984a: function(arg0, arg1, arg2) {
            arg0[arg1 >>> 0] = arg2;
        },
        __wbg_set_e4b9a833d288759a: function(arg0, arg1, arg2) {
            arg0[arg1] = arg2;
        },
        __wbg_set_fillStyle_95fde67582a9c695: function(arg0, arg1, arg2) {
            arg0.fillStyle = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_font_aa0c0c514c73a5fa: function(arg0, arg1, arg2) {
            arg0.font = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_globalAlpha_4bdefe001e9c79ce: function(arg0, arg1) {
            arg0.globalAlpha = arg1;
        },
        __wbg_set_lineCap_e73c68b48b40a360: function(arg0, arg1, arg2) {
            arg0.lineCap = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_lineJoin_c9ff5021d8b8197a: function(arg0, arg1, arg2) {
            arg0.lineJoin = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_lineWidth_12f039d09cd36dad: function(arg0, arg1) {
            arg0.lineWidth = arg1;
        },
        __wbg_set_shadowBlur_85cdec581d1dc1d5: function(arg0, arg1) {
            arg0.shadowBlur = arg1;
        },
        __wbg_set_shadowColor_b938633d4fa98064: function(arg0, arg1, arg2) {
            arg0.shadowColor = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_shadowOffsetX_d90df5fa6efe0d44: function(arg0, arg1) {
            arg0.shadowOffsetX = arg1;
        },
        __wbg_set_shadowOffsetY_244217183c1e23d9: function(arg0, arg1) {
            arg0.shadowOffsetY = arg1;
        },
        __wbg_set_strokeStyle_b90c8a6e0d37c80d: function(arg0, arg1, arg2) {
            arg0.strokeStyle = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_textAlign_0a32c51401892c11: function(arg0, arg1, arg2) {
            arg0.textAlign = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_textBaseline_ff2e3c5c84684b23: function(arg0, arg1, arg2) {
            arg0.textBaseline = getStringFromWasm0(arg1, arg2);
        },
        __wbg_static_accessor_GLOBAL_THIS_003ee833af9699fc: function() {
            const ret = typeof globalThis === 'undefined' ? undefined : globalThis;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_GLOBAL_e4293fb22cbe77da: function() {
            const ret = typeof global === 'undefined' ? undefined : global;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_SELF_53bb20457401453d: function() {
            const ret = typeof self === 'undefined' ? undefined : self;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_static_accessor_WINDOW_b5e0810bfecb7feb: function() {
            const ret = typeof window === 'undefined' ? undefined : window;
            return isLikeNone(ret) ? 0 : addToExternrefTable0(ret);
        },
        __wbg_strokeRect_92ff808375332184: function(arg0, arg1, arg2, arg3, arg4) {
            arg0.strokeRect(arg1, arg2, arg3, arg4);
        },
        __wbg_stroke_19fa3f3ec093bc25: function(arg0) {
            arg0.stroke();
        },
        __wbg_translate_73a88129c5d9f3b2: function() { return handleError(function (arg0, arg1, arg2) {
            arg0.translate(arg1, arg2);
        }, arguments); },
        __wbg_value_e0f328965e184931: function(arg0) {
            const ret = arg0.value;
            return ret;
        },
        __wbg_width_000813f5f73b7341: function(arg0) {
            const ret = arg0.width;
            return ret;
        },
        __wbg_width_28f5533f1b3b5abc: function(arg0) {
            const ret = arg0.width;
            return ret;
        },
        __wbg_width_babc5dc7654771b1: function(arg0) {
            const ret = arg0.width;
            return ret;
        },
        __wbindgen_generic_0000000000000001: function(arg0) {
            // Cast intrinsic for `F64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_generic_0000000000000002: function(arg0) {
            // Cast intrinsic for `I64 -> Externref`.
            const ret = arg0;
            return ret;
        },
        __wbindgen_generic_0000000000000003: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return ret;
        },
        __wbindgen_generic_0000000000000004: function(arg0) {
            // Cast intrinsic for `U64 -> Externref`.
            const ret = BigInt.asUintN(64, arg0);
            return ret;
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
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

function addToExternrefTable0(obj) {
    const idx = wasm.__wbindgen_export5();
    wasm.__wbindgen_externrefs.set(idx, obj);
    return idx;
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
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function handleError(f, args) {
    try {
        return f.apply(this, args);
    } catch (e) {
        const idx = addToExternrefTable0(e);
        wasm.__wbindgen_export4(idx);
    }
}

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

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__wbindgen_export6(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
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
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

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

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

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



const CAPABILITY_MATRIX = {"schema_version":"1.0.0","project":"frankenmermaid","status_counts":{"experimental":1,"implemented":67,"partial":2},"claims":[{"id":"diagram-type/flowchart","category":"diagram_type","title":"Support flowchart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/sequence","category":"diagram_type","title":"Support sequence diagrams","status":"partial","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as partial capability"]},{"id":"diagram-type/class","category":"diagram_type","title":"Support class diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/state","category":"diagram_type","title":"Support state diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/er","category":"diagram_type","title":"Support er diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Context","category":"diagram_type","title":"Support C4Context diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Container","category":"diagram_type","title":"Support C4Container diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Component","category":"diagram_type","title":"Support C4Component diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Dynamic","category":"diagram_type","title":"Support C4Dynamic diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Deployment","category":"diagram_type","title":"Support C4Deployment diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/architecture-beta","category":"diagram_type","title":"Support architecture-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/block-beta","category":"diagram_type","title":"Support block-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/gantt","category":"diagram_type","title":"Support gantt diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/timeline","category":"diagram_type","title":"Support timeline diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/journey","category":"diagram_type","title":"Support journey diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/gitGraph","category":"diagram_type","title":"Support gitGraph diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/sankey","category":"diagram_type","title":"Support sankey diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/mindmap","category":"diagram_type","title":"Support mindmap diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/pie","category":"diagram_type","title":"Support pie diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/quadrantChart","category":"diagram_type","title":"Support quadrantChart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/xyChart","category":"diagram_type","title":"Support xyChart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/requirementDiagram","category":"diagram_type","title":"Support requirementDiagram diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/packet-beta","category":"diagram_type","title":"Support packet-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/kanban","category":"diagram_type","title":"Support kanban diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/treemap","category":"diagram_type","title":"Support treemap diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/radar-beta","category":"diagram_type","title":"Support radar-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/info","category":"diagram_type","title":"Support info diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/ishikawa","category":"diagram_type","title":"Support ishikawa diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/treeView-beta","category":"diagram_type","title":"Support treeView-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/venn-beta","category":"diagram_type","title":"Support venn-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/wardley-beta","category":"diagram_type","title":"Support wardley-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/eventmodeling","category":"diagram_type","title":"Support eventmodeling diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"surface/wasm-render-deck","category":"surface","title":"WASM API renders deck manifests","status":"implemented","advertised_in":["README.md#graph-decks"],"code_paths":["crates/fm-wasm/src/lib.rs::render_deck_js","crates/fm-render-svg/src/deck.rs::render_svg_with_deck"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::render_deck_is_a_strict_superset_of_render_svg","note":"renderDeck().svg byte-equals renderSvg() at nominal pressure tier"},{"kind":"code_path","reference":"scripts/verify_deck_runtime.mjs (renders the showcase demo deck through pkg/)","note":null}],"notes":[]},{"id":"surface/cli-deck","category":"surface","title":"CLI deck command emitting a standalone HTML presentation","status":"implemented","advertised_in":["README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Deck","crates/fm-cli/src/main.rs::cmd_deck","crates/fm-render-svg/src/deck.rs::deck_manifest"],"evidence":[{"kind":"test","reference":"crates/fm-cli/tests/integration_test.rs::deck_subcommand_emits_standalone_html_and_manifest","note":"HTML contains SVG + manifest + runtime; deck/render manifest byte-equality"},{"kind":"code_path","reference":"crates/fm-cli/src/deck_template.html","note":null}],"notes":[]},{"id":"surface/cli-detect","category":"surface","title":"CLI detect command","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Detect","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::detects_flowchart_keyword","note":"Smoke coverage for type detection"},{"kind":"code_path","reference":"crates/fm-cli/src/main.rs::cmd_detect","note":null}],"notes":[]},{"id":"surface/cli-parse","category":"surface","title":"CLI parse command with IR JSON evidence","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Parse","crates/fm-parser/src/lib.rs::parse_evidence_json"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::parse_flowchart_extracts_nodes_edges_and_direction","note":"Validates parse output contains structural IR"}],"notes":[]},{"id":"surface/cli-render-svg","category":"surface","title":"CLI SVG rendering","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Render","crates/fm-render-svg/src/lib.rs::render_svg_with_layout"],"evidence":[{"kind":"test","reference":"crates/fm-render-svg/src/lib.rs::tests::prop_svg_render_is_total_and_counts_match","note":"SVG renderer smoke coverage"}],"notes":[]},{"id":"surface/cli-render-term","category":"surface","title":"CLI terminal rendering","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Render","crates/fm-render-term/src/lib.rs::render_term_with_config"],"evidence":[{"kind":"test","reference":"crates/fm-render-term/src/lib.rs::tests::render_term_produces_output","note":"Terminal renderer smoke coverage"}],"notes":[]},{"id":"surface/cli-validate","category":"surface","title":"CLI validate command with structured diagnostics","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Validate","crates/fm-core/src/lib.rs::StructuredDiagnostic"],"evidence":[{"kind":"test","reference":"crates/fm-cli/src/main.rs::tests::collect_validation_diagnostics_includes_parse_warnings","note":"Validate path emits structured diagnostics"}],"notes":[]},{"id":"surface/cli-capabilities","category":"surface","title":"CLI capability matrix command","status":"implemented","advertised_in":["README.md#command-reference","README.md#runtime-capability-metadata"],"code_paths":["crates/fm-cli/src/main.rs::Command::Capabilities","crates/fm-cli/src/main.rs::cmd_capabilities","crates/fm-core/src/lib.rs::capability_matrix"],"evidence":[{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::capability_matrix_json_matches_checked_in_artifact","note":"CLI command serializes the checked-in capability artifact"},{"kind":"code_path","reference":"crates/fm-cli/src/main.rs::cmd_capabilities","note":null}],"notes":[]},{"id":"surface/wasm-svg","category":"surface","title":"WASM API renders SVG","status":"implemented","advertised_in":["README.md#javascript--wasm-api","README.md#technical-architecture"],"code_paths":["crates/fm-wasm/src/lib.rs::render","crates/fm-wasm/src/lib.rs::render_svg_js","crates/fm-wasm/src/lib.rs::Diagram::render"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::render_returns_svg_and_type","note":"WASM facade smoke coverage"}],"notes":[]},{"id":"surface/wasm-capabilities","category":"surface","title":"WASM API exposes capability matrix metadata","status":"implemented","advertised_in":["README.md#javascript--wasm-api","README.md#runtime-capability-metadata"],"code_paths":["crates/fm-wasm/src/lib.rs::capability_matrix_js","crates/fm-core/src/lib.rs::capability_matrix"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::capability_matrix_js_returns_matrix_payload","note":"WASM surface returns the shared capability matrix"}],"notes":[]},{"id":"surface/canvas","category":"surface","title":"Canvas rendering backend","status":"implemented","advertised_in":["README.md#why-use-frankenmermaid","README.md#technical-architecture"],"code_paths":["crates/fm-render-canvas/src/lib.rs::render_to_canvas","crates/fm-wasm/src/lib.rs::Diagram::render"],"evidence":[{"kind":"test","reference":"crates/fm-render-canvas/src/lib.rs::tests::render_with_mock_context","note":"Canvas backend exercises draw pipeline"}],"notes":[]},{"id":"layout/deterministic","category":"layout","title":"Deterministic layout output","status":"implemented","advertised_in":["README.md#design-philosophy","README.md#faq"],"code_paths":["crates/fm-layout/src/lib.rs::layout_diagram_traced","crates/fm-layout/src/lib.rs::crossing_refinement"],"evidence":[{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::traced_layout_is_deterministic","note":"Checks full traced layout equality across runs"}],"notes":[]},{"id":"parser/recovery","category":"parser","title":"Best-effort parse with warnings instead of hard failure","status":"partial","advertised_in":["README.md#tl-dr","README.md#design-philosophy"],"code_paths":["crates/fm-parser/src/lib.rs::parse","crates/fm-core/src/lib.rs::MermaidWarning"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::empty_input_returns_warning","note":"Current coverage proves warning-based fallback for empty input"}],"notes":["Recovery exists, but README claims are broader than current automated evidence"]},{"id":"runtime/guard-report","category":"runtime","title":"Guard and degradation report types exist in shared IR","status":"experimental","advertised_in":["AGENTS.md#key-design-decisions","README.md#technical-architecture"],"code_paths":["crates/fm-core/src/lib.rs::MermaidGuardReport","crates/fm-core/src/lib.rs::MermaidDegradationPlan"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::MermaidDiagramMeta","note":"Types are threaded into IR metadata but not yet fully activated"}],"notes":["Data model exists; cross-pipeline activation is still an open backlog item"]},{"id":"layout/auto","category":"layout","title":"Layout algorithm: auto","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["All types"]},{"id":"layout/sugiyama","category":"layout","title":"Layout algorithm: sugiyama","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["General graphs (flowchart, class, state, ER, C4, requirement, DOT)"]},{"id":"layout/force","category":"layout","title":"Layout algorithm: force","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["General (fallback for dense/cyclic)"]},{"id":"layout/tree","category":"layout","title":"Layout algorithm: tree","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Tree-like graphs"]},{"id":"layout/radial","category":"layout","title":"Layout algorithm: radial","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Mindmap"]},{"id":"layout/sequence","category":"layout","title":"Layout algorithm: sequence","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Sequence diagrams"]},{"id":"layout/timeline","category":"layout","title":"Layout algorithm: timeline","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Timeline"]},{"id":"layout/gantt","category":"layout","title":"Layout algorithm: gantt","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Gantt charts"]},{"id":"layout/xychart","category":"layout","title":"Layout algorithm: xychart","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["XY charts"]},{"id":"layout/sankey","category":"layout","title":"Layout algorithm: sankey","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Sankey diagrams"]},{"id":"layout/kanban","category":"layout","title":"Layout algorithm: kanban","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Journey, kanban"]},{"id":"layout/grid","category":"layout","title":"Layout algorithm: grid","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Block-beta"]},{"id":"layout/pie","category":"layout","title":"Layout algorithm: pie","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Pie charts"]},{"id":"layout/quadrant","category":"layout","title":"Layout algorithm: quadrant","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Quadrant charts"]},{"id":"layout/gitgraph","category":"layout","title":"Layout algorithm: gitgraph","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Git graphs"]},{"id":"layout/packet","category":"layout","title":"Layout algorithm: packet","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Packet-beta"]},{"id":"layout/architecture","category":"layout","title":"Layout algorithm: architecture","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Architecture-beta"]},{"id":"layout/treemap","category":"layout","title":"Layout algorithm: treemap","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Treemap"]},{"id":"layout/radar","category":"layout","title":"Layout algorithm: radar","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Radar-beta"]},{"id":"layout/ishikawa","category":"layout","title":"Layout algorithm: ishikawa","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Ishikawa"]},{"id":"layout/treeview","category":"layout","title":"Layout algorithm: treeview","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["TreeView-beta"]},{"id":"layout/venn","category":"layout","title":"Layout algorithm: venn","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Venn-beta"]},{"id":"layout/wardley","category":"layout","title":"Layout algorithm: wardley","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["Wardley-beta"]},{"id":"layout/eventmodeling","category":"layout","title":"Layout algorithm: eventmodeling","status":"implemented","advertised_in":["README.md#layout-algorithms","docs/planning/FEATURE_PARITY.md#layout-algorithms"],"code_paths":["crates/fm-layout/src/lib.rs::LayoutAlgorithm"],"evidence":[{"kind":"code_path","reference":"crates/fm-layout/src/lib.rs::preferred_layout_algorithm_with_config","note":"Dispatch site for the specialized algorithms"},{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::layout_algorithm_rows_cover_every_variant","note":"Every enum variant carries exactly one row"}],"notes":["EventModeling"]}]};

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

