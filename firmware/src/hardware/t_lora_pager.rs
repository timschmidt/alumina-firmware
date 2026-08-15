//! Compile-only current T-LoRa Pager composition.
//!
//! The selected image owns Wi-Fi and the two executor timers but deliberately
//! constructs no Pager-specific peripheral. In particular, fitted SD media is
//! reported unavailable until its XL9555 power/detect route and shared-SPI
//! reset transaction have an implementation and physical evidence.

use alumina_board::BoardPackage;
use alumina_config::{RealtimeConfiguration, RealtimeConfigurationProfile};
use alumina_motion::{
    OutputCommitToken, ScheduledShiftOutput, ShiftImageContract, ShiftImageUpdate,
};
use alumina_protocol::DeviceCycle;
use alumina_safety::{SafetyContractId, SafetyInputMonitor};
use alumina_service::CACHE_LIMITS;
use alumina_storage::media::{AsyncBlockDevice, MediaBlock};
use alumina_storage::provisioning::ProvisionedCache;
use defmt::warn;
use esp_hal::peripherals::{Peripherals, TIMG1, WIFI};

use super::RuntimeResources;
use super::safety_inputs::{SafetyInputBackendError, SafetyInputBank, SafetyInputScan};

/// A board-local transport that cannot claim the fitted-but-unimplemented SD route.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UnavailableStorage;

/// Deterministic rejection returned by the compile-only storage boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum UnavailableStorageError {
    /// XL9555 power/detect and shared-SPI ownership are not composed yet.
    DriverUnimplemented,
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
        Err(UnavailableStorageError::DriverUnimplemented)
    }

    async fn write_block(&mut self, _block: u64, _data: &MediaBlock) -> Result<(), Self::Error> {
        Err(UnavailableStorageError::DriverUnimplemented)
    }

    async fn sync(&mut self) -> Result<(), Self::Error> {
        Err(UnavailableStorageError::DriverUnimplemented)
    }
}

pub type StorageBackend = ProvisionedCache<UnavailableStorage>;
/// Structural width retained by the common non-motion actor.
pub const JOB_AXES: usize = 1;
/// Maximum unique resource claims retained by each configuration validator.
pub const CONFIGURATION_BINDINGS: usize = 64;
/// The stub publishes no configuration-derived real-time input.
pub const SAFETY_INPUT_CAPACITY: usize = 0;
/// Target request storage derived from the immutable board capability.
pub const DIAGNOSTIC_TELEMETRY_REQUEST_BYTES: usize =
    board_t_lora_pager::DIAGNOSTIC_OVERVIEW.telemetry_request_bytes as usize;
/// Target event storage derived from the immutable board capability.
pub const DIAGNOSTIC_TELEMETRY_EVENT_BYTES: usize =
    board_t_lora_pager::DIAGNOSTIC_OVERVIEW.telemetry_event_bytes as usize;
/// Exact passive observation palette compiled into this image.
pub const DIAGNOSTIC_OVERVIEW_SAMPLES: usize =
    board_t_lora_pager::DIAGNOSTIC_OVERVIEW.resources.len();
/// No physical overview provider is composed by this stub.
pub const DIAGNOSTIC_RESOURCE_OVERVIEW: bool =
    board_t_lora_pager::DIAGNOSTIC_OVERVIEW.is_implemented();
/// Capability-published nominal overview period, zero while unsupported.
pub const DIAGNOSTIC_OVERVIEW_PERIOD_MICROS: u32 =
    board_t_lora_pager::DIAGNOSTIC_OVERVIEW.nominal_period_micros;
/// Capability-published freshness ceiling, zero while unsupported.
pub const DIAGNOSTIC_MAXIMUM_AGE_MICROS: u32 =
    board_t_lora_pager::DIAGNOSTIC_OVERVIEW.maximum_age_micros;
/// No machine output is implemented by the board stub.
pub const MOTION_OUTPUT_IMPLEMENTED: bool = false;
/// No physical motion path is qualified.
pub const MOTION_OUTPUT_QUALIFIED: bool = false;
/// No FOC-servo output owner is composed.
pub const SERVO_OUTPUT_IMPLEMENTED: bool = false;
/// No servo output path is physically qualified.
pub const SERVO_OUTPUT_QUALIFIED: bool = false;
/// No servo commit-reporting latency has been established.
pub const SERVO_MAXIMUM_COMMIT_OBSERVATION_LATENESS_CYCLES: u32 = 0;
/// No servo priming lead is qualified; the impossible value closes admission.
pub const SERVO_MINIMUM_PRIME_LEAD_CYCLES: u64 = u64::MAX;
/// Inert structural quantum; the board exposes no motion output.
pub const MOTION_OUTPUT_QUANTUM_CYCLES: u32 = 1;
/// No physical commit-lateness claim exists.
pub const MOTION_MAXIMUM_COMMIT_LATENESS_CYCLES: u32 = 0;
/// No prestart lead can authorize a nonexistent output backend.
pub const MOTION_MINIMUM_PRIME_LEAD_CYCLES: u64 = u64::MAX;
/// Fixed portable generated-image capacity; no Pager hardware consumes it.
pub const MOTION_OUTPUT_RING_IMAGES: usize = 64;
/// Structural placeholder only; no output horizon is reachable.
pub const MOTION_PRIME_HORIZON_CYCLES: u64 = 20_000;

/// The compile-only board has no shifted machine-output contract.
pub const fn motion_shift_contract() -> Option<ShiftImageContract> {
    None
}

