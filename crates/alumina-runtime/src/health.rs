//! Canonical fixed device-health response shared by firmware and clients.

use alumina_protocol::DeviceCycle;

use crate::stack::{
    STACK_WATERMARK_WIRE_BYTES, StackDomain, StackWatermarkError, StackWatermarkSnapshot,
};

/// Exact V1 runtime-health body length.
pub const RUNTIME_HEALTH_WIRE_BYTES: usize = 124;

const RUNTIME_HEALTH_MAGIC: [u8; 4] = *b"AHLT";
const RUNTIME_HEALTH_VERSION: u8 = 1;
const SERVICE_STACK_OFFSET: usize = 28;
const REALTIME_STACK_OFFSET: usize = SERVICE_STACK_OFFSET + STACK_WATERMARK_WIRE_BYTES;

/// Availability and freshness facts for one runtime-health response.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct RuntimeHealthFlags(pub u8);

impl RuntimeHealthFlags {
    /// A valid core-1 stack report has been observed this boot.
    pub const REALTIME_STACK_PRESENT: u8 = 1 << 0;
    /// The core-1 report is within the service freshness window.
    pub const REALTIME_STACK_FRESH: u8 = 1 << 1;
    const KNOWN: u8 = Self::REALTIME_STACK_PRESENT | Self::REALTIME_STACK_FRESH;

    /// Whether all bits have defined V1 meaning.
    pub const fn is_valid(self) -> bool {
        self.0 & !Self::KNOWN == 0
    }
}

/// Exact queue occupancy and two executor-stack observations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeHealthSnapshot {
    /// Report availability/freshness.
    pub flags: RuntimeHealthFlags,
    /// Service-core response cycle.
    pub snapshot_cycle: DeviceCycle,
    /// Ordered command queue occupancy and fixed capacity.
    pub command_queue_depth: u16,
    pub command_queue_capacity: u16,
    /// Deterministic work queue occupancy and fixed capacity.
    pub work_queue_depth: u16,
    pub work_queue_capacity: u16,
    /// Lossy RT-to-service telemetry queue occupancy and fixed capacity.
    pub telemetry_queue_depth: u16,
    pub telemetry_queue_capacity: u16,
    /// Same-core service-executor stack observation.
    pub service_stack: StackWatermarkSnapshot,
    /// Latest observed real-time-executor stack observation, or an absence marker.
    pub realtime_stack: StackWatermarkSnapshot,
}

impl RuntimeHealthSnapshot {
    /// Validates queue and report relationships without trusting the producer.
    pub fn validate(self) -> Result<(), RuntimeHealthError> {
        if !self.flags.is_valid()
            || self.command_queue_capacity == 0
            || self.work_queue_capacity == 0
            || self.telemetry_queue_capacity == 0
            || self.command_queue_depth > self.command_queue_capacity
            || self.work_queue_depth > self.work_queue_capacity
            || self.telemetry_queue_depth > self.telemetry_queue_capacity
        {
            return Err(RuntimeHealthError::State);
        }
        self.service_stack
            .validate()
            .map_err(RuntimeHealthError::Stack)?;
        self.realtime_stack
            .validate()
            .map_err(RuntimeHealthError::Stack)?;
        if self.service_stack.domain != StackDomain::ServiceCore
            || !self.service_stack.flags.initialized()
            || self.realtime_stack.domain != StackDomain::RealtimeCore
            || self.service_stack.sampled_at > self.snapshot_cycle
        {
            return Err(RuntimeHealthError::State);
        }
        let present = self.flags.0 & RuntimeHealthFlags::REALTIME_STACK_PRESENT != 0;
        let fresh = self.flags.0 & RuntimeHealthFlags::REALTIME_STACK_FRESH != 0;
        if fresh && !present {
            return Err(RuntimeHealthError::State);
        }
        if present != self.realtime_stack.flags.initialized() {
            return Err(RuntimeHealthError::State);
        }
        if present && self.realtime_stack.sampled_at > self.snapshot_cycle {
            return Err(RuntimeHealthError::State);
        }
        Ok(())
    }

    /// Encodes the exact canonical V1 body.
    pub fn encode(self) -> Result<[u8; RUNTIME_HEALTH_WIRE_BYTES], RuntimeHealthError> {
        self.validate()?;
        let mut encoded = [0_u8; RUNTIME_HEALTH_WIRE_BYTES];
        encoded[..4].copy_from_slice(&RUNTIME_HEALTH_MAGIC);
        encoded[4] = RUNTIME_HEALTH_VERSION;
        encoded[5] = self.flags.0;
        encoded[8..16].copy_from_slice(&self.snapshot_cycle.0.to_le_bytes());
        encoded[16..18].copy_from_slice(&self.command_queue_depth.to_le_bytes());
        encoded[18..20].copy_from_slice(&self.command_queue_capacity.to_le_bytes());
        encoded[20..22].copy_from_slice(&self.work_queue_depth.to_le_bytes());
        encoded[22..24].copy_from_slice(&self.work_queue_capacity.to_le_bytes());
        encoded[24..26].copy_from_slice(&self.telemetry_queue_depth.to_le_bytes());
        encoded[26..28].copy_from_slice(&self.telemetry_queue_capacity.to_le_bytes());
        encoded[SERVICE_STACK_OFFSET..REALTIME_STACK_OFFSET].copy_from_slice(
            &self
                .service_stack
                .encode()
                .map_err(RuntimeHealthError::Stack)?,
        );
        encoded[REALTIME_STACK_OFFSET..].copy_from_slice(
            &self
                .realtime_stack
                .encode()
                .map_err(RuntimeHealthError::Stack)?,
        );
        Ok(encoded)
    }

