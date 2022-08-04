//! GPIO

pub mod alt;
pub mod convert;

use core::marker::PhantomData;
use embedded_hal::digital::v2::PinState;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use paste::paste;

pub struct Alternate<const A: u8, OType = PushPull>(PhantomData<OType>);

pub struct OpenDrain;
pub struct PushPull;

/// Analog pin (type state)
pub struct Analog;

pub struct Input;
pub struct Output<MODE = PushPull> {
    _mode: PhantomData<MODE>,
}

/// Pull setting for an input
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Pull {
    Floating = 0b00,
    Up = 0b01,
    Down = 0b10,
}

mod marker {
    /// Marker trait that show if `ExtiPin` can be implemented
    pub trait Interruptable {}
    /// Marker trait for readable pin modes
    pub trait Readable {}
    /// Marker trait for slew rate onfigurable pin modes
    pub trait OutputSpeed {}
    /// Marker trait for active pin modes
    pub trait Active {}
    /// Marker trait for all pin modes except alternate
    pub trait NotAlt {}
    /// Marker trait for pins with alternate function `A` mapping
    pub trait IntoAf<const A: u8> {}
}

impl<MODE> marker::Interruptable for Output<MODE> {}
impl marker::Interruptable for Input {}
impl marker::Readable for Input {}
impl marker::Readable for Output<OpenDrain> {}
impl marker::Active for Input {}
impl<OType> marker::Active for Output<OType> {}
impl<const A: u8, OType> marker::Active for Alternate<A, OType> {}
impl<OType> marker::OutputSpeed for Output<OType> {}
impl<const A: u8, OType> marker::OutputSpeed for Alternate<A, OType> {}
impl marker::NotAlt for Input {}
impl<OType> marker::NotAlt for Output<OType> {}
impl marker::NotAlt for Analog {}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Speed {
    Low = 0b00,
    Medium = 0b01,
    Fast = 0b10,
    High = 0b11,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Edge {
    Rising,
    Falling,
    RisingFalling,
}

macro_rules! af {
    ($($i:literal: $AFi:ident),* $(,)?) => {
        $(
            #[doc=concat!("Alternate function ", $i, " (type state)")]
            pub type $AFi<OType = PushPull> = Alternate<$i, OType>;
        )*
    }
}

af! {
    0: AF0,
    1: AF1,
    2: AF2,
    3: AF3,
    4: AF4,
    5: AF5,
    6: AF6,
    7: AF7,
    8: AF8,
    9: AF9,
    10: AF10,
    11: AF11,
    12: AF12,
    13: AF13,
    14: AF14,
    15: AF15,
}

pub trait PinExt {
    type Mode;

    fn pin_id(&self) -> u8;
    fn port_id(&self) -> u8;
}

pub trait GpioExt {
    type Parts;

    fn split(self) -> Self::Parts;
}

pub struct Pin<const P: char, const N: u8, MODE = Analog> {
    _mode: PhantomData<MODE>,
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE> {
    const fn new() -> Self {
        Self { _mode: PhantomData }
    }
}

impl<const P: char, const N: u8, MODE> PinExt for Pin<P, N, MODE> {
    type Mode = MODE;

    #[inline(always)]
    fn pin_id(&self) -> u8 {
        N
    }

