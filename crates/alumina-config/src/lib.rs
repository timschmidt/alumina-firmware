#![no_std]
#![doc = "Canonical streaming machine configuration and board-resource validation for Alumina."]

use core::cmp::Ordering;

use alumina_board::{
    BoardPackage, BusKind, ElectricalConstraintKind, OwnerDomain, ResourceDescriptor, ResourceId,
    SafeValue,
};
use alumina_capability::{
    CapabilityError, decode_resource_id, encode_resource_id, verify_declared_identity,
};
use alumina_protocol::Digest;
use alumina_storage::media::{AsyncBlockDevice, MAX_MEDIA_CHUNK_BYTES, PublishedReader};
use alumina_storage::provisioning::{ProvisionedCache, ProvisionedCacheError};
use alumina_storage::{ContentId, ObjectKind, PublishedObject, StoredObject};
use sha2::{Digest as ShaDigest, Sha256};

/// Exact machine-configuration schema version.
pub const CONFIGURATION_VERSION: u16 = 1;
/// Bytes in the fixed canonical document header.
pub const CONFIGURATION_HEADER_BYTES: usize = 80;
/// Bytes in every V1 configuration record.
pub const CONFIGURATION_RECORD_BYTES: usize = 64;
/// Schema-wide bound independent of a board's smaller admission budget.
pub const MAX_CONFIGURATION_RECORDS: usize = 256;
/// Initial maximum logical motion-axis index plus one.
pub const MAX_AXIS_INSTANCES: usize = 16;

const DOCUMENT_MAGIC: [u8; 8] = *b"ALMCFG01";
const RECORD_KIND_BINDING: u16 = 1;
const RECORD_KIND_SCALAR: u16 = 2;
const PUBLICATION_MAGIC: [u8; 8] = *b"ALMCFQ01";
const SELECTION_MAGIC: [u8; 8] = *b"ALMCFS01";

/// Exact `ConfigurationValidate` body bytes.
pub const CONFIGURATION_PUBLICATION_BYTES: usize = 96;
/// Exact `ConfigurationCommit`/`ConfigurationRollback` selection bytes.
pub const CONFIGURATION_SELECTION_BYTES: usize = 64;

/// One published inert configuration selected for independent validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationPublication {
    /// Nonzero boot-local transaction joining service/core reports and retries.
    pub transaction_id: u64,
    /// Exact typed object and canonical chunk manifest already published on cache media.
    pub publication: PublishedObject,
}

impl ConfigurationPublication {
    /// Encodes the fixed canonical validation request.
    pub fn encode(
        self,
    ) -> Result<[u8; CONFIGURATION_PUBLICATION_BYTES], ConfigurationRequestError> {
        self.validate()?;
        let mut encoded = [0_u8; CONFIGURATION_PUBLICATION_BYTES];
        encoded[0..8].copy_from_slice(&PUBLICATION_MAGIC);
        encoded[8..10].copy_from_slice(&CONFIGURATION_VERSION.to_le_bytes());
        // Bytes 10..16 and 60..64 are reserved zero.
        encoded[16..24].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[24..56].copy_from_slice(&self.publication.object.content.digest.0);
        encoded[56..60].copy_from_slice(
            &u32::try_from(self.publication.object.byte_len)
                .map_err(|_| ConfigurationRequestError::Length)?
                .to_le_bytes(),
        );
        encoded[64..96].copy_from_slice(&self.publication.manifest.digest.0);
        Ok(encoded)
    }

    /// Decodes only the exact V1 SHA-256/configuration representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, ConfigurationRequestError> {
        if encoded.len() != CONFIGURATION_PUBLICATION_BYTES {
            return Err(ConfigurationRequestError::Length);
        }
        if encoded[0..8] != PUBLICATION_MAGIC {
            return Err(ConfigurationRequestError::Magic);
        }
        if read_u16(encoded, 8) != CONFIGURATION_VERSION {
            return Err(ConfigurationRequestError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0)
            || encoded[60..64].iter().any(|byte| *byte != 0)
        {
            return Err(ConfigurationRequestError::Reserved);
        }
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[24..56]);
        let mut manifest = [0_u8; 32];
        manifest.copy_from_slice(&encoded[64..96]);
        let request = Self {
            transaction_id: read_u64(encoded, 16),
            publication: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineConfiguration,
                    content: ContentId::from_sha256(Digest(digest)),
                    byte_len: u64::from(read_u32(encoded, 56)),
                },
                manifest: ContentId::from_sha256(Digest(manifest)),
            },
        };
        request.validate()?;
        if request.encode()? != encoded {
            return Err(ConfigurationRequestError::Noncanonical);
        }
        Ok(request)
    }

    /// Exact configuration content identity used by both cores and all jobs.
    pub const fn digest(self) -> Digest {
        self.publication.object.content.digest
    }

    /// Exact document byte count after request validation.
    pub fn byte_len(self) -> Result<u32, ConfigurationRequestError> {
        u32::try_from(self.publication.object.byte_len)
            .map_err(|_| ConfigurationRequestError::Length)
    }

    fn validate(self) -> Result<(), ConfigurationRequestError> {
        let bytes = self.byte_len()?;
        if self.transaction_id == 0
            || self.publication.object.kind != ObjectKind::MachineConfiguration
            || !self.publication.object.content.is_valid()
            || !self.publication.manifest.is_valid()
            || !configuration_length_valid(bytes)
        {
            return Err(ConfigurationRequestError::Identity);
        }
        Ok(())
    }
}

/// Exact candidate/active identity selected for commit or clearing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationSelection {
    pub transaction_id: u64,
    pub digest: Digest,
    pub byte_len: u32,
}

impl ConfigurationSelection {
    /// Selects the exact identity from a prior validation request.
    pub fn from_publication(
        publication: ConfigurationPublication,
    ) -> Result<Self, ConfigurationRequestError> {
        publication.validate()?;
        Ok(Self {
            transaction_id: publication.transaction_id,
            digest: publication.digest(),
            byte_len: publication.byte_len()?,
        })
    }

    pub fn encode(self) -> Result<[u8; CONFIGURATION_SELECTION_BYTES], ConfigurationRequestError> {
        self.validate()?;
        let mut encoded = [0_u8; CONFIGURATION_SELECTION_BYTES];
        encoded[0..8].copy_from_slice(&SELECTION_MAGIC);
        encoded[8..10].copy_from_slice(&CONFIGURATION_VERSION.to_le_bytes());
        // Bytes 10..16 and 60..64 are reserved zero.
        encoded[16..24].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[24..56].copy_from_slice(&self.digest.0);
        encoded[56..60].copy_from_slice(&self.byte_len.to_le_bytes());
        Ok(encoded)
    }

    pub fn decode(encoded: &[u8]) -> Result<Self, ConfigurationRequestError> {
        if encoded.len() != CONFIGURATION_SELECTION_BYTES {
            return Err(ConfigurationRequestError::Length);
        }
        if encoded[0..8] != SELECTION_MAGIC {
            return Err(ConfigurationRequestError::Magic);
        }
        if read_u16(encoded, 8) != CONFIGURATION_VERSION {
            return Err(ConfigurationRequestError::Version);
        }
        if encoded[10..16].iter().any(|byte| *byte != 0)
            || encoded[60..64].iter().any(|byte| *byte != 0)
        {
            return Err(ConfigurationRequestError::Reserved);
        }
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[24..56]);
        let selection = Self {
            transaction_id: read_u64(encoded, 16),
            digest: Digest(digest),
            byte_len: read_u32(encoded, 56),
        };
        selection.validate()?;
        if selection.encode()? != encoded {
            return Err(ConfigurationRequestError::Noncanonical);
        }
        Ok(selection)
    }

    fn validate(self) -> Result<(), ConfigurationRequestError> {
        if self.transaction_id == 0
            || self.digest.is_zero()
            || !configuration_length_valid(self.byte_len)
        {
            return Err(ConfigurationRequestError::Identity);
        }
        Ok(())
    }
}

/// Canonical external configuration request rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationRequestError {
    Length,
    Magic,
    Version,
    Reserved,
    Identity,
    Noncanonical,
}

/// Machine-level policy bits bound into the configuration identity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct ConfigurationFlags(pub u32);

impl ConfigurationFlags {
    /// Configuration may be used to construct motion resources.
    pub const MOTION: u32 = 1 << 0;
    /// A cached job may continue without a browser when all later safety gates pass.
    pub const CACHED_AUTONOMOUS: u32 = 1 << 1;
    /// Configuration contains field-oriented motor-control resources.
    pub const FIELD_ORIENTED_CONTROL: u32 = 1 << 2;
    /// Configuration contains non-motion laboratory/control resources.
    pub const LAB_CONTROL: u32 = 1 << 3;
    /// All V1 flags.
    pub const ALLOWED: u32 =
        Self::MOTION | Self::CACHED_AUTONOMOUS | Self::FIELD_ORIENTED_CONTROL | Self::LAB_CONTROL;

    /// Whether every requested flag is set.
    pub const fn contains(self, flags: u32) -> bool {
        self.0 & flags == flags
    }
}

/// Fixed document metadata preceding all typed records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationHeader {
    /// Exact immutable board capability required by these records.
    pub capability_digest: Digest,
    /// Total record count.
    pub record_count: u16,
    /// Derived count of records independently consumed by core 1.
    pub realtime_record_count: u16,
    /// Machine-level policy flags.
    pub flags: ConfigurationFlags,
}

impl ConfigurationHeader {
    /// Complete canonical byte length implied by the record count.
    pub fn total_bytes(self) -> Result<u32, ConfigurationError> {
        let records = u32::from(self.record_count)
            .checked_mul(
                u32::try_from(CONFIGURATION_RECORD_BYTES)
                    .map_err(|_| ConfigurationError::Length)?,
            )
            .ok_or(ConfigurationError::Length)?;
        u32::try_from(CONFIGURATION_HEADER_BYTES)
            .map_err(|_| ConfigurationError::Length)?
            .checked_add(records)
            .ok_or(ConfigurationError::Length)
    }

    /// Encodes the exact V1 header.
    pub fn encode(self) -> Result<[u8; CONFIGURATION_HEADER_BYTES], ConfigurationError> {
        self.validate()?;
        let mut encoded = [0_u8; CONFIGURATION_HEADER_BYTES];
        encoded[0..8].copy_from_slice(&DOCUMENT_MAGIC);
        encoded[8..10].copy_from_slice(&CONFIGURATION_VERSION.to_le_bytes());
        encoded[10..12].copy_from_slice(
            &u16::try_from(CONFIGURATION_HEADER_BYTES)
                .map_err(|_| ConfigurationError::Length)?
                .to_le_bytes(),
        );
        encoded[12..16].copy_from_slice(&self.total_bytes()?.to_le_bytes());
        encoded[16..48].copy_from_slice(&self.capability_digest.0);
        encoded[48..50].copy_from_slice(&self.record_count.to_le_bytes());
        encoded[50..52].copy_from_slice(&self.realtime_record_count.to_le_bytes());
        encoded[52..56].copy_from_slice(&self.flags.0.to_le_bytes());
        // Bytes 56..80 are reserved zero.
        Ok(encoded)
    }

    /// Decodes only the exact canonical V1 header.
    pub fn decode(encoded: &[u8]) -> Result<Self, ConfigurationError> {
        if encoded.len() != CONFIGURATION_HEADER_BYTES {
            return Err(ConfigurationError::Length);
        }
        if encoded[0..8] != DOCUMENT_MAGIC {
            return Err(ConfigurationError::Magic);
        }
        if read_u16(encoded, 8) != CONFIGURATION_VERSION {
            return Err(ConfigurationError::Version);
        }
        if usize::from(read_u16(encoded, 10)) != CONFIGURATION_HEADER_BYTES {
            return Err(ConfigurationError::Length);
        }
        if encoded[56..80].iter().any(|byte| *byte != 0) {
            return Err(ConfigurationError::Reserved);
        }
        let mut capability_digest = [0_u8; 32];
        capability_digest.copy_from_slice(&encoded[16..48]);
        let header = Self {
            capability_digest: Digest(capability_digest),
            record_count: read_u16(encoded, 48),
            realtime_record_count: read_u16(encoded, 50),
            flags: ConfigurationFlags(read_u32(encoded, 52)),
        };
        header.validate()?;
        if read_u32(encoded, 12) != header.total_bytes()? || header.encode()? != encoded {
            return Err(ConfigurationError::Noncanonical);
        }
        Ok(header)
    }

    fn validate(self) -> Result<(), ConfigurationError> {
        if self.capability_digest.is_zero() {
            return Err(ConfigurationError::CapabilityIdentity);
        }
        if self.record_count == 0
            || usize::from(self.record_count) > MAX_CONFIGURATION_RECORDS
            || self.realtime_record_count > self.record_count
        {
            return Err(ConfigurationError::RecordCount);
        }
        if self.flags.0 & !ConfigurationFlags::ALLOWED != 0
            || self.flags.contains(ConfigurationFlags::CACHED_AUTONOMOUS)
                && !self.flags.contains(ConfigurationFlags::MOTION)
            || self
                .flags
                .contains(ConfigurationFlags::FIELD_ORIENTED_CONTROL)
                && !self.flags.contains(ConfigurationFlags::MOTION)
        {
            return Err(ConfigurationError::Flags);
        }
        Ok(())
    }
}

/// Logical use of one typed board resource.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum BindingRole {
    AxisStep = 1,
    AxisDirection = 2,
    AxisEnable = 3,
    AxisLimitMinimum = 4,
    AxisLimitMaximum = 5,
    AxisEncoderA = 6,
    AxisEncoderB = 7,
    AxisEncoderIndex = 8,
    AxisMotorFault = 9,
    Probe = 10,
    EmergencyStop = 11,
    SafetyInterlock = 12,
    DigitalInput = 13,
    DigitalOutput = 14,
    AnalogInput = 15,
    PwmOutput = 16,
    SerialPort = 17,
    Timer = 18,
    Counter = 19,
    FocPhaseU = 20,
    FocPhaseV = 21,
    FocPhaseW = 22,
    FocCurrentA = 23,
    FocCurrentB = 24,
    FocCurrentC = 25,
    FocBusVoltage = 26,
    FocEncoder = 27,
    FocEnable = 28,
    FocFault = 29,
    ProcessOutput = 30,
    Storage = 31,
    I2cBus = 32,
    SpiBus = 33,
    TwaiBus = 34,
    CaptureInput = 35,
    WaveformOutput = 36,
    FittedDevice = 37,
}

