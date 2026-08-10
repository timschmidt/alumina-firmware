#![no_std]
#![doc = "Bounded, portable network policy shared by firmware and simulation."]

use hmac::{Hmac, Mac};
use sha2::{Digest as _, Sha256};

type HmacSha256 = Hmac<Sha256>;

/// Maximum IEEE 802.11 SSID size in bytes.
pub const MAX_SSID_BYTES: usize = 32;
/// Minimum WPA2/WPA3 passphrase size in bytes.
pub const MIN_PASSPHRASE_BYTES: usize = 8;
/// Maximum textual WPA2/WPA3 passphrase size in bytes.
pub const MAX_PASSPHRASE_BYTES: usize = 63;
/// Default Alumina provisioning subnet gateway.
pub const PROVISIONING_ADDRESS: [u8; 4] = [192, 168, 4, 1];
/// Default prefix length for the provisioning subnet.
pub const PROVISIONING_PREFIX: u8 = 24;
/// First address offered by the bounded DHCP lease table.
pub const DHCP_RANGE_START: [u8; 4] = [192, 168, 4, 100];
/// Last address offered by the bounded DHCP lease table.
pub const DHCP_RANGE_END: [u8; 4] = [192, 168, 4, 103];
/// Boot-scoped request-authentication nonce size.
pub const AUTH_NONCE_BYTES: usize = 16;
/// HMAC-SHA-256 request proof size.
pub const AUTH_TAG_BYTES: usize = 32;
/// Exact lowercase hexadecimal request-proof header size.
pub const AUTH_TAG_HEX_BYTES: usize = AUTH_TAG_BYTES * 2;
/// Largest authenticated HTTP body admitted by the first native storage route.
///
/// This is one 56-byte frame header, one 16-byte operation header, one 52-byte
/// chunk header, and at most 1,024 bytes of content.
pub const MAX_AUTHENTICATED_BODY_BYTES: usize = 56 + 16 + 52 + 1_024;
/// Header carrying a canonical nonzero decimal request counter.
pub const AUTH_COUNTER_HEADER: &str = "X-Alumina-Counter";
/// Header carrying the exact lowercase hexadecimal HMAC-SHA-256 proof.
pub const AUTH_PROOF_HEADER: &str = "X-Alumina-Authorization";
/// Response header carrying a lowercase HMAC bound to the request counter.
pub const AUTH_RESPONSE_HEADER: &str = "X-Alumina-Response-Authorization";

const AUTH_DOMAIN: &[u8] = b"ALUMINA-HTTP-AUTH-V1\0";
const AUTH_RESPONSE_DOMAIN: &[u8] = b"ALUMINA-HTTP-RESPONSE-V1\0";
const REPLAY_WINDOW_BITS: u32 = 64;

/// Provenance of the password protecting the device access point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CredentialSource {
    /// Repository-wide development credential; never production-armable.
    DevelopmentFallback,
    /// Secret supplied to the image build outside source control.
    BuildProvisioned,
    /// Unique credential loaded from transactional device storage.
    DeviceStored,
}

impl CredentialSource {
    /// Stable human-facing label used by discovery responses.
    pub const fn label(self) -> &'static str {
        match self {
            Self::DevelopmentFallback => "development-fallback",
            Self::BuildProvisioned => "build-provisioned",
            Self::DeviceStored => "device-stored",
        }
    }

    /// Only a unique stored credential is accepted for production arming.
    pub const fn production_armable(self) -> bool {
        matches!(self, Self::DeviceStored)
    }
}

/// Complete bounded policy used to construct a protected provisioning AP.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AccessPointProfile<'a> {
    /// Network name advertised by the device.
    pub ssid: &'a str,
    /// WPA2/WPA3 passphrase. It is never returned by discovery APIs.
    pub passphrase: &'a str,
    /// Credential provenance reported without disclosing the credential.
    pub credential_source: CredentialSource,
    /// Regulatory channel selected by board/application policy.
    pub channel: u8,
    /// Maximum associated clients admitted by both Wi-Fi and DHCP policy.
    pub maximum_clients: u8,
}

impl<'a> AccessPointProfile<'a> {
    /// Creates the initial four-client, channel-six provisioning profile.
    pub const fn new(
        ssid: &'a str,
        passphrase: &'a str,
        credential_source: CredentialSource,
    ) -> Self {
        Self {
            ssid,
            passphrase,
            credential_source,
            channel: 6,
            maximum_clients: 4,
        }
    }

    /// Rejects configurations the radio would truncate or expose insecurely.
    pub fn validate(self) -> Result<(), ProfileError> {
        let ssid_len = self.ssid.len();
        if ssid_len == 0 || ssid_len > MAX_SSID_BYTES {
            return Err(ProfileError::SsidLength(ssid_len));
        }
        if self.ssid.as_bytes().contains(&0) {
            return Err(ProfileError::EmbeddedNul);
        }

        let passphrase_len = self.passphrase.len();
        if !(MIN_PASSPHRASE_BYTES..=MAX_PASSPHRASE_BYTES).contains(&passphrase_len) {
            return Err(ProfileError::PassphraseLength(passphrase_len));
        }
        if self.passphrase.as_bytes().contains(&0) {
            return Err(ProfileError::EmbeddedNul);
        }
        if !(1..=13).contains(&self.channel) {
            return Err(ProfileError::Channel(self.channel));
        }
        if !(1..=4).contains(&self.maximum_clients) {
            return Err(ProfileError::MaximumClients(self.maximum_clients));
        }
        Ok(())
    }
}

/// Invalid provisioning policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProfileError {
    /// SSID is empty or exceeds the radio limit.
    SsidLength(usize),
    /// Passphrase is outside the textual WPA limit.
    PassphraseLength(usize),
    /// A string contains a NUL that would be ambiguous at the vendor boundary.
    EmbeddedNul,
    /// Channel is outside the initial 2.4 GHz policy.
    Channel(u8),
    /// Client count is zero or exceeds the fixed DHCP table.
    MaximumClients(u8),
}

