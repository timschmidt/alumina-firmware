//! Core-0 ownership of authenticated published graph installation and lifecycle.

use alumina_graph_ir::{
    CoreGraphCommand, CoreGraphExecutionAction, CoreGraphExecutionCommand, GRAPH_IR_PACKAGE_BYTES,
    GraphPublication, GraphRunRequest, GraphSelection,
};
use alumina_protocol::{
    DeviceCycle, DeviceId, Digest, FrameHeader, FrameKind, Operation, StatusCode,
};
use alumina_runtime::graph::{
    FixedGraphRealtimeActor, FixedGraphServiceActor, GRAPH_COORDINATOR_REPORT_BYTES,
    GraphActorPhase, GraphBridgePhase, GraphCoordinatorFault, GraphCoordinatorPhase,
    GraphCoordinatorReport, GraphDeploymentIdentity, GraphExecutionReport, GraphLiveError,
    GraphReleaseReport, GraphRunIdentity, GraphRuntimeAuthority, GraphRuntimeLimits,
    RealtimeGraphDeployment, RealtimeGraphExecutionReport, RealtimeGraphReport, RealtimeGraphState,
    ReloadableGraphBridge, ServiceGraphTransferError, ServiceGraphValidation,
    ServiceGraphValidationState, ServiceGraphValidationStatus,
};
use alumina_runtime::{DefaultServiceEndpoint, IntercoreFrame};
use alumina_service::{
    MAX_SERVICE_RESPONSE_BYTES, NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse,
};
use alumina_storage::MutationContext;
use alumina_storage::media::MediaError;
use alumina_storage::provisioning::ProvisionedCacheError;

use crate::clock::{MAXIMUM_START_HORIZON_CYCLES, MINIMUM_START_LEAD_CYCLES};
use crate::hardware::selected;

const _: () = assert!(
    FrameHeader::WIRE_LEN
        + alumina_protocol::MessageHeader::WIRE_LEN
        + GRAPH_COORDINATOR_REPORT_BYTES
        <= MAX_SERVICE_RESPONSE_BYTES
);

/// Core-0 graph state reservation.
pub const GRAPH_SERVICE_STATE_BYTES: usize = 2 * 1_024;
/// Core-1 graph state reservation.
pub const GRAPH_REALTIME_STATE_BYTES: usize = 2 * 1_024;
/// Core-0-local graph channel reservation.
pub const GRAPH_SERVICE_CHANNEL_BYTES: usize = 4 * 1_024;
/// Core-1-local graph channel reservation.
pub const GRAPH_REALTIME_CHANNEL_BYTES: usize = 4 * 1_024;
/// Cross-core graph channel reservation.
pub const GRAPH_BRIDGE_BYTES: usize = 4 * 1_024;

/// Permanently allocated cross-core graph bridge used by both application cores.
pub type GraphBridge = ReloadableGraphBridge<GRAPH_BRIDGE_BYTES>;
/// Permanent core-0 graph package and fixed executor arenas.
pub type ServiceGraphActor = FixedGraphServiceActor<
    'static,
    GRAPH_SERVICE_STATE_BYTES,
    GRAPH_SERVICE_CHANNEL_BYTES,
    GRAPH_BRIDGE_BYTES,
>;
/// Permanent core-1 graph package and fixed executor arenas.
pub type RealtimeGraphActor = FixedGraphRealtimeActor<
    'static,
    GRAPH_REALTIME_STATE_BYTES,
    GRAPH_REALTIME_CHANNEL_BYTES,
    GRAPH_BRIDGE_BYTES,
>;

/// Initial board-image graph arena reservations.
///
/// These exact values are enforced independently on both cores. A later
/// capability-schema slice will publish them for browser lowering before any
/// resource-bearing opcode is admitted.
pub const GRAPH_RUNTIME_LIMITS: GraphRuntimeLimits = GraphRuntimeLimits::fixed::<
    GRAPH_SERVICE_STATE_BYTES,
    GRAPH_REALTIME_STATE_BYTES,
    GRAPH_SERVICE_CHANNEL_BYTES,
    GRAPH_REALTIME_CHANNEL_BYTES,
    GRAPH_BRIDGE_BYTES,
>();

/// Sole core-1 owner of selected bytes and the permanent Realtime executor.
pub struct RealtimeGraphExecutor {
    deployment: RealtimeGraphDeployment,
    actor: RealtimeGraphActor,
    current_run: Option<GraphRunRequest>,
    last_started_run_id: u64,
    last_release_tick: Option<u64>,
}

impl RealtimeGraphExecutor {
    /// Bind package lifecycle and executor arenas to the physical target.
    pub const fn new(device_id: DeviceId, bridge: &'static GraphBridge) -> Self {
        Self {
            deployment: RealtimeGraphDeployment::new(
                device_id,
                selected::PACKAGE.board.capability_digest,
                GRAPH_RUNTIME_LIMITS,
            ),
            actor: RealtimeGraphActor::new(bridge),
            current_run: None,
            last_started_run_id: 0,
            last_release_tick: None,
        }
    }

    /// Stable target identity.
    pub const fn device_id(&self) -> DeviceId {
        self.deployment.device_id()
    }

    /// Independently validated inactive candidate.
    pub const fn candidate_identity(&self) -> Option<GraphDeploymentIdentity> {
        self.deployment.candidate_identity()
    }

    /// Independently selected active package.
    pub const fn active_identity(&self) -> Option<GraphDeploymentIdentity> {
        self.deployment.active_identity()
    }

    /// Latest package lifecycle report.
    pub fn lifecycle_report(&self) -> RealtimeGraphReport {
        self.deployment.report()
    }