    /// Decodes and independently validates exactly one V1 body.
    pub fn decode(encoded: &[u8]) -> Result<Self, RuntimeHealthError> {
        if encoded.len() != RUNTIME_HEALTH_WIRE_BYTES
            || encoded[..4] != RUNTIME_HEALTH_MAGIC
            || encoded[4] != RUNTIME_HEALTH_VERSION
            || encoded[6] != 0
            || encoded[7] != 0
        {
            return Err(RuntimeHealthError::Format);
        }
        let snapshot = Self {
            flags: RuntimeHealthFlags(encoded[5]),
            snapshot_cycle: DeviceCycle(read_u64(encoded, 8)),
            command_queue_depth: read_u16(encoded, 16),
            command_queue_capacity: read_u16(encoded, 18),
            work_queue_depth: read_u16(encoded, 20),
            work_queue_capacity: read_u16(encoded, 22),
            telemetry_queue_depth: read_u16(encoded, 24),
            telemetry_queue_capacity: read_u16(encoded, 26),
            service_stack: StackWatermarkSnapshot::decode(
                &encoded[SERVICE_STACK_OFFSET..REALTIME_STACK_OFFSET],
            )
            .map_err(RuntimeHealthError::Stack)?,
            realtime_stack: StackWatermarkSnapshot::decode(&encoded[REALTIME_STACK_OFFSET..])
                .map_err(RuntimeHealthError::Stack)?,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
}

/// Runtime-health construction or wire rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeHealthError {
    /// Magic, version, length, or reserved bytes are invalid.
    Format,
    /// Queue, freshness, or domain relationships are inconsistent.
    State,
    /// One nested stack report is invalid.
    Stack(StackWatermarkError),
}

fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("fixed range"))
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("fixed range"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stack::{StackWatermarkFlags, StackWatermarkSnapshot};

    fn stack(domain: StackDomain, sampled_at: u64) -> StackWatermarkSnapshot {
        StackWatermarkSnapshot {
            domain,
            flags: StackWatermarkFlags(
                StackWatermarkFlags::INITIALIZED
                    | StackWatermarkFlags::PARTIAL_BOOT_EPOCH
                    | StackWatermarkFlags::CURRENT_POINTER_BOUND,
            ),
            allocated_bytes: 32 * 1_024,
            excluded_low_bytes: 256,
            painted_bytes: 28 * 1_024,
            minimum_headroom_bytes: 20 * 1_024,
            samples: 4,
            completed_sweeps: 0,
            epoch_cycle: DeviceCycle(1),
            sampled_at: DeviceCycle(sampled_at),
        }
    }

    fn snapshot() -> RuntimeHealthSnapshot {
        RuntimeHealthSnapshot {
            flags: RuntimeHealthFlags(
                RuntimeHealthFlags::REALTIME_STACK_PRESENT
                    | RuntimeHealthFlags::REALTIME_STACK_FRESH,
            ),
            snapshot_cycle: DeviceCycle(200),
            command_queue_depth: 2,
            command_queue_capacity: 8,
            work_queue_depth: 3,
            work_queue_capacity: 8,
            telemetry_queue_depth: 4,
            telemetry_queue_capacity: 32,
            service_stack: stack(StackDomain::ServiceCore, 190),
            realtime_stack: stack(StackDomain::RealtimeCore, 180),
        }
    }

    #[test]
    fn exact_health_body_round_trips() {
        let snapshot = snapshot();
        let encoded = snapshot.encode().unwrap();
        assert_eq!(encoded.len(), RUNTIME_HEALTH_WIRE_BYTES);
        assert_eq!(RuntimeHealthSnapshot::decode(&encoded), Ok(snapshot));
    }

    #[test]
    fn queue_overflow_and_domain_substitution_are_rejected() {
        let mut overflow = snapshot();
        overflow.command_queue_depth = 9;
        assert_eq!(overflow.validate(), Err(RuntimeHealthError::State));

        let mut zero_capacity = snapshot();
        zero_capacity.telemetry_queue_depth = 0;
        zero_capacity.telemetry_queue_capacity = 0;
        assert_eq!(zero_capacity.validate(), Err(RuntimeHealthError::State));

        let mut substituted = snapshot();
        substituted.realtime_stack.domain = StackDomain::ServiceCore;
        assert_eq!(substituted.validate(), Err(RuntimeHealthError::State));
    }

    #[test]
    fn absent_realtime_report_requires_canonical_marker() {
        let mut absent = snapshot();
        absent.flags = RuntimeHealthFlags(0);
        absent.realtime_stack = StackWatermarkSnapshot::unavailable(StackDomain::RealtimeCore);
        assert!(absent.validate().is_ok());

        absent.flags = RuntimeHealthFlags(RuntimeHealthFlags::REALTIME_STACK_FRESH);
        assert_eq!(absent.validate(), Err(RuntimeHealthError::State));
    }
}
