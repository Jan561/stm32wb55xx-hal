//! GPIO

// Type States

use core::marker::PhantomData;

use num_enum::{IntoPrimitive, TryFromPrimitive};

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
        let offset = 2 * N;

        unsafe {}
    }
}

macro_rules! gpio {
    ($GPIOX:ident, $gpiox:ident, $Rec:ident, $PEPin:ident, $port_id:expr, $PXn:ident, [
        $($PXi:ident: ($pxi:ident, $i:expr $(, $MODE:ty)?),)*
    ]) => {
        pub mod $gpiox {
            use crate::pac::$GPIOX;
        }
    };
}