    #[inline(always)]
    fn port_id(&self) -> u8 {
        P as u8 - b'A'
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: marker::OutputSpeed,
{
    pub fn set_speed(&mut self, speed: Speed) {
        unsafe {
            (*Gpio::<P>::ptr()).ospeedr.modify(|_, w| match N {
                0 => w.ospeedr0().variant(speed.into()),
                1 => w.ospeedr1().variant(speed.into()),
                2 => w.ospeedr2().variant(speed.into()),
                3 => w.ospeedr3().variant(speed.into()),
                4 => w.ospeedr4().variant(speed.into()),
                5 => w.ospeedr5().variant(speed.into()),
                6 => w.ospeedr6().variant(speed.into()),
                7 => w.ospeedr7().variant(speed.into()),
                8 => w.ospeedr8().variant(speed.into()),
                9 => w.ospeedr9().variant(speed.into()),
                10 => w.ospeedr10().variant(speed.into()),
                11 => w.ospeedr11().variant(speed.into()),
                12 => w.ospeedr12().variant(speed.into()),
                13 => w.ospeedr13().variant(speed.into()),
                14 => w.ospeedr14().variant(speed.into()),
                15 => w.ospeedr15().variant(speed.into()),
                _ => unreachable!(),
            });
        }
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: marker::Active,
{
    pub fn set_internal_resistor(&mut self, resistor: Pull) {
        unsafe {
            (*Gpio::<P>::ptr()).pupdr.modify(|_, w| match N {
                0 => w.pupdr0().variant(resistor.into()),
                1 => w.pupdr1().variant(resistor.into()),
                2 => w.pupdr2().variant(resistor.into()),
                3 => w.pupdr3().variant(resistor.into()),
                4 => w.pupdr4().variant(resistor.into()),
                5 => w.pupdr5().variant(resistor.into()),
                6 => w.pupdr6().variant(resistor.into()),
                7 => w.pupdr7().variant(resistor.into()),
                8 => w.pupdr8().variant(resistor.into()),
                9 => w.pupdr9().variant(resistor.into()),
                10 => w.pupdr10().variant(resistor.into()),
                11 => w.pupdr11().variant(resistor.into()),
                12 => w.pupdr12().variant(resistor.into()),
                13 => w.pupdr13().variant(resistor.into()),
                14 => w.pupdr14().variant(resistor.into()),
                15 => w.pupdr15().variant(resistor.into()),
                _ => unreachable!(),
            });
        }
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE> {
    fn _set_high(&mut self) {
        unsafe {
            (*Gpio::<P>::ptr()).bsrr.write(|w| match N {
                0 => w.bs0().set_bit(),
                1 => w.bs1().set_bit(),
                2 => w.bs2().set_bit(),
                3 => w.bs3().set_bit(),
                4 => w.bs4().set_bit(),
                5 => w.bs5().set_bit(),
                6 => w.bs6().set_bit(),
                7 => w.bs7().set_bit(),
                8 => w.bs8().set_bit(),
                9 => w.bs9().set_bit(),
                10 => w.bs10().set_bit(),
                11 => w.bs11().set_bit(),
                12 => w.bs12().set_bit(),
                13 => w.bs13().set_bit(),
                14 => w.bs14().set_bit(),
                15 => w.bs15().set_bit(),
                _ => unreachable!(),
            });
        }
    }

    fn _set_low(&mut self) {
        unsafe {
            (*Gpio::<P>::ptr()).bsrr.write(|w| match N {
                0 => w.br0().set_bit(),
                1 => w.br1().set_bit(),
                2 => w.br2().set_bit(),
                3 => w.br3().set_bit(),
                4 => w.br4().set_bit(),
                5 => w.br5().set_bit(),
                6 => w.br6().set_bit(),
                7 => w.br7().set_bit(),
                8 => w.br8().set_bit(),
                9 => w.br9().set_bit(),
                10 => w.br10().set_bit(),
                11 => w.br11().set_bit(),
                12 => w.br12().set_bit(),
                13 => w.br13().set_bit(),
                14 => w.br14().set_bit(),
                15 => w.br15().set_bit(),
                _ => unreachable!(),
            });
        }
    }

    fn _is_set_low(&self) -> bool {
        unsafe {
            let r = (*Gpio::<P>::ptr()).odr.read();

            match N {
                0 => r.odr0().bit(),
                1 => r.odr1().bit(),
                2 => r.odr2().bit(),
                3 => r.odr3().bit(),
                4 => r.odr4().bit(),
                5 => r.odr5().bit(),
                6 => r.odr6().bit(),
                7 => r.odr7().bit(),
                8 => r.odr8().bit(),
                9 => r.odr9().bit(),
                10 => r.odr10().bit(),
                11 => r.odr11().bit(),
                12 => r.odr12().bit(),
                13 => r.odr13().bit(),
                14 => r.odr14().bit(),
                15 => r.odr15().bit(),
                _ => unreachable!(),
            }
        }
    }

    fn _is_low(&self) -> bool {
        unsafe {
            let r = (*Gpio::<P>::ptr()).idr.read();

            match N {
                0 => r.idr0().bit(),
                1 => r.idr1().bit(),
                2 => r.idr2().bit(),
                3 => r.idr3().bit(),
                4 => r.idr4().bit(),
                5 => r.idr5().bit(),
                6 => r.idr6().bit(),
                7 => r.idr7().bit(),
                8 => r.idr8().bit(),
                9 => r.idr9().bit(),
                10 => r.idr10().bit(),
                11 => r.idr11().bit(),
                12 => r.idr12().bit(),
                13 => r.idr13().bit(),
                14 => r.idr14().bit(),
                15 => r.idr15().bit(),
                _ => unreachable!(),
            }
        }
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, Output<MODE>> {
    #[inline(always)]
    pub fn set_high(&mut self) {
        self._set_high();
    }

    #[inline(always)]
    pub fn set_low(&mut self) {
        self._set_low();
    }

    #[inline(always)]
    pub fn get_state(&self) -> PinState {
        if self._is_set_low() {
            PinState::Low
        } else {
            PinState::High
        }
    }

    #[inline(always)]
    pub fn set_state(&mut self, state: PinState) {
        match state {
            PinState::Low => self.set_low(),
            PinState::High => self.set_high(),
        }
    }

    #[inline(always)]
    pub fn is_set_high(&self) -> bool {
        !self.is_set_low()
    }

    #[inline(always)]
    pub fn is_set_low(&self) -> bool {
        self._is_set_low()
    }

    #[inline(always)]
    pub fn toggle(&mut self) {
        if self.is_set_high() {
            self.set_low();
        } else {
            self.set_high();
        }
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: marker::Readable,
{
    #[inline(always)]
    pub fn is_high(&self) -> bool {
        !self.is_low()
    }

    #[inline(always)]
    pub fn is_low(&self) -> bool {
        self._is_low()
    }
}

macro_rules! gpio {
    ($GPIOX:ident, $port_id:expr, [
        $($PXi:ident: ($i:expr, [$($A:literal),*] $(, $MODE:ty)?),)*
    ]) => {
        paste! {
            pub mod [<$GPIOX:lower>] {
                use crate::pac::$GPIOX;
                use crate::rcc::rec;

                pub struct Parts {
                    $(
                        pub [<$PXi:lower>]: $PXi $(<$MODE>)?,
                    )*
                }

                impl super::GpioExt for $GPIOX {
                    type Parts = Parts;

                    fn split(self) -> Parts {
                        rec::$GPIOX::enable();
                        rec::$GPIOX::reset();

                        Parts {
                            $(
                                [<$PXi:lower>]: $PXi::new(),
                            )*
                        }
                    }
                }

                $(
                    pub type $PXi<MODE = super::Input> = super::Pin<$port_id, $i, MODE>;
                )*
            }
        }
    };
}

struct Gpio<const P: char>;

impl<const P: char> Gpio<P> {
    const fn ptr() -> *const crate::pac::gpioa::RegisterBlock {
        match P {
            'A' => crate::pac::GPIOA::PTR,
            'B' => crate::pac::GPIOB::PTR as _,
            'C' => crate::pac::GPIOC::PTR as _,
            'D' => crate::pac::GPIOD::PTR as _,
            'E' => crate::pac::GPIOE::PTR as _,
            'H' => crate::pac::GPIOH::PTR as _,
            _ => unreachable!(),
        }
    }
}

gpio! {
    GPIOA, 'A', [
        PA0: (0, [1, 12, 13, 14, 15]),
        PA1: (1, [1, 4, 5, 11, 15]),
        PA2: (2, [0, 1, 8, 10, 11, 12, 15]),
        PA3: (3, [1, 3, 8, 10, 11, 13, 15]),
        PA4: (4, [5, 11, 13, 14, 15]),
        PA5: (5, [1, 2, 5, 13, 14, 15]),
        PA6: (6, [1, 5, 8, 10, 11, 12, 14, 15]),
        PA7: (7, [1, 4, 5, 10, 11, 12, 14, 15]),
        PA8: (8, [0, 1, 3, 7, 11, 13, 14, 15]),
        PA9: (9, [1, 3, 4, 5, 7, 11, 13, 15]),
        PA10: (10, [1, 3, 4, 7, 10, 11, 13, 14, 15]),
        PA11: (11, [1, 2, 5, 7, 10, 12, 15]),
        PA12: (12, [1, 5, 7, 8, 10, 15]),
        PA13: (13, [0, 8, 10, 13, 15]),
        PA14: (14, [0, 1, 4, 11, 13, 15]),
        PA15: (15, [0, 1, 2, 5, 6, 11, 15]),
    ]
}

gpio! {
    GPIOB, 'B', [
        PB0: (0, [6, 12, 15]),
        PB1: (1, [8, 14, 15]),
        PB2: (2, [0, 1, 4, 5, 11, 13, 15]),
        PB3: (3, [0, 1, 5, 7, 11, 13, 15]),
        PB4: (4, [0, 4, 5, 7, 9, 11, 13, 14, 15]),
        PB5: (5, [1, 4, 5, 7, 8, 9, 11, 12, 13, 14, 15]),
        PB6: (6, [0, 1, 4, 7, 9, 11, 13, 14, 15]),
        PB7: (7, [1, 3, 4, 7, 9, 11, 14, 15]),
        PB8: (8, [1, 3, 4, 10, 11, 13, 14, 15]),
        PB9: (9, [1, 3, 4, 5, 8, 9, 10, 11, 13, 14, 15]),
        PB10: (10, [1, 4, 5, 8, 9, 10, 11, 12, 13, 15]),
        PB11: (11, [1, 4, 8, 10, 11, 12, 15]),
        PB12: (12, [1, 3, 4, 5, 8, 9, 11, 13, 15]),
        PB13: (13, [1, 4, 5, 8, 9, 11, 13, 15]),
        PB14: (14, [1, 4, 5, 9, 11, 13, 15]),
        PB15: (15, [0, 1, 5, 9, 11, 13, 15]),
    ]
}

gpio! {
    GPIOC, 'C', [
        PC0: (0, [1, 4, 8, 11, 14, 15]),
        PC1: (1, [1, 3, 4, 8, 11, 15]),
        PC2: (2, [1, 5, 11, 15]),
        PC3: (3, [1, 3, 5, 11, 13, 14, 15]),
        PC4: (4, [11, 15]),
        PC5: (5, [3, 11, 15]),
        PC6: (6, [9, 11, 15]),
        PC7: (7, [9, 11, 15]),
        PC8: (8, [9, 11, 15]),
        PC9: (9, [3, 9, 10, 11, 13, 15]),
        PC10: (10, [0, 9, 11, 15]),
        PC11: (11, [9, 11, 15]),
        PC12: (12, [0, 6, 9, 11, 15]),
        PC13: (13, [15]),
        PC14: (14, [15]),
        PC15: (15, [15]),
    ]
}

gpio! {
    GPIOD, 'D', [
        PD0: (0, [5, 15]),
        PD1: (1, [5, 15]),
        PD2: (2, [0, 9, 11, 15]),
        PD3: (3, [3, 5, 10, 15]),
        PD4: (4, [5, 9, 10, 15]),
        PD5: (5, [9, 10, 13, 15]),
        PD6: (6, [3, 9, 10, 13, 15]),
        PD7: (7, [9, 10, 11, 15]),
        PD8: (8, [2, 11, 15]),
        PD9: (9, [0, 11, 15]),
        PD10: (10, [0, 9, 11, 15]),
        PD11: (11, [9, 11, 14, 15]),
        PD12: (12, [9, 11, 14, 15]),
        PD13: (13, [9, 11, 14, 15]),
        PD14: (14, [1, 11, 15]),
        PD15: (15, [1, 11, 15]),
    ]
}

gpio! {
    GPIOE, 'E', [
        PE0: (0, [1, 9, 11, 14, 15]),
        PE1: (1, [9, 11, 14, 15]),
        PE2: (2, [0, 3, 9, 11, 13, 15]),
        PE3: (3, [15]),
        PE4: (4, [15]),
    ]
}

gpio! {
    GPIOH, 'H', [
        PH0: (0, [15]),
        PH1: (1, [15]),
        PH3: (3, [0, 15]),
    ]
}
