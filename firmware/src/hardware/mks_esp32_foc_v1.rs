//! Non-armable MKS ESP32 FOC V1.0 ownership and safe-output composition.
//!
//! The V1.0 schematic establishes no independent inverter enable and no fitted
//! SD medium. Each phase signal is wired to an EG2133 active-high HIN and
//! active-low LIN-bar pair, so a driven high or low selects a MOSFET. This
//! target therefore makes all six phase pins no-pull inputs before the realtime
//! executor starts, exposes a permanently unavailable cache backend, and
//! rejects every motion/FOC output operation.

use alumina_as5600::{As5600, MagnetStatus, Observation, RawAngle};
use alumina_board::{BoardPackage, ResourceId};
use alumina_config::{
    ConfigurationError, FocAdcAttenuation, RealtimeConfiguration, RealtimeConfigurationProfile,
};
use alumina_foc::{
    PwmCompareContract, PwmCompareError, SequentialAdcAcquisition, SequentialAdcAcquisitionError,
    SequentialAdcChannel, SequentialAdcPair, SequentialAdcRequest, TwoShuntPhasePair,
};
use alumina_motion::{
    OutputCommitToken, ScheduledShiftOutput, ShiftImageContract, ShiftImageUpdate,
};
use alumina_protocol::DeviceCycle;
use alumina_safety::{SafetyContractId, SafetyInputMonitor};
use alumina_service::CACHE_LIMITS;
use alumina_storage::media::{AsyncBlockDevice, MediaBlock};
use alumina_storage::provisioning::ProvisionedCache;
use defmt::warn;
use esp_hal::analog::adc::{Adc, AdcConfig, AdcPin, Attenuation};
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::i2c::master::{Config as I2cConfig, Error as I2cError, I2c};
use esp_hal::mcpwm::timer::{CounterDirection, PwmWorkingMode, TimerClockConfig};
use esp_hal::mcpwm::{McPwm, PeripheralClockConfig, PwmPeripheral};
use esp_hal::peripherals::{
    ADC1, GPIO0, GPIO1, GPIO2, GPIO3, GPIO5, GPIO13, GPIO14, GPIO15, GPIO18, GPIO19, GPIO23,
    GPIO25, GPIO26, GPIO27, GPIO32, GPIO33, GPIO34, GPIO35, GPIO36, GPIO39, I2C0, I2C1, MCPWM0,
    MCPWM1, Peripherals, TIMG1, UART0, WIFI,
};
use esp_hal::time::Rate;
use esp_hal::{Async, Blocking};

use super::RuntimeResources;
use super::safety_inputs::{SafetyInputBackendError, SafetyInputBank, SafetyInputScan};

/// A board-local cache transport that can never claim nonexistent media.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnavailableStorage;

/// Deterministic rejection returned by the non-fitted storage transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableStorageError {
    /// The V1.0 board has no established mutable job-cache medium.
    NotFitted,
}

impl AsyncBlockDevice for UnavailableStorage {
    type Error = UnavailableStorageError;

    fn block_count(&self) -> u64 {
        0
    }

    async fn read_block(
        &mut self,
        _block: u64,
        _output: &mut MediaBlock,
    ) -> Result<(), Self::Error> {
        Err(UnavailableStorageError::NotFitted)
    }

    async fn write_block(&mut self, _block: u64, _data: &MediaBlock) -> Result<(), Self::Error> {
        Err(UnavailableStorageError::NotFitted)
    }

    async fn sync(&mut self) -> Result<(), Self::Error> {
        Err(UnavailableStorageError::NotFitted)
    }
}

pub type StorageBackend = ProvisionedCache<UnavailableStorage>;
/// Future dual-servo stream width; the current target rejects all execution.
pub const JOB_AXES: usize = 2;
/// Maximum unique resource claims retained by each configuration validator.
pub const CONFIGURATION_BINDINGS: usize = 64;
/// This board currently exposes no configuration-derived safety-input route.
pub const SAFETY_INPUT_CAPACITY: usize = 0;
/// No MCPWM/ADC FOC backend is implemented in this safe-only composition.
pub const MOTION_OUTPUT_IMPLEMENTED: bool = false;
/// No physical PWM/current/sensor timing has been qualified.
pub const MOTION_OUTPUT_QUALIFIED: bool = false;
/// Inert structural value; no output lattice is admitted.
pub const MOTION_OUTPUT_QUANTUM_CYCLES: u32 = 1;
/// No physical commit-lateness claim exists.
pub const MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES: u32 = 0;
/// No prestart lead can authorize the nonexistent output backend.
pub const MOTION_MINIMUM_PRIME_LEAD_CYCLES: u64 = u64::MAX;
/// Portable structural capacity; no hardware owner consumes it.
pub const MOTION_OUTPUT_RING_IMAGES: usize = 64;
/// Structural placeholder only; no FOC horizon is reachable.
pub const MOTION_PRIME_HORIZON_CYCLES: u64 = 20_000;
/// Conservative board-composition rate for each independent encoder bus.
pub const ENCODER_I2C_HZ: u32 = 400_000;
/// Classic ESP32 ADC1 is used at its HAL-default 12-bit resolution.
pub const ADC1_MAXIMUM_COUNT: u16 = 4_095;

/// This direct-PWM board has no shifted step/motion image contract.
pub const fn motion_shift_contract() -> Option<ShiftImageContract> {
    None
}

