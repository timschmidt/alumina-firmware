//! Core-0 machine-configuration ownership, recovery, and authenticated routing.

use alloc::boxed::Box;

use alumina_config::{
    CONFIGURATION_COORDINATOR_STATUS_BYTES, ConfigurationCoordinatorFault,
    ConfigurationCoordinatorFlags, ConfigurationCoordinatorPhase, ConfigurationCoordinatorStatus,
    ConfigurationIdentity, ConfigurationPublication, ConfigurationSelection,
    ConfigurationTransferError, CoreConfigurationCommand, RealtimeConfigurationReport,
    RealtimeConfigurationState, ServiceConfigurationState, ServiceConfigurationStatus,
    ServiceConfigurationValidation,
};
use alumina_motion::CachedServoConfiguration;
use alumina_protocol::{
    DeviceCycle, Digest, FrameHeader, FrameKind, MessageHeader, Operation, StatusCode,
};
use alumina_runtime::{DefaultServiceEndpoint, IntercoreFrame};
use alumina_service::{
    MAX_SERVICE_RESPONSE_BYTES, NativeRequest, ServiceRequest, ServiceRequestKind, ServiceResponse,
};
use alumina_storage::MutationContext;
use alumina_storage::media::{ConfigurationTransition, DurableConfigurationSelection, MediaError};
use alumina_storage::provisioning::ProvisionedCacheError;

use crate::hardware::selected;
use crate::poll_boundary::poll_boundary;

type Validation = ServiceConfigurationValidation<'static, { selected::CONFIGURATION_BINDINGS }>;

const _: () = assert!(
    FrameHeader::WIRE_LEN + MessageHeader::WIRE_LEN + CONFIGURATION_COORDINATOR_STATUS_BYTES
        <= MAX_SERVICE_RESPONSE_BYTES
);

/// Sole service-core owner of configuration media, transfer, and commit state.
pub struct ConfigurationService {
    phase: ConfigurationCoordinatorPhase,
    fault: ConfigurationCoordinatorFault,
    publication: Option<ConfigurationPublication>,
    // Validation owns a media chunk and the complete candidate configuration.
    // It exists only during recovery/replacement and must not inflate the
    // permanent idle service actor on memory-constrained classic ESP32s.
    validation: Option<Box<Validation>>,
    validation_status: Option<ServiceConfigurationStatus>,
    service_identity: Option<ConfigurationIdentity>,
    realtime: RealtimeConfigurationReport,
    active: Option<DurableConfigurationSelection>,
    active_identity: Option<ConfigurationIdentity>,
    active_servo_configuration: Option<CachedServoConfiguration<{ selected::JOB_AXES }>>,
    durable_pending: Option<ConfigurationTransition>,
    durable_prepared: bool,
    boot_orphan: Option<ConfigurationTransition>,
    journal_loaded: bool,
    bootstrapped: bool,
    boot_recovery: bool,
    pending_command: Option<CoreConfigurationCommand>,
    command_sequence: u32,
    control_sent: bool,
    transfer_sent: bool,
}

impl ConfigurationService {
    /// Starts fail-closed until mounted-media selector replay completes.
    pub const fn new() -> Self {
        Self {
            phase: ConfigurationCoordinatorPhase::Empty,
            fault: ConfigurationCoordinatorFault::None,
            publication: None,
            validation: None,
            validation_status: None,
            service_identity: None,
            realtime: RealtimeConfigurationReport::empty(),
            active: None,
            active_identity: None,
            active_servo_configuration: None,
            durable_pending: None,
            durable_prepared: false,
            boot_orphan: None,
            journal_loaded: false,
            bootstrapped: false,
            boot_recovery: false,
            pending_command: None,
            command_sequence: 0,
            control_sent: false,
            transfer_sent: false,
        }
    }

    /// Establishes the boot state when discovery proved that no mounted cache
    /// can contain a durable configuration selector.
    ///
    /// Detached media is an authoritative empty state. A failed transport is
    /// retained as a storage rejection, but either result is fully bootstrapped
    /// and therefore does not enter the ready-media replay future.
    pub fn establish_unmounted_bootstrap(&mut self, storage_faulted: bool) {
        if self.bootstrapped {
            return;
        }
        self.journal_loaded = true;
        self.bootstrapped = true;
        self.boot_recovery = false;
        self.phase = if storage_faulted {
            ConfigurationCoordinatorPhase::Rejected
        } else {
            ConfigurationCoordinatorPhase::Empty
        };
        self.fault = if storage_faulted {
            ConfigurationCoordinatorFault::Storage
        } else {
            ConfigurationCoordinatorFault::None
        };
    }

