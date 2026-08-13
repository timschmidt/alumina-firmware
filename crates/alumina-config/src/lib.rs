#![no_std]
#![doc = "Canonical streaming machine configuration and board-resource validation for Alumina."]

use core::cmp::Ordering;

use alumina_board::{
    BoardPackage, BusKind, ElectricalConstraintKind, OwnerDomain, ResourceDescriptor, ResourceId,
    SafeValue, SupportLevel,
};
use alumina_capability::{
    CapabilityError, decode_resource_id, encode_resource_id, verify_declared_identity,
};
use alumina_foc::{
    CountUncertainty, CurrentChannelCalibration, CurrentPolarity, ElectricalPhase,
    FocTimingProfile, PiConfig, Q30, Q30Interval, RotationPrecision, RotorCountDirection,
    TwoShuntPhasePair,
};
use alumina_protocol::Digest;
use alumina_safety::{
    InputBias, InputPolarity, MAX_SAFETY_INPUTS, SafetyInputRole, SafetyInputSpec,
};
use alumina_storage::media::{AsyncBlockDevice, MAX_MEDIA_CHUNK_BYTES, PublishedReader};
use alumina_storage::provisioning::{ProvisionedCache, ProvisionedCacheError};
use alumina_storage::{ContentId, ObjectKind, PublishedObject, StoredObject};
use sha2::{Digest as ShaDigest, Sha256};

mod foc;

pub use foc::{
    FocAdcAttenuation, FocAdcFrontendParameters, FocControllerAxis, FocControllerParameters,
    FocCurrentChannel, FocCurrentChannelParameters, FocPwmAdcTimingParameters,
    FocPwmHardwareParameters, FocRotorParameters, FocRuntimeParameters,
    LoweredFocAxisConfiguration,
};

/// Exact machine-configuration schema version.
pub const CONFIGURATION_VERSION: u16 = 5;
/// Bytes in the fixed canonical document header.
pub const CONFIGURATION_HEADER_BYTES: usize = 80;
/// Bytes in every V5 configuration record.
pub const CONFIGURATION_RECORD_BYTES: usize = 64;
/// Schema-wide bound independent of a board's smaller admission budget.
pub const MAX_CONFIGURATION_RECORDS: usize = 256;
/// Initial maximum logical motion-axis index plus one.
pub const MAX_AXIS_INSTANCES: usize = 16;
/// Maximum step/direction axes in one executable per-MCU stream.
pub const MAX_EXECUTABLE_STEPPER_AXES: usize = 8;
/// Maximum FOC axes whose complete hardware contract is retained on core 1.
pub const MAX_EXECUTABLE_FOC_AXES: usize = 4;

const DOCUMENT_MAGIC: [u8; 8] = *b"ALMCFG05";
const RECORD_KIND_BINDING: u16 = 1;
const RECORD_KIND_SCALAR: u16 = 2;
const RECORD_KIND_FOC_SHUTDOWN: u16 = 3;
const RECORD_KIND_FOC_RUNTIME: u16 = 4;
const RECORD_KIND_FOC_CONTROLLER: u16 = 5;
const RECORD_KIND_FOC_ROTOR: u16 = 6;
const RECORD_KIND_FOC_CURRENT_CHANNEL: u16 = 7;
const RECORD_KIND_FOC_PWM_ADC_TIMING: u16 = 8;
const RECORD_KIND_FOC_ADC_FRONTEND: u16 = 9;
const RECORD_KIND_FOC_PWM_HARDWARE: u16 = 10;
const PUBLICATION_MAGIC: [u8; 8] = *b"ALMCFQ01";
const SELECTION_MAGIC: [u8; 8] = *b"ALMCFS01";
const COORDINATOR_STATUS_MAGIC: [u8; 8] = *b"ALMCST01";

/// Exact `ConfigurationValidate` body bytes.
pub const CONFIGURATION_PUBLICATION_BYTES: usize = 96;
/// Exact `ConfigurationCommit`/`ConfigurationRollback` selection bytes.
pub const CONFIGURATION_SELECTION_BYTES: usize = 64;
/// Exact authenticated `ConfigurationGet` and lifecycle response body.
pub const CONFIGURATION_COORDINATOR_STATUS_BYTES: usize = 264;

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

    /// Decodes only the exact V5 SHA-256/configuration representation.
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
    /// All V5 flags.
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

    /// Encodes the exact V5 header.
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

    /// Decodes only the exact canonical V5 header.
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
    FocFault = 29,
    ProcessOutput = 30,
    Storage = 31,
    I2cBus = 32,
    SpiBus = 33,
    TwaiBus = 34,
    CaptureInput = 35,
    WaveformOutput = 36,
    FittedDevice = 37,
    /// Active level disables the stepper driver. This is distinct from an
    /// active-level `AxisEnable` and prevents silent polarity inversion.
    AxisDisable = 38,
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
            29 => Self::FocFault,
            30 => Self::ProcessOutput,
            31 => Self::Storage,
            32 => Self::I2cBus,
            33 => Self::SpiBus,
            34 => Self::TwaiBus,
            35 => Self::CaptureInput,
            36 => Self::WaveformOutput,
            37 => Self::FittedDevice,
            38 => Self::AxisDisable,
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
                | Self::AxisDisable
                | Self::DigitalOutput
                | Self::PwmOutput
                | Self::FocPhaseU
                | Self::FocPhaseV
                | Self::FocPhaseW
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
                | Self::AxisDisable
                | Self::FocPhaseU
                | Self::FocPhaseV
                | Self::FocPhaseW
                | Self::ProcessOutput
        )
    }

    const fn axis_role(self) -> bool {
        matches!(
            self,
            Self::AxisStep
                | Self::AxisDirection
                | Self::AxisEnable
                | Self::AxisDisable
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
                | Self::FocFault
        )
    }

    const fn safety_input_role(self) -> Option<SafetyInputRole> {
        match self {
            Self::AxisLimitMinimum => Some(SafetyInputRole::AxisLimitMinimum),
            Self::AxisLimitMaximum => Some(SafetyInputRole::AxisLimitMaximum),
            Self::AxisMotorFault => Some(SafetyInputRole::AxisMotorFault),
            Self::Probe => Some(SafetyInputRole::Probe),
            Self::EmergencyStop => Some(SafetyInputRole::EmergencyStop),
            Self::SafetyInterlock => Some(SafetyInputRole::SafetyInterlock),
            Self::FocFault => Some(SafetyInputRole::FocFault),
            _ => None,
        }
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
    /// Dimensionless multiplier applied to nominal commanded step density.
    ///
    /// The browser derives commanded steps per metre as `full_steps *
    /// microsteps * motor_turns_per_output_turn * calibration_scale /
    /// travel_metres_per_output_turn`. Its independent uncertainty remains an
    /// absolute bound on this multiplier.
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
    /// Exact frequency of the `DeviceCycle` domain used by cached motion IR.
    TimerTickHertz = 23,
    /// Smallest stepper-output interval, expressed in `DeviceCycle` ticks.
    StepperOutputQuantumCycles = 24,
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
            24 => Self::StepperOutputQuantumCycles,
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
                | Self::PwmCarrierHertz
                | Self::ControlRateHertz
                | Self::TimerTickHertz
                | Self::StepperOutputQuantumCycles
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

/// Physically distinct ways a real-time owner can remove inverter drive.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(u16)]
pub enum FocShutdownStrategy {
    /// The inactive level of a dedicated enable input turns the stage off.
    DedicatedEnable = 1,
    /// The active level of a dedicated disable input turns the stage off.
    DedicatedDisable = 2,
    /// Releasing every phase pin lets board-local bias select both-off.
    PhaseHighImpedance = 3,
}

impl FocShutdownStrategy {
    const fn from_wire(value: u16) -> Option<Self> {
        match value {
            1 => Some(Self::DedicatedEnable),
            2 => Some(Self::DedicatedDisable),
            3 => Some(Self::PhaseHighImpedance),
            _ => None,
        }
    }
}

/// Qualified, axis-local inverter shutdown contract.
///
/// `power_stage` identifies the fitted device whose qualified topology contains
/// the axis phase resources. Dedicated strategies additionally own `control`.
/// `maximum_transition_cycles` is an inclusive device-cycle bound from the
/// shutdown request until the stage reaches its measured off state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocShutdownContract {
    pub instance: u16,
    pub strategy: FocShutdownStrategy,
    pub power_stage: ResourceId,
    pub control: Option<ResourceId>,
    pub control_polarity: SignalPolarity,
    pub maximum_transition_cycles: u32,
    pub evidence: FactEvidence,
}

/// One fixed-width canonical configuration record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationRecord {
    Binding(ResourceBinding),
    Scalar(ExactScalar),
    FocShutdown(FocShutdownContract),
    FocRuntime(FocRuntimeParameters),
    FocController(FocControllerParameters),
    FocRotor(FocRotorParameters),
    FocCurrentChannel(FocCurrentChannelParameters),
    FocPwmAdcTiming(FocPwmAdcTimingParameters),
    FocAdcFrontend(FocAdcFrontendParameters),
    FocPwmHardware(FocPwmHardwareParameters),
}

