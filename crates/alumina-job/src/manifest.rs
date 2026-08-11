//! Canonical global identity and participant map for cached multi-MCU jobs.

use alumina_machine_ir::{EXECUTION_BLOCK_BYTES, MAX_EXECUTION_AXES, StreamId, StreamTick};
use alumina_protocol::{DeviceId, Digest};
use alumina_storage::{ContentHasher, sha256};

use crate::JobNetworkPolicy;

/// Maximum participants admitted by global job-manifest schema V1.
pub const MAX_JOB_PARTICIPANTS: usize = 16;
/// Fixed bytes preceding participant records.
pub const MACHINE_JOB_MANIFEST_HEADER_BYTES: usize = 320;
/// Fixed bytes in one canonical participant record.
pub const MACHINE_JOB_PARTICIPANT_BYTES: usize = 496;

const MANIFEST_MAGIC: [u8; 8] = *b"ALMJMF01";
const MANIFEST_VERSION: u16 = 1;
const PARTICIPANT_SET_DOMAIN: [u8; 16] = *b"ALM-PARTSET-V1!\0";

/// Global compile, coordinate, timing, and safety facts shared by every
/// participant partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineJobGlobalFacts {
    /// Network-loss behavior required by the complete job.
    pub network_policy: JobNetworkPolicy,
    /// Integer ticks per exact second in the global compiler timeline.
    pub global_timebase_hz: u64,
    /// Exact terminal tick in the global compiler timeline.
    pub duration_ticks: u64,
    /// Exact source/project identity consumed by CAM.
    pub source_digest: Digest,
    /// Compiler build and schema identity.
    pub compiler_digest: Digest,
    /// Exact interface bundle/build identity that performed compilation.
    pub interface_digest: Digest,
    /// Complete precision, approximation, and scheduling policy identity.
    pub policy_digest: Digest,
    /// Global machine/kinematics/resource configuration identity.
    pub machine_digest: Digest,
    /// Shared coordinate-frame and initial-state epoch identity.
    pub coordinate_epoch_digest: Digest,
    /// Global safety/failure/hold/cancel policy identity.
    pub safety_policy_digest: Digest,
    /// Ordered synchronization-marker set identity.
    pub synchronization_digest: Digest,
}

/// Immutable facts for exactly one MCU's cached execution partition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineJobParticipant {
    /// Stable physical MCU identity; records are sorted strictly by this field.
    pub device_id: DeviceId,
    /// Identity repeated by every local execution block.
    pub stream_id: StreamId,
    /// Exact board-package/revision identity.
    pub board_package_digest: Digest,
    /// Exact capability document used by compilation.
    pub capability_digest: Digest,
    /// Exact durably active local machine configuration.
    pub config_digest: Digest,
    /// SHA-256 of the complete local execution-block object.
    pub partition_digest: Digest,
    /// SHA-256 of the local partition's ordered storage-chunk manifest.
    pub partition_manifest_digest: Digest,
    /// Digest of the terminal execution block in the local chain.
    pub terminal_block_digest: Digest,
    /// Complete set of local peripherals/pins/timers owned by this partition.
    pub resource_set_digest: Digest,
    /// Local geometric/timing/control error evidence identity.
    pub error_evidence_digest: Digest,
    /// Local interlock, failure, and maximum-energy envelope identity.
    pub safety_envelope_digest: Digest,
    /// Exact bytes in the local partition object.
    pub partition_byte_len: u64,
    /// Exact number of 512-byte blocks implied by `partition_byte_len`.
    pub block_count: u32,
    /// Active coordinate axes in each local motion record.
    pub axis_count: u8,
    /// Exact local stream timer ticks per second.
    pub local_timer_hz: u64,
    /// First local stream-relative tick; V1 requires zero.
    pub first_tick: StreamTick,
    /// Exclusive local terminal stream-relative tick.
    pub end_tick: StreamTick,
    /// Exact absolute local lattice position at `first_tick`.
    pub initial_position: [i64; MAX_EXECUTION_AXES],
    /// Exact expected absolute local lattice position at `end_tick`.
    pub final_position: [i64; MAX_EXECUTION_AXES],
}

/// Borrowed, validated global manifest ready for canonical encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MachineJobManifest<'a> {
    global: MachineJobGlobalFacts,
    participant_set_digest: Digest,
    participants: &'a [MachineJobParticipant],
}

