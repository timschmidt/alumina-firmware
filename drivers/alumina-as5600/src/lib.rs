#![no_std]
#![doc = "Read-only asynchronous AS5600 absolute-angle sensor driver."]
//!
//! The driver reports the device's unscaled 12-bit count and magnet-status
//! flags. Electrical angle, calibration, uncertainty, and control policy are
//! deliberately outside this transport crate.

use embedded_hal_async::i2c::{I2c, SevenBitAddress};

/// Fixed seven-bit I2C address established by the AS5600 datasheet.
pub const I2C_ADDRESS: SevenBitAddress = 0x36;
/// Number of distinct raw counts in one mechanical turn.
pub const COUNTS_PER_TURN: u32 = 4_096;
/// Width of the unscaled raw-angle result.
pub const RAW_ANGLE_BITS: u8 = 12;
/// Maximum SCL frequency admitted by the device datasheet.
pub const MAXIMUM_I2C_FREQUENCY_HZ: u32 = 1_000_000;

const STATUS_REGISTER: u8 = 0x0b;
const RAW_ANGLE_HIGH_REGISTER: u8 = 0x0c;
const STATUS_MAGNET_DETECTED: u8 = 1 << 5;
const STATUS_MAGNET_TOO_WEAK: u8 = 1 << 4;
const STATUS_MAGNET_TOO_STRONG: u8 = 1 << 3;
const STATUS_DEFINED_MASK: u8 =
    STATUS_MAGNET_DETECTED | STATUS_MAGNET_TOO_WEAK | STATUS_MAGNET_TOO_STRONG;

/// One exact unscaled count in the AS5600's 12-bit mechanical-turn lattice.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct RawAngle(u16);

impl RawAngle {
    /// Constructs a raw angle after checking the 12-bit device range.
    ///
    /// # Errors
    ///
    /// Returns [`RawAngleError::OutOfRange`] for values outside `0..4096`.
    pub const fn new(count: u16) -> Result<Self, RawAngleError> {
        if count < COUNTS_PER_TURN as u16 {
            Ok(Self(count))
        } else {
            Err(RawAngleError::OutOfRange { received: count })
        }
    }

    /// Returns the exact device count in `0..4096`.
    pub const fn count(self) -> u16 {
        self.0
    }

    fn decode(bytes: [u8; 2]) -> Result<Self, RawAngleError> {
        if bytes[0] & 0xf0 != 0 {
            return Err(RawAngleError::HighBitsSet { received: bytes[0] });
        }
        Self::new((u16::from(bytes[0]) << 8) | u16::from(bytes[1]))
    }
}

impl From<RawAngle> for u32 {
    fn from(value: RawAngle) -> Self {
        u32::from(value.count())
    }
}

/// Malformed raw-angle bytes or an invalid caller-provided count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RawAngleError {
    /// Bits outside the documented 12-bit high-byte field were nonzero.
    HighBitsSet { received: u8 },
    /// A caller-provided count was outside the AS5600 turn lattice.
    OutOfRange { received: u16 },
}

/// Decoded AS5600 STATUS register.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MagnetStatus {
    defined_bits: u8,
    reserved_bits: u8,
}

impl MagnetStatus {
    /// Decodes the three documented status flags and preserves all other bits.
    pub const fn from_register(register: u8) -> Self {
        Self {
            defined_bits: register & STATUS_DEFINED_MASK,
            reserved_bits: register & !STATUS_DEFINED_MASK,
        }
    }

    /// Whether the device currently reports that a magnet was detected.
    pub const fn magnet_detected(self) -> bool {
        self.defined_bits & STATUS_MAGNET_DETECTED != 0
    }

    /// Whether automatic gain control reports a magnet that is too weak.
    pub const fn magnet_too_weak(self) -> bool {
        self.defined_bits & STATUS_MAGNET_TOO_WEAK != 0
    }

    /// Whether automatic gain control reports a magnet that is too strong.
    pub const fn magnet_too_strong(self) -> bool {
        self.defined_bits & STATUS_MAGNET_TOO_STRONG != 0
    }

    /// Bits not assigned a meaning by this datasheet revision.
    pub const fn reserved_bits(self) -> u8 {
        self.reserved_bits
    }

    /// Whether all three documented flags describe a nominal field condition.
    pub const fn is_nominal(self) -> bool {
        self.magnet_detected() && !self.magnet_too_weak() && !self.magnet_too_strong()
    }

    const fn same_defined_state(self, other: Self) -> bool {
        self.defined_bits == other.defined_bits
    }
}

/// A raw angle bracketed by status reads before and after it.
///
/// The three fields come from three separate addressed I2C transactions. The
/// type deliberately does not claim that the angle and status were sampled
/// simultaneously.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Observation {
    status_before: MagnetStatus,
    raw_angle: RawAngle,
    status_after: MagnetStatus,
}

