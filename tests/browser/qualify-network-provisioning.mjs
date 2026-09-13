import fs from "node:fs";
import path from "node:path";
import crypto from "node:crypto";

const VIEWPORT = { width: 1440, height: 1000 };
const NETWORK_STATUS = 0x0801;
const AP_EXPECTED = 1 << 0;
const STATION_CONFIGURED = 1 << 1;
const STATION_ASSOCIATED = 1 << 2;
const STATION_IPV4_READY = 1 << 3;
const CREDENTIALS_DURABLE = 1 << 4;

function withoutTrailingSlash(value) {
  return value.endsWith("/") ? value.slice(0, -1) : value;
}

function positiveInteger(name, fallback) {
  const value = Number(process.env[name] ?? fallback);
  if (!Number.isSafeInteger(value) || value <= 0) {
    throw new Error(`${name} must be a positive integer`);
  }
  return value;
}

const debugOrigin = withoutTrailingSlash(
  process.env.ALUMINA_CDP_ORIGIN ?? "http://127.0.0.1:9231",
);
const deviceOrigin = withoutTrailingSlash(
  process.env.ALUMINA_DEVICE_ORIGIN ?? "http://127.0.0.1:8098",
);
const outputPrefix = process.env.ALUMINA_BROWSER_OUTPUT_PREFIX
  ?? "/tmp/alumina-network-provisioning-browser";
const deadlineMs = positiveInteger("ALUMINA_BROWSER_DEADLINE_MS", 60_000);
const deviceSecret = process.env.ALUMINA_DEVICE_SECRET ?? "alumina-development";
const stationSecret = process.env.ALUMINA_STATION_SECRET ?? "alumina-lab-secret";

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
let resolveLoaded;
let loaded = new Promise((resolve) => { resolveLoaded = resolve; });

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
};

const send = (method, params = {}) => new Promise((resolve, reject) => {
  const id = nextId++;
  pending.set(id, { resolve, reject });
  socket.send(JSON.stringify({ id, method, params }));
});

function readU16(bytes, offset) {
  return bytes.readUInt16LE(offset);
}

function readU32(bytes, offset) {
  return bytes.readUInt32LE(offset);
}

function readU64(bytes, offset) {
  return bytes.readBigUInt64LE(offset).toString();
}

function parseNativeResponse(bytes) {
  if (bytes.length < 72 || bytes.subarray(0, 4).toString("ascii") !== "ALUM") {
    throw new Error("control response is not a complete native Alumina frame");
  }
  const payloadBytes = readU32(bytes, 8);
  const bodyBytes = readU32(bytes, 68);
  if (bytes[6] !== 8
      || bytes[7] !== 0
      || bytes[58] !== 1
      || bytes[59] !== 0
      || readU16(bytes, 62) !== 0
      || payloadBytes !== bytes.length - 56
      || bodyBytes !== bytes.length - 72) {
    throw new Error("network response violates canonical outer framing");
  }
  return {
    operation: readU16(bytes, 56),
    status: readU16(bytes, 60),
    correlation: readU32(bytes, 64),
    body: bytes.subarray(72),
  };
}

async function waitFor(predicate, description, timeoutMs = deadlineMs) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const value = await predicate();
    if (value) return value;
    await new Promise((resolve) => setTimeout(resolve, 100));
  }
  throw new Error(`timed out waiting for ${description}`);
}

async function click(x, y) {
  for (const type of ["mousePressed", "mouseReleased"]) {
    await send("Input.dispatchMouseEvent", {
      type,
      x,
      y,
      button: "left",
      clickCount: 1,
    });
  }
  await new Promise((resolve) => setTimeout(resolve, 120));
}

async function replaceText(x, y, value) {
  await click(x, y);
  await send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "Control",
    code: "ControlLeft",
    windowsVirtualKeyCode: 17,
  });
  await send("Input.dispatchKeyEvent", {
    type: "rawKeyDown",
    key: "a",
    code: "KeyA",
    windowsVirtualKeyCode: 65,
    modifiers: 2,
  });
  await send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "a",
    code: "KeyA",
    windowsVirtualKeyCode: 65,
    modifiers: 2,
  });
  await send("Input.dispatchKeyEvent", {
    type: "keyUp",
    key: "Control",
    code: "ControlLeft",
    windowsVirtualKeyCode: 17,
  });
  await send("Input.insertText", { text: value });
  await new Promise((resolve) => setTimeout(resolve, 120));
}

