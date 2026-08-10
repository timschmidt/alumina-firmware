#![no_std]
#![doc = "Bounded clean-room SD memory card SPI-mode transport for Alumina."]
//!
//! The driver owns one SPI bus and chip-select line. Every wait is bounded by
//! configuration, all reads use explicit `0xff` transfer bytes, command and
//! data CRCs are enabled, and a completed block write includes card-status and
//! not-busy checks before it can satisfy the cache media's durability barrier.
//! Operations deliberately are not cancellation-safe with respect to chip
//! select: the sole core-0 storage actor must await each operation to completion.

use alumina_storage::media::{AsyncBlockDevice, MediaBlock};
use embedded_hal::digital::OutputPin;
use embedded_hal_async::delay::DelayNs;
use embedded_hal_async::spi::SpiBus;

const CMD0_GO_IDLE_STATE: u8 = 0;
const CMD8_SEND_IF_COND: u8 = 8;
const CMD9_SEND_CSD: u8 = 9;
const CMD13_SEND_STATUS: u8 = 13;
const CMD16_SET_BLOCKLEN: u8 = 16;
const CMD17_READ_SINGLE_BLOCK: u8 = 17;
const CMD24_WRITE_BLOCK: u8 = 24;
const CMD55_APP_CMD: u8 = 55;
const CMD58_READ_OCR: u8 = 58;
const CMD59_CRC_ON_OFF: u8 = 59;
const ACMD41_SD_SEND_OP_COND: u8 = 41;

const R1_READY: u8 = 0x00;
const R1_IDLE: u8 = 0x01;
const R1_ILLEGAL_COMMAND: u8 = 0x04;
const DATA_START_TOKEN: u8 = 0xfe;
const DATA_RESPONSE_MASK: u8 = 0x1f;
const DATA_RESPONSE_ACCEPTED: u8 = 0x05;
const OCR_POWER_UP: u32 = 1 << 31;
const OCR_CAPACITY_STATUS: u32 = 1 << 30;
const OCR_27_TO_36_VOLTS: u32 = 0x00ff_8000;
const CMD8_ARGUMENT: u32 = 0x0000_01aa;
const ACMD41_HCS: u32 = 1 << 30;
const SPI_MODE_ENTRY_BYTES: usize = 10;
const POLL_BUFFER_BYTES: usize = 32;

/// SPI bus extension needed to enter identification speed and then data speed.
///
/// Board crates implement this for a local wrapper around their HAL SPI type;
/// the wrapper also fixes mode 0 and eight-bit transfers.
pub trait ReconfigurableSpiBus: SpiBus<u8> {
    /// HAL-specific configuration failure.
    type ConfigError;

    /// Changes only the bus frequency while preserving SD-compatible mode.
    fn set_frequency_hz(&mut self, frequency_hz: u32) -> Result<(), Self::ConfigError>;
}

/// Bounded timing and bus policy for one card.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    /// Identification clock, at most 400 kHz.
    pub initialization_frequency_hz: u32,
    /// Default-speed SPI clock after complete identification.
    pub transfer_frequency_hz: u32,
    /// CMD0 attempts before initialization fails.
    pub reset_attempts: u16,
    /// ACMD41 attempts before initialization fails.
    pub operation_condition_attempts: u16,
    /// Delay between ACMD41 attempts.
    pub operation_condition_delay_ns: u32,
    /// Maximum dummy bytes before an R1 or data-response token.
    pub response_poll_bytes: u16,
    /// Maximum dummy bytes before a read data token.
    pub data_token_poll_bytes: u32,
    /// Maximum dummy bytes while the card holds MISO low for programming.
    pub busy_poll_bytes: u32,
}

impl Config {
    /// Conservative fixed-memory defaults for removable cards.
    pub const DEFAULT: Self = Self {
        initialization_frequency_hz: 400_000,
        transfer_frequency_hz: 10_000_000,
        reset_attempts: 16,
        operation_condition_attempts: 1_000,
        operation_condition_delay_ns: 1_000_000,
        response_poll_bytes: 16,
        data_token_poll_bytes: 250_000,
        busy_poll_bytes: 500_000,
    };

    /// Rejects zero, unbounded, or out-of-mode policy before touching hardware.
    pub const fn validate(self) -> Result<(), ConfigError> {
        if self.initialization_frequency_hz == 0 || self.initialization_frequency_hz > 400_000 {
            return Err(ConfigError::InitializationFrequency);
        }
        if self.transfer_frequency_hz < self.initialization_frequency_hz
            || self.transfer_frequency_hz > 25_000_000
        {
            return Err(ConfigError::TransferFrequency);
        }
        if self.reset_attempts == 0 || self.operation_condition_attempts == 0 {
            return Err(ConfigError::InitializationAttempts);
        }
        if self.response_poll_bytes == 0
            || self.data_token_poll_bytes == 0
            || self.busy_poll_bytes == 0
        {
            return Err(ConfigError::PollLimit);
        }
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::DEFAULT
    }
}

/// Static policy error detected without card I/O.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConfigError {
    /// Identification SPI clock was zero or exceeded 400 kHz.
    InitializationFrequency,
    /// Data clock was below identification speed or exceeded default SD speed.
    TransferFrequency,
    /// A required initialization retry count was zero.
    InitializationAttempts,
    /// A response, token, or busy bound was zero.
    PollLimit,
}

/// Address interpretation established from OCR capacity status.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Addressing {
    /// SDSC byte addresses; CMD16 fixes the transfer block to 512 bytes.
    Byte,
    /// SDHC/SDXC 512-byte block addresses.
    Block,
}

/// Immutable card facts established during one successful initialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CardInfo {
    /// Address argument interpretation for data commands.
    pub addressing: Addressing,
    /// Exact number of complete addressable 512-byte blocks derived from CSD.
    pub block_count: u64,
    /// OCR returned after ACMD41 completed.
    pub ocr: u32,
    /// CSD structure field: zero for V1, one for V2 high-capacity layout.
    pub csd_structure: u8,
}

/// Coarse driver lifecycle. Any failed block mutation requires reinitialization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    /// No complete card identity is trusted.
    Uninitialized,
    /// Card identity, addressing, capacity, and CRC mode were established.
    Ready(CardInfo),
    /// A transport/protocol operation failed; no further block I/O is admitted.
    Faulted,
}