impl Observation {
    /// Status observed before the raw-angle transaction.
    pub const fn status_before(self) -> MagnetStatus {
        self.status_before
    }

    /// Unscaled 12-bit raw-angle count.
    pub const fn raw_angle(self) -> RawAngle {
        self.raw_angle
    }

    /// Status observed after the raw-angle transaction.
    pub const fn status_after(self) -> MagnetStatus {
        self.status_after
    }

    /// Whether the three documented status flags were unchanged across the read.
    pub const fn defined_status_stable(self) -> bool {
        self.status_before.same_defined_state(self.status_after)
    }

    /// Whether both bracketing status reads reported a nominal magnetic field.
    pub const fn nominal_throughout(self) -> bool {
        self.status_before.is_nominal() && self.status_after.is_nominal()
    }
}

/// AS5600 transport or raw-field decoding failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error<BusError> {
    /// The underlying I2C implementation rejected a transaction.
    Bus(BusError),
    /// The two raw-angle bytes did not encode a documented 12-bit value.
    RawAngle(RawAngleError),
}

/// Read-only owner of one AS5600 I2C transport.
pub struct As5600<I2cType> {
    i2c: I2cType,
}

impl<I2cType> As5600<I2cType> {
    /// Takes exclusive ownership of an already configured I2C transport.
    ///
    /// Construction performs no bus transaction and changes no sensor state.
    pub const fn new(i2c: I2cType) -> Self {
        Self { i2c }
    }

    /// Releases the transport without performing a bus transaction.
    pub fn release(self) -> I2cType {
        self.i2c
    }
}