impl ConfigurationRecord {
    /// Encodes a fixed 64-byte canonical record.
    pub fn encode(self) -> Result<[u8; CONFIGURATION_RECORD_BYTES], ConfigurationError> {
        self.validate_shape()?;
        let mut encoded = [0_u8; CONFIGURATION_RECORD_BYTES];
        let (kind, instance, selector) = self.canonical_order_key();
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
            Self::FocShutdown(shutdown) => {
                encoded[8..12].copy_from_slice(&encode_resource_id(shutdown.power_stage));
                if let Some(control) = shutdown.control {
                    encoded[12..16].copy_from_slice(&encode_resource_id(control));
                }
                encoded[16..20].copy_from_slice(&shutdown.maximum_transition_cycles.to_le_bytes());
                encoded[20] = shutdown.control_polarity as u8;
                encoded[21] = shutdown.evidence as u8;
                // Bytes 22..64 are reserved zero.
            }
            Self::FocRuntime(runtime) => {
                encoded[8..10].copy_from_slice(&runtime.pole_pairs.to_le_bytes());
                encoded[12..16].copy_from_slice(&runtime.timing.pwm_hz.to_le_bytes());
                encoded[16..20].copy_from_slice(&runtime.timing.current_loop_hz.to_le_bytes());
                encoded[20..22]
                    .copy_from_slice(&runtime.timing.velocity_loop_divider.to_le_bytes());
                encoded[22..24]
                    .copy_from_slice(&runtime.timing.position_loop_divider.to_le_bytes());
                encoded[24..28]
                    .copy_from_slice(&runtime.maximum_phase_current.bits().to_le_bytes());
                encoded[28..32]
                    .copy_from_slice(&runtime.maximum_phase_voltage.bits().to_le_bytes());
                // Bytes 10..12 and 32..64 are reserved zero.
            }
            Self::FocController(controller) => {
                let parameters = controller.parameters;
                encoded[8..12].copy_from_slice(&parameters.proportional_gain.bits().to_le_bytes());
                encoded[12..16]
                    .copy_from_slice(&parameters.integral_gain_per_update.bits().to_le_bytes());
                encoded[16..20].copy_from_slice(&parameters.integral_minimum.bits().to_le_bytes());
                encoded[20..24].copy_from_slice(&parameters.integral_maximum.bits().to_le_bytes());
                encoded[24..28].copy_from_slice(&parameters.output_minimum.bits().to_le_bytes());
                encoded[28..32].copy_from_slice(&parameters.output_maximum.bits().to_le_bytes());
                // Bytes 32..64 are reserved zero.
            }
            Self::FocRotor(rotor) => {
                encoded[8..12].copy_from_slice(&rotor.counts_per_mechanical_turn.to_le_bytes());
                encoded[12..16].copy_from_slice(&rotor.count_at_reference.to_le_bytes());
                encoded[16..20]
                    .copy_from_slice(&rotor.electrical_phase_at_reference.bits().to_le_bytes());
                encoded[20..24].copy_from_slice(&rotor.maximum_alignment_error_bits.to_le_bytes());
                encoded[24..28]
                    .copy_from_slice(&rotor.maximum_count_error.numerator().to_le_bytes());
                encoded[28..32]
                    .copy_from_slice(&rotor.maximum_count_error.denominator().to_le_bytes());
                encoded[32..36].copy_from_slice(
                    &rotor
                        .rotation_precision
                        .maximum_component_width_ulps
                        .to_le_bytes(),
                );
                encoded[36..40].copy_from_slice(
                    &rotor
                        .rotation_precision
                        .maximum_norm_error_ulps
                        .to_le_bytes(),
                );
                encoded[40] = rotor.evidence as u8;
                // Bytes 41..64 are reserved zero.
            }
            Self::FocCurrentChannel(channel) => {
                let calibration = channel.calibration;
                encoded[8..10].copy_from_slice(&calibration.adc_maximum_count.to_le_bytes());
                encoded[10..12].copy_from_slice(&calibration.valid_count_minimum.to_le_bytes());
                encoded[12..14].copy_from_slice(&calibration.valid_count_maximum.to_le_bytes());
                encoded[14..16].copy_from_slice(&calibration.count_at_zero.to_le_bytes());
                encoded[16] = current_polarity_wire(calibration.polarity);
                encoded[17] = channel.evidence as u8;
                encoded[20..24].copy_from_slice(
                    &calibration
                        .normalized_current_per_count
                        .lower()
                        .bits()
                        .to_le_bytes(),
                );
                encoded[24..28].copy_from_slice(
                    &calibration
                        .normalized_current_per_count
                        .upper()
                        .bits()
                        .to_le_bytes(),
                );
                encoded[28..32]
                    .copy_from_slice(&calibration.maximum_additive_error.bits().to_le_bytes());
                encoded[32..36]
                    .copy_from_slice(&calibration.maximum_interval_width_ulps.to_le_bytes());
                // Bytes 18..20 and 36..64 are reserved zero.
            }
            Self::FocPwmAdcTiming(timing) => {
                encoded[8..12].copy_from_slice(&timing.device_cycle_hz.to_le_bytes());
                encoded[12..16].copy_from_slice(&timing.pwm_period_cycles.to_le_bytes());
                encoded[16..20]
                    .copy_from_slice(&timing.nominal_acquisition_offset_cycles.to_le_bytes());
                encoded[20..24]
                    .copy_from_slice(&timing.maximum_trigger_jitter_cycles.to_le_bytes());
                encoded[24..28].copy_from_slice(&timing.maximum_acquisition_cycles.to_le_bytes());
                encoded[28..32].copy_from_slice(&timing.maximum_channel_skew_cycles.to_le_bytes());
                encoded[32..36].copy_from_slice(&timing.maximum_conversion_cycles.to_le_bytes());
                encoded[36..40]
                    .copy_from_slice(&timing.minimum_switching_guard_cycles.to_le_bytes());
                encoded[40..44].copy_from_slice(
                    &timing
                        .maximum_normalized_current_slew_per_cycle
                        .bits()
                        .to_le_bytes(),
                );
                encoded[44..48]
                    .copy_from_slice(&timing.maximum_interchannel_skew_error.bits().to_le_bytes());
                encoded[48..52].copy_from_slice(&timing.maximum_phase_current.bits().to_le_bytes());
                encoded[52..56]
                    .copy_from_slice(&timing.maximum_phase_interval_width_ulps.to_le_bytes());
                encoded[56..60].copy_from_slice(&timing.pwm_dead_time_cycles.to_le_bytes());
                encoded[60] = timing.evidence as u8;
                // Bytes 61..64 are reserved zero.
            }
            Self::FocAdcFrontend(frontend) => {
                encoded[8] = frontend.attenuation as u8;
                encoded[9] = frontend.evidence as u8;
                // Bytes 10..64 are reserved zero.
            }
            Self::FocPwmHardware(hardware) => {
                encoded[8..12].copy_from_slice(&hardware.peripheral_source_clock_hz.to_le_bytes());
                encoded[12..16].copy_from_slice(&hardware.counter_clock_hz.to_le_bytes());
                encoded[16..18].copy_from_slice(&hardware.timer_peak_ticks.to_le_bytes());
                encoded[18..20].copy_from_slice(&hardware.minimum_active_ticks.to_le_bytes());
                encoded[20..24]
                    .copy_from_slice(&hardware.maximum_quantization_error_ulps.to_le_bytes());
                encoded[24] = hardware.peripheral_prescaler;
                encoded[25] = hardware.timer_prescaler;
                encoded[26] = hardware.evidence as u8;
                // Bytes 27..64 are reserved zero.
            }
        }
        Ok(encoded)
    }

    /// Decodes only an exact fixed-width V5 record.
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
            RECORD_KIND_FOC_SHUTDOWN => {
                if encoded[22..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                let control = if encoded[12..16].iter().all(|byte| *byte == 0) {
                    None
                } else {
                    Some(
                        decode_resource_id(&encoded[12..16])
                            .map_err(|_| ConfigurationError::ResourceEncoding)?,
                    )
                };
                Self::FocShutdown(FocShutdownContract {
                    instance,
                    strategy: FocShutdownStrategy::from_wire(selector)
                        .ok_or(ConfigurationError::Selector)?,
                    power_stage: decode_resource_id(&encoded[8..12])
                        .map_err(|_| ConfigurationError::ResourceEncoding)?,
                    control,
                    control_polarity: SignalPolarity::from_wire(encoded[20])
                        .ok_or(ConfigurationError::Polarity)?,
                    maximum_transition_cycles: read_u32(encoded, 16),
                    evidence: FactEvidence::from_wire(encoded[21])
                        .ok_or(ConfigurationError::Evidence)?,
                })
            }
            RECORD_KIND_FOC_RUNTIME => {
                if selector != 0 {
                    return Err(ConfigurationError::Selector);
                }
                if encoded[10..12].iter().any(|byte| *byte != 0)
                    || encoded[32..64].iter().any(|byte| *byte != 0)
                {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocRuntime(FocRuntimeParameters {
                    instance,
                    pole_pairs: read_u16(encoded, 8),
                    timing: FocTimingProfile {
                        pwm_hz: read_u32(encoded, 12),
                        current_loop_hz: read_u32(encoded, 16),
                        velocity_loop_divider: read_u16(encoded, 20),
                        position_loop_divider: read_u16(encoded, 22),
                    },
                    maximum_phase_current: Q30::from_bits(read_i32(encoded, 24)),
                    maximum_phase_voltage: Q30::from_bits(read_i32(encoded, 28)),
                })
            }
            RECORD_KIND_FOC_CONTROLLER => {
                if encoded[32..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocController(FocControllerParameters {
                    instance,
                    axis: FocControllerAxis::from_wire(selector)
                        .ok_or(ConfigurationError::Selector)?,
                    parameters: PiConfig {
                        proportional_gain: Q30::from_bits(read_i32(encoded, 8)),
                        integral_gain_per_update: Q30::from_bits(read_i32(encoded, 12)),
                        integral_minimum: Q30::from_bits(read_i32(encoded, 16)),
                        integral_maximum: Q30::from_bits(read_i32(encoded, 20)),
                        output_minimum: Q30::from_bits(read_i32(encoded, 24)),
                        output_maximum: Q30::from_bits(read_i32(encoded, 28)),
                    },
                })
            }
            RECORD_KIND_FOC_ROTOR => {
                if encoded[41..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocRotor(FocRotorParameters {
                    instance,
                    counts_per_mechanical_turn: read_u32(encoded, 8),
                    count_at_reference: read_u32(encoded, 12),
                    electrical_phase_at_reference: ElectricalPhase::from_bits(read_u32(
                        encoded, 16,
                    )),
                    direction: rotor_direction_from_wire(selector)
                        .ok_or(ConfigurationError::Selector)?,
                    maximum_alignment_error_bits: read_u32(encoded, 20),
                    maximum_count_error: CountUncertainty::new(
                        read_u32(encoded, 24),
                        read_u32(encoded, 28),
                    )
                    .map_err(|_| ConfigurationError::FocRotor)?,
                    rotation_precision: RotationPrecision {
                        maximum_component_width_ulps: read_u32(encoded, 32),
                        maximum_norm_error_ulps: read_u32(encoded, 36),
                    },
                    evidence: FactEvidence::from_wire(encoded[40])
                        .ok_or(ConfigurationError::Evidence)?,
                })
            }
            RECORD_KIND_FOC_CURRENT_CHANNEL => {
                if encoded[18..20].iter().any(|byte| *byte != 0)
                    || encoded[36..64].iter().any(|byte| *byte != 0)
                {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocCurrentChannel(FocCurrentChannelParameters {
                    instance,
                    channel: FocCurrentChannel::from_wire(selector)
                        .ok_or(ConfigurationError::Selector)?,
                    calibration: CurrentChannelCalibration {
                        adc_maximum_count: read_u16(encoded, 8),
                        valid_count_minimum: read_u16(encoded, 10),
                        valid_count_maximum: read_u16(encoded, 12),
                        count_at_zero: read_u16(encoded, 14),
                        polarity: current_polarity_from_wire(encoded[16])
                            .ok_or(ConfigurationError::Selector)?,
                        normalized_current_per_count: Q30Interval::new(
                            Q30::from_bits(read_i32(encoded, 20)),
                            Q30::from_bits(read_i32(encoded, 24)),
                        )
                        .map_err(|_| ConfigurationError::FocCurrent)?,
                        maximum_additive_error: Q30::from_bits(read_i32(encoded, 28)),
                        maximum_interval_width_ulps: read_u32(encoded, 32),
                    },
                    evidence: FactEvidence::from_wire(encoded[17])
                        .ok_or(ConfigurationError::Evidence)?,
                })
            }
            RECORD_KIND_FOC_PWM_ADC_TIMING => {
                if encoded[61..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocPwmAdcTiming(FocPwmAdcTimingParameters {
                    instance,
                    phase_pair: phase_pair_from_wire(selector)
                        .ok_or(ConfigurationError::Selector)?,
                    device_cycle_hz: read_u32(encoded, 8),
                    pwm_period_cycles: read_u32(encoded, 12),
                    nominal_acquisition_offset_cycles: read_u32(encoded, 16),
                    maximum_trigger_jitter_cycles: read_u32(encoded, 20),
                    maximum_acquisition_cycles: read_u32(encoded, 24),
                    maximum_channel_skew_cycles: read_u32(encoded, 28),
                    maximum_conversion_cycles: read_u32(encoded, 32),
                    minimum_switching_guard_cycles: read_u32(encoded, 36),
                    maximum_normalized_current_slew_per_cycle: Q30::from_bits(read_i32(
                        encoded, 40,
                    )),
                    maximum_interchannel_skew_error: Q30::from_bits(read_i32(encoded, 44)),
                    maximum_phase_current: Q30::from_bits(read_i32(encoded, 48)),
                    maximum_phase_interval_width_ulps: read_u32(encoded, 52),
                    pwm_dead_time_cycles: read_u32(encoded, 56),
                    evidence: FactEvidence::from_wire(encoded[60])
                        .ok_or(ConfigurationError::Evidence)?,
                })
            }
            RECORD_KIND_FOC_ADC_FRONTEND => {
                if encoded[10..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocAdcFrontend(FocAdcFrontendParameters {
                    instance,
                    channel: FocCurrentChannel::from_wire(selector)
                        .ok_or(ConfigurationError::Selector)?,
                    attenuation: FocAdcAttenuation::from_wire(encoded[8])
                        .ok_or(ConfigurationError::Selector)?,
                    evidence: FactEvidence::from_wire(encoded[9])
                        .ok_or(ConfigurationError::Evidence)?,
                })
            }
            RECORD_KIND_FOC_PWM_HARDWARE => {
                if selector != 0 {
                    return Err(ConfigurationError::Selector);
                }
                if encoded[27..64].iter().any(|byte| *byte != 0) {
                    return Err(ConfigurationError::Reserved);
                }
                Self::FocPwmHardware(FocPwmHardwareParameters {
                    instance,
                    peripheral_source_clock_hz: read_u32(encoded, 8),
                    counter_clock_hz: read_u32(encoded, 12),
                    timer_peak_ticks: read_u16(encoded, 16),
                    minimum_active_ticks: read_u16(encoded, 18),
                    maximum_quantization_error_ulps: read_u32(encoded, 20),
                    peripheral_prescaler: encoded[24],
                    timer_prescaler: encoded[25],
                    evidence: FactEvidence::from_wire(encoded[26])
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
                            | ScalarFact::StepperOutputQuantumCycles
                    )
            }
            Self::FocShutdown(_)
            | Self::FocRuntime(_)
            | Self::FocController(_)
            | Self::FocRotor(_)
            | Self::FocCurrentChannel(_)
            | Self::FocPwmAdcTiming(_)
            | Self::FocAdcFrontend(_)
            | Self::FocPwmHardware(_) => true,
        }
    }

    /// Wire-level `(kind, instance, selector)` key used for strict canonical
    /// document ordering.
    pub const fn canonical_order_key(self) -> (u16, u16, u16) {
        match self {
            Self::Binding(binding) => (RECORD_KIND_BINDING, binding.instance, binding.role as u16),
            Self::Scalar(scalar) => (RECORD_KIND_SCALAR, scalar.instance, scalar.fact as u16),
            Self::FocShutdown(shutdown) => (
                RECORD_KIND_FOC_SHUTDOWN,
                shutdown.instance,
                shutdown.strategy as u16,
            ),
            Self::FocRuntime(runtime) => (RECORD_KIND_FOC_RUNTIME, runtime.instance, 0),
            Self::FocController(controller) => (
                RECORD_KIND_FOC_CONTROLLER,
                controller.instance,
                controller.axis as u16,
            ),
            Self::FocRotor(rotor) => (
                RECORD_KIND_FOC_ROTOR,
                rotor.instance,
                rotor_direction_wire(rotor.direction),
            ),
            Self::FocCurrentChannel(channel) => (
                RECORD_KIND_FOC_CURRENT_CHANNEL,
                channel.instance,
                channel.channel as u16,
            ),
            Self::FocPwmAdcTiming(timing) => (
                RECORD_KIND_FOC_PWM_ADC_TIMING,
                timing.instance,
                phase_pair_wire(timing.phase_pair),
            ),
            Self::FocAdcFrontend(frontend) => (
                RECORD_KIND_FOC_ADC_FRONTEND,
                frontend.instance,
                frontend.channel as u16,
            ),
            Self::FocPwmHardware(hardware) => (RECORD_KIND_FOC_PWM_HARDWARE, hardware.instance, 0),
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
                let safety_role = binding.role.safety_input_role();
                let required = binding.flags.0 & BindingFlags::REQUIRED_INTERLOCK != 0;
                if required && safety_role.is_none()
                    || matches!(
                        safety_role,
                        Some(SafetyInputRole::EmergencyStop | SafetyInputRole::SafetyInterlock)
                    ) && !required
                {
                    return Err(ConfigurationError::Binding);
                }
                if safety_role.is_some() && binding.watchdog_cycles == 0 {
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
                    || matches!(
                        scalar.fact,
                        ScalarFact::TimerTickHertz | ScalarFact::StepperOutputQuantumCycles
                    ) && scalar.instance != 0
                    || scalar.fact.axis_fact() && usize::from(scalar.instance) >= MAX_AXIS_INSTANCES
                {
                    return Err(ConfigurationError::Scalar);
                }
            }
            Self::FocShutdown(shutdown) => {
                if usize::from(shutdown.instance) >= MAX_EXECUTABLE_FOC_AXES
                    || !matches!(shutdown.power_stage, ResourceId::Device(_))
                    || shutdown.maximum_transition_cycles == 0
                    || shutdown.evidence != FactEvidence::Qualified
                {
                    return Err(ConfigurationError::ShutdownContract);
                }
                match shutdown.strategy {
                    FocShutdownStrategy::PhaseHighImpedance => {
                        if shutdown.control.is_some()
                            || shutdown.control_polarity != SignalPolarity::NotApplicable
                        {
                            return Err(ConfigurationError::ShutdownContract);
                        }
                    }
                    FocShutdownStrategy::DedicatedEnable
                    | FocShutdownStrategy::DedicatedDisable => {
                        if !matches!(
                            shutdown.control,
                            Some(
                                ResourceId::Gpio(_)
                                    | ResourceId::I2sOut { .. }
                                    | ResourceId::TimedOutput { .. }
                            )
                        ) || shutdown.control_polarity == SignalPolarity::NotApplicable
                        {
                            return Err(ConfigurationError::ShutdownContract);
                        }
                    }
                }
            }
            Self::FocRuntime(runtime) => runtime.validate_shape()?,
            Self::FocController(controller) => controller.validate_shape()?,
            Self::FocRotor(rotor) => rotor.validate_shape()?,
            Self::FocCurrentChannel(channel) => channel.validate_shape()?,
            Self::FocPwmAdcTiming(timing) => timing.validate_shape()?,
            Self::FocAdcFrontend(frontend) => frontend.validate_shape()?,
            Self::FocPwmHardware(hardware) => hardware.validate_shape()?,
        }
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct AxisState {
    binding_mask: u64,
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

#[derive(Clone, Copy)]
struct StepperProfileState {
    step: Option<ResourceBinding>,
    direction: Option<ResourceBinding>,
    driver_control: Option<ResourceBinding>,
    driver_control_action: AxisDriverControl,
}

impl StepperProfileState {
    const EMPTY: Self = Self {
        step: None,
        direction: None,
        driver_control: None,
        driver_control_action: AxisDriverControl::Enable,
    };
}

#[derive(Clone, Copy)]
struct FocProfileState {
    phase_u: Option<ResourceBinding>,
    phase_v: Option<ResourceBinding>,
    phase_w: Option<ResourceBinding>,
    current_a: Option<ResourceBinding>,
    current_b: Option<ResourceBinding>,
    current_c: Option<ResourceBinding>,
    bus_voltage: Option<ResourceBinding>,
    encoder: Option<ResourceBinding>,
    fault: Option<ResourceBinding>,
    shutdown: Option<FocShutdownContract>,
    runtime: Option<FocRuntimeParameters>,
    direct_controller: Option<FocControllerParameters>,
    quadrature_controller: Option<FocControllerParameters>,
    rotor: Option<FocRotorParameters>,
    current_channel0: Option<FocCurrentChannelParameters>,
    current_channel1: Option<FocCurrentChannelParameters>,
    pwm_adc_timing: Option<FocPwmAdcTimingParameters>,
    adc_channel0: Option<FocAdcFrontendParameters>,
    adc_channel1: Option<FocAdcFrontendParameters>,
    pwm_hardware: Option<FocPwmHardwareParameters>,
    encoder_counts_per_turn: Option<Rational>,
    pole_pairs: Option<Rational>,
    pwm_carrier_hz: Option<Rational>,
    pwm_dead_time_seconds: Option<Rational>,
    control_rate_hz: Option<Rational>,
}

impl FocProfileState {
    const EMPTY: Self = Self {
        phase_u: None,
        phase_v: None,
        phase_w: None,
        current_a: None,
        current_b: None,
        current_c: None,
        bus_voltage: None,
        encoder: None,
        fault: None,
        shutdown: None,
        runtime: None,
        direct_controller: None,
        quadrature_controller: None,
        rotor: None,
        current_channel0: None,
        current_channel1: None,
        pwm_adc_timing: None,
        adc_channel0: None,
        adc_channel1: None,
        pwm_hardware: None,
        encoder_counts_per_turn: None,
        pole_pairs: None,
        pwm_carrier_hz: None,
        pwm_dead_time_seconds: None,
        control_rate_hz: None,
    };

    const fn has_configuration(self) -> bool {
        self.shutdown.is_some()
            || self.runtime.is_some()
            || self.direct_controller.is_some()
            || self.quadrature_controller.is_some()
            || self.rotor.is_some()
            || self.current_channel0.is_some()
            || self.current_channel1.is_some()
            || self.pwm_adc_timing.is_some()
            || self.adc_channel0.is_some()
            || self.adc_channel1.is_some()
            || self.pwm_hardware.is_some()
    }
}

/// Whether the configured driver-control resource asserts enable or disable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AxisDriverControl {
    Enable,
    Disable,
}

/// Exact resource and cycle-timing facts needed by a step/direction backend.
///
/// For `step`, the active/inactive fields are pulse-high and pulse-low time.
/// For `direction` and `driver_control`, they are setup-before-step and
/// hold-after-step time. This role-specific interpretation is part of
/// configuration V5.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StepperAxisProfile {
    pub instance: u16,
    pub step: ResourceBinding,
    pub direction: ResourceBinding,
    pub driver_control: ResourceBinding,
    pub driver_control_action: AxisDriverControl,
}

/// Complete resource, calibration, timing, and control facts for one FOC axis.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FocAxisProfile {
    pub instance: u16,
    pub phase_u: ResourceBinding,
    pub phase_v: ResourceBinding,
    pub phase_w: ResourceBinding,
    pub current_channel0_binding: ResourceBinding,
    pub current_channel1_binding: ResourceBinding,
    pub bus_voltage: Option<ResourceBinding>,
    pub encoder: ResourceBinding,
    pub fault: Option<ResourceBinding>,
    pub shutdown: FocShutdownContract,
    pub runtime: FocRuntimeParameters,
    pub direct_controller: FocControllerParameters,
    pub quadrature_controller: FocControllerParameters,
    pub rotor: FocRotorParameters,
    pub current_channel0: FocCurrentChannelParameters,
    pub current_channel1: FocCurrentChannelParameters,
    pub pwm_adc_timing: FocPwmAdcTimingParameters,
    pub adc_channel0: FocAdcFrontendParameters,
    pub adc_channel1: FocAdcFrontendParameters,
    pub pwm_hardware: FocPwmHardwareParameters,
}

/// Allocation-free executable facts retained from the exact configuration.
/// Empty slots remain distinguishable from configured logical-axis instances.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeConfigurationProfile {
    stepper_axes: [Option<StepperAxisProfile>; MAX_EXECUTABLE_STEPPER_AXES],
    foc_axes: [Option<FocAxisProfile>; MAX_EXECUTABLE_FOC_AXES],
    foc_axis_count: u8,
    safety_inputs: [Option<SafetyInputSpec>; MAX_SAFETY_INPUTS],
    safety_input_count: u8,
    timer_tick_hertz: Option<u64>,
    stepper_output_quantum_cycles: Option<u32>,
}

impl RealtimeConfigurationProfile {
    const EMPTY: Self = Self {
        stepper_axes: [None; MAX_EXECUTABLE_STEPPER_AXES],
        foc_axes: [None; MAX_EXECUTABLE_FOC_AXES],
        foc_axis_count: 0,
        safety_inputs: [None; MAX_SAFETY_INPUTS],
        safety_input_count: 0,
        timer_tick_hertz: None,
        stepper_output_quantum_cycles: None,
    };

    /// Profile for one logical stepper-axis instance.
    pub const fn stepper_axis(&self, instance: usize) -> Option<StepperAxisProfile> {
        if instance < MAX_EXECUTABLE_STEPPER_AXES {
            self.stepper_axes[instance]
        } else {
            None
        }
    }

    /// Count of compact, instance-ordered FOC profiles retained for core 1.
    pub const fn foc_axis_count(&self) -> usize {
        self.foc_axis_count as usize
    }

    /// One compact FOC profile slot; the profile retains its logical instance.
    pub const fn foc_axis(&self, slot: usize) -> Option<FocAxisProfile> {
        if slot < self.foc_axis_count as usize {
            self.foc_axes[slot]
        } else {
            None
        }
    }

    /// Iterates admitted FOC axes in ascending logical-instance order.
    pub fn foc_axes(&self) -> impl Iterator<Item = FocAxisProfile> + '_ {
        self.foc_axes[..self.foc_axis_count as usize]
            .iter()
            .copied()
            .flatten()
    }

    /// Count of canonical safety-input slots retained for core-1 sampling.
    pub const fn safety_input_count(&self) -> usize {
        self.safety_input_count as usize
    }

    /// One configuration-stable safety-input slot.
    pub const fn safety_input(&self, slot: usize) -> Option<SafetyInputSpec> {
        if slot < self.safety_input_count as usize {
            self.safety_inputs[slot]
        } else {
            None
        }
    }

    /// Iterates the exact configuration-stable slots without allocation.
    pub fn safety_inputs(&self) -> impl Iterator<Item = SafetyInputSpec> + '_ {
        self.safety_inputs[..self.safety_input_count as usize]
            .iter()
            .copied()
            .flatten()
    }

    /// Exact configured frequency of the cached-motion `DeviceCycle` domain.
    pub const fn timer_tick_hertz(&self) -> Option<u64> {
        self.timer_tick_hertz
    }

    /// Exact configured stepper backend output lattice in device cycles.
    pub const fn stepper_output_quantum_cycles(&self) -> Option<u32> {
        self.stepper_output_quantum_cycles
    }
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
    steppers: [StepperProfileState; MAX_EXECUTABLE_STEPPER_AXES],
    foc: [FocProfileState; MAX_EXECUTABLE_FOC_AXES],
    safety_inputs: [Option<SafetyInputSpec>; MAX_SAFETY_INPUTS],
    safety_input_count: usize,
    safety_binding: bool,
    timer_tick_hertz: Option<u64>,
    stepper_output_quantum_cycles: Option<u32>,
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
            steppers: [StepperProfileState::EMPTY; MAX_EXECUTABLE_STEPPER_AXES],
            foc: [FocProfileState::EMPTY; MAX_EXECUTABLE_FOC_AXES],
            safety_inputs: [None; MAX_SAFETY_INPUTS],
            safety_input_count: 0,
            safety_binding: false,
            timer_tick_hertz: None,
            stepper_output_quantum_cycles: None,
        })
    }

    /// Validates and consumes the next strictly ordered record.
    pub fn push(&mut self, record: ConfigurationRecord) -> Result<(), ConfigurationError> {
        if self.seen_records >= self.header.record_count {
            return Err(ConfigurationError::RecordCount);
        }
        record.validate_shape()?;
        let key = record.canonical_order_key();
        if self.last_key.is_some_and(|last| last >= key) {
            return Err(ConfigurationError::RecordOrder);
        }
        match record {
            ConfigurationRecord::Binding(binding) => self.validate_binding(binding)?,
            ConfigurationRecord::Scalar(scalar) => self.validate_scalar(scalar)?,
            ConfigurationRecord::FocShutdown(shutdown) => {
                self.validate_foc_shutdown(shutdown)?;
            }
            ConfigurationRecord::FocRuntime(runtime) => self.retain_foc_runtime(runtime)?,
            ConfigurationRecord::FocController(controller) => {
                self.retain_foc_controller(controller)?;
            }
            ConfigurationRecord::FocRotor(rotor) => self.retain_foc_rotor(rotor)?,
            ConfigurationRecord::FocCurrentChannel(channel) => {
                self.retain_foc_current_channel(channel)?;
            }
            ConfigurationRecord::FocPwmAdcTiming(timing) => {
                self.retain_foc_pwm_adc_timing(timing)?;
            }
            ConfigurationRecord::FocAdcFrontend(frontend) => {
                self.retain_foc_adc_frontend(frontend)?;
            }
            ConfigurationRecord::FocPwmHardware(hardware) => {
                self.retain_foc_pwm_hardware(hardware)?;
            }
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
        self.finish_with_profile().map(|(summary, _)| summary)
    }

    /// Completes validation while retaining the exact executable resource and
    /// cycle-timing profile. Both cores derive this from the same canonical
    /// bytes; only the real-time owner may later use it to construct outputs.
    pub fn finish_with_profile(
        self,
    ) -> Result<(ConfigurationSummary, RealtimeConfigurationProfile), ConfigurationError> {
        if self.seen_records != self.header.record_count
            || self.realtime_records != self.header.realtime_record_count
        {
            return Err(ConfigurationError::RecordCount);
        }
        let mut stepper_axes = 0_u8;
        let mut foc_axes = 0_u8;
        let mut profile = RealtimeConfigurationProfile::EMPTY;
        for (instance, axis) in self.axes.iter().copied().enumerate() {
            let has_step = axis.binding_mask & role_bit(BindingRole::AxisStep) != 0;
            let foc_binding_mask = role_bit(BindingRole::FocPhaseU)
                | role_bit(BindingRole::FocPhaseV)
                | role_bit(BindingRole::FocPhaseW)
                | role_bit(BindingRole::FocCurrentA)
                | role_bit(BindingRole::FocCurrentB)
                | role_bit(BindingRole::FocCurrentC)
                | role_bit(BindingRole::FocBusVoltage)
                | role_bit(BindingRole::FocEncoder)
                | role_bit(BindingRole::FocFault);
            let retained_foc = self
                .foc
                .get(instance)
                .copied()
                .unwrap_or(FocProfileState::EMPTY);
            let has_foc =
                axis.binding_mask & foc_binding_mask != 0 || retained_foc.has_configuration();
            if has_step && has_foc {
                return Err(ConfigurationError::AxisKind);
            }
            if has_step {
                if instance >= MAX_EXECUTABLE_STEPPER_AXES {
                    return Err(ConfigurationError::AxisCount);
                }
                let required_bindings =
                    role_bit(BindingRole::AxisStep) | role_bit(BindingRole::AxisDirection);
                let has_enable = axis.binding_mask & role_bit(BindingRole::AxisEnable) != 0;
                let has_disable = axis.binding_mask & role_bit(BindingRole::AxisDisable) != 0;
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
                if axis.binding_mask & required_bindings != required_bindings
                    || has_enable == has_disable
                    || axis.scalar_mask & scalars != scalars
                {
                    return Err(ConfigurationError::IncompleteAxis);
                }
                let retained = self.steppers[instance];
                let step = retained.step.ok_or(ConfigurationError::IncompleteAxis)?;
                let direction = retained
                    .direction
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let driver_control = retained
                    .driver_control
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let driver_control_action = retained.driver_control_action;
                if direction.minimum_active_cycles == 0
                    || direction.minimum_inactive_cycles == 0
                    || driver_control.minimum_active_cycles == 0
                    || driver_control.minimum_inactive_cycles == 0
                {
                    return Err(ConfigurationError::Timing);
                }
                profile.stepper_axes[instance] = Some(StepperAxisProfile {
                    instance: u16::try_from(instance).map_err(|_| ConfigurationError::AxisCount)?,
                    step,
                    direction,
                    driver_control,
                    driver_control_action,
                });
                stepper_axes = stepper_axes
                    .checked_add(1)
                    .ok_or(ConfigurationError::AxisCount)?;
            }
            if has_foc {
                if instance >= MAX_EXECUTABLE_FOC_AXES {
                    return Err(ConfigurationError::AxisCount);
                }
                let bindings = role_bit(BindingRole::FocPhaseU)
                    | role_bit(BindingRole::FocPhaseV)
                    | role_bit(BindingRole::FocPhaseW)
                    | role_bit(BindingRole::FocEncoder);
                let scalars = fact_bit(ScalarFact::AxisEncoderCountsPerTurn)
                    | fact_bit(ScalarFact::MotorPolePairs)
                    | fact_bit(ScalarFact::MotorCurrentLimitAmperes)
                    | fact_bit(ScalarFact::MotorVoltageLimitVolts)
                    | fact_bit(ScalarFact::PwmCarrierHertz)
                    | fact_bit(ScalarFact::PwmDeadTimeSeconds)
                    | fact_bit(ScalarFact::ControlRateHertz)
                    | fact_bit(ScalarFact::CurrentSenseOhms)
                    | fact_bit(ScalarFact::CurrentSenseVoltsPerAmpere);
                if axis.binding_mask & bindings != bindings || axis.scalar_mask & scalars != scalars
                {
                    return Err(ConfigurationError::IncompleteAxis);
                }
                let retained = retained_foc;
                let phase_u = retained.phase_u.ok_or(ConfigurationError::IncompleteAxis)?;
                let phase_v = retained.phase_v.ok_or(ConfigurationError::IncompleteAxis)?;
                let phase_w = retained.phase_w.ok_or(ConfigurationError::IncompleteAxis)?;
                let encoder = retained.encoder.ok_or(ConfigurationError::IncompleteAxis)?;
                let shutdown = retained
                    .shutdown
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let runtime = retained.runtime.ok_or(ConfigurationError::IncompleteAxis)?;
                let direct_controller = retained
                    .direct_controller
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let quadrature_controller = retained
                    .quadrature_controller
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let rotor = retained.rotor.ok_or(ConfigurationError::IncompleteAxis)?;
                let current_channel0 = retained
                    .current_channel0
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let current_channel1 = retained
                    .current_channel1
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let pwm_adc_timing = retained
                    .pwm_adc_timing
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let adc_channel0 = retained
                    .adc_channel0
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let adc_channel1 = retained
                    .adc_channel1
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let pwm_hardware = retained
                    .pwm_hardware
                    .ok_or(ConfigurationError::IncompleteAxis)?;
                let current_mask = role_bit(BindingRole::FocCurrentA)
                    | role_bit(BindingRole::FocCurrentB)
                    | role_bit(BindingRole::FocCurrentC);
                let (expected_current_mask, current_channel0_binding, current_channel1_binding) =
                    match pwm_adc_timing.phase_pair {
                        TwoShuntPhasePair::Ab => (
                            role_bit(BindingRole::FocCurrentA) | role_bit(BindingRole::FocCurrentB),
                            retained.current_a,
                            retained.current_b,
                        ),
                        TwoShuntPhasePair::Bc => (
                            role_bit(BindingRole::FocCurrentB) | role_bit(BindingRole::FocCurrentC),
                            retained.current_b,
                            retained.current_c,
                        ),
                        TwoShuntPhasePair::Ca => (
                            role_bit(BindingRole::FocCurrentC) | role_bit(BindingRole::FocCurrentA),
                            retained.current_c,
                            retained.current_a,
                        ),
                    };
                if axis.binding_mask & current_mask != expected_current_mask {
                    return Err(ConfigurationError::IncompleteAxis);
                }
                let current_channel0_binding =
                    current_channel0_binding.ok_or(ConfigurationError::IncompleteAxis)?;
                let current_channel1_binding =
                    current_channel1_binding.ok_or(ConfigurationError::IncompleteAxis)?;
                if [phase_u, phase_v, phase_w]
                    .iter()
                    .any(|phase| phase.maximum_frequency_hz < runtime.timing.pwm_hz)
                    || [current_channel0_binding, current_channel1_binding]
                        .iter()
                        .any(|channel| {
                            channel.maximum_frequency_hz < runtime.timing.current_loop_hz
                        })
                    || u64::from(encoder.maximum_frequency_hz)
                        * u64::from(runtime.timing.velocity_loop_divider)
                        < u64::from(runtime.timing.current_loop_hz)
                {
                    return Err(ConfigurationError::Frequency);
                }
                self.validate_foc_shutdown_topology(
                    phase_u,
                    phase_v,
                    phase_w,
                    current_channel0_binding,
                    current_channel1_binding,
                    shutdown,
                )?;
                if matches!(encoder.resource, ResourceId::Device(_))
                    && !self.package.devices.iter().any(|device| {
                        device.resource == encoder.resource
                            && device.owner == OwnerDomain::Realtime
                            && device.support != SupportLevel::Described
                    })
                {
                    return Err(ConfigurationError::FocRotor);
                }
                if retained
                    .pole_pairs
                    .is_none_or(|value| !rational_equals_u32(value, u32::from(runtime.pole_pairs)))
                    || retained
                        .pwm_carrier_hz
                        .is_none_or(|value| !rational_equals_u32(value, runtime.timing.pwm_hz))
                    || retained.control_rate_hz.is_none_or(|value| {
                        !rational_equals_u32(value, runtime.timing.current_loop_hz)
                    })
                    || retained.encoder_counts_per_turn.is_none_or(|value| {
                        !rational_equals_u32(value, rotor.counts_per_mechanical_turn)
                    })
                    || retained.pwm_dead_time_seconds.is_none_or(|value| {
                        !rational_equals_ratio(
                            value,
                            pwm_adc_timing.pwm_dead_time_cycles,
                            pwm_adc_timing.device_cycle_hz,
                        )
                    })
                {
                    return Err(ConfigurationError::FocRuntime);
                }
                let foc_profile = FocAxisProfile {
                    instance: u16::try_from(instance).map_err(|_| ConfigurationError::AxisCount)?,
                    phase_u,
                    phase_v,
                    phase_w,
                    current_channel0_binding,
                    current_channel1_binding,
                    bus_voltage: retained.bus_voltage,
                    encoder,
                    fault: retained.fault,
                    shutdown,
                    runtime,
                    direct_controller,
                    quadrature_controller,
                    rotor,
                    current_channel0,
                    current_channel1,
                    pwm_adc_timing,
                    adc_channel0,
                    adc_channel1,
                    pwm_hardware,
                };
                foc::validate_profile(foc_profile)?;
                profile.foc_axes[usize::from(foc_axes)] = Some(foc_profile);
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
            && (stepper_axes == 0 && foc_axes == 0
                || !self.safety_binding
                || self.timer_tick_hertz.is_none()
                || stepper_axes != 0 && self.stepper_output_quantum_cycles.is_none())
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
        profile.safety_inputs = self.safety_inputs;
        profile.foc_axis_count = foc_axes;
        profile.safety_input_count = u8::try_from(self.safety_input_count)
            .map_err(|_| ConfigurationError::SafetyInputCapacity)?;
        profile.timer_tick_hertz = self.timer_tick_hertz;
        profile.stepper_output_quantum_cycles = self.stepper_output_quantum_cycles;
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
        Ok((summary, profile))
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
        let safety_spec = if let Some(role) = binding.role.safety_input_role() {
            if self.safety_input_count >= MAX_SAFETY_INPUTS {
                return Err(ConfigurationError::SafetyInputCapacity);
            }
            let polarity = match binding.polarity {
                SignalPolarity::ActiveHigh => InputPolarity::ActiveHigh,
                SignalPolarity::ActiveLow => InputPolarity::ActiveLow,
                SignalPolarity::NotApplicable => return Err(ConfigurationError::Polarity),
            };
            let bias = if binding.flags.0 & BindingFlags::PULL_UP != 0 {
                InputBias::PullUp
            } else if binding.flags.0 & BindingFlags::PULL_DOWN != 0 {
                InputBias::PullDown
            } else {
                InputBias::Floating
            };
            let spec = SafetyInputSpec {
                instance: binding.instance,
                role,
                resource: binding.resource,
                polarity,
                bias,
                required_for_arm: role.conservatively_requires_clear_to_arm()
                    || binding.flags.0 & BindingFlags::REQUIRED_INTERLOCK != 0,
                minimum_active_cycles: binding.minimum_active_cycles,
                minimum_inactive_cycles: binding.minimum_inactive_cycles,
                maximum_sample_gap_cycles: binding.watchdog_cycles,
            };
            spec.validate().map_err(|_| ConfigurationError::Binding)?;
            Some(spec)
        } else {
            None
        };
        self.claim_resource(binding.resource)?;

        if let Some(spec) = safety_spec {
            self.safety_inputs[self.safety_input_count] = Some(spec);
            self.safety_input_count += 1;
        }

        if binding.role.axis_role() {
            let instance = usize::from(binding.instance);
            let axis = &mut self.axes[instance];
            axis.binding_mask |= role_bit(binding.role);
            if let Some(stepper) = self.steppers.get_mut(instance) {
                match binding.role {
                    BindingRole::AxisStep => stepper.step = Some(binding),
                    BindingRole::AxisDirection => stepper.direction = Some(binding),
                    BindingRole::AxisEnable => {
                        stepper.driver_control = Some(binding);
                        stepper.driver_control_action = AxisDriverControl::Enable;
                    }
                    BindingRole::AxisDisable => {
                        stepper.driver_control = Some(binding);
                        stepper.driver_control_action = AxisDriverControl::Disable;
                    }
                    _ => {}
                }
            }
            if let Some(foc) = self.foc.get_mut(instance) {
                match binding.role {
                    BindingRole::FocPhaseU => foc.phase_u = Some(binding),
                    BindingRole::FocPhaseV => foc.phase_v = Some(binding),
                    BindingRole::FocPhaseW => foc.phase_w = Some(binding),
                    BindingRole::FocCurrentA => foc.current_a = Some(binding),
                    BindingRole::FocCurrentB => foc.current_b = Some(binding),
                    BindingRole::FocCurrentC => foc.current_c = Some(binding),
                    BindingRole::FocBusVoltage => foc.bus_voltage = Some(binding),
                    BindingRole::FocEncoder => foc.encoder = Some(binding),
                    BindingRole::FocFault => foc.fault = Some(binding),
                    _ => {}
                }
            }
        }
        if matches!(
            binding.role,
            BindingRole::EmergencyStop | BindingRole::SafetyInterlock
        ) {
            self.safety_binding = true;
        }
        Ok(())
    }

    fn claim_resource(&mut self, resource: ResourceId) -> Result<(), ConfigurationError> {
        self.claim_resources(&[resource])
    }

    fn claim_resources(&mut self, resources: &[ResourceId]) -> Result<(), ConfigurationError> {
        for (index, resource) in resources.iter().copied().enumerate() {
            if self.claimed[..self.claimed_len].contains(&Some(resource))
                || resources[..index].contains(&resource)
            {
                return Err(ConfigurationError::DuplicateResource(resource));
            }
        }
        let end = self
            .claimed_len
            .checked_add(resources.len())
            .ok_or(ConfigurationError::BindingCapacity)?;
        if end > self.claimed.len() {
            return Err(ConfigurationError::BindingCapacity);
        }
        for resource in resources {
            self.claimed[self.claimed_len] = Some(*resource);
            self.claimed_len += 1;
        }
        Ok(())
    }

    fn retain_foc_runtime(
        &mut self,
        runtime: FocRuntimeParameters,
    ) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(runtime.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        if state.runtime.replace(runtime).is_some() {
            return Err(ConfigurationError::FocRuntime);
        }
        Ok(())
    }

    fn retain_foc_controller(
        &mut self,
        controller: FocControllerParameters,
    ) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(controller.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        let selected = match controller.axis {
            FocControllerAxis::Direct => &mut state.direct_controller,
            FocControllerAxis::Quadrature => &mut state.quadrature_controller,
        };
        if selected.replace(controller).is_some() {
            return Err(ConfigurationError::FocRuntime);
        }
        Ok(())
    }

    fn retain_foc_rotor(&mut self, rotor: FocRotorParameters) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(rotor.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        if state.rotor.replace(rotor).is_some() {
            return Err(ConfigurationError::FocRotor);
        }
        Ok(())
    }

    fn retain_foc_current_channel(
        &mut self,
        channel: FocCurrentChannelParameters,
    ) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(channel.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        let selected = match channel.channel {
            FocCurrentChannel::Channel0 => &mut state.current_channel0,
            FocCurrentChannel::Channel1 => &mut state.current_channel1,
        };
        if selected.replace(channel).is_some() {
            return Err(ConfigurationError::FocCurrent);
        }
        Ok(())
    }

    fn retain_foc_pwm_adc_timing(
        &mut self,
        timing: FocPwmAdcTimingParameters,
    ) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(timing.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        if state.pwm_adc_timing.replace(timing).is_some() {
            return Err(ConfigurationError::FocCurrent);
        }
        Ok(())
    }

    fn retain_foc_adc_frontend(
        &mut self,
        frontend: FocAdcFrontendParameters,
    ) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(frontend.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        let selected = match frontend.channel {
            FocCurrentChannel::Channel0 => &mut state.adc_channel0,
            FocCurrentChannel::Channel1 => &mut state.adc_channel1,
        };
        if selected.replace(frontend).is_some() {
            return Err(ConfigurationError::FocHardware);
        }
        Ok(())
    }

    fn retain_foc_pwm_hardware(
        &mut self,
        hardware: FocPwmHardwareParameters,
    ) -> Result<(), ConfigurationError> {
        let state = self
            .foc
            .get_mut(usize::from(hardware.instance))
            .ok_or(ConfigurationError::AxisCount)?;
        if state.pwm_hardware.replace(hardware).is_some() {
            return Err(ConfigurationError::FocHardware);
        }
        Ok(())
    }

    fn validate_foc_shutdown(
        &mut self,
        shutdown: FocShutdownContract,
    ) -> Result<(), ConfigurationError> {
        let instance = usize::from(shutdown.instance);
        let state = self
            .foc
            .get(instance)
            .ok_or(ConfigurationError::AxisCount)?;
        if state.shutdown.is_some() {
            return Err(ConfigurationError::ShutdownContract);
        }
        let stage_resource = find_resource(self.package, shutdown.power_stage)
            .ok_or(ConfigurationError::UnknownResource(shutdown.power_stage))?;
        let stage = self
            .package
            .devices
            .iter()
            .find(|device| device.resource == shutdown.power_stage)
            .ok_or(ConfigurationError::ShutdownContract)?;
        if stage_resource.owner != OwnerDomain::Realtime
            || !stage_resource.hazardous_output
            || stage.owner != OwnerDomain::Realtime
            || stage.support != SupportLevel::Qualified
        {
            return Err(ConfigurationError::ShutdownUnqualified);
        }
        if let Some(control) = shutdown.control {
            let descriptor = find_resource(self.package, control)
                .ok_or(ConfigurationError::UnknownResource(control))?;
            if descriptor.owner != OwnerDomain::Realtime || !descriptor.hazardous_output {
                return Err(ConfigurationError::ShutdownContract);
            }
            for constraint in self.package.electrical_constraints {
                if !constraint
                    .resources
                    .iter()
                    .any(|resource| constraint_applies(*resource, control))
                {
                    continue;
                }
                match constraint.kind {
                    ElectricalConstraintKind::InputOnly => {
                        return Err(ConfigurationError::ShutdownContract);
                    }
                    ElectricalConstraintKind::ActiveHigh
                        if shutdown.control_polarity != SignalPolarity::ActiveHigh =>
                    {
                        return Err(ConfigurationError::Polarity);
                    }
                    ElectricalConstraintKind::ActiveLow
                        if shutdown.control_polarity != SignalPolarity::ActiveLow =>
                    {
                        return Err(ConfigurationError::Polarity);
                    }
                    _ => {}
                }
            }
        }
        if let Some(control) = shutdown.control {
            self.claim_resources(&[shutdown.power_stage, control])?;
        } else {
            self.claim_resource(shutdown.power_stage)?;
        }
        self.foc[instance].shutdown = Some(shutdown);
        Ok(())
    }

    fn validate_foc_shutdown_topology(
        &self,
        phase_u: ResourceBinding,
        phase_v: ResourceBinding,
        phase_w: ResourceBinding,
        current_channel0: ResourceBinding,
        current_channel1: ResourceBinding,
        shutdown: FocShutdownContract,
    ) -> Result<(), ConfigurationError> {
        let stage = self
            .package
            .devices
            .iter()
            .find(|device| device.resource == shutdown.power_stage)
            .ok_or(ConfigurationError::ShutdownContract)?;
        let phases = [phase_u.resource, phase_v.resource, phase_w.resource];
        if phases
            .iter()
            .any(|phase| !stage.auxiliary_resources.contains(phase))
        {
            return Err(ConfigurationError::ShutdownContract);
        }
        let current_channels = [current_channel0.resource, current_channel1.resource];
        if current_channels
            .iter()
            .any(|channel| !stage.auxiliary_resources.contains(channel))
        {
            return Err(ConfigurationError::FocCurrent);
        }
        match shutdown.strategy {
            FocShutdownStrategy::PhaseHighImpedance => {
                let stage_resource = find_resource(self.package, shutdown.power_stage)
                    .ok_or(ConfigurationError::UnknownResource(shutdown.power_stage))?;
                if stage_resource.safe_value != SafeValue::HighImpedance
                    || phases.iter().any(|phase| {
                        find_resource(self.package, *phase)
                            .is_none_or(|resource| resource.safe_value != SafeValue::HighImpedance)
                    })
                {
                    return Err(ConfigurationError::ShutdownContract);
                }
            }
            FocShutdownStrategy::DedicatedEnable | FocShutdownStrategy::DedicatedDisable => {
                let control = shutdown
                    .control
                    .ok_or(ConfigurationError::ShutdownContract)?;
                if !stage.auxiliary_resources.contains(&control) {
                    return Err(ConfigurationError::ShutdownContract);
                }
                let descriptor = find_resource(self.package, control)
                    .ok_or(ConfigurationError::UnknownResource(control))?;
                let safe_is_active = match (descriptor.safe_value, shutdown.control_polarity) {
                    (SafeValue::High, SignalPolarity::ActiveHigh)
                    | (SafeValue::Low, SignalPolarity::ActiveLow) => true,
                    (SafeValue::Low, SignalPolarity::ActiveHigh)
                    | (SafeValue::High, SignalPolarity::ActiveLow) => false,
                    _ => return Err(ConfigurationError::ShutdownContract),
                };
                if safe_is_active
                    != matches!(shutdown.strategy, FocShutdownStrategy::DedicatedDisable)
                {
                    return Err(ConfigurationError::ShutdownContract);
                }
            }
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

    fn validate_scalar(&mut self, scalar: ExactScalar) -> Result<(), ConfigurationError> {
        match scalar.fact {
            ScalarFact::TimerTickHertz => {
                self.timer_tick_hertz = Some(
                    u64::try_from(scalar.value.numerator)
                        .map_err(|_| ConfigurationError::Scalar)?,
                );
            }
            ScalarFact::StepperOutputQuantumCycles => {
                self.stepper_output_quantum_cycles = Some(
                    u32::try_from(scalar.value.numerator)
                        .map_err(|_| ConfigurationError::Scalar)?,
                );
            }
            _ => {}
        }
        if scalar.fact.axis_fact() {
            let instance = usize::from(scalar.instance);
            let axis = &mut self.axes[instance];
            axis.scalar_mask |= fact_bit(scalar.fact);
            match scalar.fact {
                ScalarFact::AxisPositionMinimumMetres => axis.minimum = Some(scalar.value),
                ScalarFact::AxisPositionMaximumMetres => axis.maximum = Some(scalar.value),
                _ => {}
            }
            if let Some(foc) = self.foc.get_mut(instance) {
                match scalar.fact {
                    ScalarFact::AxisEncoderCountsPerTurn => {
                        foc.encoder_counts_per_turn = Some(scalar.value);
                    }
                    ScalarFact::MotorPolePairs => foc.pole_pairs = Some(scalar.value),
                    ScalarFact::PwmCarrierHertz => foc.pwm_carrier_hz = Some(scalar.value),
                    ScalarFact::PwmDeadTimeSeconds => {
                        foc.pwm_dead_time_seconds = Some(scalar.value);
                    }
                    ScalarFact::ControlRateHertz => foc.control_rate_hz = Some(scalar.value),
                    _ => {}
                }
            }
        }
        Ok(())
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

/// Borrowed, independently validated view of one complete canonical document.
///
/// The view retains no executable output ownership. It is intended for
/// service-side inspection and for the authoritative browser compiler, which
/// must derive physical limits from exactly the same bytes accepted by both
/// firmware cores.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationDocumentView<'a> {
    encoded: &'a [u8],
    header: ConfigurationHeader,
    identity: ConfigurationIdentity,
}

impl<'a> ConfigurationDocumentView<'a> {
    /// Validates syntax, canonical ordering, board capability, cross-record
    /// policy, complete SHA-256 identity, and the caller's binding budget.
    pub fn decode<const MAX_BINDINGS: usize>(
        package: &BoardPackage<'_>,
        encoded: &'a [u8],
        expected_digest: Digest,
    ) -> Result<Self, ConfigurationError> {
        let header_bytes = encoded
            .get(..CONFIGURATION_HEADER_BYTES)
            .ok_or(ConfigurationError::Length)?;
        let header = ConfigurationHeader::decode(header_bytes)?;
        let expected_bytes =
            u32::try_from(encoded.len()).map_err(|_| ConfigurationError::Length)?;
        let mut validator = ConfigurationStreamValidator::<MAX_BINDINGS>::new(
            package,
            expected_digest,
            expected_bytes,
        )?;
        validator.push(encoded)?;
        let identity = validator.finish()?;
        Ok(Self {
            encoded,
            header,
            identity,
        })
    }

    /// Complete immutable canonical byte representation.
    pub const fn encoded(self) -> &'a [u8] {
        self.encoded
    }

    /// Canonical document header.
    pub const fn header(self) -> ConfigurationHeader {
        self.header
    }

    /// SHA-256 identity and semantic summary proven at construction.
    pub const fn identity(self) -> ConfigurationIdentity {
        self.identity
    }

    /// Allocation-free iteration over the already validated record region.
    pub fn records(self) -> ConfigurationRecordIter<'a> {
        ConfigurationRecordIter {
            remaining: &self.encoded[CONFIGURATION_HEADER_BYTES..],
        }
    }

    /// Finds one exact logical resource binding. Canonical validation proves
    /// that a matching key is unique.
    pub fn binding(
        self,
        instance: u16,
        role: BindingRole,
    ) -> Result<Option<ResourceBinding>, ConfigurationError> {
        for record in self.records() {
            if let ConfigurationRecord::Binding(binding) = record?
                && binding.instance == instance
                && binding.role == role
            {
                return Ok(Some(binding));
            }
        }
        Ok(None)
    }

    /// Finds one exact scalar fact. Canonical validation proves that a
    /// matching key is unique.
    pub fn scalar(
        self,
        instance: u16,
        fact: ScalarFact,
    ) -> Result<Option<ExactScalar>, ConfigurationError> {
        for record in self.records() {
            if let ConfigurationRecord::Scalar(scalar) = record?
                && scalar.instance == instance
                && scalar.fact == fact
            {
                return Ok(Some(scalar));
            }
        }
        Ok(None)
    }
}

/// Exact-size iterator over a validated canonical document's records.
#[derive(Clone, Debug)]
pub struct ConfigurationRecordIter<'a> {
    remaining: &'a [u8],
}

impl Iterator for ConfigurationRecordIter<'_> {
    type Item = Result<ConfigurationRecord, ConfigurationError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining.is_empty() {
            return None;
        }
        if self.remaining.len() < CONFIGURATION_RECORD_BYTES {
            self.remaining = &[];
            return Some(Err(ConfigurationError::Length));
        }
        let (record, remaining) = self.remaining.split_at(CONFIGURATION_RECORD_BYTES);
        self.remaining = remaining;
        Some(ConfigurationRecord::decode(record))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let records = self.remaining.len().div_ceil(CONFIGURATION_RECORD_BYTES);
        (records, Some(records))
    }
}

