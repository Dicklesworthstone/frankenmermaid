/* @ts-self-types="./frankenmermaid.d.ts" */

export class Diagram {
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
            this.__wbg_ptr = r0 >>> 0;
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
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_960c155d3d49e4c2: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return addHeapObject(ret);
        },
        __wbg_Number_32bf70a599af1d4b: function(arg0) {
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
        __wbg___wbindgen_boolean_get_6ea149f0a8dcc5ff: function(arg0) {
            const v = getObject(arg0);
            const ret = typeof(v) === 'boolean' ? v : undefined;
            return isLikeNone(ret) ? 0xFFFFFF : ret ? 1 : 0;
        },
        __wbg___wbindgen_in_a5d8b22e52b24dd1: function(arg0, arg1) {
            const ret = getObject(arg0) in getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_is_null_52ff4ec04186736f: function(arg0) {
            const ret = getObject(arg0) === null;
            return ret;
        },
        __wbg___wbindgen_is_object_63322ec0cd6ea4ef: function(arg0) {
            const val = getObject(arg0);
            const ret = typeof(val) === 'object' && val !== null;
            return ret;
        },
        __wbg___wbindgen_is_string_6df3bf7ef1164ed3: function(arg0) {
            const ret = typeof(getObject(arg0)) === 'string';
            return ret;
        },
        __wbg___wbindgen_is_undefined_29a43b4d42920abd: function(arg0) {
            const ret = getObject(arg0) === undefined;
            return ret;
        },
        __wbg___wbindgen_jsval_loose_eq_cac3565e89b4134c: function(arg0, arg1) {
            const ret = getObject(arg0) == getObject(arg1);
            return ret;
        },
        __wbg___wbindgen_number_get_c7f42aed0525c451: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'number' ? obj : undefined;
            getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
        },
        __wbg___wbindgen_string_get_7ed5322991caaec5: function(arg0, arg1) {
            const obj = getObject(arg1);
            const ret = typeof(obj) === 'string' ? obj : undefined;
            var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_export, wasm.__wbindgen_export2);
            var len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
            getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
        },
        __wbg___wbindgen_throw_6b64449b9b9ed33c: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_addEventListener_8176dab41b09531c: function() { return handleError(function (arg0, arg1, arg2, arg3) {
            getObject(arg0).addEventListener(getStringFromWasm0(arg1, arg2), getObject(arg3));
        }, arguments); },
        __wbg_arcTo_941456c2ac39464e: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            getObject(arg0).arcTo(arg1, arg2, arg3, arg4, arg5);
        }, arguments); },
        __wbg_arc_817de096f286078c: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5) {
            getObject(arg0).arc(arg1, arg2, arg3, arg4, arg5);
        }, arguments); },
        __wbg_beginPath_6d95cc267dd3e88f: function(arg0) {
            getObject(arg0).beginPath();
        },
        __wbg_bezierCurveTo_ee45420e339643c8: function(arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            getObject(arg0).bezierCurveTo(arg1, arg2, arg3, arg4, arg5, arg6);
        },
        __wbg_clearRect_5fb1d6b44e6b6738: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).clearRect(arg1, arg2, arg3, arg4);
        },
        __wbg_closePath_d9cf40637e9c89c2: function(arg0) {
            getObject(arg0).closePath();
        },
        __wbg_entries_e0b73aa8571ddb56: function(arg0) {
            const ret = Object.entries(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_fillRect_992c5a4646ea7a7f: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).fillRect(arg1, arg2, arg3, arg4);
        },
        __wbg_fillText_dabb33ea287042e2: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).fillText(getStringFromWasm0(arg1, arg2), arg3, arg4);
        }, arguments); },
        __wbg_fill_ec5da5f3916cf924: function(arg0) {
            getObject(arg0).fill();
        },
        __wbg_getContext_fc146f8ec021d074: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).getContext(getStringFromWasm0(arg1, arg2));
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        }, arguments); },
        __wbg_get_6011fa3a58f61074: function() { return handleError(function (arg0, arg1) {
            const ret = Reflect.get(getObject(arg0), getObject(arg1));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_get_8360291721e2339f: function(arg0, arg1) {
            const ret = getObject(arg0)[arg1 >>> 0];
            return addHeapObject(ret);
        },
        __wbg_get_with_ref_key_6412cf3094599694: function(arg0, arg1) {
            const ret = getObject(arg0)[getObject(arg1)];
            return addHeapObject(ret);
        },
        __wbg_height_528848d067cc2221: function(arg0) {
            const ret = getObject(arg0).height;
            return ret;
        },
        __wbg_instanceof_ArrayBuffer_7c8433c6ed14ffe3: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof ArrayBuffer;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_CanvasRenderingContext2d_24a3fe06e62b98d7: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof CanvasRenderingContext2D;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_instanceof_Uint8Array_152ba1f289edcf3f: function(arg0) {
            let result;
            try {
                result = getObject(arg0) instanceof Uint8Array;
            } catch (_) {
                result = false;
            }
            const ret = result;
            return ret;
        },
        __wbg_isSafeInteger_4fc213d1989d6d2a: function(arg0) {
            const ret = Number.isSafeInteger(getObject(arg0));
            return ret;
        },
        __wbg_length_3d4ecd04bd8d22f1: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_length_9f1775224cf1d815: function(arg0) {
            const ret = getObject(arg0).length;
            return ret;
        },
        __wbg_lineTo_c9f1e0dd4824ae31: function(arg0, arg1, arg2) {
            getObject(arg0).lineTo(arg1, arg2);
        },
        __wbg_measureText_9378d40a63a0dd0b: function() { return handleError(function (arg0, arg1, arg2) {
            const ret = getObject(arg0).measureText(getStringFromWasm0(arg1, arg2));
            return addHeapObject(ret);
        }, arguments); },
        __wbg_moveTo_d3deaceb55dc2d80: function(arg0, arg1, arg2) {
            getObject(arg0).moveTo(arg1, arg2);
        },
        __wbg_new_0c7403db6e782f19: function(arg0) {
            const ret = new Uint8Array(getObject(arg0));
            return addHeapObject(ret);
        },
        __wbg_new_34d45cc8e36aaead: function() {
            const ret = new Map();
            return addHeapObject(ret);
        },
        __wbg_new_682678e2f47e32bc: function() {
            const ret = new Array();
            return addHeapObject(ret);
        },
        __wbg_new_aa8d0fa9762c29bd: function() {
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
        __wbg_prototypesetcall_a6b02eb00b0f4ce2: function(arg0, arg1, arg2) {
            Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), getObject(arg2));
        },
        __wbg_push_471a5b068a5295f6: function(arg0, arg1) {
            const ret = getObject(arg0).push(getObject(arg1));
            return ret;
        },
        __wbg_rect_a7f5a58f447e85c2: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).rect(arg1, arg2, arg3, arg4);
        },
        __wbg_restore_f103803ad0dc390b: function(arg0) {
            getObject(arg0).restore();
        },
        __wbg_rotate_fb8a7a0e39ad85a6: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).rotate(arg1);
        }, arguments); },
        __wbg_save_5b07d6d1028c3e4d: function(arg0) {
            getObject(arg0).save();
        },
        __wbg_setLineDash_c273ecd8ca7d242d: function() { return handleError(function (arg0, arg1) {
            getObject(arg0).setLineDash(getObject(arg1));
        }, arguments); },
        __wbg_setTransform_e43c6ac3207fe112: function() { return handleError(function (arg0, arg1, arg2, arg3, arg4, arg5, arg6) {
            getObject(arg0).setTransform(arg1, arg2, arg3, arg4, arg5, arg6);
        }, arguments); },
        __wbg_set_3bf1de9fab0cd644: function(arg0, arg1, arg2) {
            getObject(arg0)[arg1 >>> 0] = takeObject(arg2);
        },
        __wbg_set_6be42768c690e380: function(arg0, arg1, arg2) {
            getObject(arg0)[takeObject(arg1)] = takeObject(arg2);
        },
        __wbg_set_fde2cec06c23692b: function(arg0, arg1, arg2) {
            const ret = getObject(arg0).set(getObject(arg1), getObject(arg2));
            return addHeapObject(ret);
        },
        __wbg_set_fillStyle_e51447e54357dc46: function(arg0, arg1, arg2) {
            getObject(arg0).fillStyle = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_font_295aa505e45244aa: function(arg0, arg1, arg2) {
            getObject(arg0).font = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_lineWidth_2fae105117e1a89f: function(arg0, arg1) {
            getObject(arg0).lineWidth = arg1;
        },
        __wbg_set_strokeStyle_0429d48dae657e53: function(arg0, arg1, arg2) {
            getObject(arg0).strokeStyle = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_textAlign_e1a83482b00339b8: function(arg0, arg1, arg2) {
            getObject(arg0).textAlign = getStringFromWasm0(arg1, arg2);
        },
        __wbg_set_textBaseline_8662e97190d0164a: function(arg0, arg1, arg2) {
            getObject(arg0).textBaseline = getStringFromWasm0(arg1, arg2);
        },
        __wbg_static_accessor_GLOBAL_8cfadc87a297ca02: function() {
            const ret = typeof global === 'undefined' ? null : global;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_GLOBAL_THIS_602256ae5c8f42cf: function() {
            const ret = typeof globalThis === 'undefined' ? null : globalThis;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_SELF_e445c1c7484aecc3: function() {
            const ret = typeof self === 'undefined' ? null : self;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_static_accessor_WINDOW_f20e8576ef1e0f17: function() {
            const ret = typeof window === 'undefined' ? null : window;
            return isLikeNone(ret) ? 0 : addHeapObject(ret);
        },
        __wbg_strokeRect_502699d92aeb85f1: function(arg0, arg1, arg2, arg3, arg4) {
            getObject(arg0).strokeRect(arg1, arg2, arg3, arg4);
        },
        __wbg_stroke_42f07013960cf81b: function(arg0) {
            getObject(arg0).stroke();
        },
        __wbg_translate_34989493d69eaecd: function() { return handleError(function (arg0, arg1, arg2) {
            getObject(arg0).translate(arg1, arg2);
        }, arguments); },
        __wbg_width_5adcb07d04d08bdf: function(arg0) {
            const ret = getObject(arg0).width;
            return ret;
        },
        __wbg_width_aae9d517bb5828f8: function(arg0) {
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
    : new FinalizationRegistry(ptr => wasm.__wbg_diagram_free(ptr >>> 0, 1));

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
    ptr = ptr >>> 0;
    return decodeText(ptr, len);
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

let wasmModule, wasm;
function __wbg_finalize_init(instance, module) {
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



const CAPABILITY_MATRIX = {"schema_version":"1.0.0","project":"frankenmermaid","status_counts":{"experimental":1,"implemented":33,"partial":2},"claims":[{"id":"diagram-type/flowchart","category":"diagram_type","title":"Support flowchart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/sequence","category":"diagram_type","title":"Support sequence diagrams","status":"partial","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as partial capability"]},{"id":"diagram-type/class","category":"diagram_type","title":"Support class diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/state","category":"diagram_type","title":"Support state diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/er","category":"diagram_type","title":"Support er diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Context","category":"diagram_type","title":"Support C4Context diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Container","category":"diagram_type","title":"Support C4Container diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Component","category":"diagram_type","title":"Support C4Component diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Dynamic","category":"diagram_type","title":"Support C4Dynamic diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/C4Deployment","category":"diagram_type","title":"Support C4Deployment diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/architecture-beta","category":"diagram_type","title":"Support architecture-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/block-beta","category":"diagram_type","title":"Support block-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/gantt","category":"diagram_type","title":"Support gantt diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/timeline","category":"diagram_type","title":"Support timeline diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/journey","category":"diagram_type","title":"Support journey diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/gitGraph","category":"diagram_type","title":"Support gitGraph diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/sankey","category":"diagram_type","title":"Support sankey diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/mindmap","category":"diagram_type","title":"Support mindmap diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/pie","category":"diagram_type","title":"Support pie diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/quadrantChart","category":"diagram_type","title":"Support quadrantChart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/xyChart","category":"diagram_type","title":"Support xyChart diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/requirementDiagram","category":"diagram_type","title":"Support requirementDiagram diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/packet-beta","category":"diagram_type","title":"Support packet-beta diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"diagram-type/kanban","category":"diagram_type","title":"Support kanban diagrams","status":"implemented","advertised_in":["README.md#supported-diagram-types"],"code_paths":["crates/fm-core/src/lib.rs::DiagramType","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::DiagramType::support_level","note":"Source-of-truth support taxonomy"},{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::diagram_type_support_contract_matches_surface_expectations","note":"Verifies advertised support level mapping"}],"notes":["README advertises this family; current code marks it as full capability"]},{"id":"surface/cli-detect","category":"surface","title":"CLI detect command","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Detect","crates/fm-parser/src/lib.rs::detect_type_with_confidence"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::detects_flowchart_keyword","note":"Smoke coverage for type detection"},{"kind":"code_path","reference":"crates/fm-cli/src/main.rs::cmd_detect","note":null}],"notes":[]},{"id":"surface/cli-parse","category":"surface","title":"CLI parse command with IR JSON evidence","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Parse","crates/fm-parser/src/lib.rs::parse_evidence_json"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::parse_flowchart_extracts_nodes_edges_and_direction","note":"Validates parse output contains structural IR"}],"notes":[]},{"id":"surface/cli-render-svg","category":"surface","title":"CLI SVG rendering","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Render","crates/fm-render-svg/src/lib.rs::render_svg_with_layout"],"evidence":[{"kind":"test","reference":"crates/fm-render-svg/src/lib.rs::tests::prop_svg_render_is_total_and_counts_match","note":"SVG renderer smoke coverage"}],"notes":[]},{"id":"surface/cli-render-term","category":"surface","title":"CLI terminal rendering","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Render","crates/fm-render-term/src/lib.rs::render_term_with_config"],"evidence":[{"kind":"test","reference":"crates/fm-render-term/src/lib.rs::tests::render_term_produces_output","note":"Terminal renderer smoke coverage"}],"notes":[]},{"id":"surface/cli-validate","category":"surface","title":"CLI validate command with structured diagnostics","status":"implemented","advertised_in":["README.md#quick-example","README.md#command-reference"],"code_paths":["crates/fm-cli/src/main.rs::Command::Validate","crates/fm-core/src/lib.rs::StructuredDiagnostic"],"evidence":[{"kind":"test","reference":"crates/fm-cli/src/main.rs::tests::collect_validation_diagnostics_includes_parse_warnings","note":"Validate path emits structured diagnostics"}],"notes":[]},{"id":"surface/cli-capabilities","category":"surface","title":"CLI capability matrix command","status":"implemented","advertised_in":["README.md#command-reference","README.md#runtime-capability-metadata"],"code_paths":["crates/fm-cli/src/main.rs::Command::Capabilities","crates/fm-cli/src/main.rs::cmd_capabilities","crates/fm-core/src/lib.rs::capability_matrix"],"evidence":[{"kind":"test","reference":"crates/fm-core/src/lib.rs::tests::capability_matrix_json_matches_checked_in_artifact","note":"CLI command serializes the checked-in capability artifact"},{"kind":"code_path","reference":"crates/fm-cli/src/main.rs::cmd_capabilities","note":null}],"notes":[]},{"id":"surface/wasm-svg","category":"surface","title":"WASM API renders SVG","status":"implemented","advertised_in":["README.md#javascript--wasm-api","README.md#technical-architecture"],"code_paths":["crates/fm-wasm/src/lib.rs::render","crates/fm-wasm/src/lib.rs::render_svg_js","crates/fm-wasm/src/lib.rs::Diagram::render"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::render_returns_svg_and_type","note":"WASM facade smoke coverage"}],"notes":[]},{"id":"surface/wasm-capabilities","category":"surface","title":"WASM API exposes capability matrix metadata","status":"implemented","advertised_in":["README.md#javascript--wasm-api","README.md#runtime-capability-metadata"],"code_paths":["crates/fm-wasm/src/lib.rs::capability_matrix_js","crates/fm-core/src/lib.rs::capability_matrix"],"evidence":[{"kind":"test","reference":"crates/fm-wasm/src/lib.rs::tests::capability_matrix_js_returns_matrix_payload","note":"WASM surface returns the shared capability matrix"}],"notes":[]},{"id":"surface/canvas","category":"surface","title":"Canvas rendering backend","status":"implemented","advertised_in":["README.md#why-use-frankenmermaid","README.md#technical-architecture"],"code_paths":["crates/fm-render-canvas/src/lib.rs::render_to_canvas","crates/fm-wasm/src/lib.rs::Diagram::render"],"evidence":[{"kind":"test","reference":"crates/fm-render-canvas/src/lib.rs::tests::render_with_mock_context","note":"Canvas backend exercises draw pipeline"}],"notes":[]},{"id":"layout/deterministic","category":"layout","title":"Deterministic layout output","status":"implemented","advertised_in":["README.md#design-philosophy","README.md#faq"],"code_paths":["crates/fm-layout/src/lib.rs::layout_diagram_traced","crates/fm-layout/src/lib.rs::crossing_refinement"],"evidence":[{"kind":"test","reference":"crates/fm-layout/src/lib.rs::tests::traced_layout_is_deterministic","note":"Checks full traced layout equality across runs"}],"notes":[]},{"id":"parser/recovery","category":"parser","title":"Best-effort parse with warnings instead of hard failure","status":"partial","advertised_in":["README.md#tl-dr","README.md#design-philosophy"],"code_paths":["crates/fm-parser/src/lib.rs::parse","crates/fm-core/src/lib.rs::MermaidWarning"],"evidence":[{"kind":"test","reference":"crates/fm-parser/src/lib.rs::tests::empty_input_returns_warning","note":"Current coverage proves warning-based fallback for empty input"}],"notes":["Recovery exists, but README claims are broader than current automated evidence"]},{"id":"runtime/guard-report","category":"runtime","title":"Guard and degradation report types exist in shared IR","status":"experimental","advertised_in":["AGENTS.md#key-design-decisions","README.md#technical-architecture"],"code_paths":["crates/fm-core/src/lib.rs::MermaidGuardReport","crates/fm-core/src/lib.rs::MermaidDegradationPlan"],"evidence":[{"kind":"code_path","reference":"crates/fm-core/src/lib.rs::MermaidDiagramMeta","note":"Types are threaded into IR metadata but not yet fully activated"}],"notes":["Data model exists; cross-pipeline activation is still an open backlog item"]}]};

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

