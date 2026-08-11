#![no_std]
#![doc = "Bounded service/realtime ownership boundary for dual-core Alumina firmware."]

use core::mem::size_of;
use core::sync::atomic::{AtomicU32, Ordering};

use alumina_machine_ir::ExecutionBlock;
use alumina_protocol::{DeviceCycle, Digest, FrameHeader, FrameKind, HeaderError};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::{Channel, Receiver, Sender, TryReceiveError, TrySendError};

/// Default number of admitted service-to-realtime commands.
pub const COMMAND_QUEUE_DEPTH: usize = 8;
/// Default number of lossy realtime-to-service telemetry samples.
pub const TELEMETRY_QUEUE_DEPTH: usize = 32;
/// Fixed payload capacity of one cross-core command.
pub const COMMAND_PAYLOAD_BYTES: usize = 336;
/// Fixed payload capacity of one cross-core telemetry sample.
pub const TELEMETRY_PAYLOAD_BYTES: usize = 128;
/// Initial number of canonical 512-byte work units owned by the RT horizon queue.
pub const WORK_QUEUE_DEPTH: usize = 8;
/// Default application-core stack size in 32-bit words.
pub const APP_CORE_STACK_WORDS: usize = 8_192;

/// The production-shaped boundary used by both initial board images.
pub type DefaultBoundary = IntercoreBoundary<
    COMMAND_QUEUE_DEPTH,
    TELEMETRY_QUEUE_DEPTH,
    COMMAND_PAYLOAD_BYTES,
    TELEMETRY_PAYLOAD_BYTES,
    WORK_QUEUE_DEPTH,
>;
/// Core-0 endpoint for the production-shaped boundary.
pub type DefaultServiceEndpoint = ServiceEndpoint<
    'static,
    COMMAND_QUEUE_DEPTH,
    TELEMETRY_QUEUE_DEPTH,
    COMMAND_PAYLOAD_BYTES,
    TELEMETRY_PAYLOAD_BYTES,
    WORK_QUEUE_DEPTH,
>;
/// Core-1 endpoint for the production-shaped boundary.
pub type DefaultRealtimeEndpoint = RealtimeEndpoint<
    'static,
    COMMAND_QUEUE_DEPTH,
    TELEMETRY_QUEUE_DEPTH,
    COMMAND_PAYLOAD_BYTES,
    TELEMETRY_PAYLOAD_BYTES,
    WORK_QUEUE_DEPTH,
>;

/// A fixed-size frame whose unused bytes are always zeroed.
///
/// The frame owns all bytes crossing cores. It cannot contain a reference,
/// allocator-owned value, peripheral token, or trait object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct IntercoreFrame<const PAYLOAD: usize> {
    header: FrameHeader,
    payload: [u8; PAYLOAD],
}

impl<const PAYLOAD: usize> IntercoreFrame<PAYLOAD> {
    /// Copies a bounded payload into a fully owned frame.
    pub fn new(
        kind: FrameKind,
        sequence: u32,
        cycle: DeviceCycle,
        config_digest: Digest,
        payload: &[u8],
    ) -> Result<Self, FrameError> {
        let payload_len =
            u32::try_from(payload.len()).map_err(|_| FrameError::PayloadTooLarge {
                received: payload.len(),
                capacity: PAYLOAD,
            })?;
        if payload.len() > PAYLOAD {
            return Err(FrameError::PayloadTooLarge {
                received: payload.len(),
                capacity: PAYLOAD,
            });
        }

        let mut storage = [0; PAYLOAD];
        storage[..payload.len()].copy_from_slice(payload);
        Ok(Self {
            header: FrameHeader::new(kind, payload_len, sequence, cycle, config_digest),
            payload: storage,
        })
    }

    /// Returns the universal frame header.
    pub const fn header(&self) -> &FrameHeader {
        &self.header
    }

    /// Returns only initialized payload bytes after checking the stored length.
    pub fn payload(&self) -> Result<&[u8], FrameError> {
        let length =
            usize::try_from(self.header.payload_len).map_err(|_| FrameError::PayloadTooLarge {
                received: usize::MAX,
                capacity: PAYLOAD,
            })?;
        self.payload
            .get(..length)
            .ok_or(FrameError::PayloadTooLarge {
                received: length,
                capacity: PAYLOAD,
            })
    }

