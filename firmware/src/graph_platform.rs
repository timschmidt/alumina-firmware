//! Exact statically allocated graph executor selected by the board package.

use alumina_graph_ir::{
    GRAPH_IR_PACKAGE_BYTES, GRAPH_IR_VERSION, MAX_GRAPH_IR_CHANNELS, MAX_GRAPH_IR_NODES,
    MAX_GRAPH_IR_QUEUE_ITEMS,
};
use alumina_runtime::graph::{
    FixedGraphRealtimeActor, FixedGraphServiceActor, GraphRuntimeLimits, ReloadableGraphBridge,
};

use crate::hardware::selected;

// The no-PSRAM TinyBee publishes a smaller, exact graph arena so its service
// core can retain two independent browser admissions alongside the Wi-Fi
// driver. Larger boards keep the original general-purpose graph envelope.
#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
/// Core-0 graph state reservation.
pub const GRAPH_SERVICE_STATE_BYTES: usize = 1_024;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-t-deck-pro",
    feature = "board-t-lora-pager"
))]
/// Core-0 graph state reservation.
pub const GRAPH_SERVICE_STATE_BYTES: usize = 2 * 1_024;

#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
/// Core-1 graph state reservation.
pub const GRAPH_REALTIME_STATE_BYTES: usize = 1_024;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-t-deck-pro",
    feature = "board-t-lora-pager"
))]
/// Core-1 graph state reservation.
pub const GRAPH_REALTIME_STATE_BYTES: usize = 2 * 1_024;

#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
/// Core-0-local graph channel reservation.
pub const GRAPH_SERVICE_CHANNEL_BYTES: usize = 2 * 1_024;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-t-deck-pro",
    feature = "board-t-lora-pager"
))]
/// Core-0-local graph channel reservation.
pub const GRAPH_SERVICE_CHANNEL_BYTES: usize = 4 * 1_024;

#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
/// Core-1-local graph channel reservation.
pub const GRAPH_REALTIME_CHANNEL_BYTES: usize = 2 * 1_024;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-t-deck-pro",
    feature = "board-t-lora-pager"
))]
/// Core-1-local graph channel reservation.
pub const GRAPH_REALTIME_CHANNEL_BYTES: usize = 4 * 1_024;

#[cfg(any(feature = "board-mks-tinybee", feature = "board-mks-tinybee-4mb"))]
/// Cross-core graph channel reservation.
pub const GRAPH_BRIDGE_BYTES: usize = 2 * 1_024;
#[cfg(any(
    feature = "board-mks-esp32-foc-v1",
    feature = "board-t-deck-pro",
    feature = "board-t-lora-pager"
))]
/// Cross-core graph channel reservation.
pub const GRAPH_BRIDGE_BYTES: usize = 4 * 1_024;

/// Permanently allocated cross-core graph bridge used by both application cores.
pub type GraphBridge = ReloadableGraphBridge<GRAPH_BRIDGE_BYTES>;
/// Permanent core-0 graph package and fixed executor arenas.
pub type ServiceGraphActor = FixedGraphServiceActor<
    'static,
    GRAPH_SERVICE_STATE_BYTES,
    GRAPH_SERVICE_CHANNEL_BYTES,
    GRAPH_BRIDGE_BYTES,
>;
/// Permanent core-1 graph package and fixed executor arenas.
pub type RealtimeGraphActor = FixedGraphRealtimeActor<
    'static,
    GRAPH_REALTIME_STATE_BYTES,
    GRAPH_REALTIME_CHANNEL_BYTES,
    GRAPH_BRIDGE_BYTES,
>;

/// Exact board-published graph arena, opcode, and resource authority.
pub const GRAPH_RUNTIME_LIMITS: GraphRuntimeLimits = GraphRuntimeLimits::fixed_with_capabilities::<
    GRAPH_SERVICE_STATE_BYTES,
    GRAPH_REALTIME_STATE_BYTES,
    GRAPH_SERVICE_CHANNEL_BYTES,
    GRAPH_REALTIME_CHANNEL_BYTES,
    GRAPH_BRIDGE_BYTES,
>(
    selected::PACKAGE.graph.opcodes,
    selected::PACKAGE.graph.resources,
);

const _: () = {
    assert!(selected::PACKAGE.graph.ir_version == GRAPH_IR_VERSION);
    assert!(selected::PACKAGE.graph.package_bytes as usize == GRAPH_IR_PACKAGE_BYTES);
    assert!(selected::PACKAGE.graph.maximum_nodes as usize == MAX_GRAPH_IR_NODES);
    assert!(selected::PACKAGE.graph.maximum_channels as usize == MAX_GRAPH_IR_CHANNELS);
    assert!(selected::PACKAGE.graph.maximum_queue_items == MAX_GRAPH_IR_QUEUE_ITEMS);
    assert!(selected::PACKAGE.graph.service_state_bytes as usize == GRAPH_SERVICE_STATE_BYTES);
    assert!(selected::PACKAGE.graph.realtime_state_bytes as usize == GRAPH_REALTIME_STATE_BYTES);
    assert!(selected::PACKAGE.graph.service_channel_bytes as usize == GRAPH_SERVICE_CHANNEL_BYTES);
    assert!(
        selected::PACKAGE.graph.realtime_channel_bytes as usize == GRAPH_REALTIME_CHANNEL_BYTES
    );
    assert!(selected::PACKAGE.graph.bridge_channel_bytes as usize == GRAPH_BRIDGE_BYTES);
};
