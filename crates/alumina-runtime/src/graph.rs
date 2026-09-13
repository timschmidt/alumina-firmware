//! Allocation-free ownership and execution of the fixed deployed graph subset.

use core::cell::RefCell;
use core::mem::size_of;

use alumina_board::{
    GraphOpcodeDescriptor, GraphResourceAccess, GraphResourceDescriptor, OwnerDomain, ResourceId,
    SupportLevel,
};
use alumina_graph_ir::{
    BOOLEAN_LATEST_STATE_BYTES, BOOLEAN_STREAM_ITEM_BYTES, GraphIrBooleanStreamItem,
    GraphIrBooleanValue, GraphIrChannel, GraphIrChannelOwner, GraphIrDomain, GraphIrError,
    GraphIrFullPolicy, GraphIrOpcode, GraphIrPackage, GraphIrSummary, MAX_GRAPH_IR_CHANNELS,
    MAX_GRAPH_IR_NODES, decode_graph_resource_pair_parameter, decode_graph_resource_parameter,
};
use alumina_protocol::{DeviceCycle, DeviceId, Digest};
use embassy_sync::blocking_mutex::Mutex as BlockingMutex;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

use crate::{LatestSignal, SignalSnapshot};

mod deployment;
mod live;

pub use deployment::*;
pub use live::*;

const NO_CHANNEL: u8 = u8::MAX;

/// Resource-free V2 opcode palette used by portable fixed-runtime fixtures.
///
/// Physical graph I/O always requires an explicitly supplied board palette.
pub const RESOURCE_FREE_GRAPH_OPCODES: &[GraphOpcodeDescriptor] = &[
    GraphOpcodeDescriptor {
        opcode: GraphIrOpcode::BooleanStreamConstant as u8,
        domain: OwnerDomain::Service,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    },
    GraphOpcodeDescriptor {
        opcode: GraphIrOpcode::BooleanLatest as u8,
        domain: OwnerDomain::Realtime,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    },
    GraphOpcodeDescriptor {
        opcode: GraphIrOpcode::BooleanStreamSink as u8,
        domain: OwnerDomain::Realtime,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    },
];

/// Exact package identities implemented and active on one firmware target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphRuntimeAuthority {
    /// Provisioned physical MCU identity.
    pub device_id: DeviceId,
    /// Canonical compiled board-capability identity.
    pub capability_digest: Digest,
    /// Independently active stored machine-configuration identity.
    pub config_digest: Digest,
    /// Exact implementation-registry identity selected by immutable image or
    /// authenticated installation authority.
    pub implementation_digest: Digest,
}

/// Compile-time arena limits independently enforced before a package may run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphRuntimeLimits {
    /// Core-0 node-state bytes.
    pub service_state_bytes: usize,
    /// Core-1 node-state bytes.
    pub realtime_state_bytes: usize,
    /// Core-0-local queue bytes.
    pub service_channel_bytes: usize,
    /// Core-1-local queue bytes.
    pub realtime_channel_bytes: usize,
    /// One-way core-0-to-core-1 queue bytes.
    pub bridge_channel_bytes: usize,
    /// Exact opcode palette implemented by this firmware image.
    pub opcodes: &'static [GraphOpcodeDescriptor],
    /// Exact physical resources and bounded accesses admitted by this image.
    pub resources: &'static [GraphResourceDescriptor],
}

impl GraphRuntimeLimits {
    /// Limits represented by one concrete fixed runtime type.
    pub const fn fixed<
        const SERVICE_STATE: usize,
        const REALTIME_STATE: usize,
        const SERVICE_CHANNELS: usize,
        const REALTIME_CHANNELS: usize,
        const BRIDGE_CHANNELS: usize,
    >() -> Self {
        Self {
            service_state_bytes: SERVICE_STATE,
            realtime_state_bytes: REALTIME_STATE,
            service_channel_bytes: SERVICE_CHANNELS,
            realtime_channel_bytes: REALTIME_CHANNELS,
            bridge_channel_bytes: BRIDGE_CHANNELS,
            opcodes: RESOURCE_FREE_GRAPH_OPCODES,
            resources: &[],
        }
    }

    /// Limits represented by one concrete fixed runtime and board palette.
    pub const fn fixed_with_capabilities<
        const SERVICE_STATE: usize,
        const REALTIME_STATE: usize,
        const SERVICE_CHANNELS: usize,
        const REALTIME_CHANNELS: usize,
        const BRIDGE_CHANNELS: usize,
    >(
        opcodes: &'static [GraphOpcodeDescriptor],
        resources: &'static [GraphResourceDescriptor],
    ) -> Self {
        Self {
            service_state_bytes: SERVICE_STATE,
            realtime_state_bytes: REALTIME_STATE,
            service_channel_bytes: SERVICE_CHANNELS,
            realtime_channel_bytes: REALTIME_CHANNELS,
            bridge_channel_bytes: BRIDGE_CHANNELS,
            opcodes,
            resources,
        }
    }
}

/// Statically reserved arena family used by a fixed graph runtime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphRuntimeArena {
    /// Core-0 node state.
    ServiceState,
    /// Core-1 node state.
    RealtimeState,
    /// Core-0-local queues.
    ServiceChannels,
    /// Core-1-local queues.
    RealtimeChannels,
    /// One-way core-0-to-core-1 queues.
    ServiceToRealtime,
}

/// Monotonic portable runtime phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphRuntimePhase {
    /// No package has been admitted.
    Empty,
    /// One exact package and its static layout have been admitted.
    Installed,
    /// Tick zero is primed and an exact start cycle is fixed.
    Prepared,
    /// Unique Service and Realtime endpoint ownership has been issued.
    Split,
    /// Start priming encountered a terminal execution fault.
    Faulted,
}

/// Arena bytes used by one independently admitted package.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphRuntimeUsage {
    /// Service node-state bytes used.
    pub service_state_bytes: u32,
    /// Realtime node-state bytes used.
    pub realtime_state_bytes: u32,
    /// Service-local queue bytes used.
    pub service_channel_bytes: u32,
    /// Realtime-local queue bytes used.
    pub realtime_channel_bytes: u32,
    /// Service-to-Realtime queue bytes used.
    pub bridge_channel_bytes: u32,
}

impl GraphRuntimeUsage {
    /// Exact state and queue payload bytes used, excluding fixed executor metadata.
    pub fn payload_bytes(self) -> Result<u32, GraphRuntimeError> {
        self.service_state_bytes
            .checked_add(self.realtime_state_bytes)
            .and_then(|value| value.checked_add(self.service_channel_bytes))
            .and_then(|value| value.checked_add(self.realtime_channel_bytes))
            .and_then(|value| value.checked_add(self.bridge_channel_bytes))
            .ok_or(GraphRuntimeError::Arithmetic)
    }
}

/// Transactional package-admission result retained before any graph execution.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphInstallReport {
    /// Canonical complete package identity.
    pub package_digest: Digest,
    /// Canonical source structural-graph identity.
    pub graph_digest: Digest,
    /// Independently reconstructed node/channel totals.
    pub summary: GraphIrSummary,
    /// Exact package-selected arena usage.
    pub usage: GraphRuntimeUsage,
    /// Complete bytes reserved by this concrete Rust runtime type.
    pub static_runtime_bytes: usize,
}

/// Successful deterministic start preparation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphStartReport {
    /// Exact package being started.
    pub package_digest: Digest,
    /// Shared device-cycle epoch for release tick zero.
    pub start_cycle: DeviceCycle,
    /// Service tick-zero work completed before the Realtime endpoint is issued.
    pub primed_service_release: Option<GraphReleaseReport>,
}

/// Fixed package lifecycle/admission rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphRuntimeError {
    /// Fixed safety/job ownership forbids package mutation.
    MutationForbidden,
    /// Fixed safety/job ownership forbids start preparation.
    ExecutionForbidden,
    /// The operation is not legal in the current monotonic phase.
    Phase {
        /// Required phase.
        required: GraphRuntimePhase,
        /// Observed phase.
        actual: GraphRuntimePhase,
    },
    /// The expected package identity was not established.
    MissingExpectedDigest,
    /// Independent canonical package decoding failed.
    Package(GraphIrError),
    /// Canonical package bytes did not match the requested content identity.
    PackageDigest {
        /// Requested digest.
        expected: Digest,
        /// Independently decoded digest.
        received: Digest,
    },
    /// The package targeted a different firmware authority.
    Identity(GraphRuntimeIdentity),
    /// One decoded arena cannot fit its compile-time reservation.
    Capacity {
        /// Rejected arena.
        arena: GraphRuntimeArena,
        /// Exact package requirement.
        required: u32,
        /// Compile-time bytes available.
        available: usize,
    },
    /// A node opcode/domain was absent or unqualified in the image palette.
    OpcodeCapability {
        /// Rejected node record.
        node: u16,
        /// Exact opcode requested by that node.
        opcode: u8,
    },
    /// A typed selector/access was absent or unqualified in the image palette.
    ResourceCapability {
        /// Rejected node record.
        node: u16,
        /// Exact canonical selector requested by that node.
        resource: ResourceId,
    },
    /// A validated package contradicted the fixed V2 executor shape.
    RuntimeShape,
    /// Checked release or storage arithmetic overflowed.
    Arithmetic,
    /// Tick-zero priming latched a terminal execution fault.
    Execution(GraphExecutionError),
}