impl<I2cType, BusError> As5600<I2cType>
where
    I2cType: I2c<SevenBitAddress, Error = BusError>,
    BusError: embedded_hal_async::i2c::Error,
{
    /// Reads the unscaled RAW ANGLE high and low bytes in one transaction.
    ///
    /// The one-byte write selects the address pointer; it does not write a
    /// configuration value. No sensor configuration or OTP operation is
    /// exposed by this driver.
    pub async fn read_raw_angle(&mut self) -> Result<RawAngle, Error<BusError>> {
        let mut bytes = [0_u8; 2];
        self.i2c
            .write_read(I2C_ADDRESS, &[RAW_ANGLE_HIGH_REGISTER], &mut bytes)
            .await
            .map_err(Error::Bus)?;
        RawAngle::decode(bytes).map_err(Error::RawAngle)
    }

    /// Reads and decodes the documented magnet-status flags.
    pub async fn read_status(&mut self) -> Result<MagnetStatus, Error<BusError>> {
        let mut register = [0_u8; 1];
        self.i2c
            .write_read(I2C_ADDRESS, &[STATUS_REGISTER], &mut register)
            .await
            .map_err(Error::Bus)?;
        Ok(MagnetStatus::from_register(register[0]))
    }

    /// Brackets one raw-angle read with status reads before and after it.
    ///
    /// This costs three I2C transactions. Callers that have a separately
    /// scheduled diagnostic policy can use [`Self::read_raw_angle`] directly.
    pub async fn read_observation(&mut self) -> Result<Observation, Error<BusError>> {
        let status_before = self.read_status().await?;
        let raw_angle = self.read_raw_angle().await?;
        let status_after = self.read_status().await?;
        Ok(Observation {
            status_before,
            raw_angle,
            status_after,
        })
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use core::future::Future;
    use core::task::{Context, Poll, Waker};
    use embedded_hal_async::i2c::{ErrorKind, ErrorType, Operation};
    use std::boxed::Box;
    use std::collections::VecDeque;
    use std::sync::Arc;
    use std::task::Wake;
    use std::vec::Vec;

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum MockError {
        Bus,
        Script,
    }

    impl embedded_hal_async::i2c::Error for MockError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    struct Reply {
        register: u8,
        bytes: Vec<u8>,
        error: Option<MockError>,
    }

    struct MockI2c {
        replies: VecDeque<Reply>,
    }

    impl MockI2c {
        fn new(replies: impl IntoIterator<Item = Reply>) -> Self {
            Self {
                replies: replies.into_iter().collect(),
            }
        }

        fn reply(register: u8, bytes: &[u8]) -> Reply {
            Reply {
                register,
                bytes: bytes.to_vec(),
                error: None,
            }
        }

        fn fault(register: u8, error: MockError) -> Reply {
            Reply {
                register,
                bytes: Vec::new(),
                error: Some(error),
            }
        }

        fn assert_exhausted(&self) {
            assert!(self.replies.is_empty());
        }
    }

    impl ErrorType for MockI2c {
        type Error = MockError;
    }

    impl I2c<SevenBitAddress> for MockI2c {
        async fn write_read(
            &mut self,
            address: SevenBitAddress,
            write: &[u8],
            read: &mut [u8],
        ) -> Result<(), Self::Error> {
            let reply = self.replies.pop_front().ok_or(MockError::Script)?;
            if address != I2C_ADDRESS || write != [reply.register] {
                return Err(MockError::Script);
            }
            if let Some(error) = reply.error {
                return Err(error);
            }
            if read.len() != reply.bytes.len() {
                return Err(MockError::Script);
            }
            read.copy_from_slice(&reply.bytes);
            Ok(())
        }

        async fn transaction(
            &mut self,
            _address: SevenBitAddress,
            _operations: &mut [Operation<'_>],
        ) -> Result<(), Self::Error> {
            Err(MockError::Script)
        }
    }

    struct NoopWake;

    impl Wake for NoopWake {
        fn wake(self: Arc<Self>) {}
    }

    fn block_on<F: Future>(future: F) -> F::Output {
        let waker = Waker::from(Arc::new(NoopWake));
        let mut context = Context::from_waker(&waker);
        let mut future = Box::pin(future);
        loop {
            match future.as_mut().poll(&mut context) {
                Poll::Ready(output) => return output,
                Poll::Pending => std::thread::yield_now(),
            }
        }
    }

    #[test]
    fn raw_angle_is_one_addressed_two_byte_read() {
        let bus = MockI2c::new([MockI2c::reply(RAW_ANGLE_HIGH_REGISTER, &[0x0a, 0xbc])]);
        let mut sensor = As5600::new(bus);
        let angle = block_on(sensor.read_raw_angle()).unwrap();
        assert_eq!(angle.count(), 0x0abc);
        assert_eq!(u32::from(angle), 0x0abc);
        sensor.release().assert_exhausted();
    }

    #[test]
    fn undocumented_high_bits_are_rejected_not_masked() {
        let bus = MockI2c::new([MockI2c::reply(RAW_ANGLE_HIGH_REGISTER, &[0xfa, 0xbc])]);
        let mut sensor = As5600::new(bus);
        assert_eq!(
            block_on(sensor.read_raw_angle()),
            Err(Error::RawAngle(RawAngleError::HighBitsSet {
                received: 0xfa
            }))
        );
        sensor.release().assert_exhausted();
    }

    #[test]
    fn exact_count_constructor_enforces_the_device_lattice() {
        assert_eq!(RawAngle::new(0).unwrap().count(), 0);
        assert_eq!(RawAngle::new(4_095).unwrap().count(), 4_095);
        assert_eq!(
            RawAngle::new(4_096),
            Err(RawAngleError::OutOfRange { received: 4_096 })
        );
    }

    #[test]
    fn status_preserves_reserved_bits_without_assigning_them_policy() {
        let status = MagnetStatus::from_register(0b1111_0111);
        assert!(status.magnet_detected());
        assert!(status.magnet_too_weak());
        assert!(!status.magnet_too_strong());
        assert_eq!(status.reserved_bits(), 0b1100_0111);
        assert!(!status.is_nominal());

        let nominal = MagnetStatus::from_register(STATUS_MAGNET_DETECTED);
        assert!(nominal.is_nominal());
        assert_eq!(nominal.reserved_bits(), 0);
    }

    #[test]
    fn observation_explicitly_brackets_the_angle_transaction() {
        let bus = MockI2c::new([
            MockI2c::reply(STATUS_REGISTER, &[STATUS_MAGNET_DETECTED | 0x01]),
            MockI2c::reply(RAW_ANGLE_HIGH_REGISTER, &[0x01, 0x23]),
            MockI2c::reply(STATUS_REGISTER, &[STATUS_MAGNET_DETECTED | 0x04]),
        ]);
        let mut sensor = As5600::new(bus);
        let observation = block_on(sensor.read_observation()).unwrap();
        assert_eq!(observation.raw_angle().count(), 0x0123);
        assert!(observation.defined_status_stable());
        assert!(observation.nominal_throughout());
        assert_ne!(observation.status_before(), observation.status_after());
        sensor.release().assert_exhausted();
    }

    #[test]
    fn observation_reports_a_defined_status_transition() {
        let bus = MockI2c::new([
            MockI2c::reply(STATUS_REGISTER, &[STATUS_MAGNET_DETECTED]),
            MockI2c::reply(RAW_ANGLE_HIGH_REGISTER, &[0x00, 0x09]),
            MockI2c::reply(STATUS_REGISTER, &[STATUS_MAGNET_TOO_WEAK]),
        ]);
        let mut sensor = As5600::new(bus);
        let observation = block_on(sensor.read_observation()).unwrap();
        assert!(!observation.defined_status_stable());
        assert!(!observation.nominal_throughout());
        sensor.release().assert_exhausted();
    }

    #[test]
    fn exact_bus_error_is_preserved() {
        let bus = MockI2c::new([MockI2c::fault(STATUS_REGISTER, MockError::Bus)]);
        let mut sensor = As5600::new(bus);
        assert_eq!(
            block_on(sensor.read_status()),
            Err(Error::Bus(MockError::Bus))
        );
        sensor.release().assert_exhausted();
    }
}
