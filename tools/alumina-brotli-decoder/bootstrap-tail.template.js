const ALUMINA_BOOTSTRAP_ASSETS = Object.freeze({
    decoder: Object.freeze({
        path: '/alumina-brotli-decoder_bg.wasm',
        wireBytes: @@DECODER_WIRE_BYTES@@,
        sourceBytes: @@DECODER_WIRE_BYTES@@,
        wireSha256: '@@DECODER_WIRE_SHA256@@',
        sourceSha256: '@@DECODER_WIRE_SHA256@@',
        representation: 'identity',
    }),
    module: Object.freeze({
        path: '/alumina-interface.js.br',
        wireBytes: @@MODULE_WIRE_BYTES@@,
        sourceBytes: @@MODULE_SOURCE_BYTES@@,
        wireSha256: '@@MODULE_WIRE_SHA256@@',
        sourceSha256: '@@MODULE_SOURCE_SHA256@@',
        representation: 'brotli-rfc7932-q11-w23',
    }),
    wasm: Object.freeze({
        path: '/alumina-interface_bg.wasm.br',
        wireBytes: @@WASM_WIRE_BYTES@@,
        sourceBytes: @@WASM_SOURCE_BYTES@@,
        wireSha256: '@@WASM_WIRE_SHA256@@',
        sourceSha256: '@@WASM_SOURCE_SHA256@@',
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
