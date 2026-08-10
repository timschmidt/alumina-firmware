#![no_std]
#![doc = "Complete-image static shift-register transport for safe-output bootstrap."]
//!
//! This clean-room driver only models the three observable 74HC595-style
//! signals: serial data, shift clock, and storage-register latch. It performs
//! no peripheral register access and makes no assumptions about FluidNC or any
//! other firmware implementation. Board composition supplies the exact chain
//! width, physical cascade order, and complete safe image.

use embedded_hal::delay::DelayNs;
use embedded_hal::digital::OutputPin;

/// Order in which logical image bits enter the physical cascade.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BitOrder {
    /// Highest logical bit enters first; bit zero enters last.
    MostSignificantFirst,
    /// Bit zero enters first; highest logical bit enters last.
    LeastSignificantFirst,
}

/// One complete image for a chain containing at most 32 outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompleteImage {
    /// Exact number of shifted outputs.
    pub width: u8,
    /// Must contain every bit below `width` and no other bit.
    pub defined_mask: u32,
    /// Values to latch; bits outside `defined_mask` are forbidden.
    pub bits: u32,
    /// Physical cascade serialization order.
    pub order: BitOrder,
}

impl CompleteImage {
    /// Checks that every physical output has an explicit value.
    pub const fn validate(self) -> Result<(), ImageError> {
        let expected_mask = match self.width {
            0 => return Err(ImageError::Width { received: 0 }),
            1..=31 => (1_u32 << self.width) - 1,
            32 => u32::MAX,
            received => return Err(ImageError::Width { received }),
        };
        if self.defined_mask != expected_mask {
            return Err(ImageError::Incomplete {
                received_mask: self.defined_mask,
                expected_mask,
            });
        }
        if self.bits & !self.defined_mask != 0 {
            return Err(ImageError::BitsOutsideMask {
                bits: self.bits,
                defined_mask: self.defined_mask,
            });
        }
        Ok(())
    }
}

/// Minimum control-line timing requested from an embedded-hal delay provider.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Timing {
    /// Stable-data interval before each rising shift-clock edge.
    pub data_setup_ns: u32,
    /// Minimum high duration of every shift-clock pulse.
    pub clock_high_ns: u32,
    /// Minimum low duration between shift-clock pulses.
    pub clock_low_ns: u32,
    /// Minimum low latch setup before shifting begins.
    pub latch_setup_ns: u32,
    /// Minimum high duration of the commit latch pulse.
    pub latch_high_ns: u32,
}

impl Timing {
    /// Conservative bootstrap timing used by the first 74HC595 board target.
    pub const CONSERVATIVE_100NS: Self = Self {
        data_setup_ns: 100,
        clock_high_ns: 100,
        clock_low_ns: 100,
        latch_setup_ns: 100,
        latch_high_ns: 100,
    };

    /// Rejects zero-width pulses and accidental service-scale blocking waits.
    pub const fn validate(self) -> Result<(), TimingError> {
        let values = [
            self.data_setup_ns,
            self.clock_high_ns,
            self.clock_low_ns,
            self.latch_setup_ns,
            self.latch_high_ns,
        ];
        let mut index = 0;
        while index < values.len() {
            if values[index] == 0 || values[index] > 1_000_000 {
                return Err(TimingError {
                    field_index: index as u8,
                    received_ns: values[index],
                });
            }
            index += 1;
        }
        Ok(())
    }
}

/// Invalid bounded shift/latch timing field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimingError {
    /// Field order matches [`Timing`] declaration order, starting at zero.
    pub field_index: u8,
    /// Zero or more than the one-millisecond bootstrap ceiling.
    pub received_ns: u32,
}

/// Static complete-image validation failure detected before pin activity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImageError {
    /// Chain width was zero or exceeded the fixed 32-bit representation.
    Width { received: u8 },
    /// At least one physical output was omitted or an out-of-width bit was named.
    Incomplete {
        received_mask: u32,
        expected_mask: u32,
    },
    /// Image attempted to set a bit that the contract did not define.
    BitsOutsideMask { bits: u32, defined_mask: u32 },
}

