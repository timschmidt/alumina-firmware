Alumina is an integrated CAD/CAM, physics simulation, and motion control solution written entirely in Rust.  It is intended to control laser and plasma cutters, 3D printers, CNC routers and mills, and lathes.  There are two parts to Alumina: the [firmware](https://github.com/timschmidt/alumina-firmware) which targets the esp32c3 microcontroller, sets up a Wifi AP called "Alumina", serves the [Alumina UI](https://github.com/timschmidt/alumina-ui) via HTTP, responds to commands from the Alumina UI via HTTP, and performs motion planning and step generation.  The UI targets [WebAssembly](https://en.wikipedia.org/wiki/WebAssembly), draws geometry using WebGL and egui, and manipulates geometry using [csgrs](https://github.com/timschmidt/csgrs).  Both parts fit in the onboard flash of the esp32c3.

<img src="doc/alumina-diagram.png" width="50%" alt="Diagram"/>

## Community
[![](https://dcbadge.limes.pink/api/server/https://discord.gg/cCHRjpkPhQ)](https://discord.gg/9WkD3WFxMC)

## Development
### Build and flash firmware
```shell
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
- get motion control working
- klipper style multi-mcu support
- get endstops and homing working
- implement sd card support
- update to more recent esp-idf and supporting crates