impl<'a> MachineJobManifest<'a> {
    /// Validates global/participant invariants and derives the canonical ordered
    /// participant-set identity.
    pub fn new(
        global: MachineJobGlobalFacts,
        participants: &'a [MachineJobParticipant],
    ) -> Result<Self, MachineJobManifestError> {
        validate_global(global)?;
        validate_participants(global, participants)?;
        let participant_set_digest = hash_participant_set(participants)?;
        Ok(Self {
            global,
            participant_set_digest,
            participants,
        })
    }

    /// Return shared compiler/timing/safety facts.
    pub const fn global(&self) -> MachineJobGlobalFacts {
        self.global
    }

    /// Borrow the strictly ordered participant records.
    pub const fn participants(&self) -> &'a [MachineJobParticipant] {
        self.participants
    }

    /// Return the digest copied into every participant's schedule commit.
    pub const fn participant_set_digest(&self) -> Digest {
        self.participant_set_digest
    }

    /// Return the exact canonical byte length.
    pub fn wire_len(&self) -> Result<usize, MachineJobManifestError> {
        manifest_wire_len(self.participants.len())
    }

    /// Encode into an exactly sized caller-owned buffer.
    pub fn encode_into(&self, output: &mut [u8]) -> Result<usize, MachineJobManifestError> {
        let expected = self.wire_len()?;
        if output.len() != expected {
            return Err(MachineJobManifestError::WireLength {
                received: output.len(),
                expected,
            });
        }
        output[..MACHINE_JOB_MANIFEST_HEADER_BYTES].copy_from_slice(&encode_header(
            self.global,
            self.participant_set_digest,
            self.participants.len(),
        )?);
        for (index, participant) in self.participants.iter().copied().enumerate() {
            let start = MACHINE_JOB_MANIFEST_HEADER_BYTES
                .checked_add(
                    index
                        .checked_mul(MACHINE_JOB_PARTICIPANT_BYTES)
                        .ok_or(MachineJobManifestError::Arithmetic)?,
                )
                .ok_or(MachineJobManifestError::Arithmetic)?;
            output[start..start + MACHINE_JOB_PARTICIPANT_BYTES]
                .copy_from_slice(&encode_participant(participant));
        }
        Ok(expected)
    }

    /// SHA-256 of the exact canonical manifest bytes, without allocating the
    /// complete manifest in firmware.
    pub fn global_job_digest(&self) -> Result<Digest, MachineJobManifestError> {
        let mut hasher = ContentHasher::new();
        hasher.update(&encode_header(
            self.global,
            self.participant_set_digest,
            self.participants.len(),
        )?);
        for participant in self.participants.iter().copied() {
            hasher.update(&encode_participant(participant));
        }
        Ok(hasher.finalize().digest)
    }
}

/// Allocation-free decoded view over an exact canonical manifest object.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DecodedMachineJobManifest<'a> {
    encoded: &'a [u8],
    global: MachineJobGlobalFacts,
    participant_set_digest: Digest,
    participant_count: usize,
}

impl<'a> DecodedMachineJobManifest<'a> {
    /// Decode and independently validate an exact complete manifest.
    pub fn decode(encoded: &'a [u8]) -> Result<Self, MachineJobManifestError> {
        if encoded.len() < MACHINE_JOB_MANIFEST_HEADER_BYTES {
            return Err(MachineJobManifestError::WireLength {
                received: encoded.len(),
                expected: MACHINE_JOB_MANIFEST_HEADER_BYTES,
            });
        }
        if encoded[0..8] != MANIFEST_MAGIC {
            return Err(MachineJobManifestError::Magic);
        }
        let version = read_u16(encoded, 8);
        if version != MANIFEST_VERSION {
            return Err(MachineJobManifestError::Version { received: version });
        }
        if encoded[11] != 0 {
            return Err(MachineJobManifestError::Reserved);
        }
        let participant_count = usize::try_from(read_u32(encoded, 12))
            .map_err(|_| MachineJobManifestError::ParticipantCount)?;
        let expected = manifest_wire_len(participant_count)?;
        if encoded.len() != expected {
            return Err(MachineJobManifestError::WireLength {
                received: encoded.len(),
                expected,
            });
        }
        let global = MachineJobGlobalFacts {
            network_policy: JobNetworkPolicy::from_wire(encoded[10])
                .ok_or(MachineJobManifestError::NetworkPolicy)?,
            global_timebase_hz: read_u64(encoded, 16),
            duration_ticks: read_u64(encoded, 24),
            source_digest: read_digest(encoded, 32),
            compiler_digest: read_digest(encoded, 64),
            interface_digest: read_digest(encoded, 96),
            policy_digest: read_digest(encoded, 128),
            machine_digest: read_digest(encoded, 160),
            coordinate_epoch_digest: read_digest(encoded, 192),
            safety_policy_digest: read_digest(encoded, 224),
            synchronization_digest: read_digest(encoded, 256),
        };
        let participant_set_digest = read_digest(encoded, 288);
        validate_global(global)?;

        let decoded = Self {
            encoded,
            global,
            participant_set_digest,
            participant_count,
        };
        decoded.validate_participants()?;
        Ok(decoded)
    }