impl From<GraphIrError> for GraphRuntimeError {
    fn from(value: GraphIrError) -> Self {
        Self::Package(value)
    }
}

/// Exact authority field that differed during package admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GraphRuntimeIdentity {
    /// Physical MCU identity.
    Device,
    /// Board-capability digest.
    Capability,
    /// Active machine-configuration digest.
    Configuration,
    /// Compiled implementation-registry digest.
    Implementation,
}

/// Stable fail-stop reason shared independently of ordinary telemetry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphExecutionFault {
    /// Fixed safety authority was absent at a release boundary.
    SafetyNotAuthorized = 1,
    /// A core invoked a release at a cycle other than the exact schedule.
    UnexpectedReleaseCycle = 2,
    /// A declared fault-on-full queue had no free item slot.
    QueueFull = 3,
    /// Queue bytes, cursor state, or source sequence were not canonical.
    QueueCorrupt = 4,
    /// A latest-at-or-before node had no value at its first release.
    MissingInitialValue = 5,
    /// Checked cycle, tick, counter, or offset arithmetic overflowed.
    Arithmetic = 6,
    /// Runtime metadata contradicted an already admitted package.
    RuntimeShape = 7,
    /// A release was requested for an absent package domain.
    DomainAbsent = 8,
    /// A capability-admitted input had no known fresh semantic sample.
    ResourceUnavailable = 9,
}

impl GraphExecutionFault {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::SafetyNotAuthorized),
            2 => Some(Self::UnexpectedReleaseCycle),
            3 => Some(Self::QueueFull),
            4 => Some(Self::QueueCorrupt),
            5 => Some(Self::MissingInitialValue),
            6 => Some(Self::Arithmetic),
            7 => Some(Self::RuntimeShape),
            8 => Some(Self::DomainAbsent),
            9 => Some(Self::ResourceUnavailable),
            _ => None,
        }
    }
}

/// One newest fail-stop observation from the independent latest-value mailbox.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphFaultObservation {
    /// Wrapping nonzero publication generation.
    pub generation: u16,
    /// Stable fault family.
    pub fault: GraphExecutionFault,
    /// Node or channel record index when applicable.
    pub detail: u8,
}

impl GraphFaultObservation {
    fn from_signal(signal: SignalSnapshot) -> Self {
        Self {
            generation: signal.generation,
            fault: GraphExecutionFault::from_wire(signal.code)
                .unwrap_or(GraphExecutionFault::RuntimeShape),
            detail: signal.detail,
        }
    }
}

/// Terminal release error paired with exact cycle evidence when applicable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphExecutionError {
    /// Latched shared fault.
    pub observation: GraphFaultObservation,
    /// Required release cycle for a scheduling fault.
    pub expected_cycle: Option<DeviceCycle>,
    /// Received release cycle for a scheduling fault.
    pub received_cycle: Option<DeviceCycle>,
}

/// Bounded facts produced by one completed domain release.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphReleaseReport {
    /// Exact scheduled device cycle.
    pub cycle: DeviceCycle,
    /// Zero-based release tick in this package domain.
    pub release_tick: u64,
    /// Fixed nodes executed in this domain.
    pub nodes_executed: u16,
    /// Queue items consumed across all node inputs.
    pub items_consumed: u32,
    /// Queue items emitted across all fanout edges.
    pub items_emitted: u32,
    /// Items consumed by sink nodes.
    pub sink_items: u32,
    /// Last sink value in topological order, when one was due.
    pub last_sink_value: Option<bool>,
}

/// Exact scheduled release and the package-declared latest dispatch boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphReleaseWindow {
    /// Exact cycle passed to the fixed executor.
    pub scheduled_cycle: DeviceCycle,
    /// Last cycle at which dispatch may begin within the declared reserve.
    pub latest_dispatch_cycle: DeviceCycle,
}

impl GraphReleaseReport {
    const fn new(cycle: DeviceCycle, release_tick: u64) -> Self {
        Self {
            cycle,
            release_tick,
            nodes_executed: 0,
            items_consumed: 0,
            items_emitted: 0,
            sink_items: 0,
            last_sink_value: None,
        }
    }
}

#[derive(Clone, Copy)]
struct QueueCursor {
    head: u32,
    len: u32,
}

impl QueueCursor {
    const EMPTY: Self = Self { head: 0, len: 0 };
}

#[derive(Clone, Copy)]
struct DomainCursor {
    present: bool,
    next_cycle: u64,
    next_tick: u64,
    period_cycles: u64,
    executor_reserve_cycles: u64,
}

impl DomainCursor {
    const EMPTY: Self = Self {
        present: false,
        next_cycle: 0,
        next_tick: 0,
        period_cycles: 0,
        executor_reserve_cycles: 0,
    };
}

#[derive(Clone, Copy)]
struct GraphRuntimeMetadata {
    input_channel: [u8; MAX_GRAPH_IR_NODES],
    output_channels: [u64; MAX_GRAPH_IR_NODES],
    service_channel_bytes: u32,
    realtime_channel_bytes: u32,
    bridge_channel_bytes: u32,
}

impl GraphRuntimeMetadata {
    const EMPTY: Self = Self {
        input_channel: [NO_CHANNEL; MAX_GRAPH_IR_NODES],
        output_channels: [0; MAX_GRAPH_IR_NODES],
        service_channel_bytes: 0,
        realtime_channel_bytes: 0,
        bridge_channel_bytes: 0,
    };

    fn from_package(package: &GraphIrPackage) -> Result<Self, GraphRuntimeError> {
        let mut metadata = Self::EMPTY;
        for index in 0..package.summary().channel_count {
            let channel = package
                .channel(index)
                .ok_or(GraphRuntimeError::RuntimeShape)?;
            if channel.full_policy != GraphIrFullPolicy::Fault {
                return Err(GraphRuntimeError::RuntimeShape);
            }
            let source = usize::from(channel.source_node);
            let target = usize::from(channel.target_node);
            let input = metadata
                .input_channel
                .get_mut(target)
                .ok_or(GraphRuntimeError::RuntimeShape)?;
            if *input != NO_CHANNEL {
                return Err(GraphRuntimeError::RuntimeShape);
            }
            *input = u8::try_from(index).map_err(|_| GraphRuntimeError::RuntimeShape)?;
            let mask = 1_u64
                .checked_shl(u32::from(index))
                .ok_or(GraphRuntimeError::RuntimeShape)?;
            let outputs = metadata
                .output_channels
                .get_mut(source)
                .ok_or(GraphRuntimeError::RuntimeShape)?;
            *outputs |= mask;
            match channel.owner {
                GraphIrChannelOwner::Service => {
                    metadata.service_channel_bytes = metadata
                        .service_channel_bytes
                        .checked_add(channel.storage_bytes)
                        .ok_or(GraphRuntimeError::Arithmetic)?;
                }
                GraphIrChannelOwner::Realtime => {
                    metadata.realtime_channel_bytes = metadata
                        .realtime_channel_bytes
                        .checked_add(channel.storage_bytes)
                        .ok_or(GraphRuntimeError::Arithmetic)?;
                }
                GraphIrChannelOwner::ServiceToRealtime => {
                    metadata.bridge_channel_bytes = metadata
                        .bridge_channel_bytes
                        .checked_add(channel.storage_bytes)
                        .ok_or(GraphRuntimeError::Arithmetic)?;
                }
            }
        }
        Ok(metadata)
    }
}

struct BridgeArena<const BYTES: usize> {
    bytes: [u8; BYTES],
    queues: [QueueCursor; MAX_GRAPH_IR_CHANNELS],
}

impl<const BYTES: usize> BridgeArena<BYTES> {
    const fn new() -> Self {
        Self {
            bytes: [0; BYTES],
            queues: [QueueCursor::EMPTY; MAX_GRAPH_IR_CHANNELS],
        }
    }
}

/// One statically sized package, state set, queue set, and split-core bridge.
///
/// The concrete const parameters are part of a board image's memory budget.
/// Package admission can use fewer bytes but can never enlarge these arrays.
pub struct FixedGraphRuntime<
    const SERVICE_STATE: usize,
    const REALTIME_STATE: usize,
    const SERVICE_CHANNELS: usize,
    const REALTIME_CHANNELS: usize,
    const BRIDGE_CHANNELS: usize,
