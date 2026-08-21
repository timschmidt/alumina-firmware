//! Reloadable core-local graph actors around one generation-bound bridge.

use core::cell::RefCell;
use core::mem::size_of;

use alumina_board::ResourceId;
use alumina_graph_ir::{GraphIrPackage, graph_ir_content_digest};
use alumina_protocol::{DeviceCycle, Digest};
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::LatestSignal;

use super::{
    BridgeArena, DomainCursor, GraphDeploymentFault, GraphDeploymentIdentity, GraphExecutionError,
    GraphExecutionFault, GraphFaultObservation, GraphInstallReport, GraphReleaseReport,
    GraphReleaseWindow, GraphRuntimeArena, GraphRuntimeAuthority, GraphRuntimeError,
    GraphRuntimeLimits, GraphRuntimeMetadata, GraphStartReport, QueueCursor, admit_package,
    check_capacity, checked_next_cycle, checked_next_tick, cursor_after_prime, cursor_at_start,
    cursor_release_window, execute_realtime_release, execute_service_release, release_prelude,
};

/// Exact boot-local identity for one execution of one selected package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphRunIdentity {
    /// Graph installation transaction selected by both cores.
    pub transaction_id: u64,
    /// Nonzero boot-local run identity; one installed graph may run repeatedly.
    pub run_id: u64,
    /// SHA-256 over all stored package bytes.
    pub content_digest: Digest,
    /// Embedded canonical package digest.
    pub package_digest: Digest,
    /// Exact future device cycle used as release tick zero.
    pub start_cycle: DeviceCycle,
}

impl GraphRunIdentity {
    /// Reject an incomplete run identity before it may reserve the bridge.
    pub fn validate(self) -> Result<(), GraphLiveError> {
        if self.transaction_id == 0
            || self.run_id == 0
            || self.content_digest.is_zero()
            || self.package_digest.is_zero()
            || self.start_cycle.0 == 0
        {
            Err(GraphLiveError::RunIdentity)
        } else {
            Ok(())
        }
    }
}

/// Stable lifecycle of one permanent core-local actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphActorPhase {
    /// No package is retained.
    Empty,
    /// One independently admitted package is ready for another run.
    Installed,
    /// This actor is prepared for the exact run but has not joined execution.
    Prepared,
    /// Exact releases are admitted for this run.
    Running,
    /// This actor or its peer latched a terminal execution fault.
    Faulted,
}

impl GraphActorPhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Installed),
            2 => Some(Self::Prepared),
            3 => Some(Self::Running),
            4 => Some(Self::Faulted),
            _ => None,
        }
    }
}

/// Shared bridge lifecycle, advanced only by explicit actor handshakes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphBridgePhase {
    /// Both prior actors acknowledged stop; no run owns bridge bytes.
    Empty,
    /// Service owns reset bridge bytes while performing source-first tick zero.
    Priming,
    /// Tick zero completed; Realtime may bind the same exact run.
    Primed,
    /// Both domains may execute their exact releases.
    Running,
    /// No new release may begin while both actors acknowledge stop.
    Stopping,
}

impl GraphBridgePhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Priming),
            2 => Some(Self::Primed),
            3 => Some(Self::Running),
            4 => Some(Self::Stopping),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct GraphBridgeControl {
    phase: GraphBridgePhase,
    run: Option<GraphRunIdentity>,
    service_package: Option<GraphBridgePackageIdentity>,
    realtime_package: Option<GraphBridgePackageIdentity>,
    service_stopped: bool,
    realtime_stopped: bool,
}

impl GraphBridgeControl {
    const EMPTY: Self = Self {
        phase: GraphBridgePhase::Empty,
        run: None,
        service_package: None,
        realtime_package: None,
        service_stopped: false,
        realtime_stopped: false,
    };

    fn finish_stop(&mut self) {
        self.phase = GraphBridgePhase::Empty;
        self.run = None;
        self.service_stopped = false;
        self.realtime_stopped = false;
    }
}

#[derive(Clone, Copy, Eq, PartialEq)]
struct GraphBridgePackageIdentity {
    transaction_id: u64,
    content_digest: Digest,
    package_digest: Digest,
}

impl GraphBridgePackageIdentity {
    const fn from_deployment(identity: GraphDeploymentIdentity) -> Self {
        Self {
            transaction_id: identity.transaction_id,
            content_digest: identity.content_digest,
            package_digest: identity.package_digest,
        }
    }

    const fn from_run(run: GraphRunIdentity) -> Self {
        Self {
            transaction_id: run.transaction_id,
            content_digest: run.content_digest,
            package_digest: run.package_digest,
        }
    }
}

#[derive(Clone, Copy)]
enum GraphBridgeSide {
    Service,
    Realtime,
}

/// Permanently allocated, repeatedly reusable Service-to-Realtime bridge.
///
/// Package and core-local arenas remain owned by their respective actors. Only
/// the bounded bridge bytes, queue cursors, run handshake, and first-cause
/// mailbox cross cores. A new run can clear them only after both actors have
/// entered `Stopping` and acknowledged that no release remains in progress.
pub struct ReloadableGraphBridge<const BYTES: usize> {
    control: BlockingMutex<CriticalSectionRawMutex, RefCell<GraphBridgeControl>>,
    arena: BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BYTES>>>,
    fault: LatestSignal,
}

impl<const BYTES: usize> ReloadableGraphBridge<BYTES> {
    /// Construct an empty bridge suitable for static allocation before either
    /// application core starts.
    pub const fn new() -> Self {
        Self {
            control: BlockingMutex::new(RefCell::new(GraphBridgeControl::EMPTY)),
            arena: BlockingMutex::new(RefCell::new(BridgeArena::new())),
            fault: LatestSignal::new(),
        }
    }

    /// Complete bytes reserved by this concrete bridge type.
    pub const fn static_bridge_bytes() -> usize {
        size_of::<Self>()
    }

    /// Current cross-core execution phase.
    pub fn phase(&self) -> GraphBridgePhase {
        self.control.lock(|cell| cell.borrow().phase)
    }

    /// Current exact run, including while stopping.
    pub fn run_identity(&self) -> Option<GraphRunIdentity> {
        self.control.lock(|cell| cell.borrow().run)
    }

    /// Newest first-cause fault for the current run.
    pub fn fault_after(&self, generation: u16) -> Option<GraphFaultObservation> {
        self.fault
            .after(generation)
            .map(GraphFaultObservation::from_signal)
    }

    fn select_package(
        &self,
        side: GraphBridgeSide,
        identity: GraphDeploymentIdentity,
    ) -> Result<(), GraphLiveError> {
        self.control.lock(|cell| {
            let mut control = cell.borrow_mut();
            if control.phase != GraphBridgePhase::Empty || control.run.is_some() {
                return Err(GraphLiveError::BridgePhase {
                    required: GraphBridgePhase::Empty,
                    actual: control.phase,
                });
            }
            let slot = match side {
                GraphBridgeSide::Service => &mut control.service_package,
                GraphBridgeSide::Realtime => &mut control.realtime_package,
            };
            if slot.is_some() {
                return Err(GraphLiveError::BridgeSelection);
            }
            *slot = Some(GraphBridgePackageIdentity::from_deployment(identity));
            Ok(())
        })
    }

    fn clear_package(
        &self,
        side: GraphBridgeSide,
        identity: GraphDeploymentIdentity,
    ) -> Result<(), GraphLiveError> {
        self.control.lock(|cell| {
            let mut control = cell.borrow_mut();
            if control.phase != GraphBridgePhase::Empty || control.run.is_some() {
                return Err(GraphLiveError::BridgePhase {
                    required: GraphBridgePhase::Empty,
                    actual: control.phase,
                });
            }
            let slot = match side {
                GraphBridgeSide::Service => &mut control.service_package,
                GraphBridgeSide::Realtime => &mut control.realtime_package,
            };
            if *slot != Some(GraphBridgePackageIdentity::from_deployment(identity)) {
                return Err(GraphLiveError::BridgeSelection);
            }
            *slot = None;
            Ok(())
        })
    }

    fn begin_prime(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        run.validate()?;
        self.control.lock(|cell| {
            let mut control = cell.borrow_mut();
            if control.phase != GraphBridgePhase::Empty || control.run.is_some() {
                return Err(GraphLiveError::BridgePhase {
                    required: GraphBridgePhase::Empty,
                    actual: control.phase,
                });
            }
            let package = Some(GraphBridgePackageIdentity::from_run(run));
            if control.service_package != package || control.realtime_package != package {
                return Err(GraphLiveError::BridgeSelection);
            }
            control.phase = GraphBridgePhase::Priming;
            control.run = Some(run);
            control.service_stopped = false;
            control.realtime_stopped = false;
            Ok(())
        })?;
        self.arena.lock(|cell| {
            let mut arena = cell.borrow_mut();
            arena.bytes.fill(0);
            arena.queues.fill(QueueCursor::EMPTY);
        });
        self.fault.reset_session();
        Ok(())
    }

    fn finish_prime(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        if let Some(observation) = self.fault_after(0) {
            return Err(GraphLiveError::Execution(GraphExecutionError {
                observation,
                expected_cycle: None,
                received_cycle: None,
            }));
        }
        self.transition(run, GraphBridgePhase::Priming, GraphBridgePhase::Primed)
    }

    fn activate(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        self.transition(run, GraphBridgePhase::Primed, GraphBridgePhase::Running)
    }

    fn require_running(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        self.require(run, GraphBridgePhase::Running)
    }

    fn require_primed(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        self.require(run, GraphBridgePhase::Primed)
    }

    fn transition(
        &self,
        run: GraphRunIdentity,
        required: GraphBridgePhase,
        next: GraphBridgePhase,
    ) -> Result<(), GraphLiveError> {
        self.control.lock(|cell| {
            let mut control = cell.borrow_mut();
            if control.run != Some(run) {
                return Err(GraphLiveError::BridgeIdentity);
            }
            if control.phase != required {
                return Err(GraphLiveError::BridgePhase {
                    required,
                    actual: control.phase,
                });
            }
            control.phase = next;
            Ok(())
        })
    }