/// Bounded wait which expired.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Timeout {
    /// No valid R1 byte arrived within the command-response bound.
    CommandResponse { command: u8 },
    /// Card did not accept reset into SPI idle state.
    Reset,
    /// Card did not leave idle during ACMD41 initialization.
    OperationCondition,
    /// No start token arrived for a register or data block.
    DataToken,
    /// Card did not emit a write-data response token.
    DataResponse,
    /// Card remained busy beyond the configured programming bound.
    Busy,
}

/// Exact SD SPI transport failure.
#[derive(Debug, Eq, PartialEq)]
pub enum Error<BusError, PinError, BusConfigError> {
    /// Underlying SPI transfer or flush failed.
    Bus(BusError),
    /// Chip select could not be asserted or safely released.
    ChipSelect(PinError),
    /// HAL rejected identification or transfer frequency.
    BusConfiguration(BusConfigError),
    /// Static bounded policy was invalid.
    Config(ConfigError),
    /// Block I/O was attempted before successful initialization.
    NotInitialized,
    /// A prior failure requires explicit reinitialization.
    Faulted,
    /// A bounded protocol wait expired.
    Timeout(Timeout),
    /// R1 contained a state/error value other than the exact expected value.
    UnexpectedR1 {
        /// Command index producing the response.
        command: u8,
        /// Expected exact R1 byte.
        expected: u8,
        /// Observed R1 byte.
        observed: u8,
    },
    /// CMD8 voltage/check-pattern echo was not the requested 0x1aa.
    InterfaceCondition(u32),
    /// OCR was not powered up or did not advertise the 2.7–3.6 V window.
    OperatingConditions(u32),
    /// CSD structure or capacity fields were unsupported or impossible.
    Csd,
    /// Read payload CRC-16 did not match the card's CRC bytes.
    DataCrc {
        /// Locally computed CRC-16/CCITT value.
        expected: u16,
        /// CRC returned following the data payload.
        observed: u16,
    },
    /// A non-start read-data token was returned.
    DataToken(u8),
    /// Write-data response was not `accepted`.
    DataResponse(u8),
    /// CMD13 reported a nonzero first or second status byte.
    CardStatus { r1: u8, status: u8 },
    /// Requested block or byte-address conversion escaped the card.
    Address,
    /// Card drove an impossible nonzero byte while expected to be busy.
    BusyValue(u8),
}

/// Concrete driver error projected from one bus and chip-select type.
pub type SdSpiError<BUS, CS> = Error<
    <BUS as embedded_hal::spi::ErrorType>::Error,
    <CS as embedded_hal::digital::ErrorType>::Error,
    <BUS as ReconfigurableSpiBus>::ConfigError,
>;

/// Exclusive, async SD SPI-mode block transport.
pub struct SdSpiCard<BUS, CS, DELAY> {
    bus: BUS,
    chip_select: CS,
    delay: DELAY,
    config: Config,
    state: State,
}