impl BindingRole {
    const fn from_wire(value: u16) -> Option<Self> {
        Some(match value {
            1 => Self::AxisStep,
            2 => Self::AxisDirection,
            3 => Self::AxisEnable,
            4 => Self::AxisLimitMinimum,
            5 => Self::AxisLimitMaximum,
            6 => Self::AxisEncoderA,
            7 => Self::AxisEncoderB,
            8 => Self::AxisEncoderIndex,
            9 => Self::AxisMotorFault,
            10 => Self::Probe,
            11 => Self::EmergencyStop,
            12 => Self::SafetyInterlock,
            13 => Self::DigitalInput,
            14 => Self::DigitalOutput,
            15 => Self::AnalogInput,
            16 => Self::PwmOutput,
            17 => Self::SerialPort,
            18 => Self::Timer,
            19 => Self::Counter,
            20 => Self::FocPhaseU,
            21 => Self::FocPhaseV,
            22 => Self::FocPhaseW,
            23 => Self::FocCurrentA,
            24 => Self::FocCurrentB,
            25 => Self::FocCurrentC,
            26 => Self::FocBusVoltage,
            27 => Self::FocEncoder,
            28 => Self::FocEnable,
            29 => Self::FocFault,
            30 => Self::ProcessOutput,
            31 => Self::Storage,
            32 => Self::I2cBus,
            33 => Self::SpiBus,
            34 => Self::TwaiBus,
            35 => Self::CaptureInput,
            36 => Self::WaveformOutput,
            37 => Self::FittedDevice,
            _ => return None,
        })
    }

    const fn is_input(self) -> bool {
        matches!(
            self,
            Self::AxisLimitMinimum
                | Self::AxisLimitMaximum
                | Self::AxisEncoderA
                | Self::AxisEncoderB
                | Self::AxisEncoderIndex
                | Self::AxisMotorFault
                | Self::Probe
                | Self::EmergencyStop
                | Self::SafetyInterlock
                | Self::DigitalInput
                | Self::AnalogInput
                | Self::FocCurrentA
                | Self::FocCurrentB
                | Self::FocCurrentC
                | Self::FocBusVoltage
                | Self::FocEncoder
                | Self::FocFault
                | Self::CaptureInput
        )
    }

    const fn is_output(self) -> bool {
        matches!(
            self,
            Self::AxisStep
                | Self::AxisDirection
                | Self::AxisEnable
                | Self::DigitalOutput
                | Self::PwmOutput
                | Self::FocPhaseU
                | Self::FocPhaseV
                | Self::FocPhaseW
                | Self::FocEnable
                | Self::ProcessOutput
                | Self::WaveformOutput
        )
    }

    const fn is_pwm(self) -> bool {
        matches!(
            self,
            Self::PwmOutput
                | Self::FocPhaseU
                | Self::FocPhaseV
                | Self::FocPhaseW
                | Self::WaveformOutput
        )
    }

    const fn is_analog_input(self) -> bool {
        matches!(
            self,
            Self::AnalogInput
                | Self::FocCurrentA
                | Self::FocCurrentB
                | Self::FocCurrentC
                | Self::FocBusVoltage
        )
    }

    const fn is_sampled_input(self) -> bool {
        self.is_analog_input()
            || matches!(
                self,
                Self::AxisEncoderA
                    | Self::AxisEncoderB
                    | Self::AxisEncoderIndex
                    | Self::FocEncoder
                    | Self::CaptureInput
            )
    }

    const fn is_hazardous_role(self) -> bool {
        matches!(
            self,
            Self::AxisStep
                | Self::AxisDirection
                | Self::AxisEnable
                | Self::FocPhaseU
                | Self::FocPhaseV
                | Self::FocPhaseW
                | Self::FocEnable
                | Self::ProcessOutput
        )
    }

    const fn axis_role(self) -> bool {
        matches!(
            self,
            Self::AxisStep
                | Self::AxisDirection
                | Self::AxisEnable
                | Self::AxisLimitMinimum
                | Self::AxisLimitMaximum
                | Self::AxisEncoderA
                | Self::AxisEncoderB
                | Self::AxisEncoderIndex
                | Self::AxisMotorFault
                | Self::FocPhaseU
                | Self::FocPhaseV
                | Self::FocPhaseW
                | Self::FocCurrentA
                | Self::FocCurrentB
                | Self::FocCurrentC
                | Self::FocBusVoltage
                | Self::FocEncoder
                | Self::FocEnable
                | Self::FocFault
        )
    }
}

/// Explicit active electrical polarity for a signal-like binding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum SignalPolarity {
    /// No polarity applies to a controller, bus, storage, or fitted device.
    NotApplicable = 0,
    ActiveHigh = 1,
    ActiveLow = 2,
}

impl SignalPolarity {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::NotApplicable),
            1 => Some(Self::ActiveHigh),
            2 => Some(Self::ActiveLow),
            _ => None,
        }
    }
}

/// Electrical and admission modifiers for one binding.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct BindingFlags(pub u16);

impl BindingFlags {
    pub const PULL_UP: u16 = 1 << 0;
    pub const PULL_DOWN: u16 = 1 << 1;
    pub const OPEN_DRAIN: u16 = 1 << 2;
    pub const REQUIRED_INTERLOCK: u16 = 1 << 3;
    pub const ALLOWED: u16 =
        Self::PULL_UP | Self::PULL_DOWN | Self::OPEN_DRAIN | Self::REQUIRED_INTERLOCK;
}

/// One logical function bound to exactly one immutable board resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceBinding {
    pub instance: u16,
    pub role: BindingRole,
    pub resource: ResourceId,
    pub owner: OwnerDomain,
    pub polarity: SignalPolarity,
    pub flags: BindingFlags,
    /// Minimum asserted/output-active duration in device cycles, or debounce high time.
    pub minimum_active_cycles: u32,
    /// Minimum inactive duration in device cycles, or debounce low time.
    pub minimum_inactive_cycles: u32,
    /// Requested maximum event/carrier/bus frequency; zero for static functions.
    pub maximum_frequency_hz: u32,
    /// Local fail-safe watchdog bound in cycles; zero only where no driven energy exists.
    pub watchdog_cycles: u32,
}

/// Typed exact fact whose physical dimension is fixed by its enum value.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum ScalarFact {
    AxisFullStepsPerTurn = 1,
    AxisMicrosteps = 2,
    AxisMotorTurnsPerOutputTurn = 3,
    AxisTravelMetresPerOutputTurn = 4,
    AxisCalibrationScale = 5,
    AxisPositionMinimumMetres = 6,
    AxisPositionMaximumMetres = 7,
    AxisVelocityLimitMetresPerSecond = 8,
    AxisAccelerationLimitMetresPerSecondSquared = 9,
    AxisJerkLimitMetresPerSecondCubed = 10,
    AxisFollowingErrorMetres = 11,
    AxisEncoderCountsPerTurn = 12,
    MotorPolePairs = 13,
    MotorCurrentLimitAmperes = 14,
    MotorVoltageLimitVolts = 15,
    PwmCarrierHertz = 16,
    PwmDeadTimeSeconds = 17,
    ControlRateHertz = 18,
    CurrentSenseOhms = 19,
    CurrentSenseVoltsPerAmpere = 20,
    SafetyMaximumReactionSeconds = 21,
    ProcessMaximumDurationSeconds = 22,
    TimerTickHertz = 23,
}

impl ScalarFact {
    const fn from_wire(value: u16) -> Option<Self> {
        Some(match value {
            1 => Self::AxisFullStepsPerTurn,
            2 => Self::AxisMicrosteps,
            3 => Self::AxisMotorTurnsPerOutputTurn,
            4 => Self::AxisTravelMetresPerOutputTurn,
            5 => Self::AxisCalibrationScale,
            6 => Self::AxisPositionMinimumMetres,
            7 => Self::AxisPositionMaximumMetres,
            8 => Self::AxisVelocityLimitMetresPerSecond,
            9 => Self::AxisAccelerationLimitMetresPerSecondSquared,
            10 => Self::AxisJerkLimitMetresPerSecondCubed,
            11 => Self::AxisFollowingErrorMetres,
            12 => Self::AxisEncoderCountsPerTurn,
            13 => Self::MotorPolePairs,
            14 => Self::MotorCurrentLimitAmperes,
            15 => Self::MotorVoltageLimitVolts,
            16 => Self::PwmCarrierHertz,
            17 => Self::PwmDeadTimeSeconds,
            18 => Self::ControlRateHertz,
            19 => Self::CurrentSenseOhms,
            20 => Self::CurrentSenseVoltsPerAmpere,
            21 => Self::SafetyMaximumReactionSeconds,
            22 => Self::ProcessMaximumDurationSeconds,
            23 => Self::TimerTickHertz,
            _ => return None,
        })
    }

    const fn requires_positive(self) -> bool {
        !matches!(
            self,
            Self::AxisPositionMinimumMetres | Self::AxisPositionMaximumMetres
        )
    }

    const fn requires_integer(self) -> bool {
        matches!(
            self,
            Self::AxisFullStepsPerTurn
                | Self::AxisMicrosteps
                | Self::AxisEncoderCountsPerTurn
                | Self::MotorPolePairs
        )
    }

    const fn axis_fact(self) -> bool {
        matches!(
            self,
            Self::AxisFullStepsPerTurn
                | Self::AxisMicrosteps
                | Self::AxisMotorTurnsPerOutputTurn
                | Self::AxisTravelMetresPerOutputTurn
                | Self::AxisCalibrationScale
                | Self::AxisPositionMinimumMetres
                | Self::AxisPositionMaximumMetres
                | Self::AxisVelocityLimitMetresPerSecond
                | Self::AxisAccelerationLimitMetresPerSecondSquared
                | Self::AxisJerkLimitMetresPerSecondCubed
                | Self::AxisFollowingErrorMetres
                | Self::AxisEncoderCountsPerTurn
                | Self::MotorPolePairs
                | Self::MotorCurrentLimitAmperes
                | Self::MotorVoltageLimitVolts
                | Self::PwmCarrierHertz
                | Self::PwmDeadTimeSeconds
                | Self::ControlRateHertz
                | Self::CurrentSenseOhms
                | Self::CurrentSenseVoltsPerAmpere
        )
    }
}

/// Reduced exact signed rational. Zero is represented only as `0/1`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Rational {
    pub numerator: i64,
    pub denominator: u64,
}

impl Rational {
    /// Constructs only a reduced positive-denominator representation.
    pub fn new(numerator: i64, denominator: u64) -> Result<Self, ConfigurationError> {
        let value = Self {
            numerator,
            denominator,
        };
        value.validate()?;
        Ok(value)
    }

    fn validate(self) -> Result<(), ConfigurationError> {
        if self.denominator == 0
            || self.numerator == 0 && self.denominator != 1
            || gcd(self.numerator.unsigned_abs(), self.denominator) != 1
        {
            return Err(ConfigurationError::Rational);
        }
        Ok(())
    }

    fn exact_cmp(self, other: Self) -> Ordering {
        (i128::from(self.numerator) * i128::from(other.denominator))
            .cmp(&(i128::from(other.numerator) * i128::from(self.denominator)))
    }
}

/// Evidence origin for a machine fact.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum FactEvidence {
    Declared = 1,
    Measured = 2,
    Qualified = 3,
}

impl FactEvidence {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Declared),
            2 => Some(Self::Measured),
            3 => Some(Self::Qualified),
            _ => None,
        }
    }
}

/// Exact nominal value plus a nonnegative exact absolute uncertainty.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExactScalar {
    pub instance: u16,
    pub fact: ScalarFact,
    pub value: Rational,
    pub uncertainty: Rational,
    pub evidence: FactEvidence,
}

/// One fixed-width canonical configuration record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationRecord {
    Binding(ResourceBinding),
    Scalar(ExactScalar),
}

impl ConfigurationRecord {
    /// Encodes a fixed 64-byte canonical record.
    pub fn encode(self) -> Result<[u8; CONFIGURATION_RECORD_BYTES], ConfigurationError> {
        self.validate_shape()?;
        let mut encoded = [0_u8; CONFIGURATION_RECORD_BYTES];
        let (kind, instance, selector) = self.key();
        encoded[0..2].copy_from_slice(&kind.to_le_bytes());
        encoded[2..4].copy_from_slice(
            &u16::try_from(CONFIGURATION_RECORD_BYTES)
                .map_err(|_| ConfigurationError::Length)?
                .to_le_bytes(),
        );
        encoded[4..6].copy_from_slice(&instance.to_le_bytes());
        encoded[6..8].copy_from_slice(&selector.to_le_bytes());
        match self {
            Self::Binding(binding) => {
                encoded[8..12].copy_from_slice(&encode_resource_id(binding.resource));
                encoded[12] = owner_wire(binding.owner);
                encoded[13] = binding.polarity as u8;
                encoded[14..16].copy_from_slice(&binding.flags.0.to_le_bytes());
                encoded[16..20].copy_from_slice(&binding.minimum_active_cycles.to_le_bytes());
                encoded[20..24].copy_from_slice(&binding.minimum_inactive_cycles.to_le_bytes());
                encoded[24..28].copy_from_slice(&binding.maximum_frequency_hz.to_le_bytes());
                encoded[28..32].copy_from_slice(&binding.watchdog_cycles.to_le_bytes());
                // Bytes 32..64 are reserved zero.
            }
            Self::Scalar(scalar) => {
                encoded[8..16].copy_from_slice(&scalar.value.numerator.to_le_bytes());
                encoded[16..24].copy_from_slice(&scalar.value.denominator.to_le_bytes());
                encoded[24..32].copy_from_slice(
                    &u64::try_from(scalar.uncertainty.numerator)
                        .map_err(|_| ConfigurationError::Uncertainty)?
                        .to_le_bytes(),
                );
                encoded[32..40].copy_from_slice(&scalar.uncertainty.denominator.to_le_bytes());
                encoded[40] = scalar.evidence as u8;
                // Bytes 41..64 are reserved zero.
            }
        }
        Ok(encoded)
    }