impl ExactSizeIterator for ConfigurationRecordIter<'_> {}
impl core::iter::FusedIterator for ConfigurationRecordIter<'_> {}

/// Core-1-only executable facts paired with the compact identity of the exact
/// document from which they were independently derived.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RealtimeConfiguration {
    identity: ConfigurationIdentity,
    profile: RealtimeConfigurationProfile,
}

impl RealtimeConfiguration {
    /// Exact SHA-256 identity and summary paired with this executable profile.
    pub const fn identity(&self) -> ConfigurationIdentity {
        self.identity
    }

    /// Independently validated, allocation-free core-1 configuration facts.
    pub const fn profile(&self) -> &RealtimeConfigurationProfile {
        &self.profile
    }
}

/// Fixed command prefix before an optional core-to-core configuration byte chunk.
pub const CORE_CONFIGURATION_COMMAND_PREFIX_BYTES: usize = 64;
/// Maximum document bytes transferred in one 256-byte intercore command.
pub const MAX_CORE_CONFIGURATION_DATA_BYTES: usize = 192;
/// Complete fixed command storage, bounded below the default runtime payload.
pub const CORE_CONFIGURATION_COMMAND_CAPACITY: usize =
    CORE_CONFIGURATION_COMMAND_PREFIX_BYTES + MAX_CORE_CONFIGURATION_DATA_BYTES;
