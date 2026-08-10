# embedded-bus-async

`embedded-bus-async` adapts one asynchronous I2C or SPI controller into multiple `embedded-hal-async` device handles. It is `no_std`, uses Embassy's `RwLock`, and is intended for cooperative single-threaded executors.

The shared owner is `Rc<RwLock<CriticalSectionRawMutex, BUS>>`. Consequently, these handles are not `Send` and do not provide cross-thread synchronization. Every transaction still takes the lock exclusively so operations from different device tasks cannot interleave.

## API

- `i2c::RwLockI2cDevice<BUS, ERROR>` implements `embedded_hal_async::i2c::I2c`. The target address remains an argument to each I2C operation.
- `spi::RwLockDevice<BUS, CS, DELAY>` implements `embedded_hal_async::spi::SpiDevice<u8>` for any `embedded_hal::digital::OutputPin` chip-select.
- `spi::transaction` exposes the shared transaction sequence for callers that need it directly.
- `spi::DeviceError` distinguishes an SPI-bus failure from an actual chip-select failure.

## I2C sharing

Create one locked owner and clone the `Rc` into each driver:

```rust,ignore
use alloc::rc::Rc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embedded_bus_async::i2c::RwLockI2cDevice;

let bus = Rc::new(RwLock::<CriticalSectionRawMutex, _>::new(i2c));
let battery_i2c = RwLockI2cDevice::new(bus.clone());
let keyboard_i2c = RwLockI2cDevice::new(bus.clone());
let touch_i2c = RwLockI2cDevice::new(bus);
```

Each wrapper implements the full I2C trait, so it can be moved directly into a device driver such as `BatteryService` or `KeyboardController`.

## SPI sharing

Each SPI handle owns its chip-select pin and delay provider:

```rust,ignore
use alloc::rc::Rc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embassy_time::Delay;
use embedded_bus_async::spi::RwLockDevice;

let bus = Rc::new(RwLock::<CriticalSectionRawMutex, _>::new(spi));
let display_spi = RwLockDevice::new(bus.clone(), display_cs, Delay)?;
let radio_spi = RwLockDevice::new(bus, radio_cs, Delay)?;
```

For every `SpiDevice::transaction`, the wrapper acquires the bus, drives CS low, performs the operations in order, flushes even after an operation error, and finally drives CS high. Delay operations flush before sleeping.

## References

- [`embedded-hal-async` I2C traits](https://docs.rs/embedded-hal-async/1/embedded_hal_async/i2c/)
- [`embedded-hal-async` SPI traits](https://docs.rs/embedded-hal-async/1/embedded_hal_async/spi/)
- [Embassy shared-bus adapters](https://github.com/embassy-rs/embassy/tree/main/embassy-embedded-hal/src/shared_bus)
- [`embedded-hal-bus`](https://docs.rs/embedded-hal-bus/0.3), the synchronous reference design

## License

Licensed under [Apache-2.0](../LICENSE).
