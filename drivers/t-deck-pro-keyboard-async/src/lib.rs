//! An asynchronous, `no_std` driver for the T-Deck's keyboard.
//!
//! [`keyboard::KeyboardController`] configures the TCA8418 FIFO and returns
//! T-Deck-mapped [`keyboard::KeyEvent`] values. See the package README and
//! `simple_keyboard` example for integration details.

#![no_std]

pub mod keyboard;
