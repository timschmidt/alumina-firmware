import fs from "node:fs";
import path from "node:path";

const INTERFACE_COMMIT = "0a111d2e5243db0e356e9befa1ac28c77785753a";
const BUNDLE_SHA256 = "8588ad72e8a4b516d1299060f9ad470d9d1db397bf6a4c96a3fb99b18cef9e49";
const VIEWPORT = { width: 1440, height: 1000 };
const EXPECTED = new Map([
  ["/alumina-bootstrap.js", {
    encoded: 16_235,
    decoded: 16_235,
    sourceBytes: 16_235,
    source: "2d3d556f85918dd4d4a93490b40a6d957c55bce5449274220a898171307fef18",
    wire: "2d3d556f85918dd4d4a93490b40a6d957c55bce5449274220a898171307fef18",
    representation: "identity",
  }],
  ["/alumina-brotli-decoder_bg.wasm", {
    encoded: 196_134,
    decoded: 196_134,
    sourceBytes: 196_134,
    source: "8a656d9cb18dfce82838f06f6153005a6cfa0c22c3cf575ef5cf30932fb46cab",
    wire: "8a656d9cb18dfce82838f06f6153005a6cfa0c22c3cf575ef5cf30932fb46cab",
    representation: "identity",
  }],
  ["/alumina-interface.js.br", {
    encoded: 10_909,
    decoded: 10_909,
    sourceBytes: 91_816,
    source: "4f0d819019f81df95b324a7f9a521effeb1c7d95ca56172be6b3af7e6c7f192e",
    wire: "5d6515aa29962f29b17b25496c0911c3ba9b4ca619fe9fdbaef60c35f55b10c3",
    representation: "brotli-rfc7932-q11-w23",
  }],
  ["/alumina-interface_bg.wasm.br", {
    encoded: 2_820_967,
    decoded: 2_820_967,
    sourceBytes: 8_421_767,
    source: "b9670ad660146faa860bd1c7ee086e32da9b174f333fec3c161e6f11dd4ff8d9",
    wire: "9c6310d1993a7facbfd97d84be6185b20cb72af2d6997433e3bee40b1e20a415",
    representation: "brotli-rfc7932-q11-w23",
  }],
  ["/alumina-worker.js", {
    encoded: 678,
    decoded: 678,
    sourceBytes: 678,
    source: "2d23324fa4fe34169abcc34046dcbcadbd13a02b99958f465ab3878a709c9a75",
    wire: "2d23324fa4fe34169abcc34046dcbcadbd13a02b99958f465ab3878a709c9a75",
    representation: "identity",
  }],
]);

function positiveInteger(name, fallback) {
  const raw = process.env[name] ?? String(fallback);
  const value = Number(raw);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return value;
}

function withoutTrailingSlash(value) {
  return value.endsWith("/") ? value.slice(0, -1) : value;
}

function lowerCaseHeaders(headers) {
  return Object.fromEntries(
    Object.entries(headers ?? {}).map(([name, value]) => [name.toLowerCase(), String(value)]),
  );
}

const debugOrigin = withoutTrailingSlash(
  process.env.ALUMINA_CDP_ORIGIN ?? "http://127.0.0.1:9231",
);
const deviceOrigin = withoutTrailingSlash(
  process.env.ALUMINA_DEVICE_ORIGIN ?? "http://192.168.4.1",
);
const outputPrefix = process.env.ALUMINA_BROWSER_OUTPUT_PREFIX
  ?? "/tmp/alumina-embedded-interface-browser";
const readyDeadlineMs = positiveInteger("ALUMINA_BROWSER_DEADLINE_MS", 150_000);
const loadDeadlineMs = positiveInteger("ALUMINA_BROWSER_LOAD_TIMEOUT_MS", 20_000);

const pages = await fetch(`${debugOrigin}/json`).then((response) => {
  if (!response.ok) throw new Error(`CDP discovery returned HTTP ${response.status}`);
  return response.json();
});
const page = pages.find((candidate) => candidate.type === "page");
if (!page) throw new Error("no Chromium page target is available");

const socket = new WebSocket(page.webSocketDebuggerUrl);
await new Promise((resolve, reject) => {
  socket.onopen = resolve;
  socket.onerror = reject;
});

let nextId = 1;
const pending = new Map();
const exceptions = [];
const logEntries = [];
const loadingFailures = [];
const responses = [];
let resolveLoaded;
const loaded = new Promise((resolve) => { resolveLoaded = resolve; });

