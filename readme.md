To build:

cargo run --release

A new WASM file will be built, stripped, and gzip compressed within the egui directory and linked in to the build.

to flash devices which make use of a ch340 USB serial adapter you must modify ~/.config/espflash.toml like so:

```                                                                                                                                                                                   3,23          All
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
