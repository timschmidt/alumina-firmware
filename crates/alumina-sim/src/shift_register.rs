//! Independent bit-level observer for a continuous PCM-short shift stream.
//!
//! This host model consumes the portable frame plan one serial bit at a time.
//! It deliberately does not model an ESP32 peripheral, DMA descriptor, FIFO, or
//! GPIO matrix. Those remain hardware-in-the-loop claims.

use alumina_shift_register::{
    BitOrder, CompleteImage, PCM_SHORT_FRAME_BITS, PcmShortFrameError, PcmShortFrameGrid,
    PcmShortMonoFrame, PlannedPcmShortFrame,
};

/// One image observed at a modeled rising frame-sync/latch edge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ObservedPcmShortLatch {
    /// Dense frame that shifted the image into the physical cascade.
    pub transmit_index: u64,
    /// Exact start cycle of that frame.
    pub starts_at: u64,
    /// Exact following boundary at which the image became visible.
    pub latch_cycle: u64,
    /// Image reconstructed from the modeled serial wire, not frame metadata.
    pub image: CompleteImage,
}

/// Fail-closed stream fault retained by [`SimPcmShortLatch`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmShortWireFault {
    /// The bootstrap image did not define one valid complete-image contract.
    Frame(PcmShortFrameError),
    /// Frame indices skipped, repeated, or arrived out of order.
    Sequence {
        /// Exact next frame required by the observer.
        expected: u64,
        /// Frame supplied by the producer.
        received: u64,
    },
    /// A frame did not occupy its exact interval on the configured grid.
    Timing {
        /// Required start boundary.
        expected_start: u64,
        /// Start claimed by the frame.
        received_start: u64,
        /// Required following latch boundary.
        expected_latch: u64,
        /// Latch cycle claimed by the frame.
        received_latch: u64,
    },
    /// A frame changed width, coverage, or physical serialization order.
    Contract,
    /// The serially reconstructed image disagreed with the image metadata.
    WireImageMismatch {
        /// Complete image claimed by the producer.
        expected_bits: u32,
        /// Complete image reconstructed one serial bit at a time.
        observed_bits: u32,
    },
    /// No dense frame was supplied by its following latch deadline.
    Starvation {
        /// Missing frame index.
        transmit_index: u64,
        /// Boundary at which that frame should have committed.
        latch_cycle: u64,
    },
    /// Checked frame-index or cycle arithmetic overflowed.
    Arithmetic,
    /// An earlier stream fault remains authoritative.
    FaultLatched,
}

/// Deterministic bit-level shift/latch model with a latched fault state.
///
/// A newly constructed model treats `bootstrap_image` as already visible at
/// the grid epoch. Each accepted dense frame reconstructs the last complete
/// chain-width serial suffix and makes it visible at the following boundary.
/// Missing or malformed frames never synthesize continued motion.
pub struct SimPcmShortLatch {
    grid: PcmShortFrameGrid,
    contract: CompleteImage,
    visible_image: CompleteImage,
    next_transmit_index: u64,
    fault: Option<PcmShortWireFault>,
}

impl SimPcmShortLatch {
    /// Starts an observer from an image established by a separate static-safe
    /// bootstrap transaction.
    pub fn new(
        grid: PcmShortFrameGrid,
        bootstrap_image: CompleteImage,
    ) -> Result<Self, PcmShortWireFault> {
        PcmShortMonoFrame::new(bootstrap_image).map_err(PcmShortWireFault::Frame)?;
        Ok(Self {
            grid,
            contract: bootstrap_image,
            visible_image: bootstrap_image,
            next_transmit_index: 0,
            fault: None,
        })
    }