    /// Decodes only an exact fixed-width V1 record.
    pub fn decode(encoded: &[u8]) -> Result<Self, ConfigurationError> {
        if encoded.len() != CONFIGURATION_RECORD_BYTES
            || usize::from(read_u16(encoded, 2)) != CONFIGURATION_RECORD_BYTES
        {
            return Err(ConfigurationError::Length);
        }
        let instance = read_u16(encoded, 4);
        let selector = read_u16(encoded, 6);
        let record = match read_u16(encoded, 0) {
            RECORD_KIND_BINDING => {
                if encoded[32..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                Self::Binding(ResourceBinding {
                    instance,
                    role: BindingRole::from_wire(selector).ok_or(ConfigurationError::Selector)?,
                    resource: decode_resource_id(&encoded[8..12])
                        .map_err(|_| ConfigurationError::ResourceEncoding)?,
                    owner: owner_from_wire(encoded[12]).ok_or(ConfigurationError::Owner)?,
                    polarity: SignalPolarity::from_wire(encoded[13])
                        .ok_or(ConfigurationError::Polarity)?,
                    flags: BindingFlags(read_u16(encoded, 14)),
                    minimum_active_cycles: read_u32(encoded, 16),
                    minimum_inactive_cycles: read_u32(encoded, 20),
                    maximum_frequency_hz: read_u32(encoded, 24),
                    watchdog_cycles: read_u32(encoded, 28),
                })
            }
            RECORD_KIND_SCALAR => {
                if encoded[41..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                let uncertainty_numerator = read_u64(encoded, 24);
                Self::Scalar(ExactScalar {
                    instance,
                    fact: ScalarFact::from_wire(selector).ok_or(ConfigurationError::Selector)?,
                    value: Rational {
                        numerator: read_i64(encoded, 8),
                        denominator: read_u64(encoded, 16),
                    },
                    uncertainty: Rational {
                        numerator: i64::try_from(uncertainty_numerator)
                            .map_err(|_| ConfigurationError::Uncertainty)?,
                        denominator: read_u64(encoded, 32),
                    },
                    evidence: FactEvidence::from_wire(encoded[40])
                        .ok_or(ConfigurationError::Evidence)?,
                })
            }
            _ => return Err(ConfigurationError::RecordKind),
        };
        record.validate_shape()?;
        if record.encode()? != encoded {
            return Err(ConfigurationError::Noncanonical);
        }
        Ok(record)
    }

    /// Whether core 1 must independently consume and validate this record.
    pub const fn realtime_relevant(self) -> bool {
        match self {
            Self::Binding(binding) => matches!(binding.owner, OwnerDomain::Realtime),
            Self::Scalar(scalar) => {
                scalar.fact.axis_fact()
                    || matches!(
                        scalar.fact,
                        ScalarFact::SafetyMaximumReactionSeconds
                            | ScalarFact::ProcessMaximumDurationSeconds
                            | ScalarFact::TimerTickHertz
                    )
            }
        }
    }

    const fn key(self) -> (u16, u16, u16) {
        match self {
            Self::Binding(binding) => (RECORD_KIND_BINDING, binding.instance, binding.role as u16),
            Self::Scalar(scalar) => (RECORD_KIND_SCALAR, scalar.instance, scalar.fact as u16),
        }
    }

    fn validate_shape(self) -> Result<(), ConfigurationError> {
        match self {
            Self::Binding(binding) => {
                if binding.flags.0 & !BindingFlags::ALLOWED != 0
                    || binding.flags.0 & BindingFlags::PULL_UP != 0
                        && binding.flags.0 & BindingFlags::PULL_DOWN != 0
                    || binding.role.axis_role()
                        && usize::from(binding.instance) >= MAX_AXIS_INSTANCES
                {
                    return Err(ConfigurationError::Binding);
                }
                if binding.role.is_analog_input()
                    || binding.role == BindingRole::FocEncoder
                        && matches!(binding.resource, ResourceId::Device(_))
                {
                    if binding.polarity != SignalPolarity::NotApplicable
                        || binding.flags.0 != 0
                        || binding.minimum_active_cycles != 0
                        || binding.minimum_inactive_cycles != 0
                        || binding.maximum_frequency_hz == 0
                    {
                        return Err(ConfigurationError::Binding);
                    }
                } else if binding.role.is_input() {
                    if binding.polarity == SignalPolarity::NotApplicable
                        || binding.flags.0 & BindingFlags::OPEN_DRAIN != 0
                        || binding.role.is_sampled_input() == (binding.maximum_frequency_hz == 0)
                    {
                        return Err(ConfigurationError::Binding);
                    }
                } else if binding.role.is_output() {
                    if binding.polarity == SignalPolarity::NotApplicable
                        || binding.flags.0 & (BindingFlags::PULL_UP | BindingFlags::PULL_DOWN) != 0
                    {
                        return Err(ConfigurationError::Binding);
                    }
                } else if binding.polarity != SignalPolarity::NotApplicable
                    || binding.flags.0 != 0
                    || binding.minimum_active_cycles != 0
                    || binding.minimum_inactive_cycles != 0
                    || binding.watchdog_cycles != 0
                {
                    return Err(ConfigurationError::Binding);
                }
                if (binding.role == BindingRole::AxisStep
                    || binding.role.is_pwm()
                    || binding.role == BindingRole::WaveformOutput)
                    && (binding.minimum_active_cycles == 0
                        || binding.minimum_inactive_cycles == 0
                        || binding.maximum_frequency_hz == 0
                        || binding.watchdog_cycles == 0)
                {
                    return Err(ConfigurationError::Timing);
                }
                if binding.role.is_hazardous_role() && binding.watchdog_cycles == 0 {
                    return Err(ConfigurationError::Timing);
                }
            }
            Self::Scalar(scalar) => {
                scalar.value.validate()?;
                scalar.uncertainty.validate()?;
                if scalar.uncertainty.numerator < 0
                    || scalar.fact.requires_positive() && scalar.value.numerator <= 0
                    || scalar.fact.requires_integer()
                        && (scalar.value.denominator != 1 || scalar.uncertainty.numerator != 0)
                    || scalar.fact.axis_fact() && usize::from(scalar.instance) >= MAX_AXIS_INSTANCES
                {
                    return Err(ConfigurationError::Scalar);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct AxisState {
    binding_mask: u32,
    scalar_mask: u32,
    minimum: Option<Rational>,
    maximum: Option<Rational>,
}

impl AxisState {
    const EMPTY: Self = Self {
        binding_mask: 0,
        scalar_mask: 0,
        minimum: None,
        maximum: None,
    };
}

/// Allocation-free semantic validator for a complete ordered record stream.
pub struct ConfigurationValidator<'a, const MAX_BINDINGS: usize> {
    package: &'a BoardPackage<'a>,
    header: ConfigurationHeader,
    seen_records: u16,
    realtime_records: u16,
    last_key: Option<(u16, u16, u16)>,
    claimed: [Option<ResourceId>; MAX_BINDINGS],
    claimed_len: usize,
    axes: [AxisState; MAX_AXIS_INSTANCES],
    safety_binding: bool,
}

impl<'a, const MAX_BINDINGS: usize> ConfigurationValidator<'a, MAX_BINDINGS> {
    /// Starts validation only against the exact compiled board capability.
    pub fn new(
        package: &'a BoardPackage<'a>,
        header: ConfigurationHeader,
    ) -> Result<Self, ConfigurationError> {
        let capability =
            verify_declared_identity(package).map_err(ConfigurationError::Capability)?;
        header.validate()?;
        if header.capability_digest != capability.digest {
            return Err(ConfigurationError::CapabilityIdentity);
        }
        Ok(Self {
            package,
            header,
            seen_records: 0,
            realtime_records: 0,
            last_key: None,
            claimed: [None; MAX_BINDINGS],
            claimed_len: 0,
            axes: [AxisState::EMPTY; MAX_AXIS_INSTANCES],
            safety_binding: false,
        })
    }

    /// Validates and consumes the next strictly ordered record.
    pub fn push(&mut self, record: ConfigurationRecord) -> Result<(), ConfigurationError> {
        if self.seen_records >= self.header.record_count {
            return Err(ConfigurationError::RecordCount);
        }
        record.validate_shape()?;
        let key = record.key();
        if self.last_key.is_some_and(|last| last >= key) {
            return Err(ConfigurationError::RecordOrder);
        }
        match record {
            ConfigurationRecord::Binding(binding) => self.validate_binding(binding)?,
            ConfigurationRecord::Scalar(scalar) => self.validate_scalar(scalar),
        }
        self.last_key = Some(key);
        self.seen_records += 1;
        if record.realtime_relevant() {
            self.realtime_records += 1;
        }
        Ok(())
    }

    /// Completes cross-record policy and count validation.
    pub fn finish(self) -> Result<ConfigurationSummary, ConfigurationError> {
        if self.seen_records != self.header.record_count
            || self.realtime_records != self.header.realtime_record_count
        {
            return Err(ConfigurationError::RecordCount);
        }
        let mut stepper_axes = 0_u8;
        let mut foc_axes = 0_u8;
        for axis in self.axes {
            let has_step = axis.binding_mask & role_bit(BindingRole::AxisStep) != 0;
            let has_foc = axis.binding_mask & role_bit(BindingRole::FocPhaseU) != 0
                || axis.binding_mask & role_bit(BindingRole::FocPhaseV) != 0
                || axis.binding_mask & role_bit(BindingRole::FocPhaseW) != 0;
            if has_step {
                let bindings = role_bit(BindingRole::AxisStep)
                    | role_bit(BindingRole::AxisDirection)
                    | role_bit(BindingRole::AxisEnable);
                let scalars = fact_bit(ScalarFact::AxisFullStepsPerTurn)
                    | fact_bit(ScalarFact::AxisMicrosteps)
                    | fact_bit(ScalarFact::AxisMotorTurnsPerOutputTurn)
                    | fact_bit(ScalarFact::AxisTravelMetresPerOutputTurn)
                    | fact_bit(ScalarFact::AxisCalibrationScale)
                    | fact_bit(ScalarFact::AxisPositionMinimumMetres)
                    | fact_bit(ScalarFact::AxisPositionMaximumMetres)
                    | fact_bit(ScalarFact::AxisVelocityLimitMetresPerSecond)
                    | fact_bit(ScalarFact::AxisAccelerationLimitMetresPerSecondSquared)
                    | fact_bit(ScalarFact::AxisJerkLimitMetresPerSecondCubed);
                if axis.binding_mask & bindings != bindings || axis.scalar_mask & scalars != scalars
                {
                    return Err(ConfigurationError::IncompleteAxis);
                }
                stepper_axes = stepper_axes
                    .checked_add(1)
                    .ok_or(ConfigurationError::AxisCount)?;
            }
            if has_foc {
                let bindings = role_bit(BindingRole::FocPhaseU)
                    | role_bit(BindingRole::FocPhaseV)
                    | role_bit(BindingRole::FocPhaseW)
                    | role_bit(BindingRole::FocEnable);
                let scalars = fact_bit(ScalarFact::MotorPolePairs)
                    | fact_bit(ScalarFact::MotorCurrentLimitAmperes)
                    | fact_bit(ScalarFact::MotorVoltageLimitVolts)
                    | fact_bit(ScalarFact::PwmCarrierHertz)
                    | fact_bit(ScalarFact::PwmDeadTimeSeconds)
                    | fact_bit(ScalarFact::ControlRateHertz);
                if axis.binding_mask & bindings != bindings || axis.scalar_mask & scalars != scalars
                {
                    return Err(ConfigurationError::IncompleteAxis);
                }
                foc_axes = foc_axes
                    .checked_add(1)
                    .ok_or(ConfigurationError::AxisCount)?;
            }
            if let (Some(minimum), Some(maximum)) = (axis.minimum, axis.maximum)
                && minimum.exact_cmp(maximum) != Ordering::Less
            {
                return Err(ConfigurationError::AxisRange);
            }
        }
        if self.header.flags.contains(ConfigurationFlags::MOTION)
            && (stepper_axes == 0 && foc_axes == 0 || !self.safety_binding)
        {
            return Err(ConfigurationError::MotionPolicy);
        }
        if self
            .header
            .flags
            .contains(ConfigurationFlags::FIELD_ORIENTED_CONTROL)
            != (foc_axes != 0)
        {
            return Err(ConfigurationError::MotionPolicy);
        }
        let summary = ConfigurationSummary {
            record_count: self.seen_records,
            realtime_record_count: self.realtime_records,
            binding_count: u16::try_from(self.claimed_len)
                .map_err(|_| ConfigurationError::BindingCapacity)?,
            stepper_axes,
            foc_axes,
            safety_binding: self.safety_binding,
            flags: self.header.flags,
        };
        summary.validate()?;
        Ok(summary)
    }

    fn validate_binding(&mut self, binding: ResourceBinding) -> Result<(), ConfigurationError> {
        let descriptor = find_resource(self.package, binding.resource)
            .ok_or(ConfigurationError::UnknownResource(binding.resource))?;
        if descriptor.owner != binding.owner {
            return Err(ConfigurationError::Ownership);
        }
        if !role_accepts_resource(binding.role, binding.resource) {
            return Err(ConfigurationError::ResourceKind);
        }
        if descriptor.hazardous_output && !binding.role.is_hazardous_role() {
            return Err(ConfigurationError::HazardousBinding);
        }
        if binding.role.is_output()
            && descriptor.safe_value == SafeValue::NotApplicable
            && !matches!(
                binding.resource,
                ResourceId::Rmt(_) | ResourceId::TimedOutput { .. }
            )
        {
            return Err(ConfigurationError::SafeState);
        }
        self.validate_constraints(binding)?;
        self.validate_bus_rate(binding)?;
        if self.claimed[..self.claimed_len].contains(&Some(binding.resource)) {
            return Err(ConfigurationError::DuplicateResource(binding.resource));
        }
        let slot = self
            .claimed
            .get_mut(self.claimed_len)
            .ok_or(ConfigurationError::BindingCapacity)?;
        *slot = Some(binding.resource);
        self.claimed_len += 1;

        if binding.role.axis_role() {
            self.axes[usize::from(binding.instance)].binding_mask |= role_bit(binding.role);
        }
        if matches!(
            binding.role,
            BindingRole::EmergencyStop | BindingRole::SafetyInterlock
        ) {
            self.safety_binding = true;
        }
        Ok(())
    }

    fn validate_constraints(&self, binding: ResourceBinding) -> Result<(), ConfigurationError> {
        for constraint in self.package.electrical_constraints {
            if !constraint
                .resources
                .iter()
                .any(|resource| constraint_applies(*resource, binding.resource))
            {
                continue;
            }
            match constraint.kind {
                ElectricalConstraintKind::InputOnly if binding.role.is_output() => {
                    return Err(ConfigurationError::ElectricalConstraint);
                }
                ElectricalConstraintKind::OutputOnly if binding.role.is_input() => {
                    return Err(ConfigurationError::ElectricalConstraint);
                }
                ElectricalConstraintKind::NotPwm if binding.role.is_pwm() => {
                    return Err(ConfigurationError::ElectricalConstraint);
                }
                ElectricalConstraintKind::ActiveHigh
                    if binding.polarity != SignalPolarity::ActiveHigh =>
                {
                    return Err(ConfigurationError::Polarity);
                }
                ElectricalConstraintKind::ActiveLow
                    if binding.polarity != SignalPolarity::ActiveLow =>
                {
                    return Err(ConfigurationError::Polarity);
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_bus_rate(&self, binding: ResourceBinding) -> Result<(), ConfigurationError> {
        let expected = match binding.role {
            BindingRole::SerialPort => Some(BusKind::Uart),
            BindingRole::I2cBus => Some(BusKind::I2c),
            BindingRole::SpiBus => Some(BusKind::Spi),
            _ => None,
        };
        let Some(expected) = expected else {
            return Ok(());
        };
        let bus = self
            .package
            .buses
            .iter()
            .find(|bus| bus.resource == binding.resource)
            .ok_or(ConfigurationError::ResourceKind)?;
        if bus.kind != expected
            || binding.maximum_frequency_hz == 0
            || binding.maximum_frequency_hz > bus.maximum_frequency_hz
        {
            return Err(ConfigurationError::Frequency);
        }
        Ok(())
    }

    fn validate_scalar(&mut self, scalar: ExactScalar) {
        if scalar.fact.axis_fact() {
            let axis = &mut self.axes[usize::from(scalar.instance)];
            axis.scalar_mask |= fact_bit(scalar.fact);
            match scalar.fact {
                ScalarFact::AxisPositionMinimumMetres => axis.minimum = Some(scalar.value),
                ScalarFact::AxisPositionMaximumMetres => axis.maximum = Some(scalar.value),
                _ => {}
            }
        }
    }
}

/// Derived bounded facts after complete semantic validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationSummary {
    pub record_count: u16,
    pub realtime_record_count: u16,
    pub binding_count: u16,
    pub stepper_axes: u8,
    pub foc_axes: u8,
    pub safety_binding: bool,
    pub flags: ConfigurationFlags,
}

impl ConfigurationSummary {
    fn validate(self) -> Result<(), ConfigurationError> {
        let axes = u16::from(self.stepper_axes)
            .checked_add(u16::from(self.foc_axes))
            .ok_or(ConfigurationError::AxisCount)?;
        if self.record_count == 0
            || usize::from(self.record_count) > MAX_CONFIGURATION_RECORDS
            || self.realtime_record_count > self.record_count
            || self.binding_count > self.record_count
            || usize::from(axes) > MAX_AXIS_INSTANCES
            || self.flags.0 & !ConfigurationFlags::ALLOWED != 0
            || self.flags.contains(ConfigurationFlags::MOTION)
                && (axes == 0 || !self.safety_binding)
            || self.flags.contains(ConfigurationFlags::CACHED_AUTONOMOUS)
                && !self.flags.contains(ConfigurationFlags::MOTION)
            || self
                .flags
                .contains(ConfigurationFlags::FIELD_ORIENTED_CONTROL)
                != (self.foc_axes != 0)
        {
            return Err(ConfigurationError::MotionPolicy);
        }
        Ok(())
    }
}

/// Exact identity and validated summary of a complete configuration document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationIdentity {
    pub digest: Digest,
    pub byte_len: u32,
    pub capability_digest: Digest,
    pub summary: ConfigurationSummary,
}

/// Fixed command prefix before an optional core-to-core configuration byte chunk.
pub const CORE_CONFIGURATION_COMMAND_PREFIX_BYTES: usize = 64;
/// Maximum document bytes transferred in one 256-byte intercore command.
pub const MAX_CORE_CONFIGURATION_DATA_BYTES: usize = 192;
/// Complete fixed command storage, matching the default runtime command payload.
pub const CORE_CONFIGURATION_COMMAND_CAPACITY: usize =
    CORE_CONFIGURATION_COMMAND_PREFIX_BYTES + MAX_CORE_CONFIGURATION_DATA_BYTES;
/// Exact fixed core-1 configuration report bytes, filling one telemetry payload.
pub const CORE_CONFIGURATION_REPORT_BYTES: usize = 128;

const CORE_COMMAND_MAGIC: [u8; 4] = *b"ALCC";
const CORE_REPORT_MAGIC: [u8; 4] = *b"ALCR";
const CORE_WIRE_VERSION: u16 = 1;

/// Ordered action transferring or activating one independently validated configuration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum CoreConfigurationAction {
    Begin = 1,
    Data = 2,
    Finish = 3,
    Activate = 4,
    Clear = 5,
    Abort = 6,
}

impl CoreConfigurationAction {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            1 => Some(Self::Begin),
            2 => Some(Self::Data),
            3 => Some(Self::Finish),
            4 => Some(Self::Activate),
            5 => Some(Self::Clear),
            6 => Some(Self::Abort),
            _ => None,
        }
    }
}

/// Owned ordered core-0 to core-1 configuration command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreConfigurationCommand {
    pub action: CoreConfigurationAction,
    pub transaction_id: u64,
    pub digest: Digest,
    pub total_bytes: u32,
    pub offset: u32,
    data_len: u16,
    data: [u8; MAX_CORE_CONFIGURATION_DATA_BYTES],
}

impl CoreConfigurationCommand {
    pub fn begin(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        Self::without_data(
            CoreConfigurationAction::Begin,
            transaction_id,
            digest,
            total_bytes,
            0,
        )
    }

    pub fn data(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
        offset: u32,
        data: &[u8],
    ) -> Result<Self, CoreConfigurationWireError> {
        let data_len =
            u16::try_from(data.len()).map_err(|_| CoreConfigurationWireError::DataLength)?;
        let mut command = Self {
            action: CoreConfigurationAction::Data,
            transaction_id,
            digest,
            total_bytes,
            offset,
            data_len,
            data: [0; MAX_CORE_CONFIGURATION_DATA_BYTES],
        };
        let destination = command
            .data
            .get_mut(..data.len())
            .ok_or(CoreConfigurationWireError::DataLength)?;
        destination.copy_from_slice(data);
        command.validate()?;
        Ok(command)
    }

    pub fn finish(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        Self::without_data(
            CoreConfigurationAction::Finish,
            transaction_id,
            digest,
            total_bytes,
            total_bytes,
        )
    }

    pub fn activate(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        Self::without_data(
            CoreConfigurationAction::Activate,
            transaction_id,
            digest,
            total_bytes,
            0,
        )
    }

    pub fn clear(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        Self::without_data(
            CoreConfigurationAction::Clear,
            transaction_id,
            digest,
            total_bytes,
            0,
        )
    }

    pub fn abort(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        Self::without_data(
            CoreConfigurationAction::Abort,
            transaction_id,
            digest,
            total_bytes,
            0,
        )
    }

    /// Only initialized document bytes carried by a `Data` action.
    pub fn data_bytes(&self) -> &[u8] {
        &self.data[..usize::from(self.data_len)]
    }

    /// Encodes only the initialized command prefix and data bytes.
    pub fn encode(self) -> Result<EncodedCoreConfigurationCommand, CoreConfigurationWireError> {
        self.validate()?;
        let mut bytes = [0_u8; CORE_CONFIGURATION_COMMAND_CAPACITY];
        bytes[0..4].copy_from_slice(&CORE_COMMAND_MAGIC);
        bytes[4..6].copy_from_slice(&CORE_WIRE_VERSION.to_le_bytes());
        bytes[6] = self.action as u8;
        // Byte 7 and bytes 58..64 are reserved zero.
        bytes[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        bytes[16..48].copy_from_slice(&self.digest.0);
        bytes[48..52].copy_from_slice(&self.total_bytes.to_le_bytes());
        bytes[52..56].copy_from_slice(&self.offset.to_le_bytes());
        bytes[56..58].copy_from_slice(&self.data_len.to_le_bytes());
        let data_len = usize::from(self.data_len);
        bytes[CORE_CONFIGURATION_COMMAND_PREFIX_BYTES
            ..CORE_CONFIGURATION_COMMAND_PREFIX_BYTES + data_len]
            .copy_from_slice(&self.data[..data_len]);
        Ok(EncodedCoreConfigurationCommand {
            byte_len: u16::try_from(CORE_CONFIGURATION_COMMAND_PREFIX_BYTES + data_len)
                .map_err(|_| CoreConfigurationWireError::Length)?,
            bytes,
        })
    }

    /// Decodes one exact initialized command payload.
    pub fn decode(encoded: &[u8]) -> Result<Self, CoreConfigurationWireError> {
        if encoded.len() < CORE_CONFIGURATION_COMMAND_PREFIX_BYTES
            || encoded.len() > CORE_CONFIGURATION_COMMAND_CAPACITY
        {
            return Err(CoreConfigurationWireError::Length);
        }
        if encoded[0..4] != CORE_COMMAND_MAGIC {
            return Err(CoreConfigurationWireError::Magic);
        }
        if read_u16(encoded, 4) != CORE_WIRE_VERSION {
            return Err(CoreConfigurationWireError::Version);
        }
        if encoded[7] != 0 || encoded[58..64].iter().any(|byte| *byte != 0) {
            return Err(CoreConfigurationWireError::Reserved);
        }
        let data_len = read_u16(encoded, 56);
        if encoded.len()
            != CORE_CONFIGURATION_COMMAND_PREFIX_BYTES
                .checked_add(usize::from(data_len))
                .ok_or(CoreConfigurationWireError::Length)?
        {
            return Err(CoreConfigurationWireError::Length);
        }
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[16..48]);
        let mut data = [0_u8; MAX_CORE_CONFIGURATION_DATA_BYTES];
        let source = &encoded[CORE_CONFIGURATION_COMMAND_PREFIX_BYTES..];
        data.get_mut(..source.len())
            .ok_or(CoreConfigurationWireError::DataLength)?
            .copy_from_slice(source);
        let command = Self {
            action: CoreConfigurationAction::from_wire(encoded[6])
                .ok_or(CoreConfigurationWireError::Action)?,
            transaction_id: read_u64(encoded, 8),
            digest: Digest(digest),
            total_bytes: read_u32(encoded, 48),
            offset: read_u32(encoded, 52),
            data_len,
            data,
        };
        command.validate()?;
        if command.encode()?.as_bytes() != encoded {
            return Err(CoreConfigurationWireError::Noncanonical);
        }
        Ok(command)
    }

    fn without_data(
        action: CoreConfigurationAction,
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
        offset: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        let command = Self {
            action,
            transaction_id,
            digest,
            total_bytes,
            offset,
            data_len: 0,
            data: [0; MAX_CORE_CONFIGURATION_DATA_BYTES],
        };
        command.validate()?;
        Ok(command)
    }

    fn validate(self) -> Result<(), CoreConfigurationWireError> {
        if self.transaction_id == 0
            || self.digest.is_zero()
            || !configuration_length_valid(self.total_bytes)
        {
            return Err(CoreConfigurationWireError::Identity);
        }
        match self.action {
            CoreConfigurationAction::Begin
            | CoreConfigurationAction::Activate
            | CoreConfigurationAction::Clear
            | CoreConfigurationAction::Abort
                if self.offset == 0 && self.data_len == 0 =>
            {
                Ok(())
            }
            CoreConfigurationAction::Finish
                if self.offset == self.total_bytes && self.data_len == 0 =>
            {
                Ok(())
            }
            CoreConfigurationAction::Data
                if self.data_len != 0
                    && usize::from(self.data_len) <= MAX_CORE_CONFIGURATION_DATA_BYTES
                    && self
                        .offset
                        .checked_add(u32::from(self.data_len))
                        .is_some_and(|end| end <= self.total_bytes) =>
            {
                Ok(())
            }
            CoreConfigurationAction::Data => Err(CoreConfigurationWireError::DataLength),
            _ => Err(CoreConfigurationWireError::ActionState),
        }
    }
}

/// Exact initialized representation of one intercore command.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EncodedCoreConfigurationCommand {
    byte_len: u16,
    bytes: [u8; CORE_CONFIGURATION_COMMAND_CAPACITY],
}

impl EncodedCoreConfigurationCommand {
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.byte_len)]
    }
}

