//! An asynchronous, `no_std` driver for the T-Deck's CST328 touch controller.
//!
//! [`touch::TouchController`] initializes the controller and returns five
//! optional [`touch::TouchPoint`] slots per read. See the package README and
//! `simple_touch` example for integration details.

#![no_std]

pub mod touch;