/// Fixed HTTP and socket resource policy for the initial service.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WebLimits {
    /// Simultaneous TCP handlers.
    pub connections: usize,
    /// Header workspace per connection.
    pub header_bytes: usize,
    /// Parsed headers per request.
    pub header_count: usize,
    /// RX and TX bytes reserved per connection and direction.
    pub socket_bytes: usize,
    /// Timeout on each socket operation.
    pub io_timeout_ms: u32,
    /// Timeout for an idle persistent connection.
    pub keepalive_timeout_ms: u32,
    /// Timeout for one complete handler invocation.
    pub request_timeout_ms: u32,
}

impl WebLimits {
    /// Reviewed initial limits used by the ESP adapter.
    pub const INITIAL: Self = Self {
        connections: 2,
        header_bytes: 1_024,
        header_count: 12,
        socket_bytes: 2_048,
        io_timeout_ms: 2_000,
        keepalive_timeout_ms: 5_000,
        request_timeout_ms: 3_000,
    };

    /// Rejects zero/unbounded-looking policy and inconsistent timeouts.
    pub const fn validate(self) -> Result<(), LimitError> {
        if self.connections == 0 || self.connections > 4 {
            return Err(LimitError::Connections);
        }
        if self.header_bytes < 256 || self.header_bytes > 4_096 {
            return Err(LimitError::HeaderBytes);
        }
        if self.header_count == 0 || self.header_count > 32 {
            return Err(LimitError::HeaderCount);
        }
        if self.socket_bytes < 512 || self.socket_bytes > 8_192 {
            return Err(LimitError::SocketBytes);
        }
        if self.io_timeout_ms == 0 || self.keepalive_timeout_ms == 0 || self.request_timeout_ms == 0
        {
            return Err(LimitError::Timeout);
        }
        Ok(())
    }
}

/// Public boot-scoped challenge mixed into every authenticated request.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct BootNonce([u8; AUTH_NONCE_BYTES]);

impl BootNonce {
    /// Rejects the all-zero sentinel so every boot has an established challenge.
    pub const fn new(bytes: [u8; AUTH_NONCE_BYTES]) -> Result<Self, AuthError> {
        let mut index = 0;
        while index < bytes.len() {
            if bytes[index] != 0 {
                return Ok(Self(bytes));
            }
            index += 1;
        }
        Err(AuthError::MissingBootNonce)
    }

    /// Exact challenge bytes returned by the public authentication route.
    pub const fn as_bytes(self) -> [u8; AUTH_NONCE_BYTES] {
        self.0
    }
}

/// Parsed proof supplied with one authenticated HTTP request.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct RequestProof {
    /// Boot-scoped, monotonically increasing client counter.
    pub counter: u64,
    /// HMAC-SHA-256 over the canonical request transcript.
    pub tag: [u8; AUTH_TAG_BYTES],
}

impl core::fmt::Debug for RequestProof {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("RequestProof")
            .field("counter", &self.counter)
            .field("tag", &"[redacted]")
            .finish()
    }
}

/// Response representation included in the canonical response proof.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum AuthenticatedMedia {
    /// Bounded JSON discovery/status body.
    Json = 1,
    /// Exact native Alumina frame.
    NativeFrame = 2,
}

/// HMAC proof returned for an authenticated response.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct ResponseProof {
    /// Request counter to which this response is bound.
    pub counter: u64,
    /// HMAC-SHA-256 over status, representation, and exact body bytes.
    pub tag: [u8; AUTH_TAG_BYTES],
}

impl core::fmt::Debug for ResponseProof {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter
            .debug_struct("ResponseProof")
            .field("counter", &self.counter)
            .field("tag", &"[redacted]")
            .finish()
    }
}

/// Global authenticated-request token bucket.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthRateLimit {
    /// Maximum immediately admitted valid requests.
    pub burst: u16,
    /// Milliseconds required to restore one token.
    pub refill_every_ms: u32,
}

impl AuthRateLimit {
    /// Initial device-wide limit: 32-request burst and 50 requests per second.
    pub const INITIAL: Self = Self {
        burst: 32,
        refill_every_ms: 20,
    };

    /// Rejects zero or implausibly unbounded admission policy.
    pub const fn validate(self) -> Result<(), AuthError> {
        if self.burst == 0 || self.burst > 128 || self.refill_every_ms == 0 {
            Err(AuthError::InvalidRateLimit)
        } else {
            Ok(())
        }
    }
}

/// Boot-scoped replay protection and global valid-request admission state.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct AuthenticationState {
    nonce: BootNonce,
    highest_counter: u64,
    seen_counters: u64,
    rate: AuthRateLimit,
    tokens: u16,
    last_refill_ms: u64,
}

impl AuthenticationState {
    /// Starts a fresh replay window and full token bucket for one boot.
    pub const fn new(nonce: BootNonce, rate: AuthRateLimit) -> Result<Self, AuthError> {
        match rate.validate() {
            Ok(()) => Ok(Self {
                nonce,
                highest_counter: 0,
                seen_counters: 0,
                rate,
                tokens: rate.burst,
                last_refill_ms: 0,
            }),
            Err(error) => Err(error),
        }
    }

    /// Challenge that the UI includes in the canonical HMAC transcript.
    pub const fn nonce(self) -> BootNonce {
        self.nonce
    }