/// Intercore configuration command syntax or state-shape rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreConfigurationWireError {
    Length,
    Magic,
    Version,
    Reserved,
    Noncanonical,
    Action,
    ActionState,
    Identity,
    DataLength,
}

/// Core-1 configuration lifecycle state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum RealtimeConfigurationState {
    Empty = 0,
    Receiving = 1,
    CandidateValid = 2,
    Active = 3,
    Rejected = 4,
    Cleared = 5,
}

impl RealtimeConfigurationState {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Receiving),
            2 => Some(Self::CandidateValid),
            3 => Some(Self::Active),
            4 => Some(Self::Rejected),
            5 => Some(Self::Cleared),
            _ => None,
        }
    }
}

/// Stable fault category suitable for bounded status and HIL assertions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum ConfigurationFaultCode {
    None = 0,
    Sequence = 1,
    Syntax = 2,
    Capability = 3,
    Identity = 4,
    Resource = 5,
    Electrical = 6,
    Timing = 7,
    ExactFact = 8,
    Completeness = 9,
    Capacity = 10,
    ForbiddenState = 11,
}

impl ConfigurationFaultCode {
    const fn from_wire(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Sequence),
            2 => Some(Self::Syntax),
            3 => Some(Self::Capability),
            4 => Some(Self::Identity),
            5 => Some(Self::Resource),
            6 => Some(Self::Electrical),
            7 => Some(Self::Timing),
            8 => Some(Self::ExactFact),
            9 => Some(Self::Completeness),
            10 => Some(Self::Capacity),
            11 => Some(Self::ForbiddenState),
            _ => None,
        }
    }

    /// Reduces an internal validation error without losing the fail-closed family.
    pub const fn from_configuration_error(error: ConfigurationError) -> Self {
        match error {
            ConfigurationError::Capability(_) | ConfigurationError::CapabilityIdentity => {
                Self::Capability
            }
            ConfigurationError::ConfigurationIdentity => Self::Identity,
            ConfigurationError::UnknownResource(_)
            | ConfigurationError::DuplicateResource(_)
            | ConfigurationError::ResourceKind
            | ConfigurationError::Owner
            | ConfigurationError::Ownership
            | ConfigurationError::HazardousBinding
            | ConfigurationError::SafeState => Self::Resource,
            ConfigurationError::Polarity | ConfigurationError::ElectricalConstraint => {
                Self::Electrical
            }
            ConfigurationError::Timing | ConfigurationError::Frequency => Self::Timing,
            ConfigurationError::Rational
            | ConfigurationError::Uncertainty
            | ConfigurationError::Scalar
            | ConfigurationError::Evidence => Self::ExactFact,
            ConfigurationError::IncompleteAxis
            | ConfigurationError::AxisCount
            | ConfigurationError::AxisRange
            | ConfigurationError::MotionPolicy => Self::Completeness,
            ConfigurationError::BindingCapacity => Self::Capacity,
            ConfigurationError::Length
            | ConfigurationError::Magic
            | ConfigurationError::Version
            | ConfigurationError::Reserved
            | ConfigurationError::Noncanonical
            | ConfigurationError::Flags
            | ConfigurationError::RecordCount
            | ConfigurationError::RecordKind
            | ConfigurationError::RecordOrder
            | ConfigurationError::Selector
            | ConfigurationError::ResourceEncoding
            | ConfigurationError::Binding
            | ConfigurationError::Internal => Self::Syntax,
        }
    }
}

