# TinyBee diagnostic visuals

No board image is admitted yet. Add an independently licensed photograph of the
exact fixture revision only after it has been reconciled against the schematic
and physical connectors. Prefer a newly captured, evenly lit, orthographic top
view released as `CC0-1.0`; `CC-BY-4.0` is acceptable with attribution.

Do not copy vendor repository or wiki artwork into this permissively licensed
repository. A visual becomes a capability only when its package record includes
the repository-relative path, media type, exact dimensions, canonical 256-bit
content digest, SPDX license, attribution, and reviewed normalized resource polygons.
The `visual.top-hotspots` HIL requirement remains unmet until the UI overlay is
reconciled against the powered fixture.

For the first safe-image capture, retain both an unannotated fixture photograph
and an annotated derivative showing U1 pin 11/SRCLK (BCLK), pin 12/RCLK (WS),
pin 14/SER (DATA), pin 8/GND, EXP1 pin 4/LCD_RS_O (MARKER), the square EXP1 pin-1
pad, empty StepStick sockets, and every disconnected motor/heater/fan/display
connector. Do not infer pin order from the keyed EXP1 shroud. The annotation
must derive from the operator-owned photograph; a generated or vendor-board
lookalike cannot represent the actual fixture.
