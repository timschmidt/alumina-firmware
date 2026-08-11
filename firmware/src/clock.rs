//! Authenticated heartbeat service and fresh core-1 deadline evidence.

use alumina_clock::{
    BootId, ClockFlags, ClockHeartbeatRequest, ClockHeartbeatResponse, ClockSource,
    RealtimeClockReport,
};
use alumina_protocol::{DeviceCycle, FrameKind, Operation, StatusCode};
use alumina_runtime::{COMMAND_QUEUE_DEPTH, DefaultServiceEndpoint};
use alumina_safety::{EffectiveSafety, SafetyState};
use alumina_service::{NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse};
use embassy_time::{Duration, Instant, TICK_HZ};

const REALTIME_REPORT_MAX_AGE_CYCLES: u64 = Duration::from_millis(500).as_ticks();
pub const MINIMUM_START_LEAD_CYCLES: u64 = Duration::from_millis(500).as_ticks();
pub const MAXIMUM_START_HORIZON_CYCLES: u64 = Duration::from_secs(60).as_ticks();

/// Current cached-job facts folded into a heartbeat without exposing actor state.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ClockJobFacts {
    pub prepared: bool,
    pub committed: bool,
    pub running: bool,
    pub queue_horizon_cycles: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ObservedRealtimeClock {
    frame_sequence: u32,
    produced_at: DeviceCycle,
    report: RealtimeClockReport,
}

/// Why core-1 clock evidence was discarded and prior authority revoked.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RealtimeClockObservationError {
    Sequence,
    Future,
    Expired,
    Counters,
}

/// Sole core-0 owner of boot identity and cumulative RT deadline observations.
pub struct ClockService {
    boot_id: BootId,
    realtime: Option<ObservedRealtimeClock>,
    latest_probe: Option<(u64, DeviceCycle)>,
}