> {
    phase: GraphRuntimePhase,
    package: Option<GraphIrPackage>,
    metadata: GraphRuntimeMetadata,
    start_cycle: u64,
    service_cursor: DomainCursor,
    realtime_cursor: DomainCursor,
    service_state: [u8; SERVICE_STATE],
    realtime_state: [u8; REALTIME_STATE],
    service_channels: [u8; SERVICE_CHANNELS],
    realtime_channels: [u8; REALTIME_CHANNELS],
    service_queues: [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    realtime_queues: [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    realtime_initialized: [bool; MAX_GRAPH_IR_NODES],
    bridge: BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE_CHANNELS>>>,
    fault: LatestSignal,
}

impl<
    const SERVICE_STATE: usize,
    const REALTIME_STATE: usize,
    const SERVICE_CHANNELS: usize,
    const REALTIME_CHANNELS: usize,
    const BRIDGE_CHANNELS: usize,
>
    FixedGraphRuntime<
        SERVICE_STATE,
        REALTIME_STATE,
        SERVICE_CHANNELS,
        REALTIME_CHANNELS,
        BRIDGE_CHANNELS,
    >
{
    /// Construct empty storage suitable for static allocation.
    pub const fn new() -> Self {
        Self {
            phase: GraphRuntimePhase::Empty,
            package: None,
            metadata: GraphRuntimeMetadata::EMPTY,
            start_cycle: 0,
            service_cursor: DomainCursor::EMPTY,
            realtime_cursor: DomainCursor::EMPTY,
            service_state: [0; SERVICE_STATE],
            realtime_state: [0; REALTIME_STATE],
            service_channels: [0; SERVICE_CHANNELS],
            realtime_channels: [0; REALTIME_CHANNELS],
            service_queues: [QueueCursor::EMPTY; MAX_GRAPH_IR_CHANNELS],
            realtime_queues: [QueueCursor::EMPTY; MAX_GRAPH_IR_CHANNELS],
            realtime_initialized: [false; MAX_GRAPH_IR_NODES],
            bridge: BlockingMutex::new(RefCell::new(BridgeArena::new())),
            fault: LatestSignal::new(),
        }
    }

    /// Complete bytes reserved by this concrete runtime type, including
    /// package, arena capacity, cursors, metadata, mutex, and fault mailbox.
    pub const fn static_runtime_bytes() -> usize {
        size_of::<Self>()
    }

    /// Current monotonic lifecycle phase.
    pub const fn phase(&self) -> GraphRuntimePhase {
        self.phase
    }

    /// Copy, independently decode, identity-check, and transactionally install
    /// one exact package without executing any node.
    pub fn install(
        &mut self,
        bytes: &[u8],
        expected_package_digest: Digest,
        authority: GraphRuntimeAuthority,
        mutation_allowed: bool,
    ) -> Result<GraphInstallReport, GraphRuntimeError> {
        if !mutation_allowed {
            return Err(GraphRuntimeError::MutationForbidden);
        }
        self.require_phase(GraphRuntimePhase::Empty)?;
        if expected_package_digest.is_zero() {
            return Err(GraphRuntimeError::MissingExpectedDigest);
        }

        let (metadata, usage) = admit_package_into(
            &mut self.package,
            bytes,
            expected_package_digest,
            authority,
            GraphRuntimeLimits::fixed::<
                SERVICE_STATE,
                REALTIME_STATE,
                SERVICE_CHANNELS,
                REALTIME_CHANNELS,
                BRIDGE_CHANNELS,
            >(),
        )?;
        self.service_state.fill(0);
        self.realtime_state.fill(0);
        self.service_channels.fill(0);
        self.realtime_channels.fill(0);
        self.service_queues.fill(QueueCursor::EMPTY);
        self.realtime_queues.fill(QueueCursor::EMPTY);
        self.realtime_initialized.fill(false);
        self.bridge.lock(|cell| {
            let mut bridge = cell.borrow_mut();
            bridge.bytes.fill(0);
            bridge.queues.fill(QueueCursor::EMPTY);
        });
        self.metadata = metadata;
        self.phase = GraphRuntimePhase::Installed;

        let package = self
            .package
            .as_ref()
            .ok_or(GraphRuntimeError::RuntimeShape)?;
        Ok(GraphInstallReport {
            package_digest: package.digest(),
            graph_digest: package.header().graph_digest,
            summary: package.summary(),
            usage,
            static_runtime_bytes: Self::static_runtime_bytes(),
        })
    }

    /// Fix a future start epoch and pre-run Service tick zero so every
    /// Realtime latest node has deterministic source-first initialization.
    pub fn prepare_start(
        &mut self,
        start_cycle: DeviceCycle,
        execution_allowed: bool,
    ) -> Result<GraphStartReport, GraphRuntimeError> {
        if !execution_allowed {
            return Err(GraphRuntimeError::ExecutionForbidden);
        }
        self.require_phase(GraphRuntimePhase::Installed)?;
        let package = self
            .package
            .as_ref()
            .ok_or(GraphRuntimeError::RuntimeShape)?;
        let header = package.header();
        let service_cursor = cursor_after_prime(header.service_schedule, start_cycle)?;
        let realtime_cursor = cursor_at_start(header.realtime_schedule, start_cycle);

        let primed_service_release = if header.service_schedule.node_count == 0 {
            None
        } else {
            match execute_service_release(
                package,
                &self.metadata,
                &mut self.service_state,
                &mut self.service_channels,
                &mut self.service_queues,
                &self.bridge,
                &self.fault,
                start_cycle.0,
                start_cycle,
                0,
            ) {
                Ok(report) => Some(report),
                Err(error) => {
                    self.phase = GraphRuntimePhase::Faulted;
                    return Err(GraphRuntimeError::Execution(error));
                }
            }
        };

        self.start_cycle = start_cycle.0;
        self.service_cursor = service_cursor;
        self.realtime_cursor = realtime_cursor;
        self.phase = GraphRuntimePhase::Prepared;
        Ok(GraphStartReport {
            package_digest: package.digest(),
            start_cycle,
            primed_service_release,
        })
    }

    /// Uniquely partition all mutable state between core-0 Service and core-1
    /// Realtime owners. The bridge and fault mailbox remain bounded shared
    /// objects protected by the same cross-core critical-section mechanism as
    /// the existing runtime boundary.
    pub fn split(
        &mut self,
    ) -> Result<
        (
            FixedGraphServiceEndpoint<'_, SERVICE_STATE, SERVICE_CHANNELS, BRIDGE_CHANNELS>,
            FixedGraphRealtimeEndpoint<'_, REALTIME_STATE, REALTIME_CHANNELS, BRIDGE_CHANNELS>,
        ),
        GraphRuntimeError,
    > {
        self.require_phase(GraphRuntimePhase::Prepared)?;
        self.phase = GraphRuntimePhase::Split;
        let package = self
            .package
            .as_ref()
            .ok_or(GraphRuntimeError::RuntimeShape)?;
        Ok((
            FixedGraphServiceEndpoint {
                package,
                metadata: &self.metadata,
                start_cycle: self.start_cycle,
                cursor: &mut self.service_cursor,
                state: &mut self.service_state,
                channels: &mut self.service_channels,
                queues: &mut self.service_queues,
                bridge: &self.bridge,
                fault: &self.fault,
            },
            FixedGraphRealtimeEndpoint {
                package,
                metadata: &self.metadata,
                start_cycle: self.start_cycle,
                cursor: &mut self.realtime_cursor,
                state: &mut self.realtime_state,
                channels: &mut self.realtime_channels,
                queues: &mut self.realtime_queues,
                initialized: &mut self.realtime_initialized,
                bridge: &self.bridge,
                fault: &self.fault,
            },
        ))
    }

    /// Read a newly latched graph fault before endpoint ownership is issued.
    pub fn fault_after(&self, generation: u16) -> Option<GraphFaultObservation> {
        self.fault
            .after(generation)
            .map(GraphFaultObservation::from_signal)
    }

    fn require_phase(&self, required: GraphRuntimePhase) -> Result<(), GraphRuntimeError> {
        if self.phase == required {
            Ok(())
        } else {
            Err(GraphRuntimeError::Phase {
                required,
                actual: self.phase,
            })
        }
    }
}

fn admit_package(
    bytes: &[u8],
    expected_package_digest: Digest,
    authority: GraphRuntimeAuthority,
    limits: GraphRuntimeLimits,
) -> Result<(GraphIrPackage, GraphRuntimeMetadata, GraphRuntimeUsage), GraphRuntimeError> {
    let package = GraphIrPackage::from_slice(bytes)?;
    let (metadata, usage) =
        admit_decoded_package(&package, expected_package_digest, authority, limits)?;
    Ok((package, metadata, usage))
}

/// Admit directly into the caller's otherwise-empty permanent package slot.
/// Any failed identity, palette, or arena check restores the empty slot.
fn admit_package_into(
    destination: &mut Option<GraphIrPackage>,
    bytes: &[u8],
    expected_package_digest: Digest,
    authority: GraphRuntimeAuthority,
    limits: GraphRuntimeLimits,
) -> Result<(GraphRuntimeMetadata, GraphRuntimeUsage), GraphRuntimeError> {
    if destination.is_some() {
        return Err(GraphRuntimeError::RuntimeShape);
    }
    GraphIrPackage::replace_from_slice(destination, bytes)?;
    let result = admit_decoded_package(
        destination
            .as_ref()
            .ok_or(GraphRuntimeError::RuntimeShape)?,
        expected_package_digest,
        authority,
        limits,
    );
    if result.is_err() {
        *destination = None;
    }
    result
}

fn admit_decoded_package(
    package: &GraphIrPackage,
    expected_package_digest: Digest,
    authority: GraphRuntimeAuthority,
    limits: GraphRuntimeLimits,
) -> Result<(GraphRuntimeMetadata, GraphRuntimeUsage), GraphRuntimeError> {
    if expected_package_digest.is_zero() {
        return Err(GraphRuntimeError::MissingExpectedDigest);
    }
    if package.digest() != expected_package_digest {
        return Err(GraphRuntimeError::PackageDigest {
            expected: expected_package_digest,
            received: package.digest(),
        });
    }
    let header = package.header();
    if header.device_id != authority.device_id {
        return Err(GraphRuntimeError::Identity(GraphRuntimeIdentity::Device));
    }
    if header.capability_digest != authority.capability_digest {
        return Err(GraphRuntimeError::Identity(
            GraphRuntimeIdentity::Capability,
        ));
    }
    if header.config_digest != authority.config_digest {
        return Err(GraphRuntimeError::Identity(
            GraphRuntimeIdentity::Configuration,
        ));
    }
    if header.implementation_digest != authority.implementation_digest {
        return Err(GraphRuntimeError::Identity(
            GraphRuntimeIdentity::Implementation,
        ));
    }

    validate_capability_palette(package, limits)?;
    let metadata = GraphRuntimeMetadata::from_package(package)?;
    check_capacity(
        GraphRuntimeArena::ServiceState,
        header.service_state_bytes,
        limits.service_state_bytes,
    )?;
    check_capacity(
        GraphRuntimeArena::RealtimeState,
        header.realtime_state_bytes,
        limits.realtime_state_bytes,
    )?;
    check_capacity(
        GraphRuntimeArena::ServiceChannels,
        metadata.service_channel_bytes,
        limits.service_channel_bytes,
    )?;
    check_capacity(
        GraphRuntimeArena::RealtimeChannels,
        metadata.realtime_channel_bytes,
        limits.realtime_channel_bytes,
    )?;
    check_capacity(
        GraphRuntimeArena::ServiceToRealtime,
        metadata.bridge_channel_bytes,
        limits.bridge_channel_bytes,
    )?;
    let usage = GraphRuntimeUsage {
        service_state_bytes: header.service_state_bytes,
        realtime_state_bytes: header.realtime_state_bytes,
        service_channel_bytes: metadata.service_channel_bytes,
        realtime_channel_bytes: metadata.realtime_channel_bytes,
        bridge_channel_bytes: metadata.bridge_channel_bytes,
    };
    Ok((metadata, usage))
}

fn validate_capability_palette(
    package: &GraphIrPackage,
    limits: GraphRuntimeLimits,
) -> Result<(), GraphRuntimeError> {
    for index in 0..package.summary().node_count {
        let node = package.node(index).ok_or(GraphRuntimeError::RuntimeShape)?;
        let opcode = node.opcode.wire_value();
        let mut candidates = limits
            .opcodes
            .iter()
            .copied()
            .filter(|candidate| candidate.opcode == opcode);
        let descriptor = candidates
            .next()
            .ok_or(GraphRuntimeError::OpcodeCapability {
                node: index,
                opcode,
            })?;
        if candidates.next().is_some()
            || descriptor.domain != owner_domain(node.domain)
            || descriptor.support < SupportLevel::Compiles
        {
            return Err(GraphRuntimeError::OpcodeCapability {
                node: index,
                opcode,
            });
        }
        match node.opcode {
            GraphIrOpcode::StableBooleanInput => {
                let resource = decode_graph_resource_parameter(node.parameter)
                    .map_err(|_| GraphRuntimeError::RuntimeShape)?;
                let Some(class) = descriptor.resource_class else {
                    return Err(GraphRuntimeError::OpcodeCapability {
                        node: index,
                        opcode,
                    });
                };
                if descriptor.resource_access != Some(GraphResourceAccess::StableBooleanInput)
                    || !limits.resources.iter().any(|candidate| {
                        candidate.resource == resource
                            && candidate.class == class
                            && candidate.access == GraphResourceAccess::StableBooleanInput
                            && candidate.support >= SupportLevel::Compiles
                    })
                {
                    return Err(GraphRuntimeError::ResourceCapability {
                        node: index,
                        resource,
                    });
                }
            }
            GraphIrOpcode::StableBooleanPairAll => {
                let (first, second) = decode_graph_resource_pair_parameter(node.parameter)
                    .map_err(|_| GraphRuntimeError::RuntimeShape)?;
                let Some(class) = descriptor.resource_class else {
                    return Err(GraphRuntimeError::OpcodeCapability {
                        node: index,
                        opcode,
                    });
                };
                if descriptor.resource_access != Some(GraphResourceAccess::StableBooleanInput) {
                    return Err(GraphRuntimeError::OpcodeCapability {
                        node: index,
                        opcode,
                    });
                }
                for resource in [first, second] {
                    if !limits.resources.iter().any(|candidate| {
                        candidate.resource == resource
                            && candidate.class == class
                            && candidate.access == GraphResourceAccess::StableBooleanInput
                            && candidate.support >= SupportLevel::Compiles
                    }) {
                        return Err(GraphRuntimeError::ResourceCapability {
                            node: index,
                            resource,
                        });
                    }
                }
            }
            GraphIrOpcode::BooleanStreamConstant
            | GraphIrOpcode::BooleanLatest
            | GraphIrOpcode::BooleanStreamSink => {
                if descriptor.resource_class.is_some() || descriptor.resource_access.is_some() {
                    return Err(GraphRuntimeError::OpcodeCapability {
                        node: index,
                        opcode,
                    });
                }
            }
        }
    }
    Ok(())
}

const fn owner_domain(domain: GraphIrDomain) -> OwnerDomain {
    match domain {
        GraphIrDomain::Service => OwnerDomain::Service,
        GraphIrDomain::Realtime => OwnerDomain::Realtime,
    }
}

impl<
    const SERVICE_STATE: usize,
    const REALTIME_STATE: usize,
    const SERVICE_CHANNELS: usize,
    const REALTIME_CHANNELS: usize,
    const BRIDGE_CHANNELS: usize,
> Default
    for FixedGraphRuntime<
        SERVICE_STATE,
        REALTIME_STATE,
        SERVICE_CHANNELS,
        REALTIME_CHANNELS,
        BRIDGE_CHANNELS,
    >
{
    fn default() -> Self {
        Self::new()
    }
}

/// Unique core-0 owner of Service state, queues, and bridge producers.
pub struct FixedGraphServiceEndpoint<
    'a,
    const STATE: usize,
    const CHANNELS: usize,
    const BRIDGE: usize,
> {
    package: &'a GraphIrPackage,
    metadata: &'a GraphRuntimeMetadata,
    start_cycle: u64,
    cursor: &'a mut DomainCursor,
    state: &'a mut [u8; STATE],
    channels: &'a mut [u8; CHANNELS],
    queues: &'a mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    bridge: &'a BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE>>>,
    fault: &'a LatestSignal,
}

impl<const STATE: usize, const CHANNELS: usize, const BRIDGE: usize>
    FixedGraphServiceEndpoint<'_, STATE, CHANNELS, BRIDGE>
{
    /// Execute exactly the next Service release without waiting or allocating.
    pub fn release(
        &mut self,
        cycle: DeviceCycle,
        safety_authorized: bool,
    ) -> Result<GraphReleaseReport, GraphExecutionError> {
        release_prelude(self.fault, self.cursor, cycle, safety_authorized, 0)?;
        let next_cycle = checked_next_cycle(self.fault, self.cursor, 0)?;
        let next_tick = checked_next_tick(self.fault, self.cursor, 0)?;
        let report = execute_service_release(
            self.package,
            self.metadata,
            self.state,
            self.channels,
            self.queues,
            self.bridge,
            self.fault,
            self.start_cycle,
            cycle,
            self.cursor.next_tick,
        )?;
        self.cursor.next_cycle = next_cycle;
        self.cursor.next_tick = next_tick;
        Ok(report)
    }

    /// Read the newest graph fault even when ordinary telemetry is saturated.
    pub fn fault_after(&self, generation: u16) -> Option<GraphFaultObservation> {
        self.fault
            .after(generation)
            .map(GraphFaultObservation::from_signal)
    }
}

/// Unique core-1 owner of Realtime state, queues, and bridge consumers.
pub struct FixedGraphRealtimeEndpoint<
    'a,
    const STATE: usize,
    const CHANNELS: usize,
    const BRIDGE: usize,
> {
    package: &'a GraphIrPackage,
    metadata: &'a GraphRuntimeMetadata,
    start_cycle: u64,
    cursor: &'a mut DomainCursor,
    state: &'a mut [u8; STATE],
    channels: &'a mut [u8; CHANNELS],
    queues: &'a mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    initialized: &'a mut [bool; MAX_GRAPH_IR_NODES],
    bridge: &'a BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE>>>,
    fault: &'a LatestSignal,
}

impl<const STATE: usize, const CHANNELS: usize, const BRIDGE: usize>
    FixedGraphRealtimeEndpoint<'_, STATE, CHANNELS, BRIDGE>
{
    /// Execute exactly the next Realtime release without waiting or allocating.
    pub fn release<F>(
        &mut self,
        cycle: DeviceCycle,
        safety_authorized: bool,
        mut resource_input: F,
    ) -> Result<GraphReleaseReport, GraphExecutionError>
    where
        F: FnMut(ResourceId) -> Option<bool>,
    {
        release_prelude(self.fault, self.cursor, cycle, safety_authorized, 0)?;
        let next_cycle = checked_next_cycle(self.fault, self.cursor, 0)?;
        let next_tick = checked_next_tick(self.fault, self.cursor, 0)?;
        let report = execute_realtime_release(
            self.package,
            self.metadata,
            self.state,
            self.channels,
            self.queues,
            self.initialized,
            self.bridge,
            self.fault,
            self.start_cycle,
            cycle,
            self.cursor.next_tick,
            &mut resource_input,
        )?;
        self.cursor.next_cycle = next_cycle;
        self.cursor.next_tick = next_tick;
        Ok(report)
    }

    /// Read the newest graph fault even when ordinary telemetry is saturated.
    pub fn fault_after(&self, generation: u16) -> Option<GraphFaultObservation> {
        self.fault
            .after(generation)
            .map(GraphFaultObservation::from_signal)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "the executor keeps each statically owned arena explicit"
)]
fn execute_service_release<const STATE: usize, const CHANNELS: usize, const BRIDGE: usize>(
    package: &GraphIrPackage,
    metadata: &GraphRuntimeMetadata,
    _state: &mut [u8; STATE],
    channels: &mut [u8; CHANNELS],
    queues: &mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    bridge: &BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE>>>,
    fault: &LatestSignal,
    _start_cycle: u64,
    cycle: DeviceCycle,
    tick: u64,
) -> Result<GraphReleaseReport, GraphExecutionError> {
    let mut report = GraphReleaseReport::new(cycle, tick);
    for index in 0..package.summary().node_count {
        let node = package
            .node(index)
            .ok_or_else(|| latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None))?;
        if node.domain != GraphIrDomain::Service {
            continue;
        }
        report.nodes_executed = checked_report_u16(
            fault,
            report.nodes_executed,
            index,
            GraphExecutionFault::Arithmetic,
        )?;
        let value = match node.opcode {
            GraphIrOpcode::BooleanStreamConstant => node.parameter != 0,
            _ => {
                return Err(latch_fault(
                    fault,
                    GraphExecutionFault::RuntimeShape,
                    index,
                    None,
                ));
            }
        };
        let item = GraphIrBooleanStreamItem {
            value,
            source_tick: tick,
            sequence: tick,
        };
        emit_service_outputs(
            package,
            metadata.output_channels[usize::from(index)],
            channels,
            queues,
            bridge,
            fault,
            item,
            &mut report,
        )?;
    }
    Ok(report)
}

#[allow(
    clippy::too_many_arguments,
    reason = "the executor keeps each statically owned arena explicit"
)]
fn execute_realtime_release<const STATE: usize, const CHANNELS: usize, const BRIDGE: usize, F>(
    package: &GraphIrPackage,
    metadata: &GraphRuntimeMetadata,
    state: &mut [u8; STATE],
    channels: &mut [u8; CHANNELS],
    queues: &mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    initialized: &mut [bool; MAX_GRAPH_IR_NODES],
    bridge: &BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE>>>,
    fault: &LatestSignal,
    start_cycle: u64,
    cycle: DeviceCycle,
    tick: u64,
    resource_input: &mut F,
) -> Result<GraphReleaseReport, GraphExecutionError>
where
    F: FnMut(ResourceId) -> Option<bool>,
{
    let mut report = GraphReleaseReport::new(cycle, tick);
    for index in 0..package.summary().node_count {
        let node = package
            .node(index)
            .ok_or_else(|| latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None))?;
        if node.domain != GraphIrDomain::Realtime {
            continue;
        }
        report.nodes_executed = checked_report_u16(
            fault,
            report.nodes_executed,
            index,
            GraphExecutionFault::Arithmetic,
        )?;
        match node.opcode {
            GraphIrOpcode::BooleanLatest => {
                let input = required_input(metadata, index, fault)?;
                let mut newest = None;
                while let Some(item) = pop_realtime_due(
                    package,
                    input,
                    channels,
                    queues,
                    bridge,
                    fault,
                    start_cycle,
                    cycle,
                )? {
                    report.items_consumed = checked_report_u32(
                        fault,
                        report.items_consumed,
                        input,
                        GraphExecutionFault::Arithmetic,
                    )?;
                    newest = Some(item.value);
                }
                let node_index = usize::from(index);
                let value = if let Some(value) = newest {
                    let retained = state_slice(state, node.state_offset, node.state_bytes)
                        .ok_or_else(|| {
                            latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None)
                        })?;
                    retained.copy_from_slice(&GraphIrBooleanValue { value }.encode());
                    initialized[node_index] = true;
                    value
                } else if initialized[node_index] {
                    let retained = state_slice(state, node.state_offset, node.state_bytes)
                        .ok_or_else(|| {
                            latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None)
                        })?;
                    GraphIrBooleanValue::from_slice(retained)
                        .map_err(|_| {
                            latch_fault(fault, GraphExecutionFault::QueueCorrupt, index, None)
                        })?
                        .value
                } else {
                    return Err(latch_fault(
                        fault,
                        GraphExecutionFault::MissingInitialValue,
                        index,
                        None,
                    ));
                };
                emit_realtime_outputs(
                    package,
                    metadata.output_channels[node_index],
                    channels,
                    queues,
                    fault,
                    GraphIrBooleanStreamItem {
                        value,
                        source_tick: tick,
                        sequence: tick,
                    },
                    &mut report,
                )?;
            }
            GraphIrOpcode::BooleanStreamSink => {
                let input = required_input(metadata, index, fault)?;
                while let Some(item) = pop_realtime_due(
                    package,
                    input,
                    channels,
                    queues,
                    bridge,
                    fault,
                    start_cycle,
                    cycle,
                )? {
                    report.items_consumed = checked_report_u32(
                        fault,
                        report.items_consumed,
                        input,
                        GraphExecutionFault::Arithmetic,
                    )?;
                    report.sink_items = checked_report_u32(
                        fault,
                        report.sink_items,
                        input,
                        GraphExecutionFault::Arithmetic,
                    )?;
                    report.last_sink_value = Some(item.value);
                }
            }
            GraphIrOpcode::StableBooleanInput => {
                let node_index = usize::from(index);
                if metadata.input_channel[node_index] != NO_CHANNEL {
                    return Err(latch_fault(
                        fault,
                        GraphExecutionFault::RuntimeShape,
                        index,
                        None,
                    ));
                }
                let resource = decode_graph_resource_parameter(node.parameter).map_err(|_| {
                    latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None)
                })?;
                let value = resource_input(resource).ok_or_else(|| {
                    latch_fault(fault, GraphExecutionFault::ResourceUnavailable, index, None)
                })?;
                emit_realtime_outputs(
                    package,
                    metadata.output_channels[node_index],
                    channels,
                    queues,
                    fault,
                    GraphIrBooleanStreamItem {
                        value,
                        source_tick: tick,
                        sequence: tick,
                    },
                    &mut report,
                )?;
            }
            GraphIrOpcode::StableBooleanPairAll => {
                let node_index = usize::from(index);
                if metadata.input_channel[node_index] != NO_CHANNEL {
                    return Err(latch_fault(
                        fault,
                        GraphExecutionFault::RuntimeShape,
                        index,
                        None,
                    ));
                }
                let (first_resource, second_resource) =
                    decode_graph_resource_pair_parameter(node.parameter).map_err(|_| {
                        latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None)
                    })?;
                let first = resource_input(first_resource);
                let second = resource_input(second_resource);
                let (Some(first), Some(second)) = (first, second) else {
                    return Err(latch_fault(
                        fault,
                        GraphExecutionFault::ResourceUnavailable,
                        index,
                        None,
                    ));
                };
                emit_realtime_outputs(
                    package,
                    metadata.output_channels[node_index],
                    channels,
                    queues,
                    fault,
                    GraphIrBooleanStreamItem {
                        value: first && second,
                        source_tick: tick,
                        sequence: tick,
                    },
                    &mut report,
                )?;
            }
            _ => {
                return Err(latch_fault(
                    fault,
                    GraphExecutionFault::RuntimeShape,
                    index,
                    None,
                ));
            }
        }
    }
    Ok(report)
}

