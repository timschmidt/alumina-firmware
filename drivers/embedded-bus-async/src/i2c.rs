use alloc::rc::Rc;
use embassy_sync::{blocking_mutex::raw::CriticalSectionRawMutex, rwlock::RwLock};
use embedded_hal::i2c::{Operation, SevenBitAddress};
use embedded_hal_async::i2c::{self, I2c};

/// `RwLock`-based shared [`I2c`] implementation.
///
/// Clone the surrounding `Rc` to create multiple handles. Addresses remain
/// operation arguments, as required by the [`I2c`] trait.
pub struct RwLockI2cDevice<I2cType, ErrorType: embedded_hal_async::i2c::Error>
where
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
{
    bus: Rc<RwLock<CriticalSectionRawMutex, I2cType>>,
}

impl<I2cType, ErrorType: embedded_hal_async::i2c::Error> RwLockI2cDevice<I2cType, ErrorType>
where
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
{
    /// Creates a handle to `bus`.
    pub fn new(bus: Rc<RwLock<CriticalSectionRawMutex, I2cType>>) -> Self {
        Self { bus }
    }
}

impl<I2cType, ErrorType: embedded_hal_async::i2c::Error> i2c::ErrorType
    for RwLockI2cDevice<I2cType, ErrorType>
where
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
{
    type Error = ErrorType;
}

impl<I2cType, ErrorType: embedded_hal_async::i2c::Error> i2c::I2c
    for RwLockI2cDevice<I2cType, ErrorType>
where
    I2cType: I2c<SevenBitAddress, Error = ErrorType>,
{
    async fn transaction(
        &mut self,
        address: SevenBitAddress,
        operations: &mut [Operation<'_>],
    ) -> Result<(), Self::Error> {
        let mut bus = self.bus.write().await;
        bus.transaction(address, operations).await
    }
}