    /// Borrow exact canonical bytes, whose SHA-256 is the global job digest.
    pub const fn as_bytes(&self) -> &'a [u8] {
        self.encoded
    }

    /// Return shared compiler/timing/safety facts.
    pub const fn global(&self) -> MachineJobGlobalFacts {
        self.global
    }

    /// Return the exact number of sorted participant records.
    pub const fn participant_count(&self) -> usize {
        self.participant_count
    }

    /// Return the participant-set digest copied into schedule commits.
    pub const fn participant_set_digest(&self) -> Digest {
        self.participant_set_digest
    }

    /// Return SHA-256 of the complete canonical manifest object.
    pub fn global_job_digest(&self) -> Digest {
        sha256(self.encoded).digest
    }

    /// Decode one independently validated participant by sorted index.
    pub fn participant(
        &self,
        index: usize,
    ) -> Result<Option<MachineJobParticipant>, MachineJobManifestError> {
        if index >= self.participant_count {
            return Ok(None);
        }
        Ok(Some(decode_participant(self.participant_bytes(index)?)?))
    }

    fn participant_bytes(&self, index: usize) -> Result<&'a [u8], MachineJobManifestError> {
        let start = MACHINE_JOB_MANIFEST_HEADER_BYTES
            .checked_add(
                index
                    .checked_mul(MACHINE_JOB_PARTICIPANT_BYTES)
                    .ok_or(MachineJobManifestError::Arithmetic)?,
            )
            .ok_or(MachineJobManifestError::Arithmetic)?;
        self.encoded
            .get(start..start + MACHINE_JOB_PARTICIPANT_BYTES)
            .ok_or(MachineJobManifestError::WireLength {
                received: self.encoded.len(),
                expected: start + MACHINE_JOB_PARTICIPANT_BYTES,
            })
    }

    fn validate_participants(&self) -> Result<(), MachineJobManifestError> {
        let mut hasher = participant_set_hasher(self.participant_count)?;
        let mut previous_device: Option<DeviceId> = None;
        let mut index = 0_usize;
        while index < self.participant_count {
            let bytes = self.participant_bytes(index)?;
            let participant = decode_participant(bytes)?;
            validate_participant(self.global, participant, index)?;
            if previous_device.is_some_and(|previous| previous >= participant.device_id) {
                return Err(MachineJobManifestError::ParticipantOrder { index });
            }
            let mut prior_index = 0_usize;
            while prior_index < index {
                let prior = decode_participant(self.participant_bytes(prior_index)?)?;
                if prior.stream_id == participant.stream_id {
                    return Err(MachineJobManifestError::DuplicateStream { index });
                }
                prior_index += 1;
            }
            hasher.update(bytes);
            previous_device = Some(participant.device_id);
            index += 1;
        }
        if hasher.finalize().digest != self.participant_set_digest {
            return Err(MachineJobManifestError::ParticipantSetDigest);
        }
        Ok(())
    }
}