    /// Validates the universal header, expected direction, and fixed capacity.
    pub fn validate(&self, expected_kind: FrameKind) -> Result<(), FrameError> {
        let maximum = u32::try_from(PAYLOAD).unwrap_or(u32::MAX);
        self.header.validate(maximum).map_err(FrameError::Header)?;
        if self.header.kind != expected_kind {
            return Err(FrameError::WrongKind {
                expected: expected_kind,
                received: self.header.kind,
            });
        }
        let _ = self.payload()?;
        Ok(())
    }

    #[cfg(test)]
    fn header_mut(&mut self) -> &mut FrameHeader {
        &mut self.header
    }
}

/// Cross-core frame construction or admission failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    /// Universal protocol header validation failed.
    Header(HeaderError),
    /// The frame was placed on the wrong directional queue.
    WrongKind {
        /// Required frame family.
        expected: FrameKind,
        /// Observed frame family.
        received: FrameKind,
    },
    /// Payload cannot fit in the fixed frame.
    PayloadTooLarge {
        /// Requested or encoded length.
        received: usize,
        /// Frame capacity.
        capacity: usize,
    },
}

/// Safety action delivered independently of the ordered command queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UrgentKind {
    /// Request a certified constrained hold.
    Hold = 1,
    /// Request a normal stop and de-energization.
    Stop = 2,
    /// Request the shortest board-specific emergency-safe path.
    EmergencyStop = 3,
    /// Request consideration of a physical fault reset.
    ResetRequest = 4,
}

/// One atomic latest-value signal observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalSnapshot {
    /// Wrapping nonzero publication generation.
    pub generation: u16,
    /// Signal kind or fault code.
    pub code: u8,
    /// Compact reason/detail selected by the publisher.
    pub detail: u8,
}

/// Lock-free latest-value mailbox used for urgent requests and latched faults.
///
/// All fields fit in one `AtomicU32`, so consumers can never observe a torn
/// kind/reason pair. Generation zero means that nothing has been published.
pub struct LatestSignal {
    packed: AtomicU32,
}

impl LatestSignal {
    /// Constructs an empty mailbox.
    pub const fn new() -> Self {
        Self {
            packed: AtomicU32::new(0),
        }
    }

    /// Publishes a new value without waiting for either cross-core queue.
    pub fn publish(&self, code: u8, detail: u8) -> u16 {
        let previous = self
            .packed
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                let old_generation = (old >> 16) as u16;
                let mut generation = old_generation.wrapping_add(1);
                if generation == 0 {
                    generation = 1;
                }
                Some(pack_signal(generation, code, detail))
            })
            .unwrap_or_else(|unreachable| unreachable);

        let mut generation = ((previous >> 16) as u16).wrapping_add(1);
        if generation == 0 {
            generation = 1;
        }
        generation
    }

    /// Returns the current value only when it is newer than `last_generation`.
    pub fn after(&self, last_generation: u16) -> Option<SignalSnapshot> {
        let packed = self.packed.load(Ordering::Acquire);
        let generation = (packed >> 16) as u16;
        if generation == 0 || generation == last_generation {
            return None;
        }
        Some(SignalSnapshot {
            generation,
            code: ((packed >> 8) & 0xff) as u8,
            detail: (packed & 0xff) as u8,
        })
    }
}

impl Default for LatestSignal {
    fn default() -> Self {
        Self::new()
    }
}

const fn pack_signal(generation: u16, code: u8, detail: u8) -> u32 {
    ((generation as u32) << 16) | ((code as u32) << 8) | detail as u32
}