    fn require(
        &self,
        run: GraphRunIdentity,
        required: GraphBridgePhase,
    ) -> Result<(), GraphLiveError> {
        self.control.lock(|cell| {
            let control = cell.borrow();
            if control.run != Some(run) {
                return Err(GraphLiveError::BridgeIdentity);
            }
            if control.phase != required {
                return Err(GraphLiveError::BridgePhase {
                    required,
                    actual: control.phase,
                });
            }
            Ok(())
        })
    }

    fn request_stop(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        self.control.lock(|cell| {
            let mut control = cell.borrow_mut();
            if control.run != Some(run) {
                return Err(GraphLiveError::BridgeIdentity);
            }
            if control.phase == GraphBridgePhase::Empty {
                return Err(GraphLiveError::BridgePhase {
                    required: GraphBridgePhase::Stopping,
                    actual: GraphBridgePhase::Empty,
                });
            }
            control.phase = GraphBridgePhase::Stopping;
            Ok(())
        })
    }

    fn acknowledge_stop(
        &self,
        run: GraphRunIdentity,
        side: GraphBridgeSide,
    ) -> Result<bool, GraphLiveError> {
        self.control.lock(|cell| {
            let mut control = cell.borrow_mut();
            if control.run != Some(run) {
                return Err(GraphLiveError::BridgeIdentity);
            }
            if control.phase != GraphBridgePhase::Stopping {
                return Err(GraphLiveError::BridgePhase {
                    required: GraphBridgePhase::Stopping,
                    actual: control.phase,
                });
            }
            match side {
                GraphBridgeSide::Service => control.service_stopped = true,
                GraphBridgeSide::Realtime => control.realtime_stopped = true,
            }
            let empty = control.service_stopped && control.realtime_stopped;
            if empty {
                control.finish_stop();
            }
            Ok(empty)
        })
    }
}

impl<const BYTES: usize> Default for ReloadableGraphBridge<BYTES> {
    fn default() -> Self {
        Self::new()
    }
}

/// Successful stop acknowledgement from one core-local actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphStopReport {
    /// Exact run that no longer has work in this actor.
    pub run: GraphRunIdentity,
    /// Both actors have acknowledged and the bridge is reusable.
    pub bridge_empty: bool,
}

/// Exact bytes in one core-1 graph-execution telemetry report.
pub const REALTIME_GRAPH_EXECUTION_REPORT_BYTES: usize = 64;
/// Exact bytes appended to the authenticated combined graph status.
pub const GRAPH_EXECUTION_REPORT_BYTES: usize = 64;

const REALTIME_GRAPH_EXECUTION_MAGIC: [u8; 4] = *b"ALXR";
const GRAPH_EXECUTION_REPORT_MAGIC: [u8; 4] = *b"ALXS";
const GRAPH_EXECUTION_REPORT_VERSION: u16 = 1;
const EXECUTION_FLAG_SERVICE_NEXT: u8 = 1 << 0;
const EXECUTION_FLAG_REALTIME_NEXT: u8 = 1 << 1;
const EXECUTION_FLAG_SERVICE_LAST: u8 = 1 << 2;
const EXECUTION_FLAG_REALTIME_LAST: u8 = 1 << 3;
const EXECUTION_FLAG_FAULT: u8 = 1 << 4;
const EXECUTION_KNOWN_FLAGS: u8 = EXECUTION_FLAG_SERVICE_NEXT
    | EXECUTION_FLAG_REALTIME_NEXT
    | EXECUTION_FLAG_SERVICE_LAST
    | EXECUTION_FLAG_REALTIME_LAST
    | EXECUTION_FLAG_FAULT;
const REALTIME_FLAG_NEXT: u8 = 1 << 0;
const REALTIME_FLAG_LAST: u8 = 1 << 1;
const REALTIME_FLAG_FAULT: u8 = 1 << 2;
const REALTIME_KNOWN_FLAGS: u8 = REALTIME_FLAG_NEXT | REALTIME_FLAG_LAST | REALTIME_FLAG_FAULT;

/// Canonical core-1 graph actor observation crossing the bounded telemetry queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeGraphExecutionReport {
    /// Core-1 permanent actor phase.
    pub actor_phase: GraphActorPhase,
    /// Shared bridge phase observed by core 1.
    pub bridge_phase: GraphBridgePhase,
    /// Active graph installation transaction, or zero with no actor package.
    pub transaction_id: u64,
    /// Exact boot-local run, or zero when no run is being reconciled.
    pub run_id: u64,
    /// Exact release tick-zero cycle, or zero when `run_id` is zero.
    pub start_cycle: DeviceCycle,
    /// Exact next Realtime release, absent for an absent domain or stopped actor.
    pub next_release_cycle: Option<DeviceCycle>,
    /// Last completely executed Realtime release tick.
    pub last_release_tick: Option<u64>,
    /// Shared first-cause fault, when latched.
    pub fault: Option<GraphFaultObservation>,
}

impl RealtimeGraphExecutionReport {
    /// Canonical boot state before any package is selected.
    pub const fn empty() -> Self {
        Self {
            actor_phase: GraphActorPhase::Empty,
            bridge_phase: GraphBridgePhase::Empty,
            transaction_id: 0,
            run_id: 0,
            start_cycle: DeviceCycle(0),
            next_release_cycle: None,
            last_release_tick: None,
            fault: None,
        }
    }

    /// Encode one exact fixed telemetry payload.
    pub fn encode(
        self,
    ) -> Result<[u8; REALTIME_GRAPH_EXECUTION_REPORT_BYTES], GraphExecutionReportError> {
        self.validate()?;
        let mut encoded = [0_u8; REALTIME_GRAPH_EXECUTION_REPORT_BYTES];
        encoded[..4].copy_from_slice(&REALTIME_GRAPH_EXECUTION_MAGIC);
        encoded[4..6].copy_from_slice(&GRAPH_EXECUTION_REPORT_VERSION.to_le_bytes());
        encoded[6] = self.actor_phase as u8;
        encoded[7] = self.bridge_phase as u8;
        encoded[8] = (u8::from(self.next_release_cycle.is_some()) * REALTIME_FLAG_NEXT)
            | (u8::from(self.last_release_tick.is_some()) * REALTIME_FLAG_LAST)
            | (u8::from(self.fault.is_some()) * REALTIME_FLAG_FAULT);
        if let Some(fault) = self.fault {
            encoded[9] = fault.fault as u8;
            encoded[10] = fault.detail;
            encoded[12..14].copy_from_slice(&fault.generation.to_le_bytes());
        }
        // Bytes 11, 14..16, and 56..64 are reserved zero.
        encoded[16..24].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.run_id.to_le_bytes());
        encoded[32..40].copy_from_slice(&self.start_cycle.0.to_le_bytes());
        if let Some(cycle) = self.next_release_cycle {
            encoded[40..48].copy_from_slice(&cycle.0.to_le_bytes());
        }
        if let Some(tick) = self.last_release_tick {
            encoded[48..56].copy_from_slice(&tick.to_le_bytes());
        }
        Ok(encoded)
    }

    /// Decode only the exact canonical core-1 report.
    pub fn decode(encoded: &[u8]) -> Result<Self, GraphExecutionReportError> {
        if encoded.len() != REALTIME_GRAPH_EXECUTION_REPORT_BYTES {
            return Err(GraphExecutionReportError::Length);
        }
        if encoded[..4] != REALTIME_GRAPH_EXECUTION_MAGIC {
            return Err(GraphExecutionReportError::Magic);
        }
        if read_u16(encoded, 4) != GRAPH_EXECUTION_REPORT_VERSION {
            return Err(GraphExecutionReportError::Version);
        }
        let flags = encoded[8];
        if flags & !REALTIME_KNOWN_FLAGS != 0
            || encoded[11] != 0
            || encoded[14..16].iter().any(|byte| *byte != 0)
            || encoded[56..].iter().any(|byte| *byte != 0)
        {
            return Err(GraphExecutionReportError::Reserved);
        }
        let report = Self {
            actor_phase: GraphActorPhase::from_wire(encoded[6])
                .ok_or(GraphExecutionReportError::ActorPhase)?,
            bridge_phase: GraphBridgePhase::from_wire(encoded[7])
                .ok_or(GraphExecutionReportError::BridgePhase)?,
            transaction_id: read_u64(encoded, 16),
            run_id: read_u64(encoded, 24),
            start_cycle: DeviceCycle(read_u64(encoded, 32)),
            next_release_cycle: (flags & REALTIME_FLAG_NEXT != 0)
                .then(|| DeviceCycle(read_u64(encoded, 40))),
            last_release_tick: (flags & REALTIME_FLAG_LAST != 0).then(|| read_u64(encoded, 48)),
            fault: decode_fault(encoded, flags & REALTIME_FLAG_FAULT != 0, 9, 10)?,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(GraphExecutionReportError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), GraphExecutionReportError> {
        validate_run_shape(self.run_id, self.start_cycle)?;
        if self.actor_phase == GraphActorPhase::Empty {
            if self != Self::empty() {
                return Err(GraphExecutionReportError::StateShape);
            }
            return Ok(());
        }
        if self.transaction_id == 0 {
            return Err(GraphExecutionReportError::Identity);
        }
        if self.run_id == 0
            && (self.actor_phase != GraphActorPhase::Installed
                || self.bridge_phase != GraphBridgePhase::Empty
                || self.next_release_cycle.is_some()
                || self.last_release_tick.is_some()
                || self.fault.is_some())
        {
            return Err(GraphExecutionReportError::StateShape);
        }
        if matches!(
            self.actor_phase,
            GraphActorPhase::Prepared | GraphActorPhase::Running | GraphActorPhase::Faulted
        ) && self.run_id == 0
        {
            return Err(GraphExecutionReportError::StateShape);
        }
        if self.actor_phase == GraphActorPhase::Prepared
            && !matches!(
                self.bridge_phase,
                GraphBridgePhase::Primed | GraphBridgePhase::Stopping
            )
            || self.actor_phase == GraphActorPhase::Running
                && !matches!(
                    self.bridge_phase,
                    GraphBridgePhase::Running | GraphBridgePhase::Stopping
                )
            || self.actor_phase == GraphActorPhase::Faulted && self.fault.is_none()
        {
            return Err(GraphExecutionReportError::StateShape);
        }
        Ok(())
    }
}

/// Canonical combined service/core-1 graph execution observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphExecutionReport {
    /// Core-0 permanent actor phase.
    pub service_phase: GraphActorPhase,
    /// Latest accepted core-1 actor phase.
    pub realtime_phase: GraphActorPhase,
    /// Shared bridge phase observed by core 0.
    pub bridge_phase: GraphBridgePhase,
    /// Exact boot-local run, or zero when idle.
    pub run_id: u64,
    /// Exact release tick-zero cycle, or zero when idle.
    pub start_cycle: DeviceCycle,
    /// Exact next core-0 release.
    pub service_next_cycle: Option<DeviceCycle>,
    /// Exact next core-1 release.
    pub realtime_next_cycle: Option<DeviceCycle>,
    /// Last completely executed core-0 release tick.
    pub service_last_tick: Option<u64>,
    /// Last completely executed core-1 release tick.
    pub realtime_last_tick: Option<u64>,
    /// Shared first-cause execution fault.
    pub fault: Option<GraphFaultObservation>,
}