fn validate_global(global: MachineJobGlobalFacts) -> Result<(), MachineJobManifestError> {
    if global.global_timebase_hz == 0 || global.duration_ticks == 0 {
        return Err(MachineJobManifestError::GlobalTimebase);
    }
    let identities = [
        global.source_digest,
        global.compiler_digest,
        global.interface_digest,
        global.policy_digest,
        global.machine_digest,
        global.coordinate_epoch_digest,
        global.safety_policy_digest,
        global.synchronization_digest,
    ];
    if identities.into_iter().any(Digest::is_zero) {
        return Err(MachineJobManifestError::GlobalIdentity);
    }
    Ok(())
}

fn validate_participants(
    global: MachineJobGlobalFacts,
    participants: &[MachineJobParticipant],
) -> Result<(), MachineJobManifestError> {
    validate_participant_count(participants.len())?;
    let mut previous_device: Option<DeviceId> = None;
    for (index, participant) in participants.iter().copied().enumerate() {
        validate_participant(global, participant, index)?;
        if previous_device.is_some_and(|previous| previous >= participant.device_id) {
            return Err(MachineJobManifestError::ParticipantOrder { index });
        }
        if participants[..index]
            .iter()
            .any(|prior| prior.stream_id == participant.stream_id)
        {
            return Err(MachineJobManifestError::DuplicateStream { index });
        }
        previous_device = Some(participant.device_id);
    }
    Ok(())
}

fn validate_participant(
    global: MachineJobGlobalFacts,
    participant: MachineJobParticipant,
    index: usize,
) -> Result<(), MachineJobManifestError> {
    if bytes_all_zero(&participant.device_id.0) {
        return Err(MachineJobManifestError::DeviceIdentity { index });
    }
    StreamId::new(participant.stream_id.0)
        .map_err(|_| MachineJobManifestError::StreamIdentity { index })?;
    let identities = [
        participant.board_package_digest,
        participant.capability_digest,
        participant.config_digest,
        participant.partition_digest,
        participant.partition_manifest_digest,
        participant.terminal_block_digest,
        participant.resource_set_digest,
        participant.error_evidence_digest,
        participant.safety_envelope_digest,
    ];
    if identities.into_iter().any(Digest::is_zero) {
        return Err(MachineJobManifestError::ParticipantIdentity { index });
    }
    if participant.axis_count == 0 || usize::from(participant.axis_count) > MAX_EXECUTION_AXES {
        return Err(MachineJobManifestError::AxisCount {
            index,
            received: participant.axis_count,
        });
    }
    let block_bytes =
        u64::try_from(EXECUTION_BLOCK_BYTES).map_err(|_| MachineJobManifestError::Arithmetic)?;
    let expected_bytes = u64::from(participant.block_count)
        .checked_mul(block_bytes)
        .ok_or(MachineJobManifestError::Arithmetic)?;
    if participant.block_count == 0
        || participant.partition_byte_len == 0
        || participant.partition_byte_len != expected_bytes
    {
        return Err(MachineJobManifestError::PartitionLayout { index });
    }
    if participant.local_timer_hz == 0
        || participant.first_tick != StreamTick(0)
        || participant.end_tick.0 == 0
    {
        return Err(MachineJobManifestError::LocalTimebase { index });
    }
    if u128::from(participant.end_tick.0) * u128::from(global.global_timebase_hz)
        != u128::from(global.duration_ticks) * u128::from(participant.local_timer_hz)
    {
        return Err(MachineJobManifestError::DurationMismatch { index });
    }
    let used_axes = usize::from(participant.axis_count);
    if participant.initial_position[used_axes..]
        .iter()
        .chain(participant.final_position[used_axes..].iter())
        .any(|position| *position != 0)
    {
        return Err(MachineJobManifestError::PositionPadding { index });
    }
    Ok(())
}

fn manifest_wire_len(participant_count: usize) -> Result<usize, MachineJobManifestError> {
    validate_participant_count(participant_count)?;
    participant_count
        .checked_mul(MACHINE_JOB_PARTICIPANT_BYTES)
        .and_then(|bytes| bytes.checked_add(MACHINE_JOB_MANIFEST_HEADER_BYTES))
        .ok_or(MachineJobManifestError::Arithmetic)
}

fn validate_participant_count(count: usize) -> Result<(), MachineJobManifestError> {
    if count == 0 || count > MAX_JOB_PARTICIPANTS || u32::try_from(count).is_err() {
        Err(MachineJobManifestError::ParticipantCount)
    } else {
        Ok(())
    }
}

