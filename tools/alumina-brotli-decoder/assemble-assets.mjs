import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const [repositoryDirectory, interfaceDist, generatedDirectory, interfaceCommit] =
  process.argv.slice(2);
if (!repositoryDirectory || !interfaceDist || !generatedDirectory || !interfaceCommit) {
  throw new Error(
    "usage: assemble-assets.mjs REPOSITORY INTERFACE_DIST GENERATED INTERFACE_COMMIT",
  );
}

const EXPECTED_INTERFACE_COMMIT = "0a111d2e5243db0e356e9befa1ac28c77785753a";
const MAXIMUM_BOOTSTRAP_COMPRESSED_BYTES = 4 * 1_024 * 1_024;
const MAXIMUM_BOOTSTRAP_DECOMPRESSED_BYTES = 16 * 1_024 * 1_024;
const EXPECTED_INTERFACE_SOURCES = Object.freeze({
  "alumina-interface.js": Object.freeze({
    bytes: 91_816,
    sha256: "4f0d819019f81df95b324a7f9a521effeb1c7d95ca56172be6b3af7e6c7f192e",
  }),
  "alumina-interface_bg.wasm": Object.freeze({
    bytes: 8_421_767,
    sha256: "b9670ad660146faa860bd1c7ee086e32da9b174f333fec3c161e6f11dd4ff8d9",
  }),
  "favicon.ico": Object.freeze({
    bytes: 196,
    sha256: "311f2089f900726358e69d1c490236942ea17aa418f198764a5662c8eaeeb5af",
  }),
});

if (interfaceCommit !== EXPECTED_INTERFACE_COMMIT) {
  throw new Error(
    `interface HEAD ${interfaceCommit} does not match admitted commit ${EXPECTED_INTERFACE_COMMIT}`,
  );
}

const toolDirectory = path.dirname(new URL(import.meta.url).pathname);
const assetsDirectory = path.join(
  repositoryDirectory,
  "crates",
  "alumina-web-assets",
  "assets",
);
const generatedRust = path.join(
  repositoryDirectory,
  "crates",
  "alumina-web-assets",
  "src",
  "generated.rs",
);
fs.mkdirSync(assetsDirectory, { recursive: true });

function sha256(bytes) {
  return crypto.createHash("sha256").update(bytes).digest("hex");
}

function exactFile(filePath, expected) {
  const bytes = fs.readFileSync(filePath);
  const actual = { bytes: bytes.length, sha256: sha256(bytes) };
  if (actual.bytes !== expected.bytes || actual.sha256 !== expected.sha256) {
    throw new Error(
      `${filePath} is ${actual.bytes}/${actual.sha256}, expected `
      + `${expected.bytes}/${expected.sha256}`,
    );
  }
  return bytes;
}

const interfaceModule = exactFile(
  path.join(interfaceDist, "alumina-interface.js"),
  EXPECTED_INTERFACE_SOURCES["alumina-interface.js"],
);
const interfaceWasm = exactFile(
  path.join(interfaceDist, "alumina-interface_bg.wasm"),
  EXPECTED_INTERFACE_SOURCES["alumina-interface_bg.wasm"],
);
const favicon = exactFile(
  path.join(interfaceDist, "favicon.ico"),
  EXPECTED_INTERFACE_SOURCES["favicon.ico"],
);
const moduleBrotli = fs.readFileSync(
  path.join(generatedDirectory, "alumina-interface.js.br"),
);
const wasmBrotli = fs.readFileSync(
  path.join(generatedDirectory, "alumina-interface_bg.wasm.br"),
);

for (const [name, source, wire] of [
  ["interface module", interfaceModule, moduleBrotli],
  ["interface WASM", interfaceWasm, wasmBrotli],
]) {
  if (wire.length > MAXIMUM_BOOTSTRAP_COMPRESSED_BYTES) {
    throw new Error(
      `${name} Brotli representation exceeds the fixed bootstrap compressed limit`,
    );
  }
  if (source.length > MAXIMUM_BOOTSTRAP_DECOMPRESSED_BYTES) {
    throw new Error(
      `${name} source exceeds the fixed bootstrap decompressed limit`,
    );
  }
}
const decoderWasm = fs.readFileSync(
  path.join(generatedDirectory, "alumina-brotli-decoder_bg.wasm"),
);
const decoderGlue = fs.readFileSync(
  path.join(generatedDirectory, "alumina-brotli-decoder.js"),
  "utf8",
);