socket.onmessage = (event) => {
  const message = JSON.parse(event.data);
  if (message.id) {
    const completion = pending.get(message.id);
    if (!completion) return;
    pending.delete(message.id);
    if (message.error) completion.reject(new Error(JSON.stringify(message.error)));
    else completion.resolve(message.result);
    return;
  }
  if (message.method === "Page.loadEventFired") resolveLoaded();
  if (message.method === "Runtime.exceptionThrown") exceptions.push(message.params);
  if (message.method === "Log.entryAdded") logEntries.push(message.params.entry);
  if (message.method === "Network.loadingFailed") loadingFailures.push(message.params);
  if (message.method === "Network.responseReceived") {
    const response = message.params.response;
    if (response.url === deviceOrigin || response.url.startsWith(`${deviceOrigin}/`)) {
      responses.push({
        url: response.url,
        status: response.status,
        mimeType: response.mimeType,
        encodedDataLength: response.encodedDataLength,
        fromDiskCache: response.fromDiskCache,
        headers: response.headers,
      });
    }
  }
};

const send = (method, params = {}) => new Promise((resolve, reject) => {
  const id = nextId++;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params }));
});

async function evaluatePage() {
  const evaluated = await send("Runtime.evaluate", {
    expression: `(() => ({
      href: location.href,
      readyState: document.readyState,
      title: document.title,
      canvas: [...document.querySelectorAll("canvas")].map((item) => ({
        width: item.width,
        height: item.height,
        clientWidth: item.clientWidth,
        clientHeight: item.clientHeight,
      })),
      resources: performance.getEntriesByType("resource").map((entry) => ({
        name: entry.name,
        transferSize: entry.transferSize,
        encodedBodySize: entry.encodedBodySize,
        decodedBodySize: entry.decodedBodySize,
        duration: entry.duration,
      })),
      localStorageKeys: Object.keys(localStorage),
      bootstrapEvidence: globalThis.aluminaBootstrapEvidence ?? null,
      bootstrapError: globalThis.aluminaBootstrapError ?? null,
      hasWasmBindings: typeof globalThis.wasmBindings === "object",
    }))()`,
    returnByValue: true,
  });
  return evaluated.result.value;
}

function exactResources(pageState) {
  return [...EXPECTED].every(([route, expected]) => {
    const resource = pageState.resources.find((entry) => entry.name === `${deviceOrigin}${route}`);
    return resource?.encodedBodySize === expected.encoded
      && resource.decodedBodySize === expected.decoded;
  });
}

function exactCanvas(pageState) {
  return pageState.canvas.some((item) => item.width === VIEWPORT.width
    && item.height === VIEWPORT.height
    && item.clientWidth === VIEWPORT.width
    && item.clientHeight === VIEWPORT.height);
}

function exactBootstrapEvidence(pageState) {
  const evidence = pageState.bootstrapEvidence;
  if (evidence?.schemaVersion !== 1
      || evidence.format !== "alumina-exact-brotli-bootstrap-v1"
      || evidence.interfaceCommit !== INTERFACE_COMMIT
      || !(evidence.elapsedMilliseconds > 0)) {
    return false;
  }
  const mappings = [
    ["decoder", "/alumina-brotli-decoder_bg.wasm"],
    ["module", "/alumina-interface.js.br"],
    ["wasm", "/alumina-interface_bg.wasm.br"],
  ];
  return mappings.every(([name, route]) => {
    const actual = evidence.assets?.[name];
    const expected = EXPECTED.get(route);
    return actual?.path === route
      && actual.wireBytes === expected.encoded
      && actual.sourceBytes === expected.sourceBytes
      && actual.wireSha256 === expected.wire
      && actual.sourceSha256 === expected.source
      && actual.representation === expected.representation;
  });
}

function responseFor(route) {
  const url = route === "/" ? `${deviceOrigin}/` : `${deviceOrigin}${route}`;
  return responses.find((response) => response.url === url);
}

function exactResponse(route, expected) {
  const response = responseFor(route);
  if (!response || response.status !== 200 || response.fromDiskCache) return false;
  const headers = lowerCaseHeaders(response.headers);
  return headers.connection?.toLowerCase() === "close"
    && headers["x-alumina-interface-commit"] === INTERFACE_COMMIT
    && headers["x-alumina-bundle-sha256"] === BUNDLE_SHA256
    && headers["x-alumina-source-sha256"] === expected.source
    && headers["x-alumina-wire-sha256"] === expected.wire
    && headers["x-alumina-stored-representation"] === expected.representation
    && headers["content-encoding"] === undefined
    && headers["content-length"] === String(expected.encoded);
}

