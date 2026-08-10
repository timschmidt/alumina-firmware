#![no_std]
//! Single-threaded asynchronous I2C and SPI bus sharing.
//!
//! See the package README for ownership constraints and complete examples.

extern crate alloc;

pub mod i2c;
pub mod spi;