/// Semantic identity of the safe-only six-phase-high-impedance transaction.
///
/// Bytes are `MFSC`, version 2, six phase inputs high impedance, no independent
/// gate disable, no admitted safety inputs, and an unqualified reset state.
pub const SAFE_OUTPUT_CONTRACT: SafetyContractId =
    SafetyContractId([b'M', b'F', b'S', b'C', 2, 6, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/// Core-0 tokens. No storage token exists because no medium is fitted.
#[allow(
    dead_code,
    reason = "UART/boot tokens are reserved for staged service diagnostics"
)]
pub struct ServiceResources {
    wifi: Option<WIFI<'static>>,
    uart0: UART0<'static>,
    uart_tx: GPIO1<'static>,
    uart_rx: GPIO3<'static>,
    boot: GPIO0<'static>,
    usb_boot_strap_io2: GPIO2<'static>,
}

impl ServiceResources {
    /// Moves the singleton radio token into core-0 network initialization once.
    pub fn take_wifi(&mut self) -> WIFI<'static> {
        self.wifi.take().expect("Wi-Fi token already consumed")
    }

    /// Returns a permanently faulted cache without probing or mutating hardware.
    pub async fn initialize_storage(&mut self) -> StorageBackend {
        warn!("MKS ESP32 FOC V1.0 has no fitted cache medium");
        ProvisionedCache::transport_faulted(UnavailableStorage, CACHE_LIMITS)
    }
}

/// Core-1 tokens before all six gate-driver inputs are made high impedance.
#[allow(
    dead_code,
    reason = "tokens are retained for staged MCPWM/ADC/encoder work"
)]
pub struct RealtimeResources {
    timer_group1: TIMG1<'static>,
    mcpwm0: MCPWM0<'static>,
    mcpwm1: MCPWM1<'static>,
    adc1: ADC1<'static>,
    i2c0: I2C0<'static>,
    i2c1: I2C1<'static>,
    phase_u0: GPIO32<'static>,
    phase_v0: GPIO33<'static>,
    phase_w0: GPIO25<'static>,
    phase_u1: GPIO26<'static>,
    phase_v1: GPIO27<'static>,
    phase_w1: GPIO14<'static>,
    current_a0: GPIO39<'static>,
    current_b0: GPIO36<'static>,
    current_a1: GPIO35<'static>,
    current_b1: GPIO34<'static>,
    encoder_scl0: GPIO18<'static>,
    encoder_sda0: GPIO19<'static>,
    encoder_index0: GPIO15<'static>,
    encoder_scl1: GPIO5<'static>,
    encoder_sda1: GPIO23<'static>,
    encoder_index1: GPIO13<'static>,
}

/// One MCPWM token sealed together with phase pins that remain GPIO inputs.
///
/// There is intentionally no token extractor or [`alumina_foc::PowerStage`]
/// implementation. Constructing an ESP HAL MCPWM output requires a later,
/// reviewed transition that consumes this closed state.
#[allow(
    dead_code,
    reason = "closed state intentionally has no energizing operation"
)]
pub struct ClosedPowerStage<Pwm> {
    controller: Pwm,
    phase_u: Input<'static>,
    phase_v: Input<'static>,
    phase_w: Input<'static>,
}

/// Exact portable contract plus the two HAL prescalers used by one MCPWM unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClosedMcpwmConfiguration {
    contract: PwmCompareContract,
    peripheral_source_clock_hz: u32,
    peripheral_prescaler: u8,
    timer_prescaler: u8,
}

#[allow(
    dead_code,
    reason = "stored configuration accessors await qualified target activation"
)]
impl ClosedMcpwmConfiguration {
    /// Exact digest-bound portable timer/compare contract.
    pub const fn contract(self) -> PwmCompareContract {
        self.contract
    }

    /// Exact pre-divider MCPWM clock required by the stored configuration.
    pub const fn peripheral_source_clock_hz(self) -> u32 {
        self.peripheral_source_clock_hz
    }

    /// Raw zero-based MCPWM peripheral prescaler retained by configuration.
    pub const fn peripheral_prescaler(self) -> u8 {
        self.peripheral_prescaler
    }

    /// Raw zero-based MCPWM timer prescaler retained by configuration.
    pub const fn timer_prescaler(self) -> u8 {
        self.timer_prescaler
    }
}

/// Rejection before a canonical FOC profile can select fixed MKS hardware.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "selection errors remain unreachable while the compiled stage is unqualified"
)]
pub enum StoredFocHardwareSelectionError {
    /// The complete configuration or digest-bound FOC lowering rejected.
    Configuration(ConfigurationError),
    /// The configuration was validated for a different compiled board package.
    Capability,
    /// The selected stage, phase outputs, ADC routes, or phase pair is not one fixed motor.
    Topology,
    /// A current calibration does not describe the fixed 12-bit ADC1 owner.
    AdcRange,
}

impl From<ConfigurationError> for StoredFocHardwareSelectionError {
    fn from(error: ConfigurationError) -> Self {
        Self::Configuration(error)
    }
}

/// One target-checked ADC/MCPWM selection derived from canonical stored bytes.
///
/// Construction verifies the compiled capability digest and fixed schematic
/// routing. It still exposes only the unqualified ADC commissioning selection
/// and the stopped, disconnected MCPWM state; it grants no output transition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    dead_code,
    reason = "stored target selection remains unreachable while the compiled stage is unqualified"
)]
pub struct StoredFocAxisHardwareSelection {
    axis: CurrentAxis,
    adc: Adc1MotorAttenuation,
    mcpwm: ClosedMcpwmConfiguration,
}