fn hash_participant_set(
    participants: &[MachineJobParticipant],
) -> Result<Digest, MachineJobManifestError> {
    let mut hasher = participant_set_hasher(participants.len())?;
    for participant in participants.iter().copied() {
        hasher.update(&encode_participant(participant));
    }
    Ok(hasher.finalize().digest)
}

fn participant_set_hasher(count: usize) -> Result<ContentHasher, MachineJobManifestError> {
    validate_participant_count(count)?;
    let mut hasher = ContentHasher::new();
    hasher.update(&PARTICIPANT_SET_DOMAIN);
    hasher.update(
        &u32::try_from(count)
            .map_err(|_| MachineJobManifestError::ParticipantCount)?
            .to_le_bytes(),
    );
    Ok(hasher)
}

fn encode_header(
    global: MachineJobGlobalFacts,
    participant_set_digest: Digest,
    participant_count: usize,
) -> Result<[u8; MACHINE_JOB_MANIFEST_HEADER_BYTES], MachineJobManifestError> {
    let mut encoded = [0_u8; MACHINE_JOB_MANIFEST_HEADER_BYTES];
    encoded[0..8].copy_from_slice(&MANIFEST_MAGIC);
    encoded[8..10].copy_from_slice(&MANIFEST_VERSION.to_le_bytes());
    encoded[10] = global.network_policy as u8;
    // Byte 11 is reserved zero.
    encoded[12..16].copy_from_slice(
        &u32::try_from(participant_count)
            .map_err(|_| MachineJobManifestError::ParticipantCount)?
            .to_le_bytes(),
    );
    encoded[16..24].copy_from_slice(&global.global_timebase_hz.to_le_bytes());
    encoded[24..32].copy_from_slice(&global.duration_ticks.to_le_bytes());
    encoded[32..64].copy_from_slice(&global.source_digest.0);
    encoded[64..96].copy_from_slice(&global.compiler_digest.0);
    encoded[96..128].copy_from_slice(&global.interface_digest.0);
    encoded[128..160].copy_from_slice(&global.policy_digest.0);
    encoded[160..192].copy_from_slice(&global.machine_digest.0);
    encoded[192..224].copy_from_slice(&global.coordinate_epoch_digest.0);
    encoded[224..256].copy_from_slice(&global.safety_policy_digest.0);
    encoded[256..288].copy_from_slice(&global.synchronization_digest.0);
    encoded[288..320].copy_from_slice(&participant_set_digest.0);
    Ok(encoded)
}

fn encode_participant(participant: MachineJobParticipant) -> [u8; MACHINE_JOB_PARTICIPANT_BYTES] {
    let mut encoded = [0_u8; MACHINE_JOB_PARTICIPANT_BYTES];
    encoded[0..16].copy_from_slice(&participant.device_id.0);
    encoded[16..32].copy_from_slice(&participant.stream_id.0);
    encoded[32..64].copy_from_slice(&participant.board_package_digest.0);
    encoded[64..96].copy_from_slice(&participant.capability_digest.0);
    encoded[96..128].copy_from_slice(&participant.config_digest.0);
    encoded[128..160].copy_from_slice(&participant.partition_digest.0);
    encoded[160..192].copy_from_slice(&participant.partition_manifest_digest.0);
    encoded[192..224].copy_from_slice(&participant.terminal_block_digest.0);
    encoded[224..256].copy_from_slice(&participant.resource_set_digest.0);
    encoded[256..288].copy_from_slice(&participant.error_evidence_digest.0);
    encoded[288..320].copy_from_slice(&participant.safety_envelope_digest.0);
    encoded[320..328].copy_from_slice(&participant.partition_byte_len.to_le_bytes());
    encoded[328..332].copy_from_slice(&participant.block_count.to_le_bytes());
    encoded[332] = participant.axis_count;
    // Bytes 333..336 are reserved zero.
    encoded[336..344].copy_from_slice(&participant.local_timer_hz.to_le_bytes());
    encoded[344..352].copy_from_slice(&participant.first_tick.0.to_le_bytes());
    encoded[352..360].copy_from_slice(&participant.end_tick.0.to_le_bytes());
    for (axis, position) in participant.initial_position.into_iter().enumerate() {
        let offset = 360 + axis * 8;
        encoded[offset..offset + 8].copy_from_slice(&position.to_le_bytes());
    }
    for (axis, position) in participant.final_position.into_iter().enumerate() {
        let offset = 424 + axis * 8;
        encoded[offset..offset + 8].copy_from_slice(&position.to_le_bytes());
    }
    // Bytes 488..496 are reserved zero.
    encoded
}

