//! Conservative, allocation-free stack-watermark accounting.
//!
//! Target code owns the unsafe operation of painting and reading memory that is
//! known to be below the current stack pointer. This module owns all bounds,
//! incremental-scan, monotonicity, and wire-format rules and is host tested.

use core::mem::size_of;

use alumina_protocol::DeviceCycle;

/// Bytes in one target stack word.
pub const STACK_WORD_BYTES: usize = size_of::<u32>();
/// Exact fixed representation of one stack-watermark snapshot.
pub const STACK_WATERMARK_WIRE_BYTES: usize = 48;

const STACK_WATERMARK_MAGIC: [u8; 4] = *b"ASWM";
const STACK_WATERMARK_VERSION: u8 = 1;
const CANARY_SEED: u32 = 0xa17a_5e6d;

/// Executor stack measured by one report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum StackDomain {
    /// Linker-owned stack used by the service-core executor.
    ServiceCore = 0,
    /// Separately allocated stack used by the real-time-core executor.
    RealtimeCore = 1,
}

impl StackDomain {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::ServiceCore),
            1 => Some(Self::RealtimeCore),
            _ => None,
        }
    }
}

/// Meaning of one conservative stack-watermark snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct StackWatermarkFlags(pub u16);

impl StackWatermarkFlags {
    /// A same-core canary epoch was initialized successfully.
    pub const INITIALIZED: u16 = 1 << 0;
    /// At least one complete bounded scan reached the current low-water mark.
    pub const COMPLETE_SWEEP: u16 = 1 << 1;
    /// Startup before the epoch is deliberately counted as unknown/used.
    pub const PARTIAL_BOOT_EPOCH: u16 = 1 << 2;
    /// Every sample also conservatively incorporates the current stack pointer.
    pub const CURRENT_POINTER_BOUND: u16 = 1 << 3;
    const KNOWN: u16 = Self::INITIALIZED
        | Self::COMPLETE_SWEEP
        | Self::PARTIAL_BOOT_EPOCH
        | Self::CURRENT_POINTER_BOUND;

    /// Whether all bits have defined V1 meaning.
    pub const fn is_valid(self) -> bool {
        self.0 & !Self::KNOWN == 0
    }

    /// Whether the measurement epoch is live.
    pub const fn initialized(self) -> bool {
        self.0 & Self::INITIALIZED != 0
    }
}

/// Conservative observation of one executor stack.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackWatermarkSnapshot {
    /// Stack owner.
    pub domain: StackDomain,
    /// Measurement semantics and progress.
    pub flags: StackWatermarkFlags,
    /// Complete linker/allocation range, including the low guard exclusion.
    pub allocated_bytes: u32,
    /// Low bytes omitted to preserve the RTOS guard and a policy margin.
    pub excluded_low_bytes: u32,
    /// Bytes painted below the initialization-time stack-pointer reserve.
    pub painted_bytes: u32,
    /// Smallest canary-confirmed or current-pointer-bounded free prefix.
    pub minimum_headroom_bytes: u32,
    /// Number of bounded sampling passes attempted.
    pub samples: u32,
    /// Complete scans of the then-current untouched prefix.
    pub completed_sweeps: u32,
    /// Local cycle at which the partial measurement epoch began.
    pub epoch_cycle: DeviceCycle,
    /// Local cycle of the newest sample.
    pub sampled_at: DeviceCycle,
}

impl StackWatermarkSnapshot {
    /// Explicit absence marker for a domain not yet observed.
    pub const fn unavailable(domain: StackDomain) -> Self {
        Self {
            domain,
            flags: StackWatermarkFlags(0),
            allocated_bytes: 0,
            excluded_low_bytes: 0,
            painted_bytes: 0,
            minimum_headroom_bytes: 0,
            samples: 0,
            completed_sweeps: 0,
            epoch_cycle: DeviceCycle(0),
            sampled_at: DeviceCycle(0),
        }
    }

    /// Conservative maximum use since the partial epoch, excluding guard bytes.
    pub const fn maximum_used_bytes(self) -> u32 {
        self.allocated_bytes
            .saturating_sub(self.excluded_low_bytes)
            .saturating_sub(self.minimum_headroom_bytes)
    }