/// Identity of the empty output contract: no real-time-owned output exists.
pub const SAFE_OUTPUT_CONTRACT: SafetyContractId =
    SafetyContractId([b'T', b'L', b'P', b'C', 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

/// Core-0 resources. Every board-specific peripheral remains unconstructed.
pub struct ServiceResources {
    wifi: Option<WIFI<'static>>,
}

impl ServiceResources {
    /// Moves the singleton radio token into core-0 network initialization once.
    pub fn take_wifi(&mut self) -> WIFI<'static> {
        self.wifi.take().expect("Wi-Fi token already consumed")
    }

    /// Reports the fitted SD route unavailable without probing or mutating pins.
    pub async fn initialize_storage(&mut self) -> StorageBackend {
        warn!("T-LoRa Pager SD driver is not composed in the compile-only stub");
        ProvisionedCache::transport_faulted(UnavailableStorage, CACHE_LIMITS)
    }
}

/// Core-1 token before the empty safe-output contract is established.
pub struct RealtimeResources {
    timer_group1: TIMG1<'static>,
}

/// Core-1 resources after the empty safe-output contract is established.
#[allow(dead_code, reason = "timer is reserved for later deterministic I/O")]
pub struct EstablishedRealtimeResources {
    timer_group1: TIMG1<'static>,
    safety_inputs: SafetyInputBank<0>,
}

/// Static board-contract defect detected before core 1 may publish `Safe`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeOutputError {
    /// The fixed empty safety-input route was internally inconsistent.
    SafetyInput(SafetyInputBackendError),
    /// This board package has no machine-output engine.
    MotionUnsupported,
}

/// Zero-sized selected-board acknowledgement after portable validation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PreparedTargetConfiguration;

impl RealtimeResources {
    /// Establishes an empty output contract without configuring any GPIO.
    pub fn establish_safe_outputs(self) -> Result<EstablishedRealtimeResources, SafeOutputError> {
        let safety_inputs = SafetyInputBank::<0>::new([]).map_err(SafeOutputError::SafetyInput)?;
        Ok(EstablishedRealtimeResources {
            timer_group1: self.timer_group1,
            safety_inputs,
        })
    }
}

impl EstablishedRealtimeResources {
    /// The stub adds no target selection beyond portable validation.
    pub fn prepare_target_configuration(
        &self,
        _configuration: &RealtimeConfiguration,
    ) -> Result<PreparedTargetConfiguration, SafeOutputError> {
        Ok(PreparedTargetConfiguration)
    }

    /// Completes the no-op target selection transaction.
    pub fn commit_target_configuration(
        &mut self,
        _prepared: PreparedTargetConfiguration,
    ) -> Result<(), SafeOutputError> {
        Ok(())
    }

    /// Clears the no-op target selection transaction.
    pub fn clear_target_configuration(&mut self) {}

    /// The stub has no extra machine facts to replay at authorization.
    pub fn validate_target_authorization(
        &self,
        _configuration: &RealtimeConfiguration,
    ) -> Result<(), SafeOutputError> {
        Ok(())
    }

    /// Portable validation is the complete target fact layer for this stub.
    pub fn target_configuration_ready(&self, _configuration: &RealtimeConfiguration) -> bool {
        true
    }

    /// Rejects every configured safety route because none is published.
    pub fn configure_safety_inputs<const INPUTS: usize>(
        &mut self,
        profile: &RealtimeConfigurationProfile,
        nominal_scan_period_cycles: u64,
    ) -> Result<Option<SafetyInputMonitor<INPUTS>>, SafetyInputBackendError> {
        self.safety_inputs
            .configure(profile, nominal_scan_period_cycles)
    }

    /// Samples the necessarily empty active route set.
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

    /// Reasserts the empty safe contract without configuring a pin.
    pub fn force_safe_outputs(&mut self) -> Result<(), SafeOutputError> {
        Ok(())
    }

    /// Rejects every machine-output update on this non-motion board.
    #[allow(dead_code, reason = "keeps the board backend shape explicit")]
    pub fn apply_motion_image(&mut self, _update: ShiftImageUpdate) -> Result<(), SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects every future machine-output capacity query.
    pub fn motion_output_writable_horizon(
        &self,
        _observed: DeviceCycle,
    ) -> Result<DeviceCycle, SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects every ordered future machine-output image.
    pub fn stage_motion_output(
        &mut self,
        _output: ScheduledShiftOutput,
    ) -> Result<(), SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects every continuous machine-output horizon seal.
    pub fn seal_motion_output_horizon(
        &mut self,
        _through: DeviceCycle,
    ) -> Result<DeviceCycle, SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }

    /// Rejects physical output observations because no backend exists.
    pub fn take_motion_commit(
        &mut self,
    ) -> Result<Option<(OutputCommitToken, DeviceCycle)>, SafeOutputError> {
        Err(SafeOutputError::MotionUnsupported)
    }
}

pub struct SplitResources {
    pub runtime: RuntimeResources,
    pub service: ServiceResources,
    pub realtime: RealtimeResources,
}

pub const PACKAGE: &BoardPackage<'static> = &board_t_lora_pager::PACKAGE;

/// Consumes the HAL singleton and exposes only the resources implemented here.
pub fn split(peripherals: Peripherals) -> SplitResources {
    let Peripherals {
        TIMG0: timer_group0,
        TIMG1: timer_group1,
        CPU_CTRL: cpu_control,
        SW_INTERRUPT: software_interrupt,
        WIFI: wifi,
        ..
    } = peripherals;

    SplitResources {
        runtime: RuntimeResources {
            timer_group0,
            cpu_control,
            software_interrupt,
        },
        service: ServiceResources { wifi: Some(wifi) },
        realtime: RealtimeResources { timer_group1 },
    }
}
