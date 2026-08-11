#![no_std]
#![doc = "Complete-image static and scheduled shift-register transports."]
//!
//! This clean-room driver only models the three observable 74HC595-style
//! signals: serial data, shift clock, and storage-register latch. It performs
//! no peripheral register access and makes no assumptions about FluidNC or any
//! other firmware implementation. Board composition supplies the exact chain
//! width, physical cascade order, and complete safe image.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;

/// Fixed slots in the ESP32 standard/PCM-short I²S frame model.
pub const PCM_SHORT_SLOTS_PER_FRAME: u8 = 2;
/// Fixed slot width used by the initial ESP32 shift-stream contract.
pub const PCM_SHORT_SLOT_BITS: u8 = 32;
/// Shift-clock edges between consecutive rising frame-sync/latch edges.
pub const PCM_SHORT_FRAME_BITS: u8 = PCM_SHORT_SLOTS_PER_FRAME * PCM_SHORT_SLOT_BITS;

/// Order in which logical image bits enter the physical cascade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitOrder {
    /// Highest logical bit enters first; bit zero enters last.
    MostSignificantFirst,
    /// Bit zero enters first; highest logical bit enters last.
    LeastSignificantFirst,
}

/// One complete image for a chain containing at most 32 outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteImage {
    /// Exact number of shifted outputs.
    pub width: u8,
    /// Must contain every bit below `width` and no other bit.
    pub defined_mask: u32,
    /// Values to latch; bits outside `defined_mask` are forbidden.
    pub bits: u32,
    /// Physical cascade serialization order.
    pub order: BitOrder,
}

impl CompleteImage {
    /// Checks that every physical output has an explicit value.
    pub const fn validate(self) -> Result<(), ImageError> {
        let expected_mask = match self.width {
            0 => return Err(ImageError::Width { received: 0 }),
            1..=31 => (1_u32 << self.width) - 1,
            32 => u32::MAX,
            received => return Err(ImageError::Width { received }),
        };
        if self.defined_mask != expected_mask {
            return Err(ImageError::Incomplete {
                received_mask: self.defined_mask,
                expected_mask,
            });
        }
        if self.bits & !self.defined_mask != 0 {
            return Err(ImageError::BitsOutsideMask {
                bits: self.bits,
                defined_mask: self.defined_mask,
            });
        }
        Ok(())
    }
}

/// Complete image encoded as one mono 32-bit PCM-short sample.
///
/// The target contract duplicates this sample into both 32-bit I²S slots.
/// Consequently, the final `image.width` serial bits before the next rising
/// frame-sync edge always contain the complete requested image. The rising
/// frame-sync edge at the start of a frame latches the sample shifted during
/// the previous frame, so this value has an explicit one-frame pipeline delay.
/// An ESP32 adapter must prove by capture that its selected MSB-shift, slot,
/// clock-edge, and WS configuration realizes this modeled bit window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmShortMonoFrame {
    image: CompleteImage,
    sample_word: u32,
}

impl PcmShortMonoFrame {
    /// Encodes one already composed complete image without dropping any bit.
    pub const fn new(image: CompleteImage) -> Result<Self, PcmShortFrameError> {
        match image.validate() {
            Ok(()) => {}
            Err(error) => return Err(PcmShortFrameError::Image(error)),
        }
        let sample_word = match image.order {
            BitOrder::MostSignificantFirst => image.bits,
            BitOrder::LeastSignificantFirst => reverse_low_bits(image.bits, image.width),
        };
        Ok(Self { image, sample_word })
    }

    /// One numeric sample consumed by a mono ESP32 DMA frame.
    pub const fn sample_word(self) -> u32 {
        self.sample_word
    }

    /// Explicit little-endian memory representation for the ESP32 DMA buffer.
    pub const fn dma_bytes_le(self) -> [u8; 4] {
        self.sample_word.to_le_bytes()
    }

    /// Complete logical image made visible by the following latch edge.
    pub const fn commits_image(self) -> CompleteImage {
        self.image
    }

    /// Serial data level at one of the 64 required modeled BCLK positions.
    ///
    /// Both slots transmit the same sample most-significant-bit first. `None`
    /// denotes an index outside the complete frame.
    pub const fn serial_data_at(self, bit_index: u8) -> Option<bool> {
        if bit_index >= PCM_SHORT_FRAME_BITS {
            return None;
        }
        let within_slot = bit_index % PCM_SHORT_SLOT_BITS;
        let sample_bit = PCM_SHORT_SLOT_BITS - 1 - within_slot;
        Some(self.sample_word & (1_u32 << sample_bit) != 0)
    }

    /// Whether every final chain-width bit before the next latch matches the
    /// requested physical serialization order.
    pub const fn suffix_is_complete(self) -> bool {
        let mut serial_index = 0;
        while serial_index < self.image.width {
            let frame_index = PCM_SHORT_FRAME_BITS - self.image.width + serial_index;
            let expected_logical = match self.image.order {
                BitOrder::MostSignificantFirst => self.image.width - 1 - serial_index,
                BitOrder::LeastSignificantFirst => serial_index,
            };
            let expected = self.image.bits & (1_u32 << expected_logical) != 0;
            match (self.serial_data_at(frame_index), expected) {
                (Some(true), true) | (Some(false), false) => {}
                _ => return false,
            }
            serial_index += 1;
        }
        true
    }
}

/// A complete image could not be represented by the fixed PCM-short model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmShortFrameError {
    /// The image was incomplete or outside the fixed 32-bit chain width.
    Image(ImageError),
}

/// Exact relationship between device cycles and continuous I²S frame edges.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmShortFrameGrid {
    epoch_cycle: u64,
    device_cycle_hz: u64,
    frame_rate_hz: u32,
    cycles_per_frame: u64,
    bit_clock_hz: u64,
}

impl PcmShortFrameGrid {
    /// Builds an integer frame grid. Fractional device-cycle periods are
    /// rejected rather than rounded.
    pub const fn new(
        epoch_cycle: u64,
        device_cycle_hz: u64,
        frame_rate_hz: u32,
    ) -> Result<Self, PcmShortGridError> {
        if device_cycle_hz == 0 || frame_rate_hz == 0 {
            return Err(PcmShortGridError::Rate);
        }
        let frame_rate = frame_rate_hz as u64;
        if !device_cycle_hz.is_multiple_of(frame_rate) {
            return Err(PcmShortGridError::FractionalPeriod {
                device_cycle_hz,
                frame_rate_hz,
            });
        }
        let cycles_per_frame = device_cycle_hz / frame_rate;
        if cycles_per_frame == 0 {
            return Err(PcmShortGridError::Rate);
        }
        let bit_clock_hz = match frame_rate.checked_mul(PCM_SHORT_FRAME_BITS as u64) {
            Some(rate) => rate,
            None => return Err(PcmShortGridError::Arithmetic),
        };
        Ok(Self {
            epoch_cycle,
            device_cycle_hz,
            frame_rate_hz,
            cycles_per_frame,
            bit_clock_hz,
        })
    }

