//! Reloadable core-local graph actors around one generation-bound bridge.

use core::cell::RefCell;
use core::mem::size_of;

use alumina_graph_ir::{GRAPH_IR_PACKAGE_BYTES, GraphIrPackage};
use alumina_protocol::{DeviceCycle, Digest};
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::LatestSignal;

use super::{
    BridgeArena, DomainCursor, GraphDeploymentFault, GraphDeploymentIdentity, GraphExecutionError,
    GraphFaultObservation, GraphInstallReport, GraphReleaseReport, GraphRuntimeArena,
    GraphRuntimeAuthority, GraphRuntimeError, GraphRuntimeLimits, GraphRuntimeMetadata,
    GraphStartReport, QueueCursor, check_capacity, checked_next_cycle, checked_next_tick,
    cursor_after_prime, cursor_at_start, execute_realtime_release, execute_service_release,
    release_prelude, validate_graph_package,
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

/// Shared bridge lifecycle, advanced only by explicit actor handshakes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
        let (package, metadata, identity) = admit_live_package(
            bytes,
            transaction_id,
            content_digest,
            package_digest,
            authority,
            limits,
        )?;
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
        let (package, metadata, identity) = admit_live_package(
            bytes,
            transaction_id,
            content_digest,
            package_digest,
            authority,
            limits,
        )?;
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

fn admit_live_package(
    bytes: &[u8],
    transaction_id: u64,
    content_digest: Digest,
    package_digest: Digest,
    authority: GraphRuntimeAuthority,
    limits: GraphRuntimeLimits,
) -> Result<
    (
        GraphIrPackage,
        GraphRuntimeMetadata,
        GraphDeploymentIdentity,
    ),
    GraphLiveError,
> {
    let identity = validate_graph_package(
        bytes,
        transaction_id,
        content_digest,
        package_digest,
        authority,
        limits,
    )
    .map_err(GraphLiveError::Deployment)?;
    let package = GraphIrPackage::from_slice(bytes)
        .map_err(GraphRuntimeError::Package)
        .map_err(GraphLiveError::Runtime)?;
    let metadata = GraphRuntimeMetadata::from_package(&package).map_err(GraphLiveError::Runtime)?;
    if package.summary() != identity.summary
        || package.header().implementation_digest != identity.implementation_digest
        || bytes.len() != GRAPH_IR_PACKAGE_BYTES
    {
        return Err(GraphLiveError::Internal);
    }
    Ok((package, metadata, identity))
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

    use alumina_graph_ir::{
        BOOLEAN_LATEST_STATE_BYTES, BOOLEAN_STREAM_ITEM_BYTES, GraphIrChannel, GraphIrChannelOwner,
        GraphIrDomain, GraphIrFullPolicy, GraphIrHeader, GraphIrNode, GraphIrOpcode,
        GraphIrSchedule, graph_ir_content_digest,
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

        let realtime_tick_zero = realtime.release(DeviceCycle(10_000), true).unwrap();
        assert_eq!(realtime_tick_zero.last_sink_value, Some(true));
        service.release(DeviceCycle(11_000), true).unwrap();
        service.release(DeviceCycle(12_000), true).unwrap();
        let realtime_tick_one = realtime.release(DeviceCycle(12_000), true).unwrap();
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
                .release(DeviceCycle(20_000), true)
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
            realtime.release(DeviceCycle(10_000), true),
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
}