fn decode_participant(encoded: &[u8]) -> Result<MachineJobParticipant, MachineJobManifestError> {
    if encoded.len() != MACHINE_JOB_PARTICIPANT_BYTES {
        return Err(MachineJobManifestError::WireLength {
            received: encoded.len(),
            expected: MACHINE_JOB_PARTICIPANT_BYTES,
        });
    }
    if encoded[333..336].iter().any(|byte| *byte != 0)
        || encoded[488..496].iter().any(|byte| *byte != 0)
    {
        return Err(MachineJobManifestError::Reserved);
    }
    let mut device_id = [0_u8; 16];
    device_id.copy_from_slice(&encoded[0..16]);
    let mut stream_id = [0_u8; 16];
    stream_id.copy_from_slice(&encoded[16..32]);
    let mut initial_position = [0_i64; MAX_EXECUTION_AXES];
    let mut final_position = [0_i64; MAX_EXECUTION_AXES];
    for axis in 0..MAX_EXECUTION_AXES {
        initial_position[axis] = read_i64(encoded, 360 + axis * 8);
        final_position[axis] = read_i64(encoded, 424 + axis * 8);
    }
    Ok(MachineJobParticipant {
        device_id: DeviceId(device_id),
        stream_id: StreamId(stream_id),
        board_package_digest: read_digest(encoded, 32),
        capability_digest: read_digest(encoded, 64),
        config_digest: read_digest(encoded, 96),
        partition_digest: read_digest(encoded, 128),
        partition_manifest_digest: read_digest(encoded, 160),
        terminal_block_digest: read_digest(encoded, 192),
        resource_set_digest: read_digest(encoded, 224),
        error_evidence_digest: read_digest(encoded, 256),
        safety_envelope_digest: read_digest(encoded, 288),
        partition_byte_len: read_u64(encoded, 320),
        block_count: read_u32(encoded, 328),
        axis_count: encoded[332],
        local_timer_hz: read_u64(encoded, 336),
        first_tick: StreamTick(read_u64(encoded, 344)),
        end_tick: StreamTick(read_u64(encoded, 352)),
        initial_position,
        final_position,
    })
}

fn read_digest(encoded: &[u8], offset: usize) -> Digest {
    let mut digest = [0_u8; 32];
    digest.copy_from_slice(&encoded[offset..offset + 32]);
    Digest(digest)
}

const fn bytes_all_zero(bytes: &[u8]) -> bool {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != 0 {
            return false;
        }
        index += 1;
    }
    true
}

const fn read_u16(encoded: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([encoded[offset], encoded[offset + 1]])
}

const fn read_u32(encoded: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        encoded[offset],
        encoded[offset + 1],
        encoded[offset + 2],
        encoded[offset + 3],
    ])
}