#[allow(
    dead_code,
    reason = "stored target selection remains unreachable while the compiled stage is unqualified"
)]
impl StoredFocAxisHardwareSelection {
    /// Derives one fixed motor selection from an independently validated profile.
    pub fn from_configuration(
        configuration: &RealtimeConfiguration,
        slot: usize,
    ) -> Result<Self, StoredFocHardwareSelectionError> {
        if configuration.identity().capability_digest
            != board_mks_esp32_foc_v1::PACKAGE.board.capability_digest
        {
            return Err(StoredFocHardwareSelectionError::Capability);
        }
        let profile = configuration
            .profile()
            .foc_axis(slot)
            .ok_or(ConfigurationError::IncompleteAxis)?;
        let lowered = configuration.lower_foc_axis(slot)?;
        if lowered.instance != profile.instance
            || lowered.current.snapshot().phase_pair != TwoShuntPhasePair::Ab
        {
            return Err(StoredFocHardwareSelectionError::Topology);
        }

        let (axis, phase_resources, current_resources) = match profile.shutdown.power_stage {
            ResourceId::Device(board_mks_esp32_foc_v1::device::POWER_STAGE_0) => (
                CurrentAxis::Axis0,
                [
                    ResourceId::TimedOutput {
                        engine: 0,
                        channel: 0,
                    },
                    ResourceId::TimedOutput {
                        engine: 0,
                        channel: 1,
                    },
                    ResourceId::TimedOutput {
                        engine: 0,
                        channel: 2,
                    },
                ],
                [
                    ResourceId::Adc {
                        unit: 1,
                        channel: 3,
                    },
                    ResourceId::Adc {
                        unit: 1,
                        channel: 0,
                    },
                ],
            ),
            ResourceId::Device(board_mks_esp32_foc_v1::device::POWER_STAGE_1) => (
                CurrentAxis::Axis1,
                [
                    ResourceId::TimedOutput {
                        engine: 1,
                        channel: 0,
                    },
                    ResourceId::TimedOutput {
                        engine: 1,
                        channel: 1,
                    },
                    ResourceId::TimedOutput {
                        engine: 1,
                        channel: 2,
                    },
                ],
                [
                    ResourceId::Adc {
                        unit: 1,
                        channel: 7,
                    },
                    ResourceId::Adc {
                        unit: 1,
                        channel: 6,
                    },
                ],
            ),
            _ => return Err(StoredFocHardwareSelectionError::Topology),
        };
        if [
            profile.phase_u.resource,
            profile.phase_v.resource,
            profile.phase_w.resource,
        ] != phase_resources
            || [
                profile.current_channel0_binding.resource,
                profile.current_channel1_binding.resource,
            ] != current_resources
        {
            return Err(StoredFocHardwareSelectionError::Topology);
        }
        let current = lowered.current.snapshot();
        if current.channel0.adc_maximum_count != ADC1_MAXIMUM_COUNT
            || current.channel1.adc_maximum_count != ADC1_MAXIMUM_COUNT
        {
            return Err(StoredFocHardwareSelectionError::AdcRange);
        }

        Ok(Self {
            axis,
            adc: Adc1MotorAttenuation {
                current_a: lowered.adc_channel0.attenuation.into(),
                current_b: lowered.adc_channel1.attenuation.into(),
            },
            mcpwm: ClosedMcpwmConfiguration {
                contract: lowered.pwm_compare,
                peripheral_source_clock_hz: lowered.pwm_hardware.peripheral_source_clock_hz,
                peripheral_prescaler: lowered.pwm_hardware.peripheral_prescaler,
                timer_prescaler: lowered.pwm_hardware.timer_prescaler,
            },
        })
    }

    pub const fn axis(self) -> CurrentAxis {
        self.axis
    }

    pub const fn adc(self) -> Adc1MotorAttenuation {
        self.adc
    }

    pub const fn mcpwm(self) -> ClosedMcpwmConfiguration {
        self.mcpwm
    }
}

/// MCPWM ownership after timer 0 is configured, stopped, and reset to zero.
///
/// All three physical phase pins remain no-pull GPIO inputs. The controller is
/// private, there is no pin attachment or compare-write method, and this type
/// deliberately does not implement [`alumina_foc::PowerStage`].
#[allow(
    dead_code,
    reason = "closed MCPWM owner is retained until a qualified output transition exists"
)]
pub struct ClosedMcpwmStage<Pwm: 'static> {
    controller: McPwm<'static, Pwm>,
    phase_u: Input<'static>,
    phase_v: Input<'static>,
    phase_w: Input<'static>,
    configuration: ClosedMcpwmConfiguration,
}

impl<Pwm: 'static> ClosedMcpwmStage<Pwm> {
    /// Returns the exact validated clock/compare facts retained by this owner.
    pub const fn configuration(&self) -> ClosedMcpwmConfiguration {
        self.configuration
    }
}

/// Failure before either MCPWM singleton changes ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClosedMcpwmInitializationError {
    /// The portable clock/compare contract is invalid.
    Contract(PwmCompareError),
    /// The live HAL clock tree cannot represent the requested counter clock exactly.
    Clock,
}

/// ADC1 and all four typed input-only GPIOs before analog calibration exists.
///
/// Retaining the raw ADC token prevents another owner from sampling these
/// channels. GPIO34–39 have no digital output driver on classic ESP32, while
/// retaining their concrete types lets a later calibrated transition prove the
/// ADC1 channel mapping without stealing or reconstructing a token. No ESP HAL
/// ADC driver or [`alumina_foc::CurrentSense`] backend is constructible through
/// this state.
#[allow(
    dead_code,
    reason = "uncalibrated state intentionally has no conversion operation"
)]
pub struct UncalibratedCurrentSense {
    adc1: ADC1<'static>,
    current_a0: GPIO39<'static>,
    current_b0: GPIO36<'static>,
    current_a1: GPIO35<'static>,
    current_b1: GPIO34<'static>,
}