impl<BUS, CS, DELAY> SdSpiCard<BUS, CS, DELAY>
where
    BUS: ReconfigurableSpiBus,
    CS: OutputPin,
    DELAY: DelayNs,
{
    /// Creates an uninitialized driver and immediately makes chip select idle.
    pub fn new(
        bus: BUS,
        mut chip_select: CS,
        delay: DELAY,
        config: Config,
    ) -> Result<Self, SdSpiError<BUS, CS>> {
        config.validate().map_err(Error::Config)?;
        chip_select.set_high().map_err(Error::ChipSelect)?;
        Ok(Self {
            bus,
            chip_select,
            delay,
            config,
            state: State::Uninitialized,
        })
    }

    /// Current lifecycle without I/O.
    pub const fn state(&self) -> State {
        self.state
    }

    /// Trusted card facts only after complete initialization.
    pub const fn card_info(&self) -> Option<CardInfo> {
        match self.state {
            State::Ready(info) => Some(info),
            State::Uninitialized | State::Faulted => None,
        }
    }

    /// Releases the owned bus, chip select, and delay provider.
    pub fn into_parts(self) -> (BUS, CS, DELAY) {
        (self.bus, self.chip_select, self.delay)
    }

    /// Enters SPI mode, negotiates SD V2 conditions, enables CRC, and reads CSD.
    ///
    /// Legacy CMD8-illegal cards are rejected for the first hardware milestone.
    pub async fn initialize(&mut self) -> Result<CardInfo, SdSpiError<BUS, CS>> {
        self.state = State::Uninitialized;
        let result = self.initialize_inner().await;
        match result {
            Ok(info) => {
                self.state = State::Ready(info);
                Ok(info)
            }
            Err(error) => {
                self.state = State::Faulted;
                Err(error)
            }
        }
    }

    async fn initialize_inner(&mut self) -> Result<CardInfo, SdSpiError<BUS, CS>> {
        self.bus.flush().await.map_err(Error::Bus)?;
        self.bus
            .set_frequency_hz(self.config.initialization_frequency_hz)
            .map_err(Error::BusConfiguration)?;
        self.chip_select.set_high().map_err(Error::ChipSelect)?;
        let mut entry = [0xff; SPI_MODE_ENTRY_BYTES];
        self.transfer(&mut entry).await?;
        self.bus.flush().await.map_err(Error::Bus)?;

        let mut entered_idle = false;
        for _ in 0..self.config.reset_attempts {
            match self.command_r1(CMD0_GO_IDLE_STATE, 0).await {
                Ok(R1_IDLE) => {
                    entered_idle = true;
                    break;
                }
                Ok(_) | Err(Error::Timeout(Timeout::CommandResponse { .. })) => {}
                Err(error) => return Err(error),
            }
        }
        if !entered_idle {
            return Err(Error::Timeout(Timeout::Reset));
        }

        let (r1, interface_condition) = self
            .command_extended(CMD8_SEND_IF_COND, CMD8_ARGUMENT)
            .await?;
        if r1 & R1_ILLEGAL_COMMAND != 0 {
            return Err(Error::UnexpectedR1 {
                command: CMD8_SEND_IF_COND,
                expected: R1_IDLE,
                observed: r1,
            });
        }
        require_r1(CMD8_SEND_IF_COND, r1, R1_IDLE)?;
        if interface_condition != CMD8_ARGUMENT {
            return Err(Error::InterfaceCondition(interface_condition));
        }

        let mut initialized = false;
        for _ in 0..self.config.operation_condition_attempts {
            let app = self.command_r1(CMD55_APP_CMD, 0).await?;
            if app != R1_IDLE && app != R1_READY {
                return Err(Error::UnexpectedR1 {
                    command: CMD55_APP_CMD,
                    expected: R1_IDLE,
                    observed: app,
                });
            }
            let condition = self.command_r1(ACMD41_SD_SEND_OP_COND, ACMD41_HCS).await?;
            if condition == R1_READY {
                initialized = true;
                break;
            }
            require_r1(ACMD41_SD_SEND_OP_COND, condition, R1_IDLE)?;
            self.delay
                .delay_ns(self.config.operation_condition_delay_ns)
                .await;
        }
        if !initialized {
            return Err(Error::Timeout(Timeout::OperationCondition));
        }

        let (r1, ocr) = self.command_extended(CMD58_READ_OCR, 0).await?;
        require_r1(CMD58_READ_OCR, r1, R1_READY)?;
        if ocr & OCR_POWER_UP == 0 || ocr & OCR_27_TO_36_VOLTS == 0 {
            return Err(Error::OperatingConditions(ocr));
        }
        let addressing = if ocr & OCR_CAPACITY_STATUS == 0 {
            Addressing::Byte
        } else {
            Addressing::Block
        };

        let crc_response = self.command_r1(CMD59_CRC_ON_OFF, 1).await?;
        require_r1(CMD59_CRC_ON_OFF, crc_response, R1_READY)?;

        if addressing == Addressing::Byte {
            let block_len = self.command_r1(CMD16_SET_BLOCKLEN, 512).await?;
            require_r1(CMD16_SET_BLOCKLEN, block_len, R1_READY)?;
        }

        let mut csd = [0_u8; 16];
        self.read_data_command(CMD9_SEND_CSD, 0, &mut csd).await?;
        let (csd_structure, block_count) = parse_csd(&csd)?;
        let addressing_matches_csd = matches!(
            (addressing, csd_structure),
            (Addressing::Byte, 0) | (Addressing::Block, 1)
        );
        let byte_address_capacity_fits = addressing != Addressing::Byte
            || block_count
                .checked_mul(512)
                .is_some_and(|bytes| bytes <= u64::from(u32::MAX) + 1);
        if block_count == 0 || !addressing_matches_csd || !byte_address_capacity_fits {
            return Err(Error::Csd);
        }

        self.bus.flush().await.map_err(Error::Bus)?;
        self.bus
            .set_frequency_hz(self.config.transfer_frequency_hz)
            .map_err(Error::BusConfiguration)?;
        Ok(CardInfo {
            addressing,
            block_count,
            ocr,
            csd_structure,
        })
    }

    /// Reads and CRC-verifies exactly one 512-byte block.
    pub async fn read_block(
        &mut self,
        block: u64,
        output: &mut MediaBlock,
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let result = self.read_block_inner(block, output).await;
        if result.is_err() {
            self.state = State::Faulted;
        }
        result
    }

    async fn read_block_inner(
        &mut self,
        block: u64,
        output: &mut MediaBlock,
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let info = self.ready_info()?;
        let address = block_argument(info, block)?;
        self.read_data_command(CMD17_READ_SINGLE_BLOCK, address, output)
            .await
    }

    /// Writes one block, waits through programming, and validates CMD13 status.
    pub async fn write_block(
        &mut self,
        block: u64,
        data: &MediaBlock,
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let result = self.write_block_inner(block, data).await;
        if result.is_err() {
            self.state = State::Faulted;
        }
        result
    }

    async fn write_block_inner(
        &mut self,
        block: u64,
        data: &MediaBlock,
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let info = self.ready_info()?;
        let address = block_argument(info, block)?;
        let result = match self.select().await {
            Ok(()) => self.write_selected(address, data).await,
            Err(error) => Err(error),
        };
        self.finish_selected(result).await?;
        self.card_status().await
    }

    async fn write_selected(
        &mut self,
        address: u32,
        data: &MediaBlock,
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let response = self.command_selected(CMD24_WRITE_BLOCK, address).await?;
        require_r1(CMD24_WRITE_BLOCK, response, R1_READY)?;
        self.bus
            .write(&[DATA_START_TOKEN])
            .await
            .map_err(Error::Bus)?;
        self.bus.write(data).await.map_err(Error::Bus)?;
        self.bus
            .write(&crc16(data).to_be_bytes())
            .await
            .map_err(Error::Bus)?;
        let response = self.wait_data_response().await?;
        if response & DATA_RESPONSE_MASK != DATA_RESPONSE_ACCEPTED {
            return Err(Error::DataResponse(response));
        }
        self.wait_ready().await
    }

    /// Confirms the card is not busy and reports clean R2 status.
    pub async fn sync(&mut self) -> Result<(), SdSpiError<BUS, CS>> {
        let result = match self.state {
            State::Ready(_) => self.card_status().await,
            State::Uninitialized => Err(Error::NotInitialized),
            State::Faulted => Err(Error::Faulted),
        };
        if result.is_err() {
            self.state = State::Faulted;
        }
        result
    }

    fn ready_info(&self) -> Result<CardInfo, SdSpiError<BUS, CS>> {
        match self.state {
            State::Ready(info) => Ok(info),
            State::Uninitialized => Err(Error::NotInitialized),
            State::Faulted => Err(Error::Faulted),
        }
    }

    async fn command_r1(&mut self, command: u8, argument: u32) -> Result<u8, SdSpiError<BUS, CS>> {
        let result = match self.select().await {
            Ok(()) => self.command_selected(command, argument).await,
            Err(error) => Err(error),
        };
        self.finish_selected(result).await
    }

    async fn command_extended(
        &mut self,
        command: u8,
        argument: u32,
    ) -> Result<(u8, u32), SdSpiError<BUS, CS>> {
        let result = match self.select().await {
            Ok(()) => match self.command_selected(command, argument).await {
                Ok(r1) => {
                    let mut extension = [0xff; 4];
                    match self.transfer(&mut extension).await {
                        Ok(()) => Ok((r1, u32::from_be_bytes(extension))),
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        self.finish_selected(result).await
    }

    async fn read_data_command(
        &mut self,
        command: u8,
        argument: u32,
        output: &mut [u8],
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let result = match self.select().await {
            Ok(()) => match self.command_selected(command, argument).await {
                Ok(response) => match require_r1(command, response, R1_READY) {
                    Ok(()) => self.read_data_payload(output).await,
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        self.finish_selected(result).await
    }

    async fn card_status(&mut self) -> Result<(), SdSpiError<BUS, CS>> {
        let result = match self.select().await {
            Ok(()) => match self.command_selected(CMD13_SEND_STATUS, 0).await {
                Ok(r1) => {
                    let mut second = [0xff];
                    match self.transfer(&mut second).await {
                        Ok(()) if r1 == R1_READY && second[0] == 0 => Ok(()),
                        Ok(()) => Err(Error::CardStatus {
                            r1,
                            status: second[0],
                        }),
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
            Err(error) => Err(error),
        };
        self.finish_selected(result).await
    }

    async fn command_selected(
        &mut self,
        command: u8,
        argument: u32,
    ) -> Result<u8, SdSpiError<BUS, CS>> {
        let mut packet = command_packet(command, argument);
        self.transfer(&mut packet).await?;
        for _ in 0..self.config.response_poll_bytes {
            let mut response = [0xff];
            self.transfer(&mut response).await?;
            if response[0] & 0x80 == 0 {
                return Ok(response[0]);
            }
        }
        Err(Error::Timeout(Timeout::CommandResponse { command }))
    }

    async fn read_data_payload(&mut self, output: &mut [u8]) -> Result<(), SdSpiError<BUS, CS>> {
        let mut polled = 0_u32;
        let mut poll = [0xff; POLL_BUFFER_BYTES];
        while polled < self.config.data_token_poll_bytes {
            let remaining = self.config.data_token_poll_bytes - polled;
            let count = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(POLL_BUFFER_BYTES);
            poll[..count].fill(0xff);
            self.transfer(&mut poll[..count]).await?;
            polled = polled.saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
            if let Some(position) = poll[..count].iter().position(|byte| *byte != 0xff) {
                let token = poll[position];
                if token != DATA_START_TOKEN {
                    return Err(Error::DataToken(token));
                }
                return self
                    .finish_payload_from_prefetch(output, &poll[position + 1..count])
                    .await;
            }
        }
        Err(Error::Timeout(Timeout::DataToken))
    }

    async fn finish_payload_from_prefetch(
        &mut self,
        output: &mut [u8],
        prefetched: &[u8],
    ) -> Result<(), SdSpiError<BUS, CS>> {
        let total = output.len().checked_add(2).ok_or(Error::Csd)?;
        let mut received = 0_usize;
        let mut observed_crc = [0_u8; 2];
        for byte in prefetched.iter().copied().take(total) {
            place_payload_byte(output, &mut observed_crc, received, byte);
            received += 1;
        }
        let mut buffer = [0xff; POLL_BUFFER_BYTES];
        while received < total {
            let count = (total - received).min(POLL_BUFFER_BYTES);
            buffer[..count].fill(0xff);
            self.transfer(&mut buffer[..count]).await?;
            for byte in buffer[..count].iter().copied() {
                place_payload_byte(output, &mut observed_crc, received, byte);
                received += 1;
            }
        }
        let expected = crc16(output);
        let observed = u16::from_be_bytes(observed_crc);
        if expected != observed {
            return Err(Error::DataCrc { expected, observed });
        }
        Ok(())
    }

    async fn wait_data_response(&mut self) -> Result<u8, SdSpiError<BUS, CS>> {
        for _ in 0..self.config.response_poll_bytes {
            let mut byte = [0xff];
            self.transfer(&mut byte).await?;
            if byte[0] != 0xff {
                return Ok(byte[0]);
            }
        }
        Err(Error::Timeout(Timeout::DataResponse))
    }

    async fn select(&mut self) -> Result<(), SdSpiError<BUS, CS>> {
        self.chip_select.set_low().map_err(Error::ChipSelect)?;
        self.wait_ready().await
    }

    async fn wait_ready(&mut self) -> Result<(), SdSpiError<BUS, CS>> {
        let mut polled = 0_u32;
        let mut buffer = [0xff; POLL_BUFFER_BYTES];
        while polled < self.config.busy_poll_bytes {
            let remaining = self.config.busy_poll_bytes - polled;
            let count = usize::try_from(remaining)
                .unwrap_or(usize::MAX)
                .min(POLL_BUFFER_BYTES);
            buffer[..count].fill(0xff);
            self.transfer(&mut buffer[..count]).await?;
            polled = polled.saturating_add(u32::try_from(count).unwrap_or(u32::MAX));
            if buffer[..count].contains(&0xff) {
                return Ok(());
            }
            if let Some(value) = buffer[..count].iter().copied().find(|byte| *byte != 0) {
                return Err(Error::BusyValue(value));
            }
        }
        Err(Error::Timeout(Timeout::Busy))
    }

    async fn finish_selected<T>(
        &mut self,
        result: Result<T, SdSpiError<BUS, CS>>,
    ) -> Result<T, SdSpiError<BUS, CS>> {
        let flush = self.bus.flush().await.map_err(Error::Bus);
        let release = self.chip_select.set_high().map_err(Error::ChipSelect);
        let trailing = if release.is_ok() {
            let mut byte = [0xff];
            match self.transfer(&mut byte).await {
                Ok(()) => self.bus.flush().await.map_err(Error::Bus),
                Err(error) => Err(error),
            }
        } else {
            Ok(())
        };
        release?;
        flush?;
        trailing?;
        result
    }

    async fn transfer(&mut self, bytes: &mut [u8]) -> Result<(), SdSpiError<BUS, CS>> {
        self.bus.transfer_in_place(bytes).await.map_err(Error::Bus)
    }
}

impl<BUS, CS, DELAY> AsyncBlockDevice for SdSpiCard<BUS, CS, DELAY>
where
    BUS: ReconfigurableSpiBus,
    CS: OutputPin,
    DELAY: DelayNs,
{
    type Error = SdSpiError<BUS, CS>;

    fn block_count(&self) -> u64 {
        self.card_info().map_or(0, |info| info.block_count)
    }

    async fn read_block(&mut self, block: u64, output: &mut MediaBlock) -> Result<(), Self::Error> {
        SdSpiCard::read_block(self, block, output).await
    }

    async fn write_block(&mut self, block: u64, data: &MediaBlock) -> Result<(), Self::Error> {
        SdSpiCard::write_block(self, block, data).await
    }

    async fn sync(&mut self) -> Result<(), Self::Error> {
        SdSpiCard::sync(self).await
    }
}

fn require_r1<BusError, PinError, BusConfigError>(
    command: u8,
    observed: u8,
    expected: u8,
) -> Result<(), Error<BusError, PinError, BusConfigError>> {
    if observed == expected {
        Ok(())
    } else {
        Err(Error::UnexpectedR1 {
            command,
            expected,
            observed,
        })
    }
}

fn block_argument<BusError, PinError, BusConfigError>(
    info: CardInfo,
    block: u64,
) -> Result<u32, Error<BusError, PinError, BusConfigError>> {
    if block >= info.block_count {
        return Err(Error::Address);
    }
    match info.addressing {
        Addressing::Block => u32::try_from(block).map_err(|_| Error::Address),
        Addressing::Byte => block
            .checked_mul(512)
            .and_then(|address| u32::try_from(address).ok())
            .ok_or(Error::Address),
    }
}

fn place_payload_byte(output: &mut [u8], crc: &mut [u8; 2], index: usize, byte: u8) {
    if index < output.len() {
        output[index] = byte;
    } else if let Some(target) = crc.get_mut(index - output.len()) {
        *target = byte;
    }
}

fn command_packet(command: u8, argument: u32) -> [u8; 6] {
    let argument = argument.to_be_bytes();
    let mut packet = [
        0x40 | command,
        argument[0],
        argument[1],
        argument[2],
        argument[3],
        0,
    ];
    packet[5] = (crc7(&packet[..5]) << 1) | 1;
    packet
}

/// SD command CRC-7 using polynomial x^7 + x^3 + 1.
pub fn crc7(bytes: &[u8]) -> u8 {
    let mut crc = 0_u8;
    for byte in bytes {
        let mut value = *byte;
        for _ in 0..8 {
            crc <<= 1;
            if (value ^ crc) & 0x80 != 0 {
                crc ^= 0x09;
            }
            value <<= 1;
        }
    }
    crc & 0x7f
}

/// SD data CRC-16/CCITT using polynomial x^16 + x^12 + x^5 + 1.
pub fn crc16(bytes: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in bytes {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn parse_csd<BusError, PinError, BusConfigError>(
    csd: &[u8; 16],
) -> Result<(u8, u64), Error<BusError, PinError, BusConfigError>> {
    let structure = u8::try_from(csd_bits(csd, 127, 126)).map_err(|_| Error::Csd)?;
    let blocks = match structure {
        1 => {
            let c_size = u64::from(csd_bits(csd, 69, 48));
            c_size
                .checked_add(1)
                .and_then(|value| value.checked_mul(1_024))
                .ok_or(Error::Csd)?
        }
        0 => {
            let read_block_len = csd_bits(csd, 83, 80);
            let c_size = u64::from(csd_bits(csd, 73, 62));
            let c_size_mult = csd_bits(csd, 49, 47);
            if read_block_len > 11 || c_size_mult > 7 {
                return Err(Error::Csd);
            }
            let block_len = 1_u64.checked_shl(read_block_len).ok_or(Error::Csd)?;
            let multiplier = 1_u64.checked_shl(c_size_mult + 2).ok_or(Error::Csd)?;
            let bytes = c_size
                .checked_add(1)
                .and_then(|value| value.checked_mul(multiplier))
                .and_then(|value| value.checked_mul(block_len))
                .ok_or(Error::Csd)?;
            if !bytes.is_multiple_of(512) {
                return Err(Error::Csd);
            }
            bytes / 512
        }
        _ => return Err(Error::Csd),
    };
    Ok((structure, blocks))
}

fn csd_bits(csd: &[u8; 16], most_significant: u32, least_significant: u32) -> u32 {
    let mut value = 0_u32;
    let mut bit = most_significant;
    loop {
        let byte_index = usize::try_from((127 - bit) / 8).expect("CSD bit index fits usize");
        let bit_index = bit % 8;
        value = (value << 1) | u32::from((csd[byte_index] >> bit_index) & 1);
        if bit == least_significant {
            break;
        }
        bit -= 1;
    }
    value
}

#[cfg(test)]
#[allow(
    clippy::std_instead_of_core,
    reason = "the semantic SD card and inspection controls are host-only"
)]
mod tests {
    extern crate std;

    use core::cell::{Cell, RefCell};
    use core::convert::Infallible;
    use std::boxed::Box;
    use std::collections::{BTreeMap, VecDeque};
    use std::rc::Rc;
    use std::vec::Vec;

    use alumina_storage::media::{CacheMedia, MediaAvailability, MediaId, MediaRegion};
    use alumina_storage::{
        CacheLimits, ChunkUploadHeader, FinalizeUploadRequest, ManifestHasher, MutationContext,
        ObjectKind, StoredObject, UploadId, UploadPlan, sha256,
    };
    use embassy_futures::block_on;
    use embedded_hal::spi::{Error as _, ErrorKind, ErrorType};

    use super::*;

    const TEST_BLOCKS: u64 = 4_096;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FakeBusError {
        Transfer,
        Protocol,
    }

    impl embedded_hal::spi::Error for FakeBusError {
        fn kind(&self) -> ErrorKind {
            ErrorKind::Other
        }
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum FakeConfigError {
        Frequency,
    }

    #[derive(Clone, Debug, Eq, PartialEq)]
    enum WritePhase {
        Token { block: u32 },
        Data { block: u32 },
        Crc { block: u32, data: Box<MediaBlock> },
    }

    struct FakeCard {
        frequencies: Vec<u32>,
        commands: Vec<(u8, u32)>,
        responses: VecDeque<u8>,
        blocks: BTreeMap<u32, MediaBlock>,
        idle: bool,
        application_command: bool,
        acmd41_idle_responses: u16,
        ocr: u32,
        csd: [u8; 16],
        crc_enabled: bool,
        corrupt_next_read_crc: bool,
        next_write_response: Option<u8>,
        busy_after_write: u32,
        busy_remaining: u32,
        write_phase: Option<WritePhase>,
        fail_next_transfer: bool,
        preclock_bytes: usize,
        cmd8_echo: u32,
    }

    impl FakeCard {
        fn high_capacity() -> Self {
            let mut csd = [0_u8; 16];
            set_csd_bits(&mut csd, 127, 126, 1);
            set_csd_bits(
                &mut csd,
                69,
                48,
                u32::try_from(TEST_BLOCKS / 1_024 - 1).unwrap(),
            );
            Self {
                frequencies: Vec::new(),
                commands: Vec::new(),
                responses: VecDeque::new(),
                blocks: BTreeMap::new(),
                idle: true,
                application_command: false,
                acmd41_idle_responses: 1,
                ocr: OCR_POWER_UP | OCR_CAPACITY_STATUS | OCR_27_TO_36_VOLTS,
                csd,
                crc_enabled: false,
                corrupt_next_read_crc: false,
                next_write_response: None,
                busy_after_write: 5,
                busy_remaining: 0,
                write_phase: None,
                fail_next_transfer: false,
                preclock_bytes: 0,
                cmd8_echo: CMD8_ARGUMENT,
            }
        }

        fn queue(&mut self, bytes: &[u8]) {
            self.responses.extend(bytes.iter().copied());
        }

        fn handle_command(&mut self, packet: [u8; 6]) -> Result<(), FakeBusError> {
            if packet[0] & 0xc0 != 0x40 || packet[5] != (crc7(&packet[..5]) << 1) | 1 {
                return Err(FakeBusError::Protocol);
            }
            let command = packet[0] & 0x3f;
            let argument = u32::from_be_bytes([packet[1], packet[2], packet[3], packet[4]]);
            self.commands.push((command, argument));
            match command {
                CMD0_GO_IDLE_STATE => {
                    self.idle = true;
                    self.application_command = false;
                    self.queue(&[R1_IDLE]);
                }
                CMD8_SEND_IF_COND => {
                    let echo = self.cmd8_echo.to_be_bytes();
                    self.queue(&[R1_IDLE, echo[0], echo[1], echo[2], echo[3]]);
                }
                CMD55_APP_CMD => {
                    self.application_command = true;
                    self.queue(&[if self.idle { R1_IDLE } else { R1_READY }]);
                }
                ACMD41_SD_SEND_OP_COND if self.application_command => {
                    self.application_command = false;
                    if self.acmd41_idle_responses == 0 {
                        self.idle = false;
                        self.queue(&[R1_READY]);
                    } else {
                        self.acmd41_idle_responses -= 1;
                        self.queue(&[R1_IDLE]);
                    }
                }
                CMD58_READ_OCR => {
                    let ocr = self.ocr.to_be_bytes();
                    self.queue(&[R1_READY, ocr[0], ocr[1], ocr[2], ocr[3]]);
                }
                CMD59_CRC_ON_OFF => {
                    self.crc_enabled = argument == 1;
                    self.queue(&[R1_READY]);
                }
                CMD16_SET_BLOCKLEN => self.queue(&[R1_READY]),
                CMD9_SEND_CSD => {
                    let csd = self.csd;
                    let crc = crc16(&csd).to_be_bytes();
                    self.queue(&[R1_READY, 0xff, DATA_START_TOKEN]);
                    self.queue(&csd);
                    self.queue(&crc);
                }
                CMD17_READ_SINGLE_BLOCK => {
                    let block = argument;
                    if u64::from(block) >= TEST_BLOCKS {
                        self.queue(&[0x20]);
                    } else {
                        let data = self.blocks.get(&block).copied().unwrap_or([0; 512]);
                        let mut crc = crc16(&data);
                        if self.corrupt_next_read_crc {
                            self.corrupt_next_read_crc = false;
                            crc ^= 1;
                        }
                        self.queue(&[R1_READY, 0xff, DATA_START_TOKEN]);
                        self.queue(&data);
                        self.queue(&crc.to_be_bytes());
                    }
                }
                CMD24_WRITE_BLOCK => {
                    if u64::from(argument) >= TEST_BLOCKS {
                        self.queue(&[0x20]);
                    } else {
                        self.write_phase = Some(WritePhase::Token { block: argument });
                        self.queue(&[R1_READY]);
                    }
                }
                CMD13_SEND_STATUS => self.queue(&[R1_READY, 0]),
                _ => self.queue(&[R1_ILLEGAL_COMMAND]),
            }
            Ok(())
        }

        fn handle_write(&mut self, words: &[u8]) -> Result<(), FakeBusError> {
            let phase = self.write_phase.take().ok_or(FakeBusError::Protocol)?;
            match phase {
                WritePhase::Token { block } if words == [DATA_START_TOKEN] => {
                    self.write_phase = Some(WritePhase::Data { block });
                }
                WritePhase::Data { block } if words.len() == 512 => {
                    let mut data = [0_u8; 512];
                    data.copy_from_slice(words);
                    self.write_phase = Some(WritePhase::Crc {
                        block,
                        data: Box::new(data),
                    });
                }
                WritePhase::Crc { block, data } if words.len() == 2 => {
                    let observed = u16::from_be_bytes([words[0], words[1]]);
                    let response = self.next_write_response.take().unwrap_or_else(|| {
                        if observed == crc16(data.as_slice()) {
                            DATA_RESPONSE_ACCEPTED
                        } else {
                            0x0b
                        }
                    });
                    if response & DATA_RESPONSE_MASK == DATA_RESPONSE_ACCEPTED {
                        self.blocks.insert(block, *data);
                    }
                    self.queue(&[response]);
                    self.busy_remaining = self.busy_after_write;
                }
                _ => return Err(FakeBusError::Protocol),
            }
            Ok(())
        }
    }

    #[derive(Clone)]
    struct FakeControl {
        card: Rc<RefCell<FakeCard>>,
        selected: Rc<Cell<bool>>,
        delay_calls: Rc<Cell<u32>>,
    }

    impl FakeControl {
        fn set_block(&self, block: u32, data: MediaBlock) {
            self.card.borrow_mut().blocks.insert(block, data);
        }

        fn block(&self, block: u32) -> Option<MediaBlock> {
            self.card.borrow().blocks.get(&block).copied()
        }
    }

    struct FakeBus {
        card: Rc<RefCell<FakeCard>>,
        selected: Rc<Cell<bool>>,
    }

    impl ErrorType for FakeBus {
        type Error = FakeBusError;
    }

    impl FakeBus {
        fn check_failure(&self) -> Result<(), FakeBusError> {
            let mut card = self.card.borrow_mut();
            if card.fail_next_transfer {
                card.fail_next_transfer = false;
                Err(FakeBusError::Transfer)
            } else {
                Ok(())
            }
        }

        fn exchange(&mut self, words: &mut [u8]) -> Result<(), FakeBusError> {
            self.check_failure()?;
            let selected = self.selected.get();
            if !selected {
                let mut card = self.card.borrow_mut();
                card.preclock_bytes = card.preclock_bytes.saturating_add(words.len());
                card.responses.clear();
                words.fill(0xff);
                return Ok(());
            }

            if words.len() == 6 && words[0] & 0xc0 == 0x40 {
                let packet: [u8; 6] = (*words).try_into().map_err(|_| FakeBusError::Protocol)?;
                words.fill(0xff);
                return self.card.borrow_mut().handle_command(packet);
            }

            let mut card = self.card.borrow_mut();
            for word in words {
                *word = if let Some(response) = card.responses.pop_front() {
                    response
                } else if card.busy_remaining != 0 {
                    card.busy_remaining -= 1;
                    0
                } else {
                    0xff
                };
            }
            Ok(())
        }
    }

    impl SpiBus<u8> for FakeBus {
        async fn read(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            words.fill(0xff);
            self.exchange(words)
        }

        async fn write(&mut self, words: &[u8]) -> Result<(), Self::Error> {
            self.check_failure()?;
            if !self.selected.get() {
                return Err(FakeBusError::Protocol);
            }
            self.card.borrow_mut().handle_write(words)
        }

        async fn transfer(&mut self, read: &mut [u8], write: &[u8]) -> Result<(), Self::Error> {
            if read.len() != write.len() {
                return Err(FakeBusError::Protocol);
            }
            read.copy_from_slice(write);
            self.exchange(read)
        }

        async fn transfer_in_place(&mut self, words: &mut [u8]) -> Result<(), Self::Error> {
            self.exchange(words)
        }

        async fn flush(&mut self) -> Result<(), Self::Error> {
            self.check_failure()
        }
    }

    impl ReconfigurableSpiBus for FakeBus {
        type ConfigError = FakeConfigError;

        fn set_frequency_hz(&mut self, frequency_hz: u32) -> Result<(), Self::ConfigError> {
            if frequency_hz == 0 {
                Err(FakeConfigError::Frequency)
            } else {
                self.card.borrow_mut().frequencies.push(frequency_hz);
                Ok(())
            }
        }
    }

    struct FakePin {
        selected: Rc<Cell<bool>>,
    }

    impl embedded_hal::digital::ErrorType for FakePin {
        type Error = Infallible;
    }

    impl OutputPin for FakePin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.selected.set(true);
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.selected.set(false);
            Ok(())
        }
    }

    struct FakeDelay {
        calls: Rc<Cell<u32>>,
    }

    impl DelayNs for FakeDelay {
        async fn delay_ns(&mut self, _ns: u32) {
            self.calls.set(self.calls.get().saturating_add(1));
        }
    }

    fn test_config() -> Config {
        Config {
            initialization_frequency_hz: 400_000,
            transfer_frequency_hz: 8_000_000,
            reset_attempts: 2,
            operation_condition_attempts: 4,
            operation_condition_delay_ns: 1,
            response_poll_bytes: 8,
            data_token_poll_bytes: 64,
            busy_poll_bytes: 64,
        }
    }

    fn fake_driver() -> (SdSpiCard<FakeBus, FakePin, FakeDelay>, FakeControl) {
        let card = Rc::new(RefCell::new(FakeCard::high_capacity()));
        let selected = Rc::new(Cell::new(false));
        let delay_calls = Rc::new(Cell::new(0));
        let bus = FakeBus {
            card: card.clone(),
            selected: selected.clone(),
        };
        let pin = FakePin {
            selected: selected.clone(),
        };
        let delay = FakeDelay {
            calls: delay_calls.clone(),
        };
        let control = FakeControl {
            card,
            selected,
            delay_calls,
        };
        (
            SdSpiCard::new(bus, pin, delay, test_config()).unwrap(),
            control,
        )
    }

    fn set_csd_bits(csd: &mut [u8; 16], most_significant: u32, least_significant: u32, value: u32) {
        let mut bit = least_significant;
        loop {
            let source_bit = bit - least_significant;
            let byte_index = usize::try_from((127 - bit) / 8).unwrap();
            let bit_index = bit % 8;
            let mask = 1_u8 << bit_index;
            if value & (1 << source_bit) != 0 {
                csd[byte_index] |= mask;
            } else {
                csd[byte_index] &= !mask;
            }
            if bit == most_significant {
                break;
            }
            bit += 1;
        }
    }

    #[test]
    fn command_and_data_crc_match_standard_vectors() {
        assert_eq!(
            command_packet(CMD0_GO_IDLE_STATE, 0),
            [0x40, 0, 0, 0, 0, 0x95]
        );
        assert_eq!(
            command_packet(CMD8_SEND_IF_COND, CMD8_ARGUMENT),
            [0x48, 0, 0, 1, 0xaa, 0x87]
        );
        assert_eq!(crc16(b"123456789"), 0x31c3);
    }

    #[test]
    fn policy_rejects_unbounded_or_out_of_mode_configuration() {
        assert_eq!(
            Config {
                initialization_frequency_hz: 400_001,
                ..Config::DEFAULT
            }
            .validate(),
            Err(ConfigError::InitializationFrequency)
        );
        assert_eq!(
            Config {
                response_poll_bytes: 0,
                ..Config::DEFAULT
            }
            .validate(),
            Err(ConfigError::PollLimit)
        );
    }

    #[test]
    fn high_capacity_initialization_enables_crc_and_derives_csd_capacity() {
        let (mut driver, control) = fake_driver();
        let info = block_on(driver.initialize()).unwrap();
        assert_eq!(
            info,
            CardInfo {
                addressing: Addressing::Block,
                block_count: TEST_BLOCKS,
                ocr: OCR_POWER_UP | OCR_CAPACITY_STATUS | OCR_27_TO_36_VOLTS,
                csd_structure: 1,
            }
        );
        assert_eq!(driver.state(), State::Ready(info));
        let card = control.card.borrow();
        assert_eq!(card.frequencies, [400_000, 8_000_000]);
        assert!(card.crc_enabled);
        assert!(card.preclock_bytes >= SPI_MODE_ENTRY_BYTES);
        assert_eq!(control.delay_calls.get(), 1);
        assert!(card.commands.contains(&(CMD8_SEND_IF_COND, CMD8_ARGUMENT)));
        assert!(card.commands.contains(&(CMD9_SEND_CSD, 0)));
        assert!(!control.selected.get());
    }

    #[test]
    fn exact_block_read_write_and_sync_round_trip() {
        let (mut driver, control) = fake_driver();
        block_on(driver.initialize()).unwrap();
        let first = core::array::from_fn(|index| u8::try_from(index % 251).unwrap());
        control.set_block(7, first);
        let mut readback = [0_u8; 512];
        block_on(driver.read_block(7, &mut readback)).unwrap();
        assert_eq!(readback, first);

        let replacement = core::array::from_fn(|index| u8::try_from(255 - index % 251).unwrap());
        block_on(driver.write_block(7, &replacement)).unwrap();
        block_on(driver.sync()).unwrap();
        assert_eq!(control.block(7), Some(replacement));
        readback.fill(0);
        block_on(driver.read_block(7, &mut readback)).unwrap();
        assert_eq!(readback, replacement);
        assert!(!control.selected.get());
    }

    #[test]
    fn read_crc_failure_faults_until_explicit_reinitialization() {
        let (mut driver, control) = fake_driver();
        block_on(driver.initialize()).unwrap();
        control.card.borrow_mut().corrupt_next_read_crc = true;
        let mut output = [0_u8; 512];
        assert!(matches!(
            block_on(driver.read_block(0, &mut output)),
            Err(Error::DataCrc { .. })
        ));
        assert_eq!(driver.state(), State::Faulted);
        assert_eq!(
            block_on(driver.read_block(0, &mut output)),
            Err(Error::Faulted)
        );
        assert_eq!(
            block_on(driver.initialize()).unwrap().block_count,
            TEST_BLOCKS
        );
    }

    #[test]
    fn rejected_write_never_changes_media_and_faults_driver() {
        let (mut driver, control) = fake_driver();
        block_on(driver.initialize()).unwrap();
        control.card.borrow_mut().next_write_response = Some(0x0b);
        let data = [0xa5; 512];
        assert_eq!(
            block_on(driver.write_block(9, &data)),
            Err(Error::DataResponse(0x0b))
        );
        assert_eq!(control.block(9), None);
        assert_eq!(driver.state(), State::Faulted);
        assert!(!control.selected.get());
    }

    #[test]
    fn transport_failure_still_releases_chip_select() {
        let (mut driver, control) = fake_driver();
        block_on(driver.initialize()).unwrap();
        control.card.borrow_mut().fail_next_transfer = true;
        let mut output = [0_u8; 512];
        assert_eq!(
            block_on(driver.read_block(0, &mut output)),
            Err(Error::Bus(FakeBusError::Transfer))
        );
        assert!(!control.selected.get());
        assert_eq!(driver.state(), State::Faulted);
    }

    #[test]
    fn invalid_interface_echo_and_out_of_range_block_fail_closed() {
        let (mut driver, control) = fake_driver();
        control.card.borrow_mut().cmd8_echo = 0x1ab;
        assert_eq!(
            block_on(driver.initialize()),
            Err(Error::InterfaceCondition(0x1ab))
        );
        assert!(!control.selected.get());

        let (mut driver, _) = fake_driver();
        block_on(driver.initialize()).unwrap();
        let mut output = [0_u8; 512];
        assert_eq!(
            block_on(driver.read_block(TEST_BLOCKS, &mut output)),
            Err(Error::Address)
        );
    }

    #[test]
    fn version_one_csd_capacity_and_byte_address_are_checked() {
        let mut csd = [0_u8; 16];
        set_csd_bits(&mut csd, 127, 126, 0);
        set_csd_bits(&mut csd, 83, 80, 9);
        set_csd_bits(&mut csd, 73, 62, 1_023);
        set_csd_bits(&mut csd, 49, 47, 0);
        assert_eq!(parse_csd::<(), (), ()>(&csd), Ok((0, 4_096)));
        let info = CardInfo {
            addressing: Addressing::Byte,
            block_count: 4_096,
            ocr: OCR_POWER_UP | OCR_27_TO_36_VOLTS,
            csd_structure: 0,
        };
        assert_eq!(block_argument::<(), (), ()>(info, 3), Ok(1_536));
        assert_eq!(
            block_argument::<(), (), ()>(info, 4_096),
            Err(Error::Address)
        );
        assert_eq!(FakeBusError::Transfer.kind(), ErrorKind::Other);
    }

    #[test]
    fn initialized_transport_runs_the_real_cache_media_commit_protocol() {
        const LIMITS: CacheLimits = CacheLimits {
            maximum_object_bytes: 1_024,
            maximum_chunk_bytes: 1_024,
            maximum_chunks: 16,
        };
        const REGION: MediaRegion = MediaRegion {
            start_block: 8,
            block_count: 256,
        };

        let bytes = b"exact SD transport";
        let object = StoredObject {
            kind: ObjectKind::MachineJobPartition,
            content: sha256(bytes),
            byte_len: u64::try_from(bytes.len()).unwrap(),
        };
        let mut manifest = ManifestHasher::new(object, 1_024, 1, LIMITS).unwrap();
        manifest
            .push(0, sha256(bytes), u32::try_from(bytes.len()).unwrap())
            .unwrap();
        let plan = UploadPlan {
            upload_id: UploadId(9),
            object,
            manifest: manifest.finalize().unwrap(),
            chunk_bytes: 1_024,
            chunk_count: 1,
        };

        let (mut driver, _) = fake_driver();
        block_on(driver.initialize()).unwrap();
        let mut media = CacheMedia::new(driver, REGION, LIMITS);
        block_on(media.format(MediaId::new([0x5a; 16]).unwrap())).unwrap();
        block_on(media.begin_upload(plan, MutationContext::DISARMED_IDLE)).unwrap();
        block_on(media.put_chunk(
            ChunkUploadHeader {
                upload_id: plan.upload_id,
                index: 0,
                byte_len: u32::try_from(bytes.len()).unwrap(),
                content: sha256(bytes),
            },
            bytes,
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();
        block_on(media.finalize_upload(
            FinalizeUploadRequest {
                upload_id: plan.upload_id,
            },
            MutationContext::DISARMED_IDLE,
        ))
        .unwrap();

        let driver = media.into_device();
        let mut remounted = CacheMedia::new(driver, REGION, LIMITS);
        let status = block_on(remounted.mount()).unwrap();
        assert_eq!(status.availability, MediaAvailability::Ready);
        assert_eq!(status.published_objects, 1);
        assert_eq!(status.upload, None);
    }
}