function parseStatus(body) {
  if (body.length !== 80 || body.subarray(0, 8).toString("ascii") !== "ALMNST01") {
    throw new Error("network status body is not canonical ALMNST01");
  }
  const ssidBytes = body[31];
  return {
    generation: readU32(body, 8),
    scanGeneration: readU32(body, 12),
    transaction: readU64(body, 16),
    phase: body[24],
    stationLink: body[25],
    authentication: body[26],
    flags: body[27],
    channel: body[28],
    signalDbm: body.readInt8(29),
    ipv4Prefix: body[30],
    ipv4Address: [...body.subarray(32, 36)],
    ipv4Gateway: [...body.subarray(36, 40)],
    bssid: body.subarray(40, 46).toString("hex"),
    lastFailure: readU16(body, 46),
    ssid: body.subarray(48, 48 + ssidBytes).toString("utf8"),
  };
}

function encodeNetworkStatusRequest() {
  const bytes = Buffer.alloc(72);
  bytes.write("ALUM", 0, "ascii");
  bytes.writeUInt16LE(1, 4);
  bytes[6] = 8;
  bytes.writeUInt32LE(16, 8);
  bytes.writeUInt32LE(1, 12);
  bytes.writeUInt16LE(NETWORK_STATUS, 56);
  bytes.writeUInt32LE(1, 64);
  return bytes;
}

function updateU16(mac, value) {
  const bytes = Buffer.alloc(2);
  bytes.writeUInt16LE(value);
  mac.update(bytes);
}

function updateU32(mac, value) {
  const bytes = Buffer.alloc(4);
  bytes.writeUInt32LE(value);
  mac.update(bytes);
}

function updateU64(mac, value) {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(value);
  mac.update(bytes);
}

function requestProof(secret, nonce, counter, origin, body) {
  const route = Buffer.from("/api/v1/control", "utf8");
  const originBytes = Buffer.from(origin, "utf8");
  const mac = crypto.createHmac("sha256", secret);
  mac.update(Buffer.from("ALUMINA-HTTP-AUTH-V2\0", "utf8"));
  mac.update(nonce);
  updateU64(mac, counter);
  mac.update(Buffer.from([2]));
  updateU16(mac, route.length);
  mac.update(route);
  updateU16(mac, originBytes.length);
  mac.update(originBytes);
  updateU32(mac, body.length);
  mac.update(crypto.createHash("sha256").update(body).digest());
  return mac.digest("hex");
}

function responseProof(secret, nonce, counter, status, origin, body) {
  const originBytes = Buffer.from(origin, "utf8");
  const mac = crypto.createHmac("sha256", secret);
  mac.update(Buffer.from("ALUMINA-HTTP-RESPONSE-V2\0", "utf8"));
  mac.update(nonce);
  updateU64(mac, counter);
  updateU16(mac, status);
  mac.update(Buffer.from([2]));
  updateU16(mac, originBytes.length);
  mac.update(originBytes);
  updateU32(mac, body.length);
  mac.update(crypto.createHash("sha256").update(body).digest());
  return mac.digest("hex");
}

