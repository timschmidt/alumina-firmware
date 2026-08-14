//! Passive service-core health reporting with no motion or safety authority.

use alumina_protocol::{DeviceCycle, FrameKind, Operation, StatusCode};
use alumina_runtime::health::{
    RUNTIME_HEALTH_WIRE_BYTES, RuntimeHealthFlags, RuntimeHealthSnapshot,
};
use alumina_runtime::stack::{StackDomain, StackWatermarkSnapshot};

use crate::{NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse};

/// Queue occupancies sampled by the sole service owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeQueueHealth {
    pub command_depth: u16,
    pub command_capacity: u16,
    pub work_depth: u16,
    pub work_capacity: u16,
    pub telemetry_depth: u16,
    pub telemetry_capacity: u16,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObservedRealtimeStack {
    frame_sequence: u32,
    produced_at: DeviceCycle,
    snapshot: StackWatermarkSnapshot,
}

/// Why a core-1 stack observation was rejected and prior health data revoked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RuntimeHealthObservationError {
    Sequence,
    Future,
    Expired,
    Domain,
    State,
}

/// Sole core-0 owner of passive runtime-health observations.
pub struct RuntimeHealthService {
    maximum_realtime_age_cycles: u64,
    realtime: Option<ObservedRealtimeStack>,
}

impl RuntimeHealthService {
    /// Creates an empty observer with an explicit freshness bound.
    pub const fn new(maximum_realtime_age_cycles: u64) -> Self {
        Self {
            maximum_realtime_age_cycles,
            realtime: None,
        }
    }

    /// Whether a valid universal request selects the health family.
    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes())
                .is_ok_and(|request| request.frame.kind == FrameKind::Health)
    }

    /// Accepts only fresh, monotonic reports from the real-time stack owner.
    pub fn observe_realtime(
        &mut self,
        frame_sequence: u32,
        produced_at: DeviceCycle,
        observed_at: DeviceCycle,
        snapshot: StackWatermarkSnapshot,
    ) -> Result<(), RuntimeHealthObservationError> {
        let result = self.validate_observation(frame_sequence, produced_at, observed_at, snapshot);
        match result {
            Ok(()) => {
                self.realtime = Some(ObservedRealtimeStack {
                    frame_sequence,
                    produced_at,
                    snapshot,
                });
                Ok(())
            }
            Err(error) => {
                self.realtime = None;
                Err(error)
            }
        }
    }

    /// Revokes a previously observed report after boundary or fault corruption.
    pub fn invalidate_realtime(&mut self) {
        self.realtime = None;
    }

    /// Returns the passive fixed health body for one authenticated request.
    pub fn dispatch(
        &self,
        request: &ServiceRequest,
        now: DeviceCycle,
        queues: RuntimeQueueHealth,
        service_stack: Option<StackWatermarkSnapshot>,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Health {
            return ServiceResponse::invalid_native();
        }
        if native.message.operation != Operation::HealthSnapshot {
            return respond(native, now, StatusCode::Unsupported, &[]);
        }
        if !native.frame.config_digest.is_zero() || !native.body.is_empty() {
            return respond(native, now, StatusCode::InvalidRequest, &[]);
        }
        let Some(service_stack) = service_stack else {
            return respond(native, now, StatusCode::Unsupported, &[]);
        };

        let mut flags = RuntimeHealthFlags(0);
        let realtime_stack = if let Some(realtime) = self.realtime {
            flags.0 |= RuntimeHealthFlags::REALTIME_STACK_PRESENT;
            if self.is_fresh(realtime, now) {
                flags.0 |= RuntimeHealthFlags::REALTIME_STACK_FRESH;
            }
            realtime.snapshot
        } else {
            StackWatermarkSnapshot::unavailable(StackDomain::RealtimeCore)
        };
        let snapshot = RuntimeHealthSnapshot {
            flags,
            snapshot_cycle: now,
            command_queue_depth: queues.command_depth,
            command_queue_capacity: queues.command_capacity,
            work_queue_depth: queues.work_depth,
            work_queue_capacity: queues.work_capacity,
            telemetry_queue_depth: queues.telemetry_depth,
            telemetry_queue_capacity: queues.telemetry_capacity,
            service_stack,
            realtime_stack,
        };
        let body = match snapshot.encode() {
            Ok(body) => body,
            Err(_) => return respond(native, now, StatusCode::Internal, &[]),
        };
        respond(native, now, StatusCode::Ok, &body)
    }

    fn validate_observation(
        &self,
        frame_sequence: u32,
        produced_at: DeviceCycle,
        observed_at: DeviceCycle,
        snapshot: StackWatermarkSnapshot,
    ) -> Result<(), RuntimeHealthObservationError> {
        snapshot
            .validate()
            .map_err(|_| RuntimeHealthObservationError::State)?;
        if snapshot.domain != StackDomain::RealtimeCore || !snapshot.flags.initialized() {
            return Err(RuntimeHealthObservationError::Domain);
        }
        if frame_sequence == 0 {
            return Err(RuntimeHealthObservationError::Sequence);
        }
        if snapshot.sampled_at > produced_at {
            return Err(RuntimeHealthObservationError::Future);
        }
        let age = observed_at
            .0
            .checked_sub(produced_at.0)
            .ok_or(RuntimeHealthObservationError::Future)?;
        let sample_age = observed_at
            .0
            .checked_sub(snapshot.sampled_at.0)
            .ok_or(RuntimeHealthObservationError::Future)?;
        if age > self.maximum_realtime_age_cycles || sample_age > self.maximum_realtime_age_cycles {
            return Err(RuntimeHealthObservationError::Expired);
        }
        if let Some(previous) = self.realtime {
            if !serial_is_newer(frame_sequence, previous.frame_sequence) {
                return Err(RuntimeHealthObservationError::Sequence);
            }
            let old = previous.snapshot;
            if snapshot.epoch_cycle != old.epoch_cycle
                || snapshot.allocated_bytes != old.allocated_bytes
                || snapshot.excluded_low_bytes != old.excluded_low_bytes
                || snapshot.painted_bytes != old.painted_bytes
                || snapshot.minimum_headroom_bytes > old.minimum_headroom_bytes
                || snapshot.samples < old.samples
                || snapshot.completed_sweeps < old.completed_sweeps
                || snapshot.sampled_at <= old.sampled_at
            {
                return Err(RuntimeHealthObservationError::State);
            }
        }
        Ok(())
    }

    fn is_fresh(&self, observed: ObservedRealtimeStack, now: DeviceCycle) -> bool {
        let frame_fresh = now
            .0
            .checked_sub(observed.produced_at.0)
            .is_some_and(|age| age <= self.maximum_realtime_age_cycles);
        let sample_fresh = now
            .0
            .checked_sub(observed.snapshot.sampled_at.0)
            .is_some_and(|age| age <= self.maximum_realtime_age_cycles);
        frame_fresh && sample_fresh
    }
}

