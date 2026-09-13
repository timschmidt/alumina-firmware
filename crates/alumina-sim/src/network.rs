//! Deterministic AP-preserving infrastructure-WLAN provisioning fixture.

use alumina_net::NetworkSupervisor;
use alumina_net::provisioning::{
    NetworkAuthentication, NetworkFailure, NetworkJoinRequest, NetworkMutationRequest,
    NetworkScanEntry, NetworkScanRequest, NetworkScanResult, NetworkStatus, NetworkStatusFlags,
    StationLinkState,
};
use alumina_protocol::{DeviceCycle, Digest, FrameKind, Operation, StatusCode};
use alumina_service::{NativeRequest, ServiceResponse};
use alumina_storage::sha256;

const LAB_SSID: &str = "Alumina Lab";
const LAB_PASSPHRASE: &str = "alumina-lab-secret";
const LAB_BSSID: [u8; 6] = [0x02, 0xa1, 0x51, 0x00, 0x00, 0x01];
const OPEN_SSID: &str = "Open Bench";
const OPEN_BSSID: [u8; 6] = [0x02, 0xa1, 0x51, 0x00, 0x00, 0x02];

/// Deterministic native network-operation owner used by host and browser tests.
pub struct SimulatedNetworkService {
    supervisor: NetworkSupervisor,
    status: NetworkStatus,
    last_scan: Option<NetworkScanResult>,
    last_mutation_operation: Option<Operation>,
    last_mutation_digest: Digest,
}

impl SimulatedNetworkService {
    /// Boot into a protected AP with no selected infrastructure profile.
    pub fn new() -> Self {
        let mut supervisor = NetworkSupervisor::new();
        supervisor
            .begin_access_point()
            .expect("fresh simulator can begin AP startup");
        supervisor
            .access_point_ready()
            .expect("simulator AP startup completes deterministically");
        Self {
            status: NetworkStatus::provisioning(supervisor),
            supervisor,
            last_scan: None,
            last_mutation_operation: None,
            last_mutation_digest: Digest::ZERO,
        }
    }

    /// Current credential-free network state.
    pub const fn status(&self) -> NetworkStatus {
        self.status
    }

    /// Handle one native network request, or return `None` for another family.
    pub fn dispatch(
        &mut self,
        request: NativeRequest<'_>,
        now: DeviceCycle,
    ) -> Option<ServiceResponse> {
        if request.frame.kind != FrameKind::Network {
            return None;
        }
        let response = match request.message.operation {
            Operation::NetworkStatus if request.body.is_empty() => {
                native(request, now, StatusCode::Ok, &self.status.encode())
            }
            Operation::NetworkScan => {
                let Ok(scan) = NetworkScanRequest::decode(request.body) else {
                    return Some(native(request, now, StatusCode::InvalidRequest, &[]));
                };
                match self.scan(scan, now) {
                    Ok(result) => native(request, now, StatusCode::Ok, &result.encode()),
                    Err(status) => native(request, now, status, &[]),
                }
            }
            Operation::NetworkJoin => {
                let Ok(join) = NetworkJoinRequest::decode(request.body) else {
                    return Some(native(request, now, StatusCode::InvalidRequest, &[]));
                };
                let status = self.join(&join);
                native(request, now, status, &self.status.encode())
            }
            Operation::NetworkLeave => {
                let Ok(mutation) = NetworkMutationRequest::decode(request.body) else {
                    return Some(native(request, now, StatusCode::InvalidRequest, &[]));
                };
                let status = self.leave(mutation, request.body);
                native(request, now, status, &self.status.encode())
            }
            Operation::NetworkRecoverAp => {
                let Ok(mutation) = NetworkMutationRequest::decode(request.body) else {
                    return Some(native(request, now, StatusCode::InvalidRequest, &[]));
                };
                let status = self.recover(mutation, request.body);
                native(request, now, status, &self.status.encode())
            }
            _ => native(request, now, StatusCode::InvalidRequest, &[]),
        };
        Some(response)
    }