    /// Apply one package transfer/selection command and keep actor selection atomic.
    pub fn apply_lifecycle(
        &mut self,
        command: CoreGraphCommand,
        authority: GraphRuntimeAuthority,
        mutation_allowed: bool,
    ) -> Result<RealtimeGraphReport, ()> {
        match command.action {
            alumina_graph_ir::CoreGraphAction::Authorize if mutation_allowed => {
                let identity = self.deployment.active_identity().ok_or(())?;
                if !command_matches_identity(command, identity) {
                    return Err(());
                }
                if self.actor.installed_identity() != Some(identity) {
                    match self.actor.phase() {
                        GraphActorPhase::Empty => {}
                        GraphActorPhase::Installed => self.actor.clear(true).map_err(|_| ())?,
                        GraphActorPhase::Prepared
                        | GraphActorPhase::Running
                        | GraphActorPhase::Faulted => return Err(()),
                    }
                    let package = self.deployment.selected_package_bytes().ok_or(())?;
                    let report = self
                        .actor
                        .install(
                            package,
                            command.transaction_id,
                            command.content_digest,
                            command.package_digest,
                            authority,
                            GRAPH_RUNTIME_LIMITS,
                            true,
                        )
                        .map_err(|_| ())?;
                    if report.package_digest != command.package_digest
                        || report.usage != identity.usage
                    {
                        return Err(());
                    }
                    self.current_run = None;
                    self.last_started_run_id = 0;
                    self.last_release_tick = None;
                }
            }
            alumina_graph_ir::CoreGraphAction::Clear if mutation_allowed => {
                let identity = self.deployment.active_identity().ok_or(())?;
                if !command_matches_identity(command, identity) {
                    return Err(());
                }
                match self.actor.phase() {
                    GraphActorPhase::Empty => {}
                    GraphActorPhase::Installed => self.actor.clear(true).map_err(|_| ())?,
                    GraphActorPhase::Prepared
                    | GraphActorPhase::Running
                    | GraphActorPhase::Faulted => return Err(()),
                }
                self.current_run = None;
                self.last_started_run_id = 0;
                self.last_release_tick = None;
            }
            alumina_graph_ir::CoreGraphAction::Begin
            | alumina_graph_ir::CoreGraphAction::Data
            | alumina_graph_ir::CoreGraphAction::Finish
            | alumina_graph_ir::CoreGraphAction::Activate
            | alumina_graph_ir::CoreGraphAction::Abort
            | alumina_graph_ir::CoreGraphAction::Authorize
            | alumina_graph_ir::CoreGraphAction::Clear => {}
        }
        Ok(self.deployment.apply(command, authority, mutation_allowed))
    }

    /// Apply one exact start/stop command after inter-core frame validation.
    pub fn apply_execution(
        &mut self,
        command: CoreGraphExecutionCommand,
        now: DeviceCycle,
        execution_allowed: bool,
    ) -> Result<RealtimeGraphExecutionReport, ()> {
        let request = command.request;
        let identity = self.deployment.active_identity().ok_or(())?;
        if !request_matches_identity(request, identity)
            || self.deployment.authorized_package_bytes().is_none()
        {
            return Err(());
        }
        let run = graph_run_identity(request);
        match command.action {
            CoreGraphExecutionAction::Start => {
                if self.current_run == Some(request)
                    && matches!(
                        self.actor.phase(),
                        GraphActorPhase::Prepared
                            | GraphActorPhase::Running
                            | GraphActorPhase::Faulted
                    )
                {
                    return Ok(self.execution_report());
                }
                if !execution_allowed
                    || request.run_id <= self.last_started_run_id
                    || now.0 >= request.start_cycle.0
                    || self.actor.phase() != GraphActorPhase::Installed
                {
                    return Err(());
                }
                self.actor.prepare_start(run, true).map_err(|_| ())?;
                self.actor.activate(run).map_err(|_| ())?;
                self.current_run = Some(request);
                self.last_started_run_id = request.run_id;
                self.last_release_tick = None;
            }
            CoreGraphExecutionAction::Stop => {
                if self.current_run.is_some() && self.current_run != Some(request) {
                    return Err(());
                }
                match (self.actor.phase(), self.actor.bridge_phase()) {
                    (GraphActorPhase::Installed, GraphBridgePhase::Empty)
                        if self.current_run == Some(request) => {}
                    (GraphActorPhase::Installed, GraphBridgePhase::Stopping) => {
                        self.actor.stop(run).map_err(|_| ())?;
                    }
                    (GraphActorPhase::Prepared, _)
                    | (GraphActorPhase::Running, _)
                    | (GraphActorPhase::Faulted, _) => {
                        self.actor.stop(run).map_err(|_| ())?;
                    }
                    _ => return Err(()),
                }
                self.current_run = Some(request);
            }
        }
        Ok(self.execution_report())
    }

    /// Exact next core-1 release cycle, when running.
    pub fn next_release_cycle(&self) -> Option<DeviceCycle> {
        self.actor.next_release_cycle()
    }

    /// Execute at most one due release with declared dispatch-lateness enforcement.
    pub fn release_due(
        &mut self,
        now: DeviceCycle,
        execution_allowed: bool,
    ) -> Result<Option<GraphReleaseReport>, GraphLiveError> {
        if self.actor.phase() != GraphActorPhase::Running
            || self.actor.bridge_phase() != GraphBridgePhase::Running
        {
            return Ok(None);
        }
        let Some(window) = self.actor.next_release_window()? else {
            return Ok(None);
        };
        if execution_allowed && now < window.scheduled_cycle {
            return Ok(None);
        }
        let release_cycle = if execution_allowed && now <= window.latest_dispatch_cycle {
            window.scheduled_cycle
        } else if execution_allowed {
            now
        } else {
            window.scheduled_cycle
        };
        let report = self.actor.release(release_cycle, execution_allowed)?;
        self.last_release_tick = Some(report.release_tick);
        Ok(Some(report))
    }

    /// Canonical execution telemetry, including a completed run awaiting service ack.
    pub fn execution_report(&self) -> RealtimeGraphExecutionReport {
        let Some(identity) = self.deployment.active_identity() else {
            return RealtimeGraphExecutionReport::empty();
        };
        let run = self.current_run;
        RealtimeGraphExecutionReport {
            actor_phase: self.actor.phase(),
            bridge_phase: self.actor.bridge_phase(),
            transaction_id: identity.transaction_id,
            run_id: run.map_or(0, |request| request.run_id),
            start_cycle: run.map_or(DeviceCycle(0), |request| request.start_cycle),
            next_release_cycle: self.actor.next_release_cycle(),
            last_release_tick: self.last_release_tick,
            fault: self.actor.fault_after(0),
        }
    }
}