/// Exact fixed core-1 configuration report bytes, filling one telemetry payload.
pub const CORE_CONFIGURATION_REPORT_BYTES: usize = 128;

const CORE_COMMAND_MAGIC: [u8; 4] = *b"ALCC";
const CORE_REPORT_MAGIC: [u8; 4] = *b"ALCR";
const CORE_WIRE_VERSION: u16 = 2;

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
    /// Marks the exact active identity as durably committed and available to
    /// other real-time actors. This always follows the core-0 media commit.
    Authorize = 7,
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
            7 => Some(Self::Authorize),
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

    /// Confirms that core 0 durably committed the exact active identity.
    pub fn authorize(
        transaction_id: u64,
        digest: Digest,
        total_bytes: u32,
    ) -> Result<Self, CoreConfigurationWireError> {
        Self::without_data(
            CoreConfigurationAction::Authorize,
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
            | CoreConfigurationAction::Authorize
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
    Shutdown = 12,
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
            12 => Some(Self::Shutdown),
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
            ConfigurationError::ShutdownContract | ConfigurationError::ShutdownUnqualified => {
                Self::Shutdown
            }
            ConfigurationError::Timing | ConfigurationError::Frequency => Self::Timing,
            ConfigurationError::Rational
            | ConfigurationError::Uncertainty
            | ConfigurationError::Scalar
            | ConfigurationError::Evidence
            | ConfigurationError::FocRuntime
            | ConfigurationError::FocRotor
            | ConfigurationError::FocCurrent
            | ConfigurationError::FocHardware => Self::ExactFact,
            ConfigurationError::IncompleteAxis
            | ConfigurationError::AxisKind
            | ConfigurationError::AxisCount
            | ConfigurationError::AxisRange
            | ConfigurationError::MotionPolicy => Self::Completeness,
            ConfigurationError::BindingCapacity | ConfigurationError::SafetyInputCapacity => {
                Self::Capacity
            }
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
    /// True only after core 0 confirmed the exact active identity is durable.
    pub active_authorized: bool,
}

impl RealtimeConfigurationReport {
    /// Canonical boot state before core 1 receives any configuration bytes.
    pub const fn empty() -> Self {
        Self {
            state: RealtimeConfigurationState::Empty,
            transaction_id: 0,
            digest: Digest::ZERO,
            total_bytes: 0,
            consumed_bytes: 0,
            summary: None,
            fault: ConfigurationFaultCode::None,
            active_digest: Digest::ZERO,
            active_bytes: 0,
            active_authorized: false,
        }
    }

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
        encoded[71] = u8::from(self.active_authorized);
        // Bytes 108..128 are reserved zero.
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
            || encoded[71] & !1 != 0
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
            active_authorized: encoded[71] != 0,
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
            || self.active_authorized && self.active_digest.is_zero()
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
                    && self.active_digest.is_zero()
                    && !self.active_authorized =>
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

/// Core-0 lifecycle phase exposed by authenticated configuration status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ConfigurationCoordinatorPhase {
    Empty = 0,
    Recovering = 1,
    Validating = 2,
    CandidateValid = 3,
    Preparing = 4,
    Activating = 5,
    Committing = 6,
    Authorizing = 7,
    Active = 8,
    Clearing = 9,
    Aborting = 10,
    Rejected = 11,
}

impl ConfigurationCoordinatorPhase {
    const fn from_wire(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Empty),
            1 => Some(Self::Recovering),
            2 => Some(Self::Validating),
            3 => Some(Self::CandidateValid),
            4 => Some(Self::Preparing),
            5 => Some(Self::Activating),
            6 => Some(Self::Committing),
            7 => Some(Self::Authorizing),
            8 => Some(Self::Active),
            9 => Some(Self::Clearing),
            10 => Some(Self::Aborting),
            11 => Some(Self::Rejected),
            _ => None,
        }
    }
}