/// Exact core-1 observation of a receiving, validated, active, or rejected identity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RealtimeConfigurationReport {
    pub state: RealtimeConfigurationState,
    pub transaction_id: u64,
    pub digest: Digest,
    pub total_bytes: u32,
    pub consumed_bytes: u32,
    pub summary: Option<ConfigurationSummary>,
    pub fault: ConfigurationFaultCode,
    /// Independently active identity retained while another candidate is handled.
    pub active_digest: Digest,
    /// Exact active document bytes, or zero together with `active_digest`.
    pub active_bytes: u32,
}

impl RealtimeConfigurationReport {
    /// Encodes one canonical fixed report.
    pub fn encode(
        self,
    ) -> Result<[u8; CORE_CONFIGURATION_REPORT_BYTES], CoreConfigurationReportError> {
        self.validate()?;
        let mut encoded = [0_u8; CORE_CONFIGURATION_REPORT_BYTES];
        encoded[0..4].copy_from_slice(&CORE_REPORT_MAGIC);
        encoded[4..6].copy_from_slice(&CORE_WIRE_VERSION.to_le_bytes());
        encoded[6] = self.state as u8;
        encoded[7] = u8::from(self.summary.is_some());
        encoded[8..16].copy_from_slice(&self.transaction_id.to_le_bytes());
        encoded[16..48].copy_from_slice(&self.digest.0);
        encoded[48..52].copy_from_slice(&self.total_bytes.to_le_bytes());
        encoded[52..56].copy_from_slice(&self.consumed_bytes.to_le_bytes());
        if let Some(summary) = self.summary {
            encoded[56..58].copy_from_slice(&summary.record_count.to_le_bytes());
            encoded[58..60].copy_from_slice(&summary.realtime_record_count.to_le_bytes());
            encoded[60..62].copy_from_slice(&summary.binding_count.to_le_bytes());
            encoded[62] = summary.stepper_axes;
            encoded[63] = summary.foc_axes;
            encoded[64..68].copy_from_slice(&summary.flags.0.to_le_bytes());
            encoded[70] = u8::from(summary.safety_binding);
        }
        encoded[68..70].copy_from_slice(&(self.fault as u16).to_le_bytes());
        encoded[72..104].copy_from_slice(&self.active_digest.0);
        encoded[104..108].copy_from_slice(&self.active_bytes.to_le_bytes());
        // Byte 71 and bytes 108..128 are reserved zero.
        Ok(encoded)
    }

    /// Decodes only the exact canonical report representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, CoreConfigurationReportError> {
        if encoded.len() != CORE_CONFIGURATION_REPORT_BYTES {
            return Err(CoreConfigurationReportError::Length);
        }
        if encoded[0..4] != CORE_REPORT_MAGIC {
            return Err(CoreConfigurationReportError::Magic);
        }
        if read_u16(encoded, 4) != CORE_WIRE_VERSION {
            return Err(CoreConfigurationReportError::Version);
        }
        if encoded[7] & !1 != 0
            || encoded[71] != 0
            || encoded[108..128].iter().any(|byte| *byte != 0)
        {
            return Err(CoreConfigurationReportError::Reserved);
        }
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&encoded[16..48]);
        let mut active_digest = [0_u8; 32];
        active_digest.copy_from_slice(&encoded[72..104]);
        let summary = if encoded[7] == 1 {
            Some(ConfigurationSummary {
                record_count: read_u16(encoded, 56),
                realtime_record_count: read_u16(encoded, 58),
                binding_count: read_u16(encoded, 60),
                stepper_axes: encoded[62],
                foc_axes: encoded[63],
                safety_binding: encoded[70] != 0,
                flags: ConfigurationFlags(read_u32(encoded, 64)),
            })
        } else {
            if encoded[56..68].iter().any(|byte| *byte != 0) || encoded[70] != 0 {
                return Err(CoreConfigurationReportError::Reserved);
            }
            None
        };
        let report = Self {
            state: RealtimeConfigurationState::from_wire(encoded[6])
                .ok_or(CoreConfigurationReportError::State)?,
            transaction_id: read_u64(encoded, 8),
            digest: Digest(digest),
            total_bytes: read_u32(encoded, 48),
            consumed_bytes: read_u32(encoded, 52),
            summary,
            fault: ConfigurationFaultCode::from_wire(read_u16(encoded, 68))
                .ok_or(CoreConfigurationReportError::Fault)?,
            active_digest: Digest(active_digest),
            active_bytes: read_u32(encoded, 104),
        };
        report.validate()?;
        if report.encode()? != encoded {
            return Err(CoreConfigurationReportError::Noncanonical);
        }
        Ok(report)
    }

    fn validate(self) -> Result<(), CoreConfigurationReportError> {
        if self
            .summary
            .is_some_and(|summary| summary.validate().is_err())
        {
            return Err(CoreConfigurationReportError::Summary);
        }
        if self.active_digest.is_zero() != (self.active_bytes == 0)
            || self.active_bytes != 0 && !configuration_length_valid(self.active_bytes)
        {
            return Err(CoreConfigurationReportError::ActiveIdentity);
        }
        match self.state {
            RealtimeConfigurationState::Empty
                if self.transaction_id == 0
                    && self.digest.is_zero()
                    && self.total_bytes == 0
                    && self.consumed_bytes == 0
                    && self.summary.is_none()
                    && self.fault == ConfigurationFaultCode::None
                    && self.active_digest.is_zero() =>
            {
                Ok(())
            }
            RealtimeConfigurationState::Receiving
                if self.valid_identity()
                    && self.consumed_bytes <= self.total_bytes
                    && self.summary.is_none()
                    && self.fault == ConfigurationFaultCode::None =>
            {
                Ok(())
            }
            RealtimeConfigurationState::CandidateValid
                if self.valid_identity()
                    && self.consumed_bytes == self.total_bytes
                    && self.summary.is_some()
                    && self.fault == ConfigurationFaultCode::None =>
            {
                Ok(())
            }
            RealtimeConfigurationState::Active
                if self.valid_identity()
                    && self.consumed_bytes == self.total_bytes
                    && self.summary.is_some()
                    && self.fault == ConfigurationFaultCode::None
                    && self.active_digest == self.digest
                    && self.active_bytes == self.total_bytes =>
            {
                Ok(())
            }
            RealtimeConfigurationState::Rejected
                if self.valid_identity()
                    && self.consumed_bytes <= self.total_bytes
                    && self.summary.is_none()
                    && self.fault != ConfigurationFaultCode::None =>
            {
                Ok(())
            }
            RealtimeConfigurationState::Cleared
                if self.valid_identity()
                    && self.consumed_bytes == self.total_bytes
                    && self.summary.is_none()
                    && self.fault == ConfigurationFaultCode::None
                    && self.active_digest.is_zero() =>
            {
                Ok(())
            }
            _ => Err(CoreConfigurationReportError::StateShape),
        }
    }

    fn valid_identity(self) -> bool {
        self.transaction_id != 0
            && !self.digest.is_zero()
            && configuration_length_valid(self.total_bytes)
    }
}

/// Canonical core report rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CoreConfigurationReportError {
    Length,
    Magic,
    Version,
    Reserved,
    Noncanonical,
    State,
    Fault,
    Summary,
    ActiveIdentity,
    StateShape,
}

/// Core-1 owner of candidate bytes, independent semantic validation, and active identity.
pub struct RealtimeConfigurationService<'a, const MAX_BINDINGS: usize> {
    package: &'a BoardPackage<'a>,
    receiving: Option<ConfigurationStreamValidator<'a, MAX_BINDINGS>>,
    transaction_id: u64,
    digest: Digest,
    total_bytes: u32,
    consumed_bytes: u32,
    candidate: Option<ConfigurationIdentity>,
    active: Option<ConfigurationIdentity>,
    last_fault: ConfigurationFaultCode,
}

impl<'a, const MAX_BINDINGS: usize> RealtimeConfigurationService<'a, MAX_BINDINGS> {
    /// Starts with no received candidate and no active machine configuration.
    pub const fn new(package: &'a BoardPackage<'a>) -> Self {
        Self {
            package,
            receiving: None,
            transaction_id: 0,
            digest: Digest::ZERO,
            total_bytes: 0,
            consumed_bytes: 0,
            candidate: None,
            active: None,
            last_fault: ConfigurationFaultCode::None,
        }
    }