/// Sole service-core owner of the published-reader and graph lifecycle state.
pub struct GraphService {
    device_id: DeviceId,
    active_config: Digest,
    phase: GraphCoordinatorPhase,
    fault: GraphCoordinatorFault,
    operation: Option<GraphPublication>,
    validation: Option<ServiceGraphValidation>,
    validation_status: Option<ServiceGraphValidationStatus>,
    active: Option<GraphPublication>,
    realtime: RealtimeGraphReport,
    realtime_execution: RealtimeGraphExecutionReport,
    pending_command: Option<CoreGraphCommand>,
    command_sequence: u32,
    control_sent: bool,
    transfer_sent: bool,
    actor: ServiceGraphActor,
    current_run: Option<GraphRunRequest>,
    last_started_run_id: u64,
    service_last_release_tick: Option<u64>,
    execution_command_sent: bool,
}

impl GraphService {
    /// Starts with no graph; graph selection is intentionally boot-ephemeral in
    /// this slice even though its package bytes are immutable on SD.
    pub const fn new(device_id: DeviceId, bridge: &'static GraphBridge) -> Self {
        Self {
            device_id,
            active_config: Digest::ZERO,
            phase: GraphCoordinatorPhase::Empty,
            fault: GraphCoordinatorFault::None,
            operation: None,
            validation: None,
            validation_status: None,
            active: None,
            realtime: RealtimeGraphReport::empty(),
            realtime_execution: RealtimeGraphExecutionReport::empty(),
            pending_command: None,
            command_sequence: 0,
            control_sent: false,
            transfer_sent: false,
            actor: ServiceGraphActor::new(bridge),
            current_run: None,
            last_started_run_id: 0,
            service_last_release_tick: None,
            execution_command_sent: false,
        }
    }

