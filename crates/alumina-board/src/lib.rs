#![no_std]
#![doc = "Typed compile-time board facts and validation for Alumina firmware."]

use alumina_protocol::Digest;

/// ESP application MCU families currently admitted by the architecture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Chip {
    /// Original dual-core Xtensa ESP32.
    Esp32,
    /// Dual-core Xtensa ESP32-S3.
    Esp32S3,
}

/// Evidence-backed support level for one exact board revision.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Qualification {
    /// Metadata validates, with no build claim.
    Described,
    /// Firmware compiles for the exact target.
    Compiles,
    /// Safe-state and named-peripheral hardware smoke tests pass.
    Bench,
    /// Real-time timing and fault tests pass a declared motion envelope.
    MotionQualified,
    /// Full production evidence passes.
    ProductionQualified,
}

/// Physical executor domain that exclusively owns a resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OwnerDomain {
    /// Wi-Fi, filesystem, web, display, and background executor on core 0.
    Service,
    /// Safety, motion, control loops, and deterministic I/O on core 1.
    Realtime,
}

/// Stable typed resource identifier. Numeric GPIO aliases never cross namespaces.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ResourceId {
    /// Physical GPIO number.
    Gpio(u8),
    /// Bit in a specific I²S-driven shifted-output engine.
    I2sOut { engine: u8, bit: u8 },
    /// ADC unit and channel.
    Adc { unit: u8, channel: u8 },
    /// Timer group and timer index.
    Timer { group: u8, index: u8 },
    /// I²S/parallel engine, distinct from its routed GPIOs and virtual bits.
    I2s(u8),
    /// RMT channel.
    Rmt(u8),
    /// LEDC/MCPWM/RMT-like timed-output engine and channel.
    TimedOutput { engine: u8, channel: u8 },
    /// I²C controller.
    I2c(u8),
    /// SPI controller.
    Spi(u8),
    /// UART controller.
    Uart(u8),
    /// PCNT unit.
    Pcnt(u8),
    /// DMA channel from the chip-specific DMA domain.
    Dma(u8),
    /// TWAI/CAN controller.
    Twai(u8),
    /// Board storage device or volume.
    Storage(u8),
    /// Chip or fitted radio device.
    Radio(u8),
    /// Board-local safety input chain.
    SafetyInput(u8),
    /// Board-fitted device from a board-local stable namespace.
    Device(u16),
}

/// Value driven before configuration, after a fault, and during reset.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SafeValue {
    /// Resource cannot be driven by firmware.
    NotApplicable,
    /// Input/high-impedance mode.
    HighImpedance,
    /// Logical inactive/low state.
    Low,
    /// Logical active/high state.
    High,
    /// Complete shifted-engine image is supplied elsewhere by the board package.
    EngineImage,
}

/// One physically routed resource and its immutable ownership facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceDescriptor {
    /// Stable typed identity.
    pub id: ResourceId,
    /// Exclusive executor domain.
    pub owner: OwnerDomain,
    /// Reset/fault behavior.
    pub safe_value: SafeValue,
    /// True when the resource can energize motion, torque, heat, or process power.
    pub hazardous_output: bool,
}

/// Stable capability-derived class used by graph resource handles.
///
/// A class is semantic authority, not a display category. Graph packages bind
/// both this class and one canonical [`ResourceId`] selector.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct GraphResourceClass(u32);

impl GraphResourceClass {
    /// Construct a class identifier. Board validation rejects zero.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Canonical integer representation.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Bounded operation a reviewed graph opcode may perform on one resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum GraphResourceAccess {
    /// Read the fresh, debounced semantic state of a configured safety input.
    StableBooleanInput = 1,
}

/// One fixed graph opcode implemented by this exact board image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphOpcodeDescriptor {
    /// Nonzero graph-IR opcode value.
    pub opcode: u8,
    /// Sole executor domain implementing the opcode.
    pub domain: OwnerDomain,
    /// Evidence level for the implementation and its declared timing.
    pub support: SupportLevel,
    /// Required resource class, absent for resource-free opcodes.
    pub resource_class: Option<GraphResourceClass>,
    /// Required access, present exactly when `resource_class` is present.
    pub resource_access: Option<GraphResourceAccess>,
}

/// One physical resource explicitly admitted to a graph opcode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphResourceDescriptor {
    /// Typed board resource selected by the graph parameter.
    pub resource: ResourceId,
    /// Semantic class accepted by the matching opcode.
    pub class: GraphResourceClass,
    /// Sole bounded operation permitted on the resource.
    pub access: GraphResourceAccess,
    /// Evidence level for the complete board-to-runtime path.
    pub support: SupportLevel,
}

/// Exact fixed-memory graph executor published by one board image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GraphExecutorDescriptor<'a> {
    /// Exact deployed graph-IR schema understood by the image.
    pub ir_version: u16,
    /// Exact fixed package bytes admitted by the decoder.
    pub package_bytes: u32,
    /// Format and image node-count ceiling.
    pub maximum_nodes: u16,
    /// Format and image channel-count ceiling.
    pub maximum_channels: u16,
    /// Per-channel item-count ceiling.
    pub maximum_queue_items: u32,
    /// Permanently reserved core-0 node-state bytes.
    pub service_state_bytes: u32,
    /// Permanently reserved core-1 node-state bytes.
    pub realtime_state_bytes: u32,
    /// Permanently reserved core-0 queue bytes.
    pub service_channel_bytes: u32,
    /// Permanently reserved core-1 queue bytes.
    pub realtime_channel_bytes: u32,
    /// Permanently reserved one-way core-0-to-core-1 bridge bytes.
    pub bridge_channel_bytes: u32,
    /// Evidence level for the fixed executor as a whole.
    pub support: SupportLevel,
    /// Exact opcode palette in ascending wire-value order.
    pub opcodes: &'a [GraphOpcodeDescriptor],
    /// Explicit graph-addressable resource palette.
    pub resources: &'a [GraphResourceDescriptor],
}

/// Passive semantic value exposed by one diagnostic-overview record.
///
/// This is observation authority only. It neither admits a graph operation nor
/// grants a lease, pin-mode change, interrupt route, or output capability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DiagnosticObservationKind {
    /// Freshness-bounded, debounced state of a configured safety input.
    StableBooleanInput = 1,
}

/// One typed resource admitted to the passive diagnostic-overview provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticResourceDescriptor {
    /// Exact physical or board-semantic resource represented in samples.
    pub resource: ResourceId,
    /// Semantic observation produced for this resource.
    pub observation: DiagnosticObservationKind,
    /// Evidence level for the complete resource-to-overview path.
    pub support: SupportLevel,
}

/// Exact fixed-memory passive overview provider published by one board image.
///
/// Unsupported images use [`Self::NONE`]. The timing values are expressed in
/// microseconds so a browser can reconcile them with authenticated device-clock
/// evidence without importing an ESP- or Embassy-specific tick rate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DiagnosticOverviewDescriptor<'a> {
    /// Exact `ALMOVW` record schema emitted by this provider, or zero if absent.
    pub schema_version: u16,
    /// Whole-provider evidence floor, absent when no provider is composed.
    pub support: Option<SupportLevel>,
    /// Largest resource selection admitted by one subscription.
    pub maximum_resources: u16,
    /// Permanently reserved canonical subscription bytes.
    pub telemetry_request_bytes: u32,
    /// Permanently reserved canonical event bytes.
    pub telemetry_event_bytes: u32,
    /// Nominal interval between provider publications.
    pub nominal_period_micros: u32,
    /// Oldest physical sample that may still be reported as fresh.
    pub maximum_age_micros: u32,
    /// Strictly ordered passive observation palette.
    pub resources: &'a [DiagnosticResourceDescriptor],
}

impl DiagnosticOverviewDescriptor<'_> {
    /// Canonical declaration for an image with no passive overview provider.
    pub const NONE: Self = Self {
        schema_version: 0,
        support: None,
        maximum_resources: 0,
        telemetry_request_bytes: 0,
        telemetry_event_bytes: 0,
        nominal_period_micros: 0,
        maximum_age_micros: 0,
        resources: &[],
    };

    /// Whether this exact image has at least a compiling overview provider.
    pub const fn is_implemented(self) -> bool {
        matches!(
            self.support,
            Some(SupportLevel::Compiles | SupportLevel::Bench | SupportLevel::Qualified)
        )
    }
}

/// Acquisition mechanism admitted for one device-produced digital channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum DigitalCaptureSourceKind {
    /// Deterministic host simulation, never physical measurement.
    Simulated = 1,
    /// ESP RMT edge/timestamp acquisition.
    Rmt = 2,
    /// ESP pulse-counter acquisition.
    Pcnt = 3,
    /// Peripheral DMA acquisition.
    Dma = 4,
    /// Qualified bounded software sampling.
    Software = 5,
}

/// Canonical waveform-configure flags admitted by a digital-capture provider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct DigitalCaptureConfigureFlags(pub u16);

impl DigitalCaptureConfigureFlags {
    /// Capture retains edge timestamps rather than a sampled analog stream.
    pub const EDGE_TIMESTAMPS: u16 = 1 << 0;
    /// Caller may accept qualified bounded software sampling.
    pub const ALLOW_SOFTWARE: u16 = 1 << 1;
    /// All flags understood by this board schema.
    pub const KNOWN: u16 = Self::EDGE_TIMESTAMPS | Self::ALLOW_SOFTWARE;

    /// Whether one flag is present.
    pub const fn contains(self, flag: u16) -> bool {
        self.0 & flag != 0
    }
}

/// Trigger predicates admitted by one digital-capture provider.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct DigitalCaptureTriggerSet(pub u8);

