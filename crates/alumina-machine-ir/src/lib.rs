#![no_std]
#![doc = "Canonical bounded integer work accepted by an Alumina real-time executor."]

use alumina_protocol::{DeviceCycle, Digest};

/// Magic identifying a per-MCU Alumina machine-IR partition.
pub const JOB_MAGIC: [u8; 4] = *b"AJOB";

/// Exact machine-IR schema implemented here.
pub const MACHINE_IR_VERSION: u16 = 1;

/// A fixed job-partition prefix validated before arming.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct JobHeader {
    /// Constant [`JOB_MAGIC`].
    pub magic: [u8; 4],
    /// Must equal [`MACHINE_IR_VERSION`].
    pub version: u16,
    /// Number of coordinate axes encoded by each segment.
    pub axis_count: u8,
    /// Reserved flags; V1 requires zero.
    pub flags: u8,
    /// Number of following [`Segment`] values.
    pub segment_count: u32,
    /// Exact board capability set used by the compiler.
    pub capability_digest: Digest,
    /// Exact active machine configuration used by the compiler.
    pub config_digest: Digest,
    /// Digest of the immutable per-MCU partition bytes.
    pub partition_digest: Digest,
}

impl JobHeader {
    /// Constructs a V1 header. A serializer fills `partition_digest` after encoding.
    pub const fn new(axis_count: u8, segment_count: u32) -> Self {
        Self {
            magic: JOB_MAGIC,
            version: MACHINE_IR_VERSION,
            axis_count,
            flags: 0,
            segment_count,
            capability_digest: Digest::ZERO,
            config_digest: Digest::ZERO,
            partition_digest: Digest::ZERO,
        }
    }
}

/// Smallest auditable V1 coordinated integer-motion segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(C)]
pub struct Segment<const AXES: usize> {
    /// Inclusive local-device start tick.
    pub start_cycle: DeviceCycle,
    /// Exclusive local-device end tick.
    pub end_cycle: DeviceCycle,
    /// Signed commanded lattice displacement for each axis.
    pub delta_steps: [i64; AXES],
    /// Reserved V1 segment flags; must be zero.
    pub flags: u32,
}

/// Board/config-derived limits used for bounded firmware validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ValidationLimits {
    /// Longest accepted segment duration in device ticks.
    pub maximum_segment_ticks: u64,
    /// Largest absolute step displacement in one segment on any axis.
    pub maximum_steps_per_segment: u64,
}

/// Borrowed machine partition after core-0 decoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Job<'a, const AXES: usize> {
    /// Canonical partition header.
    pub header: JobHeader,
    /// Fixed-axis coordinated segments.
    pub segments: &'a [Segment<AXES>],
}

impl<const AXES: usize> Job<'_, AXES> {
    /// Validates structural, identity, time, displacement, and overflow invariants.
    pub fn validate(&self, limits: ValidationLimits) -> Result<JobSummary<AXES>, Error> {
        if self.header.magic != JOB_MAGIC {
            return Err(Error::Magic);
        }
        if self.header.version != MACHINE_IR_VERSION {
            return Err(Error::Version {
                received: self.header.version,
            });
        }
        if usize::from(self.header.axis_count) != AXES || AXES == 0 {
            return Err(Error::AxisCount {
                encoded: self.header.axis_count,
                expected: AXES,
            });
        }
        if self.header.flags != 0 {
            return Err(Error::HeaderFlags(self.header.flags));
        }
        if usize::try_from(self.header.segment_count).ok() != Some(self.segments.len()) {
            return Err(Error::SegmentCount {
                encoded: self.header.segment_count,
                actual: self.segments.len(),
            });
        }
        if self.header.capability_digest.is_zero() {
            return Err(Error::MissingCapabilityDigest);
        }
        if self.header.config_digest.is_zero() {
            return Err(Error::MissingConfigDigest);
        }
        if self.header.partition_digest.is_zero() {
            return Err(Error::MissingPartitionDigest);
        }

        let mut end_cycle = DeviceCycle(0);
        let mut final_steps = [0_i64; AXES];
        let mut index = 0;
        while index < self.segments.len() {
            let segment = self.segments[index];
            if segment.flags != 0 {
                return Err(Error::SegmentFlags {
                    index,
                    flags: segment.flags,
                });
            }
            if segment.end_cycle.0 <= segment.start_cycle.0 {
                return Err(Error::EmptyOrReversedTime { index });
            }
            if index != 0 && segment.start_cycle != end_cycle {
                return Err(Error::NonContiguousTime { index });
            }
            let duration = segment.end_cycle.0 - segment.start_cycle.0;
            if duration > limits.maximum_segment_ticks {
                return Err(Error::SegmentTooLong { index, duration });
            }

            let mut axis = 0;
            while axis < AXES {
                let delta = segment.delta_steps[axis];
                let magnitude = delta.unsigned_abs();
                if magnitude > limits.maximum_steps_per_segment {
                    return Err(Error::TooManySteps {
                        index,
                        axis,
                        magnitude,
                    });
                }
                final_steps[axis] = final_steps[axis]
                    .checked_add(delta)
                    .ok_or(Error::PositionOverflow { index, axis })?;
                axis += 1;
            }
            end_cycle = segment.end_cycle;
            index += 1;
        }

        Ok(JobSummary {
            end_cycle,
            final_steps,
        })
    }
}