    /// Consumes all 64 modeled wire bits and observes the following latch edge.
    pub fn consume_frame(
        &mut self,
        frame: PlannedPcmShortFrame,
    ) -> Result<ObservedPcmShortLatch, PcmShortWireFault> {
        if self.fault.is_some() {
            return Err(PcmShortWireFault::FaultLatched);
        }
        if frame.transmit_index != self.next_transmit_index {
            return self.fail(PcmShortWireFault::Sequence {
                expected: self.next_transmit_index,
                received: frame.transmit_index,
            });
        }
        let expected_start = match self.grid.boundary_cycle(self.next_transmit_index) {
            Ok(cycle) => cycle,
            Err(_) => return self.fail(PcmShortWireFault::Arithmetic),
        };
        let following_index = match self.next_transmit_index.checked_add(1) {
            Some(index) => index,
            None => return self.fail(PcmShortWireFault::Arithmetic),
        };
        let expected_latch = match self.grid.boundary_cycle(following_index) {
            Ok(cycle) => cycle,
            Err(_) => return self.fail(PcmShortWireFault::Arithmetic),
        };
        if frame.starts_at != expected_start || frame.commits_at != expected_latch {
            return self.fail(PcmShortWireFault::Timing {
                expected_start,
                received_start: frame.starts_at,
                expected_latch,
                received_latch: frame.commits_at,
            });
        }

        let claimed = frame.frame.commits_image();
        if !same_contract(self.contract, claimed) {
            return self.fail(PcmShortWireFault::Contract);
        }
        let mask = self.contract.defined_mask;
        let mut shifted = 0_u32;
        let mut bit_index = 0_u8;
        while bit_index < PCM_SHORT_FRAME_BITS {
            let serial_bit = match frame.frame.serial_data_at(bit_index) {
                Some(bit) => bit,
                None => return self.fail(PcmShortWireFault::Arithmetic),
            };
            shifted = ((shifted << 1) | u32::from(serial_bit)) & mask;
            bit_index += 1;
        }
        let observed_bits = match self.contract.order {
            BitOrder::MostSignificantFirst => shifted,
            BitOrder::LeastSignificantFirst => reverse_low_bits(shifted, self.contract.width),
        };
        if observed_bits != claimed.bits {
            return self.fail(PcmShortWireFault::WireImageMismatch {
                expected_bits: claimed.bits,
                observed_bits,
            });
        }

        let image = CompleteImage {
            bits: observed_bits,
            ..self.contract
        };
        self.visible_image = image;
        self.next_transmit_index = following_index;
        Ok(ObservedPcmShortLatch {
            transmit_index: frame.transmit_index,
            starts_at: frame.starts_at,
            latch_cycle: frame.commits_at,
            image,
        })
    }

    /// Checks whether the next dense frame has missed its latch deadline.
    ///
    /// Returning `Ok(())` only means the deadline is still in the future. At
    /// or beyond the boundary, starvation latches before any further frame can
    /// be accepted.
    pub fn check_starvation_at(&mut self, observed_cycle: u64) -> Result<(), PcmShortWireFault> {
        if self.fault.is_some() {
            return Err(PcmShortWireFault::FaultLatched);
        }
        let latch_cycle = match self.next_frame_deadline() {
            Ok(cycle) => cycle,
            Err(_) => return self.fail(PcmShortWireFault::Arithmetic),
        };
        if observed_cycle < latch_cycle {
            return Ok(());
        }
        self.fail(PcmShortWireFault::Starvation {
            transmit_index: self.next_transmit_index,
            latch_cycle,
        })
    }

    /// Exact following boundary by which the next dense frame must exist.
    pub fn next_frame_deadline(&self) -> Result<u64, PcmShortWireFault> {
        self.grid
            .boundary_cycle(
                self.next_transmit_index
                    .checked_add(1)
                    .ok_or(PcmShortWireFault::Arithmetic)?,
            )
            .map_err(|_| PcmShortWireFault::Arithmetic)
    }

    /// Last complete image proven visible by the model.
    pub const fn visible_image(&self) -> CompleteImage {
        self.visible_image
    }

    /// First retained stream fault, if any.
    pub const fn fault(&self) -> Option<PcmShortWireFault> {
        self.fault
    }

    fn fail<T>(&mut self, fault: PcmShortWireFault) -> Result<T, PcmShortWireFault> {
        self.fault = Some(fault);
        Err(fault)
    }
}

const fn same_contract(left: CompleteImage, right: CompleteImage) -> bool {
    left.width == right.width
        && left.defined_mask == right.defined_mask
        && matches!(
            (left.order, right.order),
            (
                BitOrder::MostSignificantFirst,
                BitOrder::MostSignificantFirst
            ) | (
                BitOrder::LeastSignificantFirst,
                BitOrder::LeastSignificantFirst
            )
        )
}