const replacements = Object.freeze({
  "@@DECODER_WIRE_BYTES@@": String(decoderWasm.length),
  "@@DECODER_WIRE_SHA256@@": sha256(decoderWasm),
  "@@MODULE_SOURCE_BYTES@@": String(interfaceModule.length),
  "@@MODULE_SOURCE_SHA256@@": sha256(interfaceModule),
  "@@MODULE_WIRE_BYTES@@": String(moduleBrotli.length),
  "@@MODULE_WIRE_SHA256@@": sha256(moduleBrotli),
  "@@WASM_SOURCE_BYTES@@": String(interfaceWasm.length),
  "@@WASM_SOURCE_SHA256@@": sha256(interfaceWasm),
  "@@WASM_WIRE_BYTES@@": String(wasmBrotli.length),
  "@@WASM_WIRE_SHA256@@": sha256(wasmBrotli),
});

function replaceAll(template, values) {
  let output = template;
  for (const [placeholder, value] of Object.entries(values)) {
    if (!output.includes(placeholder)) {
      throw new Error(`template is missing ${placeholder}`);
    }
    output = output.replaceAll(placeholder, value);
  }
  if (output.includes("@@")) {
    throw new Error("an unresolved generated-asset placeholder remains");
  }
  return output;
}

const bootstrapTail = replaceAll(
  fs.readFileSync(path.join(toolDirectory, "bootstrap-tail.template.js"), "utf8"),
  replacements,
);
const bootstrap = Buffer.from(
  `${decoderGlue.trimEnd()}\n\n${bootstrapTail.trimStart()}`,
  "utf8",
);
const bootstrapSri = `sha384-${crypto
  .createHash("sha384")
  .update(bootstrap)
  .digest("base64")}`;
const index = Buffer.from(
  replaceAll(
    fs.readFileSync(path.join(toolDirectory, "index.template.html"), "utf8"),
    { "@@BOOTSTRAP_SRI@@": bootstrapSri },
  ),
  "utf8",
);
const worker = fs.readFileSync(path.join(toolDirectory, "alumina-worker.js"));

const assets = [
  {
    constant: "INDEX",
    file: "index.html",
    path: "/",
    sourceMediaType: "text/html; charset=utf-8",
    wireMediaType: "text/html; charset=utf-8",
    representation: "identity",
    rustRepresentation: "Identity",
    source: index,
    wire: index,
  },
  {
    constant: "BOOTSTRAP",
    file: "alumina-bootstrap.js",
    path: "/alumina-bootstrap.js",
    sourceMediaType: "text/javascript; charset=utf-8",
    wireMediaType: "text/javascript; charset=utf-8",
    representation: "identity",
    rustRepresentation: "Identity",
    source: bootstrap,
    wire: bootstrap,
  },
  {
    constant: "DECODER_WASM",
    file: "alumina-brotli-decoder_bg.wasm",
    path: "/alumina-brotli-decoder_bg.wasm",
    sourceMediaType: "application/wasm",
    wireMediaType: "application/wasm",
    representation: "identity",
    rustRepresentation: "Identity",
    source: decoderWasm,
    wire: decoderWasm,
  },
  {
    constant: "MODULE_BROTLI",
    file: "alumina-interface.js.br",
    path: "/alumina-interface.js.br",
    sourceMediaType: "text/javascript; charset=utf-8",
    wireMediaType: "application/octet-stream",
    representation: "brotli-rfc7932-q11-w23",
    rustRepresentation: "BrotliRfc7932Q11W23",
    source: interfaceModule,
    wire: moduleBrotli,
  },
  {
    constant: "WASM_BROTLI",
    file: "alumina-interface_bg.wasm.br",
    path: "/alumina-interface_bg.wasm.br",
    sourceMediaType: "application/wasm",
    wireMediaType: "application/octet-stream",
    representation: "brotli-rfc7932-q11-w23",
    rustRepresentation: "BrotliRfc7932Q11W23",
    source: interfaceWasm,
    wire: wasmBrotli,
  },
  {
    constant: "WORKER",
    file: "alumina-worker.js",
    path: "/alumina-worker.js",
    sourceMediaType: "text/javascript; charset=utf-8",
    wireMediaType: "text/javascript; charset=utf-8",
    representation: "identity",
    rustRepresentation: "Identity",
    source: worker,
    wire: worker,
  },
  {
    constant: "FAVICON",
    file: "favicon.ico",
    path: "/favicon.ico",
    sourceMediaType: "image/x-icon",
    wireMediaType: "image/x-icon",
    representation: "identity",
    rustRepresentation: "Identity",
    source: favicon,
    wire: favicon,
  },
];