/// Explicit classic-ESP32 ADC attenuation selected for one current input.
///
/// These names intentionally retain the HAL's approximate hardware settings;
/// no voltage range or current accuracy follows from selecting one. A stored
/// calibration must later bind the setting to measured analog evidence.
#[allow(
    dead_code,
    reason = "all attenuation selections are retained for later stored configuration"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Adc1Attenuation {
    Db0,
    Db2p5,
    Db6,
    Db11,
}

impl Adc1Attenuation {
    const fn into_hal(self) -> Attenuation {
        match self {
            Self::Db0 => Attenuation::_0dB,
            Self::Db2p5 => Attenuation::_2p5dB,
            Self::Db6 => Attenuation::_6dB,
            Self::Db11 => Attenuation::_11dB,
        }
    }
}

impl From<FocAdcAttenuation> for Adc1Attenuation {
    fn from(attenuation: FocAdcAttenuation) -> Self {
        match attenuation {
            FocAdcAttenuation::Db0 => Self::Db0,
            FocAdcAttenuation::Db2p5 => Self::Db2p5,
            FocAdcAttenuation::Db6 => Self::Db6,
            FocAdcAttenuation::Db11 => Self::Db11,
        }
    }
}

/// Named attenuation settings for one stage's two schematic current routes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Adc1MotorAttenuation {
    pub current_a: Adc1Attenuation,
    pub current_b: Adc1Attenuation,
}

/// Per-route attenuation required before ADC1 takes ownership of the pins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Adc1AcquisitionConfiguration {
    pub motor0: Adc1MotorAttenuation,
    pub motor1: Adc1MotorAttenuation,
}

/// Physical motor selecting one pair of current-amplifier outputs.
#[allow(
    dead_code,
    reason = "both diagnostic current paths compile but are unscheduled before HIL"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentAxis {
    Axis0,
    Axis1,
}

impl CurrentAxis {
    const fn instance(self) -> u16 {
        match self {
            Self::Axis0 => 0,
            Self::Axis1 => 1,
        }
    }
}

type Adc1Driver = Adc<'static, ADC1<'static>, Blocking>;
type CurrentA0 = AdcPin<GPIO39<'static>, ADC1<'static>>;
type CurrentB0 = AdcPin<GPIO36<'static>, ADC1<'static>>;
type CurrentA1 = AdcPin<GPIO35<'static>, ADC1<'static>>;
type CurrentB1 = AdcPin<GPIO34<'static>, ADC1<'static>>;

/// Error from the explicitly unqualified ADC1 commissioning path.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Adc1AcquisitionError {
    /// The portable ordering/range contract rejected the operation.
    Sequence(SequentialAdcAcquisitionError),
    /// The HAL returned a terminal conversion error.
    Peripheral,
}

impl From<SequentialAdcAcquisitionError> for Adc1AcquisitionError {
    fn from(error: SequentialAdcAcquisitionError) -> Self {
        Self::Sequence(error)
    }
}

/// Sole ADC1 owner for raw, software-started commissioning conversions.
///
/// Classic ESP32's current `esp-hal` API exposes a software-started one-shot
/// path here, not an MCPWM-triggered aperture. This owner therefore returns
/// only [`SequentialAdcPair`], records conversion-completion observations, and
/// deliberately does not implement [`alumina_foc::CurrentSense`].
pub struct UnqualifiedAdc1CurrentSense {
    adc: Adc1Driver,
    current_a0: CurrentA0,
    current_b0: CurrentB0,
    current_a1: CurrentA1,
    current_b1: CurrentB1,
    acquisition: SequentialAdcAcquisition,
    configuration: Adc1AcquisitionConfiguration,
}

impl UnqualifiedAdc1CurrentSense {
    /// Returns the exact attenuation selections currently programmed in ADC1.
    pub const fn configuration(&self) -> Adc1AcquisitionConfiguration {
        self.configuration
    }

    /// Begins one ordered diagnostic pair without claiming a sampling instant.
    pub fn begin_pair(
        &mut self,
        axis: CurrentAxis,
        token: u32,
        requested_at: DeviceCycle,
    ) -> Result<(), Adc1AcquisitionError> {
        self.acquisition
            .begin(SequentialAdcRequest {
                axis: axis.instance(),
                token,
                requested_at,
            })
            .map_err(Into::into)
    }

    /// Polls the currently selected conversion once.
    ///
    /// `observed_at` is the cycle at which software observes a completed ADC
    /// conversion. It is not represented as a sample-and-hold timestamp.
    pub fn poll_pair(
        &mut self,
        observed_at: DeviceCycle,
    ) -> Result<Option<SequentialAdcPair>, Adc1AcquisitionError> {
        let request = self
            .acquisition
            .pending_request()
            .ok_or(SequentialAdcAcquisitionError::Idle)?;
        let channel = self
            .acquisition
            .pending_channel()
            .ok_or(SequentialAdcAcquisitionError::Idle)?;
        let conversion = match (request.axis, channel) {
            (0, SequentialAdcChannel::Channel0) => self.adc.read_oneshot(&mut self.current_a0),
            (0, SequentialAdcChannel::Channel1) => self.adc.read_oneshot(&mut self.current_b0),
            (1, SequentialAdcChannel::Channel0) => self.adc.read_oneshot(&mut self.current_a1),
            (1, SequentialAdcChannel::Channel1) => self.adc.read_oneshot(&mut self.current_b1),
            _ => {
                self.acquisition.abort();
                return Err(Adc1AcquisitionError::Sequence(
                    SequentialAdcAcquisitionError::ChannelOrder,
                ));
            }
        };
        match conversion {
            Ok(raw_count) => self
                .acquisition
                .record_conversion(channel, raw_count, observed_at)
                .map_err(Into::into),
            Err(nb::Error::WouldBlock) => Ok(None),
            Err(nb::Error::Other(())) => {
                self.acquisition.abort();
                Err(Adc1AcquisitionError::Peripheral)
            }
        }
    }