/// Static queues and independent latest-value mailboxes between the two cores.
pub struct IntercoreBoundary<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize = WORK_QUEUE_DEPTH,
> {
    commands: Channel<CriticalSectionRawMutex, IntercoreFrame<COMMAND_PAYLOAD>, COMMANDS>,
    work: Channel<CriticalSectionRawMutex, ExecutionBlock, WORK_BLOCKS>,
    telemetry: Channel<CriticalSectionRawMutex, IntercoreFrame<TELEMETRY_PAYLOAD>, TELEMETRY>,
    urgent: LatestSignal,
    fault: LatestSignal,
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> IntercoreBoundary<COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    /// Creates an empty statically allocatable boundary.
    pub const fn new() -> Self {
        Self {
            commands: Channel::new(),
            work: Channel::new(),
            telemetry: Channel::new(),
            urgent: LatestSignal::new(),
            fault: LatestSignal::new(),
        }
    }

    /// Uniquely partitions the boundary into directional core endpoints.
    ///
    /// Requiring a mutable static reference prevents safe code from splitting
    /// the same boundary twice.
    pub fn split(
        &'static mut self,
    ) -> (
        ServiceEndpoint<
            'static,
            COMMANDS,
            TELEMETRY,
            COMMAND_PAYLOAD,
            TELEMETRY_PAYLOAD,
            WORK_BLOCKS,
        >,
        RealtimeEndpoint<
            'static,
            COMMANDS,
            TELEMETRY,
            COMMAND_PAYLOAD,
            TELEMETRY_PAYLOAD,
            WORK_BLOCKS,
        >,
    ) {
        let service = ServiceEndpoint {
            command_tx: self.commands.sender(),
            work_tx: self.work.sender(),
            telemetry_rx: self.telemetry.receiver(),
            urgent: &self.urgent,
            fault: &self.fault,
        };
        let realtime = RealtimeEndpoint {
            command_rx: self.commands.receiver(),
            work_rx: self.work.receiver(),
            telemetry_tx: self.telemetry.sender(),
            urgent: &self.urgent,
            fault: &self.fault,
        };
        (service, realtime)
    }

    /// Returns statically reserved bytes, excluding channel bookkeeping.
    pub const fn payload_storage_bytes() -> usize {
        COMMANDS * size_of::<IntercoreFrame<COMMAND_PAYLOAD>>()
            + WORK_BLOCKS * size_of::<ExecutionBlock>()
            + TELEMETRY * size_of::<IntercoreFrame<TELEMETRY_PAYLOAD>>()
    }
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> Default
    for IntercoreBoundary<COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    fn default() -> Self {
        Self::new()
    }
}

/// Core-0 handle. Ordered commands may wait; urgent safety publication cannot.
pub struct ServiceEndpoint<
    'a,
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize = WORK_QUEUE_DEPTH,
> {
    command_tx: Sender<'a, CriticalSectionRawMutex, IntercoreFrame<COMMAND_PAYLOAD>, COMMANDS>,
    work_tx: Sender<'a, CriticalSectionRawMutex, ExecutionBlock, WORK_BLOCKS>,
    telemetry_rx:
        Receiver<'a, CriticalSectionRawMutex, IntercoreFrame<TELEMETRY_PAYLOAD>, TELEMETRY>,
    urgent: &'a LatestSignal,
    fault: &'a LatestSignal,
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> ServiceEndpoint<'_, COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    /// Waits for capacity in the ordered command queue.
    pub async fn send_command(&mut self, frame: IntercoreFrame<COMMAND_PAYLOAD>) {
        self.command_tx.send(frame).await;
    }

    /// Attempts an ordered command admission without waiting.
    pub fn try_send_command(
        &mut self,
        frame: IntercoreFrame<COMMAND_PAYLOAD>,
    ) -> Result<(), TrySendError<IntercoreFrame<COMMAND_PAYLOAD>>> {
        self.command_tx.try_send(frame)
    }

    /// Waits for one ownership credit, then transfers a complete work block.
    pub async fn send_work(&mut self, block: ExecutionBlock) {
        self.work_tx.send(block).await;
    }

    /// Transfers one work block only when a bounded ownership credit is free.
    #[allow(
        clippy::result_large_err,
        reason = "a full queue must return the inline block so core 0 retains ownership without allocation"
    )]
    pub fn try_send_work(
        &mut self,
        block: ExecutionBlock,
    ) -> Result<(), TrySendError<ExecutionBlock>> {
        self.work_tx.try_send(block)
    }

    /// Current work-block credits available to the core-0 prefetch actor.
    pub fn work_free_capacity(&self) -> usize {
        self.work_tx.free_capacity()
    }

    /// Work blocks currently owned by the queue rather than either core task.
    pub fn work_depth(&self) -> usize {
        self.work_tx.len()
    }

    /// Publishes an urgent action independently of ordered queue pressure.
    pub fn publish_urgent(&self, kind: UrgentKind, detail: u8) -> u16 {
        self.urgent.publish(kind as u8, detail)
    }

    /// Receives lossy ordinary telemetry.
    pub async fn receive_telemetry(&mut self) -> IntercoreFrame<TELEMETRY_PAYLOAD> {
        self.telemetry_rx.receive().await
    }

    /// Attempts to receive ordinary telemetry without waiting.
    pub fn try_receive_telemetry(
        &mut self,
    ) -> Result<IntercoreFrame<TELEMETRY_PAYLOAD>, TryReceiveError> {
        self.telemetry_rx.try_receive()
    }

    /// Reads a newly latched fault even if the telemetry queue is saturated.
    pub fn fault_after(&self, generation: u16) -> Option<SignalSnapshot> {
        self.fault.after(generation)
    }

    /// Current ordered-command depth for admission telemetry.
    pub fn command_depth(&self) -> usize {
        self.command_tx.len()
    }
}