    /// Authenticates, consumes the counter, and applies the valid-request limit.
    ///
    /// Invalid HMACs never consume counters or rate tokens. A valid proof always
    /// consumes its counter, including when the rate bucket is empty, so it
    /// cannot be replayed later.
    pub fn authorize(
        &mut self,
        secret: &[u8],
        proof: RequestProof,
        method: HttpMethod,
        path: &str,
        body: &[u8],
        now_ms: u64,
    ) -> Result<(), AuthError> {
        verify_request_proof(secret, self.nonce, proof, method, path, body)?;
        self.accept_counter(proof.counter)?;
        self.refill(now_ms);
        if self.tokens == 0 {
            return Err(AuthError::RateLimited);
        }
        self.tokens -= 1;
        Ok(())
    }

    fn accept_counter(&mut self, counter: u64) -> Result<(), AuthError> {
        if counter == 0 {
            return Err(AuthError::Counter);
        }
        if self.highest_counter == 0 {
            self.highest_counter = counter;
            self.seen_counters = 1;
            return Ok(());
        }
        if counter > self.highest_counter {
            let shift = counter - self.highest_counter;
            self.seen_counters = if shift >= u64::from(REPLAY_WINDOW_BITS) {
                1
            } else {
                (self.seen_counters << shift) | 1
            };
            self.highest_counter = counter;
            return Ok(());
        }

        let age = self.highest_counter - counter;
        if age >= u64::from(REPLAY_WINDOW_BITS) {
            return Err(AuthError::ReplayTooOld);
        }
        let bit = 1_u64 << age;
        if self.seen_counters & bit != 0 {
            return Err(AuthError::Replay);
        }
        self.seen_counters |= bit;
        Ok(())
    }

    fn refill(&mut self, now_ms: u64) {
        if now_ms < self.last_refill_ms {
            return;
        }
        let elapsed = now_ms - self.last_refill_ms;
        let restored = elapsed / u64::from(self.rate.refill_every_ms);
        if restored == 0 {
            return;
        }
        let restored = u16::try_from(restored).unwrap_or(u16::MAX);
        self.tokens = self.tokens.saturating_add(restored).min(self.rate.burst);
        self.last_refill_ms = self
            .last_refill_ms
            .saturating_add(u64::from(restored) * u64::from(self.rate.refill_every_ms));
        if self.tokens == self.rate.burst {
            self.last_refill_ms = now_ms;
        }
    }
}

/// Creates a request proof for browser/WASM, simulator, or golden-vector use.
pub fn sign_request(
    secret: &[u8],
    nonce: BootNonce,
    counter: u64,
    method: HttpMethod,
    path: &str,
    body: &[u8],
) -> Result<RequestProof, AuthError> {
    if counter == 0 {
        return Err(AuthError::Counter);
    }
    let mac = request_mac(secret, nonce, counter, method, path, body)?;
    let mut tag = [0_u8; AUTH_TAG_BYTES];
    tag.copy_from_slice(&mac.finalize().into_bytes());
    Ok(RequestProof { counter, tag })
}

fn request_mac(
    secret: &[u8],
    nonce: BootNonce,
    counter: u64,
    method: HttpMethod,
    path: &str,
    body: &[u8],
) -> Result<HmacSha256, AuthError> {
    if secret.is_empty() {
        return Err(AuthError::MissingSecret);
    }
    let path_len = u16::try_from(path.len()).map_err(|_| AuthError::PathLength)?;
    let body_len = u32::try_from(body.len()).map_err(|_| AuthError::BodyLength)?;
    let method = method.auth_value().ok_or(AuthError::Method)?;
    let body_digest = Sha256::digest(body);
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::MissingSecret)?;
    mac.update(AUTH_DOMAIN);
    mac.update(&nonce.0);
    mac.update(&counter.to_le_bytes());
    mac.update(&[method]);
    mac.update(&path_len.to_le_bytes());
    mac.update(path.as_bytes());
    mac.update(&body_len.to_le_bytes());
    mac.update(&body_digest);
    Ok(mac)
}

/// Verifies one request proof in constant time without changing replay state.
pub fn verify_request_proof(
    secret: &[u8],
    nonce: BootNonce,
    proof: RequestProof,
    method: HttpMethod,
    path: &str,
    body: &[u8],
) -> Result<(), AuthError> {
    if proof.counter == 0 {
        return Err(AuthError::Counter);
    }
    let mac = request_mac(secret, nonce, proof.counter, method, path, body)?;
    mac.verify_slice(&proof.tag)
        .map_err(|_| AuthError::Unauthorized)
}

/// Signs one authenticated response, including its transport status and media.
pub fn sign_response(
    secret: &[u8],
    nonce: BootNonce,
    counter: u64,
    http_status: u16,
    media: AuthenticatedMedia,
    body: &[u8],
) -> Result<ResponseProof, AuthError> {
    let mac = response_mac(secret, nonce, counter, http_status, media, body)?;
    let mut tag = [0_u8; AUTH_TAG_BYTES];
    tag.copy_from_slice(&mac.finalize().into_bytes());
    Ok(ResponseProof { counter, tag })
}

/// Verifies a response proof against the exact bytes observed by the caller.
pub fn verify_response_proof(
    secret: &[u8],
    nonce: BootNonce,
    proof: ResponseProof,
    http_status: u16,
    media: AuthenticatedMedia,
    body: &[u8],
) -> Result<(), AuthError> {
    let mac = response_mac(secret, nonce, proof.counter, http_status, media, body)?;
    mac.verify_slice(&proof.tag)
        .map_err(|_| AuthError::Unauthorized)
}

