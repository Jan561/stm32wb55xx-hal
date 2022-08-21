use core::convert::Infallible;
use cortex_m::peripheral::syst::SystClkSource;
use cortex_m::peripheral::SYST;
use embedded_hal::delay::blocking::DelayUs;
use embedded_hal_02::timer::CountDown;

use crate::rcc::{Clocks, TrustedClocks};
use crate::time::Hertz;
use fugit::RateExtU32;
use nb::block;

pub trait DelayExt {
    fn delay<'a, CLOCKS>(self, clocks: CLOCKS) -> Delay<CLOCKS>
    where
        CLOCKS: Clocks + TrustedClocks<'a>;
}

impl DelayExt for SYST {
    fn delay<'a, CLOCKS>(self, clocks: impl Clocks) -> Delay<CLOCKS>
    where
        CLOCKS: Clocks + TrustedClocks<'a>,
    {
        Delay::new(self, clocks)
    }
}

/// System timer (SysTick) as a delay provider
pub struct Delay<CLOCKS> {
    clocks: CLOCKS,
    syst: SYST,
}

/// Implements [CountDown](embedded_hal::timer::CountDown) for the System timer (SysTick).
pub struct Countdown<'a, CLOCKS> {
    clocks: CLOCKS,
    syst: &'a mut SYST,
    total_rvr: u64,
    finished: bool,
}

impl<'a, CLOCKS> Countdown<'a, CLOCKS>
where
    CLOCKS: Clocks + TrustedClocks<'a>,
{
    /// Create a new [CountDown] measured in microseconds.
    pub fn new(syst: &'a mut SYST, clocks: CLOCKS) -> Self {
        Self {
            syst,
            clocks,
            total_rvr: 0,
            finished: true,
        }
    }

    /// start a wait cycle and sets finished to true if [CountdownUs] is done waiting.
    fn start_wait(&mut self) {
        // The SysTick Reload Value register supports values between 1 and 0x00FFFFFF.
        const MAX_RVR: u32 = 0x00FF_FFFF;

        if self.total_rvr != 0 {
            self.finished = false;
            let current_rvr = if self.total_rvr <= MAX_RVR.into() {
                self.total_rvr as u32
            } else {
                MAX_RVR
            };

            self.syst.set_reload(current_rvr);
            self.syst.clear_current();
            self.syst.enable_counter();

            self.total_rvr -= current_rvr as u64;
        } else {
            self.finished = true;
        }
    }
}

impl<'a, CLOCKS> CountDown for Countdown<'a, CLOCKS>
where
    CLOCKS: Clocks + TrustedClocks<'a>,
{
    type Time = fugit::MicrosDurationU32;

    fn start<T>(&mut self, count: T)
    where
        T: Into<Self::Time>,
    {
        let us = count.into().ticks();

        // With c_ck up to 480e6, we need u64 for delays > 8.9s

        self.total_rvr = if cfg!(not(feature = "revision_v")) {
            // See errata ES0392 §2.2.3. Revision Y does not have the /8 divider
            u64::from(us) * u64::from(self.clocks.c_ck().raw() / 1_000_000)
        } else if cfg!(feature = "cm4") {
            // CM4 dervived from HCLK
            u64::from(us) * u64::from(self.clocks.hclk().raw() / 8_000_000)
        } else {
            // Normally divide by 8
            u64::from(us) * u64::from(self.clocks.c_ck().raw() / 8_000_000)
        };

        self.start_wait();
    }

    fn wait(&mut self) -> nb::Result<(), Infallible> {
        if self.finished {
            return Ok(());
        }

        if self.syst.has_wrapped() {
            self.syst.disable_counter();
            self.start_wait();
        }

        Err(nb::Error::WouldBlock)
    }
}

impl<'a, CLOCKS> Delay<CLOCKS>
where
    CLOCKS: Clocks + TrustedClocks<'a>,
{
    /// Configures the system timer (SysTick) as a delay provider
    pub fn new(mut syst: SYST, clocks: CLOCKS) -> Self {
        syst.set_clock_source(SystClkSource::External);

        Delay { clocks, syst }
    }

    /// Releases the system timer (SysTick) resource
    pub fn free(self) -> SYST {
        self.syst
    }
}

impl<'a, CLOCKS> DelayUs for Delay<CLOCKS>
where
    CLOCKS: Clocks + TrustedClocks<'a>,
{
    fn delay_us(&mut self, us: u32) {
        // The SysTick Reload Value register supports values between 1 and 0x00FFFFFF.
        const MAX_RVR: u32 = 0x00FF_FFFF;

        // With c_ck up to 480e6, we need u64 for delays > 8.9s

        let mut total_rvr = if cfg!(not(feature = "revision_v")) {
            // See errata ES0392 §2.2.3. Revision Y does not have the /8 divider
            u64::from(us) * u64::from(self.clocks.c_ck().raw() / 1_000_000)
        } else if cfg!(feature = "cm4") {
            // CM4 derived from HCLK
            u64::from(us) * u64::from(self.clocks.hclk().raw() / 8_000_000)
        } else {
            // Normally divide by 8
            u64::from(us) * u64::from(self.clocks.c_ck().raw() / 8_000_000)
        };

        while total_rvr != 0 {
            let current_rvr = if total_rvr <= MAX_RVR.into() {
                total_rvr as u32
            } else {
                MAX_RVR
            };

            self.syst.set_reload(current_rvr);
            self.syst.clear_current();
            self.syst.enable_counter();

            // Update the tracking variable while we are waiting...
            total_rvr -= u64::from(current_rvr);

            while !self.syst.has_wrapped() {}

            self.syst.disable_counter();
        }
    }
}

/// CountDown Timer as a delay provider
pub struct DelayFromCountDownTimer<T>(T);

impl<T> DelayFromCountDownTimer<T> {
    /// Creates delay provider from a CountDown timer
    pub fn new(timer: T) -> Self {
        Self(timer)
    }

    /// Releases the Timer
    pub fn free(self) -> T {
        self.0
    }
}

macro_rules! impl_delay_from_count_down_timer  {
    ($(($Delay:ident, $delay:ident, $num:expr)),+) => {
        $(

            impl<T> $Delay for DelayFromCountDownTimer<T>
            where
                T: CountDown<Time = Hertz>,
            {
                fn $delay(&mut self, t: u32) {
                    let mut time_left = t;

                    // Due to the LpTimer having only a 3 bit scaler, it is
                    // possible that the max timeout we can set is
                    // (128 * 65536) / clk_hz milliseconds.
                    // Assuming the fastest clk_hz = 480Mhz this is roughly ~17ms,
                    // or a frequency of ~57.2Hz. We use a 60Hz frequency for each
                    // loop step here to ensure that we stay within these bounds.
                    let looping_delay = $num / 60;
                    let looping_delay_hz = Hertz::from_raw($num / looping_delay);

                    self.0.start(looping_delay_hz);
                    while time_left > looping_delay {
                        block!(self.0.wait()).ok();
                        time_left -= looping_delay;
                    }

                    if time_left > 0 {
                        self.0.start(($num / time_left).Hz());
                        block!(self.0.wait()).ok();
                    }
                }
            }
        )+
    }
}

impl_delay_from_count_down_timer! {
    (DelayUs, delay_us, 1_000_000)
}