    /// Cancels a partial diagnostic pair without manufacturing a result.
    pub fn abort_pair(&mut self) -> Option<SequentialAdcRequest> {
        self.acquisition.abort()
    }
}

type EncoderTransport = I2c<'static, Async>;
type Encoder = As5600<EncoderTransport>;

/// Encoder connector ownership before a stored configuration selects a mode.
#[allow(
    dead_code,
    reason = "dormant connector state is consumed only after configuration selection"
)]
pub struct DormantEncoderResources {
    i2c0: I2C0<'static>,
    i2c1: I2C1<'static>,
    encoder_scl0: Input<'static>,
    encoder_sda0: Input<'static>,
    encoder_scl1: Input<'static>,
    encoder_sda1: Input<'static>,
    encoder_index0: Input<'static>,
    encoder_index1: Input<'static>,
}

/// Read-only dual-AS5600 ownership plus the two unassigned auxiliary inputs.
#[allow(
    dead_code,
    reason = "AS5600 mode compiles but is not selected before configuration/HIL"
)]
pub struct As5600EncoderResources {
    encoder0: Encoder,
    encoder1: Encoder,
    encoder_index0: Input<'static>,
    encoder_index1: Input<'static>,
}

/// Core-1 resources after the safe phase transition.
#[allow(
    dead_code,
    reason = "closed PWM and selected current ownership remain staged target work"
)]
pub struct EstablishedRealtimeResources<
    Encoders = DormantEncoderResources,
    Current = UncalibratedCurrentSense,
    Stage0 = ClosedPowerStage<MCPWM0<'static>>,
    Stage1 = ClosedPowerStage<MCPWM1<'static>>,
> {
    timer_group1: TIMG1<'static>,
    stage0: Stage0,
    stage1: Stage1,
    current_sense: Current,
    encoders: Encoders,
    safety_inputs: SafetyInputBank<0>,
}

/// Physical motor axis selecting one independent AS5600 bus.
#[allow(
    dead_code,
    reason = "read-only sensor path is compiled but not scheduled before HIL"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EncoderAxis {
    Axis0,
    Axis1,
}

/// Read-only AS5600 transaction failure on an ESP HAL I2C transport.
#[allow(
    dead_code,
    reason = "read-only sensor path is compiled but not scheduled before HIL"
)]
pub type EncoderError = alumina_as5600::Error<I2cError>;

/// Safe-only target rejection before any FOC adapter is qualified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeOutputError {
    /// Fixed board safety-input routes were internally inconsistent.
    SafetyInput(SafetyInputBackendError),
    /// MCPWM/ADC/current/sensor execution is intentionally unavailable.
    MotionUnsupported,
}

impl RealtimeResources {
    /// Makes all phase and digital observation pins high impedance.
    ///
    /// The four current channels retain their concrete hardware-input-only
    /// GPIO types so a later ADC transition cannot lose channel identity.
    /// No I2C, ADC, or MCPWM peripheral is initialized. Selecting an encoder
    /// connector mode is a separate synchronous type-state transition after a
    /// stored configuration has been accepted.
    pub fn establish_safe_outputs(self) -> Result<EstablishedRealtimeResources, SafeOutputError> {
        let input = InputConfig::default().with_pull(Pull::None);
        let phase_u0 = Input::new(self.phase_u0, input);
        let phase_v0 = Input::new(self.phase_v0, input);
        let phase_w0 = Input::new(self.phase_w0, input);
        let phase_u1 = Input::new(self.phase_u1, input);
        let phase_v1 = Input::new(self.phase_v1, input);
        let phase_w1 = Input::new(self.phase_w1, input);

        let encoder_scl0 = Input::new(self.encoder_scl0, input);
        let encoder_sda0 = Input::new(self.encoder_sda0, input);
        let encoder_index0 = Input::new(self.encoder_index0, input);
        let encoder_scl1 = Input::new(self.encoder_scl1, input);
        let encoder_sda1 = Input::new(self.encoder_sda1, input);
        let encoder_index1 = Input::new(self.encoder_index1, input);

        let safety_inputs = SafetyInputBank::<0>::new([]).map_err(SafeOutputError::SafetyInput)?;

        Ok(EstablishedRealtimeResources {
            timer_group1: self.timer_group1,
            stage0: ClosedPowerStage {
                controller: self.mcpwm0,
                phase_u: phase_u0,
                phase_v: phase_v0,
                phase_w: phase_w0,
            },
            stage1: ClosedPowerStage {
                controller: self.mcpwm1,
                phase_u: phase_u1,
                phase_v: phase_v1,
                phase_w: phase_w1,
            },
            current_sense: UncalibratedCurrentSense {
                adc1: self.adc1,
                current_a0: self.current_a0,
                current_b0: self.current_b0,
                current_a1: self.current_a1,
                current_b1: self.current_b1,
            },
            encoders: DormantEncoderResources {
                i2c0: self.i2c0,
                i2c1: self.i2c1,
                encoder_scl0,
                encoder_sda0,
                encoder_scl1,
                encoder_sda1,
                encoder_index0,
                encoder_index1,
            },
            safety_inputs,
        })
    }
}

