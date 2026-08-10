//! An asynchronous, `no_std` driver for the T-Deck's LoRa module (SX1262).
//!
//! [`lora::LoraRadio`] provides board-specific initialization, send, and receive
//! operations over the generic `sx126x_async` driver. See the package README
//! and the sender/receiver examples for complete wiring.

#![no_std]
#![deny(missing_docs)]

//! Board-specific T-Deck Pro LoRa types.
extern crate alloc;

pub mod lora;