    /// Device cycle at which frame zero begins. The software model treats the
    /// separately established bootstrap image as already visible here.
    pub const fn epoch_cycle(self) -> u64 {
        self.epoch_cycle
    }

    /// Exact device counter frequency used to construct the grid.
    pub const fn device_cycle_hz(self) -> u64 {
        self.device_cycle_hz
    }

    /// Exact frame/latch rate.
    pub const fn frame_rate_hz(self) -> u32 {
        self.frame_rate_hz
    }

    /// Integer device cycles between rising latch edges.
    pub const fn cycles_per_frame(self) -> u64 {
        self.cycles_per_frame
    }

    /// Required continuous bit-clock rate for two 32-bit slots.
    pub const fn bit_clock_hz(self) -> u64 {
        self.bit_clock_hz
    }

    /// Exact rising frame-sync/latch cycle for an absolute frame boundary.
    pub const fn boundary_cycle(self, boundary_index: u64) -> Result<u64, PcmShortGridError> {
        let offset = match boundary_index.checked_mul(self.cycles_per_frame) {
            Some(offset) => offset,
            None => return Err(PcmShortGridError::Arithmetic),
        };
        match self.epoch_cycle.checked_add(offset) {
            Some(cycle) => Ok(cycle),
            None => Err(PcmShortGridError::Arithmetic),
        }
    }

    /// Absolute frame-boundary index for one exactly aligned device cycle.
    pub const fn boundary_index_for_cycle(self, cycle: u64) -> Result<u64, PcmShortGridError> {
        if cycle < self.epoch_cycle {
            return Err(PcmShortGridError::BeforeEpoch {
                epoch_cycle: self.epoch_cycle,
                received_cycle: cycle,
            });
        }
        let delta = cycle - self.epoch_cycle;
        if !delta.is_multiple_of(self.cycles_per_frame) {
            return Err(PcmShortGridError::Unaligned {
                received_cycle: cycle,
                cycles_per_frame: self.cycles_per_frame,
            });
        }
        Ok(delta / self.cycles_per_frame)
    }

    /// Frame that must transmit an image for it to become visible at the
    /// requested exact latch cycle.
    pub const fn transmit_index_for_commit(
        self,
        commit_cycle: u64,
    ) -> Result<u64, PcmShortGridError> {
        let boundary_index = match self.boundary_index_for_cycle(commit_cycle) {
            Ok(index) => index,
            Err(error) => return Err(error),
        };
        match boundary_index.checked_sub(1) {
            Some(frame_index) => Ok(frame_index),
            None => Err(PcmShortGridError::PipelineLead {
                commit_cycle,
                first_commit_cycle: match self.boundary_cycle(1) {
                    Ok(cycle) => cycle,
                    Err(_) => return Err(PcmShortGridError::Arithmetic),
                },
            }),
        }
    }
}

/// Exact frame-grid construction or scheduling failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmShortGridError {
    /// A device or frame rate was zero, or the frame rate exceeded the cycle
    /// domain.
    Rate,
    /// One frame would occupy a fractional number of device cycles.
    FractionalPeriod {
        device_cycle_hz: u64,
        frame_rate_hz: u32,
    },
    /// A requested commit preceded the stream epoch.
    BeforeEpoch {
        epoch_cycle: u64,
        received_cycle: u64,
    },
    /// A requested commit was not on the exact frame grid.
    Unaligned {
        received_cycle: u64,
        cycles_per_frame: u64,
    },
    /// The epoch boundary has no preceding frame in which to shift an update.
    PipelineLead {
        commit_cycle: u64,
        first_commit_cycle: u64,
    },
    /// Checked frame/cycle arithmetic overflowed.
    Arithmetic,
}

/// Sparse complete-image update consumed by the dense frame planner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ScheduledCompleteImage {
    /// Exact future rising latch edge.
    pub commit_cycle: u64,
    /// Full image that must become visible at that edge.
    pub image: CompleteImage,
}

/// One dense mono sample and its exact one-frame pipeline facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlannedPcmShortFrame {
    /// Absolute dense frame number from the stream epoch.
    pub transmit_index: u64,
    /// Exact device cycle at which this modeled frame begins.
    pub starts_at: u64,
    /// Exact following boundary at which the image is modeled as visible.
    pub commits_at: u64,
    /// Complete image and required serial bit pattern for this frame.
    pub frame: PcmShortMonoFrame,
}

/// Fixed-capacity sparse-to-dense complete-image planner.
///
/// It contains no peripheral state. A target producer can fill a continuous
/// DMA horizon from [`Self::next_frame`] while a separate completion observer
/// proves which rising latch edges have physically occurred.
pub struct PcmShortTimeline<const UPDATES: usize> {
    grid: PcmShortFrameGrid,
    contract: CompleteImage,
    current_image: CompleteImage,
    updates: [Option<ScheduledCompleteImage>; UPDATES],
    head: usize,
    len: usize,
    next_transmit_index: u64,
}

impl<const UPDATES: usize> PcmShortTimeline<UPDATES> {
    /// Starts at frame zero with the exact image already established by the
    /// static bootstrap transaction.
    pub const fn new(
        grid: PcmShortFrameGrid,
        initial_image: CompleteImage,
    ) -> Result<Self, PcmShortTimelineError> {
        match PcmShortMonoFrame::new(initial_image) {
            Ok(_) => {}
            Err(error) => return Err(PcmShortTimelineError::Frame(error)),
        }
        Ok(Self {
            grid,
            contract: initial_image,
            current_image: initial_image,
            updates: [None; UPDATES],
            head: 0,
            len: 0,
            next_transmit_index: 0,
        })
    }

    /// Queues one strictly ordered, frame-aligned future complete image.
    pub fn schedule(
        &mut self,
        update: ScheduledCompleteImage,
    ) -> Result<(), PcmShortTimelineError> {
        PcmShortMonoFrame::new(update.image).map_err(PcmShortTimelineError::Frame)?;
        if !same_image_contract(self.contract, update.image) {
            return Err(PcmShortTimelineError::Contract);
        }
        let transmit_index = self
            .grid
            .transmit_index_for_commit(update.commit_cycle)
            .map_err(PcmShortTimelineError::Grid)?;
        if transmit_index < self.next_transmit_index {
            return Err(PcmShortTimelineError::Order);
        }
        if self.len != 0 {
            let last = (self.head + self.len - 1) % UPDATES;
            if self.updates[last].is_none_or(|prior| prior.commit_cycle >= update.commit_cycle) {
                return Err(PcmShortTimelineError::Order);
            }
        }
        if self.len == UPDATES {
            return Err(PcmShortTimelineError::Capacity);
        }
        if UPDATES == 0 {
            return Err(PcmShortTimelineError::Capacity);
        }
        let tail = (self.head + self.len) % UPDATES;
        self.updates[tail] = Some(update);
        self.len += 1;
        Ok(())
    }