impl<Current, Stage0, Stage1>
    EstablishedRealtimeResources<DormantEncoderResources, Current, Stage0, Stage1>
{
    /// Selects read-only AS5600 mode for both independent encoder connectors.
    ///
    /// The phase, ADC, timer, and safety owners are moved unchanged. Connecting
    /// SDA/SCL configures released open-drain lines at the board-package limit;
    /// construction performs no I2C transaction and writes no sensor register.
    #[allow(
        dead_code,
        reason = "AS5600 mode compiles but awaits stored selection and HIL"
    )]
    pub fn activate_as5600_encoders(
        self,
    ) -> EstablishedRealtimeResources<As5600EncoderResources, Current, Stage0, Stage1> {
        let EstablishedRealtimeResources {
            timer_group1,
            stage0,
            stage1,
            current_sense,
            encoders,
            safety_inputs,
        } = self;
        let DormantEncoderResources {
            i2c0,
            i2c1,
            encoder_scl0,
            encoder_sda0,
            encoder_scl1,
            encoder_sda1,
            encoder_index0,
            encoder_index1,
        } = encoders;
        let i2c_config = I2cConfig::default().with_frequency(Rate::from_hz(ENCODER_I2C_HZ));
        let encoder0 = I2c::new(i2c0, i2c_config)
            .unwrap_or_else(|_| panic!("invalid fixed MKS encoder-0 I2C configuration"))
            .with_sda(encoder_sda0.into_flex())
            .with_scl(encoder_scl0.into_flex())
            .into_async();
        let encoder1 = I2c::new(i2c1, i2c_config)
            .unwrap_or_else(|_| panic!("invalid fixed MKS encoder-1 I2C configuration"))
            .with_sda(encoder_sda1.into_flex())
            .with_scl(encoder_scl1.into_flex())
            .into_async();

        EstablishedRealtimeResources {
            timer_group1,
            stage0,
            stage1,
            current_sense,
            encoders: As5600EncoderResources {
                encoder0: As5600::new(encoder0),
                encoder1: As5600::new(encoder1),
                encoder_index0,
                encoder_index1,
            },
            safety_inputs,
        }
    }
}

impl<Encoders, Stage0, Stage1>
    EstablishedRealtimeResources<Encoders, UncalibratedCurrentSense, Stage0, Stage1>
{
    /// Gives ADC1 sole ownership of all four current-amplifier routes.
    ///
    /// Construction selects the caller-supplied approximate attenuation for
    /// every physical pin and the HAL-default 12-bit resolution. It neither
    /// starts a conversion nor binds the choices to stored current calibration.
    /// The resulting software-started path remains diagnostic-only and cannot
    /// satisfy [`alumina_foc::CurrentSense`].
    #[allow(
        dead_code,
        reason = "diagnostic ADC1 ownership compiles but awaits stored selection and HIL"
    )]
    pub fn activate_unqualified_adc1(
        self,
        configuration: Adc1AcquisitionConfiguration,
    ) -> EstablishedRealtimeResources<Encoders, UnqualifiedAdc1CurrentSense, Stage0, Stage1> {
        let EstablishedRealtimeResources {
            timer_group1,
            stage0,
            stage1,
            current_sense,
            encoders,
            safety_inputs,
        } = self;
        let UncalibratedCurrentSense {
            adc1,
            current_a0,
            current_b0,
            current_a1,
            current_b1,
        } = current_sense;
        let mut adc_configuration = AdcConfig::<ADC1<'static>>::new();
        let current_a0 =
            adc_configuration.enable_pin(current_a0, configuration.motor0.current_a.into_hal());
        let current_b0 =
            adc_configuration.enable_pin(current_b0, configuration.motor0.current_b.into_hal());
        let current_a1 =
            adc_configuration.enable_pin(current_a1, configuration.motor1.current_a.into_hal());
        let current_b1 =
            adc_configuration.enable_pin(current_b1, configuration.motor1.current_b.into_hal());
        let adc = Adc::new(adc1, adc_configuration);
        let acquisition = SequentialAdcAcquisition::new(ADC1_MAXIMUM_COUNT)
            .unwrap_or_else(|_| panic!("nonzero fixed classic-ESP32 ADC1 range rejected"));

        EstablishedRealtimeResources {
            timer_group1,
            stage0,
            stage1,
            current_sense: UnqualifiedAdc1CurrentSense {
                adc,
                current_a0,
                current_b0,
                current_a1,
                current_b1,
                acquisition,
                configuration,
            },
            encoders,
            safety_inputs,
        }
    }
}

impl<Encoders, Current>
    EstablishedRealtimeResources<
        Encoders,
        Current,
        ClosedPowerStage<MCPWM0<'static>>,
        ClosedPowerStage<MCPWM1<'static>>,
    >
{
    /// Configures and immediately stops timer 0 in each disconnected MCPWM unit.
    ///
    /// Both contracts and both live clock representations are checked before
    /// either singleton is consumed. Timer start/stop is needed because the
    /// current HAL has no configure-while-stopped operation; no operator is
    /// connected to a pin, and all six phase pins remain no-pull inputs.
    #[allow(
        dead_code,
        reason = "closed MCPWM ownership compiles but awaits stored selection and HIL"
    )]
    pub fn activate_closed_mcpwm(
        self,
        motor0: ClosedMcpwmConfiguration,
        motor1: ClosedMcpwmConfiguration,
    ) -> Result<
        EstablishedRealtimeResources<
            Encoders,
            Current,
            ClosedMcpwmStage<MCPWM0<'static>>,
            ClosedMcpwmStage<MCPWM1<'static>>,
        >,
        ClosedMcpwmInitializationError,
    > {
        let motor0_hal = validate_closed_mcpwm_configuration(motor0)?;
        let motor1_hal = validate_closed_mcpwm_configuration(motor1)?;
        let EstablishedRealtimeResources {
            timer_group1,
            stage0,
            stage1,
            current_sense,
            encoders,
            safety_inputs,
        } = self;
        Ok(EstablishedRealtimeResources {
            timer_group1,
            stage0: initialize_closed_mcpwm_stage(stage0, motor0, motor0_hal),
            stage1: initialize_closed_mcpwm_stage(stage1, motor1, motor1_hal),
            current_sense,
            encoders,
            safety_inputs,
        })
    }
}