    /// Validates all V1 relationships without trusting a producer.
    pub fn validate(self) -> Result<(), StackWatermarkError> {
        if !self.flags.is_valid() {
            return Err(StackWatermarkError::Flags);
        }
        if !self.flags.initialized() {
            return if self == Self::unavailable(self.domain) {
                Ok(())
            } else {
                Err(StackWatermarkError::State)
            };
        }
        let monitored = self
            .allocated_bytes
            .checked_sub(self.excluded_low_bytes)
            .ok_or(StackWatermarkError::Range)?;
        if monitored == 0
            || self.painted_bytes > monitored
            || self.minimum_headroom_bytes > self.painted_bytes
            || !self.allocated_bytes.is_multiple_of(STACK_WORD_BYTES as u32)
            || !self
                .excluded_low_bytes
                .is_multiple_of(STACK_WORD_BYTES as u32)
            || !self.painted_bytes.is_multiple_of(STACK_WORD_BYTES as u32)
            || !self
                .minimum_headroom_bytes
                .is_multiple_of(STACK_WORD_BYTES as u32)
            || self.sampled_at < self.epoch_cycle
            || self.completed_sweeps > self.samples
        {
            return Err(StackWatermarkError::State);
        }
        let complete = self.flags.0 & StackWatermarkFlags::COMPLETE_SWEEP != 0;
        if complete != (self.completed_sweeps != 0)
            || self.flags.0 & StackWatermarkFlags::PARTIAL_BOOT_EPOCH == 0
            || self.flags.0 & StackWatermarkFlags::CURRENT_POINTER_BOUND == 0
        {
            return Err(StackWatermarkError::State);
        }
        Ok(())
    }

    /// Encodes one fixed canonical report.
    pub fn encode(self) -> Result<[u8; STACK_WATERMARK_WIRE_BYTES], StackWatermarkError> {
        self.validate()?;
        let mut encoded = [0_u8; STACK_WATERMARK_WIRE_BYTES];
        encoded[..4].copy_from_slice(&STACK_WATERMARK_MAGIC);
        encoded[4] = STACK_WATERMARK_VERSION;
        encoded[5] = self.domain as u8;
        encoded[6..8].copy_from_slice(&self.flags.0.to_le_bytes());
        encoded[8..12].copy_from_slice(&self.allocated_bytes.to_le_bytes());
        encoded[12..16].copy_from_slice(&self.excluded_low_bytes.to_le_bytes());
        encoded[16..20].copy_from_slice(&self.painted_bytes.to_le_bytes());
        encoded[20..24].copy_from_slice(&self.minimum_headroom_bytes.to_le_bytes());
        encoded[24..28].copy_from_slice(&self.samples.to_le_bytes());
        encoded[28..32].copy_from_slice(&self.completed_sweeps.to_le_bytes());
        encoded[32..40].copy_from_slice(&self.epoch_cycle.0.to_le_bytes());
        encoded[40..48].copy_from_slice(&self.sampled_at.0.to_le_bytes());
        Ok(encoded)
    }

