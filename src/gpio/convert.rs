use super::*;
use sealed::sealed;

#[sealed]
pub trait PinMode {
    #[doc(hidden)]
    const MODER: u8;
    #[doc(hidden)]
    const OTYPER: Option<bool> = None;
    #[doc(hidden)]
    const AFR: Option<u8> = None;
}

#[sealed]
impl PinMode for Input {
    const MODER: u8 = 0b00;
}

#[sealed]
impl PinMode for Analog {
    const MODER: u8 = 0b11;
}

#[sealed]
impl PinMode for Output<OpenDrain> {
    const MODER: u8 = 0b01;
    const OTYPER: Option<bool> = Some(true);
}

#[sealed]
impl PinMode for Output<PushPull> {
    const MODER: u8 = 0b01;
    const OTYPER: Option<bool> = Some(false);
}

#[sealed]
impl<const A: u8> PinMode for Alternate<A, OpenDrain> {
    const MODER: u8 = 0b10;
    const OTYPER: Option<bool> = Some(true);
    const AFR: Option<u8> = Some(A);
}

#[sealed]
impl<const A: u8> PinMode for Alternate<A, PushPull> {
    const MODER: u8 = 0b10;
    const OTYPER: Option<bool> = Some(false);
    const AFR: Option<u8> = Some(A);
}

mod marker {
    pub trait Convertable<MODE> {}
}

impl<const P: char, const N: u8, MODE> marker::Convertable<Input> for Pin<P, N, MODE> {}
impl<const P: char, const N: u8, MODE, OType> marker::Convertable<Output<OType>>
    for Pin<P, N, MODE>
{
}
impl<const P: char, const N: u8, MODE> marker::Convertable<Analog> for Pin<P, N, MODE> {}
impl<const P: char, const N: u8, MODE, const A: u8, OType> marker::Convertable<Alternate<A, OType>>
    for Pin<P, N, MODE>
where
    Self: super::marker::IntoAf<A>,
{
}

impl<const P: char, const N: u8, MODE: PinMode> Pin<P, N, MODE> {
    #[inline(always)]
    pub fn into_input(self) -> Pin<P, N, Input> {
        self.into_mode()
    }

    #[inline(always)]
    pub fn into_floating_input(self) -> Pin<P, N, Input> {
        self.into_mode().internal_resistor(Pull::Floating)
    }

    #[inline(always)]
    pub fn into_pull_up_input(self) -> Pin<P, N, Input> {
        self.into_mode().internal_resistor(Pull::Up)
    }

    #[inline(always)]
    pub fn into_pull_down_input(self) -> Pin<P, N, Input> {
        self.into_mode().internal_resistor(Pull::Down)
    }

    #[inline(always)]
    pub fn into_open_drain_output(self) -> Pin<P, N, Output<OpenDrain>> {
        self.into_mode()
    }

    #[inline(always)]
    pub fn into_open_drain_output_in_state(
        mut self,
        initial_state: PinState,
    ) -> Pin<P, N, Output<OpenDrain>> {
        self._set_state(initial_state);
        self.into_mode()
    }

    /// Configures the pin to operate as an push pull output pin
    ///
    /// Initial state will be low
    #[inline(always)]
    pub fn into_push_pull_output(mut self) -> Pin<P, N, Output<PushPull>> {
        self._set_low();
        self.into_mode()
    }

    #[inline(always)]
    pub fn into_alternate<const A: u8>(self) -> Pin<P, N, Alternate<A, PushPull>>
    where
        Self: super::marker::IntoAf<A>,
    {
        self.into_mode()
    }

    #[inline(always)]
    pub fn into_alternate_open_drain<const A: u8>(self) -> Pin<P, N, Alternate<A, OpenDrain>>
    where
        Self: super::marker::IntoAf<A>,
    {
        self.into_mode()
    }

    #[inline(always)]
    pub fn into_mode<M: PinMode>(mut self) -> Pin<P, N, M>
    where
        Self: marker::Convertable<M>,
    {
        self._set_mode::<M>();
        Pin::new()
    }

    fn _set_mode<M: PinMode>(&mut self) {
        unsafe {
            if MODE::OTYPER != M::OTYPER {
                if let Some(otyper) = M::OTYPER {
                    (*Gpio::<P>::ptr()).otyper.modify(|_, w| match N {
                        0 => w.ot0().bit(otyper),
                        1 => w.ot1().bit(otyper),
                        2 => w.ot2().bit(otyper),
                        3 => w.ot3().bit(otyper),
                        4 => w.ot4().bit(otyper),
                        5 => w.ot5().bit(otyper),
                        6 => w.ot6().bit(otyper),
                        7 => w.ot7().bit(otyper),
                        8 => w.ot8().bit(otyper),
                        9 => w.ot9().bit(otyper),
                        10 => w.ot10().bit(otyper),
                        11 => w.ot11().bit(otyper),
                        12 => w.ot12().bit(otyper),
                        13 => w.ot13().bit(otyper),
                        14 => w.ot14().bit(otyper),
                        15 => w.ot15().bit(otyper),
                        _ => unreachable!(),
                    });
                }
            }

            if MODE::AFR != M::AFR {
                if let Some(afr) = M::AFR {
                    if N < 8 {
                        (*Gpio::<P>::ptr()).afrl.modify(|_, w| match N {
                            0 => w.afsel0().variant(afr),
                            1 => w.afsel1().variant(afr),
                            2 => w.afsel2().variant(afr),
                            3 => w.afsel3().variant(afr),
                            4 => w.afsel4().variant(afr),
                            5 => w.afsel5().variant(afr),
                            6 => w.afsel6().variant(afr),
                            7 => w.afsel7().variant(afr),
                            _ => unreachable!(),
                        });
                    } else {
                        (*Gpio::<P>::ptr()).afrh.modify(|_, w| match N {
                            8 => w.afsel8().variant(afr),
                            9 => w.afsel9().variant(afr),
                            10 => w.afsel10().variant(afr),
                            11 => w.afsel11().variant(afr),
                            12 => w.afsel12().variant(afr),
                            13 => w.afsel13().variant(afr),
                            14 => w.afsel14().variant(afr),
                            15 => w.afsel15().variant(afr),
                            _ => unreachable!(),
                        });
                    }
                }
            }

            if MODE::MODER != M::MODER {
                (*Gpio::<P>::ptr()).moder.modify(|_, w| match N {
                    0 => w.moder0().variant(M::MODER),
                    1 => w.moder1().variant(M::MODER),
                    2 => w.moder2().variant(M::MODER),
                    3 => w.moder3().variant(M::MODER),
                    4 => w.moder4().variant(M::MODER),
                    5 => w.moder5().variant(M::MODER),
                    6 => w.moder6().variant(M::MODER),
                    7 => w.moder7().variant(M::MODER),
                    8 => w.moder8().variant(M::MODER),
                    9 => w.moder9().variant(M::MODER),
                    10 => w.moder10().variant(M::MODER),
                    11 => w.moder11().variant(M::MODER),
                    12 => w.moder12().variant(M::MODER),
                    13 => w.moder13().variant(M::MODER),
                    14 => w.moder14().variant(M::MODER),
                    15 => w.moder15().variant(M::MODER),
                    _ => unreachable!(),
                });
            }
        }
    }
}