impl<Encoders, Current>
    EstablishedRealtimeResources<
        Encoders,
        Current,
        ClosedMcpwmStage<MCPWM0<'static>>,
        ClosedMcpwmStage<MCPWM1<'static>>,
    >
{
    /// Returns both configured-but-disconnected MCPWM clock contracts.
    #[allow(
        dead_code,
        reason = "closed MCPWM ownership compiles but is not reported before HIL"
    )]
    pub const fn closed_mcpwm_configurations(&self) -> [ClosedMcpwmConfiguration; 2] {
        [self.stage0.configuration(), self.stage1.configuration()]
    }
}

fn validate_closed_mcpwm_configuration(
    configuration: ClosedMcpwmConfiguration,
) -> Result<(PeripheralClockConfig, TimerClockConfig), ClosedMcpwmInitializationError> {
    configuration
        .contract
        .validate()
        .map_err(ClosedMcpwmInitializationError::Contract)?;
    let peripheral_clock =
        PeripheralClockConfig::with_prescaler(configuration.peripheral_prescaler);
    let peripheral_hz = peripheral_clock.frequency().as_hz();
    if u64::from(peripheral_hz).checked_mul(u64::from(configuration.peripheral_prescaler) + 1)
        != Some(u64::from(configuration.peripheral_source_clock_hz))
    {
        return Err(ClosedMcpwmInitializationError::Clock);
    }
    let timer_divisor = u32::from(configuration.timer_prescaler) + 1;
    if !peripheral_hz.is_multiple_of(timer_divisor)
        || peripheral_hz / timer_divisor != configuration.contract.counter_clock_hz()
    {
        return Err(ClosedMcpwmInitializationError::Clock);
    }
    let timer_clock = peripheral_clock.timer_clock_with_prescaler(
        configuration.contract.timer_peak_ticks(),
        PwmWorkingMode::UpDown,
        configuration.timer_prescaler,
    );
    if timer_clock.frequency().as_hz() != configuration.contract.pwm_hz() {
        return Err(ClosedMcpwmInitializationError::Clock);
    }
    Ok((peripheral_clock, timer_clock))
}

fn initialize_closed_mcpwm_stage<Pwm>(
    stage: ClosedPowerStage<Pwm>,
    configuration: ClosedMcpwmConfiguration,
    hal_configuration: (PeripheralClockConfig, TimerClockConfig),
) -> ClosedMcpwmStage<Pwm>
where
    Pwm: PwmPeripheral + 'static,
{
    let (peripheral_clock, timer_clock) = hal_configuration;
    let mut controller = McPwm::new(stage.controller, peripheral_clock);
    controller.timer0.start(timer_clock);
    controller.timer0.stop();
    controller
        .timer0
        .set_counter(0, CounterDirection::Increasing);
    ClosedMcpwmStage {
        controller,
        phase_u: stage.phase_u,
        phase_v: stage.phase_v,
        phase_w: stage.phase_w,
        configuration,
    }
}

impl<Encoders, Stage0, Stage1>
    EstablishedRealtimeResources<Encoders, UnqualifiedAdc1CurrentSense, Stage0, Stage1>
{
    /// Returns the programmed but unqualified ADC1 attenuation selections.
    #[allow(
        dead_code,
        reason = "diagnostic ADC1 ownership compiles but is not reported before HIL"
    )]
    pub const fn unqualified_current_configuration(&self) -> Adc1AcquisitionConfiguration {
        self.current_sense.configuration()
    }

    /// Begins one software-started diagnostic pair on the selected motor.
    #[allow(
        dead_code,
        reason = "diagnostic ADC1 ownership compiles but is not scheduled before HIL"
    )]
    pub fn begin_unqualified_current_pair(
        &mut self,
        axis: CurrentAxis,
        token: u32,
        requested_at: DeviceCycle,
    ) -> Result<(), Adc1AcquisitionError> {
        self.current_sense.begin_pair(axis, token, requested_at)
    }

    /// Polls the active diagnostic pair once without creating PWM evidence.
    #[allow(
        dead_code,
        reason = "diagnostic ADC1 ownership compiles but is not scheduled before HIL"
    )]
    pub fn poll_unqualified_current_pair(
        &mut self,
        observed_at: DeviceCycle,
    ) -> Result<Option<SequentialAdcPair>, Adc1AcquisitionError> {
        self.current_sense.poll_pair(observed_at)
    }

    /// Cancels a partial diagnostic pair.
    #[allow(
        dead_code,
        reason = "diagnostic ADC1 ownership compiles but is not scheduled before HIL"
    )]
    pub fn abort_unqualified_current_pair(&mut self) -> Option<SequentialAdcRequest> {
        self.current_sense.abort_pair()
    }
}