async function authenticatedNetworkStatus() {
  const challengeResponse = await fetch(`${deviceOrigin}/api/v1/auth`);
  if (!challengeResponse.ok) {
    throw new Error(`authentication challenge returned HTTP ${challengeResponse.status}`);
  }
  const challenge = await challengeResponse.json();
  if (!/^[0-9a-f]{32}$/.test(challenge.boot_nonce ?? "")) {
    throw new Error("authentication challenge has a non-canonical boot nonce");
  }
  const nonce = Buffer.from(challenge.boot_nonce, "hex");
  const counter = 0xffffffffffffffffn;
  const body = encodeNetworkStatusRequest();
  const proof = requestProof(deviceSecret, nonce, counter, deviceOrigin, body);
  const response = await fetch(`${deviceOrigin}/api/v1/control`, {
    method: "POST",
    headers: {
      "Content-Type": "application/vnd.alumina.frame",
      "X-Alumina-Counter": counter.toString(),
      "X-Alumina-Authorization": proof,
      Origin: deviceOrigin,
    },
    body,
  });
  const responseBytes = Buffer.from(await response.arrayBuffer());
  const responseCounter = response.headers.get("x-alumina-counter");
  const observedProof = response.headers.get("x-alumina-response-authorization");
  if (responseCounter !== counter.toString()) {
    throw new Error("authenticated status response did not echo the exact request counter");
  }
  const expectedProof = responseProof(
    deviceSecret,
    nonce,
    counter,
    response.status,
    deviceOrigin,
    responseBytes,
  );
  if (observedProof !== expectedProof) {
    throw new Error("authenticated status response proof did not verify");
  }
  const native = parseNativeResponse(responseBytes);
  if (!response.ok || native.status !== 0 || native.operation !== NETWORK_STATUS) {
    throw new Error("authenticated status request did not complete successfully");
  }
  return parseStatus(native.body);
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
loaded = new Promise((resolve) => { resolveLoaded = resolve; });
await send("Page.navigate", { url: `${deviceOrigin}/` });
await Promise.race([
  loaded,
  new Promise((_, reject) => setTimeout(
    () => reject(new Error("page load timed out")),
    20_000,
  )),
]);
await waitFor(async () => {
  const state = await send("Runtime.evaluate", {
    expression: `({
      canvas: document.querySelector("canvas")?.width ?? 0,
      ready: typeof globalThis.wasmBindings === "object",
      failed: globalThis.aluminaBootstrapError ?? null,
    })`,
    returnByValue: true,
  });
  if (state.result.value.failed) {
    throw new Error(`bootstrap failed: ${state.result.value.failed}`);
  }
  return state.result.value.ready && state.result.value.canvas === VIEWPORT.width;
}, "WASM application startup");

// Open the fixed right-panel connection form and authenticate to the simulator.
await click(1077, 122);
await replaceText(1230, 199, deviceOrigin);
await replaceText(1230, 238, deviceSecret);
await click(1165, 258);
await new Promise((resolve) => setTimeout(resolve, 5_000));

// The stable connection-ID collapsing identity must survive normal sampling
// phase changes. Provisioning is intentionally the first detailed tool so the
// prerequisite network controls need no timing-dependent panel scroll.
await new Promise((resolve) => setTimeout(resolve, 1_500));
await click(1150, 471);
await click(1115, 710);
await new Promise((resolve) => setTimeout(resolve, 1_000));
const scannedScreenshot = await send("Page.captureScreenshot", {
  format: "png",
  captureBeyondViewport: false,
});
const scannedScreenshotBytes = Buffer.from(scannedScreenshot.data, "base64");

// The strongest retained entry is the protected laboratory WLAN. Coordinates
// are fixed to the admitted 1440x1000 top-priority provisioning layout.
await replaceText(1200, 734, stationSecret);
await click(1415, 755);
await new Promise((resolve) => setTimeout(resolve, 2_000));

const screenshot = await send("Page.captureScreenshot", {
  format: "png",
  captureBeyondViewport: false,
});
const screenshotBytes = Buffer.from(screenshot.data, "base64");
// No mutation is performed outside the UI. This final read-only native query
// uses the maximum boot-scoped counter after the screenshot, verifies the
// response HMAC, and proves the state produced by the browser workflow.
const joined = await authenticatedNetworkStatus();
const expectedFlags = AP_EXPECTED
  | STATION_CONFIGURED
  | STATION_ASSOCIATED
  | STATION_IPV4_READY;
const checks = {
  uiScanGenerationRetained: joined.scanGeneration !== 0,
  exactBssidJoined: joined.ssid === "Alumina Lab"
    && joined.bssid === "02a151000001"
    && joined.authentication === 2
    && joined.channel === 6,
  stationAddressed: joined.stationLink === 4
    && joined.ipv4Prefix === 24
    && joined.ipv4Address.join(".") === "192.168.1.77"
    && joined.ipv4Gateway.join(".") === "192.168.1.1",
  recoveryApPreserved: (joined.flags & AP_EXPECTED) !== 0,
  volatileCredentialExplicit: joined.flags === expectedFlags
    && (joined.flags & CREDENTIALS_DURABLE) === 0,
  noNetworkFailure: joined.lastFailure === 0,
  nontrivialScanScreenshot: scannedScreenshotBytes.length >= 50_000,
  nontrivialScreenshot: screenshotBytes.length >= 50_000,
  noLoadingFailures: loadingFailures.length === 0,
  noExceptions: exceptions.length === 0,
};
const result = {
  qualified: Object.values(checks).every(Boolean),
  configuration: {
    debugOrigin,
    deviceOrigin,
    viewport: VIEWPORT,
    deadlineMs,
  },
  checks,
  joined,
  evidenceBoundary: "UI-only mutation followed by authenticated read-only status",
  scanScreenshotBytes: scannedScreenshotBytes.length,
  screenshotBytes: screenshotBytes.length,
  loadingFailures,
  exceptions,
  logEntries,
};

fs.mkdirSync(path.dirname(outputPrefix), { recursive: true });
fs.writeFileSync(`${outputPrefix}-scan.png`, scannedScreenshotBytes);
fs.writeFileSync(`${outputPrefix}.png`, screenshotBytes);
fs.writeFileSync(`${outputPrefix}.json`, `${JSON.stringify(result, null, 2)}\n`);
console.log(JSON.stringify(result));
socket.close();
if (!result.qualified) {
  throw new Error("network-provisioning browser qualification failed strict checks");
}
