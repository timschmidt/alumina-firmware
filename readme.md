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