impl GraphExecutionReport {
    /// Canonical state before either actor owns a package.
    pub const fn empty() -> Self {
        Self {
            service_phase: GraphActorPhase::Empty,
            realtime_phase: GraphActorPhase::Empty,
            bridge_phase: GraphBridgePhase::Empty,
            run_id: 0,
            start_cycle: DeviceCycle(0),
            service_next_cycle: None,
            realtime_next_cycle: None,
            service_last_tick: None,
            realtime_last_tick: None,
            fault: None,
        }
    }

    /// Encode one exact authenticated status suffix.
    pub fn encode(self) -> Result<[u8; GRAPH_EXECUTION_REPORT_BYTES], GraphExecutionReportError> {
        self.validate()?;
        let mut encoded = [0_u8; GRAPH_EXECUTION_REPORT_BYTES];
        encoded[..4].copy_from_slice(&GRAPH_EXECUTION_REPORT_MAGIC);
        encoded[4..6].copy_from_slice(&GRAPH_EXECUTION_REPORT_VERSION.to_le_bytes());
        encoded[6] = self.service_phase as u8;
        encoded[7] = self.realtime_phase as u8;
        encoded[8] = self.bridge_phase as u8;
        encoded[9] = (u8::from(self.service_next_cycle.is_some()) * EXECUTION_FLAG_SERVICE_NEXT)
            | (u8::from(self.realtime_next_cycle.is_some()) * EXECUTION_FLAG_REALTIME_NEXT)
            | (u8::from(self.service_last_tick.is_some()) * EXECUTION_FLAG_SERVICE_LAST)
            | (u8::from(self.realtime_last_tick.is_some()) * EXECUTION_FLAG_REALTIME_LAST)
            | (u8::from(self.fault.is_some()) * EXECUTION_FLAG_FAULT);
        if let Some(fault) = self.fault {
            encoded[10] = fault.fault as u8;
            encoded[11] = fault.detail;
            encoded[12..14].copy_from_slice(&fault.generation.to_le_bytes());
        }
        // Bytes 14..16 are reserved zero.
        encoded[16..24].copy_from_slice(&self.run_id.to_le_bytes());
        encoded[24..32].copy_from_slice(&self.start_cycle.0.to_le_bytes());
        if let Some(cycle) = self.service_next_cycle {
            encoded[32..40].copy_from_slice(&cycle.0.to_le_bytes());
        }
        if let Some(cycle) = self.realtime_next_cycle {
            encoded[40..48].copy_from_slice(&cycle.0.to_le_bytes());
        }
        if let Some(tick) = self.service_last_tick {
            encoded[48..56].copy_from_slice(&tick.to_le_bytes());
        }
        if let Some(tick) = self.realtime_last_tick {
            encoded[56..64].copy_from_slice(&tick.to_le_bytes());
        }
        Ok(encoded)
    }

    /// Decode only the exact canonical combined execution report.
    pub fn decode(encoded: &[u8]) -> Result<Self, GraphExecutionReportError> {
        if encoded.len() != GRAPH_EXECUTION_REPORT_BYTES {
            return Err(GraphExecutionReportError::Length);
        }
        if encoded[..4] != GRAPH_EXECUTION_REPORT_MAGIC {
            return Err(GraphExecutionReportError::Magic);
        }
        if read_u16(encoded, 4) != GRAPH_EXECUTION_REPORT_VERSION {
            return Err(GraphExecutionReportError::Version);
        }
        let flags = encoded[9];
        if flags & !EXECUTION_KNOWN_FLAGS != 0 || encoded[14..16].iter().any(|byte| *byte != 0) {
            return Err(GraphExecutionReportError::Reserved);
        }
        let report = Self {
            service_phase: GraphActorPhase::from_wire(encoded[6])
                .ok_or(GraphExecutionReportError::ActorPhase)?,
            realtime_phase: GraphActorPhase::from_wire(encoded[7])
                .ok_or(GraphExecutionReportError::ActorPhase)?,
            bridge_phase: GraphBridgePhase::from_wire(encoded[8])
                .ok_or(GraphExecutionReportError::BridgePhase)?,
            run_id: read_u64(encoded, 16),
            start_cycle: DeviceCycle(read_u64(encoded, 24)),
            service_next_cycle: (flags & EXECUTION_FLAG_SERVICE_NEXT != 0)
                .then(|| DeviceCycle(read_u64(encoded, 32))),
            realtime_next_cycle: (flags & EXECUTION_FLAG_REALTIME_NEXT != 0)
                .then(|| DeviceCycle(read_u64(encoded, 40))),
            service_last_tick: (flags & EXECUTION_FLAG_SERVICE_LAST != 0)
                .then(|| read_u64(encoded, 48)),
            realtime_last_tick: (flags & EXECUTION_FLAG_REALTIME_LAST != 0)
                .then(|| read_u64(encoded, 56)),
            fault: decode_fault(encoded, flags & EXECUTION_FLAG_FAULT != 0, 10, 11)?,
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(GraphExecutionReportError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), GraphExecutionReportError> {
        validate_run_shape(self.run_id, self.start_cycle)?;
        if self.run_id == 0
            && (self.service_next_cycle.is_some()
                || self.realtime_next_cycle.is_some()
                || self.service_last_tick.is_some()
                || self.realtime_last_tick.is_some()
                || self.fault.is_some()
                || self.bridge_phase != GraphBridgePhase::Empty)
        {
            return Err(GraphExecutionReportError::StateShape);
        }
        if self.service_phase == GraphActorPhase::Empty
            && self.realtime_phase != GraphActorPhase::Empty
            || self.realtime_phase == GraphActorPhase::Empty
                && self.service_phase != GraphActorPhase::Empty
            || self.service_phase == GraphActorPhase::Faulted && self.fault.is_none()
            || self.realtime_phase == GraphActorPhase::Faulted && self.fault.is_none()
        {
            return Err(GraphExecutionReportError::StateShape);
        }
        Ok(())
    }
}

/// Canonical graph execution report rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphExecutionReportError {
    /// Fixed report length differed.
    Length,
    /// Report magic differed.
    Magic,
    /// Report schema version differed.
    Version,
    /// A reserved or unknown flag byte was nonzero.
    Reserved,
    /// Actor phase byte was unknown.
    ActorPhase,
    /// Bridge phase byte was unknown.
    BridgePhase,
    /// Run or transaction identity was incomplete.
    Identity,
    /// Fault code or generation was incomplete.
    Fault,
    /// Phase, run, release, bridge, or fault facts contradicted one another.
    StateShape,
    /// Decode followed by encode changed the bytes.
    Noncanonical,
}

fn validate_run_shape(
    run_id: u64,
    start_cycle: DeviceCycle,
) -> Result<(), GraphExecutionReportError> {
    if (run_id == 0) != (start_cycle.0 == 0) {
        Err(GraphExecutionReportError::Identity)
    } else {
        Ok(())
    }
}

fn decode_fault(
    encoded: &[u8],
    present: bool,
    code_offset: usize,
    detail_offset: usize,
) -> Result<Option<GraphFaultObservation>, GraphExecutionReportError> {
    if !present {
        if encoded[code_offset..14].iter().any(|byte| *byte != 0) {
            return Err(GraphExecutionReportError::Reserved);
        }
        return Ok(None);
    }
    let generation = read_u16(encoded, 12);
    let fault = GraphExecutionFault::from_wire(encoded[code_offset])
        .ok_or(GraphExecutionReportError::Fault)?;
    if generation == 0 {
        return Err(GraphExecutionReportError::Fault);
    }
    Ok(Some(GraphFaultObservation {
        generation,
        fault,
        detail: encoded[detail_offset],
    }))
}

fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([encoded[offset], encoded[offset + 1]])
}

fn read_u64(encoded: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
        encoded[offset + 4],
        encoded[offset + 5],
        encoded[offset + 6],
        encoded[offset + 7],
    ])
}

/// Permanent core-0 graph owner supporting repeated install/run/stop cycles.
pub struct FixedGraphServiceActor<
    'a,
    const STATE: usize,
    const CHANNELS: usize,
    const BRIDGE: usize,
