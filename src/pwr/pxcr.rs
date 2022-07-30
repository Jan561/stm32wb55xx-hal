//! Pull-Up/Down Control Register
//!
//! This module configures the internal resistors of the GPIO ports during Standby / Shutdown.
//! In all other run/stop modes, this doesn't have any effect
//!
//! Note: After Shutdown, a reset is triggered clearing all registers so this needs to be reconfigured

use crate::pac::PWR;
use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    None,
    PullUp,
    PullDown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u32)]
pub enum Pin {
    P0 = 0,
    P1 = 1,
    P2 = 2,
    P3 = 3,
    P4 = 4,
    P5 = 5,
    P6 = 6,
    P7 = 7,
    P8 = 8,
    P9 = 9,
    P10 = 10,
    P11 = 11,
    P12 = 12,
    P13 = 13,
    P14 = 14,
    P15 = 15,
}

macro_rules! pxcr {
    ($port:ident, $reg_d:ident, $reg_u:ident) => {
        pub fn $port<F>(&self, op: F)
        where
            F: for<'w> FnOnce(&PxcrR, &'w mut PxcrW) -> &'w mut PxcrW,
        {
            let r = PxcrR {
                pd: self.rb().$reg_d.read().bits(),
                pu: self.rb().$reg_u.read().bits(),
            };
            let mut wc = PxcrW { pd: r.pd, pu: r.pu };

            op(&r, &mut wc);

            self.rb().$reg_d.modify(|_, w| {
                // SAFETY: See RM0434 Rev 10 p. 182
                unsafe { w.bits(wc.pd) }
            });

            self.rb().$reg_u.modify(|_, w| {
                // SAFETY: See RM0434 Rev 10 p. 181
                unsafe { w.bits(wc.pu) }
            });
        }
    };
}

pub struct Pxcr<'a>(pub(super) &'a super::Pwr);

impl Pxcr<'_> {
    fn rb(&self) -> &PWR {
        &self.0.pwr
    }

    pxcr!(port_a, pdcra, pucra);
    pxcr!(port_b, pdcrb, pucrb);
    pxcr!(port_c, pdcrc, pucrc);
    pxcr!(port_d, pdcrd, pucrd);
    pxcr!(port_e, pdcre, pucre);
    pxcr!(port_h, pdcrh, pucrh);
}

pub struct PxcrR {
    pd: u32,
    pu: u32,
}

impl PxcrR {
    pub fn mode(&self, pin: Pin) -> Mode {
        let mask = 0x1 << u32::from(pin);

        if self.pd & mask != 0 {
            Mode::PullDown
        } else if self.pu & mask != 0 {
            Mode::PullUp
        } else {
            Mode::None
        }
    }
}

pub struct PxcrW {
    pd: u32,
    pu: u32,
}

impl PxcrW {
    pub fn mode(&mut self, pin: Pin, mode: Mode) -> &mut Self {
        let mask = 0x1 << u32::from(pin);

        match mode {
            Mode::PullDown => self.pd |= mask,
            Mode::PullUp => {
                self.pu |= mask;
                self.pd &= !mask;
            }
            Mode::None => {
                self.pu &= !mask;
                self.pd &= !mask;
            }
        }

        self
    }
}