    /// Whether the coordinator has one bounded background transition to run.
    ///
    /// Keeping the stable `Empty`, `CandidateValid`, `Active`, and `Rejected`
    /// phases out of the async step path is material on classic ESP32: the
    /// validation future has a deliberately large, bounded stack frame even
    /// when its state-machine arm would immediately return.
    pub fn requires_step(&self) -> bool {
        !self.bootstrapped
            || matches!(
                self.phase,
                ConfigurationCoordinatorPhase::Recovering
                    | ConfigurationCoordinatorPhase::Validating
                    | ConfigurationCoordinatorPhase::Preparing
                    | ConfigurationCoordinatorPhase::Activating
                    | ConfigurationCoordinatorPhase::Committing
                    | ConfigurationCoordinatorPhase::Authorizing
                    | ConfigurationCoordinatorPhase::Clearing
                    | ConfigurationCoordinatorPhase::Aborting
            )
    }

    /// Whether a valid universal request selects the configuration family.
    pub fn handles(request: &ServiceRequest) -> bool {
        request.kind() == ServiceRequestKind::NativeFrame
            && NativeRequest::decode(request.bytes())
                .is_ok_and(|request| request.frame.kind == FrameKind::Configuration)
    }

    /// Processes one authenticated lifecycle request without waiting for the
    /// multi-step core/media transaction to finish.
    #[inline(never)]
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
        if native.frame.kind != FrameKind::Configuration {
            return ServiceResponse::invalid_native();
        }
        let status = match native.message.operation {
            Operation::ConfigurationGet if native.body.is_empty() => {
                if !self.bootstrapped {
                    StatusCode::Busy
                } else if !native.frame.config_digest.is_zero()
                    && native.frame.config_digest != self.active_digest()
                {
                    StatusCode::Conflict
                } else {
                    StatusCode::Ok
                }
            }
            Operation::ConfigurationValidate => {
                poll_boundary(self.begin_validation(cache, native, mutation)).await
            }
            Operation::ConfigurationCommit => self.request_commit(native, mutation),
            Operation::ConfigurationRollback => self.request_rollback(native, mutation),
            _ => StatusCode::Unsupported,
        };
        self.respond(native, now, status)
    }

    /// Advances at most one validation chunk/command or one lifecycle action.
    #[inline(never)]
    pub async fn step(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        now: DeviceCycle,
        mutation: MutationContext,
    ) {
        if !self.bootstrapped {
            poll_boundary(self.bootstrap_step(cache, mutation)).await;
            if !self.bootstrapped {
                return;
            }
        }
        if let Some(fault) = self.realtime_operation_rejection() {
            self.reject(fault);
            return;
        }
        if mutation.validate().is_err() && self.lifecycle_busy() {
            return;
        }
        match self.phase {
            ConfigurationCoordinatorPhase::Recovering
            | ConfigurationCoordinatorPhase::Validating => {
                poll_boundary(self.validation_step(cache, endpoint, now)).await;
            }
            ConfigurationCoordinatorPhase::Preparing => {
                poll_boundary(self.prepare_activation(cache, mutation)).await;
            }
            ConfigurationCoordinatorPhase::Activating => {
                self.activation_step(endpoint, now);
            }
            ConfigurationCoordinatorPhase::Committing => {
                poll_boundary(self.commit_activation(cache, mutation)).await;
            }
            ConfigurationCoordinatorPhase::Authorizing => {
                self.authorization_step(endpoint, now);
            }
            ConfigurationCoordinatorPhase::Clearing => {
                poll_boundary(self.clear_step(cache, endpoint, now, mutation)).await;
            }
            ConfigurationCoordinatorPhase::Aborting => {
                poll_boundary(self.abort_step(cache, endpoint, now, mutation)).await;
            }
            ConfigurationCoordinatorPhase::Empty
            | ConfigurationCoordinatorPhase::CandidateValid
            | ConfigurationCoordinatorPhase::Active
            | ConfigurationCoordinatorPhase::Rejected => {}
        }
    }

    /// Installs one canonical report only when its active identity is known to
    /// the durable selector or current operation.
    pub fn observe_realtime(&mut self, report: RealtimeConfigurationReport) -> Result<(), ()> {
        if !report.active_digest.is_zero()
            && !self.identity_is_known(report.active_digest, report.active_bytes)
        {
            return Err(());
        }
        self.realtime = report;
        Ok(())
    }

    /// Digest that both job actors may use now; zero blocks admission.
    pub fn authorized_digest(&self) -> Digest {
        let Some(active) = self.active else {
            return Digest::ZERO;
        };
        if self.lifecycle_busy() || !self.realtime_authorizes_durable() {
            Digest::ZERO
        } else {
            active.publication().object.content.digest
        }
    }

    /// Exact service-core identity admitted to jobs only after the durable
    /// selection and independently validated realtime identity agree.
    pub fn authorized_identity(&self) -> Option<ConfigurationIdentity> {
        let digest = self.authorized_digest();
        self.active_identity
            .filter(|identity| !digest.is_zero() && identity.digest == digest)
    }

    /// Compact independently validated servo-admission facts paired with the
    /// durably authorized identity.
    ///
    /// This value authorizes only bounded service-side admission checks. It
    /// never owns or constructs a physical output peripheral.
    pub fn authorized_servo_configuration(
        &self,
    ) -> Option<&CachedServoConfiguration<{ selected::JOB_AXES }>> {
        let digest = self.authorized_digest();
        self.active_servo_configuration
            .as_ref()
            .filter(|configuration| {
                !digest.is_zero() && configuration.configuration_digest() == digest
            })
    }

    /// True while a configuration operation excludes new job preparation.
    pub fn blocks_job_admission(&self) -> bool {
        self.lifecycle_busy()
            || self.durable_pending.is_some()
            || self.boot_orphan.is_some()
            || self.active.is_some() && !self.realtime_authorizes_durable()
            || self.active.is_none() && !self.realtime.active_digest.is_zero()
    }

    /// Whether destructive cache reprovisioning would invalidate an active
    /// selector, including during boot recovery.
    pub const fn has_durable_active(&self) -> bool {
        self.active.is_some()
    }

    fn lifecycle_busy(&self) -> bool {
        matches!(
            self.phase,
            ConfigurationCoordinatorPhase::Recovering
                | ConfigurationCoordinatorPhase::Validating
                | ConfigurationCoordinatorPhase::CandidateValid
                | ConfigurationCoordinatorPhase::Preparing
                | ConfigurationCoordinatorPhase::Activating
                | ConfigurationCoordinatorPhase::Committing
                | ConfigurationCoordinatorPhase::Authorizing
                | ConfigurationCoordinatorPhase::Clearing
                | ConfigurationCoordinatorPhase::Aborting
        )
    }

    async fn bootstrap_step(
        &mut self,
        cache: &mut selected::StorageBackend,
        mutation: MutationContext,
    ) {
        if !self.journal_loaded {
            let journal = match cache.configuration_journal() {
                Ok(journal) => journal,
                Err(ProvisionedCacheError::NotMounted) => return,
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Storage);
                    self.bootstrapped = true;
                    return;
                }
            };
            self.active = journal.active;
            self.boot_orphan = journal.pending;
            self.journal_loaded = true;
            self.boot_recovery = self.active.is_some() || self.boot_orphan.is_some();
            if let Some(orphan) = self.boot_orphan {
                self.publication = Some(publication_from_durable(orphan.selection()));
                self.phase = ConfigurationCoordinatorPhase::Aborting;
            }
        }

        if let Some(orphan) = self.boot_orphan {
            if mutation.validate().is_err() {
                return;
            }
            match poll_boundary(cache.abort_configuration_transition(orphan, mutation)).await {
                Ok(_) => {
                    self.boot_orphan = None;
                    self.publication = None;
                    self.phase = ConfigurationCoordinatorPhase::Empty;
                }
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Durability);
                    self.bootstrapped = true;
                    return;
                }
            }
        }

        let Some(active) = self.active else {
            self.bootstrapped = true;
            self.boot_recovery = false;
            self.phase = ConfigurationCoordinatorPhase::Empty;
            return;
        };
        let publication = publication_from_durable(active);
        match poll_boundary(Validation::open(cache, selected::PACKAGE, publication)).await {
            Ok(validation) => {
                self.publication = Some(publication);
                self.validation_status = Some(validation.status());
                self.validation = Some(Box::new(validation));
                self.phase = ConfigurationCoordinatorPhase::Recovering;
                self.fault = ConfigurationCoordinatorFault::None;
                self.bootstrapped = true;
            }
            Err(_) => {
                self.publication = Some(publication);
                self.reject(ConfigurationCoordinatorFault::ServiceValidation);
                self.bootstrapped = true;
            }
        }
    }

    async fn begin_validation(
        &mut self,
        cache: &mut selected::StorageBackend,
        native: NativeRequest<'_>,
        mutation: MutationContext,
    ) -> StatusCode {
        let publication = match ConfigurationPublication::decode(native.body) {
            Ok(publication) => publication,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if native.frame.config_digest != self.active_digest() {
            return StatusCode::Conflict;
        }
        if !self.bootstrapped
            || self.durable_pending.is_some()
            || self.boot_orphan.is_some()
            || !self.realtime_matches_durable_or_empty()
        {
            return StatusCode::Busy;
        }
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        if self.lifecycle_busy() {
            return if self.publication == Some(publication) {
                StatusCode::Ok
            } else {
                StatusCode::Busy
            };
        }
        let validation =
            match poll_boundary(Validation::open(cache, selected::PACKAGE, publication)).await {
                Ok(validation) => validation,
                Err(error) => {
                    let (status, fault) = transfer_error(error);
                    self.publication = Some(publication);
                    self.reject(fault);
                    return status;
                }
            };
        self.publication = Some(publication);
        self.validation_status = Some(validation.status());
        self.validation = Some(Box::new(validation));
        self.service_identity = None;
        self.durable_pending = None;
        self.durable_prepared = false;
        self.pending_command = None;
        self.control_sent = false;
        self.transfer_sent = false;
        self.boot_recovery = false;
        self.phase = ConfigurationCoordinatorPhase::Validating;
        self.fault = ConfigurationCoordinatorFault::None;
        StatusCode::Ok
    }

    fn request_commit(
        &mut self,
        native: NativeRequest<'_>,
        mutation: MutationContext,
    ) -> StatusCode {
        let selection = match ConfigurationSelection::decode(native.body) {
            Ok(selection) => selection,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if native.frame.config_digest != selection.digest {
            return StatusCode::Conflict;
        }
        if !self.bootstrapped {
            return StatusCode::Busy;
        }
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        if matches!(
            self.phase,
            ConfigurationCoordinatorPhase::Preparing
                | ConfigurationCoordinatorPhase::Activating
                | ConfigurationCoordinatorPhase::Committing
                | ConfigurationCoordinatorPhase::Authorizing
        ) && self.selection_matches_operation(selection)
        {
            return StatusCode::Ok;
        }
        if self.phase != ConfigurationCoordinatorPhase::CandidateValid
            || !self.selection_matches_operation(selection)
            || !self.realtime_candidate_matches()
        {
            return StatusCode::Conflict;
        }
        self.phase = ConfigurationCoordinatorPhase::Preparing;
        self.control_sent = false;
        StatusCode::Ok
    }

    fn request_rollback(
        &mut self,
        native: NativeRequest<'_>,
        mutation: MutationContext,
    ) -> StatusCode {
        let selection = match ConfigurationSelection::decode(native.body) {
            Ok(selection) => selection,
            Err(_) => return StatusCode::InvalidRequest,
        };
        if native.frame.config_digest != selection.digest {
            return StatusCode::Conflict;
        }
        if !self.bootstrapped {
            return StatusCode::Busy;
        }
        if mutation.validate().is_err() {
            return StatusCode::ForbiddenState;
        }
        if self.publication_matches_selection(selection) {
            match self.phase {
                ConfigurationCoordinatorPhase::Validating
                | ConfigurationCoordinatorPhase::CandidateValid
                | ConfigurationCoordinatorPhase::Preparing => {
                    self.phase = ConfigurationCoordinatorPhase::Aborting;
                    self.control_sent = false;
                    return StatusCode::Ok;
                }
                ConfigurationCoordinatorPhase::Activating if !self.control_sent => {
                    self.phase = ConfigurationCoordinatorPhase::Aborting;
                    return StatusCode::Ok;
                }
                ConfigurationCoordinatorPhase::Rejected if !self.realtime_active_is_operation() => {
                    self.phase = ConfigurationCoordinatorPhase::Aborting;
                    self.control_sent = false;
                    return StatusCode::Ok;
                }
                ConfigurationCoordinatorPhase::Aborting
                | ConfigurationCoordinatorPhase::Clearing => return StatusCode::Ok,
                ConfigurationCoordinatorPhase::Recovering
                | ConfigurationCoordinatorPhase::Activating
                | ConfigurationCoordinatorPhase::Committing
                | ConfigurationCoordinatorPhase::Authorizing => return StatusCode::Busy,
                ConfigurationCoordinatorPhase::Empty
                | ConfigurationCoordinatorPhase::Active
                | ConfigurationCoordinatorPhase::Rejected => {}
            }
        }
        if self.lifecycle_busy() {
            return StatusCode::Busy;
        }
        let Some(active) = self.active else {
            return StatusCode::NotFound;
        };
        let active_publication = active.publication();
        if active_publication.object.content.digest != selection.digest
            || active_publication.object.byte_len != u64::from(selection.byte_len)
        {
            return StatusCode::Conflict;
        }
        let durable = match DurableConfigurationSelection::new(
            selection.transaction_id,
            active_publication,
        ) {
            Ok(durable) => durable,
            Err(_) => return StatusCode::InvalidRequest,
        };
        self.publication = Some(publication_from_durable(durable));
        self.service_identity = self.active_identity;
        self.validation = None;
        self.validation_status = None;
        self.durable_pending = Some(ConfigurationTransition::clear(durable));
        self.durable_prepared = false;
        self.phase = ConfigurationCoordinatorPhase::Clearing;
        self.control_sent = false;
        self.transfer_sent = false;
        self.fault = ConfigurationCoordinatorFault::None;
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
                self.reject(ConfigurationCoordinatorFault::Internal);
                return;
            };
            let next = poll_boundary(validation.next(cache)).await;
            let status = validation.status();
            self.validation_status = Some(status);
            self.service_identity = status.identity;
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
                    self.reject(ConfigurationCoordinatorFault::Protocol);
                    return;
                }
            }
        }

        let complete = self.validation_status.is_some_and(|status| {
            status.state == ServiceConfigurationState::Complete && status.identity.is_some()
        });
        if !complete || self.pending_command.is_some() || !self.realtime_candidate_matches() {
            return;
        }
        if self.phase == ConfigurationCoordinatorPhase::Recovering {
            self.phase = ConfigurationCoordinatorPhase::Activating;
            self.control_sent = false;
        } else {
            self.phase = ConfigurationCoordinatorPhase::CandidateValid;
        }
    }

    async fn prepare_activation(
        &mut self,
        cache: &mut selected::StorageBackend,
        mutation: MutationContext,
    ) {
        let Some(publication) = self.publication else {
            self.reject(ConfigurationCoordinatorFault::Internal);
            return;
        };
        let durable = match DurableConfigurationSelection::new(
            publication.transaction_id,
            publication.publication,
        ) {
            Ok(durable) => durable,
            Err(_) => {
                self.reject(ConfigurationCoordinatorFault::Request);
                return;
            }
        };
        let transition = ConfigurationTransition::activate(durable);
        match poll_boundary(cache.prepare_configuration_transition(transition, mutation)).await {
            Ok(_) => {
                self.durable_pending = Some(transition);
                self.durable_prepared = true;
                self.phase = ConfigurationCoordinatorPhase::Activating;
                self.control_sent = false;
            }
            Err(_) => self.reject(ConfigurationCoordinatorFault::Durability),
        }
    }

    fn activation_step(&mut self, endpoint: &mut DefaultServiceEndpoint, now: DeviceCycle) {
        let Some(publication) = self.publication else {
            self.reject(ConfigurationCoordinatorFault::Internal);
            return;
        };
        if !self.control_sent {
            let command = match CoreConfigurationCommand::activate(
                publication.transaction_id,
                publication.digest(),
                publication.byte_len().unwrap_or(0),
            ) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(sent) => self.control_sent = sent,
                Err(()) => self.reject(ConfigurationCoordinatorFault::Protocol),
            }
            return;
        }
        if !self.realtime_active_matches(false) {
            return;
        }
        if self.boot_recovery {
            let Ok(configuration) = self.service_servo_configuration_for_identity() else {
                self.reject(ConfigurationCoordinatorFault::Internal);
                return;
            };
            self.active_identity = self.service_identity;
            self.active_servo_configuration = configuration;
            self.phase = ConfigurationCoordinatorPhase::Authorizing;
            self.control_sent = false;
        } else {
            self.phase = ConfigurationCoordinatorPhase::Committing;
        }
    }

    async fn commit_activation(
        &mut self,
        cache: &mut selected::StorageBackend,
        mutation: MutationContext,
    ) {
        let Some(transition) = self.durable_pending else {
            self.reject(ConfigurationCoordinatorFault::Internal);
            return;
        };
        let Ok(configuration) = self.service_servo_configuration_for_identity() else {
            self.reject(ConfigurationCoordinatorFault::Internal);
            return;
        };
        match poll_boundary(cache.commit_configuration_transition(transition, mutation)).await {
            Ok(journal) => {
                self.active = journal.active;
                self.active_identity = self.service_identity;
                self.active_servo_configuration = configuration;
                self.durable_pending = None;
                self.durable_prepared = false;
                self.phase = ConfigurationCoordinatorPhase::Authorizing;
                self.control_sent = false;
            }
            Err(_) => self.reject(ConfigurationCoordinatorFault::Durability),
        }
    }

    fn authorization_step(&mut self, endpoint: &mut DefaultServiceEndpoint, now: DeviceCycle) {
        let Some(publication) = self.publication else {
            self.reject(ConfigurationCoordinatorFault::Internal);
            return;
        };
        if !self.control_sent {
            let command = match CoreConfigurationCommand::authorize(
                publication.transaction_id,
                publication.digest(),
                publication.byte_len().unwrap_or(0),
            ) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(sent) => self.control_sent = sent,
                Err(()) => self.reject(ConfigurationCoordinatorFault::Protocol),
            }
            return;
        }
        if !self.realtime_active_matches(true) {
            return;
        }
        self.finish_operation(ConfigurationCoordinatorPhase::Active);
    }

    async fn clear_step(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        now: DeviceCycle,
        mutation: MutationContext,
    ) {
        let Some(transition) = self.durable_pending else {
            self.reject(ConfigurationCoordinatorFault::Internal);
            return;
        };
        let prepared = cache
            .configuration_journal()
            .is_ok_and(|journal| journal.pending == Some(transition));
        if !prepared {
            match poll_boundary(cache.prepare_configuration_transition(transition, mutation)).await
            {
                Ok(_) => {
                    self.durable_prepared = true;
                    return;
                }
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Durability);
                    return;
                }
            }
        } else {
            self.durable_prepared = true;
        }
        if !self.control_sent {
            let selection = transition.selection();
            let publication = selection.publication();
            let command = match CoreConfigurationCommand::clear(
                selection.transaction_id(),
                publication.object.content.digest,
                u32::try_from(publication.object.byte_len).unwrap_or(0),
            ) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(sent) => self.control_sent = sent,
                Err(()) => self.reject(ConfigurationCoordinatorFault::Protocol),
            }
            return;
        }
        if !self.realtime_cleared_matches() {
            return;
        }
        match poll_boundary(cache.commit_configuration_transition(transition, mutation)).await {
            Ok(journal) => {
                self.active = journal.active;
                self.active_identity = None;
                self.active_servo_configuration = None;
                self.durable_pending = None;
                self.durable_prepared = false;
                self.finish_operation(ConfigurationCoordinatorPhase::Empty);
            }
            Err(_) => self.reject(ConfigurationCoordinatorFault::Durability),
        }
    }

    async fn abort_step(
        &mut self,
        cache: &mut selected::StorageBackend,
        endpoint: &mut DefaultServiceEndpoint,
        now: DeviceCycle,
        mutation: MutationContext,
    ) {
        if !self.control_sent && self.transfer_sent {
            let Some(publication) = self.publication else {
                self.reject(ConfigurationCoordinatorFault::Internal);
                return;
            };
            let command = match CoreConfigurationCommand::abort(
                publication.transaction_id,
                publication.digest(),
                publication.byte_len().unwrap_or(0),
            ) {
                Ok(command) => command,
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Protocol);
                    return;
                }
            };
            match self.try_send(endpoint, command, now) {
                Ok(sent) => self.control_sent = sent,
                Err(()) => self.reject(ConfigurationCoordinatorFault::Protocol),
            }
            return;
        }
        self.control_sent = true;
        if let Some(transition) = self.durable_pending {
            match poll_boundary(cache.abort_configuration_transition(transition, mutation)).await {
                Ok(_) => self.durable_pending = None,
                Err(_) => {
                    self.reject(ConfigurationCoordinatorFault::Durability);
                    return;
                }
            }
        }
        let phase = if self.active.is_some() {
            ConfigurationCoordinatorPhase::Active
        } else {
            ConfigurationCoordinatorPhase::Empty
        };
        self.finish_operation(phase);
    }

    fn try_send(
        &mut self,
        endpoint: &mut DefaultServiceEndpoint,
        command: CoreConfigurationCommand,
        now: DeviceCycle,
    ) -> Result<bool, ()> {
        let encoded = command.encode().map_err(|_| ())?;
        let sequence = next_nonzero(self.command_sequence);
        let frame = IntercoreFrame::new(
            FrameKind::Configuration,
            sequence,
            now,
            command.digest,
            encoded.as_bytes(),
        )
        .map_err(|_| ())?;
        if endpoint.try_send_command(frame).is_err() {
            return Ok(false);
        }
        self.command_sequence = sequence;
        Ok(true)
    }

    fn selection_matches_operation(&self, selection: ConfigurationSelection) -> bool {
        self.publication_matches_selection(selection)
            && self.service_identity.is_some_and(|identity| {
                identity.digest == selection.digest && identity.byte_len == selection.byte_len
            })
    }

    fn publication_matches_selection(&self, selection: ConfigurationSelection) -> bool {
        self.publication.is_some_and(|publication| {
            publication.transaction_id == selection.transaction_id
                && publication.digest() == selection.digest
                && publication.byte_len().ok() == Some(selection.byte_len)
        })
    }

    fn realtime_active_is_operation(&self) -> bool {
        self.publication.is_some_and(|publication| {
            self.realtime.active_digest == publication.digest()
                && self.realtime.active_bytes == publication.byte_len().unwrap_or(0)
        })
    }

    fn realtime_candidate_matches(&self) -> bool {
        let Some(publication) = self.publication else {
            return false;
        };
        let Some(identity) = self.service_identity else {
            return false;
        };
        self.realtime.state == RealtimeConfigurationState::CandidateValid
            && self.realtime.transaction_id == publication.transaction_id
            && self.realtime.digest == identity.digest
            && self.realtime.total_bytes == identity.byte_len
            && self.realtime.summary == Some(identity.summary)
    }

    fn realtime_active_matches(&self, authorized: bool) -> bool {
        let Some(publication) = self.publication else {
            return false;
        };
        let Some(identity) = self.service_identity else {
            return false;
        };
        self.realtime.state == RealtimeConfigurationState::Active
            && self.realtime.transaction_id == publication.transaction_id
            && self.realtime.digest == identity.digest
            && self.realtime.total_bytes == identity.byte_len
            && self.realtime.summary == Some(identity.summary)
            && self.realtime.active_digest == publication.digest()
            && self.realtime.active_bytes == publication.byte_len().unwrap_or(0)
            && self.realtime.active_authorized == authorized
    }

    fn realtime_cleared_matches(&self) -> bool {
        let Some(publication) = self.publication else {
            return false;
        };
        self.realtime.state == RealtimeConfigurationState::Cleared
            && self.realtime.transaction_id == publication.transaction_id
            && self.realtime.digest == publication.digest()
            && self.realtime.total_bytes == publication.byte_len().unwrap_or(0)
            && self.realtime.active_digest.is_zero()
            && !self.realtime.active_authorized
    }

    fn realtime_operation_rejection(&self) -> Option<ConfigurationCoordinatorFault> {
        let sent = match self.phase {
            ConfigurationCoordinatorPhase::Recovering
            | ConfigurationCoordinatorPhase::Validating => self.transfer_sent,
            ConfigurationCoordinatorPhase::Activating
            | ConfigurationCoordinatorPhase::Authorizing
            | ConfigurationCoordinatorPhase::Clearing => self.control_sent,
            ConfigurationCoordinatorPhase::Empty
            | ConfigurationCoordinatorPhase::CandidateValid
            | ConfigurationCoordinatorPhase::Preparing
            | ConfigurationCoordinatorPhase::Committing
            | ConfigurationCoordinatorPhase::Active
            | ConfigurationCoordinatorPhase::Aborting
            | ConfigurationCoordinatorPhase::Rejected => false,
        };
        let publication = self.publication?;
        if !sent
            || self.realtime.state != RealtimeConfigurationState::Rejected
            || self.realtime.transaction_id != publication.transaction_id
            || self.realtime.digest != publication.digest()
            || self.realtime.total_bytes != publication.byte_len().unwrap_or(0)
        {
            return None;
        }
        Some(
            if self.realtime.fault == alumina_config::ConfigurationFaultCode::ForbiddenState {
                ConfigurationCoordinatorFault::SafetyState
            } else {
                ConfigurationCoordinatorFault::RealtimeValidation
            },
        )
    }

    fn identity_is_known(&self, digest: Digest, bytes: u32) -> bool {
        self.active.is_some_and(|selection| {
            let publication = selection.publication();
            publication.object.content.digest == digest
                && publication.object.byte_len == u64::from(bytes)
        }) || self.publication.is_some_and(|publication| {
            publication.digest() == digest && publication.byte_len().ok() == Some(bytes)
        })
    }

    fn realtime_authorizes_durable(&self) -> bool {
        self.active.is_some_and(|selection| {
            let publication = selection.publication();
            self.realtime.active_authorized
                && self.realtime.active_digest == publication.object.content.digest
                && self.realtime.active_bytes
                    == u32::try_from(publication.object.byte_len).unwrap_or(0)
        })
    }

    fn realtime_matches_durable_or_empty(&self) -> bool {
        self.active.map_or_else(
            || self.realtime.active_digest.is_zero(),
            |selection| {
                let publication = selection.publication();
                self.realtime.active_digest == publication.object.content.digest
                    && self.realtime.active_bytes
                        == u32::try_from(publication.object.byte_len).unwrap_or(0)
                    && self.realtime.active_authorized
            },
        )
    }

    fn service_servo_configuration_for_identity(
        &self,
    ) -> Result<Option<CachedServoConfiguration<{ selected::JOB_AXES }>>, ()> {
        let identity = self.service_identity.ok_or(())?;
        let configuration = self
            .validation
            .as_ref()
            .and_then(|validation| validation.validated_configuration())
            .filter(|configuration| configuration.identity() == identity)
            .ok_or(())?;
        if identity.summary.foc_axes == 0 {
            return Ok(None);
        }
        CachedServoConfiguration::from_configuration(configuration)
            .map(Some)
            .map_err(|_| ())
    }

    fn finish_operation(&mut self, phase: ConfigurationCoordinatorPhase) {
        self.phase = phase;
        self.fault = ConfigurationCoordinatorFault::None;
        self.publication = None;
        self.validation = None;
        self.validation_status = None;
        self.service_identity = None;
        self.pending_command = None;
        self.control_sent = false;
        self.transfer_sent = false;
        self.boot_recovery = false;
        self.durable_prepared = false;
    }

    fn reject(&mut self, fault: ConfigurationCoordinatorFault) {
        self.phase = ConfigurationCoordinatorPhase::Rejected;
        self.fault = fault;
        self.validation = None;
        self.pending_command = None;
        self.control_sent = false;
    }

    fn active_digest(&self) -> Digest {
        self.active.map_or(Digest::ZERO, |selection| {
            selection.publication().object.content.digest
        })
    }

    fn status(&self) -> ConfigurationCoordinatorStatus {
        let (operation_transaction_id, operation_digest, operation_bytes) = self
            .publication
            .map_or((0, Digest::ZERO, 0), |publication| {
                (
                    publication.transaction_id,
                    publication.digest(),
                    publication.byte_len().unwrap_or(0),
                )
            });
        let (validated_bytes, storage_chunks_read) =
            self.validation_status.map_or((0, 0), |status| {
                (status.validated_bytes, status.storage_chunks_read)
            });
        let (active_transaction_id, active_digest, active_bytes) =
            self.active.map_or((0, Digest::ZERO, 0), |selection| {
                let publication = selection.publication();
                (
                    selection.transaction_id(),
                    publication.object.content.digest,
                    u32::try_from(publication.object.byte_len).unwrap_or(0),
                )
            });
        let summary = if self.publication.is_some() {
            self.service_identity.map(|identity| identity.summary)
        } else {
            self.active_identity.map(|identity| identity.summary)
        };
        let mut flags = 0_u8;
        if self.durable_prepared || self.boot_orphan.is_some() {
            flags |= ConfigurationCoordinatorFlags::DURABLE_PREPARED;
        }
        if self.boot_recovery {
            flags |= ConfigurationCoordinatorFlags::BOOT_RECOVERY;
        }
        if !self.authorized_digest().is_zero() {
            flags |= ConfigurationCoordinatorFlags::JOBS_AUTHORIZED;
        }
        if summary.is_some() {
            flags |= ConfigurationCoordinatorFlags::CORE0_VALID;
        }
        ConfigurationCoordinatorStatus {
            phase: self.phase,
            flags: ConfigurationCoordinatorFlags(flags),
            fault: self.fault,
            operation_transaction_id,
            operation_digest,
            operation_bytes,
            validated_bytes,
            storage_chunks_read,
            active_transaction_id,
            active_digest,
            active_bytes,
            summary,
            realtime: self.realtime,
        }
    }

    fn respond(
        &self,
        native: NativeRequest<'_>,
        now: DeviceCycle,
        status: StatusCode,
    ) -> ServiceResponse {
        let body = match self.status().encode() {
            Ok(body) => body,
            Err(_) => {
                return ServiceResponse::native(native, now, StatusCode::Internal, &[])
                    .unwrap_or_else(|_| ServiceResponse::invalid_native());
            }
        };
        ServiceResponse::native(native, now, status, &body)
            .unwrap_or_else(|_| ServiceResponse::invalid_native())
    }
}