fn required_input(
    metadata: &GraphRuntimeMetadata,
    node: u16,
    fault: &LatestSignal,
) -> Result<u8, GraphExecutionError> {
    let input = metadata.input_channel[usize::from(node)];
    if input == NO_CHANNEL {
        Err(latch_fault(
            fault,
            GraphExecutionFault::RuntimeShape,
            node,
            None,
        ))
    } else {
        Ok(input)
    }
}

#[allow(
    clippy::too_many_arguments,
    reason = "fanout retains distinct local and bridge ownership"
)]
fn emit_service_outputs<const CHANNELS: usize, const BRIDGE: usize>(
    package: &GraphIrPackage,
    mut mask: u64,
    local: &mut [u8; CHANNELS],
    local_queues: &mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    bridge: &BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE>>>,
    fault: &LatestSignal,
    item: GraphIrBooleanStreamItem,
    report: &mut GraphReleaseReport,
) -> Result<(), GraphExecutionError> {
    while mask != 0 {
        let index = mask.trailing_zeros() as u16;
        mask &= mask - 1;
        let channel = package
            .channel(index)
            .ok_or_else(|| latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None))?;
        let result = match channel.owner {
            GraphIrChannelOwner::Service => {
                queue_push(local, &mut local_queues[usize::from(index)], channel, item)
            }
            GraphIrChannelOwner::ServiceToRealtime => bridge.lock(|cell| {
                let mut arena = cell.borrow_mut();
                let BridgeArena { bytes, queues } = &mut *arena;
                queue_push(bytes, &mut queues[usize::from(index)], channel, item)
            }),
            GraphIrChannelOwner::Realtime => Err(QueueIssue::Shape),
        };
        result.map_err(|issue| queue_error(fault, issue, index))?;
        report.items_emitted = checked_report_u32(
            fault,
            report.items_emitted,
            index,
            GraphExecutionFault::Arithmetic,
        )?;
    }
    Ok(())
}