/// Stable core-0 failure family for configuration lifecycle status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
pub enum ConfigurationCoordinatorFault {
    None = 0,
    Request = 1,
    Storage = 2,
    ServiceValidation = 3,
    RealtimeValidation = 4,
    SafetyState = 5,
    Durability = 6,
    Protocol = 7,
    Internal = 8,
}

impl ConfigurationCoordinatorFault {
    const fn from_wire(value: u16) -> Option<Self> {
        match value {
            0 => Some(Self::None),
            1 => Some(Self::Request),
            2 => Some(Self::Storage),
            3 => Some(Self::ServiceValidation),
            4 => Some(Self::RealtimeValidation),
            5 => Some(Self::SafetyState),
            6 => Some(Self::Durability),
            7 => Some(Self::Protocol),
            8 => Some(Self::Internal),
            _ => None,
        }
    }
}

/// Flags in the fixed coordinator status body.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct ConfigurationCoordinatorFlags(pub u8);

impl ConfigurationCoordinatorFlags {
    /// The raw-media journal contains this operation's prepared transition.
    pub const DURABLE_PREPARED: u8 = 1 << 0;
    /// The operation was synthesized from a committed boot selection.
    pub const BOOT_RECOVERY: u8 = 1 << 1;
    /// Both job actors received the exact durably committed active digest.
    pub const JOBS_AUTHORIZED: u8 = 1 << 2;
    /// The status carries a core-0 independently validated summary.
    pub const CORE0_VALID: u8 = 1 << 3;

    const KNOWN: u8 =
        Self::DURABLE_PREPARED | Self::BOOT_RECOVERY | Self::JOBS_AUTHORIZED | Self::CORE0_VALID;

    /// Tests one known status bit.
    pub const fn contains(self, flag: u8) -> bool {
        self.0 & flag != 0
    }
}

/// Fixed authenticated status joining core-0 progress, durable selection, and
/// the latest independently decoded core-1 report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ConfigurationCoordinatorStatus {
    pub phase: ConfigurationCoordinatorPhase,
    pub flags: ConfigurationCoordinatorFlags,
    pub fault: ConfigurationCoordinatorFault,
    pub operation_transaction_id: u64,
    pub operation_digest: Digest,
    pub operation_bytes: u32,
    pub validated_bytes: u32,
    pub storage_chunks_read: u32,
    pub active_transaction_id: u64,
    pub active_digest: Digest,
    pub active_bytes: u32,
    pub summary: Option<ConfigurationSummary>,
    pub realtime: RealtimeConfigurationReport,
}

impl ConfigurationCoordinatorStatus {
    /// Encodes the exact 264-byte V5 status body.
    pub fn encode(
        self,
    ) -> Result<[u8; CONFIGURATION_COORDINATOR_STATUS_BYTES], ConfigurationCoordinatorStatusError>
    {
        self.validate()?;
        let mut encoded = [0_u8; CONFIGURATION_COORDINATOR_STATUS_BYTES];
        encoded[0..8].copy_from_slice(&COORDINATOR_STATUS_MAGIC);
        encoded[8..10].copy_from_slice(&CONFIGURATION_VERSION.to_le_bytes());
        encoded[10] = self.phase as u8;
        encoded[11] = self.flags.0;
        encoded[12..14].copy_from_slice(&(self.fault as u16).to_le_bytes());
        // Bytes 14..16, 68..72, and 116..120 are reserved zero.
        encoded[16..24].copy_from_slice(&self.operation_transaction_id.to_le_bytes());
        encoded[24..56].copy_from_slice(&self.operation_digest.0);
        encoded[56..60].copy_from_slice(&self.operation_bytes.to_le_bytes());
        encoded[60..64].copy_from_slice(&self.validated_bytes.to_le_bytes());
        encoded[64..68].copy_from_slice(&self.storage_chunks_read.to_le_bytes());
        encoded[72..80].copy_from_slice(&self.active_transaction_id.to_le_bytes());
        encoded[80..112].copy_from_slice(&self.active_digest.0);
        encoded[112..116].copy_from_slice(&self.active_bytes.to_le_bytes());
        if let Some(summary) = self.summary {
            encoded[120..122].copy_from_slice(&summary.record_count.to_le_bytes());
            encoded[122..124].copy_from_slice(&summary.realtime_record_count.to_le_bytes());
            encoded[124..126].copy_from_slice(&summary.binding_count.to_le_bytes());
            encoded[126] = summary.stepper_axes;
            encoded[127] = summary.foc_axes;
            encoded[128..132].copy_from_slice(&summary.flags.0.to_le_bytes());
            encoded[132] = u8::from(summary.safety_binding);
        }
        // Bytes 133..136 are reserved zero.
        encoded[136..264].copy_from_slice(
            &self
                .realtime
                .encode()
                .map_err(ConfigurationCoordinatorStatusError::Realtime)?,
        );
        Ok(encoded)
    }

    /// Decodes and re-encodes to require the unique V5 representation.
    pub fn decode(encoded: &[u8]) -> Result<Self, ConfigurationCoordinatorStatusError> {
        if encoded.len() != CONFIGURATION_COORDINATOR_STATUS_BYTES {
            return Err(ConfigurationCoordinatorStatusError::Length);
        }
        if encoded[0..8] != COORDINATOR_STATUS_MAGIC {
            return Err(ConfigurationCoordinatorStatusError::Magic);
        }
        if read_u16(encoded, 8) != CONFIGURATION_VERSION {
            return Err(ConfigurationCoordinatorStatusError::Version);
        }
        if encoded[14..16].iter().any(|byte| *byte != 0)
            || encoded[68..72].iter().any(|byte| *byte != 0)
            || encoded[116..120].iter().any(|byte| *byte != 0)
            || encoded[133..136].iter().any(|byte| *byte != 0)
        {
            return Err(ConfigurationCoordinatorStatusError::Reserved);
        }
        let flags = ConfigurationCoordinatorFlags(encoded[11]);
        let summary = if flags.contains(ConfigurationCoordinatorFlags::CORE0_VALID) {
            Some(ConfigurationSummary {
                record_count: read_u16(encoded, 120),
                realtime_record_count: read_u16(encoded, 122),
                binding_count: read_u16(encoded, 124),
                stepper_axes: encoded[126],
                foc_axes: encoded[127],
                safety_binding: encoded[132] != 0,
                flags: ConfigurationFlags(read_u32(encoded, 128)),
            })
        } else {
            if encoded[120..133].iter().any(|byte| *byte != 0) {
                return Err(ConfigurationCoordinatorStatusError::Reserved);
            }
            None
        };
        let mut operation_digest = [0_u8; 32];
        operation_digest.copy_from_slice(&encoded[24..56]);
        let mut active_digest = [0_u8; 32];
        active_digest.copy_from_slice(&encoded[80..112]);
        let status = Self {
            phase: ConfigurationCoordinatorPhase::from_wire(encoded[10])
                .ok_or(ConfigurationCoordinatorStatusError::Phase)?,
            flags,
            fault: ConfigurationCoordinatorFault::from_wire(read_u16(encoded, 12))
                .ok_or(ConfigurationCoordinatorStatusError::Fault)?,
            operation_transaction_id: read_u64(encoded, 16),
            operation_digest: Digest(operation_digest),
            operation_bytes: read_u32(encoded, 56),
            validated_bytes: read_u32(encoded, 60),
            storage_chunks_read: read_u32(encoded, 64),
            active_transaction_id: read_u64(encoded, 72),
            active_digest: Digest(active_digest),
            active_bytes: read_u32(encoded, 112),
            summary,
            realtime: RealtimeConfigurationReport::decode(&encoded[136..264])
                .map_err(ConfigurationCoordinatorStatusError::Realtime)?,
        };
        status.validate()?;
        if status.encode()? != encoded {
            return Err(ConfigurationCoordinatorStatusError::Noncanonical);
        }
        Ok(status)
    }

    fn validate(self) -> Result<(), ConfigurationCoordinatorStatusError> {
        if self.flags.0 & !ConfigurationCoordinatorFlags::KNOWN != 0 {
            return Err(ConfigurationCoordinatorStatusError::Flags);
        }
        if self.summary.is_some()
            != self
                .flags
                .contains(ConfigurationCoordinatorFlags::CORE0_VALID)
            || self
                .summary
                .is_some_and(|summary| summary.validate().is_err())
        {
            return Err(ConfigurationCoordinatorStatusError::Summary);
        }
        let operation_empty = self.operation_transaction_id == 0
            && self.operation_digest.is_zero()
            && self.operation_bytes == 0
            && self.validated_bytes == 0
            && self.storage_chunks_read == 0;
        let operation_valid = self.operation_transaction_id != 0
            && !self.operation_digest.is_zero()
            && configuration_length_valid(self.operation_bytes)
            && self.validated_bytes <= self.operation_bytes;
        if !operation_empty && !operation_valid {
            return Err(ConfigurationCoordinatorStatusError::OperationIdentity);
        }
        let active_empty = self.active_transaction_id == 0
            && self.active_digest.is_zero()
            && self.active_bytes == 0;
        let active_valid = self.active_transaction_id != 0
            && !self.active_digest.is_zero()
            && configuration_length_valid(self.active_bytes);
        if !active_empty && !active_valid {
            return Err(ConfigurationCoordinatorStatusError::ActiveIdentity);
        }
        if (self
            .flags
            .contains(ConfigurationCoordinatorFlags::DURABLE_PREPARED)
            || self
                .flags
                .contains(ConfigurationCoordinatorFlags::BOOT_RECOVERY))
            && !operation_valid
        {
            return Err(ConfigurationCoordinatorStatusError::Flags);
        }
        if self
            .flags
            .contains(ConfigurationCoordinatorFlags::JOBS_AUTHORIZED)
            && (!active_valid
                || !self.realtime.active_authorized
                || self.realtime.active_digest != self.active_digest
                || self.realtime.active_bytes != self.active_bytes)
        {
            return Err(ConfigurationCoordinatorStatusError::Authorization);
        }
        match self.phase {
            ConfigurationCoordinatorPhase::Empty
                if operation_empty
                    && active_empty
                    && self.fault == ConfigurationCoordinatorFault::None =>
            {
                Ok(())
            }
            ConfigurationCoordinatorPhase::Active
                if active_valid
                    && self
                        .flags
                        .contains(ConfigurationCoordinatorFlags::JOBS_AUTHORIZED)
                    && self.fault == ConfigurationCoordinatorFault::None =>
            {
                Ok(())
            }
            ConfigurationCoordinatorPhase::Rejected
                if self.fault != ConfigurationCoordinatorFault::None =>
            {
                Ok(())
            }
            _ if self.phase != ConfigurationCoordinatorPhase::Empty
                && self.phase != ConfigurationCoordinatorPhase::Active
                && self.phase != ConfigurationCoordinatorPhase::Rejected
                && operation_valid
                && self.fault == ConfigurationCoordinatorFault::None =>
            {
                Ok(())
            }
            _ => Err(ConfigurationCoordinatorStatusError::StateShape),
        }
    }
}

/// Canonical coordinator-status rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigurationCoordinatorStatusError {
    Length,
    Magic,
    Version,
    Reserved,
    Noncanonical,
    Phase,
    Fault,
    Flags,
    Summary,
    OperationIdentity,
    ActiveIdentity,
    Authorization,
    StateShape,
    Realtime(CoreConfigurationReportError),
}

/// Core-1 owner of candidate bytes, independent semantic validation, and active identity.
pub struct RealtimeConfigurationService<'a, const MAX_BINDINGS: usize> {
    package: &'a BoardPackage<'a>,
    receiving: Option<ConfigurationStreamValidator<'a, MAX_BINDINGS>>,
    transaction_id: u64,
    digest: Digest,
    total_bytes: u32,
    consumed_bytes: u32,
    candidate: Option<RealtimeConfiguration>,
    active: Option<RealtimeConfiguration>,
    active_authorized: bool,
    cleared: bool,
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
            active_authorized: false,
            cleared: false,
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
                self.cleared = false;
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
                match validator.finish_with_profile() {
                    Ok((identity, profile)) => {
                        self.candidate = Some(RealtimeConfiguration { identity, profile });
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
                        .as_ref()
                        .is_none_or(|candidate| !identity_matches(candidate.identity, command))
                {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                self.active = self.candidate.take();
                self.active_authorized = false;
                self.cleared = false;
                self.receiving = None;
                self.last_fault = ConfigurationFaultCode::None;
                self.report()
            }
            CoreConfigurationAction::Clear => {
                if self
                    .active
                    .as_ref()
                    .is_some_and(|active| !identity_matches(active.identity, command))
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
                    active_authorized: false,
                };
                self.receiving = None;
                self.candidate = None;
                self.active = None;
                self.active_authorized = false;
                self.transaction_id = command.transaction_id;
                self.digest = command.digest;
                self.total_bytes = command.total_bytes;
                self.consumed_bytes = command.total_bytes;
                self.last_fault = ConfigurationFaultCode::None;
                self.cleared = true;
                report
            }
            CoreConfigurationAction::Abort => {
                if !self.matches(command) {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                self.receiving = None;
                self.candidate = None;
                self.cleared = false;
                self.last_fault = ConfigurationFaultCode::Identity;
                self.report()
            }
            CoreConfigurationAction::Authorize => {
                if self
                    .active
                    .as_ref()
                    .is_none_or(|active| !identity_matches(active.identity, command))
                    || self.candidate.is_some()
                    || self.receiving.is_some()
                {
                    return self.reject(command, ConfigurationFaultCode::Sequence);
                }
                self.active_authorized = true;
                self.last_fault = ConfigurationFaultCode::None;
                self.report()
            }
        }
    }

    /// Latest state suitable for periodic replay after lossy telemetry.
    pub fn report(&self) -> RealtimeConfigurationReport {
        if let Some(candidate) = self.candidate.as_ref() {
            return identity_report(
                RealtimeConfigurationState::CandidateValid,
                self.transaction_id,
                candidate.identity,
                self.active_identity(),
                self.active_authorized,
            );
        }
        let (active_digest, active_bytes) = active_fields(self.active_identity());
        if self.cleared {
            return RealtimeConfigurationReport {
                state: RealtimeConfigurationState::Cleared,
                transaction_id: self.transaction_id,
                digest: self.digest,
                total_bytes: self.total_bytes,
                consumed_bytes: self.total_bytes,
                summary: None,
                fault: ConfigurationFaultCode::None,
                active_digest: Digest::ZERO,
                active_bytes: 0,
                active_authorized: false,
            };
        }
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
                active_authorized: self.active_authorized,
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
                active_authorized: self.active_authorized,
            };
        }
        if let Some(active) = self.active.as_ref() {
            return identity_report(
                RealtimeConfigurationState::Active,
                self.transaction_id,
                active.identity,
                Some(active.identity),
                self.active_authorized,
            );
        }
        RealtimeConfigurationReport::empty()
    }

    /// Exact identity independently active on core 1.
    pub fn active_identity(&self) -> Option<ConfigurationIdentity> {
        self.active
            .as_ref()
            .map(|configuration| configuration.identity)
    }

    /// Independently validated active executable configuration. Physical
    /// core-1 owners use this immediately after activation to establish input
    /// modes; job authority remains gated by [`Self::authorized_configuration`].
    pub const fn active_configuration(&self) -> Option<&RealtimeConfiguration> {
        self.active.as_ref()
    }

    /// Active identity admitted to other core-1 actors only after durable
    /// service-core confirmation.
    pub fn authorized_identity(&self) -> Option<ConfigurationIdentity> {
        if self.active_authorized {
            self.active_identity()
        } else {
            None
        }
    }

    /// Active executable profile admitted to other core-1 actors only after
    /// durable service-core confirmation of the paired exact identity.
    pub fn authorized_configuration(&self) -> Option<&RealtimeConfiguration> {
        if self.active_authorized {
            self.active.as_ref()
        } else {
            None
        }
    }

    /// Exact independently validated but inactive candidate.
    pub fn candidate_identity(&self) -> Option<ConfigurationIdentity> {
        self.candidate
            .as_ref()
            .map(|configuration| configuration.identity)
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
        self.cleared = false;
        RealtimeConfigurationReport {
            state: RealtimeConfigurationState::Rejected,
            transaction_id: self.transaction_id,
            digest: self.digest,
            total_bytes: self.total_bytes,
            consumed_bytes: self.consumed_bytes,
            summary: None,
            fault,
            active_digest: active_fields(self.active_identity()).0,
            active_bytes: active_fields(self.active_identity()).1,
            active_authorized: self.active_authorized,
        }
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
            && (byte_len - CONFIGURATION_HEADER_BYTES).is_multiple_of(CONFIGURATION_RECORD_BYTES)
    })
}

