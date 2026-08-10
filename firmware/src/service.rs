//! Cancellation-safe, fixed-memory HTTP-to-service request bridge.

use core::sync::atomic::{AtomicU32, Ordering};

pub use alumina_service::StorageServiceState;
use alumina_service::{ServiceRequest, ServiceResponse};
use embassy_sync::blocking_mutex::raw::NoopRawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::signal::Signal;
use static_cell::StaticCell;

static SERVICE_BRIDGE: StaticCell<ServiceBridge> = StaticCell::new();

/// Initializes the single bridge before any core-0 HTTP/service task is spawned.
pub fn init_service_bridge() -> &'static ServiceBridge {
    SERVICE_BRIDGE.init(ServiceBridge::new())
}

#[derive(Debug)]
pub struct RequestEnvelope {
    transaction: u32,
    request: ServiceRequest,
}

impl RequestEnvelope {
    /// Authenticated request body without the private bridge correlation.
    pub const fn request(&self) -> &ServiceRequest {
        &self.request
    }
}

#[derive(Clone, Copy, Debug)]
struct ResponseEnvelope {
    transaction: u32,
    response: ServiceResponse,
}

/// One in-flight authenticated request and one response signal.
///
/// The transaction mutex serializes the initial mutation surface. Correlation
/// keeps cancellation safe: a late response from a timed-out handler cannot be
/// mistaken for the next request's result.
pub struct ServiceBridge {
    requests: Channel<NoopRawMutex, RequestEnvelope, 1>,
    response: Signal<NoopRawMutex, ResponseEnvelope>,
    transaction: Mutex<NoopRawMutex, ()>,
    next_transaction: AtomicU32,
}

impl ServiceBridge {
    /// Creates an empty fixed-memory bridge.
    pub const fn new() -> Self {
        Self {
            requests: Channel::new(),
            response: Signal::new(),
            transaction: Mutex::new(()),
            next_transaction: AtomicU32::new(0),
        }
    }

    /// Sends one request and waits for its exact correlated service response.
    pub async fn transact(&self, request: ServiceRequest) -> ServiceResponse {
        let _guard = self.transaction.lock().await;
        let transaction = self.next_transaction();
        self.requests
            .send(RequestEnvelope {
                transaction,
                request,
            })
            .await;
        loop {
            let response = self.response.wait().await;
            if response.transaction == transaction {
                return response.response;
            }
        }
    }

    /// Takes one queued request without blocking the service coordinator loop.
    pub fn try_receive(&self) -> Option<RequestEnvelope> {
        self.requests.try_receive().ok()
    }

    /// Publishes the result for one request previously taken by core 0.
    pub fn respond(&self, request: &RequestEnvelope, response: ServiceResponse) {
        self.response.signal(ResponseEnvelope {
            transaction: request.transaction,
            response,
        });
    }

    fn next_transaction(&self) -> u32 {
        let mut transaction = self
            .next_transaction
            .fetch_add(1, Ordering::Relaxed)
            .wrapping_add(1);
        if transaction == 0 {
            transaction = self
                .next_transaction
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
        }
        transaction
    }
}

impl Default for ServiceBridge {
    fn default() -> Self {
        Self::new()
    }
}
