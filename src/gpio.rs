//! GPIO

// Type States

use crate::rcc::rec;
use core::marker::PhantomData;
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
impl<OType> marker::OutputSpeed for Output<OType> {}
impl<const A: u8, OType> marker::OutputSpeed for Alternate<A, OType> {}
impl<OType> marker::Active for Output<OType> {}
impl<const A: u8, OType> marker::Active for Alternate<A, OType> {}
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
            })
        }
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