impl DigitalCaptureTriggerSet {
    /// Begin acquisition immediately without a channel event.
    pub const IMMEDIATE: u8 = 1 << 0;
    /// Low-to-high logical transition.
    pub const RISING: u8 = 1 << 1;
    /// High-to-low logical transition.
    pub const FALLING: u8 = 1 << 2;
    /// Either logical transition.
    pub const EITHER: u8 = 1 << 3;
    /// All trigger bits understood by this board schema.
    pub const KNOWN: u8 = Self::IMMEDIATE | Self::RISING | Self::FALLING | Self::EITHER;

    /// Whether one trigger bit is present.
    pub const fn contains(self, trigger: u8) -> bool {
        self.0 & trigger != 0
    }
}

/// One typed resource admitted to a device-produced digital capture.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalCaptureResourceDescriptor {
    /// Exact physical or board-semantic resource represented by the channel.
    pub resource: ResourceId,
    /// Acquisition mechanism the retained channel must report.
    pub source: DigitalCaptureSourceKind,
    /// Evidence level for this complete resource-to-capture path.
    pub support: SupportLevel,
}

/// Exact fixed-memory digital edge-capture provider published by one image.
///
/// This grants only bounded evidence acquisition over the listed resources. It
/// grants no graph operation, output lease, pin-mode change, arm transition, or
/// safety authority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DigitalCaptureDescriptor<'a> {
    /// Exact `ALMDIG` record schema emitted by this provider, or zero if absent.
    pub schema_version: u16,
    /// Whole-provider evidence floor, absent when no provider is declared.
    pub support: Option<SupportLevel>,
    /// Exact canonical configure flags accepted by this provider.
    pub configure_flags: DigitalCaptureConfigureFlags,
    /// Set of admitted trigger predicates.
    pub trigger_kinds: DigitalCaptureTriggerSet,
    /// Largest channel selection admitted by one configuration.
    pub maximum_channels: u16,
    /// Largest retained transition capacity admitted by one configuration.
    pub maximum_transitions: u32,
    /// Permanently reserved canonical configure-request bytes.
    pub configure_bytes: u32,
    /// Permanently reserved complete retained-record bytes.
    pub record_bytes: u32,
    /// Largest canonical range/chunk payload.
    pub maximum_chunk_bytes: u32,
    /// Largest pretrigger interval in microseconds.
    pub maximum_pretrigger_micros: u32,
    /// Largest complete requested capture duration in microseconds.
    pub maximum_duration_micros: u32,
    /// Largest trigger-deadline window in microseconds.
    pub arm_horizon_micros: u32,
    /// Strictly ordered channel palette.
    pub resources: &'a [DigitalCaptureResourceDescriptor],
}

impl DigitalCaptureDescriptor<'_> {
    /// Canonical declaration for an image with no digital-capture provider.
    pub const NONE: Self = Self {
        schema_version: 0,
        support: None,
        configure_flags: DigitalCaptureConfigureFlags(0),
        trigger_kinds: DigitalCaptureTriggerSet(0),
        maximum_channels: 0,
        maximum_transitions: 0,
        configure_bytes: 0,
        record_bytes: 0,
        maximum_chunk_bytes: 0,
        maximum_pretrigger_micros: 0,
        maximum_duration_micros: 0,
        arm_horizon_micros: 0,
        resources: &[],
    };

    /// Whether this exact image has at least a compiling capture provider.
    pub const fn is_implemented(self) -> bool {
        matches!(
            self.support,
            Some(SupportLevel::Compiles | SupportLevel::Bench | SupportLevel::Qualified)
        )
    }
}

/// Compile-time flash and RAM facts for one PCB/module revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryDescriptor {
    /// Executable/data flash bytes fitted to the selected module.
    pub flash_bytes: usize,
    /// Internal SRAM bytes before static/runtime reservations.
    pub internal_sram_bytes: usize,
    /// External PSRAM bytes, or zero when absent.
    pub psram_bytes: usize,
    /// PSRAM is never admitted for the deterministic core's active state.
    pub realtime_psram_allowed: bool,
}

/// Fixed application-core assignment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CoreAssignment {
    /// Wi-Fi, storage, UI, and background executor.
    pub service_core: u8,
    /// Safety, motion, and hardware-timed executor.
    pub realtime_core: u8,
}

/// One canonical or silkscreen alias resolved before real-time admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AliasDescriptor<'a> {
    /// Case-sensitive stable alias.
    pub name: &'a str,
    /// Typed target; aliases never change the target namespace.
    pub resource: ResourceId,
}

/// Board-level serial bus family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BusKind {
    /// Inter-integrated circuit controller.
    I2c,
    /// Serial peripheral interface controller.
    Spi,
    /// Asynchronous serial controller.
    Uart,
}

/// Routed bus and its exclusive pin ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusDescriptor<'a> {
    /// Typed I²C/SPI/UART controller resource.
    pub resource: ResourceId,
    /// Expected controller family.
    pub kind: BusKind,
    /// Core domain that constructs and owns the controller and interrupts.
    pub owner: OwnerDomain,
    /// GPIO resources forming the bus, in board-documented order.
    pub pins: &'a [ResourceId],
    /// Highest board-admitted bus frequency before machine-specific reduction.
    pub maximum_frequency_hz: u32,
}

/// Addressing/routing mechanism for a fitted device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceRoute {
    /// No serial-bus route; device uses dedicated resources.
    Dedicated,
    /// Seven-bit I²C address on the referenced bus.
    I2cAddress(u8),
    /// Independent active-low SPI chip-select GPIO.
    SpiChipSelect(ResourceId),
    /// Point-to-point UART route.
    Uart,
}

/// Implementation evidence for a routed device or engine.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum SupportLevel {
    /// Fact is described, but no implementation claim exists.
    Described,
    /// A driver exists and compiles for the selected chip.
    Compiles,
    /// Driver passes a named physical smoke test.
    Bench,
    /// Driver passes its declared deterministic timing envelope.
    Qualified,
}

/// One fitted board device or board-routed configurable device endpoint.
///
/// A routed endpoint describes the only driver/bus/address combination the
/// board can admit; `support` is implementation evidence, not proof that an
/// optional external peripheral is physically present.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DeviceDescriptor<'a> {
    /// Must be a `ResourceId::Device` or `ResourceId::Storage` in `resources`.
    pub resource: ResourceId,
    /// Exclusive executor domain.
    pub owner: OwnerDomain,
    /// Controller resource, if any.
    pub bus: Option<ResourceId>,
    /// Address or chip-select routing within the bus.
    pub route: DeviceRoute,
    /// Dedicated interrupt, reset, enable, data/clock, or other routed signals.
    /// Each retains its own domain; a service bus may observe an RT-owned input
    /// through the cross-core boundary.
    pub auxiliary_resources: &'a [ResourceId],
    /// Current compile/bench evidence.
    pub support: SupportLevel,
}

/// Intended use of one immutable flash interval.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlashRegionKind {
    /// Second-stage bootloader image.
    Bootloader,
    /// ESP partition table.
    PartitionTable,
    /// Executable firmware slot.
    Application,
    /// Transactional device and machine configuration.
    Configuration,
    /// Exact web/WASM bundle served by the device.
    WebBundle,
    /// Firmware update candidate or rollback slot.
    UpdateSlot,
    /// Bounded crash/fault record storage.
    CrashLog,
}

/// One non-overlapping region in the board image's selected flash layout.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FlashRegionDescriptor<'a> {
    /// Stable partition/region name.
    pub name: &'a str,
    /// Byte offset from the beginning of flash.
    pub offset: u32,
    /// Region length in bytes.
    pub length: u32,
    /// Intended contents.
    pub kind: FlashRegionKind,
    /// Must be false for every region in an armable image.
    pub writable_while_armed: bool,
    /// Evidence for this exact layout rather than a vendor/example layout.
    pub support: SupportLevel,
}

/// Executor relationship of one clock domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockDomain {
    /// Shared chip/module clock or source.
    Chip,
    /// Clock used only by service-core work.
    Service,
    /// Clock used for deterministic scheduling or I/O.
    Realtime,
}

/// Physical or derived source of a clock.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClockSource {
    /// Board/module crystal oscillator.
    Crystal,
    /// Internal phase-locked loop.
    Pll,
    /// APB/peripheral clock tree.
    PeripheralBus,
    /// RTC slow/fast clock tree.
    Rtc,
    /// Externally supplied clock or reference.
    External,
}

/// Board clock fact exposed to timing admission and the UI compiler.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClockDescriptor<'a> {
    /// Stable clock-domain name.
    pub name: &'a str,
    /// Source family.
    pub source: ClockSource,
    /// Nominal integer frequency.
    pub nominal_hz: u64,
    /// `None` until a datasheet bound or measurement is admitted.
    pub maximum_error_ppm: Option<u32>,
    /// Core-domain relationship.
    pub domain: ClockDomain,
    /// Evidence for the frequency and error bound.
    pub support: SupportLevel,
}

/// Electrical or routing rule that cannot be inferred from a numeric pin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ElectricalConstraintKind {
    /// Silicon or board route cannot drive this signal.
    InputOnly,
    /// Shifted or dedicated output cannot be sampled as an input.
    OutputOnly,
    /// Boot value affects reset/boot mode and must be preserved.
    BootStrap,
    /// One physical route has mutually exclusive named functions.
    SharedRoute,
    /// Active state is high.
    ActiveHigh,
    /// Active state is low.
    ActiveLow,
    /// Route cannot provide hardware PWM semantics.
    NotPwm,
    /// Route is limited to 3.3 V logic unless external conditioning is declared.
    Logic3v3,
    /// Safe reset behavior is not yet established physically.
    ResetStateUnverified,
}