    fn scan(
        &mut self,
        request: NetworkScanRequest,
        now: DeviceCycle,
    ) -> Result<NetworkScanResult, StatusCode> {
        if let Some(previous) = self.last_scan {
            if previous.transaction_id == request.transaction_id {
                return Ok(previous);
            }
            if request.transaction_id < previous.transaction_id {
                return Err(StatusCode::Conflict);
            }
        }
        let entries = [
            NetworkScanEntry::try_new(
                LAB_SSID,
                NetworkAuthentication::Wpa2Personal,
                6,
                -36,
                LAB_BSSID,
            )
            .expect("fixed simulator WLAN is valid"),
            NetworkScanEntry::try_new(OPEN_SSID, NetworkAuthentication::Open, 11, -61, OPEN_BSSID)
                .expect("fixed simulator WLAN is valid"),
        ];
        let generation = self.status.scan_generation.wrapping_add(1).max(1);
        let result = NetworkScanResult::try_new(
            request.transaction_id,
            self.supervisor.generation(),
            generation,
            now.0,
            &entries,
            false,
        )
        .map_err(|_| StatusCode::Internal)?;
        self.last_scan = Some(result);
        self.status.scan_generation = generation;
        self.status.flags.0 &= !NetworkStatusFlags::SCAN_TRUNCATED;
        if result.truncated() {
            self.status.flags.0 |= NetworkStatusFlags::SCAN_TRUNCATED;
        }
        Ok(result)
    }

    fn join(&mut self, request: &NetworkJoinRequest) -> StatusCode {
        let body = request.encode();
        if let Some(replay) = self.reconcile(Operation::NetworkJoin, request.transaction_id, &body)
        {
            return replay;
        }
        if request.expected_generation != self.supervisor.generation() {
            return StatusCode::Conflict;
        }
        self.remember(Operation::NetworkJoin, request.transaction_id, &body);
        if self.supervisor.begin_station_join().is_err() {
            self.replace_status(
                Some(request),
                StationLinkState::Disconnected,
                NetworkFailure::Interrupted,
            );
            return StatusCode::Conflict;
        }
        self.replace_status(
            Some(request),
            StationLinkState::Associating,
            NetworkFailure::None,
        );
        let selected_lab = request.ssid() == LAB_SSID
            && request.authentication() == NetworkAuthentication::Wpa2Personal
            && request.passphrase() == LAB_PASSPHRASE
            && request.bssid().is_none_or(|value| value == LAB_BSSID)
            && request.channel().is_none_or(|value| value == 6);
        let selected_open = request.ssid() == OPEN_SSID
            && request.authentication() == NetworkAuthentication::Open
            && request.passphrase().is_empty()
            && request.bssid().is_none_or(|value| value == OPEN_BSSID)
            && request.channel().is_none_or(|value| value == 11);
        if !selected_lab && !selected_open {
            self.supervisor
                .station_join_failed()
                .expect("simulator join failure follows join start");
            self.replace_status(
                Some(request),
                StationLinkState::Disconnected,
                NetworkFailure::Association,
            );
            return StatusCode::Unauthorized;
        }
        self.supervisor
            .station_joined()
            .expect("simulator join success follows join start");
        self.replace_status(
            Some(request),
            StationLinkState::Addressed,
            NetworkFailure::None,
        );
        StatusCode::Ok
    }

    fn leave(&mut self, request: NetworkMutationRequest, body: &[u8]) -> StatusCode {
        if let Some(replay) = self.reconcile(Operation::NetworkLeave, request.transaction_id, body)
        {
            return replay;
        }
        if request.expected_generation != self.supervisor.generation() {
            return StatusCode::Conflict;
        }
        self.remember(Operation::NetworkLeave, request.transaction_id, body);
        if self.supervisor.leave_station().is_err() {
            self.replace_status(
                None,
                StationLinkState::Disconnected,
                NetworkFailure::Interrupted,
            );
            return StatusCode::Conflict;
        }
        self.replace_status(None, StationLinkState::Disconnected, NetworkFailure::None);
        StatusCode::Ok
    }