/// Core-1 handle. Real-time publication never waits on service-core progress.
pub struct RealtimeEndpoint<
    'a,
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize = WORK_QUEUE_DEPTH,
> {
    command_rx: Receiver<'a, CriticalSectionRawMutex, IntercoreFrame<COMMAND_PAYLOAD>, COMMANDS>,
    work_rx: Receiver<'a, CriticalSectionRawMutex, ExecutionBlock, WORK_BLOCKS>,
    telemetry_tx: Sender<'a, CriticalSectionRawMutex, IntercoreFrame<TELEMETRY_PAYLOAD>, TELEMETRY>,
    urgent: &'a LatestSignal,
    fault: &'a LatestSignal,
}

impl<
    const COMMANDS: usize,
    const TELEMETRY: usize,
    const COMMAND_PAYLOAD: usize,
    const TELEMETRY_PAYLOAD: usize,
    const WORK_BLOCKS: usize,
> RealtimeEndpoint<'_, COMMANDS, TELEMETRY, COMMAND_PAYLOAD, TELEMETRY_PAYLOAD, WORK_BLOCKS>
{
    /// Attempts to take the next validated ordered command without waiting.
    pub fn try_receive_command(
        &mut self,
    ) -> Result<IntercoreFrame<COMMAND_PAYLOAD>, TryReceiveError> {
        self.command_rx.try_receive()
    }

    /// Takes ownership of the next canonical work block without waiting.
    pub fn try_receive_work(&mut self) -> Result<ExecutionBlock, TryReceiveError> {
        self.work_rx.try_receive()
    }

    /// Work blocks waiting behind the core-1-local block currently executing.
    pub fn work_depth(&self) -> usize {
        self.work_rx.len()
    }

    /// Reads the latest urgent request independently of command queue pressure.
    pub fn urgent_after(&self, generation: u16) -> Option<SignalSnapshot> {
        self.urgent.after(generation)
    }

    /// Attempts ordinary telemetry publication and returns immediately when full.
    pub fn try_publish_telemetry(
        &mut self,
        frame: IntercoreFrame<TELEMETRY_PAYLOAD>,
    ) -> Result<(), TrySendError<IntercoreFrame<TELEMETRY_PAYLOAD>>> {
        self.telemetry_tx.try_send(frame)
    }

    /// Publishes the latest latched fault independently of telemetry pressure.
    pub fn publish_fault(&self, fault_code: u8, detail: u8) -> u16 {
        self.fault.publish(fault_code, detail)
    }

    /// Current free telemetry slots for decimation policy.
    pub fn telemetry_free_capacity(&self) -> usize {
        self.telemetry_tx.free_capacity()
    }
}

/// Declared static and timing budget for one board image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RuntimeBudget {
    /// Reserved bytes available for boundary frames and executor stack.
    pub internal_bytes: usize,
    /// Stack allocation for the application-core scheduler, in 32-bit words.
    pub app_core_stack_words: usize,
    /// Maximum measured service/cache stall the RT horizon must cover.
    pub maximum_service_stall_cycles: u64,
    /// Minimum queued deterministic work admitted before arming.
    pub minimum_realtime_horizon_cycles: u64,
}

impl RuntimeBudget {
    /// Validates a budget against a concrete boundary shape.
    pub fn validate_for<
        const COMMANDS: usize,
        const TELEMETRY: usize,
        const COMMAND_PAYLOAD: usize,
        const TELEMETRY_PAYLOAD: usize,
    >(
        &self,
    ) -> Result<usize, BudgetError> {
        self.validate_for_with_work::<
            COMMANDS,
            TELEMETRY,
            COMMAND_PAYLOAD,
            TELEMETRY_PAYLOAD,
            WORK_QUEUE_DEPTH,
        >()
    }