    /// Whether a valid universal request selects the graph family.
    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes())
                .is_ok_and(|request| request.frame.kind == FrameKind::Graph)
    }

    /// Update the independently authorized configuration identity used by both cores.
    pub fn set_active_config(&mut self, digest: Digest) {
        self.active_config = digest;
    }

    /// Any candidate or active graph excludes configuration replacement.
    pub fn blocks_configuration_mutation(&self) -> bool {
        self.active.is_some()
            || !matches!(
                self.phase,
                GraphCoordinatorPhase::Empty | GraphCoordinatorPhase::Rejected
            )
    }

    /// Any candidate or active graph excludes cached motion-job admission.
    pub fn blocks_job_admission(&self) -> bool {
        self.active.is_some()
            || !matches!(
                self.phase,
                GraphCoordinatorPhase::Empty | GraphCoordinatorPhase::Rejected
            )
    }

    /// Any reader/lifecycle operation serializes ordinary external storage mutation.
    pub fn blocks_storage_mutation(&self) -> bool {
        !matches!(
            self.phase,
            GraphCoordinatorPhase::Empty
                | GraphCoordinatorPhase::Active
                | GraphCoordinatorPhase::Rejected
        )
    }

    /// Exact next Service-domain release used to wake the core-0 executor.
    pub fn next_release_cycle(&self) -> Option<DeviceCycle> {
        if self.phase == GraphCoordinatorPhase::Running {
            self.actor.next_release_cycle()
        } else {
            None
        }
    }

    /// Install a canonical core-1 report only when every exposed identity is known.
    pub fn observe_realtime(
        &mut self,
        config_digest: Digest,
        report: RealtimeGraphReport,
    ) -> Result<(), ()> {
        if config_digest != self.active_config {
            return Err(());
        }
        if report.state == RealtimeGraphState::Empty {
            if self.active.is_some()
                || !matches!(
                    self.phase,
                    GraphCoordinatorPhase::Empty
                        | GraphCoordinatorPhase::Validating
                        | GraphCoordinatorPhase::Aborting
                        | GraphCoordinatorPhase::Rejected
                )
            {
                return Err(());
            }
            self.realtime = report;
            return Ok(());
        }
        let matches_operation = self
            .operation
            .is_some_and(|publication| report_matches(publication, report));
        let matches_active = self
            .active
            .is_some_and(|publication| report_matches(publication, report));
        if !matches_operation && !matches_active {
            // The HTTP install request becomes visible one service turn before
            // Begin can cross the bounded command queue. A periodic report for
            // the preceding core-1 operation may already be in flight; it is
            // canonical but cannot replace the new operation observation.
            if self.phase == GraphCoordinatorPhase::Validating && !self.transfer_sent {
                return Ok(());
            }
            return Err(());
        }
        let active_known = report.active_content_digest.is_zero()
            || self.active.is_some_and(|publication| {
                publication.content_digest() == report.active_content_digest
            })
            || report.state == RealtimeGraphState::Active
                && matches_operation
                && self.operation.is_some_and(|publication| {
                    publication.content_digest() == report.active_content_digest
                });
        if !active_known {
            return Err(());
        }
        self.realtime = report;
        Ok(())
    }

    /// Install one canonical core-1 executor observation and advance only an
    /// exact matching start/stop handshake.
    pub fn observe_realtime_execution(
        &mut self,
        config_digest: Digest,
        report: RealtimeGraphExecutionReport,
    ) -> Result<(), ()> {
        if config_digest != self.active_config {
            return Err(());
        }
        let Some(active) = self.active else {
            if report != RealtimeGraphExecutionReport::empty() {
                return Err(());
            }
            self.realtime_execution = report;
            return Ok(());
        };
        if report.transaction_id != active.transaction_id {
            return Err(());
        }
        if report.run_id != 0 {
            let request = self.current_run.ok_or(())?;
            if report.run_id != request.run_id || report.start_cycle != request.start_cycle {
                return Err(());
            }
        }
        self.realtime_execution = report;
        if report.fault.is_some() {
            if self.current_run.is_none() {
                return Err(());
            }
            self.phase = GraphCoordinatorPhase::ExecutionFaulted;
            self.fault = GraphCoordinatorFault::Execution;
            return Ok(());
        }
        match self.phase {
            GraphCoordinatorPhase::Starting
                if report.actor_phase == GraphActorPhase::Running
                    && report.bridge_phase == GraphBridgePhase::Running
                    && report.run_id != 0 =>
            {
                let run = graph_run_identity(self.current_run.ok_or(())?);
                self.actor.observe_realtime_started(run).map_err(|_| ())?;
                self.phase = GraphCoordinatorPhase::Running;
            }
            GraphCoordinatorPhase::Stopping
                if report.actor_phase == GraphActorPhase::Installed
                    && report.bridge_phase == GraphBridgePhase::Empty
                    && report.run_id != 0 =>
            {
                self.execution_command_sent = false;
                self.phase = GraphCoordinatorPhase::Active;
                self.fault = GraphCoordinatorFault::None;
            }
            _ => {}
        }
        Ok(())
    }

    /// Dispatch one already authenticated graph lifecycle request.
    pub async fn dispatch(
        &mut self,
        cache: &mut selected::StorageBackend,
        request: &ServiceRequest,
        now: DeviceCycle,
        mutation: MutationContext,
    ) -> ServiceResponse {
        let Ok(native) = NativeRequest::decode(request.bytes()) else {
            return ServiceResponse::invalid_native();
        };
        if native.frame.kind != FrameKind::Graph {
            return ServiceResponse::invalid_native();
        }
        let status =
            if native.frame.config_digest != self.active_config || self.active_config.is_zero() {
                StatusCode::Conflict
            } else {
                match native.message.operation {
                    Operation::GraphGet if native.body.is_empty() => StatusCode::Ok,
                    Operation::GraphInstall => self.begin_install(cache, native, mutation).await,
                    Operation::GraphActivate => self.request_activate(native, mutation),
                    Operation::GraphClear => self.request_clear(native, mutation),
                    Operation::GraphStart => self.request_start(native, now, mutation),
                    Operation::GraphStop => self.request_stop(native, mutation),
                    _ => StatusCode::Unsupported,
                }
            };
        self.respond(native, now, status)
    }

    /// Advance at most one media read, command send, or lifecycle transition.
    pub async fn step(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        now: DeviceCycle,
        mutation: MutationContext,
    ) {
        if self.realtime_rejected_operation() {
            self.reject(GraphCoordinatorFault::Realtime);
            return;
        }
        if mutation.validate().is_err() && self.package_lifecycle_busy() {
            return;
        }
        match self.phase {
            GraphCoordinatorPhase::Validating => self.validation_step(cache, endpoint, now).await,
            GraphCoordinatorPhase::Activating => self.activation_step(endpoint, now),
            GraphCoordinatorPhase::Authorizing => self.authorization_step(endpoint, now),
            GraphCoordinatorPhase::Clearing => self.clear_step(endpoint, now),
            GraphCoordinatorPhase::Aborting => self.abort_step(endpoint, now),
            GraphCoordinatorPhase::Starting => {
                if mutation.validate().is_err() {
                    self.cancel_unauthorized_start();
                    self.execution_command_step(endpoint, now, CoreGraphExecutionAction::Stop);
                } else {
                    self.execution_command_step(endpoint, now, CoreGraphExecutionAction::Start);
                }
            }
            GraphCoordinatorPhase::Running => {
                self.execution_release_step(now, mutation.validate().is_ok());
            }
            GraphCoordinatorPhase::Stopping => {
                self.execution_command_step(endpoint, now, CoreGraphExecutionAction::Stop)
            }
            GraphCoordinatorPhase::Empty
            | GraphCoordinatorPhase::CandidateValid
            | GraphCoordinatorPhase::Active
            | GraphCoordinatorPhase::Rejected
            | GraphCoordinatorPhase::ExecutionFaulted => {}
        }
    }

    async fn begin_install(
        &mut self,
        cache: &mut selected::StorageBackend,
        native: NativeRequest<'_>,
        mutation: MutationContext,
    ) -> StatusCode {
        let publication = match GraphPublication::decode(native.body) {
            Ok(publication) => publication,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        if self.lifecycle_busy() {
            return if self.operation == Some(publication) {
                StatusCode::Ok
            } else {
                StatusCode::Busy
            };
        }
        if self.active == Some(publication)
            && self.phase == GraphCoordinatorPhase::Active
            && self.realtime.active_authorized
        {
            return StatusCode::Ok;
        }
        self.validation_status = None;
        self.pending_command = None;
        self.control_sent = false;
        self.transfer_sent = false;
        let authority = GraphRuntimeAuthority {
            device_id: self.device_id,
            capability_digest: selected::PACKAGE.board.capability_digest,
            config_digest: self.active_config,
            implementation_digest: publication.implementation_digest,
        };
        let validation =
            match ServiceGraphValidation::open(cache, publication, authority, GRAPH_RUNTIME_LIMITS)
                .await
            {
                Ok(validation) => validation,
                Err(error) => {
                    let (status, fault) = transfer_error(error);
                    self.operation = Some(publication);
                    self.reject(fault);
                    return status;
                }
            };
        self.operation = Some(publication);
        self.validation_status = Some(validation.status());
        self.validation = Some(validation);
        self.pending_command = None;
        self.control_sent = false;
        self.transfer_sent = false;
        self.phase = GraphCoordinatorPhase::Validating;
        self.fault = GraphCoordinatorFault::None;
        StatusCode::Ok
    }

    fn request_activate(
        &mut self,
        native: NativeRequest<'_>,
        mutation: MutationContext,
    ) -> StatusCode {
        let selection = match GraphSelection::decode(native.body) {
            Ok(selection) => selection,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        if matches!(
            self.phase,
            GraphCoordinatorPhase::Activating | GraphCoordinatorPhase::Authorizing
        ) && self.operation_matches(selection)
        {
            return StatusCode::Ok;
        }
        if self.phase != GraphCoordinatorPhase::CandidateValid
            || !self.operation_matches(selection)
            || !self.realtime_candidate_matches()
        {
            return StatusCode::Conflict;
        }
        self.phase = GraphCoordinatorPhase::Activating;
        self.control_sent = false;
        StatusCode::Ok
    }

    fn request_clear(
        &mut self,
        native: NativeRequest<'_>,
        mutation: MutationContext,
    ) -> StatusCode {
        let selection = match GraphSelection::decode(native.body) {
            Ok(selection) => selection,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        if self.operation_matches(selection)
            && matches!(
                self.phase,
                GraphCoordinatorPhase::Validating
                    | GraphCoordinatorPhase::CandidateValid
                    | GraphCoordinatorPhase::Rejected
                    | GraphCoordinatorPhase::Aborting
            )
        {
            self.phase = GraphCoordinatorPhase::Aborting;
            self.control_sent = false;
            return StatusCode::Ok;
        }
        if self.active_matches(selection) {
            if matches!(self.phase, GraphCoordinatorPhase::Clearing) {
                return StatusCode::Ok;
            }
            if self.lifecycle_busy() {
                return StatusCode::Busy;
            }
            self.operation = self.active;
            self.phase = GraphCoordinatorPhase::Clearing;
            self.control_sent = false;
            return StatusCode::Ok;
        }
        StatusCode::Conflict
    }

    fn request_start(
        &mut self,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        mutation: MutationContext,
    ) -> StatusCode {
        let request = match GraphRunRequest::decode(native.body) {
            Ok(request) => request,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        let Some(active) = self.active else {
            return StatusCode::Conflict;
        };
        if !request_matches_publication(request, active) {
            return StatusCode::Conflict;
        }
        if self.current_run == Some(request)
            && matches!(
                self.phase,
                GraphCoordinatorPhase::Starting
                    | GraphCoordinatorPhase::Running
                    | GraphCoordinatorPhase::ExecutionFaulted
            )
        {
            return StatusCode::Ok;
        }
        if self.phase != GraphCoordinatorPhase::Active {
            return StatusCode::Busy;
        }
        let Some(lead) = request.start_cycle.0.checked_sub(now.0) else {
            return StatusCode::Deadline;
        };
        if lead < MINIMUM_START_LEAD_CYCLES
            || lead > MAXIMUM_START_HORIZON_CYCLES
            || request.run_id <= self.last_started_run_id
            || self.actor.phase() != GraphActorPhase::Installed
        {
            return if request.run_id <= self.last_started_run_id {
                StatusCode::Conflict
            } else {
                StatusCode::Deadline
            };
        }
        self.current_run = Some(request);
        self.last_started_run_id = request.run_id;
        self.service_last_release_tick = None;
        self.execution_command_sent = false;
        match self.actor.prepare_start(graph_run_identity(request), true) {
            Ok(report) => {
                self.service_last_release_tick = report
                    .primed_service_release
                    .map(|release| release.release_tick);
                self.phase = GraphCoordinatorPhase::Starting;
                self.fault = GraphCoordinatorFault::None;
                StatusCode::Ok
            }
            Err(_) => {
                self.phase = GraphCoordinatorPhase::ExecutionFaulted;
                self.fault = GraphCoordinatorFault::Execution;
                StatusCode::Internal
            }
        }
    }

    fn request_stop(
        &mut self,
        native: NativeRequest<'_>,
        _mutation: MutationContext,
    ) -> StatusCode {
        let request = match GraphRunRequest::decode(native.body) {
            Ok(request) => request,
            Err(_) => return StatusCode::InvalidRequest,
        };
        let Some(active) = self.active else {
            return StatusCode::Conflict;
        };
        if !request_matches_publication(request, active) || self.current_run != Some(request) {
            return StatusCode::Conflict;
        }
        if self.phase == GraphCoordinatorPhase::Stopping {
            return StatusCode::Ok;
        }
        if self.phase == GraphCoordinatorPhase::Active
            && self.actor.phase() == GraphActorPhase::Installed
            && self.actor.bridge_phase() == GraphBridgePhase::Empty
        {
            return StatusCode::Ok;
        }
        if !matches!(
            self.phase,
            GraphCoordinatorPhase::Starting
                | GraphCoordinatorPhase::Running
                | GraphCoordinatorPhase::ExecutionFaulted
        ) {
            return StatusCode::Conflict;
        }
        if self.actor.stop(graph_run_identity(request)).is_err() {
            self.phase = GraphCoordinatorPhase::ExecutionFaulted;
            self.fault = GraphCoordinatorFault::Execution;
            return StatusCode::Internal;
        }
        self.phase = GraphCoordinatorPhase::Stopping;
        self.execution_command_sent = false;
        StatusCode::Ok
    }

    async fn validation_step(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        now: DeviceCycle,
    ) {
        if self.pending_command.is_none() {
            let Some(validation) = self.validation.as_mut() else {
                self.reject(GraphCoordinatorFault::Internal);
                return;
            };
            let next = validation.next(cache).await;
            self.validation_status = Some(validation.status());
            match next {
                Ok(command) => self.pending_command = command,
                Err(error) => {
                    let (_, fault) = transfer_error(error);
                    self.reject(fault);
                    return;
                }
            }
        }
        if let Some(command) = self.pending_command {
            match self.try_send(endpoint, command, now) {
                Ok(true) => {
                    self.pending_command = None;
                    self.transfer_sent = true;
                }
                Ok(false) => return,
                Err(()) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            }
        }
        let complete = self.validation_status.is_some_and(|status| {
            status.state == ServiceGraphValidationState::Complete && status.identity.is_some()
        });
        if complete && self.realtime_candidate_matches() {
            self.phase = GraphCoordinatorPhase::CandidateValid;
        }
    }

    fn activation_step(&mut self, endpoint: &mut DefaultServiceEndpoint, now: DeviceCycle) {
        let Some(publication) = self.operation else {
            self.reject(GraphCoordinatorFault::Internal);
            return;
        };
        if !self.control_sent {
            let command = match CoreGraphCommand::activate(publication) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(true) => self.control_sent = true,
                Ok(false) => return,
                Err(()) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            }
        }
        if self.realtime_active_matches(false) {
            self.phase = GraphCoordinatorPhase::Authorizing;
            self.control_sent = false;
        }
    }

    fn authorization_step(&mut self, endpoint: &mut DefaultServiceEndpoint, now: DeviceCycle) {
        let Some(publication) = self.operation else {
            self.reject(GraphCoordinatorFault::Internal);
            return;
        };
        if !self.control_sent {
            let command = match CoreGraphCommand::authorize(publication) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(true) => self.control_sent = true,
                Ok(false) => return,
                Err(()) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            }
        }
        if self.realtime_active_matches(true) {
            if self.install_active_actor(publication).is_err() {
                self.reject(GraphCoordinatorFault::Internal);
                return;
            }
            self.active = Some(publication);
            self.validation = None;
            self.pending_command = None;
            self.phase = GraphCoordinatorPhase::Active;
            self.fault = GraphCoordinatorFault::None;
        }
    }

    fn clear_step(&mut self, endpoint: &mut DefaultServiceEndpoint, now: DeviceCycle) {
        let Some(publication) = self.operation else {
            self.reject(GraphCoordinatorFault::Internal);
            return;
        };
        if !self.control_sent {
            let command = match CoreGraphCommand::clear(publication) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(true) => self.control_sent = true,
                Ok(false) => return,
                Err(()) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            }
        }
        if self.realtime.state == RealtimeGraphState::Cleared
            && self.realtime.content_digest == publication.content_digest()
            && self.realtime.package_digest == publication.package_digest
        {
            if self.clear_active_actor().is_err() {
                self.reject(GraphCoordinatorFault::Internal);
                return;
            }
            self.active = None;
            self.finish_operation();
        }
    }

    fn abort_step(&mut self, endpoint: &mut DefaultServiceEndpoint, now: DeviceCycle) {
        let Some(publication) = self.operation else {
            self.finish_operation();
            return;
        };
        if !self.transfer_sent {
            self.finish_operation();
            return;
        }
        if !self.control_sent {
            let command = match CoreGraphCommand::abort(publication) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(true) => self.control_sent = true,
                Ok(false) => return,
                Err(()) => {
                    self.reject(GraphCoordinatorFault::Protocol);
                    return;
                }
            }
        }
        let settled = if let Some(active) = self.active {
            self.realtime.state == RealtimeGraphState::Active
                && self.realtime.content_digest == active.content_digest()
        } else {
            self.realtime.state == RealtimeGraphState::Empty
        };
        if settled {
            self.finish_operation();
        }
    }

    fn execution_command_step(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        now: DeviceCycle,
        action: CoreGraphExecutionAction,
    ) {
        if self.execution_command_sent {
            return;
        }
        let Some(request) = self.current_run else {
            self.phase = GraphCoordinatorPhase::ExecutionFaulted;
            self.fault = GraphCoordinatorFault::Internal;
            return;
        };
        let command = match action {
            CoreGraphExecutionAction::Start => CoreGraphExecutionCommand::start(request),
            CoreGraphExecutionAction::Stop => CoreGraphExecutionCommand::stop(request),
        };
        let Ok(command) = command else {
            self.phase = GraphCoordinatorPhase::ExecutionFaulted;
            self.fault = GraphCoordinatorFault::Internal;
            return;
        };
        match self.try_send_execution(endpoint, command, now) {
            Ok(true) => self.execution_command_sent = true,
            Ok(false) => {}
            Err(()) => {
                self.phase = GraphCoordinatorPhase::ExecutionFaulted;
                self.fault = GraphCoordinatorFault::Protocol;
            }
        }
    }

    fn execution_release_step(&mut self, now: DeviceCycle, execution_allowed: bool) {
        if self.actor.phase() != GraphActorPhase::Running
            || self.actor.bridge_phase() != GraphBridgePhase::Running
        {
            return;
        }
        let window = match self.actor.next_release_window() {
            Ok(window) => window,
            Err(_) => {
                self.phase = GraphCoordinatorPhase::ExecutionFaulted;
                self.fault = GraphCoordinatorFault::Execution;
                return;
            }
        };
        let Some(window) = window else {
            return;
        };
        if execution_allowed && now < window.scheduled_cycle {
            return;
        }
        let release_cycle = if execution_allowed && now <= window.latest_dispatch_cycle {
            window.scheduled_cycle
        } else if execution_allowed {
            now
        } else {
            window.scheduled_cycle
        };
        match self.actor.release(release_cycle, execution_allowed) {
            Ok(report) => self.service_last_release_tick = Some(report.release_tick),
            Err(_) => {
                self.phase = GraphCoordinatorPhase::ExecutionFaulted;
                self.fault = GraphCoordinatorFault::Execution;
            }
        }
    }

    fn cancel_unauthorized_start(&mut self) {
        let Some(request) = self.current_run else {
            self.phase = GraphCoordinatorPhase::ExecutionFaulted;
            self.fault = GraphCoordinatorFault::Internal;
            return;
        };
        if self.actor.stop(graph_run_identity(request)).is_err() {
            self.phase = GraphCoordinatorPhase::ExecutionFaulted;
            self.fault = GraphCoordinatorFault::Execution;
            return;
        }
        self.phase = GraphCoordinatorPhase::Stopping;
        self.execution_command_sent = false;
    }

    fn try_send(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        command: CoreGraphCommand,
        now: DeviceCycle,
    ) -> Result<bool, ()> {
        let encoded = command.encode().map_err(|_| ())?;
        let sequence = next_nonzero(self.command_sequence);
        let frame = IntercoreFrame::new(
            FrameKind::Graph,
            sequence,
            now,
            self.active_config,
            encoded.as_bytes(),
        )
        .map_err(|_| ())?;
        if endpoint.try_send_command(frame).is_err() {
            return Ok(false);
        }
        self.command_sequence = sequence;
        Ok(true)
    }

    fn try_send_execution(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        command: CoreGraphExecutionCommand,
        now: DeviceCycle,
    ) -> Result<bool, ()> {
        let encoded = command.encode().map_err(|_| ())?;
        let sequence = next_nonzero(self.command_sequence);
        let frame = IntercoreFrame::new(
            FrameKind::Graph,
            sequence,
            now,
            self.active_config,
            &encoded,
        )
        .map_err(|_| ())?;
        if endpoint.try_send_command(frame).is_err() {
            return Ok(false);
        }
        self.command_sequence = sequence;
        Ok(true)
    }

    fn respond(
        &self,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        status: StatusCode,
    ) -> ServiceResponse {
        let body = match self.report().encode() {
            Ok(body) => body,
            Err(_) => return ServiceResponse::invalid_native(),
        };
        ServiceResponse::native(native, now, status, &body)
            .unwrap_or_else(|_| ServiceResponse::invalid_native())
    }

    fn report(&self) -> GraphCoordinatorReport {
        let publication = self.operation.or(self.active);
        let status = self.validation_status;
        GraphCoordinatorReport {
            phase: self.phase,
            fault: self.fault,
            transaction_id: publication.map_or(0, |value| value.transaction_id),
            content_digest: publication.map_or(Digest::ZERO, GraphPublication::content_digest),
            package_digest: publication.map_or(Digest::ZERO, |value| value.package_digest),
            validated_bytes: status.map_or_else(
                || {
                    if publication.is_some()
                        && matches!(
                            self.phase,
                            GraphCoordinatorPhase::Active
                                | GraphCoordinatorPhase::Starting
                                | GraphCoordinatorPhase::Running
                                | GraphCoordinatorPhase::Stopping
                                | GraphCoordinatorPhase::ExecutionFaulted
                        )
                    {
                        GRAPH_IR_PACKAGE_BYTES as u32
                    } else {
                        0
                    }
                },
                |status| status.validated_bytes,
            ),
            storage_chunks_read: status.map_or(0, |status| status.storage_chunks_read),
            active_content_digest: self
                .active
                .map_or(Digest::ZERO, GraphPublication::content_digest),
            realtime: self.realtime,
            execution: self.execution_report(),
        }
    }

    fn execution_report(&self) -> GraphExecutionReport {
        let Some(active) = self.active else {
            return GraphExecutionReport::empty();
        };
        let run = self.current_run;
        let realtime_matches_run = run.is_some_and(|request| {
            self.realtime_execution.transaction_id == active.transaction_id
                && self.realtime_execution.run_id == request.run_id
                && self.realtime_execution.start_cycle == request.start_cycle
        });
        let realtime_phase = if realtime_matches_run {
            self.realtime_execution.actor_phase
        } else {
            GraphActorPhase::Installed
        };
        let fault = self.actor.fault_after(0).or(if realtime_matches_run {
            self.realtime_execution.fault
        } else {
            None
        });
        GraphExecutionReport {
            service_phase: self.actor.phase(),
            realtime_phase,
            bridge_phase: self.actor.bridge_phase(),
            run_id: run.map_or(0, |request| request.run_id),
            start_cycle: run.map_or(DeviceCycle(0), |request| request.start_cycle),
            service_next_cycle: self.actor.next_release_cycle(),
            realtime_next_cycle: if realtime_matches_run {
                self.realtime_execution.next_release_cycle
            } else {
                None
            },
            service_last_tick: self.service_last_release_tick,
            realtime_last_tick: if realtime_matches_run {
                self.realtime_execution.last_release_tick
            } else {
                None
            },
            fault,
        }
    }

    fn lifecycle_busy(&self) -> bool {
        matches!(
            self.phase,
            GraphCoordinatorPhase::Validating
                | GraphCoordinatorPhase::CandidateValid
                | GraphCoordinatorPhase::Activating
                | GraphCoordinatorPhase::Authorizing
                | GraphCoordinatorPhase::Clearing
                | GraphCoordinatorPhase::Aborting
                | GraphCoordinatorPhase::Starting
                | GraphCoordinatorPhase::Running
                | GraphCoordinatorPhase::Stopping
                | GraphCoordinatorPhase::ExecutionFaulted
        )
    }

    fn package_lifecycle_busy(&self) -> bool {
        matches!(
            self.phase,
            GraphCoordinatorPhase::Validating
                | GraphCoordinatorPhase::CandidateValid
                | GraphCoordinatorPhase::Activating
                | GraphCoordinatorPhase::Authorizing
                | GraphCoordinatorPhase::Clearing
                | GraphCoordinatorPhase::Aborting
        )
    }

    fn operation_matches(&self, selection: GraphSelection) -> bool {
        self.operation.is_some_and(|publication| {
            publication.transaction_id == selection.transaction_id
                && publication.content_digest() == selection.content_digest
                && publication.package_digest == selection.package_digest
        })
    }

    fn active_matches(&self, selection: GraphSelection) -> bool {
        self.active.is_some_and(|publication| {
            publication.transaction_id == selection.transaction_id
                && publication.content_digest() == selection.content_digest
                && publication.package_digest == selection.package_digest
        })
    }

    fn realtime_candidate_matches(&self) -> bool {
        self.operation.is_some_and(|publication| {
            self.realtime.state == RealtimeGraphState::CandidateValid
                && self.realtime.transaction_id == publication.transaction_id
                && self.realtime.content_digest == publication.content_digest()
                && self.realtime.package_digest == publication.package_digest
                && self.realtime.consumed_bytes == GRAPH_IR_PACKAGE_BYTES as u32
                && self.realtime.summary.is_some()
        })
    }

    fn realtime_active_matches(&self, authorized: bool) -> bool {
        self.operation.is_some_and(|publication| {
            self.realtime.state == RealtimeGraphState::Active
                && self.realtime.transaction_id == publication.transaction_id
                && self.realtime.content_digest == publication.content_digest()
                && self.realtime.package_digest == publication.package_digest
                && self.realtime.active_content_digest == publication.content_digest()
                && self.realtime.active_authorized == authorized
        })
    }

    fn realtime_rejected_operation(&self) -> bool {
        matches!(
            self.phase,
            GraphCoordinatorPhase::Validating
                | GraphCoordinatorPhase::CandidateValid
                | GraphCoordinatorPhase::Activating
                | GraphCoordinatorPhase::Authorizing
        ) && self.operation.is_some_and(|publication| {
            self.realtime.state == RealtimeGraphState::Rejected
                && self.realtime.transaction_id == publication.transaction_id
                && self.realtime.content_digest == publication.content_digest()
                && self.realtime.package_digest == publication.package_digest
        })
    }

    fn install_active_actor(&mut self, publication: GraphPublication) -> Result<(), ()> {
        let validation = self.validation.as_ref().ok_or(())?;
        let identity = validation.status().identity.ok_or(())?;
        if identity.transaction_id != publication.transaction_id
            || identity.content_digest != publication.content_digest()
            || identity.package_digest != publication.package_digest
            || identity.implementation_digest != publication.implementation_digest
        {
            return Err(());
        }
        if self.actor.installed_identity() == Some(identity) {
            return Ok(());
        }
        match self.actor.phase() {
            GraphActorPhase::Empty => {}
            GraphActorPhase::Installed => self.actor.clear(true).map_err(|_| ())?,
            GraphActorPhase::Prepared | GraphActorPhase::Running | GraphActorPhase::Faulted => {
                return Err(());
            }
        }
        let package = validation.validated_package_bytes().ok_or(())?;
        let authority = GraphRuntimeAuthority {
            device_id: self.device_id,
            capability_digest: selected::PACKAGE.board.capability_digest,
            config_digest: self.active_config,
            implementation_digest: publication.implementation_digest,
        };
        let report = self
            .actor
            .install(
                package,
                publication.transaction_id,
                publication.content_digest(),
                publication.package_digest,
                authority,
                GRAPH_RUNTIME_LIMITS,
                true,
            )
            .map_err(|_| ())?;
        if report.package_digest != publication.package_digest || report.usage != identity.usage {
            return Err(());
        }
        self.current_run = None;
        self.last_started_run_id = 0;
        self.service_last_release_tick = None;
        self.execution_command_sent = false;
        Ok(())
    }

    fn clear_active_actor(&mut self) -> Result<(), ()> {
        match self.actor.phase() {
            GraphActorPhase::Empty => Ok(()),
            GraphActorPhase::Installed => {
                self.actor.clear(true).map_err(|_| ())?;
                self.current_run = None;
                self.last_started_run_id = 0;
                self.service_last_release_tick = None;
                self.execution_command_sent = false;
                self.realtime_execution = RealtimeGraphExecutionReport::empty();
                Ok(())
            }
            GraphActorPhase::Prepared | GraphActorPhase::Running | GraphActorPhase::Faulted => {
                Err(())
            }
        }
    }

    fn reject(&mut self, fault: GraphCoordinatorFault) {
        self.validation = None;
        self.pending_command = None;
        self.control_sent = false;
        self.phase = GraphCoordinatorPhase::Rejected;
        self.fault = fault;
    }

    fn finish_operation(&mut self) {
        self.operation = self.active;
        self.validation = None;
        self.validation_status = None;
        self.pending_command = None;
        self.control_sent = false;
        self.transfer_sent = false;
        self.fault = GraphCoordinatorFault::None;
        self.phase = if self.active.is_some() {
            GraphCoordinatorPhase::Active
        } else {
            GraphCoordinatorPhase::Empty
        };
    }
}