    fn recover(&mut self, request: NetworkMutationRequest, body: &[u8]) -> StatusCode {
        if let Some(replay) =
            self.reconcile(Operation::NetworkRecoverAp, request.transaction_id, body)
        {
            return replay;
        }
        if request.expected_generation != self.supervisor.generation() {
            return StatusCode::Conflict;
        }
        self.remember(Operation::NetworkRecoverAp, request.transaction_id, body);
        self.supervisor.begin_recovery();
        self.supervisor
            .begin_access_point()
            .expect("simulator recovery begins AP startup");
        self.supervisor
            .access_point_ready()
            .expect("simulator recovery restores AP");
        self.replace_status(None, StationLinkState::Disconnected, NetworkFailure::None);
        StatusCode::Ok
    }

    fn reconcile(
        &self,
        operation: Operation,
        transaction_id: u64,
        body: &[u8],
    ) -> Option<StatusCode> {
        let previous = self.status.last_transaction_id;
        if transaction_id < previous {
            return Some(StatusCode::Conflict);
        }
        if transaction_id != previous || previous == 0 {
            return None;
        }
        Some(
            if self.last_mutation_operation == Some(operation)
                && self.last_mutation_digest == sha256(body).digest
            {
                failure_status(self.status.last_failure)
            } else {
                StatusCode::Conflict
            },
        )
    }

    fn remember(&mut self, operation: Operation, transaction_id: u64, body: &[u8]) {
        self.last_mutation_operation = Some(operation);
        self.last_mutation_digest = sha256(body).digest;
        self.status.last_transaction_id = transaction_id;
    }

    fn replace_status(
        &mut self,
        selected: Option<&NetworkJoinRequest>,
        link: StationLinkState,
        failure: NetworkFailure,
    ) {
        let configured = selected.is_some();
        let associated = matches!(
            link,
            StationLinkState::Associated | StationLinkState::Addressed
        );
        let mut flags = NetworkStatusFlags::AP_EXPECTED;
        if configured {
            flags |= NetworkStatusFlags::STATION_CONFIGURED;
        }
        if associated {
            flags |= NetworkStatusFlags::STATION_ASSOCIATED;
        }
        if link == StationLinkState::Addressed {
            flags |= NetworkStatusFlags::STATION_IPV4_READY;
        }
        if failure != NetworkFailure::None {
            flags |= NetworkStatusFlags::LAST_OPERATION_FAILED;
        }
        let authentication = selected.map_or(NetworkAuthentication::Unsupported, |request| {
            request.authentication()
        });
        let channel = selected.and_then(NetworkJoinRequest::channel).unwrap_or(0);
        let bssid = selected
            .and_then(NetworkJoinRequest::bssid)
            .unwrap_or([0; 6]);
        let addressed = link == StationLinkState::Addressed;
        self.status = NetworkStatus::try_new(
            self.supervisor.phase(),
            self.supervisor.generation(),
            self.status.scan_generation,
            self.status.last_transaction_id,
            link,
            authentication,
            NetworkStatusFlags(flags),
            channel,
            if associated { -37 } else { i8::MIN },
            if addressed { 24 } else { 0 },
            if addressed { [192, 168, 1, 77] } else { [0; 4] },
            if addressed { [192, 168, 1, 1] } else { [0; 4] },
            bssid,
            failure,
            selected.map(NetworkJoinRequest::ssid),
        )
        .expect("deterministic simulator status is internally valid");
    }
}

impl Default for SimulatedNetworkService {
    fn default() -> Self {
        Self::new()
    }
}

fn native(
    request: NativeRequest<'_>,
    now: DeviceCycle,
    status: StatusCode,
    body: &[u8],
) -> ServiceResponse {
    ServiceResponse::native(request, now, status, body)
        .unwrap_or_else(|_| ServiceResponse::invalid_native())
}

fn failure_status(failure: NetworkFailure) -> StatusCode {
    match failure {
        NetworkFailure::None => StatusCode::Ok,
        NetworkFailure::Association => StatusCode::Unauthorized,
        NetworkFailure::Timeout | NetworkFailure::Dhcp => StatusCode::Deadline,
        NetworkFailure::Interrupted => StatusCode::Conflict,
        NetworkFailure::Scan | NetworkFailure::Driver | NetworkFailure::Storage => {
            StatusCode::Internal
        }
    }
}

#[cfg(test)]
mod tests {
    use alumina_protocol::{FrameHeader, MessageHeader};

    use super::*;