fn respond(
    request: NativeRequest<'_>,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> ServiceResponse {
    ServiceResponse::native(request, now, status, body)
        .unwrap_or_else(|_| ServiceResponse::invalid_native())
}

const fn serial_is_newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance < (1_u32 << 31)
}

const _: () = assert!(
    RUNTIME_HEALTH_WIRE_BYTES
        + alumina_protocol::FrameHeader::WIRE_LEN
        + alumina_protocol::MessageHeader::WIRE_LEN
        <= crate::MAX_SERVICE_RESPONSE_BYTES
);

#[cfg(test)]
mod tests {
    use super::*;
    use alumina_net::MAX_AUTHENTICATED_BODY_BYTES;
    use alumina_protocol::{Digest, FrameHeader, MessageDirection, MessageHeader, StatusCode};
    use alumina_runtime::health::RuntimeHealthSnapshot;
    use alumina_runtime::stack::StackWatermarkTracker;

    fn report(domain: StackDomain, at: u64, headroom: usize) -> StackWatermarkSnapshot {
        let mut tracker =
            StackWatermarkTracker::new(domain, 32 * 1_024, 256, 28 * 1_024, DeviceCycle(1))
                .unwrap();
        tracker
            .sample(headroom, 32, DeviceCycle(at), |_| true)
            .unwrap()
    }

    fn request(operation: Operation, body: &[u8]) -> ServiceRequest {
        let payload_len = MessageHeader::WIRE_LEN + body.len();
        let frame = FrameHeader::new(
            operation.frame_kind(),
            u32::try_from(payload_len).unwrap(),
            3,
            DeviceCycle(5),
            Digest::ZERO,
        );
        let message = MessageHeader::request(operation, 11, u32::try_from(body.len()).unwrap());
        let mut bytes = [0_u8; MAX_AUTHENTICATED_BODY_BYTES];
        bytes[..FrameHeader::WIRE_LEN].copy_from_slice(&frame.encode());
        bytes[FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN]
            .copy_from_slice(&message.encode());
        bytes[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..FrameHeader::WIRE_LEN + payload_len]
            .copy_from_slice(body);
        ServiceRequest::native(&bytes[..FrameHeader::WIRE_LEN + payload_len]).unwrap()
    }

    fn response_parts(response: &ServiceResponse) -> (StatusCode, &[u8]) {
        let frame = FrameHeader::decode(
            &response.bytes()[..FrameHeader::WIRE_LEN],
            u32::try_from(MAX_AUTHENTICATED_BODY_BYTES).unwrap(),
        )
        .unwrap();
        let message = MessageHeader::decode_and_validate(
            &response.bytes()
                [FrameHeader::WIRE_LEN..FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN],
            frame.kind,
            frame.payload_len,
        )
        .unwrap();
        assert_eq!(message.direction, MessageDirection::Response);
        (
            message.status,
            &response.bytes()[FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN..],
        )
    }