const fn read_u64(encoded: &[u8], offset: usize) -> u64 {
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

fn read_i64(encoded: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(
        encoded[offset..offset + 8]
            .try_into()
            .expect("fixed participant position field"),
    )
}

/// Canonical manifest construction or decode failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineJobManifestError {
    /// Complete byte length did not match the participant count.
    WireLength {
        /// Received bytes.
        received: usize,
        /// Required bytes.
        expected: usize,
    },
    /// Manifest magic did not match schema V1.
    Magic,
    /// Manifest schema version did not match exactly.
    Version {
        /// Received version.
        received: u16,
    },
    /// Network policy discriminant was unassigned.
    NetworkPolicy,
    /// A reserved byte was nonzero.
    Reserved,
    /// Participant count was zero, too large, or unrepresentable.
    ParticipantCount,
    /// Global timebase or duration was zero.
    GlobalTimebase,
    /// A required global source/compiler/policy/machine identity was zero.
    GlobalIdentity,
    /// One stable device identity was zero.
    DeviceIdentity {
        /// Sorted participant index.
        index: usize,
    },
    /// One local stream identity was zero.
    StreamIdentity {
        /// Sorted participant index.
        index: usize,
    },
    /// A required board/config/partition/resource/evidence identity was zero.
    ParticipantIdentity {
        /// Sorted participant index.
        index: usize,
    },
    /// Device records were not strictly sorted and unique.
    ParticipantOrder {
        /// First offending participant index.
        index: usize,
    },
    /// Two participants reused one local execution stream identity.
    DuplicateStream {
        /// Second participant using the identity.
        index: usize,
    },
    /// Header participant-set digest did not match canonical ordered records.
    ParticipantSetDigest,
    /// Local axis width was zero or exceeded machine-IR V1.
    AxisCount {
        /// Participant index.
        index: usize,
        /// Received width.
        received: u8,
    },
    /// Partition bytes and block count did not agree exactly.
    PartitionLayout {
        /// Participant index.
        index: usize,
    },
    /// Local timer, first tick, or terminal tick was invalid.
    LocalTimebase {
        /// Participant index.
        index: usize,
    },
    /// Local duration was not exactly the declared global duration.
    DurationMismatch {
        /// Participant index.
        index: usize,
    },
    /// Position slots above the participant's axis width were nonzero.
    PositionPadding {
        /// Participant index.
        index: usize,
    },
    /// Checked byte/count arithmetic overflowed.
    Arithmetic,
}

#[cfg(test)]
mod tests {
    use alumina_clock::BootId;
    use alumina_protocol::DeviceCycle;
    use alumina_storage::{ContentId, ObjectKind, StoredObject};

    use crate::{JobCommitId, JobCommitRequest, PreparedJobToken};

    use super::*;

    fn digest(value: u8) -> Digest {
        Digest([value; 32])
    }

    fn global() -> MachineJobGlobalFacts {
        MachineJobGlobalFacts {
            network_policy: JobNetworkPolicy::NetworkAttended,
            global_timebase_hz: 1_000_000,
            duration_ticks: 2_000_000,
            source_digest: digest(1),
            compiler_digest: digest(2),
            interface_digest: digest(3),
            policy_digest: digest(4),
            machine_digest: digest(5),
            coordinate_epoch_digest: digest(6),
            safety_policy_digest: digest(7),
            synchronization_digest: digest(8),
        }
    }

    fn participant(device: u8, stream: u8, local_timer_hz: u64) -> MachineJobParticipant {
        let mut initial_position = [0_i64; MAX_EXECUTION_AXES];
        initial_position[0] = i64::from(device) * 10;
        initial_position[1] = -i64::from(device);
        let mut final_position = initial_position;
        final_position[0] += 320;
        MachineJobParticipant {
            device_id: DeviceId([device; 16]),
            stream_id: StreamId::new([stream; 16]).unwrap(),
            board_package_digest: digest(device.wrapping_add(10)),
            capability_digest: digest(device.wrapping_add(20)),
            config_digest: digest(device.wrapping_add(30)),
            partition_digest: digest(device.wrapping_add(40)),
            partition_manifest_digest: digest(device.wrapping_add(50)),
            terminal_block_digest: digest(device.wrapping_add(60)),
            resource_set_digest: digest(device.wrapping_add(70)),
            error_evidence_digest: digest(device.wrapping_add(80)),
            safety_envelope_digest: digest(device.wrapping_add(90)),
            partition_byte_len: 1_024,
            block_count: 2,
            axis_count: 2,
            local_timer_hz,
            first_tick: StreamTick(0),
            end_tick: StreamTick(local_timer_hz * 2),
            initial_position,
            final_position,
        }
    }

