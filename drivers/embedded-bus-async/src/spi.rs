//! A shared SPI bus implementation using `RwLock` for exclusive async access.
//!
//! This module provides `RwLockDevice`, a wrapper that allows multiple parts of an
//! application to share a single `SpiBus` instance. Each `RwLockDevice` manages its
//! own chip-select (CS) pin. The surrounding `Rc` keeps the design single-threaded.

use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};

use alloc::rc::Rc;
use core::fmt::Debug;
use embedded_hal::spi::{Error, ErrorKind};
use embedded_hal::spi::{ErrorType, Operation};
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::{SpiBus, SpiDevice};
use esp_hal::gpio::Output;

/// A `RwLock`-based shared bus [`SpiDevice`] implementation.
///
/// Each handle owns a chip-select pin while sharing the locked [`SpiBus`].
pub struct RwLockDevice<'a, BUS, D> {
    bus: Rc<RwLock<CriticalSectionRawMutex, BUS>>,
    cs: Output<'a>,
    delay: D,
}

impl<'a, BUS, D> RwLockDevice<'a, BUS, D> {
    /// Creates a new `RwLockDevice`.
    ///
    /// The constructor immediately drives `cs` high.
    #[inline]
    pub fn new(
        bus: Rc<RwLock<CriticalSectionRawMutex, BUS>>,
        mut cs: Output<'a>,
        delay: D,
    ) -> Self {
        cs.set_high();
        Self { bus, cs, delay }
    }
}

impl<'a, BUS, D> ErrorType for RwLockDevice<'a, BUS, D>
where
    BUS: ErrorType,
{
    type Error = DeviceError<BUS::Error, CsError>;
}

/// Reserved error type for chip-select operations.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CsError;

impl<'a, BUS, D> SpiDevice<u8> for RwLockDevice<'a, BUS, D>
where
    BUS: SpiBus<u8>,
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
pub async fn transaction<'a, Word, BUS, D>(
    operations: &mut [Operation<'_, Word>],
    bus: &mut BUS,
    delay: &mut D,
    cs: &mut Output<'a>,
) -> Result<(), DeviceError<BUS::Error, CsError>>
where
    BUS: SpiBus<Word> + ErrorType,
    D: DelayNs,
    Word: Copy,
{
    cs.set_low();

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
    cs.set_high();

    op_res.map_err(DeviceError::Spi)?;
    flush_res.map_err(DeviceError::Spi)?;

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
