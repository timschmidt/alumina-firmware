//! Initialization-related parameters.

/// Standby mode configuration.
#[repr(u8)]
#[derive(Copy, Clone)]
pub enum StandbyConfig {
    /// Standby with RC 13MHz oscillator.
    StbyRc = 0x00,
    /// Standby with XOSC (crystal oscillator).
    StbyXOSC = 0x01,
}