    /// Decodes and independently validates exactly one report.
    pub fn decode(encoded: &[u8]) -> Result<Self, StackWatermarkError> {
        if encoded.len() != STACK_WATERMARK_WIRE_BYTES
            || encoded[..4] != STACK_WATERMARK_MAGIC
            || encoded[4] != STACK_WATERMARK_VERSION
        {
            return Err(StackWatermarkError::Format);
        }
        let snapshot = Self {
            domain: StackDomain::from_wire(encoded[5]).ok_or(StackWatermarkError::Format)?,
            flags: StackWatermarkFlags(read_u16(encoded, 6)),
            allocated_bytes: read_u32(encoded, 8),
            excluded_low_bytes: read_u32(encoded, 12),
            painted_bytes: read_u32(encoded, 16),
            minimum_headroom_bytes: read_u32(encoded, 20),
            samples: read_u32(encoded, 24),
            completed_sweeps: read_u32(encoded, 28),
            epoch_cycle: DeviceCycle(read_u64(encoded, 32)),
            sampled_at: DeviceCycle(read_u64(encoded, 40)),
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

/// Portable owner of one incremental downward-stack scan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StackWatermarkTracker {
    snapshot: StackWatermarkSnapshot,
    cursor_words: u32,
}

impl StackWatermarkTracker {
    /// Starts a partial-boot canary epoch after target code paints the range.
    pub fn new(
        domain: StackDomain,
        allocated_bytes: usize,
        excluded_low_bytes: usize,
        painted_bytes: usize,
        epoch_cycle: DeviceCycle,
    ) -> Result<Self, StackWatermarkError> {
        let allocated_bytes =
            u32::try_from(allocated_bytes).map_err(|_| StackWatermarkError::Range)?;
        let excluded_low_bytes =
            u32::try_from(excluded_low_bytes).map_err(|_| StackWatermarkError::Range)?;
        let painted_bytes = u32::try_from(painted_bytes).map_err(|_| StackWatermarkError::Range)?;
        let snapshot = StackWatermarkSnapshot {
            domain,
            flags: StackWatermarkFlags(
                StackWatermarkFlags::INITIALIZED
                    | StackWatermarkFlags::PARTIAL_BOOT_EPOCH
                    | StackWatermarkFlags::CURRENT_POINTER_BOUND,
            ),
            allocated_bytes,
            excluded_low_bytes,
            painted_bytes,
            minimum_headroom_bytes: painted_bytes,
            samples: 0,
            completed_sweeps: 0,
            epoch_cycle,
            sampled_at: epoch_cycle,
        };
        snapshot.validate()?;
        Ok(Self {
            snapshot,
            cursor_words: 0,
        })
    }

    /// Samples a bounded canary window and folds in a conservative current-SP bound.
    ///
    /// `available_below_sp_bytes` is the distance from the first monitored word
    /// to a caller-selected reserve below the current stack pointer. `matches`
    /// receives word offsets from that same first monitored word.
    pub fn sample(
        &mut self,
        available_below_sp_bytes: usize,
        maximum_scan_words: usize,
        sampled_at: DeviceCycle,
        mut matches: impl FnMut(usize) -> bool,
    ) -> Result<StackWatermarkSnapshot, StackWatermarkError> {
        if maximum_scan_words == 0 || sampled_at < self.snapshot.sampled_at {
            return Err(StackWatermarkError::State);
        }
        let available_words = available_below_sp_bytes / STACK_WORD_BYTES;
        let painted_words = self.snapshot.painted_bytes as usize / STACK_WORD_BYTES;
        let mut untouched_words = self.snapshot.minimum_headroom_bytes as usize / STACK_WORD_BYTES;
        let bounded_available = available_words.min(painted_words);
        if bounded_available < untouched_words {
            untouched_words = bounded_available;
            self.cursor_words = 0;
        }

        let start = (self.cursor_words as usize).min(untouched_words);
        let end = start
            .saturating_add(maximum_scan_words)
            .min(untouched_words);
        let mut mismatch = None;
        for word in start..end {
            if !matches(word) {
                mismatch = Some(word);
                break;
            }
        }

        if let Some(word) = mismatch {
            untouched_words = word;
            self.cursor_words = 0;
        } else if end == untouched_words {
            self.cursor_words = 0;
            self.snapshot.completed_sweeps = self.snapshot.completed_sweeps.saturating_add(1);
            self.snapshot.flags.0 |= StackWatermarkFlags::COMPLETE_SWEEP;
        } else {
            self.cursor_words = u32::try_from(end).map_err(|_| StackWatermarkError::Range)?;
        }

        self.snapshot.minimum_headroom_bytes = u32::try_from(
            untouched_words
                .checked_mul(STACK_WORD_BYTES)
                .ok_or(StackWatermarkError::Range)?,
        )
        .map_err(|_| StackWatermarkError::Range)?;
        self.snapshot.samples = self.snapshot.samples.saturating_add(1);
        self.snapshot.sampled_at = sampled_at;
        self.snapshot.validate()?;
        Ok(self.snapshot)
    }

    /// Latest observation without performing target memory access.
    pub const fn snapshot(&self) -> StackWatermarkSnapshot {
        self.snapshot
    }
}

/// Stack-watermark construction or wire rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StackWatermarkError {
    /// Magic, version, length, or domain is unknown.
    Format,
    /// A numeric range cannot represent a target stack.
    Range,
    /// Flags contain undefined bits.
    Flags,
    /// Fields or sampling progress are internally inconsistent.
    State,
}

/// Address-dependent canary value used by target painters and scanners.
pub const fn stack_canary_word(byte_address: usize) -> u32 {
    let folded = byte_address as u64 ^ ((byte_address as u64) >> 32);
    let mut value = (folded as u32) ^ CANARY_SEED;
    value ^= value.rotate_left(13);
    value = value.wrapping_mul(0x9e37_79b1);
    value ^ value.rotate_right(11)
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("fixed range"))
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("fixed range"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("fixed range"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> StackWatermarkTracker {
        StackWatermarkTracker::new(
            StackDomain::RealtimeCore,
            32 * 1_024,
            256,
            28 * 1_024,
            DeviceCycle(100),
        )
        .unwrap()
    }

    #[test]
    fn address_canary_is_stable_and_address_dependent() {
        assert_eq!(
            stack_canary_word(0x3ffb_0000),
            stack_canary_word(0x3ffb_0000)
        );
        assert_ne!(
            stack_canary_word(0x3ffb_0000),
            stack_canary_word(0x3ffb_0004)
        );
    }

    #[test]
    fn incremental_sweep_is_bounded_and_marked_complete() {
        let mut tracker = tracker();
        let words = 28 * 1_024 / STACK_WORD_BYTES;
        for sample in 0..words / 32 {
            let report = tracker
                .sample(28 * 1_024, 32, DeviceCycle(101 + sample as u64), |_| true)
                .unwrap();
            assert_eq!(report.minimum_headroom_bytes, 28 * 1_024);
        }
        let report = tracker.snapshot();
        assert_eq!(report.completed_sweeps, 1);
        assert_ne!(report.flags.0 & StackWatermarkFlags::COMPLETE_SWEEP, 0);
    }

    #[test]
    fn current_pointer_and_canary_only_reduce_headroom() {
        let mut tracker = tracker();
        let first = tracker
            .sample(20 * 1_024 + 3, 16, DeviceCycle(101), |_| true)
            .unwrap();
        assert_eq!(first.minimum_headroom_bytes, 20 * 1_024);
        let second = tracker
            .sample(27 * 1_024, 64, DeviceCycle(102), |word| word != 40)
            .unwrap();
        assert_eq!(second.minimum_headroom_bytes, 40 * STACK_WORD_BYTES as u32);
        let third = tracker
            .sample(27 * 1_024, 64, DeviceCycle(103), |_| true)
            .unwrap();
        assert_eq!(third.minimum_headroom_bytes, second.minimum_headroom_bytes);
        assert_eq!(third.maximum_used_bytes(), 32 * 1_024 - 256 - 160);
    }

    #[test]
    fn wire_round_trip_and_tamper_rejection_are_exact() {
        let mut tracker = tracker();
        let report = tracker
            .sample(24 * 1_024, 64, DeviceCycle(101), |_| true)
            .unwrap();
        let encoded = report.encode().unwrap();
        assert_eq!(StackWatermarkSnapshot::decode(&encoded), Ok(report));

        let mut bad = encoded;
        bad[6..8].copy_from_slice(&0x8000_u16.to_le_bytes());
        assert_eq!(
            StackWatermarkSnapshot::decode(&bad),
            Err(StackWatermarkError::Flags)
        );

        let mut impossible = encoded;
        impossible[20..24].copy_from_slice(&(30 * 1_024_u32).to_le_bytes());
        assert_eq!(
            StackWatermarkSnapshot::decode(&impossible),
            Err(StackWatermarkError::State)
        );
    }

    #[test]
    fn unavailable_marker_is_canonical() {
        let absent = StackWatermarkSnapshot::unavailable(StackDomain::ServiceCore);
        assert_eq!(
            StackWatermarkSnapshot::decode(&absent.encode().unwrap()),
            Ok(absent)
        );
        let mut noncanonical = absent;
        noncanonical.samples = 1;
        assert_eq!(noncanonical.validate(), Err(StackWatermarkError::State));
    }
}