> {
    bridge: &'a ReloadableGraphBridge<BRIDGE>,
    phase: GraphActorPhase,
    identity: Option<GraphDeploymentIdentity>,
    package: Option<GraphIrPackage>,
    metadata: GraphRuntimeMetadata,
    run: Option<GraphRunIdentity>,
    start_cycle: u64,
    cursor: DomainCursor,
    state: [u8; STATE],
    channels: [u8; CHANNELS],
    queues: [QueueCursor; alumina_graph_ir::MAX_GRAPH_IR_CHANNELS],
}

impl<'a, const STATE: usize, const CHANNELS: usize, const BRIDGE: usize>
    FixedGraphServiceActor<'a, STATE, CHANNELS, BRIDGE>
{
    /// Bind permanent core-local arrays to the one shared bridge.
    pub const fn new(bridge: &'a ReloadableGraphBridge<BRIDGE>) -> Self {
        Self {
            bridge,
            phase: GraphActorPhase::Empty,
            identity: None,
            package: None,
            metadata: GraphRuntimeMetadata::EMPTY,
            run: None,
            start_cycle: 0,
            cursor: DomainCursor::EMPTY,
            state: [0; STATE],
            channels: [0; CHANNELS],
            queues: [QueueCursor::EMPTY; alumina_graph_ir::MAX_GRAPH_IR_CHANNELS],
        }
    }

    /// Complete bytes reserved by this concrete actor, excluding the bridge.
    pub const fn static_actor_bytes() -> usize {
        size_of::<Self>()
    }

    /// Current core-local lifecycle phase.
    pub const fn phase(&self) -> GraphActorPhase {
        self.phase
    }

    /// Independently admitted package identity.
    pub const fn installed_identity(&self) -> Option<GraphDeploymentIdentity> {
        self.identity
    }

    /// Exact active run for this actor.
    pub const fn run_identity(&self) -> Option<GraphRunIdentity> {
        self.run
    }

    /// Independently admit one exact package into core-0-owned storage.
    #[allow(clippy::too_many_arguments)]
    pub fn install(
        &mut self,
        bytes: &[u8],
        transaction_id: u64,
        content_digest: Digest,
        package_digest: Digest,
        authority: GraphRuntimeAuthority,
        limits: GraphRuntimeLimits,
        mutation_allowed: bool,
    ) -> Result<GraphInstallReport, GraphLiveError> {
        if !mutation_allowed {
            return Err(GraphLiveError::MutationForbidden);
        }
        self.require_phase(GraphActorPhase::Empty)?;
        if transaction_id == 0 || content_digest.is_zero() {
            return Err(GraphLiveError::Deployment(GraphDeploymentFault::Sequence));
        }
        if graph_ir_content_digest(bytes) != content_digest {
            return Err(GraphLiveError::Deployment(
                GraphDeploymentFault::ContentDigest,
            ));
        }
        let (package, metadata, usage) = admit_package(bytes, package_digest, authority, limits)
            .map_err(GraphDeploymentFault::from_runtime)
            .map_err(GraphLiveError::Deployment)?;
        let identity = GraphDeploymentIdentity {
            transaction_id,
            content_digest,
            package_digest,
            implementation_digest: authority.implementation_digest,
            graph_digest: package.header().graph_digest,
            summary: package.summary(),
            usage,
        };
        check_capacity(
            GraphRuntimeArena::ServiceState,
            identity.usage.service_state_bytes,
            STATE,
        )
        .map_err(GraphLiveError::Runtime)?;
        check_capacity(
            GraphRuntimeArena::ServiceChannels,
            identity.usage.service_channel_bytes,
            CHANNELS,
        )
        .map_err(GraphLiveError::Runtime)?;
        check_capacity(
            GraphRuntimeArena::ServiceToRealtime,
            identity.usage.bridge_channel_bytes,
            BRIDGE,
        )
        .map_err(GraphLiveError::Runtime)?;
        self.bridge
            .select_package(GraphBridgeSide::Service, identity)?;
        self.reset_local();
        self.identity = Some(identity);
        self.package = Some(package);
        self.metadata = metadata;
        self.phase = GraphActorPhase::Installed;
        Ok(install_report::<Self>(identity))
    }

    /// Reset the shared bridge and run Service tick zero for one future epoch.
    pub fn prepare_start(
        &mut self,
        run: GraphRunIdentity,
        execution_allowed: bool,
    ) -> Result<GraphStartReport, GraphLiveError> {
        if !execution_allowed {
            return Err(GraphLiveError::ExecutionForbidden);
        }
        self.require_phase(GraphActorPhase::Installed)?;
        self.require_run_matches(run)?;
        self.reset_local();
        let package = self.package.as_ref().ok_or(GraphLiveError::Internal)?;
        let cursor = cursor_after_prime(package.header().service_schedule, run.start_cycle)
            .map_err(GraphLiveError::Runtime)?;
        self.bridge.begin_prime(run)?;
        let primed_service_release = if package.header().service_schedule.node_count == 0 {
            None
        } else {
            match execute_service_release(
                package,
                &self.metadata,
                &mut self.state,
                &mut self.channels,
                &mut self.queues,
                &self.bridge.arena,
                &self.bridge.fault,
                run.start_cycle.0,
                run.start_cycle,
                0,
            ) {
                Ok(report) => Some(report),
                Err(error) => {
                    self.phase = GraphActorPhase::Faulted;
                    self.run = Some(run);
                    return Err(GraphLiveError::Execution(error));
                }
            }
        };
        self.bridge.finish_prime(run)?;
        self.start_cycle = run.start_cycle.0;
        self.cursor = cursor;
        self.run = Some(run);
        self.phase = GraphActorPhase::Prepared;
        Ok(GraphStartReport {
            package_digest: run.package_digest,
            start_cycle: run.start_cycle,
            primed_service_release,
        })
    }

    /// Accept only the matching Realtime actor's transition to Running.
    pub fn observe_realtime_started(
        &mut self,
        run: GraphRunIdentity,
    ) -> Result<(), GraphLiveError> {
        self.require_phase(GraphActorPhase::Prepared)?;
        if self.run != Some(run) {
            return Err(GraphLiveError::RunIdentity);
        }
        self.bridge.require_running(run)?;
        self.phase = GraphActorPhase::Running;
        Ok(())
    }

    /// Execute the exact next Service release without waiting or allocation.
    pub fn release(
        &mut self,
        cycle: DeviceCycle,
        safety_authorized: bool,
    ) -> Result<GraphReleaseReport, GraphLiveError> {
        self.require_phase(GraphActorPhase::Running)?;
        let run = self.run.ok_or(GraphLiveError::RunIdentity)?;
        self.bridge.require_running(run)?;
        let package = self.package.as_ref().ok_or(GraphLiveError::Internal)?;
        let result = (|| {
            release_prelude(
                &self.bridge.fault,
                &self.cursor,
                cycle,
                safety_authorized,
                0,
            )?;
            let next_cycle = checked_next_cycle(&self.bridge.fault, &self.cursor, 0)?;
            let next_tick = checked_next_tick(&self.bridge.fault, &self.cursor, 0)?;
            let report = execute_service_release(
                package,
                &self.metadata,
                &mut self.state,
                &mut self.channels,
                &mut self.queues,
                &self.bridge.arena,
                &self.bridge.fault,
                self.start_cycle,
                cycle,
                self.cursor.next_tick,
            )?;
            self.cursor.next_cycle = next_cycle;
            self.cursor.next_tick = next_tick;
            Ok(report)
        })();
        match result {
            Ok(report) => Ok(report),
            Err(error) => {
                self.phase = GraphActorPhase::Faulted;
                Err(GraphLiveError::Execution(error))
            }
        }
    }

    /// Next exact release cycle while prepared or running.
    pub fn next_release_cycle(&self) -> Option<DeviceCycle> {
        if matches!(
            self.phase,
            GraphActorPhase::Prepared | GraphActorPhase::Running
        ) && self.cursor.present
        {
            Some(DeviceCycle(self.cursor.next_cycle))
        } else {
            None
        }
    }

    /// Exact next release and package-declared latest dispatch boundary.
    pub fn next_release_window(&self) -> Result<Option<GraphReleaseWindow>, GraphLiveError> {
        if !matches!(
            self.phase,
            GraphActorPhase::Prepared | GraphActorPhase::Running
        ) {
            return Ok(None);
        }
        cursor_release_window(&self.cursor).map_err(GraphLiveError::Runtime)
    }

    /// Current shared bridge phase observed from core 0.
    pub fn bridge_phase(&self) -> GraphBridgePhase {
        self.bridge.phase()
    }

    /// Newest shared first-cause execution fault.
    pub fn fault_after(&self, generation: u16) -> Option<GraphFaultObservation> {
        self.bridge.fault_after(generation)
    }

    /// Stop new releases and acknowledge that core 0 owns no in-progress work.
    pub fn stop(&mut self, run: GraphRunIdentity) -> Result<GraphStopReport, GraphLiveError> {
        self.require_stoppable(run)?;
        self.bridge.request_stop(run)?;
        let bridge_empty = self
            .bridge
            .acknowledge_stop(run, GraphBridgeSide::Service)?;
        self.run = None;
        self.cursor = DomainCursor::EMPTY;
        self.phase = GraphActorPhase::Installed;
        Ok(GraphStopReport { run, bridge_empty })
    }

    /// Remove the installed package only after both actors stopped.
    pub fn clear(&mut self, mutation_allowed: bool) -> Result<(), GraphLiveError> {
        if !mutation_allowed {
            return Err(GraphLiveError::MutationForbidden);
        }
        self.require_phase(GraphActorPhase::Installed)?;
        let identity = self.identity.ok_or(GraphLiveError::Internal)?;
        self.bridge
            .clear_package(GraphBridgeSide::Service, identity)?;
        self.identity = None;
        self.package = None;
        self.metadata = GraphRuntimeMetadata::EMPTY;
        self.reset_local();
        self.phase = GraphActorPhase::Empty;
        Ok(())
    }

    fn require_phase(&self, required: GraphActorPhase) -> Result<(), GraphLiveError> {
        require_actor_phase(self.phase, required)
    }

    fn require_run_matches(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        require_run_matches(self.identity, run)
    }

    fn require_stoppable(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        if self.phase == GraphActorPhase::Empty {
            return Err(GraphLiveError::ActorPhase {
                required: GraphActorPhase::Installed,
                actual: GraphActorPhase::Empty,
            });
        }
        self.require_run_matches(run)?;
        if self.run.is_some() && self.run != Some(run) {
            return Err(GraphLiveError::RunIdentity);
        }
        Ok(())
    }

    fn reset_local(&mut self) {
        self.start_cycle = 0;
        self.cursor = DomainCursor::EMPTY;
        self.state.fill(0);
        self.channels.fill(0);
        self.queues.fill(QueueCursor::EMPTY);
    }
}

