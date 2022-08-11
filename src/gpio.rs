//! GPIO

// pub mod alt;
pub mod convert;

use core::convert::Infallible;
use core::marker::PhantomData;
use embedded_hal::digital::PinState;
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

macro_rules! p {
    ($P:expr, $mac:ident) => {
        paste! {
            match $P {
                'A' => $mac!(a),
                'B' => $mac!(b),
                'C' => $mac!(c),
                'D' => $mac!(d),
                'E' => $mac!(e),
                'H' => $mac!(h),
                #[allow(unused_unsafe)]
                _ => unsafe { core::hint::unreachable_unchecked() },
            }
        }
    };
}

macro_rules! n {
    ($N: expr, $mac:ident) => {{
        paste! {
            match $N {
                0 => $mac!(0),
                1 => $mac!(1),
                2 => $mac!(2),
                3 => $mac!(3),
                4 => $mac!(4),
                5 => $mac!(5),
                6 => $mac!(6),
                7 => $mac!(7),
                8 => $mac!(8),
                9 => $mac!(9),
                10 => $mac!(10),
                11 => $mac!(11),
                12 => $mac!(12),
                13 => $mac!(13),
                14 => $mac!(14),
                15 => $mac!(15),
                #[allow(unused_unsafe)]
                _ => unsafe { core::hint::unreachable_unchecked() },
            }
        }
    }};
}

macro_rules! n_reg_w {
    ($n:expr, $w:expr, $field:ident, $val:expr) => {{
        macro_rules! __n_reg {
            ($nn:literal) => {{
                paste! {
                    $w.[<$field $nn>]().variant($val)
                }
            }};
        }

        n!($n, __n_reg)
    }};
}