    /// Emits the next dense sample. Unchanged image intervals repeat exactly;
    /// an update is consumed in the frame immediately before its latch edge.
    pub fn next_frame(&mut self) -> Result<PlannedPcmShortFrame, PcmShortTimelineError> {
        let transmit_index = self.next_transmit_index;
        let starts_at = self
            .grid
            .boundary_cycle(transmit_index)
            .map_err(PcmShortTimelineError::Grid)?;
        let commits_at = self
            .grid
            .boundary_cycle(
                transmit_index
                    .checked_add(1)
                    .ok_or(PcmShortTimelineError::Arithmetic)?,
            )
            .map_err(PcmShortTimelineError::Grid)?;
        if self.len != 0 {
            let update = self.updates[self.head].ok_or(PcmShortTimelineError::State)?;
            if update.commit_cycle < commits_at {
                return Err(PcmShortTimelineError::Order);
            }
            if update.commit_cycle == commits_at {
                self.current_image = update.image;
                self.updates[self.head] = None;
                self.head = if UPDATES == 0 {
                    0
                } else {
                    (self.head + 1) % UPDATES
                };
                self.len -= 1;
            }
        }
        let frame =
            PcmShortMonoFrame::new(self.current_image).map_err(PcmShortTimelineError::Frame)?;
        self.next_transmit_index = transmit_index
            .checked_add(1)
            .ok_or(PcmShortTimelineError::Arithmetic)?;
        Ok(PlannedPcmShortFrame {
            transmit_index,
            starts_at,
            commits_at,
            frame,
        })
    }

    /// Next frame boundary that can still receive a newly scheduled image.
    pub fn next_commit_cycle(&self) -> Result<u64, PcmShortTimelineError> {
        self.grid
            .boundary_cycle(
                self.next_transmit_index
                    .checked_add(1)
                    .ok_or(PcmShortTimelineError::Arithmetic)?,
            )
            .map_err(PcmShortTimelineError::Grid)
    }

    /// Sparse updates waiting to enter the dense DMA horizon.
    pub const fn queued_updates(&self) -> usize {
        self.len
    }
}

/// Sparse-to-dense frame planning failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmShortTimelineError {
    /// A complete image could not be encoded into the frame contract.
    Frame(PcmShortFrameError),
    /// A rate, alignment, lead, or boundary calculation was invalid.
    Grid(PcmShortGridError),
    /// A queued image changed chain width, coverage, or physical bit order.
    Contract,
    /// Updates were duplicated, reordered, or already behind the producer.
    Order,
    /// The fixed sparse-update queue was full.
    Capacity,
    /// Internal queue shape was inconsistent.
    State,
    /// Frame indexing overflowed.
    Arithmetic,
}

/// One target-owned sparse update correlated with an opaque caller tag.
///
/// The tag is never serialized by this driver. Firmware can retain its
/// process-local output token without making the transport depend on a motion
/// crate or exposing a token constructor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaggedScheduledCompleteImage<TAG> {
    /// Opaque identity returned only after the corresponding latch boundary is
    /// independently observed.
    pub tag: TAG,
    /// Exact future rising latch edge.
    pub commit_cycle: u64,
    /// Complete image that must become visible at that edge.
    pub image: CompleteImage,
}

/// One staged tag whose exact boundary has been independently observed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PcmShortCommitObservation<TAG> {
    pub tag: TAG,
    pub commit_cycle: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TrackedPcmShortUpdate<TAG> {
    tag: TAG,
    commit_cycle: u64,
    image: CompleteImage,
    materialized: bool,
}

/// Allocation-free ownership model for a safe-prefilled circular PCM-short
/// DMA ring.
///
/// `FRAMES` safe frames are already hardware-owned when this model is created.
/// A target reports how many whole frame slots its DMA API has released for
/// refill. Dense frames then cross an explicit preview/accept boundary: logical
/// planning may preview a frame, but the continuous sealed horizon advances only
/// after the target confirms that exact four-byte sample was accepted. Sparse
/// tags become observable only after both materialization and a separate
/// monotonic latch-boundary observation.
///
/// This type deliberately does not interpret a DMA descriptor completion as a
/// physical latch edge. A chip adapter must qualify and supply that observation
/// independently.
pub struct PcmShortDmaHorizon<TAG, const UPDATES: usize, const FRAMES: usize>
where
    TAG: Copy + Eq,
{
    grid: PcmShortFrameGrid,
    timeline: PcmShortTimeline<UPDATES>,
    sealed_frames: u64,
    released_frames: u64,
    refill_credit_frames: usize,
    pending_frame: Option<PlannedPcmShortFrame>,
    updates: [Option<TrackedPcmShortUpdate<TAG>>; UPDATES],
    update_head: usize,
    update_len: usize,
    observed_through: u64,
    fault: Option<PcmShortDmaHorizonError>,
}