/// Evidence-backed electrical constraint over one or more resources.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ElectricalConstraintDescriptor<'a> {
    /// Stable rule ID used by diagnostics.
    pub id: &'a str,
    /// Machine-checkable family.
    pub kind: ElectricalConstraintKind,
    /// Resources governed by the rule.
    pub resources: &'a [ResourceId],
    /// Concise human context; not parsed for admission.
    pub note: &'a str,
    /// Evidence for this exact PCB revision.
    pub support: SupportLevel,
}

/// Hardware interrupt assertion behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InterruptTrigger {
    /// Rising edge.
    Rising,
    /// Falling edge.
    Falling,
    /// Either edge, typically chosen by stored configuration.
    AnyEdge,
    /// Active-low level.
    LowLevel,
    /// Active-high level.
    HighLevel,
    /// Polarity/edge is selected by machine configuration.
    Configurable,
}

/// One interrupt-capable route and its fixed core ownership.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InterruptDescriptor {
    /// GPIO or controller resource that asserts the interrupt.
    pub source: ResourceId,
    /// Executor/interrupt domain that handles it.
    pub owner: OwnerDomain,
    /// Electrical trigger behavior.
    pub trigger: InterruptTrigger,
    /// Qualified worst-case response in device cycles, once measured.
    pub maximum_latency_cycles: Option<u64>,
    /// Current evidence level.
    pub support: SupportLevel,
}

/// Integer point in normalized 0–10,000 image coordinates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NormalizedPoint {
    /// Horizontal coordinate, left to right.
    pub x: u16,
    /// Vertical coordinate, top to bottom.
    pub y: u16,
}

/// Polygon linking a visible region to one typed resource.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HotspotDescriptor<'a> {
    /// Stable hotspot ID.
    pub id: &'a str,
    /// Typed resource shown by this polygon.
    pub resource: ResourceId,
    /// Three or more normalized polygon vertices.
    pub polygon: &'a [NormalizedPoint],
}

/// Independently licensed board visual and its resource overlay.
///
/// Physical packages use reviewed, revision-specific photographs. A simulator
/// package may instead declare an unmistakably synthetic fixture so the same
/// acquisition and presentation boundary can be exercised without implying
/// physical correspondence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardVisualDescriptor<'a> {
    /// Stable view ID such as `top` or `connector-side`.
    pub id: &'a str,
    /// Repository-relative raster asset path.
    pub asset_path: &'a str,
    /// MIME type of the raster asset.
    pub media_type: &'a str,
    /// Exact pixel dimensions used when the hotspot map was reviewed.
    pub pixel_width: u32,
    /// Exact pixel dimensions used when the hotspot map was reviewed.
    pub pixel_height: u32,
    /// Content digest of the asset bytes.
    pub asset_digest: Digest,
    /// SPDX expression applying to the asset.
    pub license: &'a str,
    /// Required human-readable attribution/source.
    pub attribution: &'a str,
    /// Resource polygons in normalized coordinates.
    pub hotspots: &'a [HotspotDescriptor<'a>],
}

/// Hardware-in-the-loop evidence family.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HilKind {
    /// Revision and physical-route reconciliation.
    BoardIdentity,
    /// Boot, unconfigured, fault, and watchdog output image.
    SafeState,
    /// One fitted device or bus smoke path.
    PeripheralSmoke,
    /// Core ownership and service-load isolation.
    CoreIsolation,
    /// Interrupt or executor deadline envelope.
    Timing,
    /// Limit, E-stop, malformed data, watchdog, or reset injection.
    FaultInjection,
    /// Photograph and hotspot reconciliation against the fixture.
    VisualReconciliation,
}

/// Required evidence item; results live in immutable evidence records.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HilRequirement<'a> {
    /// Stable test ID.
    pub id: &'a str,
    /// Test family.
    pub kind: HilKind,
    /// Resources explicitly exercised, or empty for a whole-board test.
    pub resources: &'a [ResourceId],
    /// Qualification that cannot be claimed without this passing result.
    pub required_for: Qualification,
}

/// Complete safe image for one serialized output engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SafeOutputImage {
    /// I²S engine whose virtual bits are covered.
    pub engine: u8,
    /// Bits defined by this image. Unrouted/reserved bits remain excluded.
    pub defined_mask: u32,
    /// Values driven at boot, unconfigured state, fault, and watchdog expiry.
    pub safe_bits: u32,
    /// False until physical polarity and reset behavior have been reconciled.
    pub bench_verified: bool,
}

/// Full immutable package exported to firmware, simulator, and capability tools.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardPackage<'a> {
    /// Stable identity and flat typed-resource inventory.
    pub board: BoardDescriptor<'a>,
    /// Memory fitted to this exact module/revision.
    pub memory: MemoryDescriptor,
    /// Physical executor assignment.
    pub cores: CoreAssignment,
    /// Fixed graph executor, arenas, opcodes, and resource palette.
    pub graph: GraphExecutorDescriptor<'a>,
    /// Passive diagnostic-overview provider and observation palette.
    pub diagnostic_overview: DiagnosticOverviewDescriptor<'a>,
    /// Explicit digital edge-capture provider and channel palette.
    pub digital_capture: DigitalCaptureDescriptor<'a>,
    /// Canonical configuration aliases.
    pub aliases: &'a [AliasDescriptor<'a>],
    /// Routed controller/pin groups.
    pub buses: &'a [BusDescriptor<'a>],
    /// Fitted device topology.
    pub devices: &'a [DeviceDescriptor<'a>],
    /// Selected internal-flash layout; empty until a layout is established.
    pub flash_regions: &'a [FlashRegionDescriptor<'a>],
    /// Clock facts and admitted error bounds.
    pub clocks: &'a [ClockDescriptor<'a>],
    /// Electrical, multiplexing, and boot-strap constraints.
    pub electrical_constraints: &'a [ElectricalConstraintDescriptor<'a>],
    /// Interrupt-capable routes and fixed executor ownership.
    pub interrupts: &'a [InterruptDescriptor],
    /// Serialized-engine boot/fault images.
    pub safe_output_images: &'a [SafeOutputImage],
    /// Independently licensed board photos and resource polygons.
    pub visuals: &'a [BoardVisualDescriptor<'a>],
    /// HIL suite required to promote this exact revision.
    pub hil_requirements: &'a [HilRequirement<'a>],
    /// False prevents arming even when metadata validation succeeds.
    pub armable: bool,
}

/// Immutable facts for one PCB revision.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoardDescriptor<'a> {
    /// Stable board/revision ID used in manifests.
    pub id: &'a str,
    /// Human-readable revision string.
    pub revision: &'a str,
    /// Application MCU.
    pub chip: Chip,
    /// Number of application cores available to Alumina.
    pub application_cores: u8,
    /// Current evidence level.
    pub qualification: Qualification,
    /// Canonical serialized capability digest, once generated.
    pub capability_digest: Digest,
    /// Complete set of routed resources advertised by this package.
    pub resources: &'a [ResourceDescriptor],
}

impl BoardDescriptor<'_> {
    /// Validates architecture-wide board invariants without allocation.
    pub fn validate(&self) -> Result<(), BoardError> {
        if self.id.is_empty() {
            return Err(BoardError::MissingId);
        }
        if self.revision.is_empty() {
            return Err(BoardError::MissingRevision);
        }
        if self.application_cores < 2 {
            return Err(BoardError::InsufficientCores {
                found: self.application_cores,
            });
        }

        let mut outer = 0;
        while outer < self.resources.len() {
            let resource = self.resources[outer];
            if resource.hazardous_output {
                if resource.owner != OwnerDomain::Realtime {
                    return Err(BoardError::HazardOwnedByService { id: resource.id });
                }
                if matches!(resource.safe_value, SafeValue::NotApplicable) {
                    return Err(BoardError::HazardMissingSafeValue { id: resource.id });
                }
            }

            let mut inner = outer + 1;
            while inner < self.resources.len() {
                if resource.id == self.resources[inner].id {
                    return Err(BoardError::DuplicateResource { id: resource.id });
                }
                inner += 1;
            }
            outer += 1;
        }
        Ok(())
    }
}

