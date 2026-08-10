//! A shared SPI bus implementation using `RwLock` for exclusive async access.
//!
//! This module provides `RwLockDevice`, a wrapper that allows multiple parts of an
//! application to share a single `SpiBus` instance. Each `RwLockDevice` manages its
//! own chip-select (CS) pin. The surrounding `Rc` keeps the design single-threaded.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};

use alloc::rc::Rc;
use core::fmt::Debug;
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::{Error, ErrorKind};
use embedded_hal::spi::{ErrorType, Operation};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::{SpiBus, SpiDevice};

/// A `RwLock`-based shared bus [`SpiDevice`] implementation.
///
/// Each handle owns a chip-select pin while sharing the locked [`SpiBus`].
pub struct RwLockDevice<BUS, CS, D> {
    bus: Rc<RwLock<CriticalSectionRawMutex, BUS>>,
    cs: CS,
    delay: D,
}

impl<BUS, CS, D> RwLockDevice<BUS, CS, D>
where
    CS: OutputPin,
{
    /// Creates a new `RwLockDevice`.
    ///
    /// The constructor immediately drives `cs` high.
    #[inline]
    pub fn new(
        bus: Rc<RwLock<CriticalSectionRawMutex, BUS>>,
        mut cs: CS,
        delay: D,
    ) -> Result<Self, CS::Error> {
        cs.set_high()?;
        Ok(Self { bus, cs, delay })
    }
}

impl<BUS, CS, D> ErrorType for RwLockDevice<BUS, CS, D>
where
    BUS: ErrorType,
    CS: OutputPin,
{
    type Error = DeviceError<BUS::Error, CS::Error>;
}