impl<TAG, const UPDATES: usize, const FRAMES: usize> PcmShortDmaHorizon<TAG, UPDATES, FRAMES>
where
    TAG: Copy + Eq,
{
    /// Starts after an external transaction has filled the complete physical
    /// ring with the supplied safe image.
    pub fn new(
        grid: PcmShortFrameGrid,
        safe_image: CompleteImage,
    ) -> Result<Self, PcmShortDmaHorizonError> {
        if FRAMES == 0 {
            return Err(PcmShortDmaHorizonError::RingCapacity);
        }
        let prefilled = u64::try_from(FRAMES).map_err(|_| PcmShortDmaHorizonError::Arithmetic)?;
        grid.boundary_cycle(prefilled)
            .map_err(PcmShortDmaHorizonError::Grid)?;
        let mut timeline =
            PcmShortTimeline::new(grid, safe_image).map_err(PcmShortDmaHorizonError::Timeline)?;
        // These frames already exist in the externally established DMA buffer.
        // No sparse update can target them because `schedule` rejects a
        // transmit index below this exact boundary.
        timeline.next_transmit_index = prefilled;
        Ok(Self {
            grid,
            timeline,
            sealed_frames: prefilled,
            released_frames: 0,
            refill_credit_frames: 0,
            pending_frame: None,
            updates: [None; UPDATES],
            update_head: 0,
            update_len: 0,
            observed_through: grid.epoch_cycle(),
            fault: None,
        })
    }

    /// Queues one immutable sparse update strictly beyond the sealed horizon.
    pub fn stage(
        &mut self,
        update: TaggedScheduledCompleteImage<TAG>,
    ) -> Result<(), PcmShortDmaHorizonError> {
        self.ensure_live()?;
        if UPDATES == 0 || self.update_len == UPDATES {
            return self.fail(PcmShortDmaHorizonError::UpdateCapacity);
        }
        let mut offset = 0;
        while offset < self.update_len {
            let index = (self.update_head + offset) % UPDATES;
            if self.updates[index].is_some_and(|tracked| tracked.tag == update.tag) {
                return self.fail(PcmShortDmaHorizonError::DuplicateTag);
            }
            offset += 1;
        }
        self.timeline
            .schedule(ScheduledCompleteImage {
                commit_cycle: update.commit_cycle,
                image: update.image,
            })
            .map_err(|error| self.latch(PcmShortDmaHorizonError::Timeline(error)))?;
        let tail = (self.update_head + self.update_len) % UPDATES;
        self.updates[tail] = Some(TrackedPcmShortUpdate {
            tag: update.tag,
            commit_cycle: update.commit_cycle,
            image: update.image,
            materialized: false,
        });
        self.update_len = self
            .update_len
            .checked_add(1)
            .ok_or_else(|| self.latch(PcmShortDmaHorizonError::Arithmetic))?;
        Ok(())
    }

    /// Reconciles the target DMA API's current whole-frame refill capacity.
    ///
    /// Capacity cannot shrink except through [`Self::accept_refill`]. A shrink
    /// therefore proves an untracked write or inconsistent descriptor report
    /// and latches the model.
    pub fn synchronize_refill_availability(
        &mut self,
        available_frames: usize,
    ) -> Result<(), PcmShortDmaHorizonError> {
        self.ensure_live()?;
        if available_frames > FRAMES || available_frames < self.refill_credit_frames {
            return self.fail(PcmShortDmaHorizonError::Availability {
                reported: available_frames,
                retained: self.refill_credit_frames,
                ring_frames: FRAMES,
            });
        }
        let newly_released = available_frames - self.refill_credit_frames;
        let newly_released = u64::try_from(newly_released)
            .map_err(|_| self.latch(PcmShortDmaHorizonError::Arithmetic))?;
        let released = self
            .released_frames
            .checked_add(newly_released)
            .ok_or_else(|| self.latch(PcmShortDmaHorizonError::Arithmetic))?;
        if released > self.sealed_frames {
            return self.fail(PcmShortDmaHorizonError::Availability {
                reported: available_frames,
                retained: self.refill_credit_frames,
                ring_frames: FRAMES,
            });
        }
        self.released_frames = released;
        self.refill_credit_frames = available_frames;
        Ok(())
    }

    /// Returns the next exact dense frame without advancing the sealed
    /// hardware horizon. Repeated calls return the same pending frame until it
    /// is accepted or the model is faulted.
    pub fn next_refill_frame(
        &mut self,
    ) -> Result<Option<PlannedPcmShortFrame>, PcmShortDmaHorizonError> {
        self.ensure_live()?;
        if let Some(frame) = self.pending_frame {
            return Ok(Some(frame));
        }
        if self.refill_credit_frames == 0 {
            return Ok(None);
        }
        let frame = self
            .timeline
            .next_frame()
            .map_err(|error| self.latch(PcmShortDmaHorizonError::Timeline(error)))?;
        if frame.transmit_index != self.sealed_frames {
            return self.fail(PcmShortDmaHorizonError::FrameOrder {
                expected: self.sealed_frames,
                received: frame.transmit_index,
            });
        }
        self.pending_frame = Some(frame);
        Ok(Some(frame))
    }

    /// Confirms that the target accepted the exact pending four-byte sample.
    pub fn accept_refill(
        &mut self,
        frame: PlannedPcmShortFrame,
    ) -> Result<u64, PcmShortDmaHorizonError> {
        self.ensure_live()?;
        let Some(expected) = self.pending_frame else {
            return self.fail(PcmShortDmaHorizonError::NoPendingFrame);
        };
        if frame != expected {
            return self.fail(PcmShortDmaHorizonError::PendingFrameMismatch);
        }
        if self.refill_credit_frames == 0 || frame.transmit_index != self.sealed_frames {
            return self.fail(PcmShortDmaHorizonError::FrameOrder {
                expected: self.sealed_frames,
                received: frame.transmit_index,
            });
        }

        let mut offset = 0;
        while offset < self.update_len {
            let index = (self.update_head + offset) % UPDATES;
            let tracked = self.updates[index]
                .ok_or_else(|| self.latch(PcmShortDmaHorizonError::InternalState))?;
            if !tracked.materialized {
                if tracked.commit_cycle < frame.commits_at {
                    return self.fail(PcmShortDmaHorizonError::MissedUpdate {
                        commit_cycle: tracked.commit_cycle,
                        frame_commit_cycle: frame.commits_at,
                    });
                }
                if tracked.commit_cycle == frame.commits_at {
                    if tracked.image != frame.frame.commits_image() {
                        return self.fail(PcmShortDmaHorizonError::MaterializedImageMismatch);
                    }
                    self.updates[index] = Some(TrackedPcmShortUpdate {
                        materialized: true,
                        ..tracked
                    });
                }
                break;
            }
            offset += 1;
        }

        self.sealed_frames = self
            .sealed_frames
            .checked_add(1)
            .ok_or_else(|| self.latch(PcmShortDmaHorizonError::Arithmetic))?;
        self.refill_credit_frames -= 1;
        self.pending_frame = None;
        self.sealed_horizon()
    }

    /// Latest exact latch boundary already represented by hardware-owned
    /// dense frames.
    pub fn sealed_horizon(&self) -> Result<u64, PcmShortDmaHorizonError> {
        self.ensure_live()?;
        self.grid
            .boundary_cycle(self.sealed_frames)
            .map_err(PcmShortDmaHorizonError::Grid)
    }

    /// Latest exact latch boundary that can be sealed using currently released
    /// whole-frame slots.
    pub fn writable_horizon(&self) -> Result<u64, PcmShortDmaHorizonError> {
        self.ensure_live()?;
        let credits = u64::try_from(self.refill_credit_frames)
            .map_err(|_| PcmShortDmaHorizonError::Arithmetic)?;
        let boundary = self
            .sealed_frames
            .checked_add(credits)
            .ok_or(PcmShortDmaHorizonError::Arithmetic)?;
        self.grid
            .boundary_cycle(boundary)
            .map_err(PcmShortDmaHorizonError::Grid)
    }

    /// Installs an independent, monotonic physical latch observation. An
    /// observation beyond the sealed dense horizon is an exact underrun.
    pub fn observe_latches_through(&mut self, through: u64) -> Result<(), PcmShortDmaHorizonError> {
        self.ensure_live()?;
        self.grid
            .boundary_index_for_cycle(through)
            .map_err(|error| self.latch(PcmShortDmaHorizonError::Grid(error)))?;
        if through < self.observed_through {
            return self.fail(PcmShortDmaHorizonError::ObservationOrder {
                prior: self.observed_through,
                received: through,
            });
        }
        let sealed = self.sealed_horizon()?;
        if through > sealed {
            return self.fail(PcmShortDmaHorizonError::Underrun {
                observed: through,
                sealed,
            });
        }
        self.observed_through = through;
        Ok(())
    }

    /// Returns one materialized tag only after its exact latch boundary was
    /// independently observed. Multiple observations drain in stage order.
    pub fn take_commit(
        &mut self,
    ) -> Result<Option<PcmShortCommitObservation<TAG>>, PcmShortDmaHorizonError> {
        self.ensure_live()?;
        if self.update_len == 0 {
            return Ok(None);
        }
        let tracked = self.updates[self.update_head]
            .ok_or_else(|| self.latch(PcmShortDmaHorizonError::InternalState))?;
        if !tracked.materialized || tracked.commit_cycle > self.observed_through {
            return Ok(None);
        }
        self.updates[self.update_head] = None;
        self.update_head = if UPDATES == 0 {
            0
        } else {
            (self.update_head + 1) % UPDATES
        };
        self.update_len -= 1;
        Ok(Some(PcmShortCommitObservation {
            tag: tracked.tag,
            commit_cycle: tracked.commit_cycle,
        }))
    }

    /// Exact number of target-reported frame slots awaiting refill.
    pub const fn refill_credit_frames(&self) -> usize {
        self.refill_credit_frames
    }

    /// Exact number of sparse updates awaiting physical observation.
    pub const fn pending_updates(&self) -> usize {
        self.update_len
    }

    /// First retained fault, if any.
    pub const fn fault(&self) -> Option<PcmShortDmaHorizonError> {
        self.fault
    }

    /// Latches an external peripheral/safety failure and invalidates every tag.
    pub fn invalidate(&mut self) {
        if self.fault.is_none() {
            let _ = self.latch(PcmShortDmaHorizonError::ExternalFault);
        }
    }

    fn ensure_live(&self) -> Result<(), PcmShortDmaHorizonError> {
        if self.fault.is_some() {
            Err(PcmShortDmaHorizonError::FaultLatched)
        } else {
            Ok(())
        }
    }

    fn fail<T>(&mut self, error: PcmShortDmaHorizonError) -> Result<T, PcmShortDmaHorizonError> {
        Err(self.latch(error))
    }

    fn latch(&mut self, error: PcmShortDmaHorizonError) -> PcmShortDmaHorizonError {
        if self.fault.is_none() {
            self.fault = Some(error);
            self.pending_frame = None;
            self.updates = [None; UPDATES];
            self.update_head = 0;
            self.update_len = 0;
            self.refill_credit_frames = 0;
        }
        error
    }
}