impl BoardPackage<'_> {
    /// Validates cross-table topology, ownership, safe images, and arming claims.
    pub fn validate(&self) -> Result<(), BoardError> {
        self.board.validate()?;
        if self.memory.flash_bytes == 0 || self.memory.internal_sram_bytes == 0 {
            return Err(BoardError::MissingMemoryFacts);
        }
        if self.memory.realtime_psram_allowed {
            return Err(BoardError::RealtimePsramForbidden);
        }
        if self.cores.service_core == self.cores.realtime_core
            || self.cores.service_core >= self.board.application_cores
            || self.cores.realtime_core >= self.board.application_cores
        {
            return Err(BoardError::InvalidCoreAssignment {
                service: self.cores.service_core,
                realtime: self.cores.realtime_core,
                available: self.board.application_cores,
            });
        }

        self.validate_graph_executor()?;
        self.validate_diagnostic_overview()?;
        self.validate_digital_capture()?;

        for (index, alias) in self.aliases.iter().enumerate() {
            if alias.name.is_empty() {
                return Err(BoardError::EmptyAlias { index });
            }
            if self.resource(alias.resource).is_none() {
                return Err(BoardError::AliasMissingResource {
                    index,
                    resource: alias.resource,
                });
            }
            for (other_index, other) in self.aliases[..index].iter().enumerate() {
                if alias.name == other.name {
                    return Err(BoardError::DuplicateAlias {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }

        for (index, bus) in self.buses.iter().enumerate() {
            let resource = self
                .resource(bus.resource)
                .ok_or(BoardError::BusMissingResource {
                    index,
                    resource: bus.resource,
                })?;
            if !bus_kind_matches(bus.kind, bus.resource) {
                return Err(BoardError::BusKindMismatch {
                    index,
                    resource: bus.resource,
                });
            }
            if resource.owner != bus.owner {
                return Err(BoardError::OwnershipMismatch {
                    resource: bus.resource,
                });
            }
            if bus.pins.is_empty() || bus.maximum_frequency_hz == 0 {
                return Err(BoardError::IncompleteBus { index });
            }
            for pin in bus.pins {
                let pin_descriptor = self.resource(*pin).ok_or(BoardError::BusMissingResource {
                    index,
                    resource: *pin,
                })?;
                if !matches!(pin, ResourceId::Gpio(_)) || pin_descriptor.owner != bus.owner {
                    return Err(BoardError::OwnershipMismatch { resource: *pin });
                }
            }
        }

        for (index, device) in self.devices.iter().enumerate() {
            let descriptor =
                self.resource(device.resource)
                    .ok_or(BoardError::DeviceMissingResource {
                        index,
                        resource: device.resource,
                    })?;
            if !matches!(
                device.resource,
                ResourceId::Device(_) | ResourceId::Storage(_)
            ) {
                return Err(BoardError::InvalidDeviceResource {
                    index,
                    resource: device.resource,
                });
            }
            if descriptor.owner != device.owner {
                return Err(BoardError::OwnershipMismatch {
                    resource: device.resource,
                });
            }
            for auxiliary in device.auxiliary_resources {
                let _ = self
                    .resource(*auxiliary)
                    .ok_or(BoardError::DeviceMissingResource {
                        index,
                        resource: *auxiliary,
                    })?;
            }
            match (device.bus, device.route) {
                (None, DeviceRoute::Dedicated) => {}
                (Some(bus), route) => {
                    let bus_descriptor = self
                        .buses
                        .iter()
                        .find(|candidate| candidate.resource == bus)
                        .ok_or(BoardError::DeviceMissingBus { index, bus })?;
                    if bus_descriptor.owner != device.owner
                        || !route_matches_bus(route, bus_descriptor.kind)
                    {
                        return Err(BoardError::DeviceRouteMismatch { index, bus });
                    }
                    if let DeviceRoute::SpiChipSelect(chip_select) = route {
                        let chip_select_descriptor = self.resource(chip_select).ok_or(
                            BoardError::DeviceMissingResource {
                                index,
                                resource: chip_select,
                            },
                        )?;
                        if !matches!(chip_select, ResourceId::Gpio(_))
                            || chip_select_descriptor.owner != device.owner
                        {
                            return Err(BoardError::DeviceRouteMismatch { index, bus });
                        }
                    }
                }
                _ => {
                    return Err(BoardError::DeviceRouteMismatch {
                        index,
                        bus: device.bus.unwrap_or(device.resource),
                    });
                }
            }
        }

        for (index, region) in self.flash_regions.iter().enumerate() {
            if region.name.is_empty() || region.length == 0 {
                return Err(BoardError::IncompleteFlashRegion { index });
            }
            let end = region
                .offset
                .checked_add(region.length)
                .ok_or(BoardError::FlashRegionOutsideDevice { index })?;
            let flash_bytes = u32::try_from(self.memory.flash_bytes)
                .map_err(|_| BoardError::FlashRegionOutsideDevice { index })?;
            if end > flash_bytes {
                return Err(BoardError::FlashRegionOutsideDevice { index });
            }
            if self.armable && region.writable_while_armed {
                return Err(BoardError::ArmableWritableFlashRegion { index });
            }
            for (other_index, other) in self.flash_regions[..index].iter().enumerate() {
                if region.name == other.name {
                    return Err(BoardError::DuplicateFlashRegionName {
                        first: other_index,
                        second: index,
                    });
                }
                let other_end = other
                    .offset
                    .checked_add(other.length)
                    .ok_or(BoardError::FlashRegionOutsideDevice { index: other_index })?;
                if region.offset < other_end && other.offset < end {
                    return Err(BoardError::OverlappingFlashRegions {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }

        for (index, clock) in self.clocks.iter().enumerate() {
            if clock.name.is_empty() || clock.nominal_hz == 0 {
                return Err(BoardError::IncompleteClock { index });
            }
            if let Some(error) = clock.maximum_error_ppm
                && error == 0
                && clock.support < SupportLevel::Bench
            {
                return Err(BoardError::UnsubstantiatedExactClock { index });
            }
            for (other_index, other) in self.clocks[..index].iter().enumerate() {
                if clock.name == other.name {
                    return Err(BoardError::DuplicateClock {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }

        for (index, constraint) in self.electrical_constraints.iter().enumerate() {
            if constraint.id.is_empty()
                || constraint.note.is_empty()
                || constraint.resources.is_empty()
            {
                return Err(BoardError::IncompleteElectricalConstraint { index });
            }
            for resource in constraint.resources {
                if self.resource(*resource).is_none() {
                    return Err(BoardError::ConstraintMissingResource {
                        index,
                        resource: *resource,
                    });
                }
            }
            for (other_index, other) in self.electrical_constraints[..index].iter().enumerate() {
                if constraint.id == other.id {
                    return Err(BoardError::DuplicateElectricalConstraint {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }

        for (index, interrupt) in self.interrupts.iter().enumerate() {
            let source =
                self.resource(interrupt.source)
                    .ok_or(BoardError::InterruptMissingResource {
                        index,
                        resource: interrupt.source,
                    })?;
            if source.owner != interrupt.owner {
                return Err(BoardError::OwnershipMismatch {
                    resource: interrupt.source,
                });
            }
            if interrupt.maximum_latency_cycles == Some(0) {
                return Err(BoardError::InvalidInterruptLatency { index });
            }
            if self.interrupts[..index]
                .iter()
                .any(|other| other.source == interrupt.source)
            {
                return Err(BoardError::DuplicateInterrupt {
                    resource: interrupt.source,
                });
            }
        }

        for (index, image) in self.safe_output_images.iter().enumerate() {
            if image.safe_bits & !image.defined_mask != 0 {
                return Err(BoardError::SafeImageOutsideMask {
                    engine: image.engine,
                });
            }
            let engine = ResourceId::I2s(image.engine);
            let descriptor = self
                .resource(engine)
                .ok_or(BoardError::SafeImageMissingEngine {
                    engine: image.engine,
                })?;
            if descriptor.owner != OwnerDomain::Realtime {
                return Err(BoardError::OwnershipMismatch { resource: engine });
            }
            if self.safe_output_images[..index]
                .iter()
                .any(|other| other.engine == image.engine)
            {
                return Err(BoardError::DuplicateSafeImage {
                    engine: image.engine,
                });
            }
            if self.armable && !image.bench_verified {
                return Err(BoardError::UnverifiedSafeImage {
                    engine: image.engine,
                });
            }
        }

        for resource in self.board.resources {
            let ResourceId::I2sOut { engine, bit } = resource.id else {
                continue;
            };
            if !resource.hazardous_output || resource.safe_value != SafeValue::EngineImage {
                continue;
            }
            if bit >= 32 {
                return Err(BoardError::SafeImageMissingBit { engine, bit });
            }
            let covered = self
                .safe_output_images
                .iter()
                .find(|image| image.engine == engine)
                .is_some_and(|image| image.defined_mask & (1_u32 << bit) != 0);
            if !covered {
                return Err(BoardError::SafeImageMissingBit { engine, bit });
            }
        }

        for (visual_index, visual) in self.visuals.iter().enumerate() {
            if visual.id.is_empty()
                || visual.asset_path.is_empty()
                || visual.media_type.is_empty()
                || visual.pixel_width == 0
                || visual.pixel_height == 0
                || visual.asset_digest.is_zero()
                || visual.license.is_empty()
                || visual.attribution.is_empty()
                || visual.hotspots.is_empty()
            {
                return Err(BoardError::IncompleteVisual {
                    visual: visual_index,
                });
            }
            for (other_index, other) in self.visuals[..visual_index].iter().enumerate() {
                if visual.id == other.id {
                    return Err(BoardError::DuplicateVisual {
                        first: other_index,
                        second: visual_index,
                    });
                }
            }
            for (hotspot_index, hotspot) in visual.hotspots.iter().enumerate() {
                if hotspot.id.is_empty() || hotspot.polygon.len() < 3 {
                    return Err(BoardError::IncompleteHotspot {
                        visual: visual_index,
                        hotspot: hotspot_index,
                    });
                }
                if self.resource(hotspot.resource).is_none() {
                    return Err(BoardError::HotspotMissingResource {
                        visual: visual_index,
                        hotspot: hotspot_index,
                        resource: hotspot.resource,
                    });
                }
                if hotspot
                    .polygon
                    .iter()
                    .any(|point| point.x > 10_000 || point.y > 10_000)
                {
                    return Err(BoardError::HotspotOutsideImage {
                        visual: visual_index,
                        hotspot: hotspot_index,
                    });
                }
                for (other_index, other) in visual.hotspots[..hotspot_index].iter().enumerate() {
                    if hotspot.id == other.id {
                        return Err(BoardError::DuplicateHotspot {
                            visual: visual_index,
                            first: other_index,
                            second: hotspot_index,
                        });
                    }
                }
            }
        }

        for (index, requirement) in self.hil_requirements.iter().enumerate() {
            if requirement.id.is_empty() {
                return Err(BoardError::IncompleteHilRequirement { index });
            }
            for resource in requirement.resources {
                if self.resource(*resource).is_none() {
                    return Err(BoardError::HilMissingResource {
                        index,
                        resource: *resource,
                    });
                }
            }
            for (other_index, other) in self.hil_requirements[..index].iter().enumerate() {
                if requirement.id == other.id {
                    return Err(BoardError::DuplicateHilRequirement {
                        first: other_index,
                        second: index,
                    });
                }
            }
        }

        if self.armable && self.board.capability_digest.is_zero() {
            return Err(BoardError::ArmableWithoutCapabilityDigest);
        }
        Ok(())
    }

    /// Finds one flat resource descriptor by typed ID.
    pub fn resource(&self, id: ResourceId) -> Option<&ResourceDescriptor> {
        self.board
            .resources
            .iter()
            .find(|resource| resource.id == id)
    }

    fn validate_graph_executor(&self) -> Result<(), BoardError> {
        let graph = self.graph;
        if graph.ir_version == 0
            || graph.package_bytes == 0
            || graph.maximum_nodes == 0
            || graph.maximum_channels == 0
            || graph.maximum_queue_items == 0
            || graph.service_state_bytes == 0
            || graph.realtime_state_bytes == 0
            || graph.service_channel_bytes == 0
            || graph.realtime_channel_bytes == 0
            || graph.bridge_channel_bytes == 0
            || graph.opcodes.is_empty()
        {
            return Err(BoardError::IncompleteGraphExecutor);
        }
        for (index, opcode) in graph.opcodes.iter().copied().enumerate() {
            if opcode.opcode == 0
                || opcode.resource_class.is_some() != opcode.resource_access.is_some()
                || index != 0 && graph.opcodes[index - 1].opcode >= opcode.opcode
                || opcode
                    .resource_class
                    .is_some_and(|resource_class| resource_class.get() == 0)
            {
                return Err(BoardError::InvalidGraphOpcode { index });
            }
        }
        for (index, resource) in graph.resources.iter().copied().enumerate() {
            let physical =
                self.resource(resource.resource)
                    .ok_or(BoardError::GraphMissingResource {
                        index,
                        resource: resource.resource,
                    })?;
            if resource.class.get() == 0
                || physical.owner != OwnerDomain::Realtime
                || physical.hazardous_output
                || physical.safe_value != SafeValue::HighImpedance
                || !matches!(
                    resource.resource,
                    ResourceId::Gpio(_) | ResourceId::SafetyInput(_)
                )
                || !graph.opcodes.iter().any(|opcode| {
                    opcode.domain == OwnerDomain::Realtime
                        && opcode.resource_class == Some(resource.class)
                        && opcode.resource_access == Some(resource.access)
                })
            {
                return Err(BoardError::InvalidGraphResource {
                    index,
                    resource: resource.resource,
                });
            }
            if graph.resources[..index]
                .iter()
                .any(|prior| prior.class == resource.class && prior.resource == resource.resource)
            {
                return Err(BoardError::DuplicateGraphResource {
                    resource: resource.resource,
                });
            }
        }
        Ok(())
    }

    fn validate_diagnostic_overview(&self) -> Result<(), BoardError> {
        let overview = self.diagnostic_overview;
        let has_nonzero_fact = overview.schema_version != 0
            || overview.maximum_resources != 0
            || overview.telemetry_request_bytes != 0
            || overview.telemetry_event_bytes != 0
            || overview.nominal_period_micros != 0
            || overview.maximum_age_micros != 0
            || !overview.resources.is_empty();
        let Some(support) = overview.support else {
            return if has_nonzero_fact {
                Err(BoardError::IncompleteDiagnosticOverview)
            } else {
                Ok(())
            };
        };
        if overview.schema_version == 0
            || overview.maximum_resources == 0
            || usize::from(overview.maximum_resources) < overview.resources.len()
            || overview.telemetry_request_bytes == 0
            || overview.telemetry_event_bytes == 0
            || overview.nominal_period_micros == 0
            || overview.maximum_age_micros < overview.nominal_period_micros
            || overview.resources.is_empty()
        {
            return Err(BoardError::IncompleteDiagnosticOverview);
        }
        for (index, resource) in overview.resources.iter().copied().enumerate() {
            let physical = self.resource(resource.resource).ok_or(
                BoardError::DiagnosticOverviewMissingResource {
                    index,
                    resource: resource.resource,
                },
            )?;
            if resource.support < support
                || physical.owner != OwnerDomain::Realtime
                || physical.hazardous_output
                || physical.safe_value != SafeValue::HighImpedance
                || !matches!(
                    resource.resource,
                    ResourceId::Gpio(_) | ResourceId::SafetyInput(_)
                )
                || resource.observation != DiagnosticObservationKind::StableBooleanInput
            {
                return Err(BoardError::InvalidDiagnosticOverviewResource {
                    index,
                    resource: resource.resource,
                });
            }
            if index != 0 && overview.resources[index - 1].resource >= resource.resource {
                return Err(BoardError::NoncanonicalDiagnosticOverviewResource {
                    index,
                    resource: resource.resource,
                });
            }
        }
        Ok(())
    }

    fn validate_digital_capture(&self) -> Result<(), BoardError> {
        let capture = self.digital_capture;
        let has_nonzero_fact = capture.schema_version != 0
            || capture.configure_flags.0 != 0
            || capture.trigger_kinds.0 != 0
            || capture.maximum_channels != 0
            || capture.maximum_transitions != 0
            || capture.configure_bytes != 0
            || capture.record_bytes != 0
            || capture.maximum_chunk_bytes != 0
            || capture.maximum_pretrigger_micros != 0
            || capture.maximum_duration_micros != 0
            || capture.arm_horizon_micros != 0
            || !capture.resources.is_empty();
        let Some(support) = capture.support else {
            return if has_nonzero_fact {
                Err(BoardError::IncompleteDigitalCapture)
            } else {
                Ok(())
            };
        };
        let flags = capture.configure_flags;
        let triggers = capture.trigger_kinds;
        if capture.schema_version == 0
            || flags.0 & !DigitalCaptureConfigureFlags::KNOWN != 0
            || !flags.contains(DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS)
            || triggers.0 == 0
            || triggers.0 & !DigitalCaptureTriggerSet::KNOWN != 0
            || capture.maximum_channels == 0
            || usize::from(capture.maximum_channels) < capture.resources.len()
            || capture.maximum_transitions == 0
            || capture.configure_bytes == 0
            || capture.record_bytes == 0
            || capture.maximum_chunk_bytes == 0
            || capture.maximum_chunk_bytes > capture.record_bytes
            || capture.maximum_pretrigger_micros > capture.maximum_duration_micros
            || capture.maximum_duration_micros == 0
            || capture.arm_horizon_micros == 0
            || capture.resources.is_empty()
        {
            return Err(BoardError::IncompleteDigitalCapture);
        }
        let mut has_software_source = false;
        for (index, resource) in capture.resources.iter().copied().enumerate() {
            let physical = self.resource(resource.resource).ok_or(
                BoardError::DigitalCaptureMissingResource {
                    index,
                    resource: resource.resource,
                },
            )?;
            has_software_source |= resource.source == DigitalCaptureSourceKind::Software;
            if resource.support < support
                || physical.owner != OwnerDomain::Realtime
                || physical.hazardous_output
                || physical.safe_value != SafeValue::HighImpedance
                || !matches!(
                    resource.resource,
                    ResourceId::Gpio(_) | ResourceId::SafetyInput(_)
                )
            {
                return Err(BoardError::InvalidDigitalCaptureResource {
                    index,
                    resource: resource.resource,
                });
            }
            if index != 0 && capture.resources[index - 1].resource >= resource.resource {
                return Err(BoardError::NoncanonicalDigitalCaptureResource {
                    index,
                    resource: resource.resource,
                });
            }
        }
        if flags.contains(DigitalCaptureConfigureFlags::ALLOW_SOFTWARE) != has_software_source {
            return Err(BoardError::IncompleteDigitalCapture);
        }
        Ok(())
    }
}

const fn bus_kind_matches(kind: BusKind, resource: ResourceId) -> bool {
    matches!(
        (kind, resource),
        (BusKind::I2c, ResourceId::I2c(_))
            | (BusKind::Spi, ResourceId::Spi(_))
            | (BusKind::Uart, ResourceId::Uart(_))
    )
}

const fn route_matches_bus(route: DeviceRoute, kind: BusKind) -> bool {
    matches!(
        (route, kind),
        (DeviceRoute::I2cAddress(0..=0x7f), BusKind::I2c)
            | (DeviceRoute::SpiChipSelect(_), BusKind::Spi)
            | (DeviceRoute::Uart, BusKind::Uart)
    )
}

/// Board-package validation failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BoardError {
    /// Stable board ID was empty.
    MissingId,
    /// Revision was empty.
    MissingRevision,
    /// Architecture requires two application cores.
    InsufficientCores {
        /// Core count advertised by the package.
        found: u8,
    },
    /// Two physical facts claimed the same typed resource.
    DuplicateResource {
        /// Conflicting resource.
        id: ResourceId,
    },
    /// A hazardous output was assigned to the service executor.
    HazardOwnedByService {
        /// Misassigned resource.
        id: ResourceId,
    },
    /// A hazardous output lacked a defined safe state.
    HazardMissingSafeValue {
        /// Incomplete resource.
        id: ResourceId,
    },
    /// Flash or internal-SRAM size was not established.
    MissingMemoryFacts,
    /// Real-time state may never be placed in PSRAM.
    RealtimePsramForbidden,
    /// Service/realtime cores overlap or exceed the available core count.
    InvalidCoreAssignment {
        /// Selected service core.
        service: u8,
        /// Selected real-time core.
        realtime: u8,
        /// Available application cores.
        available: u8,
    },
    /// Fixed graph executor omitted a nonzero format, capacity, arena, or opcode palette.
    IncompleteGraphExecutor,
    /// One graph opcode used zero, duplicate/out-of-order, or inconsistent resource facts.
    InvalidGraphOpcode {
        /// Opcode palette index.
        index: usize,
    },
    /// A graph resource referenced no typed board resource.
    GraphMissingResource {
        /// Graph resource palette index.
        index: usize,
        /// Missing typed resource.
        resource: ResourceId,
    },
    /// A graph resource was hazardous, misowned, or incompatible with its opcode class.
    InvalidGraphResource {
        /// Graph resource palette index.
        index: usize,
        /// Rejected resource.
        resource: ResourceId,
    },
    /// One class exposed the same physical resource more than once.
    DuplicateGraphResource {
        /// Duplicate resource.
        resource: ResourceId,
    },
    /// Passive overview support and its fixed budgets/timing were inconsistent.
    IncompleteDiagnosticOverview,
    /// A passive overview record referenced no typed board resource.
    DiagnosticOverviewMissingResource {
        /// Diagnostic observation palette index.
        index: usize,
        /// Missing typed resource.
        resource: ResourceId,
    },
    /// A passive overview resource was hazardous, misowned, or semantically invalid.
    InvalidDiagnosticOverviewResource {
        /// Diagnostic observation palette index.
        index: usize,
        /// Rejected resource.
        resource: ResourceId,
    },
    /// Passive overview resources were duplicated or not strictly ordered.
    NoncanonicalDiagnosticOverviewResource {
        /// Diagnostic observation palette index.
        index: usize,
        /// Rejected resource.
        resource: ResourceId,
    },
    /// Digital capture support and its fixed budgets/timing were inconsistent.
    IncompleteDigitalCapture,
    /// A digital-capture record referenced no typed board resource.
    DigitalCaptureMissingResource {
        /// Digital capture palette index.
        index: usize,
        /// Missing typed resource.
        resource: ResourceId,
    },
    /// A digital-capture resource was hazardous, misowned, or semantically invalid.
    InvalidDigitalCaptureResource {
        /// Digital capture palette index.
        index: usize,
        /// Rejected resource.
        resource: ResourceId,
    },
    /// Digital-capture resources were duplicated or not strictly ordered.
    NoncanonicalDigitalCaptureResource {
        /// Digital capture palette index.
        index: usize,
        /// Rejected resource.
        resource: ResourceId,
    },
    /// Alias was empty.
    EmptyAlias {
        /// Alias table index.
        index: usize,
    },
    /// Alias references a resource absent from the package.
    AliasMissingResource {
        /// Alias table index.
        index: usize,
        /// Missing typed resource.
        resource: ResourceId,
    },
    /// Two aliases use the same stable name.
    DuplicateAlias {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// Bus controller or one of its pins is absent.
    BusMissingResource {
        /// Bus table index.
        index: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Bus kind does not match its typed controller ID.
    BusKindMismatch {
        /// Bus table index.
        index: usize,
        /// Mismatched controller.
        resource: ResourceId,
    },
    /// Bus lacks pins or a nonzero admitted rate.
    IncompleteBus {
        /// Bus table index.
        index: usize,
    },
    /// A referenced resource belongs to another executor domain.
    OwnershipMismatch {
        /// Misowned resource.
        resource: ResourceId,
    },
    /// Device is absent from the flat resource inventory.
    DeviceMissingResource {
        /// Device table index.
        index: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Device table entry did not use a device/storage namespace.
    InvalidDeviceResource {
        /// Device table index.
        index: usize,
        /// Invalid resource.
        resource: ResourceId,
    },
    /// Device references a bus absent from the bus table.
    DeviceMissingBus {
        /// Device table index.
        index: usize,
        /// Missing bus.
        bus: ResourceId,
    },
    /// Device route/address is incompatible with its bus.
    DeviceRouteMismatch {
        /// Device table index.
        index: usize,
        /// Associated or expected bus.
        bus: ResourceId,
    },
    /// Flash region lacked a name or nonzero length.
    IncompleteFlashRegion {
        /// Region table index.
        index: usize,
    },
    /// Flash offset/length overflowed or exceeded fitted flash.
    FlashRegionOutsideDevice {
        /// Region table index.
        index: usize,
    },
    /// Two flash regions used the same stable name.
    DuplicateFlashRegionName {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// Two flash intervals overlap.
    OverlappingFlashRegions {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// An armable image allowed a flash write during motion/torque.
    ArmableWritableFlashRegion {
        /// Region table index.
        index: usize,
    },
    /// Clock lacked a name or nonzero nominal frequency.
    IncompleteClock {
        /// Clock table index.
        index: usize,
    },
    /// Two clock facts used the same stable name.
    DuplicateClock {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// A zero-error clock claim lacked bench-or-better evidence.
    UnsubstantiatedExactClock {
        /// Clock table index.
        index: usize,
    },
    /// Electrical rule lacked an ID, note, or governed resource.
    IncompleteElectricalConstraint {
        /// Constraint table index.
        index: usize,
    },
    /// Electrical rule referenced an absent resource.
    ConstraintMissingResource {
        /// Constraint table index.
        index: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Two electrical rules used the same stable ID.
    DuplicateElectricalConstraint {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// Interrupt source was absent from the board package.
    InterruptMissingResource {
        /// Interrupt table index.
        index: usize,
        /// Missing source.
        resource: ResourceId,
    },
    /// One physical source was registered as two interrupt routes.
    DuplicateInterrupt {
        /// Duplicate source.
        resource: ResourceId,
    },
    /// A present qualified latency must be greater than zero.
    InvalidInterruptLatency {
        /// Interrupt table index.
        index: usize,
    },
    /// Safe image contains values outside its declared routed mask.
    SafeImageOutsideMask {
        /// Shift engine.
        engine: u8,
    },
    /// Safe image references no declared I²S engine.
    SafeImageMissingEngine {
        /// Missing shift engine.
        engine: u8,
    },
    /// Two safe images claim one engine.
    DuplicateSafeImage {
        /// Duplicate shift engine.
        engine: u8,
    },
    /// A hazardous shifted bit is not covered by a complete safe image.
    SafeImageMissingBit {
        /// Shift engine.
        engine: u8,
        /// Missing bit.
        bit: u8,
    },
    /// An armable board claimed a safe image without bench polarity evidence.
    UnverifiedSafeImage {
        /// Unverified shift engine.
        engine: u8,
    },
    /// Photograph record lacked content, dimensions, license, digest, or hotspots.
    IncompleteVisual {
        /// Visual table index.
        visual: usize,
    },
    /// Two visual records used the same stable view ID.
    DuplicateVisual {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// Hotspot lacked an ID or at least three polygon vertices.
    IncompleteHotspot {
        /// Visual table index.
        visual: usize,
        /// Hotspot table index.
        hotspot: usize,
    },
    /// Hotspot references a resource absent from the package.
    HotspotMissingResource {
        /// Visual table index.
        visual: usize,
        /// Hotspot table index.
        hotspot: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Normalized hotspot point exceeded the 0–10,000 image plane.
    HotspotOutsideImage {
        /// Visual table index.
        visual: usize,
        /// Hotspot table index.
        hotspot: usize,
    },
    /// Two hotspots in one view used the same ID.
    DuplicateHotspot {
        /// Visual table index.
        visual: usize,
        /// Earlier hotspot index.
        first: usize,
        /// Conflicting hotspot index.
        second: usize,
    },
    /// HIL requirement lacked a stable ID.
    IncompleteHilRequirement {
        /// Requirement table index.
        index: usize,
    },
    /// HIL requirement references an absent resource.
    HilMissingResource {
        /// Requirement table index.
        index: usize,
        /// Missing resource.
        resource: ResourceId,
    },
    /// Two HIL requirements used the same stable ID.
    DuplicateHilRequirement {
        /// Earlier table index.
        first: usize,
        /// Conflicting table index.
        second: usize,
    },
    /// Armable packages require a canonical nonzero capability digest.
    ArmableWithoutCapabilityDigest,
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: ResourceDescriptor = ResourceDescriptor {
        id: ResourceId::I2sOut { engine: 0, bit: 1 },
        owner: OwnerDomain::Realtime,
        safe_value: SafeValue::EngineImage,
        hazardous_output: true,
    };
    const TEST_GRAPH_OPCODES: &[GraphOpcodeDescriptor] = &[GraphOpcodeDescriptor {
        opcode: 1,
        domain: OwnerDomain::Service,
        support: SupportLevel::Compiles,
        resource_class: None,
        resource_access: None,
    }];
    const TEST_GRAPH: GraphExecutorDescriptor<'static> = GraphExecutorDescriptor {
        ir_version: 2,
        package_bytes: 4_096,
        maximum_nodes: 32,
        maximum_channels: 64,
        maximum_queue_items: 4_096,
        service_state_bytes: 2_048,
        realtime_state_bytes: 2_048,
        service_channel_bytes: 4_096,
        realtime_channel_bytes: 4_096,
        bridge_channel_bytes: 4_096,
        support: SupportLevel::Compiles,
        opcodes: TEST_GRAPH_OPCODES,
        resources: &[],
    };

    fn board(resources: &[ResourceDescriptor]) -> BoardDescriptor<'_> {
        BoardDescriptor {
            id: "mks-tinybee-v1",
            revision: "1.x",
            chip: Chip::Esp32,
            application_cores: 2,
            qualification: Qualification::Described,
            capability_digest: Digest::ZERO,
            resources,
        }
    }

    #[test]
    fn valid_dual_core_board_is_accepted() {
        assert_eq!(board(&[STEP]).validate(), Ok(()));
    }

    #[test]
    fn single_core_board_is_rejected() {
        let mut descriptor = board(&[]);
        descriptor.application_cores = 1;
        assert_eq!(
            descriptor.validate(),
            Err(BoardError::InsufficientCores { found: 1 })
        );
    }

    #[test]
    fn shifted_output_and_gpio_are_distinct_namespaces() {
        let resources = [
            STEP,
            ResourceDescriptor {
                id: ResourceId::Gpio(1),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::Low,
                hazardous_output: false,
            },
        ];
        assert_eq!(board(&resources).validate(), Ok(()));
    }

    #[test]
    fn duplicate_and_service_owned_hazards_are_rejected() {
        let duplicate = [STEP, STEP];
        assert_eq!(
            board(&duplicate).validate(),
            Err(BoardError::DuplicateResource { id: STEP.id })
        );

        let service_hazard = [ResourceDescriptor {
            owner: OwnerDomain::Service,
            ..STEP
        }];
        assert_eq!(
            board(&service_hazard).validate(),
            Err(BoardError::HazardOwnedByService { id: STEP.id })
        );
    }

    fn package<'a>(
        resources: &'a [ResourceDescriptor],
        aliases: &'a [AliasDescriptor<'a>],
        buses: &'a [BusDescriptor<'a>],
        devices: &'a [DeviceDescriptor<'a>],
        safe_output_images: &'a [SafeOutputImage],
    ) -> BoardPackage<'a> {
        BoardPackage {
            board: board(resources),
            memory: MemoryDescriptor {
                flash_bytes: 8 * 1_024 * 1_024,
                internal_sram_bytes: 520 * 1_024,
                psram_bytes: 0,
                realtime_psram_allowed: false,
            },
            cores: CoreAssignment {
                service_core: 0,
                realtime_core: 1,
            },
            graph: TEST_GRAPH,
            diagnostic_overview: DiagnosticOverviewDescriptor::NONE,
            digital_capture: DigitalCaptureDescriptor::NONE,
            aliases,
            buses,
            devices,
            flash_regions: &[],
            clocks: &[],
            electrical_constraints: &[],
            interrupts: &[],
            safe_output_images,
            visuals: &[],
            hil_requirements: &[],
            armable: false,
        }
    }

    #[test]
    fn complete_shift_image_and_typed_bus_topology_validate() {
        let resources = [
            ResourceDescriptor {
                id: ResourceId::I2s(0),
                owner: OwnerDomain::Realtime,
                safe_value: SafeValue::EngineImage,
                hazardous_output: false,
            },
            STEP,
            ResourceDescriptor {
                id: ResourceId::Spi(2),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Gpio(18),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::Low,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Gpio(5),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::High,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Storage(0),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
        ];
        let aliases = [AliasDescriptor {
            name: "io129",
            resource: STEP.id,
        }];
        let bus_pins = [ResourceId::Gpio(18)];
        let buses = [BusDescriptor {
            resource: ResourceId::Spi(2),
            kind: BusKind::Spi,
            owner: OwnerDomain::Service,
            pins: &bus_pins,
            maximum_frequency_hz: 20_000_000,
        }];
        let devices = [DeviceDescriptor {
            resource: ResourceId::Storage(0),
            owner: OwnerDomain::Service,
            bus: Some(ResourceId::Spi(2)),
            route: DeviceRoute::SpiChipSelect(ResourceId::Gpio(5)),
            auxiliary_resources: &[],
            support: SupportLevel::Described,
        }];
        let images = [SafeOutputImage {
            engine: 0,
            defined_mask: 1 << 1,
            safe_bits: 0,
            bench_verified: false,
        }];

        assert_eq!(
            package(&resources, &aliases, &buses, &devices, &images).validate(),
            Ok(())
        );
    }

    #[test]
    fn graph_resource_authority_is_exact_and_read_only() {
        let input = ResourceDescriptor {
            id: ResourceId::Gpio(33),
            owner: OwnerDomain::Realtime,
            safe_value: SafeValue::HighImpedance,
            hazardous_output: false,
        };
        let opcodes = [GraphOpcodeDescriptor {
            opcode: 4,
            domain: OwnerDomain::Realtime,
            support: SupportLevel::Compiles,
            resource_class: Some(GraphResourceClass::new(1)),
            resource_access: Some(GraphResourceAccess::StableBooleanInput),
        }];
        let admitted = [GraphResourceDescriptor {
            resource: input.id,
            class: GraphResourceClass::new(1),
            access: GraphResourceAccess::StableBooleanInput,
            support: SupportLevel::Compiles,
        }];
        let graph = GraphExecutorDescriptor {
            opcodes: &opcodes,
            resources: &admitted,
            ..TEST_GRAPH
        };
        let resources = [input];
        let mut valid = package(&resources, &[], &[], &[], &[]);
        valid.graph = graph;
        assert_eq!(valid.validate(), Ok(()));

        let missing_resources = [GraphResourceDescriptor {
            resource: ResourceId::Gpio(32),
            ..admitted[0]
        }];
        let mut missing = valid;
        missing.graph.resources = &missing_resources;
        assert_eq!(
            missing.validate(),
            Err(BoardError::GraphMissingResource {
                index: 0,
                resource: ResourceId::Gpio(32),
            })
        );

        let unsafe_physical = [ResourceDescriptor {
            hazardous_output: true,
            ..input
        }];
        let mut unsafe_package = package(&unsafe_physical, &[], &[], &[], &[]);
        unsafe_package.graph = graph;
        assert_eq!(
            unsafe_package.validate(),
            Err(BoardError::InvalidGraphResource {
                index: 0,
                resource: input.id,
            })
        );

        let duplicate_resources = [admitted[0], admitted[0]];
        let mut duplicate = valid;
        duplicate.graph.resources = &duplicate_resources;
        assert_eq!(
            duplicate.validate(),
            Err(BoardError::DuplicateGraphResource { resource: input.id })
        );
    }

    #[test]
    fn passive_diagnostics_are_distinct_from_graph_authority() {
        let first = ResourceDescriptor {
            id: ResourceId::Gpio(22),
            owner: OwnerDomain::Realtime,
            safe_value: SafeValue::HighImpedance,
            hazardous_output: false,
        };
        let second = ResourceDescriptor {
            id: ResourceId::Gpio(33),
            ..first
        };
        let resources = [first, second];
        let observations = [
            DiagnosticResourceDescriptor {
                resource: first.id,
                observation: DiagnosticObservationKind::StableBooleanInput,
                support: SupportLevel::Compiles,
            },
            DiagnosticResourceDescriptor {
                resource: second.id,
                observation: DiagnosticObservationKind::StableBooleanInput,
                support: SupportLevel::Bench,
            },
        ];
        let mut valid = package(&resources, &[], &[], &[], &[]);
        valid.diagnostic_overview = DiagnosticOverviewDescriptor {
            schema_version: 1,
            support: Some(SupportLevel::Compiles),
            maximum_resources: 2,
            telemetry_request_bytes: 168,
            telemetry_event_bytes: 392,
            nominal_period_micros: 100_000,
            maximum_age_micros: 500_000,
            resources: &observations,
        };
        assert!(valid.graph.resources.is_empty());
        assert_eq!(valid.validate(), Ok(()));

        let mut unsupported_with_budget = valid;
        unsupported_with_budget.diagnostic_overview.support = None;
        assert_eq!(
            unsupported_with_budget.validate(),
            Err(BoardError::IncompleteDiagnosticOverview)
        );

        let reversed = [observations[1], observations[0]];
        let mut noncanonical = valid;
        noncanonical.diagnostic_overview.resources = &reversed;
        assert_eq!(
            noncanonical.validate(),
            Err(BoardError::NoncanonicalDiagnosticOverviewResource {
                index: 1,
                resource: first.id,
            })
        );

        let hazardous = [ResourceDescriptor {
            hazardous_output: true,
            ..first
        }];
        let one_observation = [observations[0]];
        let mut unsafe_overview = package(&hazardous, &[], &[], &[], &[]);
        unsafe_overview.diagnostic_overview = DiagnosticOverviewDescriptor {
            maximum_resources: 1,
            resources: &one_observation,
            ..valid.diagnostic_overview
        };
        assert_eq!(
            unsafe_overview.validate(),
            Err(BoardError::InvalidDiagnosticOverviewResource {
                index: 0,
                resource: first.id,
            })
        );
    }

    #[test]
    fn digital_capture_is_explicit_bounded_and_distinct_from_other_authority() {
        let first = ResourceDescriptor {
            id: ResourceId::Gpio(22),
            owner: OwnerDomain::Realtime,
            safe_value: SafeValue::HighImpedance,
            hazardous_output: false,
        };
        let second = ResourceDescriptor {
            id: ResourceId::Gpio(33),
            ..first
        };
        let resources = [first, second];
        let channels = [
            DigitalCaptureResourceDescriptor {
                resource: first.id,
                source: DigitalCaptureSourceKind::Simulated,
                support: SupportLevel::Compiles,
            },
            DigitalCaptureResourceDescriptor {
                resource: second.id,
                source: DigitalCaptureSourceKind::Simulated,
                support: SupportLevel::Bench,
            },
        ];
        let mut valid = package(&resources, &[], &[], &[], &[]);
        valid.digital_capture = DigitalCaptureDescriptor {
            schema_version: 1,
            support: Some(SupportLevel::Compiles),
            configure_flags: DigitalCaptureConfigureFlags(
                DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS,
            ),
            trigger_kinds: DigitalCaptureTriggerSet(DigitalCaptureTriggerSet::IMMEDIATE),
            maximum_channels: 2,
            maximum_transitions: 16,
            configure_bytes: 200,
            record_bytes: 1_024,
            maximum_chunk_bytes: 128,
            maximum_pretrigger_micros: 0,
            maximum_duration_micros: 2_000_000,
            arm_horizon_micros: 30_000_000,
            resources: &channels,
        };
        assert!(valid.graph.resources.is_empty());
        assert_eq!(
            valid.diagnostic_overview,
            DiagnosticOverviewDescriptor::NONE
        );
        assert_eq!(valid.validate(), Ok(()));

        let mut absent_with_budget = valid;
        absent_with_budget.digital_capture.support = None;
        assert_eq!(
            absent_with_budget.validate(),
            Err(BoardError::IncompleteDigitalCapture)
        );

        let reversed = [channels[1], channels[0]];
        let mut noncanonical = valid;
        noncanonical.digital_capture.resources = &reversed;
        assert_eq!(
            noncanonical.validate(),
            Err(BoardError::NoncanonicalDigitalCaptureResource {
                index: 1,
                resource: first.id,
            })
        );

        let mut unsupported_software_flag = valid;
        unsupported_software_flag.digital_capture.configure_flags = DigitalCaptureConfigureFlags(
            DigitalCaptureConfigureFlags::EDGE_TIMESTAMPS
                | DigitalCaptureConfigureFlags::ALLOW_SOFTWARE,
        );
        assert_eq!(
            unsupported_software_flag.validate(),
            Err(BoardError::IncompleteDigitalCapture)
        );

        let hazardous = [ResourceDescriptor {
            hazardous_output: true,
            ..first
        }];
        let one_channel = [channels[0]];
        let mut unsafe_capture = package(&hazardous, &[], &[], &[], &[]);
        unsafe_capture.digital_capture = DigitalCaptureDescriptor {
            maximum_channels: 1,
            resources: &one_channel,
            ..valid.digital_capture
        };
        assert_eq!(
            unsafe_capture.validate(),
            Err(BoardError::InvalidDigitalCaptureResource {
                index: 0,
                resource: first.id,
            })
        );
    }

    #[test]
    fn hazardous_shift_bit_requires_safe_image_coverage() {
        let resources = [
            ResourceDescriptor {
                id: ResourceId::I2s(0),
                owner: OwnerDomain::Realtime,
                safe_value: SafeValue::EngineImage,
                hazardous_output: false,
            },
            STEP,
        ];
        let images = [SafeOutputImage {
            engine: 0,
            defined_mask: 1,
            safe_bits: 0,
            bench_verified: false,
        }];

        assert_eq!(
            package(&resources, &[], &[], &[], &images).validate(),
            Err(BoardError::SafeImageMissingBit { engine: 0, bit: 1 })
        );
    }

    #[test]
    fn duplicate_alias_and_cross_domain_device_bus_reject_atomically() {
        let resources = [
            ResourceDescriptor {
                id: ResourceId::I2c(0),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Gpio(13),
                owner: OwnerDomain::Service,
                safe_value: SafeValue::HighImpedance,
                hazardous_output: false,
            },
            ResourceDescriptor {
                id: ResourceId::Device(0),
                owner: OwnerDomain::Realtime,
                safe_value: SafeValue::NotApplicable,
                hazardous_output: false,
            },
        ];
        let duplicate_aliases = [
            AliasDescriptor {
                name: "sda",
                resource: ResourceId::Gpio(13),
            },
            AliasDescriptor {
                name: "sda",
                resource: ResourceId::I2c(0),
            },
        ];
        assert_eq!(
            package(&resources, &duplicate_aliases, &[], &[], &[]).validate(),
            Err(BoardError::DuplicateAlias {
                first: 0,
                second: 1,
            })
        );

        let bus_pins = [ResourceId::Gpio(13)];
        let buses = [BusDescriptor {
            resource: ResourceId::I2c(0),
            kind: BusKind::I2c,
            owner: OwnerDomain::Service,
            pins: &bus_pins,
            maximum_frequency_hz: 400_000,
        }];
        let devices = [DeviceDescriptor {
            resource: ResourceId::Device(0),
            owner: OwnerDomain::Realtime,
            bus: Some(ResourceId::I2c(0)),
            route: DeviceRoute::I2cAddress(0x34),
            auxiliary_resources: &[],
            support: SupportLevel::Compiles,
        }];
        assert_eq!(
            package(&resources, &[], &buses, &devices, &[]).validate(),
            Err(BoardError::DeviceRouteMismatch {
                index: 0,
                bus: ResourceId::I2c(0),
            })
        );
    }

    #[test]
    fn device_auxiliary_and_flash_layout_are_validated() {
        let resources = [ResourceDescriptor {
            id: ResourceId::Device(0),
            owner: OwnerDomain::Service,
            safe_value: SafeValue::NotApplicable,
            hazardous_output: false,
        }];
        let auxiliary = [ResourceId::Gpio(4)];
        let devices = [DeviceDescriptor {
            resource: ResourceId::Device(0),
            owner: OwnerDomain::Service,
            bus: None,
            route: DeviceRoute::Dedicated,
            auxiliary_resources: &auxiliary,
            support: SupportLevel::Described,
        }];
        assert_eq!(
            package(&resources, &[], &[], &devices, &[]).validate(),
            Err(BoardError::DeviceMissingResource {
                index: 0,
                resource: ResourceId::Gpio(4),
            })
        );

        let regions = [
            FlashRegionDescriptor {
                name: "application",
                offset: 0x10_000,
                length: 0x20_000,
                kind: FlashRegionKind::Application,
                writable_while_armed: false,
                support: SupportLevel::Described,
            },
            FlashRegionDescriptor {
                name: "configuration",
                offset: 0x20_000,
                length: 0x10_000,
                kind: FlashRegionKind::Configuration,
                writable_while_armed: false,
                support: SupportLevel::Described,
            },
        ];
        let mut overlapping = package(&[], &[], &[], &[], &[]);
        overlapping.flash_regions = &regions;
        assert_eq!(
            overlapping.validate(),
            Err(BoardError::OverlappingFlashRegions {
                first: 0,
                second: 1,
            })
        );

        let writable = [FlashRegionDescriptor {
            name: "configuration",
            offset: 0x10_000,
            length: 0x10_000,
            kind: FlashRegionKind::Configuration,
            writable_while_armed: true,
            support: SupportLevel::Described,
        }];
        let mut armable = package(&[], &[], &[], &[], &[]);
        armable.flash_regions = &writable;
        armable.armable = true;
        assert_eq!(
            armable.validate(),
            Err(BoardError::ArmableWritableFlashRegion { index: 0 })
        );
    }

    #[test]
    fn clocks_constraints_and_interrupts_require_admitted_facts() {
        let exact_clock = [ClockDescriptor {
            name: "cpu",
            source: ClockSource::Pll,
            nominal_hz: 240_000_000,
            maximum_error_ppm: Some(0),
            domain: ClockDomain::Chip,
            support: SupportLevel::Compiles,
        }];
        let mut clock_package = package(&[], &[], &[], &[], &[]);
        clock_package.clocks = &exact_clock;
        assert_eq!(
            clock_package.validate(),
            Err(BoardError::UnsubstantiatedExactClock { index: 0 })
        );

        let resources = [ResourceDescriptor {
            id: ResourceId::Gpio(4),
            owner: OwnerDomain::Service,
            safe_value: SafeValue::HighImpedance,
            hazardous_output: false,
        }];
        let governed = [ResourceId::Gpio(5)];
        let constraints = [ElectricalConstraintDescriptor {
            id: "logic-level",
            kind: ElectricalConstraintKind::Logic3v3,
            resources: &governed,
            note: "fixture logic is limited to 3.3 V",
            support: SupportLevel::Described,
        }];
        let mut constraint_package = package(&resources, &[], &[], &[], &[]);
        constraint_package.electrical_constraints = &constraints;
        assert_eq!(
            constraint_package.validate(),
            Err(BoardError::ConstraintMissingResource {
                index: 0,
                resource: ResourceId::Gpio(5),
            })
        );

        let interrupts = [InterruptDescriptor {
            source: ResourceId::Gpio(4),
            owner: OwnerDomain::Service,
            trigger: InterruptTrigger::Rising,
            maximum_latency_cycles: Some(0),
            support: SupportLevel::Described,
        }];
        let mut interrupt_package = package(&resources, &[], &[], &[], &[]);
        interrupt_package.interrupts = &interrupts;
        assert_eq!(
            interrupt_package.validate(),
            Err(BoardError::InvalidInterruptLatency { index: 0 })
        );
    }

    #[test]
    fn visual_hotspots_and_hil_references_are_bounded() {
        let resources = [ResourceDescriptor {
            id: ResourceId::Gpio(4),
            owner: OwnerDomain::Service,
            safe_value: SafeValue::HighImpedance,
            hazardous_output: false,
        }];
        let polygon = [
            NormalizedPoint { x: 0, y: 0 },
            NormalizedPoint { x: 10_001, y: 0 },
            NormalizedPoint { x: 0, y: 1 },
        ];
        let hotspots = [HotspotDescriptor {
            id: "gpio4",
            resource: ResourceId::Gpio(4),
            polygon: &polygon,
        }];
        let visuals = [BoardVisualDescriptor {
            id: "top",
            asset_path: "assets/fixture.webp",
            media_type: "image/webp",
            pixel_width: 640,
            pixel_height: 480,
            asset_digest: Digest([1; 32]),
            license: "CC-BY-4.0",
            attribution: "fixture photographer",
            hotspots: &hotspots,
        }];
        let mut visual_package = package(&resources, &[], &[], &[], &[]);
        visual_package.visuals = &visuals;
        assert_eq!(
            visual_package.validate(),
            Err(BoardError::HotspotOutsideImage {
                visual: 0,
                hotspot: 0,
            })
        );

        let missing = [ResourceId::Gpio(5)];
        let requirements = [HilRequirement {
            id: "fixture-identity",
            kind: HilKind::BoardIdentity,
            resources: &missing,
            required_for: Qualification::Bench,
        }];
        let mut hil_package = package(&resources, &[], &[], &[], &[]);
        hil_package.hil_requirements = &requirements;
        assert_eq!(
            hil_package.validate(),
            Err(BoardError::HilMissingResource {
                index: 0,
                resource: ResourceId::Gpio(5),
            })
        );
    }
}