    #[test]
    fn two_participant_manifest_round_trips_and_binds_schedule() {
        let participants = [participant(1, 11, 1_000_000), participant(2, 12, 2_000_000)];
        let manifest = MachineJobManifest::new(global(), &participants).unwrap();
        let mut encoded =
            [0_u8; MACHINE_JOB_MANIFEST_HEADER_BYTES + 2 * MACHINE_JOB_PARTICIPANT_BYTES];
        assert_eq!(manifest.encode_into(&mut encoded), Ok(encoded.len()));
        let decoded = DecodedMachineJobManifest::decode(&encoded).unwrap();

        assert_eq!(decoded.global(), global());
        assert_eq!(decoded.participant_count(), 2);
        assert_eq!(decoded.participant(0), Ok(Some(participants[0])));
        assert_eq!(decoded.participant(1), Ok(Some(participants[1])));
        assert_eq!(decoded.participant(2), Ok(None));
        assert_eq!(
            decoded.participant_set_digest(),
            manifest.participant_set_digest()
        );
        assert_eq!(
            decoded.global_job_digest(),
            manifest.global_job_digest().unwrap()
        );
        assert_eq!(decoded.global_job_digest(), sha256(&encoded).digest);

        let stored = StoredObject {
            kind: ObjectKind::MachineJobManifest,
            content: ContentId::from_sha256(decoded.global_job_digest()),
            byte_len: u64::try_from(encoded.len()).unwrap(),
        };
        assert!(stored.content.is_valid());

        let commit = JobCommitRequest {
            policy: global().network_policy,
            prepare_id: 9,
            boot_id: BootId::new([0x44; 16]).unwrap(),
            global_job_digest: decoded.global_job_digest(),
            participant_set_digest: decoded.participant_set_digest(),
            prepared_token: PreparedJobToken(digest(0xaa)),
            partition_digest: participants[0].partition_digest,
            local_start_cycle: DeviceCycle(10_000),
            confirm_deadline_cycle: DeviceCycle(8_000),
            abort_guard_cycle: DeviceCycle(9_000),
            lease_expiry_cycle: DeviceCycle(2_020_000),
            clock_probe_id: 77,
            clock_uncertainty_cycles: 20,
            required_sync_tolerance_cycles: 50,
            commit_id: JobCommitId::new([0x55; 16]).unwrap(),
        };
        assert_eq!(
            JobCommitRequest::decode(&commit.encode().unwrap()),
            Ok(commit)
        );
    }

    #[test]
    fn ordering_duration_and_padding_fail_closed() {
        let reversed = [participant(2, 12, 1_000_000), participant(1, 11, 1_000_000)];
        assert_eq!(
            MachineJobManifest::new(global(), &reversed),
            Err(MachineJobManifestError::ParticipantOrder { index: 1 })
        );

        let mut bad_duration = participant(1, 11, 1_000_000);
        bad_duration.end_tick = StreamTick(1_999_999);
        assert_eq!(
            MachineJobManifest::new(global(), &[bad_duration]),
            Err(MachineJobManifestError::DurationMismatch { index: 0 })
        );

        let mut bad_padding = participant(1, 11, 1_000_000);
        bad_padding.final_position[7] = 1;
        assert_eq!(
            MachineJobManifest::new(global(), &[bad_padding]),
            Err(MachineJobManifestError::PositionPadding { index: 0 })
        );
    }

    #[test]
    fn participant_tamper_and_noncanonical_reserved_bytes_are_rejected() {
        let participants = [participant(1, 11, 1_000_000)];
        let manifest = MachineJobManifest::new(global(), &participants).unwrap();
        let mut encoded = [0_u8; MACHINE_JOB_MANIFEST_HEADER_BYTES + MACHINE_JOB_PARTICIPANT_BYTES];
        manifest.encode_into(&mut encoded).unwrap();

        let mut tampered = encoded;
        tampered[MACHINE_JOB_MANIFEST_HEADER_BYTES + 128] ^= 1;
        assert_eq!(
            DecodedMachineJobManifest::decode(&tampered),
            Err(MachineJobManifestError::ParticipantSetDigest)
        );

        let mut reserved = encoded;
        reserved[MACHINE_JOB_MANIFEST_HEADER_BYTES + 495] = 1;
        assert_eq!(
            DecodedMachineJobManifest::decode(&reserved),
            Err(MachineJobManifestError::Reserved)
        );
    }

    #[test]
    fn duplicate_stream_and_wrong_partition_layout_are_rejected() {
        let mut second = participant(2, 11, 1_000_000);
        let first = participant(1, 11, 1_000_000);
        assert_eq!(
            MachineJobManifest::new(global(), &[first, second]),
            Err(MachineJobManifestError::DuplicateStream { index: 1 })
        );

        second.stream_id = StreamId::new([12; 16]).unwrap();
        second.partition_byte_len = 513;
        assert_eq!(
            MachineJobManifest::new(global(), &[first, second]),
            Err(MachineJobManifestError::PartitionLayout { index: 1 })
        );
    }
}