fn response_mac(
    secret: &[u8],
    nonce: BootNonce,
    counter: u64,
    http_status: u16,
    media: AuthenticatedMedia,
    body: &[u8],
) -> Result<HmacSha256, AuthError> {
    if secret.is_empty() {
        return Err(AuthError::MissingSecret);
    }
    if counter == 0 {
        return Err(AuthError::Counter);
    }
    let body_len = u32::try_from(body.len()).map_err(|_| AuthError::BodyLength)?;
    let body_digest = Sha256::digest(body);
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| AuthError::MissingSecret)?;
    mac.update(AUTH_RESPONSE_DOMAIN);
    mac.update(&nonce.0);
    mac.update(&counter.to_le_bytes());
    mac.update(&http_status.to_le_bytes());
    mac.update(&[media as u8]);
    mac.update(&body_len.to_le_bytes());
    mac.update(&body_digest);
    Ok(mac)
}

/// Parses the two strict authentication headers without whitespace aliases.
pub fn parse_request_proof(counter: &str, tag: &str) -> Result<RequestProof, AuthError> {
    if counter.is_empty()
        || counter.len() > 20
        || (counter.len() > 1 && counter.as_bytes()[0] == b'0')
    {
        return Err(AuthError::Counter);
    }
    let mut value = 0_u64;
    for byte in counter.bytes() {
        if !byte.is_ascii_digit() {
            return Err(AuthError::Counter);
        }
        value = value
            .checked_mul(10)
            .and_then(|current| current.checked_add(u64::from(byte - b'0')))
            .ok_or(AuthError::Counter)?;
    }
    if value == 0 || tag.len() != AUTH_TAG_HEX_BYTES {
        return Err(if value == 0 {
            AuthError::Counter
        } else {
            AuthError::TagEncoding
        });
    }

    let bytes = tag.as_bytes();
    let mut decoded = [0_u8; AUTH_TAG_BYTES];
    let mut index = 0;
    while index < decoded.len() {
        let high = decode_lower_hex(bytes[index * 2]).ok_or(AuthError::TagEncoding)?;
        let low = decode_lower_hex(bytes[index * 2 + 1]).ok_or(AuthError::TagEncoding)?;
        decoded[index] = (high << 4) | low;
        index += 1;
    }
    Ok(RequestProof {
        counter: value,
        tag: decoded,
    })
}

/// Parsed security-relevant HTTP fields, independent of any server crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthenticatedRequestMetadata {
    /// Canonical request method included in the request proof.
    pub method: HttpMethod,
    /// Exact admitted route.
    pub route: Route,
    /// Parsed boot-scoped request proof.
    pub proof: RequestProof,
    /// Exact body bytes the HTTP adapter must read before HMAC verification.
    pub body_len: usize,
}

/// Strict single-value accumulator for security-relevant raw HTTP headers.
///
/// Feed every parsed header exactly once, then call [`Self::finish`]. Unknown
/// headers are ignored; duplicates of recognized fields are rejected.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AuthHeaderAccumulator<'a> {
    content_length: Option<&'a [u8]>,
    content_type: Option<&'a [u8]>,
    transfer_encoding: Option<&'a [u8]>,
    counter: Option<&'a [u8]>,
    proof: Option<&'a [u8]>,
}

impl<'a> AuthHeaderAccumulator<'a> {
    /// Empty header state.
    pub const fn new() -> Self {
        Self {
            content_length: None,
            content_type: None,
            transfer_encoding: None,
            counter: None,
            proof: None,
        }
    }

    /// Observes one raw field without assuming its value is UTF-8.
    pub fn observe(&mut self, name: &[u8], value: &'a [u8]) -> Result<(), HttpAdmissionError> {
        let slot = if name.eq_ignore_ascii_case(b"Content-Length") {
            Some(&mut self.content_length)
        } else if name.eq_ignore_ascii_case(b"Content-Type") {
            Some(&mut self.content_type)
        } else if name.eq_ignore_ascii_case(b"Transfer-Encoding") {
            Some(&mut self.transfer_encoding)
        } else if name.eq_ignore_ascii_case(AUTH_COUNTER_HEADER.as_bytes()) {
            Some(&mut self.counter)
        } else if name.eq_ignore_ascii_case(AUTH_PROOF_HEADER.as_bytes()) {
            Some(&mut self.proof)
        } else {
            None
        };
        if let Some(slot) = slot
            && slot.replace(value).is_some()
        {
            return Err(HttpAdmissionError::DuplicateHeader);
        }
        Ok(())
    }

    /// Applies route-specific length, media, and proof syntax policy.
    pub fn finish(
        self,
        method: HttpMethod,
        route: Route,
    ) -> Result<AuthenticatedRequestMetadata, HttpAdmissionError> {
        if !route.requires_authentication() {
            return Err(HttpAdmissionError::Route);
        }
        if self.transfer_encoding.is_some() {
            return Err(HttpAdmissionError::TransferEncoding);
        }
        let body_len = match self.content_length {
            Some(value) => parse_canonical_content_length(value)?,
            None => 0,
        };
        if body_len > route.maximum_body_bytes() {
            return Err(HttpAdmissionError::BodyTooLarge);
        }
        if route == Route::StorageCommand {
            if body_len == 0 {
                return Err(HttpAdmissionError::BodyRequired);
            }
            let content_type = self.content_type.ok_or(HttpAdmissionError::ContentType)?;
            if !content_type.eq_ignore_ascii_case(b"application/vnd.alumina.frame") {
                return Err(HttpAdmissionError::ContentType);
            }
        }
        let counter = self.counter.ok_or(HttpAdmissionError::Credentials)?;
        let proof = self.proof.ok_or(HttpAdmissionError::Credentials)?;
        let counter = core::str::from_utf8(counter).map_err(|_| HttpAdmissionError::Credentials)?;
        let proof = core::str::from_utf8(proof).map_err(|_| HttpAdmissionError::Credentials)?;
        let proof =
            parse_request_proof(counter, proof).map_err(|_| HttpAdmissionError::Credentials)?;
        Ok(AuthenticatedRequestMetadata {
            method,
            route,
            proof,
            body_len,
        })
    }
}