    fn request<'a>(
        operation: Operation,
        body: &'a [u8],
        storage: &'a mut Vec<u8>,
    ) -> NativeRequest<'a> {
        let message = MessageHeader::request(operation, 1, body.len() as u32);
        let frame = FrameHeader::new(
            FrameKind::Network,
            (MessageHeader::WIRE_LEN + body.len()) as u32,
            1,
            DeviceCycle(0),
            Digest::ZERO,
        );
        storage.extend_from_slice(&frame.encode());
        storage.extend_from_slice(&message.encode());
        storage.extend_from_slice(body);
        NativeRequest::decode(storage).unwrap()
    }

    fn response_status(response: &ServiceResponse) -> (StatusCode, Vec<u8>) {
        let bytes = response.bytes();
        let frame = FrameHeader::decode(&bytes[..FrameHeader::WIRE_LEN], 512).unwrap();
        let start = FrameHeader::WIRE_LEN;
        let end = start + MessageHeader::WIRE_LEN;
        let message =
            MessageHeader::decode_and_validate(&bytes[start..end], frame.kind, frame.payload_len)
                .unwrap();
        (message.status, bytes[end..].to_vec())
    }

    #[test]
    fn scan_wrong_password_reconcile_success_and_leave_are_deterministic() {
        let mut service = SimulatedNetworkService::new();
        let scan_body = NetworkScanRequest { transaction_id: 1 }.encode().unwrap();
        let mut bytes = Vec::new();
        let response = service
            .dispatch(
                request(Operation::NetworkScan, &scan_body, &mut bytes),
                DeviceCycle(10),
            )
            .unwrap();
        let (status, body) = response_status(&response);
        assert_eq!(status, StatusCode::Ok);
        let scan = NetworkScanResult::decode(&body).unwrap();
        assert_eq!(scan.entries().len(), 2);
        assert_eq!(scan.entries()[0].ssid(), LAB_SSID);

        let wrong = NetworkJoinRequest::try_new(
            2,
            service.status().generation,
            LAB_SSID,
            NetworkAuthentication::Wpa2Personal,
            "wrong-password",
            Some(LAB_BSSID),
            Some(6),
        )
        .unwrap();
        bytes.clear();
        let response = service
            .dispatch(
                request(Operation::NetworkJoin, &wrong.encode(), &mut bytes),
                DeviceCycle(20),
            )
            .unwrap();
        assert_eq!(response_status(&response).0, StatusCode::Unauthorized);
        assert!(
            service
                .status()
                .flags
                .contains(NetworkStatusFlags::AP_EXPECTED)
        );

        let good = NetworkJoinRequest::try_new(
            3,
            service.status().generation,
            LAB_SSID,
            NetworkAuthentication::Wpa2Personal,
            LAB_PASSPHRASE,
            Some(LAB_BSSID),
            Some(6),
        )
        .unwrap();
        let good_body = good.encode();
        bytes.clear();
        let response = service
            .dispatch(
                request(Operation::NetworkJoin, &good_body, &mut bytes),
                DeviceCycle(30),
            )
            .unwrap();
        assert_eq!(response_status(&response).0, StatusCode::Ok);
        assert_eq!(service.status().station_link, StationLinkState::Addressed);
        assert!(
            !service
                .status()
                .flags
                .contains(NetworkStatusFlags::CREDENTIALS_DURABLE)
        );

        bytes.clear();
        let replay = service
            .dispatch(
                request(Operation::NetworkJoin, &good_body, &mut bytes),
                DeviceCycle(31),
            )
            .unwrap();
        assert_eq!(response_status(&replay), response_status(&response));

        let leave = NetworkMutationRequest {
            transaction_id: 4,
            expected_generation: service.status().generation,
        }
        .encode()
        .unwrap();
        bytes.clear();
        let response = service
            .dispatch(
                request(Operation::NetworkLeave, &leave, &mut bytes),
                DeviceCycle(40),
            )
            .unwrap();
        assert_eq!(response_status(&response).0, StatusCode::Ok);
        assert_eq!(
            service.status().station_link,
            StationLinkState::Disconnected
        );
        assert!(
            service
                .status()
                .flags
                .contains(NetworkStatusFlags::AP_EXPECTED)
        );
    }
}