impl<Current, Stage0, Stage1>
    EstablishedRealtimeResources<As5600EncoderResources, Current, Stage0, Stage1>
{
    /// Reads one exact 12-bit mechanical count without changing sensor state.
    #[allow(
        dead_code,
        reason = "read-only sensor path is compiled but not scheduled before HIL"
    )]
    pub async fn read_encoder_raw(&mut self, axis: EncoderAxis) -> Result<RawAngle, EncoderError> {
        match axis {
            EncoderAxis::Axis0 => self.encoders.encoder0.read_raw_angle().await,
            EncoderAxis::Axis1 => self.encoders.encoder1.read_raw_angle().await,
        }
    }

    /// Reads the documented field-strength flags without assigning control policy.
    #[allow(
        dead_code,
        reason = "read-only sensor path is compiled but not scheduled before HIL"
    )]
    pub async fn read_encoder_status(
        &mut self,
        axis: EncoderAxis,
    ) -> Result<MagnetStatus, EncoderError> {
        match axis {
            EncoderAxis::Axis0 => self.encoders.encoder0.read_status().await,
            EncoderAxis::Axis1 => self.encoders.encoder1.read_status().await,
        }
    }

    /// Brackets one raw count with before/after status transactions.
    #[allow(
        dead_code,
        reason = "read-only sensor path is compiled but not scheduled before HIL"
    )]
    pub async fn read_encoder_observation(
        &mut self,
        axis: EncoderAxis,
    ) -> Result<Observation, EncoderError> {
        match axis {
            EncoderAxis::Axis0 => self.encoders.encoder0.read_observation().await,
            EncoderAxis::Axis1 => self.encoders.encoder1.read_observation().await,
        }
    }
}

impl<Encoders, Current, Stage0, Stage1>
    EstablishedRealtimeResources<Encoders, Current, Stage0, Stage1>
{
    /// Rejects configured safety routes because none is established on V1.0.
    pub fn configure_safety_inputs<const INPUTS: usize>(
        &mut self,
        profile: &RealtimeConfigurationProfile,
        nominal_scan_period_cycles: u64,
    ) -> Result<Option<SafetyInputMonitor<INPUTS>>, SafetyInputBackendError> {
        self.safety_inputs
            .configure(profile, nominal_scan_period_cycles)
    }

    /// Samples the necessarily empty active safety-input set.
    pub fn scan_safety_inputs<const INPUTS: usize>(
        &self,
        monitor: &mut SafetyInputMonitor<INPUTS>,
        at: DeviceCycle,
    ) -> Result<SafetyInputScan, SafetyInputBackendError> {
        self.safety_inputs.scan(monitor, at)
    }

    /// Keeps the empty route set in its canonical state.
    pub fn clear_safety_inputs(&mut self) {
        self.safety_inputs.clear();
    }

    /// Retains typed ownership of six no-pull inputs; no output operation exists.
    pub fn force_safe_outputs(&mut self) -> Result<(), SafeOutputError> {
        Ok(())
    }

    /// Rejects shifted step images on this direct-PWM target.
    #[allow(dead_code, reason = "keeps the selected-board backend shape explicit")]
    pub fn apply_motion_image(&mut self, _update: ShiftImageUpdate) -> Result<(), SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects capacity queries until a qualified FOC owner exists.
    pub fn motion_output_writable_horizon(
        &self,
        _observed: DeviceCycle,
    ) -> Result<DeviceCycle, SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects every step-image staging request on this FOC board.
    pub fn stage_motion_output(
        &mut self,
        _output: ScheduledShiftOutput,
    ) -> Result<(), SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects step-output horizon sealing on this FOC board.
    pub fn seal_motion_output_horizon(
        &mut self,
        _through: DeviceCycle,
    ) -> Result<DeviceCycle, SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects physical step-image observations because no backend exists.
    pub fn take_motion_commit(
        &mut self,
    ) -> Result<Option<(OutputCommitToken, DeviceCycle)>, SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }
}

/// Complete singleton partition across runtime, service, and realtime domains.
pub struct SplitResources {
    pub runtime: RuntimeResources,
    pub service: ServiceResources,
    pub realtime: RealtimeResources,
}

pub const PACKAGE: &BoardPackage<'static> = &board_mks_esp32_foc_v1::PACKAGE;

/// Consumes the HAL singleton once and creates physically disjoint domains.
pub fn split(peripherals: Peripherals) -> SplitResources {
    let Peripherals {
        TIMG0: timer_group0,
        TIMG1: timer_group1,
        CPU_CTRL: cpu_control,
        SW_INTERRUPT: software_interrupt,
        WIFI: wifi,
        UART0: uart0,
        GPIO1: uart_tx,
        GPIO3: uart_rx,
        GPIO0: boot,
        GPIO2: usb_boot_strap_io2,
        MCPWM0: mcpwm0,
        MCPWM1: mcpwm1,
        ADC1: adc1,
        I2C0: i2c0,
        I2C1: i2c1,
        GPIO32: phase_u0,
        GPIO33: phase_v0,
        GPIO25: phase_w0,
        GPIO26: phase_u1,
        GPIO27: phase_v1,
        GPIO14: phase_w1,
        GPIO39: current_a0,
        GPIO36: current_b0,
        GPIO35: current_a1,
        GPIO34: current_b1,
        GPIO18: encoder_scl0,
        GPIO19: encoder_sda0,
        GPIO15: encoder_index0,
        GPIO5: encoder_scl1,
        GPIO23: encoder_sda1,
        GPIO13: encoder_index1,
        ..
    } = peripherals;

    SplitResources {
        runtime: RuntimeResources {
            timer_group0,
            cpu_control,
            software_interrupt,
        },
        service: ServiceResources {
            wifi: Some(wifi),
            uart0,
            uart_tx,
            uart_rx,
            boot,
            usb_boot_strap_io2,
        },
        realtime: RealtimeResources {
            timer_group1,
            mcpwm0,
            mcpwm1,
            adc1,
            i2c0,
            i2c1,
            phase_u0,
            phase_v0,
            phase_w0,
            phase_u1,
            phase_v1,
            phase_w1,
            current_a0,
            current_b0,
            current_a1,
            current_b1,
            encoder_scl0,
            encoder_sda0,
            encoder_index0,
            encoder_scl1,
            encoder_sda1,
            encoder_index1,
        },
    }
}
