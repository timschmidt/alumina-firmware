<img src="docs/alumina-diagram.png" width="40%" alt="Diagram"/>

# Alumina Firmware

Alumina is an integrated CAD/CAM, physics simulation, and motion control solution written entirely in Rust.  It is intended to control laser and plasma cutters, 3D printers, CNC routers and mills, and lathes.

There are two parts to Alumina:
 - [firmware](https://github.com/timschmidt/alumina-firmware)
   - targets the xtensa and risc-v esp32 microcontrollers
   - sets up a Wifi AP called "Alumina"
   - serves the Alumina UI via HTTP
   - responds to commands from the Alumina UI via HTTP
   - performs motion planning and step generation
 - [UI](https://github.com/timschmidt/alumina-ui)
   - targets [WebAssembly](https://en.wikipedia.org/wiki/WebAssembly)
   - draws geometry using WebGL and egui
   - manipulates geometry using [csgrs](https://github.com/timschmidt/csgrs)

Both parts are linked together at compile time and fit in the onboard flash of the microcontroller, reducing design complexity, part count, and cost.

## Community
[![](https://dcbadge.limes.pink/api/server/https://discord.gg/cCHRjpkPhQ)](https://discord.gg/cCHRjpkPhQ)

## Hardware
### MKS TinyBee
<img src="https://raw.githubusercontent.com/makerbase-mks/MKS-TinyBee/refs/heads/main/hardware/Image/MKS%20TinyBee%20V1.x%20Wiring.png" width="60%" alt="MKS TinyBee"/>

[MKS TinyBee GitHub](https://github.com/makerbase-mks/MKS-TinyBee/)

## HTTP API
```
/						GET index.html
/alumina-ui.js			GET alumina-ui.js
/alumina-ui.html		GET alumina-ui.html.gz
/alumina-ui_bg.wasm		GET alumina-ui_bg.wasm.br
/favicon.ico			GET favaicon.gif
/time					GET 
/files					POST 
/queue					GET, POST 
/board					GET json: {{"name":"{}","image_mime":"{}","image_url":"/board/image"}}
/board/image			GET PNG formatted board image
```

## Development
### Build and flash firmware
```shell
export IDF_PATH="~/.espressif/esp-idf/v5.4.1/"

cargo run --release
```

to flash devices which make use of a ch340 USB serial adapter you must modify ~/.config/espflash.toml like so:

```toml
[connection]
# esp32-c3
#serial = "/dev/ttyACM0"
# ch340 + esp32-c3
serial = "/dev/ttyUSB0"

# esp32-c3
#[[usb_device]]
#vid = "303a"
#pid = "1001"

# ch340 + esp32-c3
[[usb_device]] 
vid="1a86"
pid="7523"
```

## Todo
- BLE support
- flesh out wifi scan / connect and nvs support
- get motion control working
- klipper style multi-mcu support
- get endstops and homing working
- implement sd card support
- data logging (from IMU, temp/humid, received commands) to SD available space
- add internet cloud to diagram animation
- crunch board graphics and link at compile time for diagnostics tab
- bus pirate features
- support http://wiki.fluidnc.com/en/hardware/existing_hardware
- FoC Servo support
- MPPT
- Active Rectification support
- PID / bang-bang heater control
- rename board to device
