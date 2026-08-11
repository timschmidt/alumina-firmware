//! Non-armable MKS ESP32 FOC V1.0 ownership and safe-output composition.
//!
//! The V1.0 schematic establishes no independent inverter enable and no fitted
//! SD medium. Each phase signal is wired to an EG2133 active-high HIN and
//! active-low LIN-bar pair, so a driven high or low selects a MOSFET. This
//! target therefore makes all six phase pins no-pull inputs before the realtime
//! executor starts, exposes a permanently unavailable cache backend, and
//! rejects every motion/FOC output operation.

use alumina_board::BoardPackage;
use alumina_config::RealtimeConfigurationProfile;
use alumina_motion::{
    OutputCommitToken, ScheduledShiftOutput, ShiftImageContract, ShiftImageUpdate,
};
use alumina_protocol::DeviceCycle;
use alumina_safety::{MAX_SAFETY_INPUTS, SafetyContractId, SafetyInputMonitor};
use alumina_service::CACHE_LIMITS;
use alumina_storage::media::{AsyncBlockDevice, MediaBlock};
use alumina_storage::provisioning::ProvisionedCache;
use defmt::warn;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::peripherals::{
    ADC1, GPIO0, GPIO1, GPIO2, GPIO3, GPIO5, GPIO13, GPIO14, GPIO15, GPIO18, GPIO19, GPIO23,
    GPIO25, GPIO26, GPIO27, GPIO32, GPIO33, GPIO34, GPIO35, GPIO36, GPIO39, I2C0, I2C1, MCPWM0,
    MCPWM1, Peripherals, TIMG1, UART0, WIFI,
};

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

/// Core-1 resources after the only currently implemented safe transaction.
#[allow(
    dead_code,
    reason = "tokens remain reserved for clean-room target drivers"
)]
pub struct EstablishedRealtimeResources {
    timer_group1: TIMG1<'static>,
    mcpwm0: MCPWM0<'static>,
    mcpwm1: MCPWM1<'static>,
    adc1: ADC1<'static>,
    i2c0: I2C0<'static>,
    i2c1: I2C1<'static>,
    phase_u0: Input<'static>,
    phase_v0: Input<'static>,
    phase_w0: Input<'static>,
    phase_u1: Input<'static>,
    phase_v1: Input<'static>,
    phase_w1: Input<'static>,
    current_a0: Input<'static>,
    current_b0: Input<'static>,
    current_a1: Input<'static>,
    current_b1: Input<'static>,
    encoder_scl0: Input<'static>,
    encoder_sda0: Input<'static>,
    encoder_index0: Input<'static>,
    encoder_scl1: Input<'static>,
    encoder_sda1: Input<'static>,
    encoder_index1: Input<'static>,
    safety_inputs: SafetyInputBank<0>,
}

/// Safe-only target rejection before any FOC adapter is qualified.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeOutputError {
    /// Fixed board safety-input routes were internally inconsistent.
    SafetyInput(SafetyInputBackendError),
    /// MCPWM/ADC/current/sensor execution is intentionally unavailable.
    MotionUnsupported,
}

impl RealtimeResources {
    /// Makes all six phase pins no-pull inputs before any core-1 task can await.
    pub fn establish_safe_outputs(self) -> Result<EstablishedRealtimeResources, SafeOutputError> {
        let input = InputConfig::default().with_pull(Pull::None);
        let safety_inputs = SafetyInputBank::<0>::new([]).map_err(SafeOutputError::SafetyInput)?;
        Ok(EstablishedRealtimeResources {
            timer_group1: self.timer_group1,
            mcpwm0: self.mcpwm0,
            mcpwm1: self.mcpwm1,
            adc1: self.adc1,
            i2c0: self.i2c0,
            i2c1: self.i2c1,
            phase_u0: Input::new(self.phase_u0, input),
            phase_v0: Input::new(self.phase_v0, input),
            phase_w0: Input::new(self.phase_w0, input),
            phase_u1: Input::new(self.phase_u1, input),
            phase_v1: Input::new(self.phase_v1, input),
            phase_w1: Input::new(self.phase_w1, input),
            current_a0: Input::new(self.current_a0, input),
            current_b0: Input::new(self.current_b0, input),
            current_a1: Input::new(self.current_a1, input),
            current_b1: Input::new(self.current_b1, input),
            encoder_scl0: Input::new(self.encoder_scl0, input),
            encoder_sda0: Input::new(self.encoder_sda0, input),
            encoder_index0: Input::new(self.encoder_index0, input),
            encoder_scl1: Input::new(self.encoder_scl1, input),
            encoder_sda1: Input::new(self.encoder_sda1, input),
            encoder_index1: Input::new(self.encoder_index1, input),
            safety_inputs,
        })
    }
}

impl EstablishedRealtimeResources {
    /// Rejects configured safety routes because none is established on V1.0.
    pub fn configure_safety_inputs(
        &mut self,
        profile: &RealtimeConfigurationProfile,
        nominal_scan_period_cycles: u64,
    ) -> Result<Option<SafetyInputMonitor<MAX_SAFETY_INPUTS>>, SafetyInputBackendError> {
        self.safety_inputs
            .configure(profile, nominal_scan_period_cycles)
    }

    /// Samples the necessarily empty active safety-input set.
    pub fn scan_safety_inputs(
        &self,
        monitor: &mut SafetyInputMonitor<MAX_SAFETY_INPUTS>,
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