/// Fail-closed circular-DMA ownership or observation error.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PcmShortDmaHorizonError {
    Timeline(PcmShortTimelineError),
    Grid(PcmShortGridError),
    RingCapacity,
    UpdateCapacity,
    DuplicateTag,
    Availability {
        reported: usize,
        retained: usize,
        ring_frames: usize,
    },
    FrameOrder {
        expected: u64,
        received: u64,
    },
    NoPendingFrame,
    PendingFrameMismatch,
    MissedUpdate {
        commit_cycle: u64,
        frame_commit_cycle: u64,
    },
    MaterializedImageMismatch,
    ObservationOrder {
        prior: u64,
        received: u64,
    },
    Underrun {
        observed: u64,
        sealed: u64,
    },
    InternalState,
    Arithmetic,
    ExternalFault,
    FaultLatched,
}

const fn same_image_contract(left: CompleteImage, right: CompleteImage) -> bool {
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
    let mut index = 0;
    while index < width {
        if bits & (1_u32 << index) != 0 {
            reversed |= 1_u32 << (width - 1 - index);
        }
        index += 1;
    }
    reversed
}

/// Minimum control-line timing requested from an embedded-hal delay provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    /// Stable-data interval before each rising shift-clock edge.
    pub data_setup_ns: u32,
    /// Minimum high duration of every shift-clock pulse.
    pub clock_high_ns: u32,
    /// Minimum low duration between shift-clock pulses.
    pub clock_low_ns: u32,
    /// Minimum low latch setup before shifting begins.
    pub latch_setup_ns: u32,
    /// Minimum high duration of the commit latch pulse.
    pub latch_high_ns: u32,
}

impl Timing {
    /// Conservative bootstrap timing used by the first 74HC595 board target.
    pub const CONSERVATIVE_100NS: Self = Self {
        data_setup_ns: 100,
        clock_high_ns: 100,
        clock_low_ns: 100,
        latch_setup_ns: 100,
        latch_high_ns: 100,
    };

    /// Rejects zero-width pulses and accidental service-scale blocking waits.
    pub const fn validate(self) -> Result<(), TimingError> {
        let values = [
            self.data_setup_ns,
            self.clock_high_ns,
            self.clock_low_ns,
            self.latch_setup_ns,
            self.latch_high_ns,
        ];
        let mut index = 0;
        while index < values.len() {
            if values[index] == 0 || values[index] > 1_000_000 {
                return Err(TimingError {
                    field_index: index as u8,
                    received_ns: values[index],
                });
            }
            index += 1;
        }
        Ok(())
    }
}

/// Invalid bounded shift/latch timing field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingError {
    /// Field order matches [`Timing`] declaration order, starting at zero.
    pub field_index: u8,
    /// Zero or more than the one-millisecond bootstrap ceiling.
    pub received_ns: u32,
}

/// Static complete-image validation failure detected before pin activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageError {
    /// Chain width was zero or exceeded the fixed 32-bit representation.
    Width { received: u8 },
    /// At least one physical output was omitted or an out-of-width bit was named.
    Incomplete {
        received_mask: u32,
        expected_mask: u32,
    },
    /// Image attempted to set a bit that the contract did not define.
    BitsOutsideMask { bits: u32, defined_mask: u32 },
}

/// Complete-image transaction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error<E> {
    /// Image was rejected before any control-line activity.
    Image(ImageError),
    /// Timing was rejected before any control-line activity.
    Timing(TimingError),
    /// A digital output operation failed.
    Pin(E),
}

/// Owned three-wire static shift-register transport.
///
/// Construction first forces latch low, then clock low, then data low. A
/// successful write leaves every control line low and retains the three pins so
/// no other task can reconfigure them while the latched image is authoritative.
pub struct StaticShiftRegister<CLOCK, DATA, LATCH, DELAY> {
    clock: CLOCK,
    data: DATA,
    latch: LATCH,
    delay: DELAY,
    latched_image: Option<CompleteImage>,
}

