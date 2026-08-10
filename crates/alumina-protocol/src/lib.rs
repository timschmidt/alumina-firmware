#![no_std]
#![doc = "Bounded wire-level identities shared by Alumina firmware, simulator, and UI."]

use core::fmt;

/// Magic bytes at the beginning of every native Alumina frame.
pub const FRAME_MAGIC: [u8; 4] = *b"ALUM";

/// The exact protocol version implemented by this source tree.
pub const PROTOCOL_VERSION: u16 = 1;

/// A canonical 256-bit content or configuration digest.
#[derive(Clone, Copy, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    /// The all-zero digest, reserved for "not established" fields.
    pub const ZERO: Self = Self([0; 32]);

    /// Returns true when no digest has been established.
    pub const fn is_zero(self) -> bool {
        let mut index = 0;
        while index < self.0.len() {
            if self.0[index] != 0 {
                return false;
            }
            index += 1;
        }
        true
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Digest(")?;
        for byte in &self.0[..4] {
            write!(formatter, "{byte:02x}")?;
        }
        formatter.write_str("…)")
    }
}

/// Stable identity provisioned into one physical MCU.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DeviceId(pub [u8; 16]);

/// Boot-scoped identity. It changes on every restart and invalidates prepared work.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct BootId(pub u64);

/// An unwrapped tick in a device's declared monotonic clock domain.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct DeviceCycle(pub u64);

/// Native bounded frame families.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FrameKind {
    /// Identity and boot information.
    Identity = 1,
    /// Board and machine capability information.
    Capabilities = 2,
    /// A timestamped clock heartbeat.
    ClockSample = 3,
    /// Transactional configuration traffic.
    Configuration = 4,
    /// Immutable job upload and control traffic.
    Job = 5,
    /// Bounded real-time or service command traffic.
    Command = 6,
    /// Sample, event, health, or fault telemetry.
    Telemetry = 7,
}

/// Fixed prefix validated before any frame payload is decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct FrameHeader {
    /// Constant [`FRAME_MAGIC`].
    pub magic: [u8; 4],
    /// Must equal [`PROTOCOL_VERSION`]; no compatibility negotiation is performed.
    pub version: u16,
    /// Frame family.
    pub kind: FrameKind,
    /// Kind-specific flags. Unknown set bits are rejected by the payload decoder.
    pub flags: u8,
    /// Encoded payload size after this header.
    pub payload_len: u32,
    /// Monotonic sequence within the relevant stream.
    pub sequence: u32,
    /// Device-cycle timestamp or requested execution cycle.
    pub cycle: DeviceCycle,
    /// Configuration this frame was compiled or sampled against.
    pub config_digest: Digest,
}

impl FrameHeader {
    /// Constructs a header for this exact protocol version.
    pub const fn new(
        kind: FrameKind,
        payload_len: u32,
        sequence: u32,
        cycle: DeviceCycle,
        config_digest: Digest,
    ) -> Self {
        Self {
            magic: FRAME_MAGIC,
            version: PROTOCOL_VERSION,
            kind,
            flags: 0,
            payload_len,
            sequence,
            cycle,
            config_digest,
        }
    }

    /// Validates the universal bounded prefix.
    pub fn validate(&self, maximum_payload_len: u32) -> Result<(), HeaderError> {
        if self.magic != FRAME_MAGIC {
            return Err(HeaderError::Magic);
        }
        if self.version != PROTOCOL_VERSION {
            return Err(HeaderError::Version {
                received: self.version,
            });
        }
        if self.payload_len > maximum_payload_len {
            return Err(HeaderError::PayloadTooLarge {
                received: self.payload_len,
                maximum: maximum_payload_len,
            });
        }
        Ok(())
    }
}

/// Universal frame-prefix validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HeaderError {
    /// Magic did not identify the native Alumina protocol.
    Magic,
    /// The frame belongs to another exact schema version.
    Version {
        /// Version found in the frame.
        received: u16,
    },
    /// Payload length exceeded the receiving endpoint's fixed budget.
    PayloadTooLarge {
        /// Length found in the frame.
        received: u32,
        /// Maximum admitted by this endpoint.
        maximum: u32,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_header_validates_at_exact_limit() {
        let header = FrameHeader::new(FrameKind::Command, 128, 7, DeviceCycle(42), Digest([3; 32]));

        assert_eq!(header.validate(128), Ok(()));
    }

    #[test]
    fn version_mismatch_is_not_negotiated() {
        let mut header = FrameHeader::new(FrameKind::Identity, 0, 0, DeviceCycle(0), Digest::ZERO);
        header.version += 1;

        assert_eq!(
            header.validate(0),
            Err(HeaderError::Version {
                received: PROTOCOL_VERSION + 1
            })
        );
    }

    #[test]
    fn oversized_payload_is_rejected_before_decode() {
        let header = FrameHeader::new(FrameKind::Telemetry, 65, 1, DeviceCycle(3), Digest::ZERO);

        assert_eq!(
            header.validate(64),
            Err(HeaderError::PayloadTooLarge {
                received: 65,
                maximum: 64
            })
        );
    }
}