impl Default for AuthHeaderAccumulator<'_> {
    fn default() -> Self {
        Self::new()
    }
}

fn parse_canonical_content_length(value: &[u8]) -> Result<usize, HttpAdmissionError> {
    if value.is_empty() || (value.len() > 1 && value[0] == b'0') {
        return Err(HttpAdmissionError::ContentLength);
    }
    let mut parsed = 0_usize;
    for byte in value.iter().copied() {
        if !byte.is_ascii_digit() {
            return Err(HttpAdmissionError::ContentLength);
        }
        parsed = parsed
            .checked_mul(10)
            .and_then(|current| current.checked_add(usize::from(byte - b'0')))
            .ok_or(HttpAdmissionError::BodyTooLarge)?;
    }
    Ok(parsed)
}

/// Strict HTTP metadata rejection before body hashing or service admission.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpAdmissionError {
    /// A security-relevant field occurred more than once.
    DuplicateHeader,
    /// Chunked or other transfer coding is not canonical for authenticated V1.
    TransferEncoding,
    /// Content length was empty, nondecimal, noncanonical, or overflowed.
    ContentLength,
    /// Declared body exceeds the exact route limit.
    BodyTooLarge,
    /// Native command route requires a nonempty body.
    BodyRequired,
    /// Native command route omitted or changed its exact media type.
    ContentType,
    /// Counter/proof fields were missing, non-UTF-8, or malformed.
    Credentials,
    /// Caller attempted authenticated metadata parsing for a public/error route.
    Route,
}

/// Writes exact lowercase hexadecimal into a caller-owned fixed buffer.
pub fn write_lower_hex(bytes: &[u8], output: &mut [u8]) -> Result<(), AuthError> {
    if output.len() != bytes.len().checked_mul(2).ok_or(AuthError::BodyLength)? {
        return Err(AuthError::TagEncoding);
    }
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for (index, byte) in bytes.iter().copied().enumerate() {
        output[index * 2] = HEX[usize::from(byte >> 4)];
        output[index * 2 + 1] = HEX[usize::from(byte & 0x0f)];
    }
    Ok(())
}

const fn decode_lower_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

/// Authentication, replay, or valid-request admission rejection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthError {
    /// Boot challenge was the reserved zero value.
    MissingBootNonce,
    /// No request-authentication secret was configured.
    MissingSecret,
    /// Counter was zero, noncanonical, or did not fit `u64`.
    Counter,
    /// Method is not admitted by the canonical transcript.
    Method,
    /// Path cannot be represented by the V1 transcript.
    PathLength,
    /// Body cannot be represented by the V1 transcript.
    BodyLength,
    /// Proof was not exactly 64 lowercase hexadecimal characters.
    TagEncoding,
    /// HMAC did not authenticate the exact request.
    Unauthorized,
    /// Counter was already admitted during this boot.
    Replay,
    /// Counter fell outside the 64-request out-of-order window.
    ReplayTooOld,
    /// Valid authenticated request exceeded the global token bucket.
    RateLimited,
    /// Token-bucket policy was zero or implausibly large.
    InvalidRateLimit,
}

/// Invalid fixed web-service resource policy.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitError {
    /// Invalid concurrent connection count.
    Connections,
    /// Invalid per-connection header buffer.
    HeaderBytes,
    /// Invalid parsed-header count.
    HeaderCount,
    /// Invalid per-direction socket buffer.
    SocketBytes,
    /// A timeout is zero.
    Timeout,
}

/// HTTP methods needed by the portable route classifier.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HttpMethod {
    /// Safe read.
    Get,
    /// Bounded mutation request.
    Post,
    /// Idempotent replacement request.
    Put,
    /// Bounded deletion request.
    Delete,
    /// Any method not admitted by protocol V1.
    Other,
}

impl HttpMethod {
    const fn auth_value(self) -> Option<u8> {
        match self {
            Self::Get => Some(1),
            Self::Post => Some(2),
            Self::Put => Some(3),
            Self::Delete => Some(4),
            Self::Other => None,
        }
    }
}

/// Routes present in the network foundation image.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Route {
    /// Minimal embedded bootstrap page.
    Bootstrap,
    /// Protocol and board identity.
    Identity,
    /// Service liveness without mutable machine state.
    Health,
    /// Network status/provisioning family, not yet enabled for mutation.
    Network,
    /// Public boot challenge and authentication scheme metadata.
    Authentication,
    /// Authenticated bounded cache status.
    StorageStatus,
    /// Authenticated native storage operation frame.
    StorageCommand,
    /// Known resource addressed with a forbidden method.
    MethodNotAllowed,
    /// Unknown path.
    NotFound,
}

impl Route {
    /// Whether the route requires a valid boot-scoped request proof.
    pub const fn requires_authentication(self) -> bool {
        matches!(self, Self::StorageStatus | Self::StorageCommand)
    }

    /// Exact maximum body accepted before authentication and operation decode.
    pub const fn maximum_body_bytes(self) -> usize {
        match self {
            Self::StorageCommand => MAX_AUTHENTICATED_BODY_BYTES,
            _ => 0,
        }
    }

    /// Canonical path included in request authentication for an admitted route.
    pub const fn canonical_path(self) -> Option<&'static str> {
        match self {
            Self::Bootstrap => Some("/"),
            Self::Identity => Some("/api/v1/identity"),
            Self::Health => Some("/api/v1/health"),
            Self::Network => Some("/api/v1/network"),
            Self::Authentication => Some("/api/v1/auth"),
            Self::StorageStatus | Self::StorageCommand => Some("/api/v1/storage"),
            Self::MethodNotAllowed | Self::NotFound => None,
        }
    }
}

