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
    use alumina_shift_register::{
        BitOrder, CompleteImage, PcmShortFrameGrid, PcmShortTimeline, ScheduledCompleteImage,
    };

    use super::*;

    const SAFE: CompleteImage = CompleteImage {
        width: 24,
        defined_mask: 0x00ff_ffff,
        bits: 0x0000_1249,
        order: BitOrder::MostSignificantFirst,
    };

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