for (const asset of assets) {
  fs.writeFileSync(path.join(assetsDirectory, asset.file), asset.wire);
  asset.sourceSha256 = sha256(asset.source);
  asset.wireSha256 = sha256(asset.wire);
}

let manifest = `format = "alumina-web-bundle-v2"\n`;
manifest += `interface_commit = "${interfaceCommit}"\n`;
manifest += `asset_count = ${assets.length}\n`;
manifest += `brotli_quality = 11\n`;
manifest += `brotli_lgwin = 23\n`;
for (const asset of assets) {
  manifest += `\n[[asset]]\n`;
  manifest += `path = "${asset.path}"\n`;
  manifest += `source_media_type = "${asset.sourceMediaType}"\n`;
  manifest += `wire_media_type = "${asset.wireMediaType}"\n`;
  manifest += `stored_representation = "${asset.representation}"\n`;
  manifest += `source_bytes = ${asset.source.length}\n`;
  manifest += `wire_bytes = ${asset.wire.length}\n`;
  manifest += `source_sha256 = "${asset.sourceSha256}"\n`;
  manifest += `wire_sha256 = "${asset.wireSha256}"\n`;
}
const manifestBytes = Buffer.from(manifest, "utf8");
fs.writeFileSync(path.join(assetsDirectory, "bundle.toml"), manifestBytes);
const manifestSha256 = sha256(manifestBytes);

function rustInteger(value) {
  return String(value).replace(/(?<=\d)(?=(\d{3})+$)/g, "_");
}

let rust = "// @generated by tools/alumina-brotli-decoder/build-assets.sh; do not edit.\n\n";
rust += `/// SHA-256 over [\u0060WEB_BUNDLE_MANIFEST\u0060].\n`;
rust += `pub const WEB_BUNDLE_DIGEST: Digest = digest("${manifestSha256}");\n\n`;
for (const asset of assets) {
  rust += `const ${asset.constant}: &[u8] = include_bytes!("../assets/${asset.file}");\n`;
}
rust += `const BUNDLE_MANIFEST: &[u8] = include_bytes!("../assets/bundle.toml");\n\n`;
rust += "/// Complete finite public asset table in exact route order.\n";
rust += "pub const WEB_ASSETS: &[EmbeddedWebAsset] = &[\n";
for (const asset of assets) {
  rust += "    EmbeddedWebAsset {\n";
  rust += `        path: "${asset.path}",\n`;
  rust += `        source_media_type: "${asset.sourceMediaType}",\n`;
  rust += `        media_type: "${asset.wireMediaType}",\n`;
  rust += `        stored_representation: StoredRepresentation::${asset.rustRepresentation},\n`;
  rust += `        source_bytes: ${rustInteger(asset.source.length)},\n`;
  rust += `        wire_bytes: ${rustInteger(asset.wire.length)},\n`;
  rust += `        source_digest: digest("${asset.sourceSha256}"),\n`;
  rust += `        wire_digest: digest("${asset.wireSha256}"),\n`;
  rust += `        bytes: ${asset.constant},\n`;
  rust += "    },\n";
}
rust += "    EmbeddedWebAsset {\n";
rust += "        path: \"/alumina-web-bundle.toml\",\n";
rust += "        source_media_type: \"text/plain; charset=utf-8\",\n";
rust += "        media_type: \"text/plain; charset=utf-8\",\n";
rust += "        stored_representation: StoredRepresentation::Identity,\n";
rust += `        source_bytes: ${rustInteger(manifestBytes.length)},\n`;
rust += `        wire_bytes: ${rustInteger(manifestBytes.length)},\n`;
rust += "        source_digest: WEB_BUNDLE_DIGEST,\n";
rust += "        wire_digest: WEB_BUNDLE_DIGEST,\n";
rust += "        bytes: BUNDLE_MANIFEST,\n";
rust += "    },\n";
rust += "];\n";
fs.writeFileSync(generatedRust, rust);

console.log(JSON.stringify({
  interfaceCommit,
  bootstrapSri,
  manifestBytes: manifestBytes.length,
  manifestSha256,
  assets: assets.map((asset) => ({
    path: asset.path,
    sourceBytes: asset.source.length,
    wireBytes: asset.wire.length,
    sourceSha256: asset.sourceSha256,
    wireSha256: asset.wireSha256,
    representation: asset.representation,
  })),
}));