fn graph_run_identity(request: GraphRunRequest) -> GraphRunIdentity {
    GraphRunIdentity {
        transaction_id: request.transaction_id,
        run_id: request.run_id,
        content_digest: request.content_digest,
        package_digest: request.package_digest,
        start_cycle: request.start_cycle,
    }
}

fn request_matches_identity(request: GraphRunRequest, identity: GraphDeploymentIdentity) -> bool {
    request.transaction_id == identity.transaction_id
        && request.content_digest == identity.content_digest
        && request.package_digest == identity.package_digest
        && request.implementation_digest == identity.implementation_digest
}

fn request_matches_publication(request: GraphRunRequest, publication: GraphPublication) -> bool {
    request.transaction_id == publication.transaction_id
        && request.content_digest == publication.content_digest()
        && request.package_digest == publication.package_digest
        && request.implementation_digest == publication.implementation_digest
}

fn command_matches_identity(command: CoreGraphCommand, identity: GraphDeploymentIdentity) -> bool {
    command.transaction_id == identity.transaction_id
        && command.content_digest == identity.content_digest
        && command.package_digest == identity.package_digest
        && command.implementation_digest == identity.implementation_digest
}

fn report_matches(publication: GraphPublication, report: RealtimeGraphReport) -> bool {
    publication.transaction_id == report.transaction_id
        && publication.content_digest() == report.content_digest
        && publication.package_digest == report.package_digest
}