/// Facts established by validation and useful for arming diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct JobSummary<const AXES: usize> {
    /// Exclusive end tick, or zero for an empty job.
    pub end_cycle: DeviceCycle,
    /// Final relative lattice displacement on every axis.
    pub final_steps: [i64; AXES],
}

/// Rejection reason carrying the exact segment/axis where possible.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    /// Partition magic mismatch.
    Magic,
    /// Exact machine-IR version mismatch.
    Version {
        /// Version found in the partition.
        received: u16,
    },
    /// Compile-time axis width did not match the encoded width.
    AxisCount {
        /// Encoded axis count.
        encoded: u8,
        /// Validator axis count.
        expected: usize,
    },
    /// Unknown header flags were set.
    HeaderFlags(u8),
    /// Header count did not match the decoded slice.
    SegmentCount {
        /// Encoded count.
        encoded: u32,
        /// Decoded count.
        actual: usize,
    },
    /// Capability identity was not bound.
    MissingCapabilityDigest,
    /// Machine configuration identity was not bound.
    MissingConfigDigest,
    /// Immutable partition identity was not bound.
    MissingPartitionDigest,
    /// Unknown segment flags were set.
    SegmentFlags {
        /// Segment index.
        index: usize,
        /// Unknown bits.
        flags: u32,
    },
    /// Segment had zero duration or reversed time.
    EmptyOrReversedTime {
        /// Segment index.
        index: usize,
    },
    /// A segment did not start exactly where its predecessor ended.
    NonContiguousTime {
        /// Segment index.
        index: usize,
    },
    /// Segment exceeded the board/config duration bound.
    SegmentTooLong {
        /// Segment index.
        index: usize,
        /// Encoded duration.
        duration: u64,
    },
    /// Axis displacement exceeded its per-segment bound.
    TooManySteps {
        /// Segment index.
        index: usize,
        /// Axis index.
        axis: usize,
        /// Absolute displacement.
        magnitude: u64,
    },
    /// Cumulative relative position exceeded `i64`.
    PositionOverflow {
        /// Segment index.
        index: usize,
        /// Axis index.
        axis: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header(segment_count: u32) -> JobHeader {
        JobHeader {
            capability_digest: Digest([1; 32]),
            config_digest: Digest([2; 32]),
            partition_digest: Digest([3; 32]),
            ..JobHeader::new(3, segment_count)
        }
    }

    fn limits() -> ValidationLimits {
        ValidationLimits {
            maximum_segment_ticks: 1_000,
            maximum_steps_per_segment: 100,
        }
    }

    #[test]
    fn contiguous_integer_job_returns_terminal_facts() {
        let segments = [
            Segment {
                start_cycle: DeviceCycle(100),
                end_cycle: DeviceCycle(200),
                delta_steps: [10, -4, 0],
                flags: 0,
            },
            Segment {
                start_cycle: DeviceCycle(200),
                end_cycle: DeviceCycle(350),
                delta_steps: [5, 4, 2],
                flags: 0,
            },
        ];
        let job = Job {
            header: header(2),
            segments: &segments,
        };

        assert_eq!(
            job.validate(limits()),
            Ok(JobSummary {
                end_cycle: DeviceCycle(350),
                final_steps: [15, 0, 2]
            })
        );
    }

    #[test]
    fn time_gap_is_rejected_at_exact_segment() {
        let segments = [
            Segment {
                start_cycle: DeviceCycle(10),
                end_cycle: DeviceCycle(20),
                delta_steps: [1, 0, 0],
                flags: 0,
            },
            Segment {
                start_cycle: DeviceCycle(21),
                end_cycle: DeviceCycle(30),
                delta_steps: [1, 0, 0],
                flags: 0,
            },
        ];
        let job = Job {
            header: header(2),
            segments: &segments,
        };

        assert_eq!(
            job.validate(limits()),
            Err(Error::NonContiguousTime { index: 1 })
        );
    }

    #[test]
    fn missing_identity_never_arms() {
        let job = Job::<3> {
            header: JobHeader::new(3, 0),
            segments: &[],
        };
        assert_eq!(job.validate(limits()), Err(Error::MissingCapabilityDigest));
    }

    #[test]
    fn extreme_delta_is_checked_without_signed_abs_overflow() {
        let segments = [Segment {
            start_cycle: DeviceCycle(0),
            end_cycle: DeviceCycle(1),
            delta_steps: [i64::MIN, 0, 0],
            flags: 0,
        }];
        let job = Job {
            header: header(1),
            segments: &segments,
        };
        assert_eq!(
            job.validate(limits()),
            Err(Error::TooManySteps {
                index: 0,
                axis: 0,
                magnitude: 1_u64 << 63
            })
        );
    }
}
