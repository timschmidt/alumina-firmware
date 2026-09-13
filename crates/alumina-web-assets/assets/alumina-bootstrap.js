/**
 * Decode exactly one bounded RFC 7932 stream.
 *
 * The caller supplies the manifest length. Output that ends early or tries to
 * exceed that exact length is rejected before any bytes are returned to JS.
 * @param {Uint8Array} compressed
 * @param {number} expected_decompressed_bytes
 * @returns {Uint8Array}
 */
export function decompressExact(compressed, expected_decompressed_bytes) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16);
        const ptr0 = passArray8ToWasm0(compressed, wasm.__wbindgen_export);
        const len0 = WASM_VECTOR_LEN;
        wasm.decompressExact(retptr, ptr0, len0, expected_decompressed_bytes);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        if (r3) {
            throw takeObject(r2);
        }
        var v2 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export2(r0, r1 * 1, 1);
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbindgen_cast_0000000000000001: function(arg0, arg1) {
            // Cast intrinsic for `Ref(String) -> Externref`.
            const ret = getStringFromWasm0(arg0, arg1);
            return addHeapObject(ret);
        },
    };
    return {
        __proto__: null,
        "./alumina-brotli-decoder_bg.js": import0,
    };
}

function addHeapObject(obj) {
    if (heap_next === heap.length) heap.push(heap.length + 1);
    const idx = heap_next;
    heap_next = heap[idx];

    heap[idx] = obj;
    return idx;
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

let heap = new Array(1024).fill(undefined);
heap.push(undefined, null, true, false);

let heap_next = heap.length;

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

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
        module_or_path = new URL('alumina-brotli-decoder_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };

const ALUMINA_BOOTSTRAP_ASSETS = Object.freeze({
    decoder: Object.freeze({
        path: '/alumina-brotli-decoder_bg.wasm',
        wireBytes: 196134,
        sourceBytes: 196134,
        wireSha256: '8a656d9cb18dfce82838f06f6153005a6cfa0c22c3cf575ef5cf30932fb46cab',
        sourceSha256: '8a656d9cb18dfce82838f06f6153005a6cfa0c22c3cf575ef5cf30932fb46cab',
        representation: 'identity',
    }),
    module: Object.freeze({
        path: '/alumina-interface.js.br',
        wireBytes: 10909,
        sourceBytes: 91816,
        wireSha256: '5d6515aa29962f29b17b25496c0911c3ba9b4ca619fe9fdbaef60c35f55b10c3',
        sourceSha256: '4f0d819019f81df95b324a7f9a521effeb1c7d95ca56172be6b3af7e6c7f192e',
        representation: 'brotli-rfc7932-q11-w23',
    }),
    wasm: Object.freeze({
        path: '/alumina-interface_bg.wasm.br',
        wireBytes: 2820967,
        sourceBytes: 8421767,
        wireSha256: '9c6310d1993a7facbfd97d84be6185b20cb72af2d6997433e3bee40b1e20a415',
        sourceSha256: 'b9670ad660146faa860bd1c7ee086e32da9b174f333fec3c161e6f11dd4ff8d9',
        representation: 'brotli-rfc7932-q11-w23',
    }),
});

const SHA256_INITIAL_STATE = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a,
    0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
]);

