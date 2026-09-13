const reportBootstrapFailure = (reason) => {
  const detail = reason instanceof Error ? reason.message : String(reason);
  self.postMessage(
    JSON.stringify({
      schema_version: 1,
      event: {
        kind: "fatal",
        message: `control worker bootstrap failed: ${detail.slice(0, 256)}`,
      },
    }),
  );
};

self.addEventListener("unhandledrejection", (event) => {
  event.preventDefault();
  reportBootstrapFailure(event.reason);
});

try {
  const { loadAluminaInterface } = await import("./alumina-bootstrap.js");
  const { bindings } = await loadAluminaInterface();
  bindings.start_control_worker();
} catch (error) {
  reportBootstrapFailure(error);
}