impl Default for ConfigurationService {
    fn default() -> Self {
        Self::new()
    }
}

fn publication_from_durable(selection: DurableConfigurationSelection) -> ConfigurationPublication {
    ConfigurationPublication {
        transaction_id: selection.transaction_id(),
        publication: selection.publication(),
    }
}

fn transfer_error<E>(
    error: ConfigurationTransferError<E>,
) -> (StatusCode, ConfigurationCoordinatorFault) {
    match error {
        ConfigurationTransferError::Request(_) => (
            StatusCode::InvalidRequest,
            ConfigurationCoordinatorFault::Request,
        ),
        ConfigurationTransferError::Configuration(_) => (
            StatusCode::Integrity,
            ConfigurationCoordinatorFault::ServiceValidation,
        ),
        ConfigurationTransferError::CoreWire(_) | ConfigurationTransferError::State => (
            StatusCode::Internal,
            ConfigurationCoordinatorFault::Protocol,
        ),
        ConfigurationTransferError::Storage(error) => match error {
            ProvisionedCacheError::Media(MediaError::PublishedNotFound) => {
                (StatusCode::NotFound, ConfigurationCoordinatorFault::Storage)
            }
            ProvisionedCacheError::Media(MediaError::Corrupt(_)) => (
                StatusCode::Integrity,
                ConfigurationCoordinatorFault::Storage,
            ),
            ProvisionedCacheError::NotMounted => (
                StatusCode::Unsupported,
                ConfigurationCoordinatorFault::Storage,
            ),
            _ => (StatusCode::Internal, ConfigurationCoordinatorFault::Storage),
        },
    }
}

const fn next_nonzero(value: u32) -> u32 {
    let next = value.wrapping_add(1);
    if next == 0 { 1 } else { next }
}