/// Permanent core-1 graph owner supporting repeated install/run/stop cycles.
pub struct FixedGraphRealtimeActor<
    'a,
    const STATE: usize,
    const CHANNELS: usize,
    const BRIDGE: usize,
> {
    bridge: &'a ReloadableGraphBridge<BRIDGE>,
    phase: GraphActorPhase,
    identity: Option<GraphDeploymentIdentity>,
    package: Option<GraphIrPackage>,
    metadata: GraphRuntimeMetadata,
    run: Option<GraphRunIdentity>,
    start_cycle: u64,
    cursor: DomainCursor,
    state: [u8; STATE],
    channels: [u8; CHANNELS],
    queues: [QueueCursor; alumina_graph_ir::MAX_GRAPH_IR_CHANNELS],
    initialized: [bool; alumina_graph_ir::MAX_GRAPH_IR_NODES],
}

impl<'a, const STATE: usize, const CHANNELS: usize, const BRIDGE: usize>
    FixedGraphRealtimeActor<'a, STATE, CHANNELS, BRIDGE>
{
    /// Bind permanent core-local arrays to the one shared bridge.
    pub const fn new(bridge: &'a ReloadableGraphBridge<BRIDGE>) -> Self {
        Self {
            bridge,
            phase: GraphActorPhase::Empty,
            identity: None,
            package: None,
            metadata: GraphRuntimeMetadata::EMPTY,
            run: None,
            start_cycle: 0,
            cursor: DomainCursor::EMPTY,
            state: [0; STATE],
            channels: [0; CHANNELS],
            queues: [QueueCursor::EMPTY; alumina_graph_ir::MAX_GRAPH_IR_CHANNELS],
            initialized: [false; alumina_graph_ir::MAX_GRAPH_IR_NODES],
        }
    }

    /// Complete bytes reserved by this concrete actor, excluding the bridge.
    pub const fn static_actor_bytes() -> usize {
        size_of::<Self>()
    }

    /// Current core-local lifecycle phase.
    pub const fn phase(&self) -> GraphActorPhase {
        self.phase
    }

    /// Independently admitted package identity.
    pub const fn installed_identity(&self) -> Option<GraphDeploymentIdentity> {
        self.identity
    }

    /// Exact active run for this actor.
    pub const fn run_identity(&self) -> Option<GraphRunIdentity> {
        self.run
    }

    /// Independently admit one exact package into core-1-owned storage.
    #[allow(clippy::too_many_arguments)]
    pub fn install(
        &mut self,
        bytes: &[u8],
        transaction_id: u64,
        content_digest: Digest,
        package_digest: Digest,
        authority: GraphRuntimeAuthority,
        limits: GraphRuntimeLimits,
        mutation_allowed: bool,
    ) -> Result<GraphInstallReport, GraphLiveError> {
        if !mutation_allowed {
            return Err(GraphLiveError::MutationForbidden);
        }
        self.require_phase(GraphActorPhase::Empty)?;
        if transaction_id == 0 || content_digest.is_zero() {
            return Err(GraphLiveError::Deployment(GraphDeploymentFault::Sequence));
        }
        if graph_ir_content_digest(bytes) != content_digest {
            return Err(GraphLiveError::Deployment(
                GraphDeploymentFault::ContentDigest,
            ));
        }
        let (package, metadata, usage) = admit_package(bytes, package_digest, authority, limits)
            .map_err(GraphDeploymentFault::from_runtime)
            .map_err(GraphLiveError::Deployment)?;
        let identity = GraphDeploymentIdentity {
            transaction_id,
            content_digest,
            package_digest,
            implementation_digest: authority.implementation_digest,
            graph_digest: package.header().graph_digest,
            summary: package.summary(),
            usage,
        };
        check_capacity(
            GraphRuntimeArena::RealtimeState,
            identity.usage.realtime_state_bytes,
            STATE,
        )
        .map_err(GraphLiveError::Runtime)?;
        check_capacity(
            GraphRuntimeArena::RealtimeChannels,
            identity.usage.realtime_channel_bytes,
            CHANNELS,
        )
        .map_err(GraphLiveError::Runtime)?;
        check_capacity(
            GraphRuntimeArena::ServiceToRealtime,
            identity.usage.bridge_channel_bytes,
            BRIDGE,
        )
        .map_err(GraphLiveError::Runtime)?;
        self.bridge
            .select_package(GraphBridgeSide::Realtime, identity)?;
        self.reset_local();
        self.identity = Some(identity);
        self.package = Some(package);
        self.metadata = metadata;
        self.phase = GraphActorPhase::Installed;
        Ok(install_report::<Self>(identity))
    }

    /// Bind core 1 to a source-first primed run without releasing a node.
    pub fn prepare_start(
        &mut self,
        run: GraphRunIdentity,
        execution_allowed: bool,
    ) -> Result<(), GraphLiveError> {
        if !execution_allowed {
            return Err(GraphLiveError::ExecutionForbidden);
        }
        self.require_phase(GraphActorPhase::Installed)?;
        self.require_run_matches(run)?;
        self.bridge.require_primed(run)?;
        self.reset_local();
        let package = self.package.as_ref().ok_or(GraphLiveError::Internal)?;
        self.start_cycle = run.start_cycle.0;
        self.cursor = cursor_at_start(package.header().realtime_schedule, run.start_cycle);
        self.run = Some(run);
        self.phase = GraphActorPhase::Prepared;
        Ok(())
    }

    /// Atomically permit both actors to begin exact scheduled releases.
    pub fn activate(&mut self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        self.require_phase(GraphActorPhase::Prepared)?;
        if self.run != Some(run) {
            return Err(GraphLiveError::RunIdentity);
        }
        self.bridge.activate(run)?;
        self.phase = GraphActorPhase::Running;
        Ok(())
    }

    /// Execute the exact next Realtime release without waiting or allocation.
    pub fn release<F>(
        &mut self,
        cycle: DeviceCycle,
        safety_authorized: bool,
        mut resource_input: F,
    ) -> Result<GraphReleaseReport, GraphLiveError>
    where
        F: FnMut(ResourceId) -> Option<bool>,
    {
        self.require_phase(GraphActorPhase::Running)?;
        let run = self.run.ok_or(GraphLiveError::RunIdentity)?;
        self.bridge.require_running(run)?;
        let package = self.package.as_ref().ok_or(GraphLiveError::Internal)?;
        let result = (|| {
            release_prelude(
                &self.bridge.fault,
                &self.cursor,
                cycle,
                safety_authorized,
                0,
            )?;
            let next_cycle = checked_next_cycle(&self.bridge.fault, &self.cursor, 0)?;
            let next_tick = checked_next_tick(&self.bridge.fault, &self.cursor, 0)?;
            let report = execute_realtime_release(
                package,
                &self.metadata,
                &mut self.state,
                &mut self.channels,
                &mut self.queues,
                &mut self.initialized,
                &self.bridge.arena,
                &self.bridge.fault,
                self.start_cycle,
                cycle,
                self.cursor.next_tick,
                &mut resource_input,
            )?;
            self.cursor.next_cycle = next_cycle;
            self.cursor.next_tick = next_tick;
            Ok(report)
        })();
        match result {
            Ok(report) => Ok(report),
            Err(error) => {
                self.phase = GraphActorPhase::Faulted;
                Err(GraphLiveError::Execution(error))
            }
        }
    }

    /// Next exact release cycle while prepared or running.
    pub fn next_release_cycle(&self) -> Option<DeviceCycle> {
        if matches!(
            self.phase,
            GraphActorPhase::Prepared | GraphActorPhase::Running
        ) && self.cursor.present
        {
            Some(DeviceCycle(self.cursor.next_cycle))
        } else {
            None
        }
    }

    /// Exact next release and package-declared latest dispatch boundary.
    pub fn next_release_window(&self) -> Result<Option<GraphReleaseWindow>, GraphLiveError> {
        if !matches!(
            self.phase,
            GraphActorPhase::Prepared | GraphActorPhase::Running
        ) {
            return Ok(None);
        }
        cursor_release_window(&self.cursor).map_err(GraphLiveError::Runtime)
    }

    /// Current shared bridge phase observed from core 1.
    pub fn bridge_phase(&self) -> GraphBridgePhase {
        self.bridge.phase()
    }

    /// Newest shared first-cause execution fault.
    pub fn fault_after(&self, generation: u16) -> Option<GraphFaultObservation> {
        self.bridge.fault_after(generation)
    }

    /// Stop new releases and acknowledge that core 1 owns no in-progress work.
    pub fn stop(&mut self, run: GraphRunIdentity) -> Result<GraphStopReport, GraphLiveError> {
        self.require_stoppable(run)?;
        self.bridge.request_stop(run)?;
        let bridge_empty = self
            .bridge
            .acknowledge_stop(run, GraphBridgeSide::Realtime)?;
        self.run = None;
        self.cursor = DomainCursor::EMPTY;
        self.phase = GraphActorPhase::Installed;
        Ok(GraphStopReport { run, bridge_empty })
    }

    /// Remove the installed package only after both actors stopped.
    pub fn clear(&mut self, mutation_allowed: bool) -> Result<(), GraphLiveError> {
        if !mutation_allowed {
            return Err(GraphLiveError::MutationForbidden);
        }
        self.require_phase(GraphActorPhase::Installed)?;
        let identity = self.identity.ok_or(GraphLiveError::Internal)?;
        self.bridge
            .clear_package(GraphBridgeSide::Realtime, identity)?;
        self.identity = None;
        self.package = None;
        self.metadata = GraphRuntimeMetadata::EMPTY;
        self.reset_local();
        self.phase = GraphActorPhase::Empty;
        Ok(())
    }

    fn require_phase(&self, required: GraphActorPhase) -> Result<(), GraphLiveError> {
        require_actor_phase(self.phase, required)
    }

    fn require_run_matches(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        require_run_matches(self.identity, run)
    }

    fn require_stoppable(&self, run: GraphRunIdentity) -> Result<(), GraphLiveError> {
        if self.phase == GraphActorPhase::Empty {
            return Err(GraphLiveError::ActorPhase {
                required: GraphActorPhase::Installed,
                actual: GraphActorPhase::Empty,
            });
        }
        self.require_run_matches(run)?;
        if self.run.is_some() && self.run != Some(run) {
            return Err(GraphLiveError::RunIdentity);
        }
        Ok(())
    }

    fn reset_local(&mut self) {
        self.start_cycle = 0;
        self.cursor = DomainCursor::EMPTY;
        self.state.fill(0);
        self.channels.fill(0);
        self.queues.fill(QueueCursor::EMPTY);
        self.initialized.fill(false);
    }
}