/// Classifies exact greenfield paths without legacy aliases or prefix matching.
pub fn classify_route(method: HttpMethod, path: &str) -> Route {
    match (method, path) {
        (HttpMethod::Get, "/") => Route::Bootstrap,
        (HttpMethod::Get, "/api/v1/identity") => Route::Identity,
        (HttpMethod::Get, "/api/v1/health") => Route::Health,
        (HttpMethod::Get, "/api/v1/network") => Route::Network,
        (HttpMethod::Get, "/api/v1/auth") => Route::Authentication,
        (HttpMethod::Get, "/api/v1/storage") => Route::StorageStatus,
        (HttpMethod::Post, "/api/v1/storage") => Route::StorageCommand,
        (
            _,
            "/" | "/api/v1/identity" | "/api/v1/health" | "/api/v1/network" | "/api/v1/auth"
            | "/api/v1/storage",
        ) => Route::MethodNotAllowed,
        _ => Route::NotFound,
    }
}

/// Recoverable AP/STA supervision phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NetworkPhase {
    /// Radio has not been initialized.
    Reset,
    /// Protected AP configuration has been accepted and is starting.
    AccessPointStarting,
    /// AP is usable; no infrastructure association is active.
    Provisioning,
    /// AP remains active while station association is attempted.
    StationJoining,
    /// AP and infrastructure station are both active.
    AccessPointAndStation,
    /// AP is being recreated after an explicit recovery request or fault.
    Recovering,
}

/// Small deterministic state machine used by firmware and network simulation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct NetworkSupervisor {
    phase: NetworkPhase,
    generation: u32,
}

impl NetworkSupervisor {
    /// Starts with no radio state assumed.
    pub const fn new() -> Self {
        Self {
            phase: NetworkPhase::Reset,
            generation: 0,
        }
    }

    /// Current phase.
    pub const fn phase(self) -> NetworkPhase {
        self.phase
    }

    /// Monotonic wrapping transition generation for status polling.
    pub const fn generation(self) -> u32 {
        self.generation
    }

    /// True while the state machine requires the recovery AP to remain usable.
    pub const fn access_point_expected(self) -> bool {
        matches!(
            self.phase,
            NetworkPhase::AccessPointStarting
                | NetworkPhase::Provisioning
                | NetworkPhase::StationJoining
                | NetworkPhase::AccessPointAndStation
        )
    }

    /// Begins initial AP bring-up or a recovery restart.
    pub fn begin_access_point(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            NetworkPhase::Reset | NetworkPhase::Recovering => {
                self.set_phase(NetworkPhase::AccessPointStarting);
                Ok(())
            }
            phase => Err(TransitionError::InvalidPhase(phase)),
        }
    }

    /// Records that the protected AP and its static network stack are live.
    pub fn access_point_ready(&mut self) -> Result<(), TransitionError> {
        self.require(NetworkPhase::AccessPointStarting)?;
        self.set_phase(NetworkPhase::Provisioning);
        Ok(())
    }

    /// Begins a station join without removing the recovery AP.
    pub fn begin_station_join(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            NetworkPhase::Provisioning | NetworkPhase::AccessPointAndStation => {
                self.set_phase(NetworkPhase::StationJoining);
                Ok(())
            }
            phase => Err(TransitionError::InvalidPhase(phase)),
        }
    }

    /// Commits successful infrastructure association.
    pub fn station_joined(&mut self) -> Result<(), TransitionError> {
        self.require(NetworkPhase::StationJoining)?;
        self.set_phase(NetworkPhase::AccessPointAndStation);
        Ok(())
    }

    /// Returns to AP provisioning after wrong or unreachable credentials.
    pub fn station_join_failed(&mut self) -> Result<(), TransitionError> {
        self.require(NetworkPhase::StationJoining)?;
        self.set_phase(NetworkPhase::Provisioning);
        Ok(())
    }

    /// Leaves infrastructure Wi-Fi while preserving the device AP.
    pub fn leave_station(&mut self) -> Result<(), TransitionError> {
        match self.phase {
            NetworkPhase::StationJoining | NetworkPhase::AccessPointAndStation => {
                self.set_phase(NetworkPhase::Provisioning);
                Ok(())
            }
            phase => Err(TransitionError::InvalidPhase(phase)),
        }
    }

    /// Invalidates radio state so recovery must recreate the protected AP.
    pub fn begin_recovery(&mut self) {
        self.set_phase(NetworkPhase::Recovering);
    }

    fn require(&self, required: NetworkPhase) -> Result<(), TransitionError> {
        if self.phase == required {
            Ok(())
        } else {
            Err(TransitionError::InvalidPhase(self.phase))
        }
    }

    fn set_phase(&mut self, phase: NetworkPhase) {
        self.phase = phase;
        self.generation = self.generation.wrapping_add(1);
    }
}

impl Default for NetworkSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