    /// Applies one ordered command only while the safety/job owner permits mutation.
    pub fn apply(
        &mut self,
        command: CoreConfigurationCommand,
        mutation_allowed: bool,
    ) -> RealtimeConfigurationReport {
        if !mutation_allowed {
            return self.reject(command, ConfigurationFaultCode::ForbiddenState);
        }
        match command.action {
            CoreConfigurationAction::Begin => {
                let validator = match ConfigurationStreamValidator::new(
                    self.package,
                    command.digest,
                    command.total_bytes,
                ) {
                    Ok(validator) => validator,
                    Err(error) => {
                        return self.reject(
                            command,
                            ConfigurationFaultCode::from_configuration_error(error),
                        );
                    }
                };
                self.receiving = Some(validator);
                self.transaction_id = command.transaction_id;
                self.digest = command.digest;
                self.total_bytes = command.total_bytes;
                self.consumed_bytes = 0;
                self.candidate = None;
                self.last_fault = ConfigurationFaultCode::None;
                self.report()
            }
            CoreConfigurationAction::Data => {
                if !self.matches(command) || command.offset != self.consumed_bytes {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                let result = self
                    .receiving
                    .as_mut()
                    .ok_or(ConfigurationError::Internal)
                    .and_then(|validator| validator.push(command.data_bytes()));
                if let Err(error) = result {
                    return self.reject(
                        command,
                        ConfigurationFaultCode::from_configuration_error(error),
                    );
                }
                self.consumed_bytes =
                    match self.consumed_bytes.checked_add(u32::from(command.data_len)) {
                        Some(consumed) => consumed,
                        None => return self.reject(command, ConfigurationFaultCode::Sequence),
                    };
                self.report()
            }
            CoreConfigurationAction::Finish => {
                if !self.matches(command) || self.consumed_bytes != self.total_bytes {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                let Some(validator) = self.receiving.take() else {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                };
                match validator.finish() {
                    Ok(identity) => {
                        self.candidate = Some(identity);
                        self.last_fault = ConfigurationFaultCode::None;
                        self.report()
                    }
                    Err(error) => self.reject(
                        command,
                        ConfigurationFaultCode::from_configuration_error(error),
                    ),
                }
            }
            CoreConfigurationAction::Activate => {
                if !self.matches(command)
                    || self
                        .candidate
                        .is_none_or(|candidate| !identity_matches(candidate, command))
                {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                self.active = self.candidate.take();
                self.receiving = None;
                self.last_fault = ConfigurationFaultCode::None;
                self.report()
            }
            CoreConfigurationAction::Clear => {
                if self
                    .active
                    .is_none_or(|active| !identity_matches(active, command))
                {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                let report = RealtimeConfigurationReport {
                    state: RealtimeConfigurationState::Cleared,
                    transaction_id: command.transaction_id,
                    digest: command.digest,
                    total_bytes: command.total_bytes,
                    consumed_bytes: command.total_bytes,
                    summary: None,
                    fault: ConfigurationFaultCode::None,
                    active_digest: Digest::ZERO,
                    active_bytes: 0,
                };
                self.receiving = None;
                self.candidate = None;
                self.active = None;
                self.reset_candidate_metadata();
                report
            }
            CoreConfigurationAction::Abort => {
                if !self.matches(command) {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                self.receiving = None;
                self.candidate = None;
                self.last_fault = ConfigurationFaultCode::Identity;
                self.report()
            }
        }
    }

    /// Latest state suitable for periodic replay after lossy telemetry.
    pub fn report(&self) -> RealtimeConfigurationReport {
        if let Some(candidate) = self.candidate {
            return identity_report(
                RealtimeConfigurationState::CandidateValid,
                self.transaction_id,
                candidate,
                self.active,
            );
        }
        let (active_digest, active_bytes) = active_fields(self.active);
        if self.receiving.is_some() {
            return RealtimeConfigurationReport {
                state: RealtimeConfigurationState::Receiving,
                transaction_id: self.transaction_id,
                digest: self.digest,
                total_bytes: self.total_bytes,
                consumed_bytes: self.consumed_bytes,
                summary: None,
                fault: ConfigurationFaultCode::None,
                active_digest,
                active_bytes,
            };
        }
        if self.last_fault != ConfigurationFaultCode::None {
            return RealtimeConfigurationReport {
                state: RealtimeConfigurationState::Rejected,
                transaction_id: self.transaction_id,
                digest: self.digest,
                total_bytes: self.total_bytes,
                consumed_bytes: self.consumed_bytes.min(self.total_bytes),
                summary: None,
                fault: self.last_fault,
                active_digest,
                active_bytes,
            };
        }
        if let Some(active) = self.active {
            return identity_report(
                RealtimeConfigurationState::Active,
                self.transaction_id,
                active,
                Some(active),
            );
        }
        RealtimeConfigurationReport {
            state: RealtimeConfigurationState::Empty,
            transaction_id: 0,
            digest: Digest::ZERO,
            total_bytes: 0,
            consumed_bytes: 0,
            summary: None,
            fault: ConfigurationFaultCode::None,
            active_digest: Digest::ZERO,
            active_bytes: 0,
        }
    }

    /// Exact identity independently active on core 1.
    pub const fn active_identity(&self) -> Option<ConfigurationIdentity> {
        self.active
    }

    /// Exact independently validated but inactive candidate.
    pub const fn candidate_identity(&self) -> Option<ConfigurationIdentity> {
        self.candidate
    }

    fn matches(&self, command: CoreConfigurationCommand) -> bool {
        self.transaction_id == command.transaction_id
            && self.digest == command.digest
            && self.total_bytes == command.total_bytes
    }

    fn reject(
        &mut self,
        command: CoreConfigurationCommand,
        fault: ConfigurationFaultCode,
    ) -> RealtimeConfigurationReport {
        self.receiving = None;
        self.candidate = None;
        self.transaction_id = command.transaction_id;
        self.digest = command.digest;
        self.total_bytes = command.total_bytes;
        self.consumed_bytes = self.consumed_bytes.min(command.total_bytes);
        self.last_fault = fault;
        RealtimeConfigurationReport {
            state: RealtimeConfigurationState::Rejected,
            transaction_id: self.transaction_id,
            digest: self.digest,
            total_bytes: self.total_bytes,
            consumed_bytes: self.consumed_bytes,
            summary: None,
            fault,
            active_digest: active_fields(self.active).0,
            active_bytes: active_fields(self.active).1,
        }
    }

    fn reset_candidate_metadata(&mut self) {
        self.transaction_id = 0;
        self.digest = Digest::ZERO;
        self.total_bytes = 0;
        self.consumed_bytes = 0;
        self.last_fault = ConfigurationFaultCode::None;
    }
}

fn identity_matches(identity: ConfigurationIdentity, command: CoreConfigurationCommand) -> bool {
    identity.digest == command.digest && identity.byte_len == command.total_bytes
}

fn configuration_length_valid(byte_len: u32) -> bool {
    usize::try_from(byte_len).is_ok_and(|byte_len| {
        (CONFIGURATION_HEADER_BYTES + CONFIGURATION_RECORD_BYTES
            ..=CONFIGURATION_HEADER_BYTES + MAX_CONFIGURATION_RECORDS * CONFIGURATION_RECORD_BYTES)
            .contains(&byte_len)
            && (byte_len - CONFIGURATION_HEADER_BYTES) % CONFIGURATION_RECORD_BYTES == 0
    })
}

fn identity_report(
    state: RealtimeConfigurationState,
    transaction_id: u64,
    identity: ConfigurationIdentity,
    active: Option<ConfigurationIdentity>,
) -> RealtimeConfigurationReport {
    let (active_digest, active_bytes) = active_fields(active);
    RealtimeConfigurationReport {
        state,
        transaction_id,
        digest: identity.digest,
        total_bytes: identity.byte_len,
        consumed_bytes: identity.byte_len,
        summary: Some(identity.summary),
        fault: ConfigurationFaultCode::None,
        active_digest,
        active_bytes,
    }
}

const fn active_fields(active: Option<ConfigurationIdentity>) -> (Digest, u32) {
    match active {
        Some(identity) => (identity.digest, identity.byte_len),
        None => (Digest::ZERO, 0),
    }
}

/// Arbitrary-chunk, allocation-free document validator used identically by both cores.
pub struct ConfigurationStreamValidator<'a, const MAX_BINDINGS: usize> {
    package: &'a BoardPackage<'a>,
    expected_digest: Digest,
    expected_bytes: u32,
    consumed: u32,
    hasher: Sha256,
    header_bytes: [u8; CONFIGURATION_HEADER_BYTES],
    header_used: usize,
    record_bytes: [u8; CONFIGURATION_RECORD_BYTES],
    record_used: usize,
    validator: Option<ConfigurationValidator<'a, MAX_BINDINGS>>,
}

impl<'a, const MAX_BINDINGS: usize> ConfigurationStreamValidator<'a, MAX_BINDINGS> {
    /// Creates a stream bound to the immutable stored object identity and length.
    pub fn new(
        package: &'a BoardPackage<'a>,
        expected_digest: Digest,
        expected_bytes: u32,
    ) -> Result<Self, ConfigurationError> {
        if expected_digest.is_zero() || !configuration_length_valid(expected_bytes) {
            return Err(ConfigurationError::ConfigurationIdentity);
        }
        Ok(Self {
            package,
            expected_digest,
            expected_bytes,
            consumed: 0,
            hasher: Sha256::new(),
            header_bytes: [0; CONFIGURATION_HEADER_BYTES],
            header_used: 0,
            record_bytes: [0; CONFIGURATION_RECORD_BYTES],
            record_used: 0,
            validator: None,
        })
    }

    /// Consumes any nonempty contiguous byte slice without retaining caller memory.
    pub fn push(&mut self, mut bytes: &[u8]) -> Result<(), ConfigurationError> {
        if bytes.is_empty()
            || self
                .consumed
                .checked_add(u32::try_from(bytes.len()).map_err(|_| ConfigurationError::Length)?)
                .is_none_or(|end| end > self.expected_bytes)
        {
            return Err(ConfigurationError::Length);
        }
        self.hasher.update(bytes);
        self.consumed += u32::try_from(bytes.len()).map_err(|_| ConfigurationError::Length)?;
        while !bytes.is_empty() {
            if self.validator.is_none() {
                let count = bytes
                    .len()
                    .min(CONFIGURATION_HEADER_BYTES - self.header_used);
                self.header_bytes[self.header_used..self.header_used + count]
                    .copy_from_slice(&bytes[..count]);
                self.header_used += count;
                bytes = &bytes[count..];
                if self.header_used == CONFIGURATION_HEADER_BYTES {
                    let header = ConfigurationHeader::decode(&self.header_bytes)?;
                    if header.total_bytes()? != self.expected_bytes {
                        return Err(ConfigurationError::Length);
                    }
                    self.validator = Some(ConfigurationValidator::new(self.package, header)?);
                }
            } else {
                let count = bytes
                    .len()
                    .min(CONFIGURATION_RECORD_BYTES - self.record_used);
                self.record_bytes[self.record_used..self.record_used + count]
                    .copy_from_slice(&bytes[..count]);
                self.record_used += count;
                bytes = &bytes[count..];
                if self.record_used == CONFIGURATION_RECORD_BYTES {
                    let record = ConfigurationRecord::decode(&self.record_bytes)?;
                    self.validator
                        .as_mut()
                        .ok_or(ConfigurationError::Internal)?
                        .push(record)?;
                    self.record_used = 0;
                    self.record_bytes.fill(0);
                }
            }
        }
        Ok(())
    }

    /// Requires exact completion, semantic validity, and final SHA-256 identity.
    pub fn finish(self) -> Result<ConfigurationIdentity, ConfigurationError> {
        if self.consumed != self.expected_bytes
            || self.header_used != CONFIGURATION_HEADER_BYTES
            || self.record_used != 0
        {
            return Err(ConfigurationError::Length);
        }
        let validator = self.validator.ok_or(ConfigurationError::Length)?;
        let capability_digest = validator.header.capability_digest;
        let summary = validator.finish()?;
        let hash = self.hasher.finalize();
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&hash);
        let digest = Digest(digest);
        if digest != self.expected_digest {
            return Err(ConfigurationError::ConfigurationIdentity);
        }
        Ok(ConfigurationIdentity {
            digest,
            byte_len: self.expected_bytes,
            capability_digest,
            summary,
        })
    }
}

/// Core-0 state of one published configuration validation/transfer pass.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ServiceConfigurationState {
    Begin = 1,
    Streaming = 2,
    Finish = 3,
    Complete = 4,
    Faulted = 5,
}

/// Fixed status of the storage reader and core-0 validator.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ServiceConfigurationStatus {
    pub state: ServiceConfigurationState,
    pub transaction_id: u64,
    pub digest: Digest,
    pub total_bytes: u32,
    pub validated_bytes: u32,
    pub storage_chunks_read: u32,
    pub identity: Option<ConfigurationIdentity>,
}

/// Sole core-0 owner of one verified publication cursor and validation buffers.
pub struct ServiceConfigurationValidation<'a, const MAX_BINDINGS: usize> {
    publication: ConfigurationPublication,
    total_bytes: u32,
    reader: PublishedReader,
    validator: Option<ConfigurationStreamValidator<'a, MAX_BINDINGS>>,
    storage: [u8; MAX_MEDIA_CHUNK_BYTES],
    storage_offset: usize,
    storage_len: usize,
    document_offset: u32,
    storage_chunks_read: u32,
    state: ServiceConfigurationState,
    identity: Option<ConfigurationIdentity>,
}

impl<'a, const MAX_BINDINGS: usize> ServiceConfigurationValidation<'a, MAX_BINDINGS> {
    /// Opens the exact typed publication but does not yet transfer a command.
    pub async fn open<D>(
        cache: &mut ProvisionedCache<D>,
        package: &'a BoardPackage<'a>,
        publication: ConfigurationPublication,
    ) -> Result<Self, ConfigurationTransferError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        publication
            .validate()
            .map_err(ConfigurationTransferError::Request)?;
        let total_bytes = publication
            .byte_len()
            .map_err(ConfigurationTransferError::Request)?;
        let reader = cache
            .open_published(publication.publication)
            .await
            .map_err(ConfigurationTransferError::Storage)?;
        let validator =
            ConfigurationStreamValidator::new(package, publication.digest(), total_bytes)
                .map_err(ConfigurationTransferError::Configuration)?;
        Ok(Self {
            publication,
            total_bytes,
            reader,
            validator: Some(validator),
            storage: [0; MAX_MEDIA_CHUNK_BYTES],
            storage_offset: 0,
            storage_len: 0,
            document_offset: 0,
            storage_chunks_read: 0,
            state: ServiceConfigurationState::Begin,
            identity: None,
        })
    }

    /// Advances at most one SD read and returns at most one ordered core command.
    pub async fn next<D>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
    ) -> Result<Option<CoreConfigurationCommand>, ConfigurationTransferError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        let result = self.next_inner(cache).await;
        if result.is_err() {
            self.state = ServiceConfigurationState::Faulted;
            self.validator = None;
            self.identity = None;
            self.storage.fill(0);
            self.storage_offset = 0;
            self.storage_len = 0;
        }
        result
    }

    /// Exact current status without media I/O.
    pub const fn status(&self) -> ServiceConfigurationStatus {
        ServiceConfigurationStatus {
            state: self.state,
            transaction_id: self.publication.transaction_id,
            digest: self.publication.digest(),
            total_bytes: self.total_bytes,
            validated_bytes: self.document_offset,
            storage_chunks_read: self.storage_chunks_read,
            identity: self.identity,
        }
    }

    async fn next_inner<D>(
        &mut self,
        cache: &mut ProvisionedCache<D>,
    ) -> Result<Option<CoreConfigurationCommand>, ConfigurationTransferError<D::Error>>
    where
        D: AsyncBlockDevice,
    {
        loop {
            match self.state {
                ServiceConfigurationState::Begin => {
                    self.state = ServiceConfigurationState::Streaming;
                    return CoreConfigurationCommand::begin(
                        self.publication.transaction_id,
                        self.publication.digest(),
                        self.total_bytes,
                    )
                    .map(Some)
                    .map_err(ConfigurationTransferError::CoreWire);
                }
                ServiceConfigurationState::Streaming => {
                    if self.storage_offset < self.storage_len {
                        let count = (self.storage_len - self.storage_offset)
                            .min(MAX_CORE_CONFIGURATION_DATA_BYTES);
                        let end = self
                            .storage_offset
                            .checked_add(count)
                            .ok_or(ConfigurationTransferError::State)?;
                        let bytes = &self.storage[self.storage_offset..end];
                        self.validator
                            .as_mut()
                            .ok_or(ConfigurationTransferError::State)?
                            .push(bytes)
                            .map_err(ConfigurationTransferError::Configuration)?;
                        let command = CoreConfigurationCommand::data(
                            self.publication.transaction_id,
                            self.publication.digest(),
                            self.total_bytes,
                            self.document_offset,
                            bytes,
                        )
                        .map_err(ConfigurationTransferError::CoreWire)?;
                        self.storage_offset = end;
                        self.document_offset = self
                            .document_offset
                            .checked_add(
                                u32::try_from(count)
                                    .map_err(|_| ConfigurationTransferError::State)?,
                            )
                            .ok_or(ConfigurationTransferError::State)?;
                        return Ok(Some(command));
                    }

                    self.storage.fill(0);
                    self.storage_offset = 0;
                    self.storage_len = 0;
                    if self.reader.is_complete() {
                        if self.document_offset != self.total_bytes {
                            return Err(ConfigurationTransferError::State);
                        }
                        let identity = self
                            .validator
                            .take()
                            .ok_or(ConfigurationTransferError::State)?
                            .finish()
                            .map_err(ConfigurationTransferError::Configuration)?;
                        self.identity = Some(identity);
                        self.state = ServiceConfigurationState::Finish;
                        continue;
                    }
                    let chunk = cache
                        .read_next_published(&mut self.reader, &mut self.storage)
                        .await
                        .map_err(ConfigurationTransferError::Storage)?
                        .ok_or(ConfigurationTransferError::State)?;
                    self.storage_len = usize::try_from(chunk.byte_len)
                        .map_err(|_| ConfigurationTransferError::State)?;
                    if self.storage_len == 0 || self.storage_len > self.storage.len() {
                        return Err(ConfigurationTransferError::State);
                    }
                    self.storage_chunks_read = self
                        .storage_chunks_read
                        .checked_add(1)
                        .ok_or(ConfigurationTransferError::State)?;
                }
                ServiceConfigurationState::Finish => {
                    self.state = ServiceConfigurationState::Complete;
                    return CoreConfigurationCommand::finish(
                        self.publication.transaction_id,
                        self.publication.digest(),
                        self.total_bytes,
                    )
                    .map(Some)
                    .map_err(ConfigurationTransferError::CoreWire);
                }
                ServiceConfigurationState::Complete => return Ok(None),
                ServiceConfigurationState::Faulted => {
                    return Err(ConfigurationTransferError::State);
                }
            }
        }
    }
}

/// Storage, canonical validation, framing, or lifecycle error during core-0 transfer.
#[derive(Debug)]
pub enum ConfigurationTransferError<E> {
    Request(ConfigurationRequestError),
    Configuration(ConfigurationError),
    CoreWire(CoreConfigurationWireError),
    Storage(ProvisionedCacheError<E>),
    State,
}