    fn queues() -> RuntimeQueueHealth {
        RuntimeQueueHealth {
            command_depth: 2,
            command_capacity: 8,
            work_depth: 3,
            work_capacity: 8,
            telemetry_depth: 4,
            telemetry_capacity: 32,
        }
    }

    #[test]
    fn health_request_returns_both_stacks_and_queue_bounds() {
        let mut service = RuntimeHealthService::new(500);
        assert!(RuntimeHealthService::handles(&request(
            Operation::HealthSnapshot,
            &[]
        )));
        service
            .observe_realtime(
                1,
                DeviceCycle(100),
                DeviceCycle(100),
                report(StackDomain::RealtimeCore, 100, 24 * 1_024),
            )
            .unwrap();
        let response = service.dispatch(
            &request(Operation::HealthSnapshot, &[]),
            DeviceCycle(200),
            queues(),
            Some(report(StackDomain::ServiceCore, 190, 20 * 1_024)),
        );
        let (status, body) = response_parts(&response);
        assert_eq!(status, StatusCode::Ok);
        let decoded = RuntimeHealthSnapshot::decode(body).unwrap();
        assert_eq!(decoded.command_queue_depth, 2);
        assert_ne!(
            decoded.flags.0 & RuntimeHealthFlags::REALTIME_STACK_FRESH,
            0
        );
        assert_eq!(decoded.realtime_stack.minimum_headroom_bytes, 24 * 1_024);
    }

    #[test]
    fn stale_report_is_retained_but_not_marked_fresh() {
        let mut service = RuntimeHealthService::new(50);
        service
            .observe_realtime(
                1,
                DeviceCycle(100),
                DeviceCycle(100),
                report(StackDomain::RealtimeCore, 100, 24 * 1_024),
            )
            .unwrap();
        let response = service.dispatch(
            &request(Operation::HealthSnapshot, &[]),
            DeviceCycle(200),
            queues(),
            Some(report(StackDomain::ServiceCore, 190, 20 * 1_024)),
        );
        let (_, body) = response_parts(&response);
        let decoded = RuntimeHealthSnapshot::decode(body).unwrap();
        assert_ne!(
            decoded.flags.0 & RuntimeHealthFlags::REALTIME_STACK_PRESENT,
            0
        );
        assert_eq!(
            decoded.flags.0 & RuntimeHealthFlags::REALTIME_STACK_FRESH,
            0
        );
    }

    #[test]
    fn substitution_and_nonmonotonic_reports_revoke_prior_state() {
        let mut service = RuntimeHealthService::new(500);
        service
            .observe_realtime(
                1,
                DeviceCycle(100),
                DeviceCycle(100),
                report(StackDomain::RealtimeCore, 100, 20 * 1_024),
            )
            .unwrap();
        assert_eq!(
            service.observe_realtime(
                2,
                DeviceCycle(110),
                DeviceCycle(110),
                report(StackDomain::ServiceCore, 110, 20 * 1_024),
            ),
            Err(RuntimeHealthObservationError::Domain)
        );
        let response = service.dispatch(
            &request(Operation::HealthSnapshot, &[]),
            DeviceCycle(120),
            queues(),
            Some(report(StackDomain::ServiceCore, 120, 20 * 1_024)),
        );
        let (_, body) = response_parts(&response);
        assert_eq!(RuntimeHealthSnapshot::decode(body).unwrap().flags.0, 0);

        let mut service = RuntimeHealthService::new(500);
        service
            .observe_realtime(
                1,
                DeviceCycle(100),
                DeviceCycle(100),
                report(StackDomain::RealtimeCore, 100, 20 * 1_024),
            )
            .unwrap();
        assert_eq!(
            service.observe_realtime(
                2,
                DeviceCycle(110),
                DeviceCycle(110),
                report(StackDomain::RealtimeCore, 110, 24 * 1_024),
            ),
            Err(RuntimeHealthObservationError::State)
        );
    }

    #[test]
    fn nonempty_health_body_is_rejected() {
        let service = RuntimeHealthService::new(500);
        let service_stack = report(StackDomain::ServiceCore, 10, 20 * 1_024);
        let invalid = service.dispatch(
            &request(Operation::HealthSnapshot, &[1]),
            DeviceCycle(10),
            queues(),
            Some(service_stack),
        );
        assert_eq!(response_parts(&invalid).0, StatusCode::InvalidRequest);
    }

    #[test]
    fn missing_service_probe_is_explicitly_unsupported() {
        let service = RuntimeHealthService::new(500);
        let response = service.dispatch(
            &request(Operation::HealthSnapshot, &[]),
            DeviceCycle(10),
            queues(),
            None,
        );
        assert_eq!(response_parts(&response).0, StatusCode::Unsupported);
    }
}