const SHA256_ROUND_CONSTANTS = new Uint32Array([
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5,
    0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3,
    0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc,
    0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
    0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13,
    0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3,
    0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5,
    0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208,
    0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

function rotateRight(value, count) {
    return (value >>> count) | (value << (32 - count));
}

/** SHA-256 for insecure LAN origins where WebCrypto is intentionally absent. */
export function sha256Hex(bytes) {
    if (!(bytes instanceof Uint8Array)) {
        throw new TypeError('SHA-256 input must be a Uint8Array');
    }
    const paddedBytes = Math.ceil((bytes.length + 9) / 64) * 64;
    const message = new Uint8Array(paddedBytes);
    message.set(bytes);
    message[bytes.length] = 0x80;
    const view = new DataView(message.buffer);
    view.setUint32(paddedBytes - 8, Math.floor(bytes.length / 0x20000000), false);
    view.setUint32(paddedBytes - 4, (bytes.length << 3) >>> 0, false);

    const state = new Uint32Array(SHA256_INITIAL_STATE);
    const words = new Uint32Array(64);
    for (let offset = 0; offset < paddedBytes; offset += 64) {
        for (let index = 0; index < 16; index += 1) {
            words[index] = view.getUint32(offset + index * 4, false);
        }
        for (let index = 16; index < 64; index += 1) {
            const prior15 = words[index - 15];
            const prior2 = words[index - 2];
            const sigma0 = rotateRight(prior15, 7)
                ^ rotateRight(prior15, 18)
                ^ (prior15 >>> 3);
            const sigma1 = rotateRight(prior2, 17)
                ^ rotateRight(prior2, 19)
                ^ (prior2 >>> 10);
            words[index] = (words[index - 16] + sigma0
                + words[index - 7] + sigma1) >>> 0;
        }

        let a = state[0];
        let b = state[1];
        let c = state[2];
        let d = state[3];
        let e = state[4];
        let f = state[5];
        let g = state[6];
        let h = state[7];
        for (let index = 0; index < 64; index += 1) {
            const sum1 = rotateRight(e, 6) ^ rotateRight(e, 11) ^ rotateRight(e, 25);
            const choice = (e & f) ^ (~e & g);
            const temporary1 = (h + sum1 + choice
                + SHA256_ROUND_CONSTANTS[index] + words[index]) >>> 0;
            const sum0 = rotateRight(a, 2) ^ rotateRight(a, 13) ^ rotateRight(a, 22);
            const majority = (a & b) ^ (a & c) ^ (b & c);
            const temporary2 = (sum0 + majority) >>> 0;
            h = g;
            g = f;
            f = e;
            e = (d + temporary1) >>> 0;
            d = c;
            c = b;
            b = a;
            a = (temporary1 + temporary2) >>> 0;
        }
        state[0] = (state[0] + a) >>> 0;
        state[1] = (state[1] + b) >>> 0;
        state[2] = (state[2] + c) >>> 0;
        state[3] = (state[3] + d) >>> 0;
        state[4] = (state[4] + e) >>> 0;
        state[5] = (state[5] + f) >>> 0;
        state[6] = (state[6] + g) >>> 0;
        state[7] = (state[7] + h) >>> 0;
    }
    return [...state]
        .map((word) => word.toString(16).padStart(8, '0'))
        .join('');
}

async function fetchExact(specification) {
    const response = await fetch(specification.path, {
        cache: 'no-store',
        credentials: 'same-origin',
    });
    if (!response.ok) {
        throw new Error(`${specification.path} returned HTTP ${response.status}`);
    }
    const expectedHeaders = {
        'content-length': String(specification.wireBytes),
        'x-alumina-source-sha256': specification.sourceSha256,
        'x-alumina-wire-sha256': specification.wireSha256,
        'x-alumina-stored-representation': specification.representation,
    };
    for (const [name, expected] of Object.entries(expectedHeaders)) {
        if (response.headers.get(name) !== expected) {
            throw new Error(`${specification.path} has an invalid ${name} header`);
        }
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length !== specification.wireBytes) {
        throw new Error(`${specification.path} has the wrong wire length`);
    }
    if (sha256Hex(bytes) !== specification.wireSha256) {
        throw new Error(`${specification.path} failed wire SHA-256 verification`);
    }
    return bytes;
}

function decodeExact(compressed, specification) {
    const decoded = decompressExact(compressed, specification.sourceBytes);
    if (decoded.length !== specification.sourceBytes) {
        throw new Error(`${specification.path} has the wrong decoded length`);
    }
    if (sha256Hex(decoded) !== specification.sourceSha256) {
        throw new Error(`${specification.path} failed source SHA-256 verification`);
    }
    return decoded;
}

let interfacePromise;

/** Load and initialize the exact Alumina UI in either a window or module worker. */
export function loadAluminaInterface() {
    interfacePromise ??= (async () => {
        const startedAt = performance.now();
        // Fetch the decoder first, then use both bounded TinyBee HTTP
        // admissions for the independently verified Brotli payloads.
        const decoderBytes = await fetchExact(ALUMINA_BOOTSTRAP_ASSETS.decoder);
        const [moduleCompressed, wasmCompressed] = await Promise.all([
            fetchExact(ALUMINA_BOOTSTRAP_ASSETS.module),
            fetchExact(ALUMINA_BOOTSTRAP_ASSETS.wasm),
        ]);
        await __wbg_init({ module_or_path: decoderBytes });
        const moduleBytes = decodeExact(moduleCompressed, ALUMINA_BOOTSTRAP_ASSETS.module);
        const wasmBytes = decodeExact(wasmCompressed, ALUMINA_BOOTSTRAP_ASSETS.wasm);
        const objectUrl = URL.createObjectURL(
            new Blob([moduleBytes], { type: 'text/javascript' }),
        );
        try {
            const bindings = await import(objectUrl);
            const interfaceWasm = await bindings.default({ module_or_path: wasmBytes });
            const evidence = Object.freeze({
                schemaVersion: 1,
                format: 'alumina-exact-brotli-bootstrap-v1',
                interfaceCommit: '0a111d2e5243db0e356e9befa1ac28c77785753a',
                assets: ALUMINA_BOOTSTRAP_ASSETS,
                elapsedMilliseconds: performance.now() - startedAt,
            });
            globalThis.aluminaBootstrapEvidence = evidence;
            return Object.freeze({ bindings, wasm: interfaceWasm, evidence });
        } finally {
            URL.revokeObjectURL(objectUrl);
        }
    })();
    return interfacePromise;
}

function reportWindowBootstrapFailure(reason) {
    const detail = reason instanceof Error ? reason.message : String(reason);
    console.error('Alumina interface bootstrap failed', reason);
    globalThis.aluminaBootstrapError = detail;
    const output = document.createElement('pre');
    output.id = 'alumina_bootstrap_error';
    output.textContent = `Alumina interface bootstrap failed: ${detail}`;
    document.body.replaceChildren(output);
}

if (typeof document === 'object') {
    loadAluminaInterface()
        .then(({ bindings, wasm: interfaceWasm }) => {
            globalThis.wasmBindings = bindings;
            dispatchEvent(new CustomEvent('TrunkApplicationStarted', {
                detail: { wasm: interfaceWasm },
            }));
        })
        .catch(reportWindowBootstrapFailure);
}