fn install_report<T>(identity: GraphDeploymentIdentity) -> GraphInstallReport {
    GraphInstallReport {
        package_digest: identity.package_digest,
        graph_digest: identity.graph_digest,
        summary: identity.summary,
        usage: identity.usage,
        static_runtime_bytes: size_of::<T>(),
    }
}

fn require_actor_phase(
    actual: GraphActorPhase,
    required: GraphActorPhase,
) -> Result<(), GraphLiveError> {
    if actual == required {
        Ok(())
    } else {
        Err(GraphLiveError::ActorPhase { required, actual })
    }
}

fn require_run_matches(
    identity: Option<GraphDeploymentIdentity>,
    run: GraphRunIdentity,
) -> Result<(), GraphLiveError> {
    run.validate()?;
    let identity = identity.ok_or(GraphLiveError::RunIdentity)?;
    if identity.transaction_id == run.transaction_id
        && identity.content_digest == run.content_digest
        && identity.package_digest == run.package_digest
    {
        Ok(())
    } else {
        Err(GraphLiveError::RunIdentity)
    }
}

/// Reloadable actor admission, handshake, or execution failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphLiveError {
    /// Safety/job ownership forbids package mutation.
    MutationForbidden,
    /// Safety/job ownership forbids execution preparation.
    ExecutionForbidden,
    /// One core-local actor was in the wrong phase.
    ActorPhase {
        /// Required actor phase.
        required: GraphActorPhase,
        /// Observed actor phase.
        actual: GraphActorPhase,
    },
    /// The shared bridge was in the wrong phase.
    BridgePhase {
        /// Required bridge phase.
        required: GraphBridgePhase,
        /// Observed bridge phase.
        actual: GraphBridgePhase,
    },
    /// Bridge run identity differed from the actor's exact run.
    BridgeIdentity,
    /// Both permanent actors did not select the same exact installed package.
    BridgeSelection,
    /// Run, transaction, content, package, or start identity was absent/different.
    RunIdentity,
    /// Independent package/deployment admission failed.
    Deployment(GraphDeploymentFault),
    /// Fixed runtime decode, shape, arithmetic, or capacity admission failed.
    Runtime(GraphRuntimeError),
    /// One release latched the shared first-cause fault.
    Execution(GraphExecutionError),
    /// Independently decoded facts contradicted one another.
    Internal,
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alumina_board::{
        GraphOpcodeDescriptor, GraphResourceAccess, GraphResourceClass, GraphResourceDescriptor,
        OwnerDomain, ResourceId, SupportLevel,
    };
    use alumina_graph_ir::{
        BOOLEAN_LATEST_STATE_BYTES, BOOLEAN_STREAM_ITEM_BYTES, GraphIrChannel, GraphIrChannelOwner,
        GraphIrDomain, GraphIrFullPolicy, GraphIrHeader, GraphIrNode, GraphIrOpcode,
        GraphIrSchedule, encode_graph_resource_pair_parameter, encode_graph_resource_parameter,
        graph_ir_content_digest,
    };
    use alumina_protocol::DeviceId;

    use super::*;
    type ServiceActor<'a> = FixedGraphServiceActor<'a, 0, 0, 42>;
    type RealtimeActor<'a> = FixedGraphRealtimeActor<'a, 5, 21, 42>;

    const LIMITS: GraphRuntimeLimits = GraphRuntimeLimits {
        service_state_bytes: 0,
        realtime_state_bytes: 5,
        service_channel_bytes: 0,
        realtime_channel_bytes: 21,
        bridge_channel_bytes: 42,
        opcodes: crate::graph::RESOURCE_FREE_GRAPH_OPCODES,
        resources: &[],
    };
    const INPUT_CLASS: GraphResourceClass = GraphResourceClass::new(1);
    const INPUT_OPCODES: &[GraphOpcodeDescriptor] = &[
        GraphOpcodeDescriptor {
            opcode: GraphIrOpcode::BooleanStreamSink as u8,
            domain: OwnerDomain::Realtime,
            support: SupportLevel::Compiles,
            resource_class: None,
            resource_access: None,
        },
        GraphOpcodeDescriptor {
            opcode: GraphIrOpcode::StableBooleanInput as u8,
            domain: OwnerDomain::Realtime,
            support: SupportLevel::Compiles,
            resource_class: Some(INPUT_CLASS),
            resource_access: Some(GraphResourceAccess::StableBooleanInput),
        },
        GraphOpcodeDescriptor {
            opcode: GraphIrOpcode::StableBooleanPairAll as u8,
            domain: OwnerDomain::Realtime,
            support: SupportLevel::Compiles,
            resource_class: Some(INPUT_CLASS),
            resource_access: Some(GraphResourceAccess::StableBooleanInput),
        },
    ];
    const INPUT_RESOURCES: &[GraphResourceDescriptor] = &[
        GraphResourceDescriptor {
            resource: ResourceId::Gpio(33),
            class: INPUT_CLASS,
            access: GraphResourceAccess::StableBooleanInput,
            support: SupportLevel::Compiles,
        },
        GraphResourceDescriptor {
            resource: ResourceId::Gpio(35),
            class: INPUT_CLASS,
            access: GraphResourceAccess::StableBooleanInput,
            support: SupportLevel::Compiles,
        },
    ];
    const WRONG_INPUT_RESOURCES: &[GraphResourceDescriptor] = &[GraphResourceDescriptor {
        resource: ResourceId::Gpio(32),
        class: INPUT_CLASS,
        access: GraphResourceAccess::StableBooleanInput,
        support: SupportLevel::Compiles,
    }];
    const INPUT_LIMITS: GraphRuntimeLimits = GraphRuntimeLimits {
        service_state_bytes: 0,
        realtime_state_bytes: 5,
        service_channel_bytes: 0,
        realtime_channel_bytes: 21,
        bridge_channel_bytes: 42,
        opcodes: INPUT_OPCODES,
        resources: INPUT_RESOURCES,
    };

    fn digest(byte: u8) -> Digest {
        Digest([byte; 32])
    }

    fn authority() -> GraphRuntimeAuthority {
        GraphRuntimeAuthority {
            device_id: DeviceId([1; 16]),
            capability_digest: digest(4),
            config_digest: digest(5),
            implementation_digest: digest(3),
        }
    }

    fn package() -> GraphIrPackage {
        GraphIrPackage::encode(
            GraphIrHeader {
                device_id: DeviceId([1; 16]),
                graph_digest: digest(2),
                implementation_digest: digest(3),
                capability_digest: digest(4),
                config_digest: digest(5),
                service_schedule: GraphIrSchedule {
                    clock_id: 10,
                    period_cycles: 1_000,
                    total_wcet_cycles: 20,
                    executor_reserve_cycles: 100,
                    node_count: 1,
                },
                realtime_schedule: GraphIrSchedule {
                    clock_id: 20,
                    period_cycles: 2_000,
                    total_wcet_cycles: 40,
                    executor_reserve_cycles: 100,
                    node_count: 2,
                },
                total_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                service_state_bytes: 0,
                realtime_state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                channel_storage_bytes: 3 * BOOLEAN_STREAM_ITEM_BYTES,
                bridge_storage_bytes: 2 * BOOLEAN_STREAM_ITEM_BYTES,
            },
            &[
                GraphIrNode {
                    graph_node_id: 1,
                    domain: GraphIrDomain::Service,
                    opcode: GraphIrOpcode::BooleanStreamConstant,
                    schedule_clock_id: 10,
                    period_cycles: 1_000,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: 0,
                    parameter: 1,
                },
                GraphIrNode {
                    graph_node_id: 2,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanLatest,
                    schedule_clock_id: 20,
                    period_cycles: 2_000,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: BOOLEAN_LATEST_STATE_BYTES,
                    parameter: 0,
                },
                GraphIrNode {
                    graph_node_id: 3,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanStreamSink,
                    schedule_clock_id: 20,
                    period_cycles: 2_000,
                    wcet_cycles: 20,
                    state_offset: BOOLEAN_LATEST_STATE_BYTES,
                    state_bytes: 0,
                    parameter: 0,
                },
            ],
            &[
                GraphIrChannel {
                    graph_wire_id: 1,
                    source_node: 0,
                    target_node: 1,
                    owner: GraphIrChannelOwner::ServiceToRealtime,
                    full_policy: GraphIrFullPolicy::Fault,
                    capacity: 2,
                    item_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                    storage_offset: 0,
                    storage_bytes: 2 * BOOLEAN_STREAM_ITEM_BYTES,
                },
                GraphIrChannel {
                    graph_wire_id: 2,
                    source_node: 1,
                    target_node: 2,
                    owner: GraphIrChannelOwner::Realtime,
                    full_policy: GraphIrFullPolicy::Fault,
                    capacity: 1,
                    item_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                    storage_offset: 0,
                    storage_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                },
            ],
        )
        .unwrap()
    }

    fn input_package() -> GraphIrPackage {
        GraphIrPackage::encode(
            GraphIrHeader {
                device_id: DeviceId([1; 16]),
                graph_digest: digest(2),
                implementation_digest: digest(3),
                capability_digest: digest(4),
                config_digest: digest(5),
                service_schedule: GraphIrSchedule::EMPTY,
                realtime_schedule: GraphIrSchedule {
                    clock_id: 20,
                    period_cycles: 2_000,
                    total_wcet_cycles: 40,
                    executor_reserve_cycles: 100,
                    node_count: 2,
                },
                total_state_bytes: 0,
                service_state_bytes: 0,
                realtime_state_bytes: 0,
                channel_storage_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                bridge_storage_bytes: 0,
            },
            &[
                GraphIrNode {
                    graph_node_id: 1,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::StableBooleanInput,
                    schedule_clock_id: 20,
                    period_cycles: 2_000,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: 0,
                    parameter: encode_graph_resource_parameter(ResourceId::Gpio(33)),
                },
                GraphIrNode {
                    graph_node_id: 2,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanStreamSink,
                    schedule_clock_id: 20,
                    period_cycles: 2_000,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: 0,
                    parameter: 0,
                },
            ],
            &[GraphIrChannel {
                graph_wire_id: 1,
                source_node: 0,
                target_node: 1,
                owner: GraphIrChannelOwner::Realtime,
                full_policy: GraphIrFullPolicy::Fault,
                capacity: 1,
                item_bytes: BOOLEAN_STREAM_ITEM_BYTES,
                storage_offset: 0,
                storage_bytes: BOOLEAN_STREAM_ITEM_BYTES,
            }],
        )
        .unwrap()
    }

    fn paired_input_package() -> GraphIrPackage {
        let mut package = input_package();
        let header = package.header();
        let mut nodes = package.nodes().collect::<std::vec::Vec<_>>();
        let channels = package.channels().collect::<std::vec::Vec<_>>();
        nodes[0].opcode = GraphIrOpcode::StableBooleanPairAll;
        nodes[0].parameter =
            encode_graph_resource_pair_parameter(ResourceId::Gpio(33), ResourceId::Gpio(35));
        package = GraphIrPackage::encode(header, &nodes, &channels).unwrap();
        package
    }

    fn install(
        service: &mut ServiceActor<'_>,
        realtime: &mut RealtimeActor<'_>,
        package: &GraphIrPackage,
        transaction_id: u64,
    ) -> GraphDeploymentIdentity {
        let content = graph_ir_content_digest(package.bytes());
        let service_report = service
            .install(
                package.bytes(),
                transaction_id,
                content,
                package.digest(),
                authority(),
                LIMITS,
                true,
            )
            .unwrap();
        let realtime_report = realtime
            .install(
                package.bytes(),
                transaction_id,
                content,
                package.digest(),
                authority(),
                LIMITS,
                true,
            )
            .unwrap();
        assert_eq!(
            service_report.package_digest,
            realtime_report.package_digest
        );
        assert_eq!(service_report.usage, realtime_report.usage);
        let identity = service.installed_identity().unwrap();
        assert_eq!(realtime.installed_identity(), Some(identity));
        identity
    }

    fn run(identity: GraphDeploymentIdentity, run_id: u64, start: u64) -> GraphRunIdentity {
        GraphRunIdentity {
            transaction_id: identity.transaction_id,
            run_id,
            content_digest: identity.content_digest,
            package_digest: identity.package_digest,
            start_cycle: DeviceCycle(start),
        }
    }

    fn start(
        service: &mut ServiceActor<'_>,
        realtime: &mut RealtimeActor<'_>,
        run: GraphRunIdentity,
    ) {
        let report = service.prepare_start(run, true).unwrap();
        assert_eq!(report.start_cycle, run.start_cycle);
        assert_eq!(report.primed_service_release.unwrap().release_tick, 0);
        realtime.prepare_start(run, true).unwrap();
        realtime.activate(run).unwrap();
        service.observe_realtime_started(run).unwrap();
    }

    fn stop(
        service: &mut ServiceActor<'_>,
        realtime: &mut RealtimeActor<'_>,
        run: GraphRunIdentity,
    ) {
        assert!(!service.stop(run).unwrap().bridge_empty);
        assert!(realtime.stop(run).unwrap().bridge_empty);
    }

    #[test]
    fn permanent_actors_run_stop_rerun_and_clear_without_lifetime_rebinding() {
        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let identity = install(&mut service, &mut realtime, &package(), 41);
        let first = run(identity, 1, 10_000);
        start(&mut service, &mut realtime, first);
        assert_eq!(bridge.phase(), GraphBridgePhase::Running);
        assert_eq!(service.next_release_cycle(), Some(DeviceCycle(11_000)));
        assert_eq!(realtime.next_release_cycle(), Some(DeviceCycle(10_000)));

        let realtime_tick_zero = realtime
            .release(DeviceCycle(10_000), true, |_| None)
            .unwrap();
        assert_eq!(realtime_tick_zero.last_sink_value, Some(true));
        service.release(DeviceCycle(11_000), true).unwrap();
        service.release(DeviceCycle(12_000), true).unwrap();
        let realtime_tick_one = realtime
            .release(DeviceCycle(12_000), true, |_| None)
            .unwrap();
        assert_eq!(realtime_tick_one.last_sink_value, Some(true));
        assert_eq!(realtime_tick_one.items_consumed, 3);

        stop(&mut service, &mut realtime, first);
        assert_eq!(bridge.phase(), GraphBridgePhase::Empty);
        assert_eq!(service.phase(), GraphActorPhase::Installed);
        assert_eq!(realtime.phase(), GraphActorPhase::Installed);

        let second = run(identity, 2, 20_000);
        start(&mut service, &mut realtime, second);
        assert_eq!(
            realtime
                .release(DeviceCycle(20_000), true, |_| None)
                .unwrap()
                .last_sink_value,
            Some(true)
        );
        stop(&mut service, &mut realtime, second);
        service.clear(true).unwrap();
        realtime.clear(true).unwrap();
        assert_eq!(service.phase(), GraphActorPhase::Empty);
        assert_eq!(realtime.phase(), GraphActorPhase::Empty);
    }

    #[test]
    fn stable_input_requires_exact_palette_and_faults_when_sample_is_unavailable() {
        let package = input_package();
        let missing_opcode = match super::super::admit_package(
            package.bytes(),
            package.digest(),
            authority(),
            LIMITS,
        ) {
            Ok(_) => panic!("resource-bearing package passed a resource-free palette"),
            Err(error) => error,
        };
        assert_eq!(
            missing_opcode,
            GraphRuntimeError::OpcodeCapability {
                node: 0,
                opcode: GraphIrOpcode::StableBooleanInput as u8,
            }
        );
        let wrong_limits = GraphRuntimeLimits {
            resources: WRONG_INPUT_RESOURCES,
            ..INPUT_LIMITS
        };
        let wrong_resource = match super::super::admit_package(
            package.bytes(),
            package.digest(),
            authority(),
            wrong_limits,
        ) {
            Ok(_) => panic!("package passed a palette containing only another resource"),
            Err(error) => error,
        };
        assert_eq!(
            wrong_resource,
            GraphRuntimeError::ResourceCapability {
                node: 0,
                resource: ResourceId::Gpio(33),
            }
        );

        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let content = graph_ir_content_digest(package.bytes());
        service
            .install(
                package.bytes(),
                51,
                content,
                package.digest(),
                authority(),
                INPUT_LIMITS,
                true,
            )
            .unwrap();
        realtime
            .install(
                package.bytes(),
                51,
                content,
                package.digest(),
                authority(),
                INPUT_LIMITS,
                true,
            )
            .unwrap();
        let identity = service.installed_identity().unwrap();
        let run = run(identity, 1, 10_000);
        let prepared = service.prepare_start(run, true).unwrap();
        assert_eq!(prepared.primed_service_release, None);
        realtime.prepare_start(run, true).unwrap();
        realtime.activate(run).unwrap();
        service.observe_realtime_started(run).unwrap();

        let mut reads = 0;
        let report = realtime
            .release(DeviceCycle(10_000), true, |resource| {
                assert_eq!(resource, ResourceId::Gpio(33));
                reads += 1;
                Some(true)
            })
            .unwrap();
        assert_eq!(reads, 1);
        assert_eq!(report.nodes_executed, 2);
        assert_eq!(report.items_emitted, 1);
        assert_eq!(report.items_consumed, 1);
        assert_eq!(report.sink_items, 1);
        assert_eq!(report.last_sink_value, Some(true));

        let unavailable = realtime
            .release(DeviceCycle(12_000), true, |_| None)
            .unwrap_err();
        assert!(matches!(
            unavailable,
            GraphLiveError::Execution(GraphExecutionError {
                observation: GraphFaultObservation {
                    fault: GraphExecutionFault::ResourceUnavailable,
                    detail: 0,
                    ..
                },
                ..
            })
        ));
        assert_eq!(realtime.phase(), GraphActorPhase::Faulted);
    }

    #[test]
    fn paired_stable_inputs_read_both_in_order_and_fault_before_emitting_on_absence() {
        let package = paired_input_package();
        let one_resource_limits = GraphRuntimeLimits {
            resources: &INPUT_RESOURCES[..1],
            ..INPUT_LIMITS
        };
        assert_eq!(
            match super::super::admit_package(
                package.bytes(),
                package.digest(),
                authority(),
                one_resource_limits,
            ) {
                Ok(_) => panic!("paired input passed with only its first resource"),
                Err(error) => error,
            },
            GraphRuntimeError::ResourceCapability {
                node: 0,
                resource: ResourceId::Gpio(35),
            }
        );

        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let content = graph_ir_content_digest(package.bytes());
        service
            .install(
                package.bytes(),
                52,
                content,
                package.digest(),
                authority(),
                INPUT_LIMITS,
                true,
            )
            .unwrap();
        realtime
            .install(
                package.bytes(),
                52,
                content,
                package.digest(),
                authority(),
                INPUT_LIMITS,
                true,
            )
            .unwrap();
        let identity = service.installed_identity().unwrap();
        let run = run(identity, 1, 10_000);
        service.prepare_start(run, true).unwrap();
        realtime.prepare_start(run, true).unwrap();
        realtime.activate(run).unwrap();
        service.observe_realtime_started(run).unwrap();

        let mut observed = std::vec::Vec::new();
        let false_report = realtime
            .release(DeviceCycle(10_000), true, |resource| {
                observed.push(resource);
                Some(resource == ResourceId::Gpio(33))
            })
            .unwrap();
        assert_eq!(observed, [ResourceId::Gpio(33), ResourceId::Gpio(35)]);
        assert_eq!(false_report.last_sink_value, Some(false));

        observed.clear();
        let true_report = realtime
            .release(DeviceCycle(12_000), true, |resource| {
                observed.push(resource);
                Some(true)
            })
            .unwrap();
        assert_eq!(observed, [ResourceId::Gpio(33), ResourceId::Gpio(35)]);
        assert_eq!(true_report.last_sink_value, Some(true));

        observed.clear();
        let unavailable = realtime
            .release(DeviceCycle(14_000), true, |resource| {
                observed.push(resource);
                (resource == ResourceId::Gpio(35)).then_some(true)
            })
            .unwrap_err();
        assert_eq!(observed, [ResourceId::Gpio(33), ResourceId::Gpio(35)]);
        assert!(matches!(
            unavailable,
            GraphLiveError::Execution(GraphExecutionError {
                observation: GraphFaultObservation {
                    fault: GraphExecutionFault::ResourceUnavailable,
                    detail: 0,
                    ..
                },
                ..
            })
        ));
        assert_eq!(realtime.phase(), GraphActorPhase::Faulted);
    }

    #[test]
    fn first_cause_is_shared_until_both_actors_stop_then_resets_for_new_run() {
        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let identity = install(&mut service, &mut realtime, &package(), 41);
        let first = run(identity, 1, 10_000);
        start(&mut service, &mut realtime, first);

        service.release(DeviceCycle(11_000), true).unwrap();
        let fault = service.release(DeviceCycle(12_000), true).unwrap_err();
        assert!(matches!(fault, GraphLiveError::Execution(_)));
        let shared = bridge.fault_after(0).unwrap();
        assert_eq!(shared.fault, super::super::GraphExecutionFault::QueueFull);
        assert!(matches!(
            realtime.release(DeviceCycle(10_000), true, |_| None),
            Err(GraphLiveError::Execution(error)) if error.observation == shared
        ));

        stop(&mut service, &mut realtime, first);
        let second = run(identity, 2, 20_000);
        service.prepare_start(second, true).unwrap();
        assert!(bridge.fault_after(0).is_none());
        realtime.prepare_start(second, true).unwrap();
        realtime.activate(second).unwrap();
        service.observe_realtime_started(second).unwrap();
    }

    #[test]
    fn missing_or_mismatched_peer_selection_fails_before_bridge_progress() {
        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let package = package();
        let content = graph_ir_content_digest(package.bytes());
        service
            .install(
                package.bytes(),
                41,
                content,
                package.digest(),
                authority(),
                LIMITS,
                true,
            )
            .unwrap();
        let service_identity = service.installed_identity().unwrap();
        let selected = run(service_identity, 1, 10_000);
        assert_eq!(
            service.prepare_start(selected, true),
            Err(GraphLiveError::BridgeSelection)
        );
        assert_eq!(bridge.phase(), GraphBridgePhase::Empty);

        realtime
            .install(
                package.bytes(),
                42,
                content,
                package.digest(),
                authority(),
                LIMITS,
                true,
            )
            .unwrap();
        assert_eq!(
            service.prepare_start(selected, true),
            Err(GraphLiveError::BridgeSelection)
        );
        assert_eq!(bridge.phase(), GraphBridgePhase::Empty);
        service.clear(true).unwrap();
        realtime.clear(true).unwrap();
    }

    #[test]
    fn foreign_run_capacity_and_second_owner_fail_before_bridge_progress() {
        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let package = package();
        let identity = install(&mut service, &mut realtime, &package, 41);
        let mut foreign = run(identity, 1, 10_000);
        foreign.transaction_id += 1;
        assert_eq!(
            service.prepare_start(foreign, true),
            Err(GraphLiveError::RunIdentity)
        );
        assert_eq!(bridge.phase(), GraphBridgePhase::Empty);

        let mut second_service = ServiceActor::new(&bridge);
        assert_eq!(
            second_service.install(
                package.bytes(),
                41,
                identity.content_digest,
                identity.package_digest,
                authority(),
                LIMITS,
                true,
            ),
            Err(GraphLiveError::BridgeSelection)
        );
        assert_eq!(bridge.phase(), GraphBridgePhase::Empty);

        let small_bridge = ReloadableGraphBridge::<41>::new();
        let mut too_small = FixedGraphServiceActor::<0, 0, 41>::new(&small_bridge);
        assert!(matches!(
            too_small.install(
                package.bytes(),
                42,
                graph_ir_content_digest(package.bytes()),
                package.digest(),
                authority(),
                LIMITS,
                true,
            ),
            Err(GraphLiveError::Runtime(GraphRuntimeError::Capacity {
                arena: GraphRuntimeArena::ServiceToRealtime,
                required: 42,
                available: 41
            }))
        ));
    }

    #[test]
    fn execution_reports_and_declared_dispatch_windows_are_canonical() {
        let bridge = ReloadableGraphBridge::<42>::new();
        let mut service = ServiceActor::new(&bridge);
        let mut realtime = RealtimeActor::new(&bridge);
        let identity = install(&mut service, &mut realtime, &package(), 41);
        let run = run(identity, 7, 10_000);
        start(&mut service, &mut realtime, run);

        assert_eq!(
            service.next_release_window().unwrap(),
            Some(GraphReleaseWindow {
                scheduled_cycle: DeviceCycle(11_000),
                latest_dispatch_cycle: DeviceCycle(11_100),
            })
        );
        assert_eq!(
            realtime.next_release_window().unwrap(),
            Some(GraphReleaseWindow {
                scheduled_cycle: DeviceCycle(10_000),
                latest_dispatch_cycle: DeviceCycle(10_100),
            })
        );

        let realtime_report = RealtimeGraphExecutionReport {
            actor_phase: realtime.phase(),
            bridge_phase: realtime.bridge_phase(),
            transaction_id: identity.transaction_id,
            run_id: run.run_id,
            start_cycle: run.start_cycle,
            next_release_cycle: realtime.next_release_cycle(),
            last_release_tick: None,
            fault: realtime.fault_after(0),
        };
        assert_eq!(
            RealtimeGraphExecutionReport::decode(&realtime_report.encode().unwrap()),
            Ok(realtime_report)
        );
        let combined = GraphExecutionReport {
            service_phase: service.phase(),
            realtime_phase: realtime_report.actor_phase,
            bridge_phase: service.bridge_phase(),
            run_id: run.run_id,
            start_cycle: run.start_cycle,
            service_next_cycle: service.next_release_cycle(),
            realtime_next_cycle: realtime_report.next_release_cycle,
            service_last_tick: Some(0),
            realtime_last_tick: None,
            fault: service.fault_after(0),
        };
        assert_eq!(
            GraphExecutionReport::decode(&combined.encode().unwrap()),
            Ok(combined)
        );

        let mut noncanonical = combined.encode().unwrap();
        noncanonical[14] = 1;
        assert_eq!(
            GraphExecutionReport::decode(&noncanonical),
            Err(GraphExecutionReportError::Reserved)
        );

        assert!(service.release(DeviceCycle(11_000), false).is_err());
        let shared_fault = service.fault_after(0).unwrap();
        let realtime_observes_peer_fault = RealtimeGraphExecutionReport {
            actor_phase: realtime.phase(),
            bridge_phase: realtime.bridge_phase(),
            transaction_id: identity.transaction_id,
            run_id: run.run_id,
            start_cycle: run.start_cycle,
            next_release_cycle: realtime.next_release_cycle(),
            last_release_tick: None,
            fault: Some(shared_fault),
        };
        assert_eq!(
            RealtimeGraphExecutionReport::decode(&realtime_observes_peer_fault.encode().unwrap()),
            Ok(realtime_observes_peer_fault)
        );

        service.stop(run).unwrap();
        let realtime_during_peer_stop = RealtimeGraphExecutionReport {
            bridge_phase: GraphBridgePhase::Stopping,
            ..realtime_observes_peer_fault
        };
        assert_eq!(
            RealtimeGraphExecutionReport::decode(&realtime_during_peer_stop.encode().unwrap()),
            Ok(realtime_during_peer_stop)
        );
        realtime.stop(run).unwrap();
    }
}
