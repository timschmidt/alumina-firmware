# M9 graph label and execution-domain editor evidence

Date: 2026-08-20

Checkpoint state: source implementation committed; optimized bundle and browser
rerun still open against the moving live Hyper stack.

This checkpoint adds the two remaining basic node-metadata operations to the
canonical graph workspace without turning either field into hidden execution
authority. Labels remain bounded canonical human metadata. Execution placement
is selected only from reviewed domain families and concrete device identities
already established by the graph.

## Source boundary

- Interface implementation:
  `f96fadda31b1ac4c1869691eeca342112f328690`
  (`feat: edit graph labels and execution domains`).
- Preceding firmware evidence checkpoint:
  `c5188b38ed05e05bd61852385c86a711d8345ef7`.
- Both repositories remained on `agent/zizmor-ci-hardening` throughout the
  checkpoint. `alumina-firmware` is the canonical firmware repository; the
  retired `aluminafw` path was not used.
- Firmware source, protocol, board packages, target images, and physical-device
  state are unchanged by this host-interface checkpoint.
- The connected bare MKS TinyBee V1.0 was not opened, reset, flashed, probed,
  or contacted. No GPIO, motor, motor power, analyzer lead, or WLAN was used.
- Current CSGRS/Hyper sibling path dependencies remained read-only. No sibling
  status, diff, reset, pin, format, stage, commit, or edit was performed.

## Canonical structural operations

`GraphWorkspaceDocument::set_node_label` transactionally replaces one exact
UTF-8 label. Complete `GraphDocument` reconstruction enforces the embedded byte
ceiling, rejects empty or control-bearing text, preserves the node kind,
domain, ports, parameters, stable identities, placement, and allocation
cursors, and leaves both graph and workspace revisions unchanged for an exact
no-op. A changed label advances both revisions and changes canonical ALGR/ALGW
identity because it is saved graph metadata. Semantic kind matching never uses
the label.

`GraphWorkspaceDocument::set_node_domain` provides the deliberately narrower
structural primitive. It replaces only one concrete `HostExact`,
`Service(device)`, or `Realtime(device)` placement and rejects a zero device
identity through complete document validation. It does not itself admit a
kind/domain pair, a cross-domain wire, a clock relationship, an implementation,
a target capability, or a deployment.

The UI closes those additional authorities before commit. It derives domain
families from the exact audited `NodeSchema`; derives nonzero device identities
only from existing device-cycle clocks or existing node placements; sorts those
identities canonically; exposes no raw device-ID text field; and reruns complete
draft semantic analysis plus canonical encoding on the cloned candidate. A
HostExact-only graph with no reviewed device owner therefore cannot invent a
Service or Realtime target. Label and domain changes use the existing bounded
canonical ALGW/ALGP pair history, refresh the probe binding, participate in
origin-local pair persistence, and detach reference trace evidence whose ALGR
identity no longer matches.

## Implemented checks

Core regression tests prove:

- Unicode label replacement, changed canonical graph identity, stable
  placement/cursors, exact replay, and no-op revision stability;
- atomic rejection of empty, control-bearing, over-byte-limit, and unknown-node
  label edits;
- structural Service placement with every unrelated workspace fact retained;
- independent audited rejection of that placement for a HostExact-only kind;
  and
- atomic rejection of a zero device identity and an unknown node.

UI regression tests prove:

- label changes update the bound ALGW/ALGP pair, undo and redo together, survive
  exact pair persistence/replay, and do not record exact no-ops;
- invalid label edits do not dirty persistence or alter history;
- the representative inspector offers only `HostExact` and rejects a directly
  attempted foreign Service placement without mutation;
- the offline TinyBee clock context deterministically yields HostExact,
  Service, and Realtime choices for its one known reference device when all
  three reviewed families are requested; and
- a headless egui frame renders the selected-node label/domain controls.

During the worktree qualification, the complete native suite passed with 304
tests: 70 application/coordinator, 82 protocol-client, 151 core, and one public
exact-control integration test. Warnings-denied native and WASM Clippy passed,
all six WASM test artifacts linked, strict rustdoc passed, and the local-source
and permissive-license audit passed. Those runs covered the transactional core,
the extracted UI controls, and all new tests; the final source-only adjustment
added the visible current UTF-8 byte count.

## Open production rerun

The optimized Trunk build was then interrupted inside the concurrently edited
live `hypercurve` path dependency before a final Alumina bundle existed. A
subsequent native rerun likewise stopped in HyperCurve before compiling Alumina;
the observed upstream compiler diagnostic changed between attempts, confirming
that the shared dependency was moving. The sibling repository was intentionally
left untouched and no old release was pinned or substituted.

Consequently this record does **not** claim a final production WASM artifact,
compression hashes, localhost browser click-through, or exact-commit full-suite
rerun yet. Those checks remain an explicit follow-up gate against the live
workspace stack. The previously qualified canonical HostExact reference ALGW
and ALGP values are not asserted as fresh artifact evidence by this checkpoint.

## Closed claims and licensing

This is bounded host authoring support. It does not authenticate or discover a
device, create a device identity, lower or upload a graph, acquire a resource,
run Service/Realtime code, read a pin, arm an output, or command motion. Firmware
safety, deployment admission, and physical ownership remain independent.

The implementation is independently authored under MIT and adds no dependency.
No GPL-family source, library, generated asset, or tool output was copied,
linked, or vendored.