/// Canonical syntax, identity, resource, or machine-policy rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationError {
    Capability(CapabilityError),
    Length,
    Magic,
    Version,
    Reserved,
    Noncanonical,
    Flags,
    RecordCount,
    RecordKind,
    RecordOrder,
    Selector,
    ResourceEncoding,
    UnknownResource(ResourceId),
    DuplicateResource(ResourceId),
    ResourceKind,
    Owner,
    Ownership,
    Polarity,
    Binding,
    BindingCapacity,
    Timing,
    Frequency,
    ElectricalConstraint,
    HazardousBinding,
    SafeState,
    Rational,
    Uncertainty,
    Scalar,
    Evidence,
    CapabilityIdentity,
    ConfigurationIdentity,
    IncompleteAxis,
    AxisCount,
    AxisRange,
    MotionPolicy,
    Internal,
}

fn find_resource(package: &BoardPackage<'_>, resource: ResourceId) -> Option<ResourceDescriptor> {
    package
        .board
        .resources
        .iter()
        .find(|descriptor| descriptor.id == resource)
        .copied()
}

const fn role_accepts_resource(role: BindingRole, resource: ResourceId) -> bool {
    match role {
        BindingRole::AxisStep | BindingRole::WaveformOutput => matches!(
            resource,
            ResourceId::Gpio(_)
                | ResourceId::I2sOut { .. }
                | ResourceId::Rmt(_)
                | ResourceId::TimedOutput { .. }
        ),
        BindingRole::AxisDirection
        | BindingRole::AxisEnable
        | BindingRole::DigitalOutput
        | BindingRole::FocEnable
        | BindingRole::ProcessOutput => matches!(
            resource,
            ResourceId::Gpio(_) | ResourceId::I2sOut { .. } | ResourceId::TimedOutput { .. }
        ),
        BindingRole::AxisLimitMinimum
        | BindingRole::AxisLimitMaximum
        | BindingRole::AxisEncoderA
        | BindingRole::AxisEncoderB
        | BindingRole::AxisEncoderIndex
        | BindingRole::AxisMotorFault
        | BindingRole::Probe
        | BindingRole::EmergencyStop
        | BindingRole::SafetyInterlock
        | BindingRole::DigitalInput
        | BindingRole::FocFault
        | BindingRole::CaptureInput => {
            matches!(resource, ResourceId::Gpio(_) | ResourceId::SafetyInput(_))
        }
        BindingRole::AnalogInput
        | BindingRole::FocCurrentA
        | BindingRole::FocCurrentB
        | BindingRole::FocCurrentC
        | BindingRole::FocBusVoltage => matches!(resource, ResourceId::Adc { .. }),
        BindingRole::PwmOutput
        | BindingRole::FocPhaseU
        | BindingRole::FocPhaseV
        | BindingRole::FocPhaseW => {
            matches!(
                resource,
                ResourceId::TimedOutput { .. } | ResourceId::Rmt(_)
            )
        }
        BindingRole::SerialPort => matches!(resource, ResourceId::Uart(_)),
        BindingRole::Timer => matches!(resource, ResourceId::Timer { .. }),
        BindingRole::Counter => matches!(resource, ResourceId::Pcnt(_)),
        BindingRole::FocEncoder => {
            matches!(resource, ResourceId::Pcnt(_) | ResourceId::Device(_))
        }
        BindingRole::Storage => matches!(resource, ResourceId::Storage(_)),
        BindingRole::I2cBus => matches!(resource, ResourceId::I2c(_)),
        BindingRole::SpiBus => matches!(resource, ResourceId::Spi(_)),
        BindingRole::TwaiBus => matches!(resource, ResourceId::Twai(_)),
        BindingRole::FittedDevice => matches!(resource, ResourceId::Device(_)),
    }
}

fn constraint_applies(constraint: ResourceId, bound: ResourceId) -> bool {
    constraint == bound
        || matches!(
            (constraint, bound),
            (
                ResourceId::I2s(engine),
                ResourceId::I2sOut {
                    engine: bound_engine,
                    ..
                }
            ) if engine == bound_engine
        )
}

const fn owner_wire(owner: OwnerDomain) -> u8 {
    match owner {
        OwnerDomain::Service => 1,
        OwnerDomain::Realtime => 2,
    }
}

const fn owner_from_wire(value: u8) -> Option<OwnerDomain> {
    match value {
        1 => Some(OwnerDomain::Service),
        2 => Some(OwnerDomain::Realtime),
        _ => None,
    }
}

const fn role_bit(role: BindingRole) -> u32 {
    let bit = (role as u16).saturating_sub(1);
    if bit < 32 { 1_u32 << bit } else { 0 }
}

const fn fact_bit(fact: ScalarFact) -> u32 {
    1_u32 << ((fact as u16) - 1)
}