/// A supervision event was applied in an impossible phase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    /// Actual phase did not admit the requested transition.
    InvalidPhase(NetworkPhase),
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEVELOPMENT: AccessPointProfile<'static> = AccessPointProfile::new(
        "Alumina-mks-tinybee-v1",
        "alumina-development",
        CredentialSource::DevelopmentFallback,
    );

    #[test]
    fn initial_profile_and_web_limits_are_bounded() {
        assert_eq!(DEVELOPMENT.validate(), Ok(()));
        assert_eq!(WebLimits::INITIAL.validate(), Ok(()));
        assert!(!DEVELOPMENT.credential_source.production_armable());
    }

    #[test]
    fn invalid_radio_strings_and_resources_are_rejected() {
        let mut invalid = DEVELOPMENT;
        invalid.passphrase = "short";
        assert_eq!(invalid.validate(), Err(ProfileError::PassphraseLength(5)));

        invalid = DEVELOPMENT;
        invalid.ssid = "bad\0ssid";
        assert_eq!(invalid.validate(), Err(ProfileError::EmbeddedNul));

        invalid = DEVELOPMENT;
        invalid.maximum_clients = 5;
        assert_eq!(invalid.validate(), Err(ProfileError::MaximumClients(5)));
    }

    #[test]
    fn exact_routes_have_no_legacy_or_prefix_aliases() {
        assert_eq!(
            classify_route(HttpMethod::Get, "/api/v1/identity"),
            Route::Identity
        );
        assert_eq!(
            classify_route(HttpMethod::Post, "/api/v1/identity"),
            Route::MethodNotAllowed
        );
        assert_eq!(classify_route(HttpMethod::Get, "/device"), Route::NotFound);
        assert_eq!(
            classify_route(HttpMethod::Get, "/api/v1/identity/extra"),
            Route::NotFound
        );
        assert_eq!(
            classify_route(HttpMethod::Get, "/api/v1/storage"),
            Route::StorageStatus
        );
        assert_eq!(
            classify_route(HttpMethod::Post, "/api/v1/storage"),
            Route::StorageCommand
        );
        assert!(Route::StorageStatus.requires_authentication());
        assert_eq!(
            Route::StorageCommand.maximum_body_bytes(),
            MAX_AUTHENTICATED_BODY_BYTES
        );
    }

    fn nonce() -> BootNonce {
        BootNonce::new([
            0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
            0xee, 0xff,
        ])
        .unwrap()
    }

    #[test]
    fn request_authentication_has_a_canonical_golden_transcript() {
        let secret = b"correct horse battery staple";
        let body = b"native-storage-frame";
        let proof = sign_request(
            secret,
            nonce(),
            42,
            HttpMethod::Post,
            "/api/v1/storage",
            body,
        )
        .unwrap();
        let mut encoded = [0_u8; AUTH_TAG_HEX_BYTES];
        write_lower_hex(&proof.tag, &mut encoded).unwrap();
        assert_eq!(
            core::str::from_utf8(&encoded).unwrap(),
            "1f2a037109085c3d7f224a3fe130bbe06967b89bf001ded3844676673dbcfeb3"
        );
        assert_eq!(
            parse_request_proof("42", core::str::from_utf8(&encoded).unwrap()).unwrap(),
            proof
        );
        assert_eq!(
            verify_request_proof(
                secret,
                nonce(),
                proof,
                HttpMethod::Post,
                "/api/v1/storage",
                body,
            ),
            Ok(())
        );
        assert_eq!(
            verify_request_proof(
                secret,
                nonce(),
                proof,
                HttpMethod::Post,
                "/api/v1/storage",
                b"changed",
            ),
            Err(AuthError::Unauthorized)
        );

        let response = sign_response(
            secret,
            nonce(),
            42,
            200,
            AuthenticatedMedia::NativeFrame,
            b"native-response-frame",
        )
        .unwrap();
        write_lower_hex(&response.tag, &mut encoded).unwrap();
        assert_eq!(
            core::str::from_utf8(&encoded).unwrap(),
            "8cd5ace4a954c4f230aab6678d0f47c7386dbc6ff28181986bb0a41cc013d43b"
        );
        assert_eq!(
            verify_response_proof(
                secret,
                nonce(),
                response,
                200,
                AuthenticatedMedia::NativeFrame,
                b"native-response-frame",
            ),
            Ok(())
        );
        assert_eq!(
            verify_response_proof(
                secret,
                nonce(),
                response,
                200,
                AuthenticatedMedia::Json,
                b"native-response-frame",
            ),
            Err(AuthError::Unauthorized)
        );
    }

    #[test]
    fn authentication_rejects_noncanonical_headers_and_invalid_proofs() {
        let valid = sign_request(
            b"secret",
            nonce(),
            7,
            HttpMethod::Get,
            "/api/v1/storage",
            b"",
        )
        .unwrap();
        let mut hex = [0_u8; AUTH_TAG_HEX_BYTES];
        write_lower_hex(&valid.tag, &mut hex).unwrap();
        let tag = core::str::from_utf8(&hex).unwrap();
        assert_eq!(parse_request_proof("07", tag), Err(AuthError::Counter));
        assert_eq!(parse_request_proof("+7", tag), Err(AuthError::Counter));

        hex[0] = b'A';
        assert_eq!(
            parse_request_proof("7", core::str::from_utf8(&hex).unwrap()),
            Err(AuthError::TagEncoding)
        );

        let mut wrong = valid;
        wrong.tag[0] ^= 1;
        assert_eq!(
            verify_request_proof(
                b"secret",
                nonce(),
                wrong,
                HttpMethod::Get,
                "/api/v1/storage",
                b"",
            ),
            Err(AuthError::Unauthorized)
        );
    }

    #[test]
    fn authenticated_header_policy_rejects_ambiguity_before_body_read() {
        let proof = sign_request(
            b"secret",
            nonce(),
            7,
            HttpMethod::Post,
            "/api/v1/storage",
            b"body",
        )
        .unwrap();
        let mut tag = [0_u8; AUTH_TAG_HEX_BYTES];
        write_lower_hex(&proof.tag, &mut tag).unwrap();

        let mut headers = AuthHeaderAccumulator::new();
        headers.observe(b"Content-Length", b"4").unwrap();
        headers
            .observe(b"Content-Type", b"application/vnd.alumina.frame")
            .unwrap();
        headers
            .observe(AUTH_COUNTER_HEADER.as_bytes(), b"7")
            .unwrap();
        headers.observe(AUTH_PROOF_HEADER.as_bytes(), &tag).unwrap();
        assert_eq!(
            headers.finish(HttpMethod::Post, Route::StorageCommand),
            Ok(AuthenticatedRequestMetadata {
                method: HttpMethod::Post,
                route: Route::StorageCommand,
                proof,
                body_len: 4,
            })
        );

        let mut duplicate = AuthHeaderAccumulator::new();
        duplicate.observe(b"Content-Length", b"4").unwrap();
        assert_eq!(
            duplicate.observe(b"content-length", b"5"),
            Err(HttpAdmissionError::DuplicateHeader)
        );

        let mut chunked = headers;
        chunked.observe(b"Transfer-Encoding", b"chunked").unwrap();
        assert_eq!(
            chunked.finish(HttpMethod::Post, Route::StorageCommand),
            Err(HttpAdmissionError::TransferEncoding)
        );

        let mut leading_zero = AuthHeaderAccumulator::new();
        leading_zero.observe(b"Content-Length", b"04").unwrap();
        leading_zero
            .observe(b"Content-Type", b"application/vnd.alumina.frame")
            .unwrap();
        leading_zero
            .observe(AUTH_COUNTER_HEADER.as_bytes(), b"7")
            .unwrap();
        leading_zero
            .observe(AUTH_PROOF_HEADER.as_bytes(), &tag)
            .unwrap();
        assert_eq!(
            leading_zero.finish(HttpMethod::Post, Route::StorageCommand),
            Err(HttpAdmissionError::ContentLength)
        );

        let mut too_large = leading_zero;
        too_large.content_length = Some(b"1149");
        assert_eq!(
            too_large.finish(HttpMethod::Post, Route::StorageCommand),
            Err(HttpAdmissionError::BodyTooLarge)
        );
    }

    #[test]
    fn replay_window_accepts_bounded_reordering_once() {
        let secret = b"secret";
        let mut state = AuthenticationState::new(nonce(), AuthRateLimit::INITIAL).unwrap();
        for counter in [2, 1, 64] {
            let proof = sign_request(
                secret,
                nonce(),
                counter,
                HttpMethod::Get,
                "/api/v1/storage",
                b"",
            )
            .unwrap();
            assert_eq!(
                state.authorize(
                    secret,
                    proof,
                    HttpMethod::Get,
                    "/api/v1/storage",
                    b"",
                    counter,
                ),
                Ok(())
            );
        }

        let replay =
            sign_request(secret, nonce(), 1, HttpMethod::Get, "/api/v1/storage", b"").unwrap();
        assert_eq!(
            state.authorize(secret, replay, HttpMethod::Get, "/api/v1/storage", b"", 65,),
            Err(AuthError::Replay)
        );

        let newest = sign_request(
            secret,
            nonce(),
            100,
            HttpMethod::Get,
            "/api/v1/storage",
            b"",
        )
        .unwrap();
        state
            .authorize(secret, newest, HttpMethod::Get, "/api/v1/storage", b"", 100)
            .unwrap();
        let old =
            sign_request(secret, nonce(), 36, HttpMethod::Get, "/api/v1/storage", b"").unwrap();
        assert_eq!(
            state.authorize(secret, old, HttpMethod::Get, "/api/v1/storage", b"", 101,),
            Err(AuthError::ReplayTooOld)
        );
    }

    #[test]
    fn only_valid_requests_consume_rate_tokens_but_limited_counters_are_spent() {
        let secret = b"secret";
        let rate = AuthRateLimit {
            burst: 2,
            refill_every_ms: 10,
        };
        let mut state = AuthenticationState::new(nonce(), rate).unwrap();
        for counter in 1..=2 {
            let proof = sign_request(
                secret,
                nonce(),
                counter,
                HttpMethod::Get,
                "/api/v1/storage",
                b"",
            )
            .unwrap();
            assert_eq!(
                state.authorize(secret, proof, HttpMethod::Get, "/api/v1/storage", b"", 0,),
                Ok(())
            );
        }

        let third =
            sign_request(secret, nonce(), 3, HttpMethod::Get, "/api/v1/storage", b"").unwrap();
        assert_eq!(
            state.authorize(secret, third, HttpMethod::Get, "/api/v1/storage", b"", 0,),
            Err(AuthError::RateLimited)
        );
        assert_eq!(
            state.authorize(secret, third, HttpMethod::Get, "/api/v1/storage", b"", 10,),
            Err(AuthError::Replay)
        );

        let fourth =
            sign_request(secret, nonce(), 4, HttpMethod::Get, "/api/v1/storage", b"").unwrap();
        assert_eq!(
            state.authorize(secret, fourth, HttpMethod::Get, "/api/v1/storage", b"", 10,),
            Ok(())
        );
    }

    #[test]
    fn failed_join_never_removes_recovery_ap() {
        let mut state = NetworkSupervisor::new();
        state.begin_access_point().unwrap();
        state.access_point_ready().unwrap();
        state.begin_station_join().unwrap();
        assert!(state.access_point_expected());
        state.station_join_failed().unwrap();
        assert_eq!(state.phase(), NetworkPhase::Provisioning);
        assert!(state.access_point_expected());
        assert_eq!(state.generation(), 4);
    }

    #[test]
    fn successful_join_can_leave_back_to_provisioning() {
        let mut state = NetworkSupervisor::new();
        state.begin_access_point().unwrap();
        state.access_point_ready().unwrap();
        state.begin_station_join().unwrap();
        state.station_joined().unwrap();
        assert_eq!(state.phase(), NetworkPhase::AccessPointAndStation);
        state.leave_station().unwrap();
        assert_eq!(state.phase(), NetworkPhase::Provisioning);
    }

    #[test]
    fn recovery_requires_access_point_restart() {
        let mut state = NetworkSupervisor::new();
        state.begin_access_point().unwrap();
        state.access_point_ready().unwrap();
        state.begin_recovery();
        assert_eq!(state.phase(), NetworkPhase::Recovering);
        assert!(!state.access_point_expected());
        state.begin_access_point().unwrap();
        assert_eq!(state.phase(), NetworkPhase::AccessPointStarting);
    }
}
