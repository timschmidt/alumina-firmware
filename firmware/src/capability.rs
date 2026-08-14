//! Selected-board adapter for the shared canonical capability service.

use alumina_protocol::DeviceCycle;
use alumina_service::capability::CapabilityDocumentService;
use alumina_service::{ServiceRequest, ServiceResponse};

use crate::hardware::selected;

/// Stateless dispatcher for one compile-time selected immutable board document.
pub struct CapabilityService;

impl CapabilityService {
    /// Whether a valid universal request selects the capability family.
    pub fn handles(request: &ServiceRequest) -> bool {
        CapabilityDocumentService::handles(request)
    }

    /// Returns one authenticated range or a correlated fail-closed status.
    pub fn dispatch(request: &ServiceRequest, now: DeviceCycle) -> ServiceResponse {
        CapabilityDocumentService::dispatch(&selected::PACKAGE, request, now)
    }
}