fn emit_realtime_outputs<const CHANNELS: usize>(
    package: &GraphIrPackage,
    mut mask: u64,
    local: &mut [u8; CHANNELS],
    local_queues: &mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    fault: &LatestSignal,
    item: GraphIrBooleanStreamItem,
    report: &mut GraphReleaseReport,
) -> Result<(), GraphExecutionError> {
    while mask != 0 {
        let index = mask.trailing_zeros() as u16;
        mask &= mask - 1;
        let channel = package
            .channel(index)
            .ok_or_else(|| latch_fault(fault, GraphExecutionFault::RuntimeShape, index, None))?;
        if channel.owner != GraphIrChannelOwner::Realtime {
            return Err(latch_fault(
                fault,
                GraphExecutionFault::RuntimeShape,
                index,
                None,
            ));
        }
        queue_push(local, &mut local_queues[usize::from(index)], channel, item)
            .map_err(|issue| queue_error(fault, issue, index))?;
        report.items_emitted = checked_report_u32(
            fault,
            report.items_emitted,
            index,
            GraphExecutionFault::Arithmetic,
        )?;
    }
    Ok(())
}

#[allow(
    clippy::too_many_arguments,
    reason = "consumer selection retains distinct local and bridge ownership"
)]
fn pop_realtime_due<const CHANNELS: usize, const BRIDGE: usize>(
    package: &GraphIrPackage,
    index: u8,
    local: &mut [u8; CHANNELS],
    local_queues: &mut [QueueCursor; MAX_GRAPH_IR_CHANNELS],
    bridge: &BlockingMutex<CriticalSectionRawMutex, RefCell<BridgeArena<BRIDGE>>>,
    fault: &LatestSignal,
    start_cycle: u64,
    cycle: DeviceCycle,
) -> Result<Option<GraphIrBooleanStreamItem>, GraphExecutionError> {
    let channel_index = u16::from(index);
    let channel = package.channel(channel_index).ok_or_else(|| {
        latch_fault(
            fault,
            GraphExecutionFault::RuntimeShape,
            channel_index,
            None,
        )
    })?;
    let source = package.node(channel.source_node).ok_or_else(|| {
        latch_fault(
            fault,
            GraphExecutionFault::RuntimeShape,
            channel_index,
            None,
        )
    })?;
    let result = match channel.owner {
        GraphIrChannelOwner::Realtime => queue_pop_due(
            local,
            &mut local_queues[usize::from(index)],
            channel,
            source.period_cycles,
            start_cycle,
            cycle.0,
        ),
        GraphIrChannelOwner::ServiceToRealtime => bridge.lock(|cell| {
            let mut arena = cell.borrow_mut();
            let BridgeArena { bytes, queues } = &mut *arena;
            queue_pop_due(
                bytes,
                &mut queues[usize::from(index)],
                channel,
                source.period_cycles,
                start_cycle,
                cycle.0,
            )
        }),
        GraphIrChannelOwner::Service => Err(QueueIssue::Shape),
    };
    result.map_err(|issue| queue_error(fault, issue, channel_index))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum QueueIssue {
    Full,
    Corrupt,
    Arithmetic,
    Shape,
}

fn queue_push(
    arena: &mut [u8],
    cursor: &mut QueueCursor,
    channel: GraphIrChannel,
    item: GraphIrBooleanStreamItem,
) -> Result<(), QueueIssue> {
    if channel.full_policy != GraphIrFullPolicy::Fault
        || cursor.head >= channel.capacity
        || cursor.len > channel.capacity
    {
        return Err(QueueIssue::Shape);
    }
    if cursor.len == channel.capacity {
        return Err(QueueIssue::Full);
    }
    let slot = cursor
        .head
        .checked_add(cursor.len)
        .ok_or(QueueIssue::Arithmetic)?
        % channel.capacity;
    let bytes = queue_slot_mut(arena, channel, slot)?;
    bytes.copy_from_slice(&item.encode());
    cursor.len = cursor.len.checked_add(1).ok_or(QueueIssue::Arithmetic)?;
    Ok(())
}

fn queue_pop_due(
    arena: &mut [u8],
    cursor: &mut QueueCursor,
    channel: GraphIrChannel,
    source_period: u64,
    start_cycle: u64,
    current_cycle: u64,
) -> Result<Option<GraphIrBooleanStreamItem>, QueueIssue> {
    if cursor.head >= channel.capacity || cursor.len > channel.capacity {
        return Err(QueueIssue::Shape);
    }
    if cursor.len == 0 {
        return Ok(None);
    }
    let item = GraphIrBooleanStreamItem::from_slice(queue_slot(arena, channel, cursor.head)?)
        .map_err(|_| QueueIssue::Corrupt)?;
    if item.sequence != item.source_tick {
        return Err(QueueIssue::Corrupt);
    }
    let due_cycle = item
        .source_tick
        .checked_mul(source_period)
        .and_then(|offset| start_cycle.checked_add(offset))
        .ok_or(QueueIssue::Arithmetic)?;
    if due_cycle > current_cycle {
        return Ok(None);
    }
    cursor.head = cursor.head.checked_add(1).ok_or(QueueIssue::Arithmetic)? % channel.capacity;
    cursor.len = cursor.len.checked_sub(1).ok_or(QueueIssue::Shape)?;
    Ok(Some(item))
}

fn queue_slot(arena: &[u8], channel: GraphIrChannel, slot: u32) -> Result<&[u8], QueueIssue> {
    let (start, end) = queue_slot_range(channel, slot)?;
    arena.get(start..end).ok_or(QueueIssue::Shape)
}

fn queue_slot_mut(
    arena: &mut [u8],
    channel: GraphIrChannel,
    slot: u32,
) -> Result<&mut [u8], QueueIssue> {
    let (start, end) = queue_slot_range(channel, slot)?;
    arena.get_mut(start..end).ok_or(QueueIssue::Shape)
}

fn queue_slot_range(channel: GraphIrChannel, slot: u32) -> Result<(usize, usize), QueueIssue> {
    if slot >= channel.capacity || channel.item_bytes != BOOLEAN_STREAM_ITEM_BYTES {
        return Err(QueueIssue::Shape);
    }
    let relative = slot
        .checked_mul(channel.item_bytes)
        .ok_or(QueueIssue::Arithmetic)?;
    let start = channel
        .storage_offset
        .checked_add(relative)
        .ok_or(QueueIssue::Arithmetic)?;
    let end = start
        .checked_add(channel.item_bytes)
        .ok_or(QueueIssue::Arithmetic)?;
    if end
        > channel
            .storage_offset
            .checked_add(channel.storage_bytes)
            .ok_or(QueueIssue::Arithmetic)?
    {
        return Err(QueueIssue::Shape);
    }
    Ok((
        usize::try_from(start).map_err(|_| QueueIssue::Arithmetic)?,
        usize::try_from(end).map_err(|_| QueueIssue::Arithmetic)?,
    ))
}

fn state_slice(state: &mut [u8], offset: u32, bytes: u32) -> Option<&mut [u8]> {
    if bytes != BOOLEAN_LATEST_STATE_BYTES {
        return None;
    }
    let start = usize::try_from(offset).ok()?;
    let end = start.checked_add(usize::try_from(bytes).ok()?)?;
    state.get_mut(start..end)
}

fn check_capacity(
    arena: GraphRuntimeArena,
    required: u32,
    available: usize,
) -> Result<(), GraphRuntimeError> {
    if usize::try_from(required)
        .ok()
        .is_some_and(|value| value <= available)
    {
        Ok(())
    } else {
        Err(GraphRuntimeError::Capacity {
            arena,
            required,
            available,
        })
    }
}

fn cursor_after_prime(
    schedule: alumina_graph_ir::GraphIrSchedule,
    start: DeviceCycle,
) -> Result<DomainCursor, GraphRuntimeError> {
    if schedule.node_count == 0 {
        return Ok(DomainCursor::EMPTY);
    }
    let next_cycle = start
        .0
        .checked_add(schedule.period_cycles)
        .ok_or(GraphRuntimeError::Arithmetic)?;
    Ok(DomainCursor {
        present: true,
        next_cycle,
        next_tick: 1,
        period_cycles: schedule.period_cycles,
        executor_reserve_cycles: schedule.executor_reserve_cycles,
    })
}

const fn cursor_at_start(
    schedule: alumina_graph_ir::GraphIrSchedule,
    start: DeviceCycle,
) -> DomainCursor {
    if schedule.node_count == 0 {
        DomainCursor::EMPTY
    } else {
        DomainCursor {
            present: true,
            next_cycle: start.0,
            next_tick: 0,
            period_cycles: schedule.period_cycles,
            executor_reserve_cycles: schedule.executor_reserve_cycles,
        }
    }
}

fn cursor_release_window(
    cursor: &DomainCursor,
) -> Result<Option<GraphReleaseWindow>, GraphRuntimeError> {
    if !cursor.present {
        return Ok(None);
    }
    let latest_dispatch_cycle = cursor
        .next_cycle
        .checked_add(cursor.executor_reserve_cycles)
        .ok_or(GraphRuntimeError::Arithmetic)?;
    Ok(Some(GraphReleaseWindow {
        scheduled_cycle: DeviceCycle(cursor.next_cycle),
        latest_dispatch_cycle: DeviceCycle(latest_dispatch_cycle),
    }))
}

fn release_prelude(
    fault: &LatestSignal,
    cursor: &DomainCursor,
    cycle: DeviceCycle,
    safety_authorized: bool,
    detail: u16,
) -> Result<(), GraphExecutionError> {
    if let Some(signal) = fault.after(0) {
        return Err(GraphExecutionError {
            observation: GraphFaultObservation::from_signal(signal),
            expected_cycle: None,
            received_cycle: None,
        });
    }
    if !safety_authorized {
        return Err(latch_fault(
            fault,
            GraphExecutionFault::SafetyNotAuthorized,
            detail,
            None,
        ));
    }
    if !cursor.present {
        return Err(latch_fault(
            fault,
            GraphExecutionFault::DomainAbsent,
            detail,
            None,
        ));
    }
    if cycle.0 != cursor.next_cycle {
        return Err(latch_fault(
            fault,
            GraphExecutionFault::UnexpectedReleaseCycle,
            detail,
            Some((DeviceCycle(cursor.next_cycle), cycle)),
        ));
    }
    Ok(())
}

fn checked_next_cycle(
    fault: &LatestSignal,
    cursor: &DomainCursor,
    detail: u16,
) -> Result<u64, GraphExecutionError> {
    cursor
        .next_cycle
        .checked_add(cursor.period_cycles)
        .ok_or_else(|| latch_fault(fault, GraphExecutionFault::Arithmetic, detail, None))
}

fn checked_next_tick(
    fault: &LatestSignal,
    cursor: &DomainCursor,
    detail: u16,
) -> Result<u64, GraphExecutionError> {
    cursor
        .next_tick
        .checked_add(1)
        .ok_or_else(|| latch_fault(fault, GraphExecutionFault::Arithmetic, detail, None))
}

fn checked_report_u16(
    fault: &LatestSignal,
    value: u16,
    detail: u16,
    reason: GraphExecutionFault,
) -> Result<u16, GraphExecutionError> {
    value
        .checked_add(1)
        .ok_or_else(|| latch_fault(fault, reason, detail, None))
}

fn checked_report_u32(
    fault: &LatestSignal,
    value: u32,
    detail: impl Into<u16>,
    reason: GraphExecutionFault,
) -> Result<u32, GraphExecutionError> {
    value
        .checked_add(1)
        .ok_or_else(|| latch_fault(fault, reason, detail.into(), None))
}

fn queue_error(fault: &LatestSignal, issue: QueueIssue, channel: u16) -> GraphExecutionError {
    let reason = match issue {
        QueueIssue::Full => GraphExecutionFault::QueueFull,
        QueueIssue::Corrupt => GraphExecutionFault::QueueCorrupt,
        QueueIssue::Arithmetic => GraphExecutionFault::Arithmetic,
        QueueIssue::Shape => GraphExecutionFault::RuntimeShape,
    };
    latch_fault(fault, reason, channel, None)
}

fn latch_fault(
    mailbox: &LatestSignal,
    fault: GraphExecutionFault,
    detail: u16,
    cycles: Option<(DeviceCycle, DeviceCycle)>,
) -> GraphExecutionError {
    let detail = u8::try_from(detail).unwrap_or(u8::MAX);
    let generation = mailbox.latch(fault as u8, detail);
    let observation = mailbox
        .after(generation.wrapping_sub(1))
        .map(GraphFaultObservation::from_signal)
        .unwrap_or(GraphFaultObservation {
            generation,
            fault,
            detail,
        });
    GraphExecutionError {
        observation,
        expected_cycle: cycles.map(|value| value.0),
        received_cycle: cycles.map(|value| value.1),
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use alumina_graph_ir::{GRAPH_IR_PACKAGE_BYTES, GraphIrHeader, GraphIrNode, GraphIrSchedule};

    use super::*;

    type FixtureRuntime = FixedGraphRuntime<0, 5, 0, 21, 42>;

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

    fn fixture_package() -> GraphIrPackage {
        fixture_package_with_periods(1_000, 2_000, 2)
    }

    fn fixture_package_with_periods(
        service_period: u64,
        realtime_period: u64,
        bridge_capacity: u32,
    ) -> GraphIrPackage {
        let bridge_bytes = bridge_capacity * BOOLEAN_STREAM_ITEM_BYTES;
        GraphIrPackage::encode(
            GraphIrHeader {
                device_id: DeviceId([1; 16]),
                graph_digest: digest(2),
                implementation_digest: digest(3),
                capability_digest: digest(4),
                config_digest: digest(5),
                service_schedule: GraphIrSchedule {
                    clock_id: 10,
                    period_cycles: service_period,
                    total_wcet_cycles: 20,
                    executor_reserve_cycles: 100,
                    node_count: 1,
                },
                realtime_schedule: GraphIrSchedule {
                    clock_id: 20,
                    period_cycles: realtime_period,
                    total_wcet_cycles: 60,
                    executor_reserve_cycles: 100,
                    node_count: 2,
                },
                total_state_bytes: 5,
                service_state_bytes: 0,
                realtime_state_bytes: 5,
                channel_storage_bytes: bridge_bytes + BOOLEAN_STREAM_ITEM_BYTES,
                bridge_storage_bytes: bridge_bytes,
            },
            &[
                GraphIrNode {
                    graph_node_id: 10,
                    domain: GraphIrDomain::Service,
                    opcode: GraphIrOpcode::BooleanStreamConstant,
                    schedule_clock_id: 10,
                    period_cycles: service_period,
                    wcet_cycles: 20,
                    state_offset: 0,
                    state_bytes: 0,
                    parameter: 1,
                },
                GraphIrNode {
                    graph_node_id: 20,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanLatest,
                    schedule_clock_id: 20,
                    period_cycles: realtime_period,
                    wcet_cycles: 40,
                    state_offset: 0,
                    state_bytes: 5,
                    parameter: 0,
                },
                GraphIrNode {
                    graph_node_id: 30,
                    domain: GraphIrDomain::Realtime,
                    opcode: GraphIrOpcode::BooleanStreamSink,
                    schedule_clock_id: 20,
                    period_cycles: realtime_period,
                    wcet_cycles: 20,
                    state_offset: 5,
                    state_bytes: 0,
                    parameter: 0,
                },
            ],
            &[
                GraphIrChannel {
                    graph_wire_id: 100,
                    source_node: 0,
                    target_node: 1,
                    owner: GraphIrChannelOwner::ServiceToRealtime,
                    full_policy: GraphIrFullPolicy::Fault,
                    capacity: bridge_capacity,
                    item_bytes: 21,
                    storage_offset: 0,
                    storage_bytes: bridge_bytes,
                },
                GraphIrChannel {
                    graph_wire_id: 200,
                    source_node: 1,
                    target_node: 2,
                    owner: GraphIrChannelOwner::Realtime,
                    full_policy: GraphIrFullPolicy::Fault,
                    capacity: 1,
                    item_bytes: 21,
                    storage_offset: 0,
                    storage_bytes: 21,
                },
            ],
        )
        .unwrap()
    }

    #[test]
    fn install_is_identity_exact_capacity_bounded_and_transactional() {
        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        assert_eq!(
            runtime.install(package.bytes(), package.digest(), authority(), false),
            Err(GraphRuntimeError::MutationForbidden)
        );
        assert_eq!(runtime.phase(), GraphRuntimePhase::Empty);

        let mut wrong = authority();
        wrong.config_digest = digest(9);
        assert_eq!(
            runtime.install(package.bytes(), package.digest(), wrong, true),
            Err(GraphRuntimeError::Identity(
                GraphRuntimeIdentity::Configuration
            ))
        );
        assert_eq!(runtime.phase(), GraphRuntimePhase::Empty);

        let mut small = FixedGraphRuntime::<0, 5, 0, 21, 41>::new();
        assert_eq!(
            small.install(package.bytes(), package.digest(), authority(), true),
            Err(GraphRuntimeError::Capacity {
                arena: GraphRuntimeArena::ServiceToRealtime,
                required: 42,
                available: 41,
            })
        );
        assert_eq!(small.phase(), GraphRuntimePhase::Empty);

        let report = runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        assert_eq!(report.package_digest, package.digest());
        assert_eq!(report.graph_digest, digest(2));
        assert_eq!(report.usage.payload_bytes(), Ok(68));
        assert_eq!(report.static_runtime_bytes, size_of::<FixtureRuntime>());
        assert_eq!(runtime.phase(), GraphRuntimePhase::Installed);
        assert_eq!(
            runtime.install(package.bytes(), package.digest(), authority(), true),
            Err(GraphRuntimeError::Phase {
                required: GraphRuntimePhase::Empty,
                actual: GraphRuntimePhase::Installed,
            })
        );
    }

    #[test]
    fn service_prime_and_split_realtime_replay_exact_multirate_values() {
        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        assert_eq!(
            runtime.prepare_start(DeviceCycle(10_000), false),
            Err(GraphRuntimeError::ExecutionForbidden)
        );
        let start = runtime.prepare_start(DeviceCycle(10_000), true).unwrap();
        let primed = start.primed_service_release.unwrap();
        assert_eq!(primed.release_tick, 0);
        assert_eq!(primed.items_emitted, 1);

        let (mut service, mut realtime) = runtime.split().unwrap();
        let first = realtime
            .release(DeviceCycle(10_000), true, |_| None)
            .unwrap();
        assert_eq!(first.release_tick, 0);
        assert_eq!(first.nodes_executed, 2);
        assert_eq!(first.items_consumed, 2);
        assert_eq!(first.items_emitted, 1);
        assert_eq!(first.sink_items, 1);
        assert_eq!(first.last_sink_value, Some(true));

        assert_eq!(
            service
                .release(DeviceCycle(11_000), true)
                .unwrap()
                .release_tick,
            1
        );
        assert_eq!(
            service
                .release(DeviceCycle(12_000), true)
                .unwrap()
                .release_tick,
            2
        );
        let second = realtime
            .release(DeviceCycle(12_000), true, |_| None)
            .unwrap();
        assert_eq!(second.release_tick, 1);
        assert_eq!(second.items_consumed, 3);
        assert_eq!(second.items_emitted, 1);
        assert_eq!(second.sink_items, 1);
        assert_eq!(second.last_sink_value, Some(true));
        assert_eq!(service.fault_after(0), None);
        assert_eq!(realtime.fault_after(0), None);
    }

    #[test]
    fn queue_full_fault_is_shared_and_stops_both_domains() {
        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        runtime.prepare_start(DeviceCycle(10_000), true).unwrap();
        let (mut service, mut realtime) = runtime.split().unwrap();
        service.release(DeviceCycle(11_000), true).unwrap();
        let overflow = service.release(DeviceCycle(12_000), true).unwrap_err();
        assert_eq!(overflow.observation.fault, GraphExecutionFault::QueueFull);
        assert_eq!(overflow.observation.detail, 0);
        assert_eq!(service.fault_after(0), Some(overflow.observation));
        let stopped = realtime
            .release(DeviceCycle(10_000), true, |_| None)
            .unwrap_err();
        assert_eq!(stopped.observation, overflow.observation);
    }

    #[test]
    fn latest_retains_the_primed_value_when_realtime_runs_faster() {
        type FasterRealtime = FixedGraphRuntime<0, 5, 0, 21, 21>;
        let package = fixture_package_with_periods(2_000, 1_000, 1);
        let mut runtime = FasterRealtime::new();
        runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        runtime.prepare_start(DeviceCycle(10_000), true).unwrap();
        let (mut service, mut realtime) = runtime.split().unwrap();

        let first = realtime
            .release(DeviceCycle(10_000), true, |_| None)
            .unwrap();
        assert_eq!(first.items_consumed, 2);
        assert_eq!(first.last_sink_value, Some(true));
        let held = realtime
            .release(DeviceCycle(11_000), true, |_| None)
            .unwrap();
        assert_eq!(held.items_consumed, 1);
        assert_eq!(held.items_emitted, 1);
        assert_eq!(held.last_sink_value, Some(true));
        service.release(DeviceCycle(12_000), true).unwrap();
        let refreshed = realtime
            .release(DeviceCycle(12_000), true, |_| None)
            .unwrap();
        assert_eq!(refreshed.items_consumed, 2);
        assert_eq!(refreshed.last_sink_value, Some(true));
    }

    #[test]
    fn missing_release_safety_authority_latches_across_both_cores() {
        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        runtime.prepare_start(DeviceCycle(10_000), true).unwrap();
        let (mut service, mut realtime) = runtime.split().unwrap();
        let denied = realtime
            .release(DeviceCycle(10_000), false, |_| None)
            .unwrap_err();
        assert_eq!(
            denied.observation.fault,
            GraphExecutionFault::SafetyNotAuthorized
        );
        let stopped = service.release(DeviceCycle(11_000), true).unwrap_err();
        assert_eq!(stopped.observation, denied.observation);
    }

    #[test]
    fn queue_corruption_and_wrong_release_cycle_fail_before_state_advances() {
        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        runtime.prepare_start(DeviceCycle(10_000), true).unwrap();
        runtime.bridge.lock(|cell| cell.borrow_mut().bytes[0] = 2);
        let (_, mut realtime) = runtime.split().unwrap();
        let corrupt = realtime
            .release(DeviceCycle(10_000), true, |_| None)
            .unwrap_err();
        assert_eq!(corrupt.observation.fault, GraphExecutionFault::QueueCorrupt);

        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        runtime
            .install(package.bytes(), package.digest(), authority(), true)
            .unwrap();
        runtime.prepare_start(DeviceCycle(20_000), true).unwrap();
        let (mut service, _) = runtime.split().unwrap();
        let wrong = service.release(DeviceCycle(21_001), true).unwrap_err();
        assert_eq!(
            wrong.observation.fault,
            GraphExecutionFault::UnexpectedReleaseCycle
        );
        assert_eq!(wrong.expected_cycle, Some(DeviceCycle(21_000)));
        assert_eq!(wrong.received_cycle, Some(DeviceCycle(21_001)));
        assert_eq!(service.fault_after(0), Some(wrong.observation));
    }

    #[test]
    fn exact_length_and_package_digest_are_required_before_any_install() {
        let package = fixture_package();
        let mut runtime = FixtureRuntime::new();
        assert_eq!(
            runtime.install(
                &package.bytes()[..GRAPH_IR_PACKAGE_BYTES - 1],
                package.digest(),
                authority(),
                true,
            ),
            Err(GraphRuntimeError::Package(GraphIrError::Length))
        );
        assert_eq!(runtime.phase(), GraphRuntimePhase::Empty);
        assert!(matches!(
            runtime.install(package.bytes(), digest(99), authority(), true),
            Err(GraphRuntimeError::PackageDigest { .. })
        ));
        assert_eq!(runtime.phase(), GraphRuntimePhase::Empty);
        assert_eq!(
            runtime.install(package.bytes(), Digest::ZERO, authority(), true),
            Err(GraphRuntimeError::MissingExpectedDigest)
        );
    }

    #[test]
    fn split_endpoint_types_can_move_to_their_pinned_core_start_closures() {
        fn assert_send<T: Send>() {}
        assert_send::<FixedGraphServiceEndpoint<'static, 0, 0, 42>>();
        assert_send::<FixedGraphRealtimeEndpoint<'static, 5, 21, 42>>();
    }
}