macro_rules! n_reg_r_bit {
    ($n:expr, $r:expr, $field:ident) => {{
        macro_rules! __n_reg {
            ($nn:literal) => {{
                paste! {
                    $r.[<$field $nn>]().bit()
                }
            }};
        }

        n!($n, __n_reg)
    }};
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: marker::OutputSpeed,
{
    pub fn set_speed(&mut self, speed: Speed) {
        unsafe {
            (*Gpio::<P>::ptr())
                .ospeedr
                .modify(|_, w| n_reg_w!(N, w, ospeedr, speed.into()));
        }
    }

    #[inline(always)]
    pub fn speed(mut self, speed: Speed) -> Self {
        self.set_speed(speed);
        self
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE>
where
    MODE: marker::Active,
{
    pub fn set_internal_resistor(&mut self, resistor: Pull) {
        unsafe {
            (*Gpio::<P>::ptr())
                .pupdr
                .modify(|_, w| n_reg_w!(N, w, pupdr, resistor.into()));
        }
    }

    pub fn set_internal_resistor_lp(&mut self, resistor: Pull) {
        let pwr = unsafe { &*crate::pac::PWR::PTR };

        let (pu, pd) = match resistor {
            Pull::Floating => (false, false),
            Pull::Up => (true, false),
            Pull::Down => (false, true),
        };

        macro_rules! outer {
            ($p:ident) => {{
                macro_rules! inner {
                    ($n:literal) => {{
                        paste! {
                            // pucra has missing fields
                            let pur = unsafe {
                                &*(&pwr.[<pucr $p>] as *const _ as *const stm32wb::Reg<crate::pac::pwr::pucrb::PUCRB_SPEC>)
                            };
                            // pucra, pucrb have missing fields
                            let pdr = unsafe {
                                &*(&pwr.[<pdcr $p>] as *const _ as *const stm32wb::Reg<crate::pac::pwr::pdcrc::PDCRC_SPEC>)
                            };

                            pur.modify(|_, w| w.[<pu $n>]().bit(pu));
                            pdr.modify(|_, w| w.[<pd $n>]().bit(pd));
                        }
                    }};
                }

                n!(N, inner);
            }};
        }

        p!(P, outer);
    }

    #[inline(always)]
    pub fn internal_resistor(mut self, resistor: Pull) -> Self {
        self.set_internal_resistor(resistor);
        self
    }
}

impl<const P: char, const N: u8, MODE> Pin<P, N, MODE> {
    #[inline(always)]
    fn _set_state(&mut self, state: PinState) {
        match state {
            PinState::Low => self._set_low(),
            PinState::High => self._set_high(),
        }
    }

    fn _set_high(&mut self) {
        unsafe {
            (*Gpio::<P>::ptr()).bsrr.write(|w| n_reg_w!(N, w, bs, true));
        }
    }

    fn _set_low(&mut self) {
        unsafe {
            (*Gpio::<P>::ptr()).bsrr.write(|w| n_reg_w!(N, w, br, true));
        }
    }

    fn _is_set_low(&self) -> bool {
        unsafe {
            let r = (*Gpio::<P>::ptr()).odr.read();

            n_reg_r_bit!(N, r, odr)
        }
    }

    fn _is_low(&self) -> bool {
        unsafe {
            let r = (*Gpio::<P>::ptr()).idr.read();

            n_reg_r_bit!(N, r, idr)
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

            $(
                pub use [<$GPIOX:lower>]::$PXi;
            )*
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

mod hal {
    use super::convert::PinMode;
    use super::*;
    use embedded_hal::digital::blocking::{
        InputPin, IoPin, OutputPin, StatefulOutputPin, ToggleableOutputPin,
    };
    use embedded_hal::digital::ErrorType;

    impl<const P: char, const N: u8, MODE> ErrorType for Pin<P, N, MODE> {
        type Error = Infallible;
    }

    impl<const P: char, const N: u8, OType> OutputPin for Pin<P, N, Output<OType>> {
        #[inline(always)]
        fn set_low(&mut self) -> Result<(), Self::Error> {
            self.set_low();
            Ok(())
        }

        #[inline(always)]
        fn set_high(&mut self) -> Result<(), Self::Error> {
            self.set_high();
            Ok(())
        }
    }

    impl<const P: char, const N: u8, OType> StatefulOutputPin for Pin<P, N, Output<OType>> {
        #[inline(always)]
        fn is_set_low(&self) -> Result<bool, Self::Error> {
            Ok(self.is_set_low())
        }

        #[inline(always)]
        fn is_set_high(&self) -> Result<bool, Self::Error> {
            Ok(self.is_set_high())
        }
    }

    impl<const P: char, const N: u8, OType> ToggleableOutputPin for Pin<P, N, Output<OType>> {
        #[inline(always)]
        fn toggle(&mut self) -> Result<(), Self::Error> {
            self.toggle();
            Ok(())
        }
    }

    impl<const P: char, const N: u8, MODE> InputPin for Pin<P, N, MODE>
    where
        MODE: marker::Readable,
    {
        #[inline(always)]
        fn is_low(&self) -> Result<bool, Self::Error> {
            Ok(self.is_low())
        }

        #[inline(always)]
        fn is_high(&self) -> Result<bool, Self::Error> {
            Ok(self.is_high())
        }
    }

    impl<const P: char, const N: u8> IoPin<Self, Self> for Pin<P, N, Output<OpenDrain>> {
        type Error = Infallible;

        fn into_input_pin(self) -> Result<Self, Self::Error> {
            Ok(self)
        }

        fn into_output_pin(mut self, state: PinState) -> Result<Self, Self::Error> {
            self.set_state(state);
            Ok(self)
        }
    }

    impl<const P: char, const N: u8, OType> IoPin<Pin<P, N, Input>, Self> for Pin<P, N, Output<OType>>
    where
        Output<OType>: PinMode,
    {
        type Error = Infallible;

        fn into_input_pin(self) -> Result<Pin<P, N, Input>, Self::Error> {
            Ok(self.into_input())
        }

        fn into_output_pin(mut self, state: PinState) -> Result<Self, Self::Error> {
            self.set_state(state);
            Ok(self)
        }
    }

    impl<const P: char, const N: u8, OType> IoPin<Self, Pin<P, N, Output<OType>>> for Pin<P, N, Input>
    where
        Output<OType>: PinMode,
    {
        type Error = Infallible;

        fn into_input_pin(self) -> Result<Self, Self::Error> {
            Ok(self)
        }

        fn into_output_pin(
            mut self,
            state: PinState,
        ) -> Result<Pin<P, N, Output<OType>>, Self::Error> {
            self._set_state(state);
            Ok(self.into_mode())
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