impl ClockService {
    pub const fn new(boot_id: BootId) -> Self {
        Self {
            boot_id,
            realtime: None,
            latest_probe: None,
        }
    }

    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes())
                .is_ok_and(|request| request.frame.kind == FrameKind::ClockSample)
    }

    /// Installs only fresh, monotonic cumulative evidence from core 1.
    pub fn observe_realtime(
        &mut self,
        frame_sequence: u32,
        produced_at: DeviceCycle,
        observed_at: DeviceCycle,
        report: RealtimeClockReport,
    ) -> Result<(), RealtimeClockObservationError> {
        let result = self.validate_observation(frame_sequence, produced_at, observed_at, report);
        match result {
            Ok(()) => {
                self.realtime = Some(ObservedRealtimeClock {
                    frame_sequence,
                    produced_at,
                    report,
                });
                Ok(())
            }
            Err(error) => {
                self.realtime = None;
                Err(error)
            }
        }
    }

    pub fn invalidate_realtime(&mut self) {
        self.realtime = None;
    }

    /// Responds with a service receive/transmit timestamp pair in Embassy's
    /// shared monotonic domain.
    pub fn dispatch(
        &mut self,
        request: &ServiceRequest,
        receive_cycle: DeviceCycle,
        endpoint: &DefaultServiceEndpoint,
        safety: EffectiveSafety,
        jobs: ClockJobFacts,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::ClockSample {
            return ServiceResponse::invalid_native();
        }
        if native.message.operation != Operation::ClockHeartbeat {
            return ServiceResponse::native(native, receive_cycle, StatusCode::Unsupported, &[])
                .unwrap_or_else(|_| ServiceResponse::invalid_native());
        }
        if !native.frame.config_digest.is_zero() {
            return ServiceResponse::native(native, receive_cycle, StatusCode::Conflict, &[])
                .unwrap_or_else(|_| ServiceResponse::invalid_native());
        }
        let heartbeat = match ClockHeartbeatRequest::decode(native.body) {
            Ok(request) => request,
            Err(_) => {
                return ServiceResponse::native(
                    native,
                    receive_cycle,
                    StatusCode::InvalidRequest,
                    &[],
                )
                .unwrap_or_else(|_| ServiceResponse::invalid_native());
            }
        };
        let transmit_cycle = DeviceCycle(Instant::now().as_ticks());
        let realtime = self.fresh_realtime(transmit_cycle);
        let deadline_healthy = realtime.is_some_and(|report| report.missed_deadlines == 0);
        let safety_unhealthy =
            !safety.fresh || matches!(safety.state, SafetyState::Boot | SafetyState::Fault);
        let mut flags = ClockFlags::MONOTONIC | ClockFlags::SHARED_BETWEEN_CORES;
        if deadline_healthy {
            flags |= ClockFlags::DEADLINE_HEALTHY;
        }
        if jobs.prepared {
            flags |= ClockFlags::JOB_PREPARED;
        }
        if jobs.committed {
            flags |= ClockFlags::JOB_COMMITTED;
        }
        if jobs.running {
            flags |= ClockFlags::JOB_RUNNING;
        }
        if safety_unhealthy {
            flags |= ClockFlags::SAFETY_UNHEALTHY;
        }
        let report = ClockHeartbeatResponse {
            flags: ClockFlags(flags),
            counter_bits: 64,
            source: ClockSource::EmbassyMonotonic,
            probe_id: heartbeat.probe_id,
            ui_send_ns: heartbeat.ui_send_ns,
            boot_id: self.boot_id,
            receive_cycle,
            transmit_cycle,
            frequency_hz: TICK_HZ,
            minimum_lead_cycles: MINIMUM_START_LEAD_CYCLES,
            maximum_schedule_horizon_cycles: MAXIMUM_START_HORIZON_CYCLES,
            queue_horizon_cycles: jobs.queue_horizon_cycles,
            maximum_lateness_cycles: realtime.map_or(0, |report| report.maximum_lateness_cycles),
            missed_deadlines: realtime.map_or(0, |report| report.missed_deadlines),
            command_queue_free: u32::try_from(
                COMMAND_QUEUE_DEPTH.saturating_sub(endpoint.command_depth()),
            )
            .expect("command queue depth fits its protocol field"),
            work_queue_depth: u32::try_from(endpoint.work_depth())
                .expect("work queue depth fits its protocol field"),
        };
        let body = match report.encode() {
            Ok(body) => body,
            Err(_) => {
                return ServiceResponse::native(native, transmit_cycle, StatusCode::Internal, &[])
                    .unwrap_or_else(|_| ServiceResponse::invalid_native());
            }
        };
        let response = match ServiceResponse::native(native, transmit_cycle, StatusCode::Ok, &body)
        {
            Ok(response) => response,
            Err(_) => return ServiceResponse::invalid_native(),
        };
        self.latest_probe = if deadline_healthy && !safety_unhealthy {
            Some((heartbeat.probe_id, transmit_cycle))
        } else {
            None
        };
        response
    }

    /// Fresh exact probe identity that a schedule commit may cite.
    pub fn latest_probe_id(&self, now: DeviceCycle) -> Option<u64> {
        let (probe_id, sampled_at) = self.latest_probe?;
        let age = now.0.checked_sub(sampled_at.0)?;
        (age <= REALTIME_REPORT_MAX_AGE_CYCLES).then_some(probe_id)
    }

    fn validate_observation(
        &self,
        frame_sequence: u32,
        produced_at: DeviceCycle,
        observed_at: DeviceCycle,
        report: RealtimeClockReport,
    ) -> Result<(), RealtimeClockObservationError> {
        if frame_sequence == 0 {
            return Err(RealtimeClockObservationError::Sequence);
        }
        let age = observed_at
            .0
            .checked_sub(produced_at.0)
            .ok_or(RealtimeClockObservationError::Future)?;
        if age > REALTIME_REPORT_MAX_AGE_CYCLES {
            return Err(RealtimeClockObservationError::Expired);
        }
        if let Some(previous) = self.realtime {
            if !serial_is_newer(frame_sequence, previous.frame_sequence) {
                return Err(RealtimeClockObservationError::Sequence);
            }
            if report.samples <= previous.report.samples
                || report.missed_deadlines < previous.report.missed_deadlines
                || report.maximum_lateness_cycles < previous.report.maximum_lateness_cycles
            {
                return Err(RealtimeClockObservationError::Counters);
            }
        }
        Ok(())
    }

    fn fresh_realtime(&self, now: DeviceCycle) -> Option<RealtimeClockReport> {
        let observed = self.realtime?;
        let age = now.0.checked_sub(observed.produced_at.0)?;
        (age <= REALTIME_REPORT_MAX_AGE_CYCLES).then_some(observed.report)
    }
}

const fn serial_is_newer(candidate: u32, previous: u32) -> bool {
    let distance = candidate.wrapping_sub(previous);
    distance != 0 && distance < (1_u32 << 31)
}