    /// Validates a budget against a boundary with an explicit work-block depth.
    pub fn validate_for_with_work<
        const COMMANDS: usize,
        const TELEMETRY: usize,
        const COMMAND_PAYLOAD: usize,
        const TELEMETRY_PAYLOAD: usize,
        const WORK_BLOCKS: usize,
    >(
        &self,
    ) -> Result<usize, BudgetError> {
        if COMMANDS == 0 || TELEMETRY == 0 || WORK_BLOCKS == 0 {
            return Err(BudgetError::ZeroDepth);
        }
        if COMMAND_PAYLOAD == 0 || TELEMETRY_PAYLOAD == 0 {
            return Err(BudgetError::ZeroPayload);
        }
        if self.app_core_stack_words < 1_024 {
            return Err(BudgetError::ApplicationStackTooSmall {
                words: self.app_core_stack_words,
            });
        }
        if self.minimum_realtime_horizon_cycles <= self.maximum_service_stall_cycles {
            return Err(BudgetError::InsufficientHorizon {
                horizon: self.minimum_realtime_horizon_cycles,
                stall: self.maximum_service_stall_cycles,
            });
        }

        let frame_bytes = IntercoreBoundary::<
            COMMANDS,
            TELEMETRY,
            COMMAND_PAYLOAD,
            TELEMETRY_PAYLOAD,
            WORK_BLOCKS,
        >::payload_storage_bytes();
        let stack_bytes = self
            .app_core_stack_words
            .checked_mul(size_of::<u32>())
            .ok_or(BudgetError::SizeOverflow)?;
        let required = frame_bytes
            .checked_add(stack_bytes)
            .ok_or(BudgetError::SizeOverflow)?;
        if required > self.internal_bytes {
            return Err(BudgetError::InternalMemoryExceeded {
                required,
                available: self.internal_bytes,
            });
        }
        Ok(required)
    }
}

/// Runtime static/timing budget failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BudgetError {
    /// A queue cannot provide a boundary with zero slots.
    ZeroDepth,
    /// A frame cannot have zero payload capacity.
    ZeroPayload,
    /// Application-core stack cannot safely start the scheduler.
    ApplicationStackTooSmall {
        /// Configured words.
        words: usize,
    },
    /// Deterministic work does not cover the admitted service stall.
    InsufficientHorizon {
        /// Admitted RT horizon.
        horizon: u64,
        /// Maximum service/cache stall.
        stall: u64,
    },
    /// Static size arithmetic overflowed.
    SizeOverflow,
    /// Boundary plus application stack exceeds reserved internal memory.
    InternalMemoryExceeded {
        /// Required bytes.
        required: usize,
        /// Available bytes.
        available: usize,
    },
}

/// Accumulates exact deadline-probe observations without floating point.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DeadlineProbe {
    samples: u64,
    misses: u64,
    maximum_lateness_cycles: u64,
}

impl DeadlineProbe {
    /// Records one expected/observed cycle pair against an allowed lateness.
    pub fn observe(
        &mut self,
        expected: DeviceCycle,
        observed: DeviceCycle,
        allowed_lateness_cycles: u64,
    ) -> DeadlineObservation {
        let lateness_cycles = observed.0.saturating_sub(expected.0);
        let missed = lateness_cycles > allowed_lateness_cycles;
        self.samples = self.samples.saturating_add(1);
        if missed {
            self.misses = self.misses.saturating_add(1);
        }
        self.maximum_lateness_cycles = self.maximum_lateness_cycles.max(lateness_cycles);
        DeadlineObservation {
            lateness_cycles,
            missed,
        }
    }

    /// Total observations.
    pub const fn samples(self) -> u64 {
        self.samples
    }

    /// Observations beyond the declared allowance.
    pub const fn misses(self) -> u64 {
        self.misses
    }

    /// Worst observed nonnegative lateness.
    pub const fn maximum_lateness_cycles(self) -> u64 {
        self.maximum_lateness_cycles
    }
}