impl<CLOCK, DATA, LATCH, DELAY, E> StaticShiftRegister<CLOCK, DATA, LATCH, DELAY>
where
    CLOCK: OutputPin<Error = E>,
    DATA: OutputPin<Error = E>,
    LATCH: OutputPin<Error = E>,
    DELAY: DelayNs,
{
    /// Takes exclusive ownership and establishes inactive control-line levels.
    pub fn new(
        mut clock: CLOCK,
        mut data: DATA,
        mut latch: LATCH,
        delay: DELAY,
    ) -> Result<Self, Error<E>> {
        latch.set_low().map_err(Error::Pin)?;
        clock.set_low().map_err(Error::Pin)?;
        data.set_low().map_err(Error::Pin)?;
        Ok(Self {
            clock,
            data,
            latch,
            delay,
            latched_image: None,
        })
    }

    /// Shifts every physical bit and commits it with one rising latch edge.
    pub fn write_complete(&mut self, image: CompleteImage, timing: Timing) -> Result<(), Error<E>> {
        image.validate().map_err(Error::Image)?;
        timing.validate().map_err(Error::Timing)?;
        self.latched_image = None;
        self.latch.set_low().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.latch_setup_ns);
        self.clock.set_low().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.clock_low_ns);

        for serial_index in 0..image.width {
            let logical_bit = match image.order {
                BitOrder::MostSignificantFirst => image.width - 1 - serial_index,
                BitOrder::LeastSignificantFirst => serial_index,
            };
            if image.bits & (1_u32 << logical_bit) == 0 {
                self.data.set_low().map_err(Error::Pin)?;
            } else {
                self.data.set_high().map_err(Error::Pin)?;
            }
            self.delay.delay_ns(timing.data_setup_ns);
            self.clock.set_high().map_err(Error::Pin)?;
            self.delay.delay_ns(timing.clock_high_ns);
            self.clock.set_low().map_err(Error::Pin)?;
            self.delay.delay_ns(timing.clock_low_ns);
        }

        self.data.set_low().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.data_setup_ns);
        self.latch.set_high().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.latch_high_ns);
        self.latch.set_low().map_err(Error::Pin)?;
        self.latched_image = Some(image);
        Ok(())
    }

    /// Returns the last image only after its entire transaction succeeded.
    pub const fn latched_image(&self) -> Option<CompleteImage> {
        self.latched_image
    }

    /// Returns owned pins for a deliberate, externally synchronized handoff.
    pub fn into_parts(self) -> (CLOCK, DATA, LATCH, DELAY) {
        (self.clock, self.data, self.latch, self.delay)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::sync::{Arc, Mutex};
    use std::vec;
    use std::vec::Vec;

    use embedded_hal::digital::ErrorType;

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Line {
        Clock,
        Data,
        Latch,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Edge(Line, bool),
        Delay(u32),
    }

    struct FakePin {
        line: Line,
        trace: Arc<Mutex<Vec<Event>>>,
    }

    impl ErrorType for FakePin {
        type Error = core::convert::Infallible;
    }

    impl OutputPin for FakePin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.trace
                .lock()
                .unwrap()
                .push(Event::Edge(self.line, false));
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.trace
                .lock()
                .unwrap()
                .push(Event::Edge(self.line, true));
            Ok(())
        }
    }

    struct FakeDelay {
        trace: Arc<Mutex<Vec<Event>>>,
    }

    type FakeTransport = StaticShiftRegister<FakePin, FakePin, FakePin, FakeDelay>;
    type Trace = Arc<Mutex<Vec<Event>>>;

    impl DelayNs for FakeDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.trace.lock().unwrap().push(Event::Delay(ns));
        }
    }

    fn transport() -> (FakeTransport, Trace) {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let pin = |line| FakePin {
            line,
            trace: Arc::clone(&trace),
        };
        let delay = FakeDelay {
            trace: Arc::clone(&trace),
        };
        let transport =
            StaticShiftRegister::new(pin(Line::Clock), pin(Line::Data), pin(Line::Latch), delay)
                .unwrap();
        (transport, trace)
    }

    const fn image(bits: u32, order: BitOrder) -> CompleteImage {
        CompleteImage {
            width: 24,
            defined_mask: 0x00ff_ffff,
            bits,
            order,
        }
    }

    #[test]
    fn pcm_short_mono_frame_repeats_a_complete_serial_suffix() {
        let requested = image(0x0012_3456, BitOrder::MostSignificantFirst);
        let frame = PcmShortMonoFrame::new(requested).unwrap();
        assert_eq!(frame.sample_word(), 0x0012_3456);
        assert_eq!(frame.dma_bytes_le(), [0x56, 0x34, 0x12, 0x00]);
        assert_eq!(frame.commits_image(), requested);
        assert!(frame.suffix_is_complete());
        for bit in 0..PCM_SHORT_SLOT_BITS {
            assert_eq!(frame.serial_data_at(bit), frame.serial_data_at(bit + 32));
        }
        assert_eq!(frame.serial_data_at(PCM_SHORT_FRAME_BITS), None);

        let least_first = CompleteImage {
            width: 4,
            defined_mask: 0b1111,
            bits: 0b1101,
            order: BitOrder::LeastSignificantFirst,
        };
        let frame = PcmShortMonoFrame::new(least_first).unwrap();
        assert_eq!(frame.sample_word(), 0b1011);
        assert!(frame.suffix_is_complete());
        assert_eq!(
            [60, 61, 62, 63].map(|index| frame.serial_data_at(index).unwrap()),
            [true, false, true, true]
        );
    }

    #[test]
    fn pcm_short_grid_rejects_rounding_and_exposes_one_frame_lead() {
        let grid = PcmShortFrameGrid::new(1_000, 1_000_000, 250_000).unwrap();
        assert_eq!(grid.epoch_cycle(), 1_000);
        assert_eq!(grid.device_cycle_hz(), 1_000_000);
        assert_eq!(grid.frame_rate_hz(), 250_000);
        assert_eq!(grid.cycles_per_frame(), 4);
        assert_eq!(grid.bit_clock_hz(), 16_000_000);
        assert_eq!(grid.boundary_cycle(3), Ok(1_012));
        assert_eq!(grid.transmit_index_for_commit(1_004), Ok(0));
        assert_eq!(grid.transmit_index_for_commit(1_012), Ok(2));
        assert_eq!(
            grid.transmit_index_for_commit(1_000),
            Err(PcmShortGridError::PipelineLead {
                commit_cycle: 1_000,
                first_commit_cycle: 1_004,
            })
        );
        assert_eq!(
            grid.transmit_index_for_commit(1_005),
            Err(PcmShortGridError::Unaligned {
                received_cycle: 1_005,
                cycles_per_frame: 4,
            })
        );
        assert_eq!(
            PcmShortFrameGrid::new(0, 1_000_000, 333_333),
            Err(PcmShortGridError::FractionalPeriod {
                device_cycle_hz: 1_000_000,
                frame_rate_hz: 333_333,
            })
        );
        assert_eq!(
            PcmShortFrameGrid::new(u64::MAX - 1, 1_000_000, 250_000)
                .unwrap()
                .boundary_cycle(1),
            Err(PcmShortGridError::Arithmetic)
        );
    }

    #[test]
    fn sparse_updates_expand_one_frame_early_without_image_gaps() {
        let grid = PcmShortFrameGrid::new(1_000, 1_000_000, 250_000).unwrap();
        let safe = image(0x0000_1249, BitOrder::MostSignificantFirst);
        let first = image(0x0000_1269, BitOrder::MostSignificantFirst);
        let second = image(0x0000_12e9, BitOrder::MostSignificantFirst);
        let mut timeline = PcmShortTimeline::<2>::new(grid, safe).unwrap();
        timeline
            .schedule(ScheduledCompleteImage {
                commit_cycle: 1_012,
                image: first,
            })
            .unwrap();
        timeline
            .schedule(ScheduledCompleteImage {
                commit_cycle: 1_020,
                image: second,
            })
            .unwrap();
        assert_eq!(timeline.queued_updates(), 2);

        let expected = [safe, safe, first, first, second];
        for (index, expected_image) in expected.into_iter().enumerate() {
            let frame = timeline.next_frame().unwrap();
            assert_eq!(frame.transmit_index, index as u64);
            assert_eq!(frame.starts_at, 1_000 + index as u64 * 4);
            assert_eq!(frame.commits_at, frame.starts_at + 4);
            assert_eq!(frame.frame.commits_image(), expected_image);
            assert!(frame.frame.suffix_is_complete());
        }
        assert_eq!(timeline.queued_updates(), 0);
        assert_eq!(timeline.next_commit_cycle(), Ok(1_024));
        assert_eq!(
            timeline.schedule(ScheduledCompleteImage {
                commit_cycle: 1_020,
                image: safe,
            }),
            Err(PcmShortTimelineError::Order)
        );
    }

    #[test]
    fn timeline_rejects_capacity_order_alignment_and_contract_changes() {
        let grid = PcmShortFrameGrid::new(100, 1_000_000, 250_000).unwrap();
        let safe = image(0x1249, BitOrder::MostSignificantFirst);
        let update = image(0x1269, BitOrder::MostSignificantFirst);
        let mut timeline = PcmShortTimeline::<1>::new(grid, safe).unwrap();
        timeline
            .schedule(ScheduledCompleteImage {
                commit_cycle: 104,
                image: update,
            })
            .unwrap();
        assert_eq!(
            timeline.schedule(ScheduledCompleteImage {
                commit_cycle: 108,
                image: safe,
            }),
            Err(PcmShortTimelineError::Capacity)
        );
        let _ = timeline.next_frame().unwrap();
        assert_eq!(
            timeline.schedule(ScheduledCompleteImage {
                commit_cycle: 109,
                image: safe,
            }),
            Err(PcmShortTimelineError::Grid(PcmShortGridError::Unaligned {
                received_cycle: 109,
                cycles_per_frame: 4,
            }))
        );
        let incompatible = CompleteImage {
            width: 16,
            defined_mask: 0xffff,
            bits: 0x1249,
            order: BitOrder::MostSignificantFirst,
        };
        assert_eq!(
            timeline.schedule(ScheduledCompleteImage {
                commit_cycle: 108,
                image: incompatible,
            }),
            Err(PcmShortTimelineError::Contract)
        );

        let mut no_updates = PcmShortTimeline::<0>::new(grid, safe).unwrap();
        assert_eq!(
            no_updates.schedule(ScheduledCompleteImage {
                commit_cycle: 104,
                image: update,
            }),
            Err(PcmShortTimelineError::Capacity)
        );
        assert_eq!(no_updates.next_frame().unwrap().frame.commits_image(), safe);
    }

    #[test]
    fn dma_horizon_separates_refill_seal_and_latch_observation() {
        let grid = PcmShortFrameGrid::new(100, 1_000_000, 250_000).unwrap();
        let safe = image(0x1249, BitOrder::MostSignificantFirst);
        let first = image(0x1269, BitOrder::MostSignificantFirst);
        let second = image(0x12e9, BitOrder::MostSignificantFirst);
        let mut horizon = PcmShortDmaHorizon::<u16, 4, 4>::new(grid, safe).unwrap();

        // Four externally prefilled frames cover boundaries 104 through 116.
        assert_eq!(horizon.sealed_horizon(), Ok(116));
        assert_eq!(horizon.writable_horizon(), Ok(116));
        assert_eq!(horizon.refill_credit_frames(), 0);
        assert_eq!(horizon.observe_latches_through(116), Ok(()));

        horizon
            .stage(TaggedScheduledCompleteImage {
                tag: 7,
                commit_cycle: 120,
                image: first,
            })
            .unwrap();
        horizon
            .stage(TaggedScheduledCompleteImage {
                tag: 8,
                commit_cycle: 128,
                image: second,
            })
            .unwrap();
        assert_eq!(horizon.pending_updates(), 2);
        assert_eq!(horizon.take_commit(), Ok(None));

        horizon.synchronize_refill_availability(2).unwrap();
        assert_eq!(horizon.writable_horizon(), Ok(124));
        let frame = horizon.next_refill_frame().unwrap().unwrap();
        assert_eq!(frame.transmit_index, 4);
        assert_eq!(frame.starts_at, 116);
        assert_eq!(frame.commits_at, 120);
        assert_eq!(frame.frame.commits_image(), first);
        assert_eq!(horizon.next_refill_frame(), Ok(Some(frame)));
        assert_eq!(horizon.accept_refill(frame), Ok(120));

        let repeated = horizon.next_refill_frame().unwrap().unwrap();
        assert_eq!(repeated.transmit_index, 5);
        assert_eq!(repeated.commits_at, 124);
        assert_eq!(repeated.frame.commits_image(), first);
        assert_eq!(horizon.accept_refill(repeated), Ok(124));
        assert_eq!(horizon.next_refill_frame(), Ok(None));
        assert_eq!(horizon.take_commit(), Ok(None));

        horizon.observe_latches_through(120).unwrap();
        assert_eq!(
            horizon.take_commit(),
            Ok(Some(PcmShortCommitObservation {
                tag: 7,
                commit_cycle: 120,
            }))
        );
        assert_eq!(horizon.take_commit(), Ok(None));

        horizon.synchronize_refill_availability(2).unwrap();
        assert_eq!(horizon.writable_horizon(), Ok(132));
        let second_frame = horizon.next_refill_frame().unwrap().unwrap();
        assert_eq!(second_frame.transmit_index, 6);
        assert_eq!(second_frame.commits_at, 128);
        assert_eq!(second_frame.frame.commits_image(), second);
        horizon.accept_refill(second_frame).unwrap();
        let second_repeat = horizon.next_refill_frame().unwrap().unwrap();
        assert_eq!(second_repeat.commits_at, 132);
        assert_eq!(second_repeat.frame.commits_image(), second);
        assert_eq!(horizon.accept_refill(second_repeat), Ok(132));

        horizon.observe_latches_through(132).unwrap();
        assert_eq!(
            horizon.take_commit(),
            Ok(Some(PcmShortCommitObservation {
                tag: 8,
                commit_cycle: 128,
            }))
        );
        assert_eq!(horizon.take_commit(), Ok(None));
        assert_eq!(horizon.fault(), None);
    }

    #[test]
    fn dma_horizon_rejects_late_updates_untracked_writes_and_underrun() {
        let grid = PcmShortFrameGrid::new(100, 1_000_000, 250_000).unwrap();
        let safe = image(0x1249, BitOrder::MostSignificantFirst);
        let update = image(0x1269, BitOrder::MostSignificantFirst);

        let mut late = PcmShortDmaHorizon::<u8, 2, 4>::new(grid, safe).unwrap();
        assert_eq!(
            late.stage(TaggedScheduledCompleteImage {
                tag: 1,
                commit_cycle: 116,
                image: update,
            }),
            Err(PcmShortDmaHorizonError::Timeline(
                PcmShortTimelineError::Order
            ))
        );
        assert_eq!(
            late.sealed_horizon(),
            Err(PcmShortDmaHorizonError::FaultLatched)
        );

        let mut untracked = PcmShortDmaHorizon::<u8, 2, 4>::new(grid, safe).unwrap();
        untracked.synchronize_refill_availability(2).unwrap();
        assert!(untracked.next_refill_frame().unwrap().is_some());
        assert_eq!(
            untracked.synchronize_refill_availability(1),
            Err(PcmShortDmaHorizonError::Availability {
                reported: 1,
                retained: 2,
                ring_frames: 4,
            })
        );
        assert_eq!(
            untracked.next_refill_frame(),
            Err(PcmShortDmaHorizonError::FaultLatched)
        );

        let mut starved = PcmShortDmaHorizon::<u8, 2, 4>::new(grid, safe).unwrap();
        assert_eq!(
            starved.observe_latches_through(120),
            Err(PcmShortDmaHorizonError::Underrun {
                observed: 120,
                sealed: 116,
            })
        );
        assert_eq!(
            starved.observe_latches_through(116),
            Err(PcmShortDmaHorizonError::FaultLatched)
        );
        assert_eq!(
            PcmShortDmaHorizon::<u8, 1, 0>::new(grid, safe).err(),
            Some(PcmShortDmaHorizonError::RingCapacity)
        );
    }

    #[test]
    fn dma_horizon_requires_exact_two_phase_refill_identity() {
        let grid = PcmShortFrameGrid::new(0, 1_000_000, 250_000).unwrap();
        let safe = image(0x1249, BitOrder::MostSignificantFirst);
        let mut horizon = PcmShortDmaHorizon::<u8, 1, 3>::new(grid, safe).unwrap();
        horizon.synchronize_refill_availability(1).unwrap();
        let expected = horizon.next_refill_frame().unwrap().unwrap();
        let mut wrong = expected;
        wrong.commits_at += 4;
        assert_eq!(
            horizon.accept_refill(wrong),
            Err(PcmShortDmaHorizonError::PendingFrameMismatch)
        );
        assert_eq!(
            horizon.fault(),
            Some(PcmShortDmaHorizonError::PendingFrameMismatch)
        );
        assert_eq!(
            horizon.accept_refill(expected),
            Err(PcmShortDmaHorizonError::FaultLatched)
        );
    }

    #[test]
    fn most_significant_first_transaction_is_exact_and_ends_inactive() {
        let (mut transport, trace) = transport();
        let image = CompleteImage {
            width: 3,
            defined_mask: 0b111,
            bits: 0b101,
            order: BitOrder::MostSignificantFirst,
        };
        let timing = Timing {
            data_setup_ns: 1,
            clock_high_ns: 2,
            clock_low_ns: 3,
            latch_setup_ns: 4,
            latch_high_ns: 5,
        };
        transport.write_complete(image, timing).unwrap();

        assert_eq!(
            *trace.lock().unwrap(),
            vec![
                Event::Edge(Line::Latch, false),
                Event::Edge(Line::Clock, false),
                Event::Edge(Line::Data, false),
                Event::Edge(Line::Latch, false),
                Event::Delay(4),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, true),
                Event::Delay(1),
                Event::Edge(Line::Clock, true),
                Event::Delay(2),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, false),
                Event::Delay(1),
                Event::Edge(Line::Clock, true),
                Event::Delay(2),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, true),
                Event::Delay(1),
                Event::Edge(Line::Clock, true),
                Event::Delay(2),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, false),
                Event::Delay(1),
                Event::Edge(Line::Latch, true),
                Event::Delay(5),
                Event::Edge(Line::Latch, false),
            ]
        );
        assert_eq!(transport.latched_image(), Some(image));
    }

    #[test]
    fn incomplete_image_is_rejected_before_any_write_activity() {
        let (mut transport, trace) = transport();
        let before = trace.lock().unwrap().len();
        let image = CompleteImage {
            width: 24,
            defined_mask: 0x00ff_7fff,
            bits: 0x1249,
            order: BitOrder::MostSignificantFirst,
        };
        assert_eq!(
            transport.write_complete(image, Timing::CONSERVATIVE_100NS),
            Err(Error::Image(ImageError::Incomplete {
                received_mask: 0x00ff_7fff,
                expected_mask: 0x00ff_ffff,
            }))
        );
        assert_eq!(trace.lock().unwrap().len(), before);
        assert_eq!(transport.latched_image(), None);
    }

    #[test]
    fn width_32_has_no_shift_overflow() {
        let (mut transport, _) = transport();
        let image = CompleteImage {
            width: 32,
            defined_mask: u32::MAX,
            bits: u32::MAX,
            order: BitOrder::LeastSignificantFirst,
        };
        assert_eq!(image.validate(), Ok(()));
        assert_eq!(
            transport.write_complete(image, Timing::CONSERVATIVE_100NS),
            Ok(())
        );
        assert_eq!(transport.latched_image(), Some(image));
    }

    #[test]
    fn invalid_timing_is_rejected_before_any_write_activity() {
        let (mut transport, trace) = transport();
        let before = trace.lock().unwrap().len();
        let image = CompleteImage {
            width: 1,
            defined_mask: 1,
            bits: 0,
            order: BitOrder::MostSignificantFirst,
        };
        let mut timing = Timing::CONSERVATIVE_100NS;
        timing.clock_high_ns = 0;
        assert_eq!(
            transport.write_complete(image, timing),
            Err(Error::Timing(TimingError {
                field_index: 1,
                received_ns: 0,
            }))
        );
        assert_eq!(trace.lock().unwrap().len(), before);
    }
}
