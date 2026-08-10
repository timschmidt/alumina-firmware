//! An asynchronous, `no_std` driver for the Semtech SX126x family of LoRa transceivers.
//!
//! This crate provides the LoRa-oriented SX126x command set over
//! `embedded-hal-async` traits.
//!
//! The main entry point is the `SX126x` struct, which takes an async SPI peripheral
//! and the necessary GPIO pins to communicate with the modem.
//!
//! # Usage
//!
//! See the package README for configuration details and `t-deck-pro-lora-async`
//! for a board-specific wrapper.

#![no_std]

pub mod conf;
pub mod op;
pub mod reg;

mod sx;
pub use sx::*;