impl<BUS, CS, D> SpiDevice<u8> for RwLockDevice<BUS, CS, D>
where
    BUS: SpiBus<u8>,
    CS: OutputPin,
    D: DelayNs,
{
    /// Acquires the bus and executes one chip-select-bounded transaction.
    #[inline]
    async fn transaction(
        &mut self,
        operations: &mut [Operation<'_, u8>],
    ) -> Result<(), Self::Error> {
        let bus = &mut *self.bus.write().await;

        let result = transaction(operations, bus, &mut self.delay, &mut self.cs).await;

        if let Err(err) = &result {
            log::warn!("Error communicating with the device: {err:?}");
        }

        result
    }
}

/// Executes operations between chip-select assertion and deassertion.
#[inline]
pub async fn transaction<Word, BUS, CS, D>(
    operations: &mut [Operation<'_, Word>],
    bus: &mut BUS,
    delay: &mut D,
    cs: &mut CS,
) -> Result<(), DeviceError<BUS::Error, CS::Error>>
where
    BUS: SpiBus<Word> + ErrorType,
    CS: OutputPin,
    D: DelayNs,
    Word: Copy,
{
    cs.set_low().map_err(DeviceError::Cs)?;

    let op_res = {
        let mut result = Ok(());
        for op in operations {
            if let Err(err) = process_op::<BUS, D, Word>(bus, delay, op).await {
                log::warn!("Error communicating with the SPI device.");
                result = Err(err);
            }
        }
        result
    };

    // On failure, it's important to still flush and deassert CS.
    let flush_res = bus.flush().await;
    let cs_res = cs.set_high();

    op_res.map_err(DeviceError::Spi)?;
    flush_res.map_err(DeviceError::Spi)?;
    cs_res.map_err(DeviceError::Cs)?;

    Ok(())
}

/// An error type for `RwLockDevice` operations.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum DeviceError<BUS, CS> {
    /// An inner SPI bus operation failed.
    Spi(BUS),
    /// Asserting or deasserting the CS pin failed.
    Cs(CS),
}

impl<BUS, CS> Error for DeviceError<BUS, CS>
where
    BUS: Error + Debug,
    CS: Debug,
{
    #[inline]
    fn kind(&self) -> ErrorKind {
        match self {
            Self::Spi(e) => e.kind(),
            Self::Cs(_) => ErrorKind::ChipSelectFault,
        }
    }
}

/// Processes a single SPI operation.
async fn process_op<'a, BUS: SpiBus<Word> + ErrorType, D: DelayNs, Word: Copy>(
    bus: &mut BUS,
    delay: &mut D,
    op: &mut Operation<'a, Word>,
) -> Result<(), <BUS as ErrorType>::Error> {
    match op {
        Operation::Read(buf) => bus.read(buf).await,
        Operation::Write(buf) => bus.write(buf).await,
        Operation::Transfer(read, write) => bus.transfer(read, write).await,
        Operation::TransferInPlace(buf) => bus.transfer_in_place(buf).await,
        Operation::DelayNs(ns) => {
            bus.flush().await?;
            delay.delay_ns(*ns).await;
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use core::convert::Infallible;

    use embassy_futures::block_on;
    use embedded_hal::digital::{Error as _, ErrorKind as DigitalErrorKind};
    use embedded_hal::spi::{Error as _, ErrorKind as SpiErrorKind};

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum PinError {
        Drive,
    }

    impl embedded_hal::digital::Error for PinError {
        fn kind(&self) -> DigitalErrorKind {
            DigitalErrorKind::Other
        }
    }

    struct FakePin {
        fail_low: bool,
        fail_high: bool,
        low_calls: usize,
        high_calls: usize,
    }

    impl embedded_hal::digital::ErrorType for FakePin {
        type Error = PinError;
    }

    impl OutputPin for FakePin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.low_calls += 1;
            if self.fail_low {
                Err(PinError::Drive)
            } else {
                Ok(())
            }
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.high_calls += 1;
            if self.fail_high {
                Err(PinError::Drive)
            } else {
                Ok(())
            }
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum BusError {
        Transfer,
    }

    impl embedded_hal::spi::Error for BusError {
        fn kind(&self) -> SpiErrorKind {
            SpiErrorKind::Other
        }
    }

    struct FakeBus {
        fail_write: bool,
        write_calls: usize,
        flush_calls: usize,
    }

    impl ErrorType for FakeBus {
        type Error = BusError;
    }

    impl SpiBus<u8> for FakeBus {
        async fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            words.fill(0);
            Ok(())
        }

        async fn write(&mut self, _words: &[u8]) -> Result<(), Self::Error> {
            self.write_calls += 1;
            if self.fail_write {
                Err(BusError::Transfer)
            } else {
                Ok(())
            }
        }

        async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
            for (destination, source) in read.iter_mut().zip(write) {
                *destination = *source;
            }
            Ok(())
        }

        async fn transfer_in_place(&mut self, _words: &mut [u8]) -> Result<(), Self::Error> {
            Ok(())
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            self.flush_calls += 1;
            Ok(())
        }
    }

    struct NoDelay;

    impl DelayNs for NoDelay {
        async fn delay_ns(&mut self, _ns: u32) {}
    }

    fn pin() -> FakePin {
        FakePin {
            fail_low: false,
            fail_high: false,
            low_calls: 0,
            high_calls: 0,
        }
    }

    #[test]
    fn constructor_propagates_initial_chip_select_failure() {
        let bus = Rc::new(RwLock::new(FakeBus {
            fail_write: false,
            write_calls: 0,
            flush_calls: 0,
        }));
        let mut cs = pin();
        cs.fail_high = true;
        let result = RwLockDevice::new(bus, cs, NoDelay);
        assert!(matches!(result, Err(PinError::Drive)));
    }

    #[test]
    fn operation_error_still_flushes_and_releases_chip_select() {
        let mut bus = FakeBus {
            fail_write: true,
            write_calls: 0,
            flush_calls: 0,
        };
        let mut cs = pin();
        let mut delay = NoDelay;
        let mut operations = [Operation::Write(&[1, 2, 3])];

        let result = block_on(transaction(&mut operations, &mut bus, &mut delay, &mut cs));

        assert_eq!(result, Err(DeviceError::Spi(BusError::Transfer)));
        assert_eq!(bus.write_calls, 1);
        assert_eq!(bus.flush_calls, 1);
        assert_eq!(cs.low_calls, 1);
        assert_eq!(cs.high_calls, 1);
    }

    #[test]
    fn chip_select_assert_and_release_failures_remain_distinct() {
        let mut bus = FakeBus {
            fail_write: false,
            write_calls: 0,
            flush_calls: 0,
        };
        let mut cs = pin();
        cs.fail_low = true;
        let mut delay = NoDelay;
        let mut no_operations: [Operation<'_, u8>; 0] = [];
        assert_eq!(
            block_on(transaction(
                &mut no_operations,
                &mut bus,
                &mut delay,
                &mut cs
            )),
            Err(DeviceError::Cs(PinError::Drive))
        );
        assert_eq!(bus.flush_calls, 0);

        cs.fail_low = false;
        cs.fail_high = true;
        assert_eq!(
            block_on(transaction(
                &mut no_operations,
                &mut bus,
                &mut delay,
                &mut cs
            )),
            Err(DeviceError::Cs(PinError::Drive))
        );
        assert_eq!(bus.flush_calls, 1);
    }

    #[test]
    fn error_kinds_match_embedded_hal_contract() {
        assert_eq!(PinError::Drive.kind(), DigitalErrorKind::Other);
        assert_eq!(BusError::Transfer.kind(), SpiErrorKind::Other);
        assert_eq!(
            DeviceError::<BusError, Infallible>::Spi(BusError::Transfer).kind(),
            SpiErrorKind::Other
        );
        assert_eq!(
            DeviceError::<Infallible, PinError>::Cs(PinError::Drive).kind(),
            SpiErrorKind::ChipSelectFault
        );
    }
}