/// Complete-image transaction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error<E> {
    /// Image was rejected before any control-line activity.
    Image(ImageError),
    /// Timing was rejected before any control-line activity.
    Timing(TimingError),
    /// A digital output operation failed.
    Pin(E),
}

/// Owned three-wire static shift-register transport.
///
/// Construction first forces latch low, then clock low, then data low. A
/// successful write leaves every control line low and retains the three pins so
/// no other task can reconfigure them while the latched image is authoritative.
pub struct StaticShiftRegister<CLOCK, DATA, LATCH, DELAY> {
    clock: CLOCK,
    data: DATA,
    latch: LATCH,
    delay: DELAY,
    latched_image: Option<CompleteImage>,
}

impl<CLOCK, DATA, LATCH, DELAY, E> StaticShiftRegister<CLOCK, DATA, LATCH, DELAY>
where
    CLOCK: OutputPin<Error = E>,
    DATA: OutputPin<Error = E>,
    LATCH: OutputPin<Error = E>,
    DELAY: DelayNs,
{
    /// Takes exclusive ownership and establishes inactive control-line levels.
    pub fn new(
        mut clock: CLOCK,
        mut data: DATA,
        mut latch: LATCH,
        delay: DELAY,
    ) -> Result<Self, Error<E>> {
        latch.set_low().map_err(Error::Pin)?;
        clock.set_low().map_err(Error::Pin)?;
        data.set_low().map_err(Error::Pin)?;
        Ok(Self {
            clock,
            data,
            latch,
            delay,
            latched_image: None,
        })
    }

    /// Shifts every physical bit and commits it with one rising latch edge.
    pub fn write_complete(&mut self, image: CompleteImage, timing: Timing) -> Result<(), Error<E>> {
        image.validate().map_err(Error::Image)?;
        timing.validate().map_err(Error::Timing)?;
        self.latched_image = None;
        self.latch.set_low().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.latch_setup_ns);
        self.clock.set_low().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.clock_low_ns);

        for serial_index in 0..image.width {
            let logical_bit = match image.order {
                BitOrder::MostSignificantFirst => image.width - 1 - serial_index,
                BitOrder::LeastSignificantFirst => serial_index,
            };
            if image.bits & (1_u32 << logical_bit) == 0 {
                self.data.set_low().map_err(Error::Pin)?;
            } else {
                self.data.set_high().map_err(Error::Pin)?;
            }
            self.delay.delay_ns(timing.data_setup_ns);
            self.clock.set_high().map_err(Error::Pin)?;
            self.delay.delay_ns(timing.clock_high_ns);
            self.clock.set_low().map_err(Error::Pin)?;
            self.delay.delay_ns(timing.clock_low_ns);
        }

        self.data.set_low().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.data_setup_ns);
        self.latch.set_high().map_err(Error::Pin)?;
        self.delay.delay_ns(timing.latch_high_ns);
        self.latch.set_low().map_err(Error::Pin)?;
        self.latched_image = Some(image);
        Ok(())
    }

    /// Returns the last image only after its entire transaction succeeded.
    pub const fn latched_image(&self) -> Option<CompleteImage> {
        self.latched_image
    }

    /// Returns owned pins for a deliberate, externally synchronized handoff.
    pub fn into_parts(self) -> (CLOCK, DATA, LATCH, DELAY) {
        (self.clock, self.data, self.latch, self.delay)
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::sync::{Arc, Mutex};
    use std::vec;
    use std::vec::Vec;

    use embedded_hal::digital::ErrorType;

    use super::*;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Line {
        Clock,
        Data,
        Latch,
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Edge(Line, bool),
        Delay(u32),
    }

    struct FakePin {
        line: Line,
        trace: Arc<Mutex<Vec<Event>>>,
    }

    impl ErrorType for FakePin {
        type Error = core::convert::Infallible;
    }

    impl OutputPin for FakePin {
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.trace
                .lock()
                .unwrap()
                .push(Event::Edge(self.line, false));
            Ok(())
        }

        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.trace
                .lock()
                .unwrap()
                .push(Event::Edge(self.line, true));
            Ok(())
        }
    }

    struct FakeDelay {
        trace: Arc<Mutex<Vec<Event>>>,
    }

    type FakeTransport = StaticShiftRegister<FakePin, FakePin, FakePin, FakeDelay>;
    type Trace = Arc<Mutex<Vec<Event>>>;

    impl DelayNs for FakeDelay {
        fn delay_ns(&mut self, ns: u32) {
            self.trace.lock().unwrap().push(Event::Delay(ns));
        }
    }

    fn transport() -> (FakeTransport, Trace) {
        let trace = Arc::new(Mutex::new(Vec::new()));
        let pin = |line| FakePin {
            line,
            trace: Arc::clone(&trace),
        };
        let delay = FakeDelay {
            trace: Arc::clone(&trace),
        };
        let transport =
            StaticShiftRegister::new(pin(Line::Clock), pin(Line::Data), pin(Line::Latch), delay)
                .unwrap();
        (transport, trace)
    }

    #[test]
    fn most_significant_first_transaction_is_exact_and_ends_inactive() {
        let (mut transport, trace) = transport();
        let image = CompleteImage {
            width: 3,
            defined_mask: 0b111,
            bits: 0b101,
            order: BitOrder::MostSignificantFirst,
        };
        let timing = Timing {
            data_setup_ns: 1,
            clock_high_ns: 2,
            clock_low_ns: 3,
            latch_setup_ns: 4,
            latch_high_ns: 5,
        };
        transport.write_complete(image, timing).unwrap();

        assert_eq!(
            *trace.lock().unwrap(),
            vec![
                Event::Edge(Line::Latch, false),
                Event::Edge(Line::Clock, false),
                Event::Edge(Line::Data, false),
                Event::Edge(Line::Latch, false),
                Event::Delay(4),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, true),
                Event::Delay(1),
                Event::Edge(Line::Clock, true),
                Event::Delay(2),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, false),
                Event::Delay(1),
                Event::Edge(Line::Clock, true),
                Event::Delay(2),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, true),
                Event::Delay(1),
                Event::Edge(Line::Clock, true),
                Event::Delay(2),
                Event::Edge(Line::Clock, false),
                Event::Delay(3),
                Event::Edge(Line::Data, false),
                Event::Delay(1),
                Event::Edge(Line::Latch, true),
                Event::Delay(5),
                Event::Edge(Line::Latch, false),
            ]
        );
        assert_eq!(transport.latched_image(), Some(image));
    }

    #[test]
    fn incomplete_image_is_rejected_before_any_write_activity() {
        let (mut transport, trace) = transport();
        let before = trace.lock().unwrap().len();
        let image = CompleteImage {
            width: 24,
            defined_mask: 0x00ff_7fff,
            bits: 0x1249,
            order: BitOrder::MostSignificantFirst,
        };
        assert_eq!(
            transport.write_complete(image, Timing::CONSERVATIVE_100NS),
            Err(Error::Image(ImageError::Incomplete {
                received_mask: 0x00ff_7fff,
                expected_mask: 0x00ff_ffff,
            }))
        );
        assert_eq!(trace.lock().unwrap().len(), before);
        assert_eq!(transport.latched_image(), None);
    }

    #[test]
    fn width_32_has_no_shift_overflow() {
        let (mut transport, _) = transport();
        let image = CompleteImage {
            width: 32,
            defined_mask: u32::MAX,
            bits: u32::MAX,
            order: BitOrder::LeastSignificantFirst,
        };
        assert_eq!(image.validate(), Ok(()));
        assert_eq!(
            transport.write_complete(image, Timing::CONSERVATIVE_100NS),
            Ok(())
        );
        assert_eq!(transport.latched_image(), Some(image));
    }

    #[test]
    fn invalid_timing_is_rejected_before_any_write_activity() {
        let (mut transport, trace) = transport();
        let before = trace.lock().unwrap().len();
        let image = CompleteImage {
            width: 1,
            defined_mask: 1,
            bits: 0,
            order: BitOrder::MostSignificantFirst,
        };
        let mut timing = Timing::CONSERVATIVE_100NS;
        timing.clock_high_ns = 0;
        assert_eq!(
            transport.write_complete(image, timing),
            Err(Error::Timing(TimingError {
                field_index: 1,
                received_ns: 0,
            }))
        );
        assert_eq!(trace.lock().unwrap().len(), before);
    }
}
