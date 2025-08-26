// This file contains code for interfacing with platform-specific pins
// can we use information from svd or hal files directly?

struct Esp32c3 {

}

struct Esp32c6 {

}

pub struct Esp32Wroom32U;

/// Common “safe” GPIOs for classic ESP32 modules (including WROOM-32U).
/// Avoid 1/3 (UART0), 6–11 (flash), and 12 (strap) unless you know what you’re doing.
impl Esp32Wroom32U {
    pub const SAFE_GPIO: [i32; 18] = [
        2, 4, 5, 13, 14, 15, 16, 17,
        18, 19, 21, 22, 23, 25, 26, 27, 32, 33,
    ];
}