const fn reverse_low_bits(bits: u32, width: u8) -> u32 {
    let mut reversed = 0_u32;
    let mut index = 0_u8;
    while index < width {
        if bits & (1_u32 << index) != 0 {
            reversed |= 1_u32 << (width - 1 - index);
        }
        index += 1;
    }
    reversed
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use alumina_board::{OwnerDomain, ResourceId};
    use alumina_config::{
        AxisDriverControl, BindingFlags, BindingRole, ResourceBinding, SignalPolarity,
        StepperAxisProfile,
    };
    use alumina_job::{
        AdmittedBlock, JobDescriptor, RealtimeJob, RealtimeJobState, RealtimePoll, WorkSource,
    };
    use alumina_machine_ir::{
        BlockValidationLimits, ExecutionBlock, ExecutionSegment, FINITE_DIFFERENCE_ONE_STEP,
        FiniteDifferenceAxis, FiniteDifferenceSegment, StreamId, StreamTick, ValidationLimits,
    };
    use alumina_motion::{
        AxisTiming, FiniteDifferenceExecutionLimits, ScheduledFiniteDifferencePlan,
        ScheduledShiftPlan, ScheduledShiftedFiniteDifferenceStepper, ScheduledShiftedStepper,
        ShiftImageContract, StepperExecutionProfile, StepperTiming,
    };
    use alumina_protocol::{DeviceCycle, Digest};
    use alumina_shift_register::{
        BitOrder, CompleteImage, PcmShortDmaHorizon, PcmShortFrameGrid, PcmShortTimeline,
        ScheduledCompleteImage, TaggedScheduledCompleteImage,
    };
    use alumina_storage::{ContentId, ObjectKind, PublishedObject, StoredObject};

    use super::*;

    const SAFE: CompleteImage = CompleteImage {
        width: 24,
        defined_mask: 0x00ff_ffff,
        bits: 0x0000_1249,
        order: BitOrder::MostSignificantFirst,
    };

    fn shifted_binding(role: BindingRole, bit: u8) -> ResourceBinding {
        ResourceBinding {
            instance: 0,
            role,
            resource: ResourceId::I2sOut { engine: 0, bit },
            owner: OwnerDomain::Realtime,
            polarity: SignalPolarity::ActiveHigh,
            flags: BindingFlags::default(),
            minimum_active_cycles: 4,
            minimum_inactive_cycles: 4,
            maximum_frequency_hz: if role == BindingRole::AxisStep {
                100_000
            } else {
                0
            },
            watchdog_cycles: 100_000,
        }
    }

    fn shifted_profile() -> StepperExecutionProfile<3> {
        StepperExecutionProfile {
            axes: core::array::from_fn(|axis| {
                let base = u8::try_from(axis * 3).unwrap();
                StepperAxisProfile {
                    instance: u16::try_from(axis).unwrap(),
                    step: ResourceBinding {
                        instance: u16::try_from(axis).unwrap(),
                        ..shifted_binding(BindingRole::AxisStep, base + 1)
                    },
                    direction: ResourceBinding {
                        instance: u16::try_from(axis).unwrap(),
                        ..shifted_binding(BindingRole::AxisDirection, base + 2)
                    },
                    driver_control: ResourceBinding {
                        instance: u16::try_from(axis).unwrap(),
                        ..shifted_binding(BindingRole::AxisDisable, base)
                    },
                    driver_control_action: AxisDriverControl::Disable,
                }
            }),
            timing: StepperTiming {
                axes: [AxisTiming {
                    pulse_high_cycles: 4,
                    pulse_low_cycles: 4,
                    direction_setup_cycles: 4,
                    direction_hold_cycles: 4,
                    enable_setup_cycles: 4,
                    enable_hold_cycles: 8,
                    maximum_step_frequency_hz: 100_000,
                }; 3],
                device_cycle_hz: 1_000_000,
                output_quantum_cycles: 4,
                maximum_lateness_cycles: 0,
            },
        }
    }

    const fn shifted_contract() -> ShiftImageContract {
        ShiftImageContract {
            engine: 0,
            width: SAFE.width,
            defined_mask: SAFE.defined_mask,
            safe_image: SAFE.bits,
        }
    }

    struct OneBlock(Option<ExecutionBlock>);

    impl WorkSource for OneBlock {
        fn try_receive(&mut self) -> Option<ExecutionBlock> {
            self.0.take()
        }

        fn depth(&self) -> usize {
            usize::from(self.0.is_some())
        }
    }

    struct TwoBlocks {
        first: Option<ExecutionBlock>,
        second: Option<ExecutionBlock>,
    }

    impl WorkSource for TwoBlocks {
        fn try_receive(&mut self) -> Option<ExecutionBlock> {
            self.first.take().or_else(|| self.second.take())
        }

        fn depth(&self) -> usize {
            usize::from(self.first.is_some()) + usize::from(self.second.is_some())
        }
    }

    fn admitted_motion_block() -> (RealtimeJob<3>, AdmittedBlock<3>) {
        let stream_id = StreamId::new([0x11; 16]).unwrap();
        let capability_digest = Digest([0x22; 32]);
        let config_digest = Digest([0x33; 32]);
        let block = ExecutionBlock::encode_motion(
            stream_id,
            capability_digest,
            config_digest,
            0,
            Digest::ZERO,
            &[ExecutionSegment {
                start_tick: StreamTick(0),
                end_tick: StreamTick(40),
                delta_steps: [2, 0, 0],
                flags: 0,
            }],
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(Digest([0x44; 32])),
                    byte_len: 512,
                },
                manifest: ContentId::from_sha256(Digest([0x55; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 3,
            execution_kind: alumina_machine_ir::ExecutionKind::Motion,
            maximum_finite_difference_updates: 0,
            block_count: 1,
            first_tick: StreamTick(0),
            initial_position: [0; alumina_machine_ir::MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let mut job = RealtimeJob::prepare(descriptor).unwrap();
        let admitted = match job.poll(&mut OneBlock(Some(block))).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("one validated block must be admitted")
            }
        };
        (job, admitted)
    }

    fn admitted_motion_pair() -> (RealtimeJob<3>, AdmittedBlock<3>, AdmittedBlock<3>) {
        let stream_id = StreamId::new([0x11; 16]).unwrap();
        let capability_digest = Digest([0x22; 32]);
        let config_digest = Digest([0x33; 32]);
        let first = ExecutionBlock::encode_motion(
            stream_id,
            capability_digest,
            config_digest,
            0,
            Digest::ZERO,
            &[ExecutionSegment {
                start_tick: StreamTick(0),
                end_tick: StreamTick(40),
                delta_steps: [2, 0, 0],
                flags: 0,
            }],
        )
        .unwrap();
        let second = ExecutionBlock::encode_motion(
            stream_id,
            capability_digest,
            config_digest,
            1,
            first.header().block_digest,
            &[ExecutionSegment {
                start_tick: StreamTick(40),
                end_tick: StreamTick(80),
                delta_steps: [2, 0, 0],
                flags: 0,
            }],
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 7,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(Digest([0x44; 32])),
                    byte_len: 1_024,
                },
                manifest: ContentId::from_sha256(Digest([0x55; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 3,
            execution_kind: alumina_machine_ir::ExecutionKind::Motion,
            maximum_finite_difference_updates: 0,
            block_count: 2,
            first_tick: StreamTick(0),
            initial_position: [0; alumina_machine_ir::MAX_EXECUTION_AXES],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let mut source = TwoBlocks {
            first: Some(first),
            second: Some(second),
        };
        let mut job = RealtimeJob::prepare(descriptor).unwrap();
        let first = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("first block must admit"),
        };
        let second = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => panic!("second block must admit"),
        };
        (job, first, second)
    }

    fn direct_segment(
        start_tick: u64,
        update_count: u32,
        initial_position: [i64; 3],
        first_difference: [i64; 3],
    ) -> FiniteDifferenceSegment<3> {
        FiniteDifferenceSegment {
            start_tick: StreamTick(start_tick),
            end_tick: StreamTick(start_tick + u64::from(update_count) * 4),
            update_period_ticks: 4,
            update_count,
            axes: core::array::from_fn(|axis| FiniteDifferenceAxis {
                initial_position: initial_position[axis],
                first_difference: first_difference[axis],
                second_difference: 0,
                third_difference: 0,
            }),
            flags: 0,
        }
    }

    fn direct_limits() -> FiniteDifferenceExecutionLimits {
        FiniteDifferenceExecutionLimits {
            maximum_segment_ticks: 1_000,
            maximum_update_count: 100,
            maximum_steps_per_segment: 100,
        }
    }

    fn admitted_direct_pair() -> (RealtimeJob<3>, AdmittedBlock<3>, AdmittedBlock<3>) {
        let stream_id = StreamId::new([0x61; 16]).unwrap();
        let capability_digest = Digest([0x62; 32]);
        let config_digest = Digest([0x63; 32]);
        let first_difference = (FINITE_DIFFERENCE_ONE_STEP - 1) / 4;
        let first_segment = direct_segment(0, 3, [0; 3], [first_difference, 0, 0]);
        let terminal = [
            first_segment
                .position_at(0, first_segment.update_count)
                .unwrap(),
            0,
            0,
        ];
        let second_segment = direct_segment(12, 4, terminal, [first_difference, 0, 0]);
        let first = ExecutionBlock::encode_finite_difference(
            stream_id,
            capability_digest,
            config_digest,
            0,
            Digest::ZERO,
            &[first_segment],
        )
        .unwrap();
        let second = ExecutionBlock::encode_finite_difference(
            stream_id,
            capability_digest,
            config_digest,
            1,
            first.header().block_digest,
            &[second_segment],
        )
        .unwrap();
        let descriptor = JobDescriptor {
            prepare_id: 17,
            partition: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineJobPartition,
                    content: ContentId::from_sha256(Digest([0x64; 32])),
                    byte_len: 1_024,
                },
                manifest: ContentId::from_sha256(Digest([0x65; 32])),
            },
            stream_id,
            capability_digest,
            config_digest,
            axis_count: 3,
            execution_kind: alumina_machine_ir::ExecutionKind::FiniteDifference,
            maximum_finite_difference_updates: 100,
            block_count: 2,
            first_tick: StreamTick(0),
            initial_position: [20, -20, 3, 0, 0, 0, 0, 0],
            limits: BlockValidationLimits {
                maximum_block_ticks: 1_000,
                segment: ValidationLimits {
                    maximum_segment_ticks: 1_000,
                    maximum_steps_per_segment: 100,
                },
            },
        };
        let mut source = TwoBlocks {
            first: Some(first),
            second: Some(second),
        };
        let mut job = RealtimeJob::prepare(descriptor).unwrap();
        let first = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("first direct block must admit")
            }
        };
        let second = match job.poll(&mut source).unwrap() {
            RealtimePoll::Block(admitted) => admitted,
            RealtimePoll::Empty | RealtimePoll::Outstanding => {
                panic!("second direct block must admit")
            }
        };
        (job, first, second)
    }

    #[test]
    fn bit_level_observer_exposes_updates_only_at_following_latches() {
        let grid = PcmShortFrameGrid::new(1_000, 1_000_000, 250_000).unwrap();
        let moving = CompleteImage {
            bits: 0x0000_1269,
            ..SAFE
        };
        let mut timeline = PcmShortTimeline::<1>::new(grid, SAFE).unwrap();
        timeline
            .schedule(ScheduledCompleteImage {
                commit_cycle: 1_012,
                image: moving,
            })
            .unwrap();
        let mut observer = SimPcmShortLatch::new(grid, SAFE).unwrap();

        for (latch_cycle, expected) in [(1_004, SAFE), (1_008, SAFE), (1_012, moving)] {
            let observed = observer
                .consume_frame(timeline.next_frame().unwrap())
                .unwrap();
            assert_eq!(observed.latch_cycle, latch_cycle);
            assert_eq!(observed.image, expected);
            assert_eq!(observer.visible_image(), expected);
        }
        assert_eq!(observer.next_frame_deadline(), Ok(1_016));
        assert_eq!(observer.fault(), None);
    }

    #[test]
    fn exact_motion_releases_only_after_dma_frames_and_physical_latches() {
        let grid = PcmShortFrameGrid::new(96, 1_000_000, 250_000).unwrap();
        assert_eq!(grid.cycles_per_frame(), 4);
        assert_eq!(grid.bit_clock_hz(), 16_000_000);
        let mut timeline = PcmShortTimeline::<8>::new(grid, SAFE).unwrap();
        let mut observer = SimPcmShortLatch::new(grid, SAFE).unwrap();
        let (mut job, admitted) = admitted_motion_block();
        let mut runner =
            ScheduledShiftedStepper::<3, 8>::new(shifted_profile(), shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(100), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(140)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(140),
            }
        );

        let mut outputs = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            timeline
                .schedule(ScheduledCompleteImage {
                    commit_cycle: output.update.at.0,
                    image: CompleteImage {
                        bits: output.update.image,
                        ..SAFE
                    },
                })
                .unwrap();
            runner.stage_output(output).unwrap();
            outputs.push(output);
        }
        assert_eq!(outputs.len(), 5);
        assert_eq!(outputs[0].update.at, DeviceCycle(100));

        let mut next_output = 0;
        for transmit_index in 0..10 {
            let frame = timeline.next_frame().unwrap();
            assert_eq!(frame.transmit_index, transmit_index);
            let observed = observer.consume_frame(frame).unwrap();
            if next_output < outputs.len()
                && outputs[next_output].update.at.0 == observed.latch_cycle
            {
                let output = outputs[next_output];
                assert_eq!(observed.image.bits, output.update.image);
                runner
                    .commit_output(output.token, DeviceCycle(observed.latch_cycle))
                    .unwrap();
                next_output += 1;
            }
        }
        assert_eq!(next_output, outputs.len());
        assert_eq!(observer.next_frame_deadline(), Ok(140));
        assert!(runner.take_completed_block(DeviceCycle(139)).is_none());

        let terminal_dwell = observer
            .consume_frame(timeline.next_frame().unwrap())
            .unwrap();
        assert_eq!(terminal_dwell.latch_cycle, 140);
        let completed = runner.take_completed_block(DeviceCycle(140)).unwrap();
        assert_eq!(completed.completion().at, DeviceCycle(140));
        assert_eq!(completed.completion().position, [2, 0, 0]);
        assert_eq!(completed.completion().maximum_half_tick_error, 4);
        assert_eq!(
            job.acknowledge(completed.into_block()).unwrap().state,
            RealtimeJobState::Complete
        );

        assert_eq!(runner.earliest_finish_cycle(), Ok(DeviceCycle(144)));
        let finish = runner.schedule_finish(DeviceCycle(144)).unwrap();
        timeline
            .schedule(ScheduledCompleteImage {
                commit_cycle: finish.update.at.0,
                image: CompleteImage {
                    bits: finish.update.image,
                    ..SAFE
                },
            })
            .unwrap();
        runner.stage_output(finish).unwrap();
        let observed_finish = observer
            .consume_frame(timeline.next_frame().unwrap())
            .unwrap();
        assert_eq!(observed_finish.latch_cycle, 144);
        assert_eq!(observed_finish.image.bits, finish.update.image);
        runner
            .commit_output(finish.token, DeviceCycle(observed_finish.latch_cycle))
            .unwrap();
        assert!(runner.take_job_complete());
        assert!(!runner.take_job_complete());
    }

    #[test]
    fn direct_recurrence_cross_block_tail_reaches_the_bit_level_pcm_owner() {
        let grid = PcmShortFrameGrid::new(96, 1_000_000, 250_000).unwrap();
        let mut bootstrap = PcmShortTimeline::<0>::new(grid, SAFE).unwrap();
        let mut wire = SimPcmShortLatch::new(grid, SAFE).unwrap();
        let mut dma = PcmShortDmaHorizon::<_, 16, 4>::new(grid, SAFE).unwrap();
        let (mut job, first, second) = admitted_direct_pair();
        let mut runner = ScheduledShiftedFiniteDifferenceStepper::<3, 16>::new(
            shifted_profile(),
            direct_limits(),
            shifted_contract(),
        )
        .unwrap();
        runner.start_job(DeviceCycle(116), [20, -20, 3]).unwrap();
        runner.admit_block(first).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(160)).unwrap(),
            ScheduledFiniteDifferencePlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(128),
            }
        );
        assert_eq!(runner.queued_outputs(), 2);
        assert_eq!(runner.sealed_outputs(), 1);
        runner.admit_block(second).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(160)).unwrap(),
            ScheduledFiniteDifferencePlan::BlockPlanned {
                sequence: 1,
                completion_at: DeviceCycle(144),
            }
        );
        assert_eq!(
            runner.plan_through(DeviceCycle(160)).unwrap(),
            ScheduledFiniteDifferencePlan::OwnerTailComplete {
                completion_at: DeviceCycle(148),
            }
        );
        assert_eq!(runner.planned_finish_cycle(), Ok(DeviceCycle(156)));
        let finish = runner.schedule_planned_finish(DeviceCycle(156)).unwrap();

        let mut outputs = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            dma.stage(TaggedScheduledCompleteImage {
                tag: output.token,
                commit_cycle: output.update.at.0,
                image: CompleteImage {
                    bits: output.update.image,
                    ..SAFE
                },
            })
            .unwrap();
            runner.stage_output(output).unwrap();
            outputs.push(output);
        }
        assert_eq!(outputs.last().copied(), Some(finish));
        assert_eq!(
            outputs
                .iter()
                .map(|output| output.update.at)
                .collect::<Vec<_>>(),
            [
                DeviceCycle(116),
                DeviceCycle(128),
                DeviceCycle(132),
                DeviceCycle(144),
                DeviceCycle(148),
                DeviceCycle(156),
            ]
        );

        let mut physical_ring = VecDeque::new();
        for _ in 0..4 {
            physical_ring.push_back(bootstrap.next_frame().unwrap());
        }
        let mut returned_sequences = Vec::new();
        let mut final_disable_observed = false;
        for _ in 0..24 {
            let transmitted = physical_ring.pop_front().unwrap();
            let observed = wire.consume_frame(transmitted).unwrap();
            dma.observe_latches_through(observed.latch_cycle).unwrap();
            while let Some(commit) = dma.take_commit().unwrap() {
                let output = outputs
                    .iter()
                    .copied()
                    .find(|output| output.token == commit.tag)
                    .unwrap();
                assert_eq!(wire.visible_image().bits, output.update.image);
                runner
                    .commit_output(commit.tag, DeviceCycle(commit.commit_cycle))
                    .unwrap();
                if runner.take_job_complete() {
                    assert_eq!(commit.tag, finish.token);
                    final_disable_observed = true;
                }
            }
            while let Some(completed) =
                runner.take_completed_block(DeviceCycle(observed.latch_cycle))
            {
                let (admitted, completion) = completed.into_parts();
                let sequence = admitted.header().sequence;
                if sequence == 0 {
                    assert_eq!(completion.at, DeviceCycle(128));
                    assert_eq!(completion.position, [21, -20, 3]);
                    assert_eq!(
                        job.acknowledge(admitted).unwrap().state,
                        RealtimeJobState::Admitted
                    );
                } else {
                    assert_eq!(sequence, 1);
                    assert_eq!(completion.at, DeviceCycle(144));
                    assert_eq!(completion.position, [22, -20, 3]);
                    assert_eq!(
                        job.acknowledge(admitted).unwrap().state,
                        RealtimeJobState::Complete
                    );
                }
                returned_sequences.push(sequence);
            }
            if final_disable_observed && returned_sequences == [0, 1] {
                break;
            }

            dma.synchronize_refill_availability(1).unwrap();
            let refill = dma.next_refill_frame().unwrap().unwrap();
            dma.accept_refill(refill).unwrap();
            physical_ring.push_back(refill);
        }

        assert_eq!(returned_sequences, [0, 1]);
        assert!(final_disable_observed);
        assert_eq!(wire.visible_image().bits, finish.update.image);
        assert!(runner.planned_status().enabled.is_empty());
        assert_eq!(runner.committed_updates(), 6);
        assert_eq!(dma.fault(), None);
        assert_eq!(wire.fault(), None);
    }

    #[test]
    fn circular_dma_lead_owns_final_disable_before_releasing_the_block() {
        let grid = PcmShortFrameGrid::new(96, 1_000_000, 250_000).unwrap();
        let mut bootstrap = PcmShortTimeline::<0>::new(grid, SAFE).unwrap();
        let mut wire = SimPcmShortLatch::new(grid, SAFE).unwrap();
        let mut dma = PcmShortDmaHorizon::<_, 8, 4>::new(grid, SAFE).unwrap();
        let (mut job, admitted) = admitted_motion_block();
        let mut runner =
            ScheduledShiftedStepper::<3, 8>::new(shifted_profile(), shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(116), [0; 3]).unwrap();
        runner.admit_block(admitted).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(156)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(156),
            }
        );
        assert_eq!(runner.planned_finish_cycle(), Ok(DeviceCycle(160)));
        let finish = runner.schedule_planned_finish(DeviceCycle(160)).unwrap();

        let mut outputs = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            dma.stage(TaggedScheduledCompleteImage {
                tag: output.token,
                commit_cycle: output.update.at.0,
                image: CompleteImage {
                    bits: output.update.image,
                    ..SAFE
                },
            })
            .unwrap();
            runner.stage_output(output).unwrap();
            outputs.push(output);
        }
        assert_eq!(outputs.len(), 6);
        assert_eq!(outputs.last().copied(), Some(finish));

        // The external safe transaction populated all four physical slots.
        let mut physical_ring = VecDeque::new();
        for _ in 0..4 {
            physical_ring.push_back(bootstrap.next_frame().unwrap());
        }
        assert_eq!(dma.sealed_horizon(), Ok(112));

        let mut final_disable_observed = false;
        let mut returned_block = None;
        for _ in 0..20 {
            let transmitted = physical_ring.pop_front().unwrap();
            let observed = wire.consume_frame(transmitted).unwrap();
            dma.observe_latches_through(observed.latch_cycle).unwrap();
            while let Some(commit) = dma.take_commit().unwrap() {
                let output = outputs
                    .iter()
                    .copied()
                    .find(|output| output.token == commit.tag)
                    .unwrap();
                assert_eq!(commit.commit_cycle, output.update.at.0);
                assert_eq!(wire.visible_image().bits, output.update.image);
                runner
                    .commit_output(commit.tag, DeviceCycle(commit.commit_cycle))
                    .unwrap();
                if runner.take_job_complete() {
                    assert_eq!(commit.tag, finish.token);
                    final_disable_observed = true;
                }
            }
            if returned_block.is_none() {
                returned_block = runner.take_completed_block(DeviceCycle(observed.latch_cycle));
            }
            if final_disable_observed && returned_block.is_some() {
                break;
            }

            // One exact descriptor became CPU-owned; refill that slot for its
            // next trip around the ring before acknowledging the dense frame.
            dma.synchronize_refill_availability(1).unwrap();
            let refill = dma.next_refill_frame().unwrap().unwrap();
            dma.accept_refill(refill).unwrap();
            physical_ring.push_back(refill);
        }

        assert!(final_disable_observed);
        assert_eq!(wire.visible_image().bits, finish.update.image);
        let completed = returned_block.unwrap();
        assert_eq!(completed.completion().at, DeviceCycle(156));
        assert_eq!(completed.completion().position, [2, 0, 0]);
        assert_eq!(
            job.acknowledge(completed.into_block()).unwrap().state,
            RealtimeJobState::Complete
        );
        assert_eq!(dma.fault(), None);
    }

    #[test]
    fn circular_dma_remains_dense_across_independent_block_barriers() {
        let grid = PcmShortFrameGrid::new(96, 1_000_000, 250_000).unwrap();
        let mut bootstrap = PcmShortTimeline::<0>::new(grid, SAFE).unwrap();
        let mut wire = SimPcmShortLatch::new(grid, SAFE).unwrap();
        let mut dma = PcmShortDmaHorizon::<_, 16, 4>::new(grid, SAFE).unwrap();
        let (mut job, first, second) = admitted_motion_pair();
        let mut runner =
            ScheduledShiftedStepper::<3, 16>::new(shifted_profile(), shifted_contract()).unwrap();
        runner.start_job(DeviceCycle(116), [0; 3]).unwrap();
        runner.admit_block(first).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(196)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 0,
                completion_at: DeviceCycle(156),
            }
        );
        let first_prefix = runner.queued_outputs();
        assert_eq!(first_prefix, 5);
        runner.admit_block(second).unwrap();
        assert_eq!(
            runner.plan_through(DeviceCycle(196)).unwrap(),
            ScheduledShiftPlan::BlockPlanned {
                sequence: 1,
                completion_at: DeviceCycle(196),
            }
        );
        let finish_at = runner.planned_finish_cycle().unwrap();
        let finish = runner.schedule_planned_finish(finish_at).unwrap();

        let mut outputs = Vec::new();
        while let Some(output) = runner.next_unstaged_output() {
            dma.stage(TaggedScheduledCompleteImage {
                tag: output.token,
                commit_cycle: output.update.at.0,
                image: CompleteImage {
                    bits: output.update.image,
                    ..SAFE
                },
            })
            .unwrap();
            runner.stage_output(output).unwrap();
            outputs.push(output);
        }
        assert!(outputs.len() > first_prefix);
        assert!(outputs.windows(2).all(|pair| {
            pair[0].update.at < pair[1].update.at && pair[0].token != pair[1].token
        }));

        let mut physical_ring = VecDeque::new();
        for _ in 0..4 {
            physical_ring.push_back(bootstrap.next_frame().unwrap());
        }
        let mut returned_sequences = Vec::new();
        let mut final_disable_observed = false;
        for _ in 0..40 {
            let transmitted = physical_ring.pop_front().unwrap();
            let observed = wire.consume_frame(transmitted).unwrap();
            dma.observe_latches_through(observed.latch_cycle).unwrap();
            while let Some(commit) = dma.take_commit().unwrap() {
                let output = outputs
                    .iter()
                    .copied()
                    .find(|output| output.token == commit.tag)
                    .unwrap();
                assert_eq!(wire.visible_image().bits, output.update.image);
                runner
                    .commit_output(commit.tag, DeviceCycle(commit.commit_cycle))
                    .unwrap();
                if runner.take_job_complete() {
                    assert_eq!(commit.tag, finish.token);
                    final_disable_observed = true;
                }
            }
            while let Some(completed) =
                runner.take_completed_block(DeviceCycle(observed.latch_cycle))
            {
                let admitted = completed.into_block();
                let sequence = admitted.header().sequence;
                if sequence == 0 {
                    assert!(runner.queued_outputs() > 0);
                    assert_eq!(
                        job.acknowledge(admitted).unwrap().state,
                        RealtimeJobState::Admitted
                    );
                } else {
                    assert_eq!(sequence, 1);
                    assert_eq!(
                        job.acknowledge(admitted).unwrap().state,
                        RealtimeJobState::Complete
                    );
                }
                returned_sequences.push(sequence);
            }
            if final_disable_observed && returned_sequences == [0, 1] {
                break;
            }

            dma.synchronize_refill_availability(1).unwrap();
            let refill = dma.next_refill_frame().unwrap().unwrap();
            dma.accept_refill(refill).unwrap();
            physical_ring.push_back(refill);
        }

        assert_eq!(returned_sequences, [0, 1]);
        assert!(final_disable_observed);
        assert_eq!(wire.visible_image().bits, finish.update.image);
        assert_eq!(dma.fault(), None);
        assert_eq!(wire.fault(), None);
    }

    #[test]
    fn wrong_frame_alignment_latches_before_the_image_changes() {
        let grid = PcmShortFrameGrid::new(50, 1_000_000, 250_000).unwrap();
        let mut timeline = PcmShortTimeline::<0>::new(grid, SAFE).unwrap();
        let mut frame = timeline.next_frame().unwrap();
        frame.starts_at += 1;
        let mut observer = SimPcmShortLatch::new(grid, SAFE).unwrap();

        assert_eq!(
            observer.consume_frame(frame),
            Err(PcmShortWireFault::Timing {
                expected_start: 50,
                received_start: 51,
                expected_latch: 54,
                received_latch: 54,
            })
        );
        assert_eq!(observer.visible_image(), SAFE);
        assert!(matches!(
            observer.fault(),
            Some(PcmShortWireFault::Timing { .. })
        ));
    }

    #[test]
    fn missing_horizon_frame_faults_at_the_exact_boundary() {
        let grid = PcmShortFrameGrid::new(100, 1_000_000, 250_000).unwrap();
        let mut timeline = PcmShortTimeline::<0>::new(grid, SAFE).unwrap();
        let first = timeline.next_frame().unwrap();
        let mut observer = SimPcmShortLatch::new(grid, SAFE).unwrap();

        assert_eq!(observer.check_starvation_at(103), Ok(()));
        assert_eq!(
            observer.check_starvation_at(104),
            Err(PcmShortWireFault::Starvation {
                transmit_index: 0,
                latch_cycle: 104,
            })
        );
        assert_eq!(observer.visible_image(), SAFE);
        assert_eq!(
            observer.consume_frame(first),
            Err(PcmShortWireFault::FaultLatched)
        );
    }

    #[test]
    fn least_significant_first_wire_suffix_reconstructs_logical_image() {
        let grid = PcmShortFrameGrid::new(0, 1_000_000, 250_000).unwrap();
        let image = CompleteImage {
            width: 5,
            defined_mask: 0b1_1111,
            bits: 0b1_0011,
            order: BitOrder::LeastSignificantFirst,
        };
        let mut timeline = PcmShortTimeline::<0>::new(grid, image).unwrap();
        let mut observer = SimPcmShortLatch::new(grid, image).unwrap();

        let observed = observer
            .consume_frame(timeline.next_frame().unwrap())
            .unwrap();
        assert_eq!(observed.image, image);
    }

    #[test]
    fn cycle_arithmetic_failure_is_terminal() {
        let grid = PcmShortFrameGrid::new(u64::MAX - 1, 1_000_000, 250_000).unwrap();
        let frame = PlannedPcmShortFrame {
            transmit_index: 0,
            starts_at: u64::MAX - 1,
            commits_at: u64::MAX,
            frame: PcmShortMonoFrame::new(SAFE).unwrap(),
        };
        let mut observer = SimPcmShortLatch::new(grid, SAFE).unwrap();

        assert_eq!(
            observer.consume_frame(frame),
            Err(PcmShortWireFault::Arithmetic)
        );
        assert_eq!(observer.fault(), Some(PcmShortWireFault::Arithmetic));
    }
}