fn transfer_error<E>(error: ServiceGraphTransferError<E>) -> (StatusCode, GraphCoordinatorFault) {
    match error {
        ServiceGraphTransferError::Request(_) => {
            (StatusCode::InvalidRequest, GraphCoordinatorFault::Request)
        }
        ServiceGraphTransferError::Validation(_) => (
            StatusCode::InvalidRequest,
            GraphCoordinatorFault::ServiceValidation,
        ),
        ServiceGraphTransferError::State => (StatusCode::Internal, GraphCoordinatorFault::Internal),
        ServiceGraphTransferError::Storage(error) => match error {
            ProvisionedCacheError::NotDiscovered | ProvisionedCacheError::NotMounted => {
                (StatusCode::Unsupported, GraphCoordinatorFault::Storage)
            }
            ProvisionedCacheError::Media(MediaError::PublishedNotFound) => {
                (StatusCode::NotFound, GraphCoordinatorFault::Storage)
            }
            ProvisionedCacheError::Media(MediaError::Corrupt(_))
            | ProvisionedCacheError::Locator(_)
            | ProvisionedCacheError::MediaIdentity => {
                (StatusCode::Integrity, GraphCoordinatorFault::Storage)
            }
            ProvisionedCacheError::Conflict | ProvisionedCacheError::RecoveryIntent => {
                (StatusCode::Conflict, GraphCoordinatorFault::Storage)
            }
            ProvisionedCacheError::Geometry
            | ProvisionedCacheError::Mutation(_)
            | ProvisionedCacheError::Arithmetic => {
                (StatusCode::InvalidRequest, GraphCoordinatorFault::Storage)
            }
            ProvisionedCacheError::Device(_)
            | ProvisionedCacheError::DeviceStateUnavailable
            | ProvisionedCacheError::Media(_) => {
                (StatusCode::Internal, GraphCoordinatorFault::Storage)
            }
        },
    }
}

const fn next_nonzero(value: u32) -> u32 {
    let next = value.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