fn identity_report(
    state: RealtimeConfigurationState,
    transaction_id: u64,
    identity: ConfigurationIdentity,
    active: Option<ConfigurationIdentity>,
    active_authorized: bool,
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
        active_authorized,
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
        self.finish_with_profile().map(|(identity, _)| identity)
    }

    /// Requires the same exact completion as [`Self::finish`] while retaining
    /// the executable profile for its sole real-time owner.
    pub fn finish_with_profile(
        self,
    ) -> Result<(ConfigurationIdentity, RealtimeConfigurationProfile), ConfigurationError> {
        if self.consumed != self.expected_bytes
            || self.header_used != CONFIGURATION_HEADER_BYTES
            || self.record_used != 0
        {
            return Err(ConfigurationError::Length);
        }
        let validator = self.validator.ok_or(ConfigurationError::Length)?;
        let capability_digest = validator.header.capability_digest;
        let (summary, realtime_profile) = validator.finish_with_profile()?;
        let hash = self.hasher.finalize();
        let mut digest = [0_u8; 32];
        digest.copy_from_slice(&hash);
        let digest = Digest(digest);
        if digest != self.expected_digest {
            return Err(ConfigurationError::ConfigurationIdentity);
        }
        Ok((
            ConfigurationIdentity {
                digest,
                byte_len: self.expected_bytes,
                capability_digest,
                summary,
            },
            realtime_profile,
        ))
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
    SafetyInputCapacity,
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
    AxisKind,
    AxisCount,
    AxisRange,
    ShutdownContract,
    ShutdownUnqualified,
    FocRuntime,
    FocRotor,
    FocCurrent,
    FocHardware,
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
        | BindingRole::AxisDisable
        | BindingRole::DigitalOutput
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

const fn role_bit(role: BindingRole) -> u64 {
    let bit = (role as u16).saturating_sub(1);
    if bit < 64 { 1_u64 << bit } else { 0 }
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

const fn read_i32(bytes: &[u8], offset: usize) -> i32 {
    i32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

const fn rotor_direction_wire(direction: RotorCountDirection) -> u16 {
    match direction {
        RotorCountDirection::Increasing => 1,
        RotorCountDirection::Decreasing => 2,
    }
}

const fn rotor_direction_from_wire(value: u16) -> Option<RotorCountDirection> {
    match value {
        1 => Some(RotorCountDirection::Increasing),
        2 => Some(RotorCountDirection::Decreasing),
        _ => None,
    }
}

const fn current_polarity_wire(polarity: CurrentPolarity) -> u8 {
    match polarity {
        CurrentPolarity::Increasing => 1,
        CurrentPolarity::Decreasing => 2,
    }
}

const fn current_polarity_from_wire(value: u8) -> Option<CurrentPolarity> {
    match value {
        1 => Some(CurrentPolarity::Increasing),
        2 => Some(CurrentPolarity::Decreasing),
        _ => None,
    }
}

const fn phase_pair_wire(pair: TwoShuntPhasePair) -> u16 {
    match pair {
        TwoShuntPhasePair::Ab => 1,
        TwoShuntPhasePair::Bc => 2,
        TwoShuntPhasePair::Ca => 3,
    }
}

const fn phase_pair_from_wire(value: u16) -> Option<TwoShuntPhasePair> {
    match value {
        1 => Some(TwoShuntPhasePair::Ab),
        2 => Some(TwoShuntPhasePair::Bc),
        3 => Some(TwoShuntPhasePair::Ca),
        _ => None,
    }
}

fn rational_equals_u32(value: Rational, expected: u32) -> bool {
    i128::from(value.numerator) == i128::from(expected) * i128::from(value.denominator)
}

fn rational_equals_ratio(value: Rational, numerator: u32, denominator: u32) -> bool {
    denominator != 0
        && i128::from(value.numerator) * i128::from(denominator)
            == i128::from(numerator) * i128::from(value.denominator)
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
    use alumina_capability::calculate_identity;
    use alumina_safety::SafetyInputMonitor;
    use alumina_storage::media::{
        ConfigurationTransition, DurableConfigurationSelection, MEDIA_BLOCK_BYTES, MediaBlock,
        MediaId, MediaRegion,
    };
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
        let safety_input = role.safety_input_role().is_some();
        let required_interlock = matches!(
            role,
            BindingRole::EmergencyStop | BindingRole::SafetyInterlock
        );
        ConfigurationRecord::Binding(ResourceBinding {
            instance,
            role,
            resource,
            owner: OwnerDomain::Realtime,
            polarity,
            flags: BindingFlags(if required_interlock {
                BindingFlags::REQUIRED_INTERLOCK
            } else {
                0
            }),
            minimum_active_cycles: u32::from(timed) * 48,
            minimum_inactive_cycles: u32::from(timed) * 48,
            maximum_frequency_hz: u32::from(timed) * 100_000,
            watchdog_cycles: if timed || role.is_hazardous_role() || safety_input {
                240_000
            } else {
                0
            },
        })
    }

    fn sampled_binding(
        instance: u16,
        role: BindingRole,
        resource: ResourceId,
        maximum_frequency_hz: u32,
    ) -> ConfigurationRecord {
        ConfigurationRecord::Binding(ResourceBinding {
            instance,
            role,
            resource,
            owner: OwnerDomain::Realtime,
            polarity: SignalPolarity::NotApplicable,
            flags: BindingFlags::default(),
            minimum_active_cycles: 0,
            minimum_inactive_cycles: 0,
            maximum_frequency_hz,
            watchdog_cycles: 240_000,
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

    fn foc_shutdown(
        strategy: FocShutdownStrategy,
        control: Option<ResourceId>,
        control_polarity: SignalPolarity,
    ) -> ConfigurationRecord {
        ConfigurationRecord::FocShutdown(FocShutdownContract {
            instance: 0,
            strategy,
            power_stage: ResourceId::Device(board_mks_esp32_foc_v1::device::POWER_STAGE_0),
            control,
            control_polarity,
            maximum_transition_cycles: 2_400,
            evidence: FactEvidence::Qualified,
        })
    }

    fn mks_foc_records(shutdown: ConfigurationRecord) -> Vec<ConfigurationRecord> {
        let controller = PiConfig {
            proportional_gain: Q30::ZERO,
            integral_gain_per_update: Q30::ZERO,
            integral_minimum: Q30::NEG_ONE,
            integral_maximum: Q30::ONE,
            output_minimum: Q30::from_bits(-Q30::HALF.bits()),
            output_maximum: Q30::HALF,
        };
        let current_channel = CurrentChannelCalibration {
            adc_maximum_count: 4_095,
            valid_count_minimum: 1_200,
            valid_count_maximum: 2_800,
            count_at_zero: 2_000,
            polarity: CurrentPolarity::Increasing,
            normalized_current_per_count: Q30Interval::point(Q30::from_bits(1 << 19)),
            maximum_additive_error: Q30::from_bits(1 << 18),
            maximum_interval_width_ulps: 1 << 19,
        };
        let mut records = Vec::from([
            binding(
                0,
                BindingRole::FocPhaseU,
                ResourceId::TimedOutput {
                    engine: 0,
                    channel: 0,
                },
                SignalPolarity::ActiveHigh,
                true,
            ),
            binding(
                0,
                BindingRole::FocPhaseV,
                ResourceId::TimedOutput {
                    engine: 0,
                    channel: 1,
                },
                SignalPolarity::ActiveHigh,
                true,
            ),
            binding(
                0,
                BindingRole::FocPhaseW,
                ResourceId::TimedOutput {
                    engine: 0,
                    channel: 2,
                },
                SignalPolarity::ActiveHigh,
                true,
            ),
            binding(
                0,
                BindingRole::EmergencyStop,
                ResourceId::Gpio(15),
                SignalPolarity::ActiveLow,
                false,
            ),
            sampled_binding(
                0,
                BindingRole::FocCurrentA,
                ResourceId::Adc {
                    unit: 1,
                    channel: 3,
                },
                20_000,
            ),
            sampled_binding(
                0,
                BindingRole::FocCurrentB,
                ResourceId::Adc {
                    unit: 1,
                    channel: 0,
                },
                20_000,
            ),
            sampled_binding(
                0,
                BindingRole::FocEncoder,
                ResourceId::Device(board_mks_esp32_foc_v1::device::ENCODER_0),
                1_000,
            ),
            shutdown,
        ]);
        records.extend([
            scalar(0, ScalarFact::AxisEncoderCountsPerTurn, rational(4_096, 1)),
            scalar(0, ScalarFact::MotorPolePairs, rational(7, 1)),
            scalar(0, ScalarFact::MotorCurrentLimitAmperes, rational(1, 2)),
            scalar(0, ScalarFact::MotorVoltageLimitVolts, rational(6, 1)),
            scalar(0, ScalarFact::PwmCarrierHertz, rational(20_000, 1)),
            scalar(0, ScalarFact::PwmDeadTimeSeconds, rational(1, 10_000_000)),
            scalar(0, ScalarFact::ControlRateHertz, rational(20_000, 1)),
            scalar(0, ScalarFact::CurrentSenseOhms, rational(1, 100)),
            scalar(0, ScalarFact::CurrentSenseVoltsPerAmpere, rational(1, 1)),
            scalar(0, ScalarFact::TimerTickHertz, rational(1_000_000, 1)),
            ConfigurationRecord::FocRuntime(FocRuntimeParameters {
                instance: 0,
                pole_pairs: 7,
                timing: FocTimingProfile {
                    pwm_hz: 20_000,
                    current_loop_hz: 20_000,
                    velocity_loop_divider: 20,
                    position_loop_divider: 10,
                },
                maximum_phase_current: Q30::ONE,
                maximum_phase_voltage: Q30::ONE,
            }),
            ConfigurationRecord::FocController(FocControllerParameters {
                instance: 0,
                axis: FocControllerAxis::Direct,
                parameters: controller,
            }),
            ConfigurationRecord::FocController(FocControllerParameters {
                instance: 0,
                axis: FocControllerAxis::Quadrature,
                parameters: controller,
            }),
            ConfigurationRecord::FocRotor(FocRotorParameters {
                instance: 0,
                counts_per_mechanical_turn: 4_096,
                count_at_reference: 0,
                electrical_phase_at_reference: ElectricalPhase::ZERO,
                direction: RotorCountDirection::Increasing,
                maximum_alignment_error_bits: 1_024,
                maximum_count_error: CountUncertainty::new(1, 2).unwrap(),
                rotation_precision: RotationPrecision {
                    maximum_component_width_ulps: 12_000_000,
                    maximum_norm_error_ulps: 12_000_000,
                },
                evidence: FactEvidence::Measured,
            }),
            ConfigurationRecord::FocCurrentChannel(FocCurrentChannelParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel0,
                calibration: current_channel,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocCurrentChannel(FocCurrentChannelParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel1,
                calibration: current_channel,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocPwmAdcTiming(FocPwmAdcTimingParameters {
                instance: 0,
                phase_pair: TwoShuntPhasePair::Ab,
                device_cycle_hz: 80_000_000,
                pwm_period_cycles: 4_000,
                nominal_acquisition_offset_cycles: 2_000,
                maximum_trigger_jitter_cycles: 2,
                maximum_acquisition_cycles: 20,
                maximum_channel_skew_cycles: 8,
                maximum_conversion_cycles: 40,
                minimum_switching_guard_cycles: 50,
                maximum_normalized_current_slew_per_cycle: Q30::from_bits(1 << 14),
                maximum_interchannel_skew_error: Q30::from_bits(1 << 17),
                maximum_phase_current: Q30::ONE,
                maximum_phase_interval_width_ulps: 1 << 22,
                pwm_dead_time_cycles: 8,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocAdcFrontend(FocAdcFrontendParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel0,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocAdcFrontend(FocAdcFrontendParameters {
                instance: 0,
                channel: FocCurrentChannel::Channel1,
                attenuation: FocAdcAttenuation::Db11,
                evidence: FactEvidence::Qualified,
            }),
            ConfigurationRecord::FocPwmHardware(FocPwmHardwareParameters {
                instance: 0,
                peripheral_source_clock_hz: 160_000_000,
                counter_clock_hz: 80_000_000,
                timer_peak_ticks: 2_000,
                minimum_active_ticks: 8,
                maximum_quantization_error_ulps: 1_000_000,
                peripheral_prescaler: 1,
                timer_prescaler: 0,
                evidence: FactEvidence::Qualified,
            }),
        ]);
        records.sort_by_key(|record| record.canonical_order_key());
        records
    }

    fn qualified_mks_package<'a>(
        devices: &'a [alumina_board::DeviceDescriptor<'a>],
    ) -> BoardPackage<'a> {
        reidentified_package(BoardPackage {
            devices,
            ..board_mks_esp32_foc_v1::PACKAGE
        })
    }

    fn reidentified_package(mut package: BoardPackage<'_>) -> BoardPackage<'_> {
        package.board.capability_digest = Digest([1; 32]);
        package.board.capability_digest = calculate_identity(&package).unwrap().digest;
        package
    }

    fn tinybee_motion_records() -> Vec<ConfigurationRecord> {
        let mut records = Vec::from([
            binding(
                0,
                BindingRole::AxisStep,
                ResourceId::I2sOut { engine: 0, bit: 1 },
                SignalPolarity::ActiveHigh,
                true,
            ),
            binding(
                0,
                BindingRole::AxisDirection,
                ResourceId::I2sOut { engine: 0, bit: 2 },
                SignalPolarity::ActiveHigh,
                true,
            ),
            binding(
                0,
                BindingRole::AxisDisable,
                ResourceId::I2sOut { engine: 0, bit: 0 },
                SignalPolarity::ActiveHigh,
                true,
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
            scalar(
                0,
                ScalarFact::AxisFollowingErrorMetres,
                rational(1, 100_000),
            ),
            scalar(0, ScalarFact::TimerTickHertz, rational(1_000_000, 1)),
            scalar(0, ScalarFact::StepperOutputQuantumCycles, rational(1, 1)),
        ]);
        records.sort_by_key(|record| record.canonical_order_key());
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
    fn validated_document_view_exposes_exact_canonical_facts_without_allocation() {
        let records = tinybee_motion_records();
        let flags = ConfigurationFlags(ConfigurationFlags::MOTION);
        let (bytes, digest) = document(&board_mks_tinybee::PACKAGE, &records, flags);
        let view =
            ConfigurationDocumentView::decode::<32>(&board_mks_tinybee::PACKAGE, &bytes, digest)
                .unwrap();

        assert_eq!(view.encoded(), bytes);
        assert_eq!(view.header().record_count, records.len() as u16);
        assert_eq!(view.header().flags, flags);
        assert_eq!(view.identity().digest, digest);
        assert_eq!(
            view.identity().capability_digest,
            board_mks_tinybee::CAPABILITY_DIGEST
        );
        assert_eq!(view.records().len(), records.len());
        assert_eq!(
            view.records().collect::<Result<Vec<_>, _>>().unwrap(),
            records
        );

        let microsteps = view.scalar(0, ScalarFact::AxisMicrosteps).unwrap().unwrap();
        assert_eq!(microsteps.value, rational(16, 1));
        assert_eq!(microsteps.uncertainty, rational(0, 1));
        assert_eq!(
            view.binding(0, BindingRole::AxisStep)
                .unwrap()
                .unwrap()
                .resource,
            ResourceId::I2sOut { engine: 0, bit: 1 }
        );
        assert_eq!(view.binding(1, BindingRole::AxisStep), Ok(None));

        let (_, realtime) = ConfigurationStreamValidator::<32>::new(
            &board_mks_tinybee::PACKAGE,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .and_then(|mut validator| {
            validator.push(&bytes)?;
            validator.finish_with_profile()
        })
        .unwrap();
        assert_eq!(realtime.timer_tick_hertz(), Some(1_000_000));
        assert_eq!(realtime.stepper_output_quantum_cycles(), Some(1));

        assert_eq!(
            ConfigurationDocumentView::decode::<32>(
                &board_mks_tinybee::PACKAGE,
                &bytes,
                Digest([0x55; 32]),
            ),
            Err(ConfigurationError::ConfigurationIdentity)
        );
        assert_eq!(
            ConfigurationDocumentView::decode::<3>(&board_mks_tinybee::PACKAGE, &bytes, digest,),
            Err(ConfigurationError::BindingCapacity)
        );

        let mut trailing = bytes.clone();
        trailing.push(0);
        assert_eq!(
            ConfigurationDocumentView::decode::<32>(&board_mks_tinybee::PACKAGE, &trailing, digest,),
            Err(ConfigurationError::ConfigurationIdentity)
        );

        let mut missing_timer = records.clone();
        missing_timer.retain(|record| {
            !matches!(
                record,
                ConfigurationRecord::Scalar(scalar)
                    if scalar.fact == ScalarFact::TimerTickHertz
            )
        });
        let (missing_bytes, missing_digest) = document(
            &board_mks_tinybee::PACKAGE,
            &missing_timer,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        assert_eq!(
            ConfigurationDocumentView::decode::<32>(
                &board_mks_tinybee::PACKAGE,
                &missing_bytes,
                missing_digest,
            ),
            Err(ConfigurationError::MotionPolicy)
        );

        let mut missing_quantum = records.clone();
        missing_quantum.retain(|record| {
            !matches!(
                record,
                ConfigurationRecord::Scalar(scalar)
                    if scalar.fact == ScalarFact::StepperOutputQuantumCycles
            )
        });
        let (missing_bytes, missing_digest) = document(
            &board_mks_tinybee::PACKAGE,
            &missing_quantum,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        assert_eq!(
            ConfigurationDocumentView::decode::<32>(
                &board_mks_tinybee::PACKAGE,
                &missing_bytes,
                missing_digest,
            ),
            Err(ConfigurationError::MotionPolicy)
        );
        assert_eq!(
            scalar(1, ScalarFact::TimerTickHertz, rational(1_000_000, 1)).encode(),
            Err(ConfigurationError::Scalar)
        );
        assert_eq!(
            scalar(1, ScalarFact::StepperOutputQuantumCycles, rational(1, 1),).encode(),
            Err(ConfigurationError::Scalar)
        );
    }

    #[test]
    fn foc_shutdown_record_is_canonical_and_has_no_v1_enable_selector() {
        let record = foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        );
        let encoded = record.encode().unwrap();
        assert_eq!(ConfigurationRecord::decode(&encoded), Ok(record));
        assert_eq!(read_u16(&encoded, 0), RECORD_KIND_FOC_SHUTDOWN);
        assert_eq!(read_u16(&encoded, 6), 3);

        let mut reserved = encoded;
        reserved[63] = 1;
        assert_eq!(
            ConfigurationRecord::decode(&reserved),
            Err(ConfigurationError::Reserved)
        );

        let mut removed_enable = binding(
            0,
            BindingRole::AxisDirection,
            ResourceId::Gpio(32),
            SignalPolarity::ActiveHigh,
            true,
        )
        .encode()
        .unwrap();
        removed_enable[6..8].copy_from_slice(&28_u16.to_le_bytes());
        assert_eq!(
            ConfigurationRecord::decode(&removed_enable),
            Err(ConfigurationError::Selector)
        );

        let ConfigurationRecord::FocShutdown(mut unqualified) = record else {
            unreachable!();
        };
        unqualified.evidence = FactEvidence::Measured;
        assert_eq!(
            ConfigurationRecord::FocShutdown(unqualified).encode(),
            Err(ConfigurationError::ShutdownContract)
        );
        unqualified.strategy = FocShutdownStrategy::DedicatedEnable;
        unqualified.evidence = FactEvidence::Qualified;
        unqualified.control_polarity = SignalPolarity::ActiveHigh;
        assert_eq!(
            ConfigurationRecord::FocShutdown(unqualified).encode(),
            Err(ConfigurationError::ShutdownContract)
        );
    }

    #[test]
    fn foc_v5_records_have_unique_canonical_fixed_width_encodings() {
        let records = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ));
        let mut checked = 0;
        for record in records {
            let (kind, _, _) = record.canonical_order_key();
            if kind < RECORD_KIND_FOC_RUNTIME {
                continue;
            }
            let encoded = record.encode().unwrap();
            assert_eq!(encoded.len(), CONFIGURATION_RECORD_BYTES);
            assert_eq!(ConfigurationRecord::decode(&encoded), Ok(record));

            let reserved_offset = match kind {
                RECORD_KIND_FOC_RUNTIME => 10,
                RECORD_KIND_FOC_CONTROLLER | RECORD_KIND_FOC_ROTOR => 63,
                RECORD_KIND_FOC_CURRENT_CHANNEL => 18,
                RECORD_KIND_FOC_PWM_ADC_TIMING => 63,
                RECORD_KIND_FOC_ADC_FRONTEND => 63,
                RECORD_KIND_FOC_PWM_HARDWARE => 63,
                _ => unreachable!(),
            };
            let mut noncanonical = encoded;
            noncanonical[reserved_offset] = 1;
            assert_eq!(
                ConfigurationRecord::decode(&noncanonical),
                Err(ConfigurationError::Reserved)
            );
            checked += 1;
        }
        assert_eq!(checked, 10);

        let frontend = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ))
        .into_iter()
        .find(|record| matches!(record, ConfigurationRecord::FocAdcFrontend(_)))
        .unwrap();
        let mut unknown_attenuation = frontend.encode().unwrap();
        unknown_attenuation[8] = 0;
        assert_eq!(
            ConfigurationRecord::decode(&unknown_attenuation),
            Err(ConfigurationError::Selector)
        );
        let ConfigurationRecord::FocAdcFrontend(mut unqualified_frontend) = frontend else {
            unreachable!();
        };
        unqualified_frontend.evidence = FactEvidence::Measured;
        assert_eq!(
            ConfigurationRecord::FocAdcFrontend(unqualified_frontend).encode(),
            Err(ConfigurationError::FocHardware)
        );

        let hardware = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ))
        .into_iter()
        .find(|record| matches!(record, ConfigurationRecord::FocPwmHardware(_)))
        .unwrap();
        let ConfigurationRecord::FocPwmHardware(mut invalid_hardware) = hardware else {
            unreachable!();
        };
        invalid_hardware.minimum_active_ticks = invalid_hardware.timer_peak_ticks / 2;
        assert_eq!(
            ConfigurationRecord::FocPwmHardware(invalid_hardware).encode(),
            Err(ConfigurationError::FocHardware)
        );

        let timing = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ))
        .into_iter()
        .find(|record| matches!(record, ConfigurationRecord::FocPwmAdcTiming(_)))
        .unwrap();
        let mut unknown_pair = timing.encode().unwrap();
        unknown_pair[6..8].copy_from_slice(&4_u16.to_le_bytes());
        assert_eq!(
            ConfigurationRecord::decode(&unknown_pair),
            Err(ConfigurationError::Selector)
        );

        let runtime = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ))
        .into_iter()
        .find(|record| matches!(record, ConfigurationRecord::FocRuntime(_)))
        .unwrap();
        let mut unknown_runtime_selector = runtime.encode().unwrap();
        unknown_runtime_selector[6..8].copy_from_slice(&1_u16.to_le_bytes());
        assert_eq!(
            ConfigurationRecord::decode(&unknown_runtime_selector),
            Err(ConfigurationError::Selector)
        );

        let header = ConfigurationHeader {
            capability_digest: board_mks_esp32_foc_v1::CAPABILITY_DIGEST,
            record_count: 1,
            realtime_record_count: 1,
            flags: ConfigurationFlags::default(),
        };
        let mut old_header = header.encode().unwrap();
        old_header[0..8].copy_from_slice(b"ALMCFG04");
        old_header[8..10].copy_from_slice(&4_u16.to_le_bytes());
        assert_eq!(
            ConfigurationHeader::decode(&old_header),
            Err(ConfigurationError::Magic)
        );
    }

    #[test]
    fn resource_claim_batches_are_transactional() {
        let header = ConfigurationHeader {
            capability_digest: board_mks_esp32_foc_v1::PACKAGE.board.capability_digest,
            record_count: 1,
            realtime_record_count: 0,
            flags: ConfigurationFlags::default(),
        };
        let gpio = ResourceId::Gpio(32);
        let stage_0 = ResourceId::Device(board_mks_esp32_foc_v1::device::POWER_STAGE_0);
        let stage_1 = ResourceId::Device(board_mks_esp32_foc_v1::device::POWER_STAGE_1);

        let mut duplicate =
            ConfigurationValidator::<3>::new(&board_mks_esp32_foc_v1::PACKAGE, header).unwrap();
        duplicate.claim_resource(gpio).unwrap();
        assert_eq!(
            duplicate.claim_resources(&[stage_0, gpio]),
            Err(ConfigurationError::DuplicateResource(gpio))
        );
        assert_eq!(duplicate.claimed_len, 1);
        assert_eq!(duplicate.claimed[0], Some(gpio));
        duplicate.claim_resource(stage_0).unwrap();

        let mut capacity =
            ConfigurationValidator::<2>::new(&board_mks_esp32_foc_v1::PACKAGE, header).unwrap();
        capacity.claim_resource(gpio).unwrap();
        assert_eq!(
            capacity.claim_resources(&[stage_0, stage_1]),
            Err(ConfigurationError::BindingCapacity)
        );
        assert_eq!(capacity.claimed_len, 1);
        assert_eq!(capacity.claimed[0], Some(gpio));
        capacity.claim_resource(stage_0).unwrap();
    }

    #[test]
    fn mks_high_impedance_shutdown_requires_qualified_stage_evidence() {
        let shutdown = foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        );
        let records = mks_foc_records(shutdown);
        let realtime_record_count = u16::try_from(
            records
                .iter()
                .filter(|record| record.realtime_relevant())
                .count(),
        )
        .unwrap();
        let header = ConfigurationHeader {
            capability_digest: board_mks_esp32_foc_v1::PACKAGE.board.capability_digest,
            record_count: u16::try_from(records.len()).unwrap(),
            realtime_record_count,
            flags: ConfigurationFlags(
                ConfigurationFlags::MOTION | ConfigurationFlags::FIELD_ORIENTED_CONTROL,
            ),
        };
        let mut validator =
            ConfigurationValidator::<32>::new(&board_mks_esp32_foc_v1::PACKAGE, header).unwrap();
        for record in records {
            if matches!(record, ConfigurationRecord::FocShutdown(_)) {
                assert_eq!(
                    validator.push(record),
                    Err(ConfigurationError::ShutdownUnqualified)
                );
                return;
            }
            validator.push(record).unwrap();
        }
        panic!("fixture must contain an FOC shutdown contract");
    }

    #[test]
    fn qualified_high_impedance_shutdown_is_retained_exactly_on_core1() {
        let mut devices = Vec::from(board_mks_esp32_foc_v1::PACKAGE.devices);
        for device in &mut devices[..2] {
            device.support = SupportLevel::Qualified;
        }
        let package = qualified_mks_package(&devices);
        let shutdown = foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        );
        let records = mks_foc_records(shutdown);
        let flags = ConfigurationFlags(
            ConfigurationFlags::MOTION | ConfigurationFlags::FIELD_ORIENTED_CONTROL,
        );
        let (bytes, digest) = document(&package, &records, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        let (identity, profile) = validator.finish_with_profile().unwrap();
        assert_eq!(identity.summary.foc_axes, 1);
        assert_eq!(profile.foc_axis_count(), 1);
        assert_eq!(
            profile.foc_axis(0).unwrap().shutdown,
            match shutdown {
                ConfigurationRecord::FocShutdown(shutdown) => shutdown,
                _ => unreachable!(),
            }
        );
        assert_eq!(profile.foc_axes().count(), 1);
    }

    #[test]
    fn complete_foc_document_lowers_only_after_exact_stream_validation() {
        let mut devices = Vec::from(board_mks_esp32_foc_v1::PACKAGE.devices);
        for device in &mut devices[..2] {
            device.support = SupportLevel::Qualified;
        }
        let package = qualified_mks_package(&devices);
        let records = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ));
        let flags = ConfigurationFlags(
            ConfigurationFlags::MOTION | ConfigurationFlags::FIELD_ORIENTED_CONTROL,
        );
        let (bytes, digest) = document(&package, &records, flags);

        for split in 1..bytes.len() {
            let mut validator = ConfigurationStreamValidator::<32>::new(
                &package,
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap();
            validator.push(&bytes[..split]).unwrap();
            validator.push(&bytes[split..]).unwrap();
            assert_eq!(validator.finish().unwrap().digest, digest);
        }

        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        let (identity, profile) = validator.finish_with_profile().unwrap();
        let configuration = RealtimeConfiguration { identity, profile };
        assert_eq!(configuration.identity(), identity);
        assert_eq!(configuration.profile().foc_axis_count(), 1);

        let lowered = configuration.lower_foc_axis(0).unwrap();
        lowered.validate().unwrap();
        assert_eq!(lowered.instance, 0);
        assert_eq!(lowered.parameters.configuration_digest, digest);
        assert_eq!(lowered.parameters.pole_pairs, 7);
        assert_eq!(lowered.parameters.timing.pwm_hz, 20_000);
        assert_eq!(lowered.rotor.configuration_digest, digest);
        assert_eq!(lowered.rotor.counts_per_mechanical_turn, 4_096);
        assert_eq!(lowered.current.snapshot().configuration_digest, digest);
        assert_eq!(lowered.current.snapshot().phase_pair, TwoShuntPhasePair::Ab);
        assert_eq!(lowered.pwm_dead_time_cycles, 8);
        assert_eq!(lowered.adc_channel0.attenuation, FocAdcAttenuation::Db11);
        assert_eq!(lowered.adc_channel1.attenuation, FocAdcAttenuation::Db11);
        assert_eq!(lowered.pwm_hardware.peripheral_source_clock_hz, 160_000_000);
        assert_eq!(lowered.pwm_hardware.peripheral_prescaler, 1);
        assert_eq!(lowered.pwm_hardware.timer_prescaler, 0);
        assert_eq!(lowered.pwm_compare.configuration_digest(), digest);
        assert_eq!(lowered.pwm_compare.counter_clock_hz(), 80_000_000);
        assert_eq!(lowered.pwm_compare.timer_peak_ticks(), 2_000);
        let mut forged_lowering = lowered;
        forged_lowering.pwm_hardware.timer_peak_ticks = 1_999;
        assert_eq!(
            forged_lowering.validate(),
            Err(ConfigurationError::FocHardware)
        );
        assert_eq!(
            configuration
                .profile()
                .foc_axis(0)
                .unwrap()
                .encoder
                .resource,
            ResourceId::Device(board_mks_esp32_foc_v1::device::ENCODER_0)
        );
        assert_eq!(
            configuration.lower_foc_axis(1),
            Err(ConfigurationError::IncompleteAxis)
        );

        let mut mismatched = records.clone();
        let ConfigurationRecord::Scalar(carrier) = mismatched
            .iter_mut()
            .find(|record| {
                matches!(
                    record,
                    ConfigurationRecord::Scalar(ExactScalar {
                        fact: ScalarFact::PwmCarrierHertz,
                        ..
                    })
                )
            })
            .unwrap()
        else {
            unreachable!();
        };
        carrier.value = rational(10_000, 1);
        let (bytes, digest) = document(&package, &mismatched, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::FocRuntime));

        let mut incompatible_timer = records.clone();
        let ConfigurationRecord::FocPwmHardware(hardware) = incompatible_timer
            .iter_mut()
            .find(|record| matches!(record, ConfigurationRecord::FocPwmHardware(_)))
            .unwrap()
        else {
            unreachable!();
        };
        hardware.timer_peak_ticks = 1_999;
        let (bytes, digest) = document(&package, &incompatible_timer, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::FocHardware));

        let mut insufficient_pulse = records.clone();
        let ConfigurationRecord::FocPwmHardware(hardware) = insufficient_pulse
            .iter_mut()
            .find(|record| matches!(record, ConfigurationRecord::FocPwmHardware(_)))
            .unwrap()
        else {
            unreachable!();
        };
        hardware.minimum_active_ticks = 7;
        let (bytes, digest) = document(&package, &insufficient_pulse, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::FocHardware));

        let mut missing_frontend = records.clone();
        missing_frontend.retain(|record| {
            !matches!(
                record,
                ConfigurationRecord::FocAdcFrontend(FocAdcFrontendParameters {
                    channel: FocCurrentChannel::Channel1,
                    ..
                })
            )
        });
        let (bytes, digest) = document(&package, &missing_frontend, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::IncompleteAxis));

        let mut incomplete = records;
        incomplete.retain(|record| !matches!(record, ConfigurationRecord::FocRotor(_)));
        let (bytes, digest) = document(&package, &incomplete, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::IncompleteAxis));

        let mut wrong_topology = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ));
        let ConfigurationRecord::Binding(current_b) = wrong_topology
            .iter_mut()
            .find(|record| {
                matches!(
                    record,
                    ConfigurationRecord::Binding(ResourceBinding {
                        role: BindingRole::FocCurrentB,
                        ..
                    })
                )
            })
            .unwrap()
        else {
            unreachable!();
        };
        current_b.resource = ResourceId::Adc {
            unit: 1,
            channel: 7,
        };
        let (bytes, digest) = document(&package, &wrong_topology, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::FocCurrent));

        let mut undersampled = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::PhaseHighImpedance,
            None,
            SignalPolarity::NotApplicable,
        ));
        let ConfigurationRecord::Binding(current_a) = undersampled
            .iter_mut()
            .find(|record| {
                matches!(
                    record,
                    ConfigurationRecord::Binding(ResourceBinding {
                        role: BindingRole::FocCurrentA,
                        ..
                    })
                )
            })
            .unwrap()
        else {
            unreachable!();
        };
        current_a.maximum_frequency_hz = 19_999;
        let (bytes, digest) = document(&package, &undersampled, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish(), Err(ConfigurationError::Frequency));
    }

    #[test]
    fn dedicated_shutdown_polarity_must_match_the_board_safe_level() {
        let control = ResourceId::Gpio(21);
        let mut resources = Vec::from(board_mks_esp32_foc_v1::PACKAGE.board.resources);
        resources.push(ResourceDescriptor {
            id: control,
            owner: OwnerDomain::Realtime,
            safe_value: SafeValue::Low,
            hazardous_output: true,
        });
        let mut stage_auxiliary =
            Vec::from(board_mks_esp32_foc_v1::PACKAGE.devices[0].auxiliary_resources);
        stage_auxiliary.push(control);
        let mut devices = Vec::from(board_mks_esp32_foc_v1::PACKAGE.devices);
        devices[0] = alumina_board::DeviceDescriptor {
            auxiliary_resources: &stage_auxiliary,
            support: SupportLevel::Qualified,
            ..devices[0]
        };
        devices[1].support = SupportLevel::Qualified;
        let mut board = board_mks_esp32_foc_v1::PACKAGE.board;
        board.resources = &resources;
        let package = reidentified_package(BoardPackage {
            board,
            devices: &devices,
            ..board_mks_esp32_foc_v1::PACKAGE
        });
        let records = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::DedicatedEnable,
            Some(control),
            SignalPolarity::ActiveHigh,
        ));
        let flags = ConfigurationFlags(
            ConfigurationFlags::MOTION | ConfigurationFlags::FIELD_ORIENTED_CONTROL,
        );
        let (bytes, digest) = document(&package, &records, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(
            validator
                .finish_with_profile()
                .unwrap()
                .1
                .foc_axis(0)
                .unwrap()
                .shutdown
                .strategy,
            FocShutdownStrategy::DedicatedEnable
        );

        resources.last_mut().unwrap().safe_value = SafeValue::High;
        let mut board = board_mks_esp32_foc_v1::PACKAGE.board;
        board.resources = &resources;
        let unsafe_package = reidentified_package(BoardPackage {
            board,
            devices: &devices,
            ..board_mks_esp32_foc_v1::PACKAGE
        });
        let (bytes, digest) = document(&unsafe_package, &records, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &unsafe_package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(
            validator.finish(),
            Err(ConfigurationError::ShutdownContract)
        );

        let disable_records = mks_foc_records(foc_shutdown(
            FocShutdownStrategy::DedicatedDisable,
            Some(control),
            SignalPolarity::ActiveHigh,
        ));
        let (bytes, digest) = document(&unsafe_package, &disable_records, flags);
        let mut validator = ConfigurationStreamValidator::<32>::new(
            &unsafe_package,
            digest,
            u32::try_from(bytes.len()).unwrap(),
        )
        .unwrap();
        validator.push(&bytes).unwrap();
        assert_eq!(validator.finish().unwrap().summary.foc_axes, 1);
    }

    #[test]
    fn tinybee_motion_document_validates_at_every_chunk_split() {
        assert!(
            core::mem::size_of::<RealtimeConfigurationProfile>() <= 4_096,
            "executable profiles are retained transactionally on core 1"
        );
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
            let (identity, realtime_profile) = validator.finish_with_profile().unwrap();
            assert_eq!(identity.digest, digest);
            assert_eq!(identity.summary.stepper_axes, 1);
            assert_eq!(identity.summary.foc_axes, 0);
            assert!(identity.summary.safety_binding);
            let axis = realtime_profile.stepper_axis(0).unwrap();
            assert_eq!(axis.instance, 0);
            assert_eq!(axis.step.resource, ResourceId::I2sOut { engine: 0, bit: 1 });
            assert_eq!(axis.step.minimum_active_cycles, 48);
            assert_eq!(axis.step.minimum_inactive_cycles, 48);
            assert_eq!(axis.direction.minimum_active_cycles, 48);
            assert_eq!(axis.direction.minimum_inactive_cycles, 48);
            assert_eq!(axis.driver_control.minimum_active_cycles, 48);
            assert_eq!(axis.driver_control.minimum_inactive_cycles, 48);
            assert_eq!(axis.driver_control_action, AxisDriverControl::Disable);
            assert!(realtime_profile.stepper_axis(1).is_none());
            assert_eq!(realtime_profile.safety_input_count(), 1);
            assert_eq!(
                realtime_profile.safety_input(0),
                Some(SafetyInputSpec {
                    instance: 0,
                    role: SafetyInputRole::EmergencyStop,
                    resource: ResourceId::Gpio(33),
                    polarity: InputPolarity::ActiveLow,
                    bias: InputBias::Floating,
                    required_for_arm: true,
                    minimum_active_cycles: 0,
                    minimum_inactive_cycles: 0,
                    maximum_sample_gap_cycles: 240_000,
                })
            );
            assert!(realtime_profile.safety_input(1).is_none());
            let monitor =
                SafetyInputMonitor::<4>::from_specs(realtime_profile.safety_inputs()).unwrap();
            assert_eq!(monitor.len(), 1);
            assert_eq!(monitor.spec(0), realtime_profile.safety_input(0));
        }
    }

    #[test]
    fn realtime_safety_input_requires_explicit_arm_gate_and_sample_watchdog() {
        let ConfigurationRecord::Binding(mut emergency) = binding(
            0,
            BindingRole::EmergencyStop,
            ResourceId::Gpio(33),
            SignalPolarity::ActiveLow,
            false,
        ) else {
            unreachable!()
        };
        emergency.flags = BindingFlags::default();
        assert_eq!(
            ConfigurationRecord::Binding(emergency).encode(),
            Err(ConfigurationError::Binding)
        );

        emergency.flags = BindingFlags(BindingFlags::REQUIRED_INTERLOCK);
        emergency.watchdog_cycles = 0;
        assert_eq!(
            ConfigurationRecord::Binding(emergency).encode(),
            Err(ConfigurationError::Timing)
        );

        let ConfigurationRecord::Binding(mut ordinary) = binding(
            0,
            BindingRole::DigitalInput,
            ResourceId::Gpio(32),
            SignalPolarity::ActiveLow,
            false,
        ) else {
            unreachable!()
        };
        ordinary.flags = BindingFlags(BindingFlags::REQUIRED_INTERLOCK);
        assert_eq!(
            ConfigurationRecord::Binding(ordinary).encode(),
            Err(ConfigurationError::Binding)
        );
    }

    #[test]
    fn executable_stepper_profile_requires_direction_and_enable_timing() {
        let mut records = tinybee_motion_records();
        for record in &mut records {
            if let ConfigurationRecord::Binding(binding) = record
                && binding.role == BindingRole::AxisDirection
            {
                binding.minimum_inactive_cycles = 0;
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
        assert_eq!(validator.finish(), Err(ConfigurationError::Timing));
    }

    #[test]
    fn executable_stepper_profile_rejects_axis_outside_machine_ir_width() {
        let mut records = tinybee_motion_records();
        for record in &mut records {
            match record {
                ConfigurationRecord::Binding(binding) if binding.role.axis_role() => {
                    binding.instance = u16::try_from(MAX_EXECUTABLE_STEPPER_AXES).unwrap();
                }
                ConfigurationRecord::Scalar(scalar) if scalar.fact.axis_fact() => {
                    scalar.instance = u16::try_from(MAX_EXECUTABLE_STEPPER_AXES).unwrap();
                }
                ConfigurationRecord::Binding(_)
                | ConfigurationRecord::Scalar(_)
                | ConfigurationRecord::FocShutdown(_)
                | ConfigurationRecord::FocRuntime(_)
                | ConfigurationRecord::FocController(_)
                | ConfigurationRecord::FocRotor(_)
                | ConfigurationRecord::FocCurrentChannel(_)
                | ConfigurationRecord::FocPwmAdcTiming(_)
                | ConfigurationRecord::FocAdcFrontend(_)
                | ConfigurationRecord::FocPwmHardware(_) => {}
            }
        }
        records.sort_by_key(|record| record.canonical_order_key());
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
        assert_eq!(validator.finish(), Err(ConfigurationError::AxisCount));
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
        assert!(CoreConfigurationCommand::authorize(7, digest, total_bytes).is_ok());
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
            active_authorized: false,
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
    fn coordinator_status_is_fixed_canonical_and_requires_durable_authorization() {
        let empty_realtime =
            RealtimeConfigurationService::<32>::new(&board_mks_tinybee::PACKAGE).report();
        let empty = ConfigurationCoordinatorStatus {
            phase: ConfigurationCoordinatorPhase::Empty,
            flags: ConfigurationCoordinatorFlags(0),
            fault: ConfigurationCoordinatorFault::None,
            operation_transaction_id: 0,
            operation_digest: Digest::ZERO,
            operation_bytes: 0,
            validated_bytes: 0,
            storage_chunks_read: 0,
            active_transaction_id: 0,
            active_digest: Digest::ZERO,
            active_bytes: 0,
            summary: None,
            realtime: empty_realtime,
        };
        let encoded = empty.encode().unwrap();
        assert_eq!(encoded.len(), CONFIGURATION_COORDINATOR_STATUS_BYTES);
        assert_eq!(ConfigurationCoordinatorStatus::decode(&encoded), Ok(empty));

        let total =
            u32::try_from(CONFIGURATION_HEADER_BYTES + 14 * CONFIGURATION_RECORD_BYTES).unwrap();
        let digest = Digest([0x44; 32]);
        let summary = ConfigurationSummary {
            record_count: 14,
            realtime_record_count: 14,
            binding_count: 4,
            stepper_axes: 1,
            foc_axes: 0,
            safety_binding: true,
            flags: ConfigurationFlags(ConfigurationFlags::MOTION),
        };
        let realtime = RealtimeConfigurationReport {
            state: RealtimeConfigurationState::Active,
            transaction_id: 77,
            digest,
            total_bytes: total,
            consumed_bytes: total,
            summary: Some(summary),
            fault: ConfigurationFaultCode::None,
            active_digest: digest,
            active_bytes: total,
            active_authorized: true,
        };
        let active = ConfigurationCoordinatorStatus {
            phase: ConfigurationCoordinatorPhase::Active,
            flags: ConfigurationCoordinatorFlags(
                ConfigurationCoordinatorFlags::CORE0_VALID
                    | ConfigurationCoordinatorFlags::JOBS_AUTHORIZED,
            ),
            fault: ConfigurationCoordinatorFault::None,
            operation_transaction_id: 0,
            operation_digest: Digest::ZERO,
            operation_bytes: 0,
            validated_bytes: 0,
            storage_chunks_read: 0,
            active_transaction_id: 77,
            active_digest: digest,
            active_bytes: total,
            summary: Some(summary),
            realtime,
        };
        let encoded = active.encode().unwrap();
        assert_eq!(ConfigurationCoordinatorStatus::decode(&encoded), Ok(active));

        let unauthorized = ConfigurationCoordinatorStatus {
            flags: ConfigurationCoordinatorFlags(ConfigurationCoordinatorFlags::CORE0_VALID),
            realtime: RealtimeConfigurationReport {
                active_authorized: false,
                ..realtime
            },
            ..active
        };
        assert_eq!(
            unauthorized.encode(),
            Err(ConfigurationCoordinatorStatusError::StateShape)
        );
        let mut reserved = encoded;
        reserved[118] = 1;
        assert_eq!(
            ConfigurationCoordinatorStatus::decode(&reserved),
            Err(ConfigurationCoordinatorStatusError::Reserved)
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
        assert!(service.authorized_identity().is_none());
        assert!(service.authorized_configuration().is_none());
        assert!(!report.active_authorized);
        let report = service.apply(
            CoreConfigurationCommand::authorize(41, digest, total).unwrap(),
            true,
        );
        assert_eq!(report.state, RealtimeConfigurationState::Active);
        assert!(report.active_authorized);
        assert_eq!(service.authorized_identity().unwrap().digest, digest);
        let executable = service.authorized_configuration().unwrap();
        assert_eq!(executable.identity.digest, digest);
        let axis = executable.profile.stepper_axis(0).unwrap();
        assert_eq!(axis.driver_control_action, AxisDriverControl::Disable);
        assert_eq!(axis.step.resource, ResourceId::I2sOut { engine: 0, bit: 1 });

        let rejected = service.apply(
            CoreConfigurationCommand::begin(42, Digest([0x77; 32]), total).unwrap(),
            false,
        );
        assert_eq!(rejected.state, RealtimeConfigurationState::Rejected);
        assert_eq!(rejected.fault, ConfigurationFaultCode::ForbiddenState);
        assert_eq!(rejected.active_digest, digest);
        assert_eq!(rejected.active_bytes, total);
        assert!(rejected.active_authorized);
        assert_eq!(service.active_identity().unwrap().digest, digest);
        assert!(service.authorized_configuration().is_some());

        let wrong_clear = service.apply(
            CoreConfigurationCommand::clear(43, Digest([0x55; 32]), total).unwrap(),
            true,
        );
        assert_eq!(wrong_clear.state, RealtimeConfigurationState::Rejected);
        assert_eq!(service.active_identity().unwrap().digest, digest);

        let cleared = service.apply(
            CoreConfigurationCommand::clear(43, digest, total).unwrap(),
            true,
        );
        assert_eq!(cleared.state, RealtimeConfigurationState::Cleared);
        assert!(service.active_identity().is_none());
        assert!(service.authorized_identity().is_none());
        assert!(service.authorized_configuration().is_none());
        assert_eq!(service.report().state, RealtimeConfigurationState::Cleared);
        let already_empty = service.apply(
            CoreConfigurationCommand::clear(44, digest, total).unwrap(),
            true,
        );
        assert_eq!(already_empty.state, RealtimeConfigurationState::Cleared);
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
        assert_eq!(
            service_status.storage_chunks_read,
            u32::try_from(bytes.len().div_ceil(173)).unwrap()
        );
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

    #[test]
    fn full_durable_activation_boot_recovery_authorization_and_clear_are_ordered() {
        let records = tinybee_motion_records();
        let (bytes, digest) = document(
            &board_mks_tinybee::PACKAGE,
            &records,
            ConfigurationFlags(ConfigurationFlags::MOTION),
        );
        let (mut cache, publication) = provisioned_configuration(&bytes, 173);
        let mut service = block_on(ServiceConfigurationValidation::<32>::open(
            &mut cache,
            &board_mks_tinybee::PACKAGE,
            publication,
        ))
        .unwrap();
        let mut realtime = RealtimeConfigurationService::<32>::new(&board_mks_tinybee::PACKAGE);
        while let Some(command) = block_on(service.next(&mut cache)).unwrap() {
            assert_ne!(
                realtime.apply(command, true).state,
                RealtimeConfigurationState::Rejected
            );
        }
        assert_eq!(
            realtime.report().state,
            RealtimeConfigurationState::CandidateValid
        );

        let durable =
            DurableConfigurationSelection::new(publication.transaction_id, publication.publication)
                .unwrap();
        let activation = ConfigurationTransition::activate(durable);
        let prepared = block_on(
            cache.prepare_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        assert_eq!(prepared.active, None);
        assert_eq!(prepared.pending, Some(activation));

        let activated = realtime.apply(
            CoreConfigurationCommand::activate(
                publication.transaction_id,
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap(),
            true,
        );
        assert_eq!(activated.state, RealtimeConfigurationState::Active);
        assert!(!activated.active_authorized);
        assert!(realtime.authorized_identity().is_none());

        let committed = block_on(
            cache.commit_configuration_transition(activation, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        assert_eq!(committed.active, Some(durable));
        assert!(realtime.authorized_identity().is_none());
        let authorized = realtime.apply(
            CoreConfigurationCommand::authorize(
                publication.transaction_id,
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap(),
            true,
        );
        assert!(authorized.active_authorized);
        assert_eq!(realtime.authorized_identity().unwrap().digest, digest);

        let device = cache.into_device();
        let mut rebooted = ProvisionedCache::new(device, TEST_CACHE_LIMITS);
        block_on(rebooted.discover()).unwrap();
        let replayed = rebooted.configuration_journal().unwrap();
        assert_eq!(replayed.active, Some(durable));
        assert_eq!(replayed.pending, None);
        let recovery_publication = ConfigurationPublication {
            transaction_id: durable.transaction_id(),
            publication: durable.publication(),
        };
        let mut recovery = block_on(ServiceConfigurationValidation::<32>::open(
            &mut rebooted,
            &board_mks_tinybee::PACKAGE,
            recovery_publication,
        ))
        .unwrap();
        let mut recovered_rt = RealtimeConfigurationService::<32>::new(&board_mks_tinybee::PACKAGE);
        while let Some(command) = block_on(recovery.next(&mut rebooted)).unwrap() {
            recovered_rt.apply(command, true);
        }
        recovered_rt.apply(
            CoreConfigurationCommand::activate(
                durable.transaction_id(),
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap(),
            true,
        );
        assert!(recovered_rt.authorized_identity().is_none());
        recovered_rt.apply(
            CoreConfigurationCommand::authorize(
                durable.transaction_id(),
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap(),
            true,
        );
        assert_eq!(recovered_rt.authorized_identity().unwrap().digest, digest);

        let clear_selection =
            DurableConfigurationSelection::new(0x7799, durable.publication()).unwrap();
        let clear = ConfigurationTransition::clear(clear_selection);
        block_on(rebooted.prepare_configuration_transition(clear, MutationContext::DISARMED_IDLE))
            .unwrap();
        let cleared = recovered_rt.apply(
            CoreConfigurationCommand::clear(
                clear_selection.transaction_id(),
                digest,
                u32::try_from(bytes.len()).unwrap(),
            )
            .unwrap(),
            true,
        );
        assert_eq!(cleared.state, RealtimeConfigurationState::Cleared);
        assert_eq!(recovered_rt.report(), cleared);
        let journal = block_on(
            rebooted.commit_configuration_transition(clear, MutationContext::DISARMED_IDLE),
        )
        .unwrap();
        assert_eq!(journal.active, None);
        assert_eq!(journal.pending, None);
    }
}