const fn gcd(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

const fn read_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

const fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

const fn read_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

#[cfg(test)]
mod tests {
    extern crate alloc;

    use super::*;
    use alloc::rc::Rc;
    use alloc::vec;
    use alloc::vec::Vec;
    use alumina_storage::media::{MEDIA_BLOCK_BYTES, MediaBlock, MediaId, MediaRegion};
    use alumina_storage::provisioning::CacheProvisionRequest;
    use alumina_storage::{
        CacheLimits, ChunkUploadHeader, FinalizeUploadRequest, ManifestHasher, MutationContext,
        UploadId, UploadPlan, sha256,
    };
    use core::cell::RefCell;
    use embassy_futures::block_on;

    const TEST_DEVICE_BLOCKS: usize = 2_300;
    const TEST_REGION: MediaRegion = MediaRegion {
        start_block: 2_048,
        block_count: 200,
    };
    const TEST_CACHE_LIMITS: CacheLimits = CacheLimits {
        maximum_object_bytes: 64 * 1_024,
        maximum_chunk_bytes: 1_024,
        maximum_chunks: 128,
    };

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum TestDeviceError {
        OutsideDevice,
    }

    struct RamBlockDevice {
        blocks: Rc<RefCell<Vec<MediaBlock>>>,
    }

    impl RamBlockDevice {
        fn erased() -> Self {
            Self {
                blocks: Rc::new(RefCell::new(vec![
                    [0xff; MEDIA_BLOCK_BYTES];
                    TEST_DEVICE_BLOCKS
                ])),
            }
        }
    }

    impl AsyncBlockDevice for RamBlockDevice {
        type Error = TestDeviceError;

        fn block_count(&self) -> u64 {
            u64::try_from(self.blocks.borrow().len()).unwrap()
        }

        async fn read_block(
            &mut self,
            block: u64,
            output: &mut MediaBlock,
        ) -> Result<(), Self::Error> {
            let index = usize::try_from(block).map_err(|_| TestDeviceError::OutsideDevice)?;
            let blocks = self.blocks.borrow();
            let source = blocks.get(index).ok_or(TestDeviceError::OutsideDevice)?;
            output.copy_from_slice(source);
            Ok(())
        }

        async fn write_block(&mut self, block: u64, data: &MediaBlock) -> Result<(), Self::Error> {
            let index = usize::try_from(block).map_err(|_| TestDeviceError::OutsideDevice)?;
            let mut blocks = self.blocks.borrow_mut();
            let target = blocks
                .get_mut(index)
                .ok_or(TestDeviceError::OutsideDevice)?;
            target.copy_from_slice(data);
            Ok(())
        }

        async fn sync(&mut self) -> Result<(), Self::Error> {
            Ok(())
        }
    }

    fn rational(numerator: i64, denominator: u64) -> Rational {
        Rational::new(numerator, denominator).unwrap()
    }

    fn binding(
        instance: u16,
        role: BindingRole,
        resource: ResourceId,
        polarity: SignalPolarity,
        timed: bool,
    ) -> ConfigurationRecord {
        ConfigurationRecord::Binding(ResourceBinding {
            instance,
            role,
            resource,
            owner: OwnerDomain::Realtime,
            polarity,
            flags: BindingFlags::default(),
            minimum_active_cycles: u32::from(timed) * 48,
            minimum_inactive_cycles: u32::from(timed) * 48,
            maximum_frequency_hz: u32::from(timed) * 100_000,
            watchdog_cycles: if timed || role.is_hazardous_role() {
                240_000
            } else {
                0
            },
        })
    }

    fn scalar(instance: u16, fact: ScalarFact, value: Rational) -> ConfigurationRecord {
        ConfigurationRecord::Scalar(ExactScalar {
            instance,
            fact,
            value,
            uncertainty: rational(0, 1),
            evidence: FactEvidence::Declared,
        })
    }

    fn tinybee_motion_records() -> Vec<ConfigurationRecord> {
        let mut records = Vec::from([
            binding(
                0,
                BindingRole::AxisStep,
                ResourceId::I2sOut { engine: 0, bit: 2 },
                SignalPolarity::ActiveHigh,
                true,
            ),
            binding(
                0,
                BindingRole::AxisDirection,
                ResourceId::I2sOut { engine: 0, bit: 1 },
                SignalPolarity::ActiveHigh,
                false,
            ),
            binding(
                0,
                BindingRole::AxisEnable,
                ResourceId::I2sOut { engine: 0, bit: 0 },
                SignalPolarity::ActiveHigh,
                false,
            ),
            binding(
                0,
                BindingRole::EmergencyStop,
                ResourceId::Gpio(33),
                SignalPolarity::ActiveLow,
                false,
            ),
        ]);
        records.extend([
            scalar(0, ScalarFact::AxisFullStepsPerTurn, rational(200, 1)),
            scalar(0, ScalarFact::AxisMicrosteps, rational(16, 1)),
            scalar(0, ScalarFact::AxisMotorTurnsPerOutputTurn, rational(1, 1)),
            scalar(
                0,
                ScalarFact::AxisTravelMetresPerOutputTurn,
                rational(1, 500),
            ),
            scalar(0, ScalarFact::AxisCalibrationScale, rational(1, 1)),
            scalar(0, ScalarFact::AxisPositionMinimumMetres, rational(0, 1)),
            scalar(0, ScalarFact::AxisPositionMaximumMetres, rational(3, 10)),
            scalar(
                0,
                ScalarFact::AxisVelocityLimitMetresPerSecond,
                rational(1, 20),
            ),
            scalar(
                0,
                ScalarFact::AxisAccelerationLimitMetresPerSecondSquared,
                rational(1, 2),
            ),
            scalar(
                0,
                ScalarFact::AxisJerkLimitMetresPerSecondCubed,
                rational(5, 1),
            ),
        ]);
        records.sort_by_key(|record| record.key());
        records
    }

    fn document(
        package: &BoardPackage<'_>,
        records: &[ConfigurationRecord],
        flags: ConfigurationFlags,
    ) -> (Vec<u8>, Digest) {
        let realtime = records
            .iter()
            .filter(|record| record.realtime_relevant())
            .count();
        let header = ConfigurationHeader {
            capability_digest: package.board.capability_digest,
            record_count: u16::try_from(records.len()).unwrap(),
            realtime_record_count: u16::try_from(realtime).unwrap(),
            flags,
        };
        let mut bytes = Vec::from(header.encode().unwrap());
        for record in records {
            bytes.extend_from_slice(&record.encode().unwrap());
        }
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&hasher.finalize());
        (bytes, Digest(digest))
    }

    fn upload_plan(bytes: &[u8], chunk_bytes: usize) -> UploadPlan {
        let object = StoredObject {
            kind: ObjectKind::MachineConfiguration,
            content: sha256(bytes),
            byte_len: u64::try_from(bytes.len()).unwrap(),
        };
        let chunk_count = bytes.len().div_ceil(chunk_bytes);
        let mut manifest = ManifestHasher::new(
            object,
            u32::try_from(chunk_bytes).unwrap(),
            u32::try_from(chunk_count).unwrap(),
            TEST_CACHE_LIMITS,
        )
        .unwrap();
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            manifest
                .push(
                    u32::try_from(index).unwrap(),
                    sha256(chunk),
                    u32::try_from(chunk.len()).unwrap(),
                )
                .unwrap();
        }
        UploadPlan {
            upload_id: UploadId(0x1234_9876),
            object,
            manifest: manifest.finalize().unwrap(),
            chunk_bytes: u32::try_from(chunk_bytes).unwrap(),
            chunk_count: u32::try_from(chunk_count).unwrap(),
        }
    }

    fn provisioned_configuration(
        bytes: &[u8],
        chunk_bytes: usize,
    ) -> (ProvisionedCache<RamBlockDevice>, ConfigurationPublication) {
        let mut cache = ProvisionedCache::new(RamBlockDevice::erased(), TEST_CACHE_LIMITS);
        block_on(cache.discover()).unwrap();
        let request = CacheProvisionRequest::new(
            u64::try_from(TEST_DEVICE_BLOCKS).unwrap(),
            0,
            None,
            TEST_REGION,
            MediaId::new([0x5a; 16]).unwrap(),
            false,
        )
        .unwrap();
        block_on(cache.provision(request, MutationContext::DISARMED_IDLE)).unwrap();
        let plan = upload_plan(bytes, chunk_bytes);
        block_on(cache.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        for (index, chunk) in bytes.chunks(chunk_bytes).enumerate() {
            block_on(cache.put_chunk(
                ChunkUploadHeader {
                    upload_id: plan.upload_id,
                    index: u32::try_from(index).unwrap(),
                    byte_len: u32::try_from(chunk.len()).unwrap(),
                    content: sha256(chunk),
                },
                chunk,
                MutationContext::DISARMED_IDLE,
            ))
            .unwrap();
        }
        let publication = block_on(cache.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        (
            cache,
            ConfigurationPublication {
                transaction_id: 0x7788,
                publication,
            },
        )
    }

    #[test]
    fn exact_rationals_and_records_reject_noncanonical_forms() {
        assert_eq!(Rational::new(2, 4), Err(ConfigurationError::Rational));
        assert_eq!(Rational::new(0, 2), Err(ConfigurationError::Rational));
        assert_eq!(rational(i64::MIN, u64::MAX).denominator, u64::MAX);

        let record = scalar(0, ScalarFact::AxisMicrosteps, rational(16, 1));
        let encoded = record.encode().unwrap();
        assert_eq!(ConfigurationRecord::decode(&encoded), Ok(record));
        let mut reserved = encoded;
        reserved[63] = 1;
        assert_eq!(
            ConfigurationRecord::decode(&reserved),
            Err(ConfigurationError::Reserved)
        );
    }

    #[test]
    fn tinybee_motion_document_validates_at_every_chunk_split() {
        let records = tinybee_motion_records();
        let flags = ConfigurationFlags(ConfigurationFlags::MOTION);
        let (bytes, digest) = document(&board_mks_tinybee::PACKAGE, &records, flags);
        for chunk_bytes in [1, 7, 63, 64, 79, 80, 127, 511] {
            let mut validator = ConfigurationStreamValidator::<32>::new(
                &board_mks_tinybee::PACKAGE,
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap();
            for chunk in bytes.chunks(chunk_bytes) {
                validator.push(chunk).unwrap();
            }
            let identity = validator.finish().unwrap();
            assert_eq!(identity.digest, digest);
            assert_eq!(identity.summary.stepper_axes, 1);
            assert_eq!(identity.summary.foc_axes, 0);
            assert!(identity.summary.safety_binding);
        }
    }

    #[test]
    fn board_ownership_constraints_duplicates_and_capability_fail_closed() {
        let records = tinybee_motion_records();
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let mut wrong_board = ConfigurationStreamValidator::<32>::new(
            &board_t_deck_pro::PACKAGE,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        assert_eq!(
            wrong_board.push(&bytes),
            Err(ConfigurationError::CapabilityIdentity)
        );

        let duplicate = ConfigurationRecord::Binding(ResourceBinding {
            instance: 1,
            role: BindingRole::DigitalOutput,
            resource: ResourceId::I2sOut { engine: 0, bit: 2 },
            owner: OwnerDomain::Realtime,
            polarity: SignalPolarity::ActiveHigh,
            flags: BindingFlags::default(),
            minimum_active_cycles: 0,
            minimum_inactive_cycles: 0,
            maximum_frequency_hz: 0,
            watchdog_cycles: 0,
        });
        let header = ConfigurationHeader {
            capability_digest: board_mks_tinybee::PACKAGE.board.capability_digest,
            record_count: 2,
            realtime_record_count: 2,
            flags: ConfigurationFlags::default(),
        };
        let mut validator =
            ConfigurationValidator::<2>::new(&board_mks_tinybee::PACKAGE, header).unwrap();
        validator.push(records[0]).unwrap();
        assert_eq!(
            validator.push(duplicate),
            Err(ConfigurationError::HazardousBinding)
        );

        let header = ConfigurationHeader {
            capability_digest: board_mks_tinybee::PACKAGE.board.capability_digest,
            record_count: 2,
            realtime_record_count: 2,
            flags: ConfigurationFlags::default(),
        };
        let mut validator =
            ConfigurationValidator::<2>::new(&board_mks_tinybee::PACKAGE, header).unwrap();
        validator
            .push(binding(
                0,
                BindingRole::EmergencyStop,
                ResourceId::Gpio(33),
                SignalPolarity::ActiveLow,
                false,
            ))
            .unwrap();
        assert_eq!(
            validator.push(binding(
                0,
                BindingRole::SafetyInterlock,
                ResourceId::Gpio(33),
                SignalPolarity::ActiveLow,
                false,
            )),
            Err(ConfigurationError::DuplicateResource(ResourceId::Gpio(33)))
        );

        let header = ConfigurationHeader {
            capability_digest: board_mks_tinybee::PACKAGE.board.capability_digest,
            record_count: 1,
            realtime_record_count: 1,
            flags: ConfigurationFlags::default(),
        };
        let mut validator =
            ConfigurationValidator::<1>::new(&board_mks_tinybee::PACKAGE, header).unwrap();
        assert_eq!(
            validator.push(binding(
                0,
                BindingRole::DigitalOutput,
                ResourceId::Gpio(34),
                SignalPolarity::ActiveHigh,
                false,
            )),
            Err(ConfigurationError::ElectricalConstraint)
        );
    }

    #[test]
    fn incomplete_axis_wrong_range_and_wrong_digest_are_rejected() {
        let mut records = tinybee_motion_records();
        records.retain(|record| {
            !matches!(
                record,
                ConfigurationRecord::Scalar(ExactScalar {
                    fact: ScalarFact::AxisJerkLimitMetresPerSecondCubed,
                    ..
                })
            )
        });
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &board_mks_tinybee::PACKAGE,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::IncompleteAxis));

        let mut records = tinybee_motion_records();
        for record in &mut records {
            if let ConfigurationRecord::Scalar(scalar) = record
                && scalar.fact == ScalarFact::AxisPositionMaximumMetres
            {
                scalar.value = rational(0, 1);
            }
        }
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &board_mks_tinybee::PACKAGE,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::AxisRange));

        let records = tinybee_motion_records();
        let (bytes, _) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &board_mks_tinybee::PACKAGE,
            Digest([0x55; 32]),
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(
            validator.finish(),
            Err(ConfigurationError::ConfigurationIdentity)
        );
    }

    #[test]
    fn intercore_commands_bind_identity_offset_length_and_unused_bytes() {
        let digest = Digest([0x5a; 32]);
        let total_bytes =
            u32::try_from(CONFIGURATION_HEADER_BYTES + 3 * CONFIGURATION_RECORD_BYTES).unwrap();
        let begin = CoreConfigurationCommand::begin(7, digest, total_bytes).unwrap();
        assert_eq!(
            CoreConfigurationCommand::decode(begin.encode().unwrap().as_bytes()),
            Ok(begin)
        );

        let data = CoreConfigurationCommand::data(
            7,
            digest,
            total_bytes,
            u32::try_from(CONFIGURATION_HEADER_BYTES).unwrap(),
            &[0xa5; MAX_CORE_CONFIGURATION_DATA_BYTES],
        )
        .unwrap();
        let encoded = data.encode().unwrap();
        assert_eq!(
            encoded.as_bytes().len(),
            CORE_CONFIGURATION_COMMAND_CAPACITY
        );
        assert_eq!(
            CoreConfigurationCommand::decode(encoded.as_bytes()),
            Ok(data)
        );
        assert_eq!(
            data.data_bytes(),
            &[0xa5; MAX_CORE_CONFIGURATION_DATA_BYTES]
        );

        let mut reserved = *encoded
            .as_bytes()
            .first_chunk::<CORE_CONFIGURATION_COMMAND_CAPACITY>()
            .unwrap();
        reserved[63] = 1;
        assert_eq!(
            CoreConfigurationCommand::decode(&reserved),
            Err(CoreConfigurationWireError::Reserved)
        );
        assert_eq!(
            CoreConfigurationCommand::data(7, digest, total_bytes, 0, &[]),
            Err(CoreConfigurationWireError::DataLength)
        );
        assert!(CoreConfigurationCommand::finish(7, digest, total_bytes).is_ok());
        assert!(CoreConfigurationCommand::activate(7, digest, total_bytes).is_ok());
        assert!(CoreConfigurationCommand::clear(7, digest, total_bytes).is_ok());
        assert!(CoreConfigurationCommand::abort(7, digest, total_bytes).is_ok());
    }

    #[test]
    fn publication_and_selection_requests_are_typed_exact_and_canonical() {
        let publication = ConfigurationPublication {
            transaction_id: 0x1234,
            publication: PublishedObject {
                object: StoredObject {
                    kind: ObjectKind::MachineConfiguration,
                    content: ContentId::from_sha256(Digest([0x45; 32])),
                    byte_len: u64::try_from(
                        CONFIGURATION_HEADER_BYTES + 14 * CONFIGURATION_RECORD_BYTES,
                    )
                    .unwrap(),
                },
                manifest: ContentId::from_sha256(Digest([0x67; 32])),
            },
        };
        assert_eq!(
            ConfigurationPublication::decode(&publication.encode().unwrap()),
            Ok(publication)
        );
        let selection = ConfigurationSelection::from_publication(publication).unwrap();
        assert_eq!(
            ConfigurationSelection::decode(&selection.encode().unwrap()),
            Ok(selection)
        );

        let mut wrong_kind = publication;
        wrong_kind.publication.object.kind = ObjectKind::OpaqueData;
        assert_eq!(
            wrong_kind.encode(),
            Err(ConfigurationRequestError::Identity)
        );
        let mut reserved = publication.encode().unwrap();
        reserved[12] = 1;
        assert_eq!(
            ConfigurationPublication::decode(&reserved),
            Err(ConfigurationRequestError::Reserved)
        );
    }

    #[test]
    fn realtime_reports_are_canonical_and_state_summary_consistent() {
        let summary = ConfigurationSummary {
            record_count: 14,
            realtime_record_count: 14,
            binding_count: 4,
            stepper_axes: 1,
            foc_axes: 0,
            safety_binding: true,
            flags: ConfigurationFlags(ConfigurationFlags::MOTION),
        };
        let report = RealtimeConfigurationReport {
            state: RealtimeConfigurationState::CandidateValid,
            transaction_id: 9,
            digest: Digest([0x44; 32]),
            total_bytes: u32::try_from(
                CONFIGURATION_HEADER_BYTES + 14 * CONFIGURATION_RECORD_BYTES,
            )
            .unwrap(),
            consumed_bytes: u32::try_from(
                CONFIGURATION_HEADER_BYTES + 14 * CONFIGURATION_RECORD_BYTES,
            )
            .unwrap(),
            summary: Some(summary),
            fault: ConfigurationFaultCode::None,
            active_digest: Digest::ZERO,
            active_bytes: 0,
        };
        let encoded = report.encode().unwrap();
        assert_eq!(RealtimeConfigurationReport::decode(&encoded), Ok(report));

        let mut malformed = encoded;
        malformed[70] = 0;
        assert_eq!(
            RealtimeConfigurationReport::decode(&malformed),
            Err(CoreConfigurationReportError::Summary)
        );
        let rejected = RealtimeConfigurationReport {
            state: RealtimeConfigurationState::Rejected,
            summary: None,
            fault: ConfigurationFaultCode::Electrical,
            ..report
        };
        assert_eq!(
            RealtimeConfigurationReport::decode(&rejected.encode().unwrap()),
            Ok(rejected)
        );
    }

    #[test]
    fn realtime_service_rehashes_validates_activates_and_clears_exact_stream() {
        let records = tinybee_motion_records();
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let total = u32::try_from(bytes.len()).unwrap();
        let mut service = RealtimeConfigurationService::<32>::new(&board_mks_tinybee::PACKAGE);
        let report = service.apply(
            CoreConfigurationCommand::begin(41, digest, total).unwrap(),
            true,
        );
        assert_eq!(report.state, RealtimeConfigurationState::Receiving);

        let mut offset = 0_u32;
        for chunk in bytes.chunks(MAX_CORE_CONFIGURATION_DATA_BYTES) {
            let report = service.apply(
                CoreConfigurationCommand::data(41, digest, total, offset, chunk).unwrap(),
                true,
            );
            offset += u32::try_from(chunk.len()).unwrap();
            assert_eq!(report.consumed_bytes, offset);
        }
        let report = service.apply(
            CoreConfigurationCommand::finish(41, digest, total).unwrap(),
            true,
        );
        assert_eq!(report.state, RealtimeConfigurationState::CandidateValid);
        assert_eq!(service.candidate_identity().unwrap().digest, digest);
        let report = service.apply(
            CoreConfigurationCommand::activate(41, digest, total).unwrap(),
            true,
        );
        assert_eq!(report.state, RealtimeConfigurationState::Active);
        assert_eq!(service.active_identity().unwrap().digest, digest);

        let rejected = service.apply(
            CoreConfigurationCommand::begin(42, Digest([0x77; 32]), total).unwrap(),
            false,
        );
        assert_eq!(rejected.state, RealtimeConfigurationState::Rejected);
        assert_eq!(rejected.fault, ConfigurationFaultCode::ForbiddenState);
        assert_eq!(rejected.active_digest, digest);
        assert_eq!(rejected.active_bytes, total);
        assert_eq!(service.active_identity().unwrap().digest, digest);

        let cleared = service.apply(
            CoreConfigurationCommand::clear(43, digest, total).unwrap(),
            true,
        );
        assert_eq!(cleared.state, RealtimeConfigurationState::Cleared);
        assert!(service.active_identity().is_none());
        assert_eq!(service.report().state, RealtimeConfigurationState::Empty);
    }

    #[test]
    fn realtime_service_rejects_gap_corruption_and_activation_before_finish() {
        let records = tinybee_motion_records();
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let total = u32::try_from(bytes.len()).unwrap();
        let mut service = RealtimeConfigurationService::<32>::new(&board_mks_tinybee::PACKAGE);
        service.apply(
            CoreConfigurationCommand::begin(51, digest, total).unwrap(),
            true,
        );
        let gap = service.apply(
            CoreConfigurationCommand::data(51, digest, total, 1, &bytes[..64]).unwrap(),
            true,
        );
        assert_eq!(gap.fault, ConfigurationFaultCode::Sequence);

        service.apply(
            CoreConfigurationCommand::begin(52, digest, total).unwrap(),
            true,
        );
        let early = service.apply(
            CoreConfigurationCommand::activate(52, digest, total).unwrap(),
            true,
        );
        assert_eq!(early.fault, ConfigurationFaultCode::Sequence);

        let mut corrupt = bytes.clone();
        corrupt[56] = 1;
        service.apply(
            CoreConfigurationCommand::begin(53, digest, total).unwrap(),
            true,
        );
        let rejected = service.apply(
            CoreConfigurationCommand::data(
                53,
                digest,
                total,
                0,
                &corrupt[..MAX_CORE_CONFIGURATION_DATA_BYTES],
            )
            .unwrap(),
            true,
        );
        assert_eq!(rejected.state, RealtimeConfigurationState::Rejected);
        assert_eq!(rejected.fault, ConfigurationFaultCode::Syntax);
    }

    #[test]
    fn published_actor_and_realtime_actor_validate_identical_sd_bytes() {
        let records = tinybee_motion_records();
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let (mut cache, publication) = provisioned_configuration(&bytes, 173);
        assert_eq!(publication.digest(), digest);
        let mut service = block_on(ServiceConfigurationValidation::<32>::open(
            &mut cache,
            &board_mks_tinybee::PACKAGE,
            publication,
        ))
        .unwrap();
        let mut realtime = RealtimeConfigurationService::<32>::new(&board_mks_tinybee::PACKAGE);
        let mut commands = 0_u32;
        while let Some(command) = block_on(service.next(&mut cache)).unwrap() {
            commands += 1;
            let report = realtime.apply(command, true);
            assert_ne!(report.state, RealtimeConfigurationState::Rejected);
        }
        let service_status = service.status();
        assert_eq!(service_status.state, ServiceConfigurationState::Complete);
        assert_eq!(
            service_status.validated_bytes,
            u32::try_from(bytes.len()).unwrap()
        );
        assert_eq!(service_status.storage_chunks_read, 6);
        assert!(commands > service_status.storage_chunks_read);
        let service_identity = service_status.identity.unwrap();
        let realtime_identity = realtime.candidate_identity().unwrap();
        assert_eq!(service_identity, realtime_identity);
        assert_eq!(service_identity.digest, digest);
        assert_eq!(
            realtime.report().state,
            RealtimeConfigurationState::CandidateValid
        );
    }
}