/// Result of one deadline observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeadlineObservation {
    /// Zero for early/on-time observations, otherwise exact cycles late.
    pub lateness_cycles: u64,
    /// True when lateness exceeded the declared allowance.
    pub missed: bool,
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use alloc::boxed::Box;

    use super::*;
    use alumina_machine_ir::{
        BlockExpectation, BlockValidationLimits, ExecutionSegment, StreamId, StreamTick,
        ValidationLimits,
    };
    use alumina_protocol::{FRAME_MAGIC, PROTOCOL_VERSION};

    fn frame<const N: usize>(kind: FrameKind, sequence: u32) -> IntercoreFrame<N> {
        IntercoreFrame::new(
            kind,
            sequence,
            DeviceCycle(1_000 + u64::from(sequence)),
            Digest([7; 32]),
            &[1, 2, 3],
        )
        .unwrap()
    }

    fn work_block(sequence: u32, start: u64, previous_digest: Digest) -> ExecutionBlock {
        ExecutionBlock::encode_motion(
            StreamId::new([1; 16]).unwrap(),
            Digest([2; 32]),
            Digest([3; 32]),
            sequence,
            previous_digest,
            &[ExecutionSegment {
                start_tick: StreamTick(start),
                end_tick: StreamTick(start + 10),
                delta_steps: [1, -1, 0],
                flags: 0,
            }],
        )
        .unwrap()
    }

    fn work_expectation(sequence: u32, start: u64, previous_digest: Digest) -> BlockExpectation {
        BlockExpectation {
            stream_id: StreamId([1; 16]),
            capability_digest: Digest([2; 32]),
            config_digest: Digest([3; 32]),
            sequence,
            start_tick: StreamTick(start),
            previous_digest,
        }
    }

    fn work_limits() -> BlockValidationLimits {
        BlockValidationLimits {
            maximum_block_ticks: 100,
            segment: ValidationLimits {
                maximum_segment_ticks: 100,
                maximum_steps_per_segment: 10,
            },
        }
    }

    #[test]
    fn frame_owns_zero_filled_bounded_payload() {
        let frame = frame::<8>(FrameKind::Command, 4);
        assert_eq!(frame.payload(), Ok(&[1, 2, 3][..]));
        assert_eq!(frame.validate(FrameKind::Command), Ok(()));
        assert_eq!(frame.header().magic, FRAME_MAGIC);
        assert_eq!(frame.header().version, PROTOCOL_VERSION);
    }

    #[test]
    fn malformed_or_wrong_direction_frame_is_rejected() {
        assert_eq!(
            IntercoreFrame::<2>::new(
                FrameKind::Command,
                0,
                DeviceCycle(0),
                Digest::ZERO,
                &[1, 2, 3],
            ),
            Err(FrameError::PayloadTooLarge {
                received: 3,
                capacity: 2,
            })
        );

        let mut malformed = frame::<8>(FrameKind::Telemetry, 1);
        malformed.header_mut().payload_len = 9;
        assert_eq!(
            malformed.validate(FrameKind::Telemetry),
            Err(FrameError::Header(HeaderError::PayloadTooLarge {
                received: 9,
                maximum: 8,
            }))
        );

        let wrong = frame::<8>(FrameKind::Command, 2);
        assert_eq!(
            wrong.validate(FrameKind::Telemetry),
            Err(FrameError::WrongKind {
                expected: FrameKind::Telemetry,
                received: FrameKind::Command,
            })
        );
    }

    #[test]
    fn urgent_signal_bypasses_a_full_ordered_queue() {
        type Boundary = IntercoreBoundary<2, 2, 8, 8>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();

        service
            .try_send_command(frame(FrameKind::Command, 1))
            .unwrap();
        service
            .try_send_command(frame(FrameKind::Command, 2))
            .unwrap();
        assert!(matches!(
            service.try_send_command(frame(FrameKind::Command, 3)),
            Err(TrySendError::Full(_))
        ));

        let generation = service.publish_urgent(UrgentKind::EmergencyStop, 9);
        assert_eq!(
            realtime.urgent_after(0),
            Some(SignalSnapshot {
                generation,
                code: UrgentKind::EmergencyStop as u8,
                detail: 9,
            })
        );
        assert_eq!(realtime.urgent_after(generation), None);

        assert_eq!(realtime.try_receive_command().unwrap().header().sequence, 1);
        assert_eq!(realtime.try_receive_command().unwrap().header().sequence, 2);
    }

    #[test]
    fn work_queue_transfers_fixed_ownership_and_exposes_exact_credits() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4, 2>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();
        assert_eq!(service.work_free_capacity(), 2);
        assert_eq!(service.work_depth(), 0);

        let first = work_block(0, 100, Digest::ZERO);
        let first_digest = first.header().block_digest;
        service.try_send_work(first).unwrap();
        service
            .try_send_work(work_block(1, 110, first_digest))
            .unwrap();
        assert_eq!(service.work_free_capacity(), 0);
        assert_eq!(service.work_depth(), 2);
        assert!(matches!(
            service.try_send_work(work_block(2, 120, Digest([9; 32]))),
            Err(TrySendError::Full(_))
        ));

        let generation = service.publish_urgent(UrgentKind::EmergencyStop, 4);
        assert_eq!(realtime.urgent_after(0).unwrap().generation, generation);
        let first = realtime.try_receive_work().unwrap();
        assert_eq!(service.work_free_capacity(), 1);
        assert_eq!(realtime.work_depth(), 1);
        first
            .validate_motion::<3>(work_expectation(0, 100, Digest::ZERO), work_limits())
            .unwrap();
        let second = realtime.try_receive_work().unwrap();
        second
            .validate_motion::<3>(work_expectation(1, 110, first_digest), work_limits())
            .unwrap();
        assert!(matches!(
            realtime.try_receive_work(),
            Err(TryReceiveError::Empty)
        ));
    }

    #[test]
    fn fault_signal_bypasses_full_telemetry_and_ordinary_samples_drop() {
        type Boundary = IntercoreBoundary<1, 1, 4, 4>;
        let boundary = Box::leak(Box::new(Boundary::new()));
        let (mut service, mut realtime) = boundary.split();

        realtime
            .try_publish_telemetry(frame(FrameKind::Telemetry, 1))
            .unwrap();
        assert!(matches!(
            realtime.try_publish_telemetry(frame(FrameKind::Telemetry, 2)),
            Err(TrySendError::Full(_))
        ));
        let generation = realtime.publish_fault(17, 3);
        assert_eq!(
            service.fault_after(0),
            Some(SignalSnapshot {
                generation,
                code: 17,
                detail: 3,
            })
        );
        assert_eq!(
            service.try_receive_telemetry().unwrap().header().sequence,
            1
        );
    }

    #[test]
    fn runtime_budget_covers_boundary_stack_and_service_stall() {
        let budget = RuntimeBudget {
            internal_bytes: 64 * 1_024,
            app_core_stack_words: APP_CORE_STACK_WORDS,
            maximum_service_stall_cycles: 10_000,
            minimum_realtime_horizon_cycles: 20_000,
        };
        let required = budget
            .validate_for::<
                COMMAND_QUEUE_DEPTH,
                TELEMETRY_QUEUE_DEPTH,
                COMMAND_PAYLOAD_BYTES,
                TELEMETRY_PAYLOAD_BYTES,
            >()
            .unwrap();
        assert_eq!(DefaultBoundary::payload_storage_bytes(), 13_120);
        assert_eq!(required, 45_888);
        assert_eq!(
            required,
            APP_CORE_STACK_WORDS * size_of::<u32>() + DefaultBoundary::payload_storage_bytes()
        );

        let invalid = RuntimeBudget {
            minimum_realtime_horizon_cycles: 10_000,
            ..budget
        };
        assert_eq!(
            invalid.validate_for::<1, 1, 1, 1>(),
            Err(BudgetError::InsufficientHorizon {
                horizon: 10_000,
                stall: 10_000,
            })
        );
        assert_eq!(
            budget.validate_for_with_work::<1, 1, 1, 1, 0>(),
            Err(BudgetError::ZeroDepth)
        );
    }

    #[test]
    fn deadline_probe_tracks_exact_worst_case_and_misses() {
        let mut probe = DeadlineProbe::default();
        assert_eq!(
            probe.observe(DeviceCycle(100), DeviceCycle(90), 2),
            DeadlineObservation {
                lateness_cycles: 0,
                missed: false,
            }
        );
        assert!(!probe.observe(DeviceCycle(100), DeviceCycle(102), 2).missed);
        assert!(probe.observe(DeviceCycle(100), DeviceCycle(105), 2).missed);
        assert_eq!(probe.samples(), 3);
        assert_eq!(probe.misses(), 1);
        assert_eq!(probe.maximum_lateness_cycles(), 5);
    }

    #[test]
    fn endpoint_types_can_move_to_pinned_core_start_closure() {
        fn assert_send<T: Send>() {}
        assert_send::<ServiceEndpoint<'static, 1, 1, 1, 1>>();
        assert_send::<RealtimeEndpoint<'static, 1, 1, 1, 1>>();
    }
}