await send("Page.enable");
await send("Runtime.enable");
await send("Log.enable");
await send("Network.enable");
await send("Network.setCacheDisabled", { cacheDisabled: true });
await send("Emulation.setDeviceMetricsOverride", {
  ...VIEWPORT,
  deviceScaleFactor: 1,
  mobile: false,
});
await send("Page.navigate", { url: `${deviceOrigin}/` });
await Promise.race([
  loaded,
  new Promise((_, reject) => setTimeout(
    () => reject(new Error("page load timed out")),
    loadDeadlineMs,
  )),
]);

let pageState;
const deadline = Date.now() + readyDeadlineMs;
while (Date.now() < deadline) {
  pageState = await evaluatePage();
  if (pageState.href === `${deviceOrigin}/`
      && pageState.title === "Alumina"
      && exactResources(pageState)
      && exactBootstrapEvidence(pageState)
      && exactCanvas(pageState)) {
    break;
  }
  await new Promise((resolve) => setTimeout(resolve, 1_000));
}

await new Promise((resolve) => setTimeout(resolve, 2_000));
pageState = await evaluatePage();
const screenshot = await send("Page.captureScreenshot", {
  format: "png",
  captureBeyondViewport: false,
});
const screenshotBytes = Buffer.from(screenshot.data, "base64");

const rootExpected = {
  encoded: 1_077,
  source: "3ccd883461eb1c786c2418512f8cb6094a141a8459a2dd597fad00cccf5a3a2b",
  wire: "3ccd883461eb1c786c2418512f8cb6094a141a8459a2dd597fad00cccf5a3a2b",
  representation: "identity",
};
const checks = {
  canonicalLocation: pageState.href === `${deviceOrigin}/`,
  applicationTitle: pageState.title === "Alumina",
  exactResources: exactResources(pageState),
  exactBootstrapEvidence: exactBootstrapEvidence(pageState),
  wasmBindingsInstalled: pageState.hasWasmBindings,
  noBootstrapError: pageState.bootstrapError === null,
  exactCanvas: exactCanvas(pageState),
  nontrivialScreenshot: screenshotBytes.length >= 50_000,
  rootHeaders: exactResponse("/", rootExpected),
  bootstrapHeaders: exactResponse(
    "/alumina-bootstrap.js",
    EXPECTED.get("/alumina-bootstrap.js"),
  ),
  decoderHeaders: exactResponse(
    "/alumina-brotli-decoder_bg.wasm",
    EXPECTED.get("/alumina-brotli-decoder_bg.wasm"),
  ),
  moduleHeaders: exactResponse(
    "/alumina-interface.js.br",
    EXPECTED.get("/alumina-interface.js.br"),
  ),
  wasmHeaders: exactResponse(
    "/alumina-interface_bg.wasm.br",
    EXPECTED.get("/alumina-interface_bg.wasm.br"),
  ),
  noNetworkFavicon: !pageState.resources.some(
    (resource) => resource.name === `${deviceOrigin}/favicon.ico`,
  ) && responseFor("/favicon.ico") === undefined,
  noLoadingFailures: loadingFailures.length === 0,
  noExceptions: exceptions.length === 0,
};
const result = {
  qualified: Object.values(checks).every(Boolean),
  configuration: {
    debugOrigin,
    deviceOrigin,
    readyDeadlineMs,
    loadDeadlineMs,
    interfaceCommit: INTERFACE_COMMIT,
    bundleSha256: BUNDLE_SHA256,
  },
  checks,
  screenshotBytes: screenshotBytes.length,
  page: pageState,
  responses,
  loadingFailures,
  exceptions,
  logEntries,
};

fs.mkdirSync(path.dirname(outputPrefix), { recursive: true });
fs.writeFileSync(`${outputPrefix}.png`, screenshotBytes);
fs.writeFileSync(`${outputPrefix}.json`, `${JSON.stringify(result, null, 2)}\n`);
console.log(JSON.stringify(result));
socket.close();
if (!result.qualified) {
  throw new Error("embedded-interface browser qualification failed one or more strict checks");
}
