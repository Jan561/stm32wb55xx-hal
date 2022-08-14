//! RCC Reset and Clock Control

pub(crate) mod rec;

use crate::flash::Latency;
use crate::pac::{FLASH, PWR, RCC};
use crate::pwr::Pwr;
use crate::pwr::Vos;
use crate::time::Hertz;
use core::convert::Infallible;
use fugit::RateExtU32;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

#[derive(Debug)]
pub struct ValueError(&'static str);

#[derive(Debug)]
pub enum Error {
    SysclkTooHighVosRange2,
    SmpsMsi24MhzTo4MhzIllegal,
    SmpsMsiUnsupportedRange,
    PllEnabled,
    SelectedClockNotEnabled,
    ClockInUse,
    PllNoClockSelected,
    PllClkIllegalRange,
    MsiNotReady,
    LseDisabled,
    PrescalerNotApplied,
    MsiPllDisabled,
}

macro_rules! value_error {
    ($str:expr) => {
        Err(ValueError($str))
    };
}

pub trait RccExt {
    fn constrain(self) -> Rcc;
}

impl RccExt for RCC {
    fn constrain(self) -> Rcc {
        Rcc { rcc: self }
    }
}

type VcoHertz = fugit::Rate<u32, 1, 3>;

enum PllSrcX {
    Msi(MsiRange),
    Hsi16,
    Hse(bool),
}

enum SysclkX {
    Msi(MsiRange),
    Hsi16,
    Hse(bool),
    Pll,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    LsiReady,
    LseReady,
    MsiReady,
    HsiReady,
    HseReady,
    PllReady,
    Pllsai1Ready,
    HseCSS,
    LseCSS,
    Hsi48Ready,
    Lsi2Ready,
}

pub struct Rcc {
    rcc: RCC,
}

impl Rcc {
    pub fn msi_enable(&mut self, en: bool) -> Result<(), Error> {
        if !en && (self.is_sysclk(SysclkSwitch::Msi) || self.is_pllclk(PllSrc::Msi)) {
            return Err(Error::ClockInUse);
        }

        Ok(())
    }

    pub fn msi_range(&mut self, range: MsiRange) -> Result<(), Error> {
        let cr = self.rcc.cr.read();
        let pllcfgr = self.rcc.pllcfgr.read();

        if cr.msion().bit() && !cr.msirdy().bit() {
            return Err(Error::MsiNotReady);
        }

        let is_sysclk = self.is_sysclk(SysclkSwitch::Msi);
        let is_pll_clk = self.is_pllclk(PllSrc::Msi);

        let vos = cortex_m::interrupt::free(|_| {
            let pwr = unsafe { &*PWR::PTR };
            pwr.cr1.read().vos().bits().try_into().unwrap()
        });

        if is_sysclk {
            let cfgr = self.rcc.cfgr.read();
            let extcfgr = self.rcc.extcfgr.read();

            self.check_sysclk(
                range.hertz(),
                cfgr.hpre().bits().try_into().unwrap(),
                extcfgr.c2hpre().bits().try_into().unwrap(),
                extcfgr.shdhpre().bits().try_into().unwrap(),
                vos,
            )?;
        }

        let old_range = MsiRange::try_from(cr.msirange().bits()).unwrap();

        if is_pll_clk {
            let vco_in = Self::pll_m_checked(
                PllSrcX::Msi(range),
                vos,
                pllcfgr.pllm().bits().try_into().unwrap(),
            )?;

            if cr.pllon().bit() {
                Self::check_pll(
                    vco_in,
                    pllcfgr.plln().bits().try_into().unwrap(),
                    pllcfgr.pllp().bits().try_into().unwrap(),
                    pllcfgr.pllq().bits().try_into().unwrap(),
                    pllcfgr.pllr().bits().try_into().unwrap(),
                )?;
            }

            if cr.pllsai1on().bit() {
                let pllsai1cfgr = self.rcc.pllsai1cfgr.read();

                Self::check_pllsai1(
                    vco_in,
                    pllsai1cfgr.plln().bits().try_into().unwrap(),
                    pllsai1cfgr.pllp().bits().try_into().unwrap(),
                    pllsai1cfgr.pllq().bits().try_into().unwrap(),
                    pllsai1cfgr.pllr().bits().try_into().unwrap(),
                )?;
            }
        }

        let flash_setup = || {
            let sysclk = self.calculate_sysclk(SysclkX::Msi(range)).unwrap();
            let shdpre = self.rcc.extcfgr.read().shdhpre().bits().try_into().unwrap();
            let hclk4 = self.calculate_hclk4(sysclk, shdpre);

            set_flash_latency(hclk4);
        };

        if is_sysclk && old_range < range {
            flash_setup();
        }

        self.rcc
            .cr
            .modify(|_, w| w.msirange().variant(range.into()));

        if is_sysclk && old_range > range {
            while self.rcc.cr.read().msirdy().bit_is_clear() {}

            flash_setup();
        }

        Ok(())
    }

    /// MSI PLL Mode / LSE calibration
    pub fn msi_pll_mode(&mut self, en: bool) -> Result<(), Error> {
        let bdcr = self.rcc.bdcr.read();

        if en && (!bdcr.lseon().bit() || !bdcr.lserdy().bit()) {
            return Err(Error::LseDisabled);
        }

        if !en && self.is_pllclk(PllSrc::Msi) {
            return Err(Error::ClockInUse);
        }

        self.rcc.cr.modify(|_, w| w.msipllen().bit(en));

        Ok(())
    }

    pub fn hsi_enable(&mut self, en: bool) -> Result<(), Error> {
        if !en && (self.is_sysclk(SysclkSwitch::Hsi16) || self.is_pllclk(PllSrc::Hsi16)) {
            return Err(Error::ClockInUse);
        }

        self.rcc.cr.modify(|_, w| w.hsion().bit(en));

        Ok(())
    }

    pub fn hsi_ker_enable(&mut self, en: bool) {
        self.rcc.cr.modify(|_, w| w.hsikeron().bit(en));
    }

    pub fn hsi_auto_start(&mut self, en: bool) {
        self.rcc.cr.modify(|_, w| w.hsiasfs().bit(en));
    }

    pub fn hse_enable(&mut self, en: bool) {
        self.rcc.cr.modify(|_, w| w.hseon().bit(en));
    }

    pub fn enable_hse_clock_security_system(&mut self) {
        self.rcc.cr.modify(|_, w| w.csson().set_bit());
    }

    pub fn hse_divider_enabled(&mut self, div_by_2: bool) {
        self.rcc.cr.modify(|_, w| w.hsepre().bit(div_by_2));
    }

    pub fn pll_enabled(&mut self, _: &Pwr, en: bool) -> Result<(), Error> {
        let pllcfgr = self.rcc.pllcfgr.read();

        if !en && self.is_sysclk(SysclkSwitch::Pll) {
            return Err(Error::ClockInUse);
        }

        if en {
            let pwr = unsafe { &*PWR::PTR };
            let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

            let pllsrc: PllSrc = pllcfgr.pllsrc().bits().try_into().unwrap();

            self.check_pllclk_rdy(pllsrc)?;

            let pllsrcx = match pllsrc {
                PllSrc::NoClock => unreachable!(),
                PllSrc::Msi => {
                    if self.rcc.cr.read().msipllen().bit_is_clear() {
                        return Err(Error::MsiPllDisabled);
                    }

                    PllSrcX::Msi(self.rcc.cr.read().msirange().bits().try_into().unwrap())
                }
                PllSrc::Hsi16 => PllSrcX::Hsi16,
                PllSrc::Hse => PllSrcX::Hse(self.rcc.cr.read().hsepre().bit()),
            };

            let vco_in =
                Self::pll_m_checked(pllsrcx, vos, pllcfgr.pllm().bits().try_into().unwrap())?;

            Self::check_pll(
                vco_in,
                pllcfgr.plln().bits().try_into().unwrap(),
                pllcfgr.pllp().bits().try_into().unwrap(),
                pllcfgr.pllq().bits().try_into().unwrap(),
                pllcfgr.pllr().bits().try_into().unwrap(),
            )?;
        }

        self.rcc.cr.modify(|_, w| w.pllon().bit(en));

        Ok(())
    }

    pub fn pllsai1_enabled(&mut self, _: &Pwr, en: bool) -> Result<(), Error> {
        let pllcfgr = self.rcc.pllcfgr.read();

        if en {
            let pwr = unsafe { &*PWR::PTR };
            let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

            let pllsrc: PllSrc = pllcfgr.pllsrc().bits().try_into().unwrap();

            self.check_pllclk_rdy(pllsrc)?;

            let pllsrcx = match pllsrc {
                PllSrc::NoClock => unreachable!(),
                PllSrc::Msi => {
                    if self.rcc.cr.read().msipllen().bit_is_clear() {
                        return Err(Error::MsiPllDisabled);
                    }

                    PllSrcX::Msi(self.rcc.cr.read().msirange().bits().try_into().unwrap())
                }
                PllSrc::Hsi16 => PllSrcX::Hsi16,
                PllSrc::Hse => PllSrcX::Hse(self.rcc.cr.read().hsepre().bit()),
            };

            let vco_in =
                Self::pll_m_checked(pllsrcx, vos, pllcfgr.pllm().bits().try_into().unwrap())?;

            let pllsai1cfgr = self.rcc.pllsai1cfgr.read();

            Self::check_pllsai1(
                vco_in,
                pllsai1cfgr.plln().bits().try_into().unwrap(),
                pllsai1cfgr.pllp().bits().try_into().unwrap(),
                pllsai1cfgr.pllq().bits().try_into().unwrap(),
                pllsai1cfgr.pllr().bits().try_into().unwrap(),
            )?;
        }

        self.rcc.cr.modify(|_, w| w.pllsai1on().bit(en));

        Ok(())
    }

    pub fn sysclk(&mut self, _: &Pwr, sw: SysclkSwitch) -> Result<(), Error> {
        let cr = self.rcc.cr.read();

        let sysclk = |sysclk| {
            let sysclkx = match sysclk {
                SysclkSwitch::Msi => SysclkX::Msi(cr.msirange().bits().try_into().unwrap()),
                SysclkSwitch::Hsi16 => SysclkX::Hsi16,
                SysclkSwitch::Hse => SysclkX::Hse(cr.hsepre().bit()),
                SysclkSwitch::Pll => SysclkX::Pll,
            };

            self.calculate_sysclk(sysclkx)
                .ok_or(Error::SelectedClockNotEnabled)
        };

        let new_sysclk = sysclk(sw)?;

        let pwr = unsafe { &*PWR::PTR };
        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

        let cfgr = self.rcc.cfgr.read();
        let extcfgr = self.rcc.extcfgr.read();

        self.check_sysclk_rdy(sw)?;
        self.check_sysclk(
            new_sysclk,
            cfgr.hpre().bits().try_into().unwrap(),
            extcfgr.c2hpre().bits().try_into().unwrap(),
            extcfgr.shdhpre().bits().try_into().unwrap(),
            vos,
        )?;

        let current_sysclk = sysclk(self.rcc.cfgr.read().sw().bits().try_into().unwrap()).unwrap();

        let flash_setup = || {
            let shdpre = self.rcc.extcfgr.read().shdhpre().bits().try_into().unwrap();
            let hclk4 = self.calculate_hclk4(new_sysclk, shdpre);

            set_flash_latency(hclk4);
        };

        if new_sysclk > current_sysclk {
            flash_setup();
        }

        self.rcc.cfgr.modify(|_, w| w.sw().variant(sw.into()));

        if new_sysclk < current_sysclk {
            while self.rcc.cfgr.read().sws().bits() != sw.into() {}

            flash_setup();
        }

        Ok(())
    }

    pub fn hclk1_prescaler(&mut self, _: &Pwr, scale: PreScaler) -> Result<(), Error> {
        let cr = self.rcc.cr.read();
        let cfgr = self.rcc.cfgr.read();

        let sw: SysclkSwitch = cfgr.sw().bits().try_into().unwrap();
        let sysclkx = match sw {
            SysclkSwitch::Msi => SysclkX::Msi(cr.msirange().bits().try_into().unwrap()),
            SysclkSwitch::Hsi16 => SysclkX::Hsi16,
            SysclkSwitch::Hse => SysclkX::Hse(cr.hsepre().bit()),
            SysclkSwitch::Pll => SysclkX::Pll,
        };

        let pwr = unsafe { &*PWR::PTR };
        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

        let extcfgr = self.rcc.extcfgr.read();

        let sysclk = self.calculate_sysclk(sysclkx).unwrap();

        self.check_sysclk(
            sysclk,
            scale,
            extcfgr.c2hpre().bits().try_into().unwrap(),
            extcfgr.shdhpre().bits().try_into().unwrap(),
            vos,
        )?;

        self.rcc.cfgr.modify(|_, w| w.hpre().variant(scale.into()));

        Ok(())
    }

    pub fn hclk2_prescaler(&mut self, _: &Pwr, scale: PreScaler) -> Result<(), Error> {
        let cr = self.rcc.cr.read();
        let cfgr = self.rcc.cfgr.read();

        let sw: SysclkSwitch = cfgr.sw().bits().try_into().unwrap();
        let sysclkx = match sw {
            SysclkSwitch::Msi => SysclkX::Msi(cr.msirange().bits().try_into().unwrap()),
            SysclkSwitch::Hsi16 => SysclkX::Hsi16,
            SysclkSwitch::Hse => SysclkX::Hse(cr.hsepre().bit()),
            SysclkSwitch::Pll => SysclkX::Pll,
        };

        let pwr = unsafe { &*PWR::PTR };
        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

        let extcfgr = self.rcc.extcfgr.read();

        let sysclk = self.calculate_sysclk(sysclkx).unwrap();

        self.check_sysclk(
            sysclk,
            cfgr.hpre().bits().try_into().unwrap(),
            scale,
            extcfgr.shdhpre().bits().try_into().unwrap(),
            vos,
        )?;

        self.rcc
            .extcfgr
            .modify(|_, w| w.c2hpre().variant(scale.into()));

        Ok(())
    }

    pub fn hclk4_prescaler(&mut self, _: &Pwr, scale: PreScaler) -> Result<(), Error> {
        let cr = self.rcc.cr.read();
        let cfgr = self.rcc.cfgr.read();

        let sw: SysclkSwitch = cfgr.sw().bits().try_into().unwrap();
        let sysclkx = match sw {
            SysclkSwitch::Msi => SysclkX::Msi(cr.msirange().bits().try_into().unwrap()),
            SysclkSwitch::Hsi16 => SysclkX::Hsi16,
            SysclkSwitch::Hse => SysclkX::Hse(cr.hsepre().bit()),
            SysclkSwitch::Pll => SysclkX::Pll,
        };

        let pwr = unsafe { &*PWR::PTR };
        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

        let extcfgr = self.rcc.extcfgr.read();

        let sysclk = self.calculate_sysclk(sysclkx).unwrap();

        self.check_sysclk(
            sysclk,
            cfgr.hpre().bits().try_into().unwrap(),
            extcfgr.c2hpre().bits().try_into().unwrap(),
            scale,
            vos,
        )?;

        let flash_setup = || {
            let hclk4 = self.calculate_hclk4(sysclk, scale);

            set_flash_latency(hclk4);
        };

        let current_scale = self.rcc.extcfgr.read().shdhpre().bits().try_into().unwrap();

        if scale < current_scale {
            flash_setup();
        }

        self.rcc
            .extcfgr
            .modify(|_, w| w.c2hpre().variant(scale.into()));

        if scale > current_scale {
            while self.rcc.extcfgr.read().shdhpref().bit_is_clear() {}

            flash_setup();
        }

        Ok(())
    }

    pub fn pclk1_prescaler(&mut self, scale: PpreScaler) {
        self.rcc.cfgr.modify(|_, w| w.ppre1().variant(scale.into()));
    }

    pub fn pclk2_prescaler(&mut self, scale: PpreScaler) {
        self.rcc.cfgr.modify(|_, w| w.ppre2().variant(scale.into()));
    }

    pub fn rf_clock(&self) -> RfClock {
        if self.rcc.extcfgr.read().rfcss().bit() {
            RfClock::Hse
        } else {
            RfClock::Hsi16
        }
    }

    pub fn stop_css_wakeup_clock(&mut self, clk: Stopwuck) {
        self.rcc
            .cfgr
            .modify(|_, w| w.stopwuck().variant(clk == Stopwuck::Hsi16));
    }

    pub fn mco(&mut self, clk: McoSelector, scale: McoPrescaler) {
        self.rcc
            .cfgr
            .modify(|_, w| w.mcopre().variant(scale.into()));
        self.rcc.cfgr.modify(|_, w| w.mcosel().variant(clk.into()));
    }

    pub fn pll_src(&mut self, src: PllSrc) -> Result<(), Error> {
        if self.rcc.cr.read().pllon().bit() || self.rcc.cr.read().pllsai1on().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllcfgr
            .modify(|_, w| w.pllsrc().variant(src.into()));

        Ok(())
    }

    pub fn pllm(&mut self, pllm: Pllm) -> Result<(), Error> {
        if self.rcc.cr.read().pllon().bit() || self.rcc.cr.read().pllsai1on().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllcfgr
            .modify(|_, w| w.pllm().variant(pllm.into()));

        Ok(())
    }

    pub fn plln(&mut self, plln: Plln) -> Result<(), Error> {
        if self.rcc.cr.read().pllon().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllcfgr
            .modify(|_, w| w.plln().variant(plln.into()));

        Ok(())
    }

    pub fn pllp(&mut self, pllp: Pllp) -> Result<(), Error> {
        if self.rcc.cr.read().pllon().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllcfgr
            .modify(|_, w| w.pllp().variant(pllp.into()));

        Ok(())
    }

    pub fn pllp_enable(&mut self, en: bool) {
        self.rcc.pllcfgr.modify(|_, w| w.pllpen().bit(en));
    }

    pub fn pllq(&mut self, pllq: PllQR) -> Result<(), Error> {
        if self.rcc.cr.read().pllon().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllcfgr
            .modify(|_, w| w.pllq().variant(pllq.into()));

        Ok(())
    }

    pub fn pllq_enable(&mut self, en: bool) {
        self.rcc.pllcfgr.modify(|_, w| w.pllqen().bit(en));
    }

    pub fn pllr(&mut self, pllr: PllQR) -> Result<(), Error> {
        if self.rcc.cr.read().pllon().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllcfgr
            .modify(|_, w| w.pllr().variant(pllr.into()));

        Ok(())
    }

    pub fn pllr_enable(&mut self, en: bool) {
        self.rcc.pllcfgr.modify(|_, w| w.pllren().bit(en));
    }

    pub fn pllsai1n(&mut self, plln: Pllsai1N) -> Result<(), Error> {
        if self.rcc.cr.read().pllsai1on().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllsai1cfgr
            .modify(|_, w| w.plln().variant(plln.into()));

        Ok(())
    }

    pub fn pllsai1p(&mut self, pllp: Pllp) -> Result<(), Error> {
        if self.rcc.cr.read().pllsai1on().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllsai1cfgr
            .modify(|_, w| w.pllp().variant(pllp.into()));

        Ok(())
    }

    pub fn pllsai1p_enable(&mut self, en: bool) {
        self.rcc.pllsai1cfgr.modify(|_, w| w.pllpen().bit(en));
    }

    pub fn pllsai1q(&mut self, pllq: PllQR) -> Result<(), Error> {
        if self.rcc.cr.read().pllsai1on().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllsai1cfgr
            .modify(|_, w| w.pllq().variant(pllq.into()));

        Ok(())
    }

    pub fn pllsai1q_enable(&mut self, en: bool) {
        self.rcc.pllsai1cfgr.modify(|_, w| w.pllqen().bit(en));
    }

    pub fn pllsai1r(&mut self, pllr: PllQR) -> Result<(), Error> {
        if self.rcc.cr.read().pllsai1on().bit() {
            return Err(Error::PllEnabled);
        }

        self.rcc
            .pllsai1cfgr
            .modify(|_, w| w.pllr().variant(pllr.into()));

        Ok(())
    }

    pub fn pllsai1r_enable(&mut self, en: bool) {
        self.rcc.pllsai1cfgr.modify(|_, w| w.pllren().bit(en));
    }

    pub fn usart1_clock(&mut self, clock: Usart1sel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.usart1sel().variant(clock.into()));
    }

    pub fn lp_uart1_clock(&mut self, clock: Usart1sel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.lpuart1sel().variant(clock.into()));
    }

    pub fn i2c1_clock(&mut self, clock: I2cSel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.i2c1sel().variant(clock.into()));
    }

    pub fn i2c3_clock(&mut self, clock: I2cSel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.i2c3sel().variant(clock.into()));
    }

    pub fn lptim1_clock(&mut self, clock: LptimSel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.lptim1sel().variant(clock.into()));
    }

    pub fn lptim2_clock(&mut self, clock: LptimSel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.lptim2sel().variant(clock.into()));
    }

    pub fn sai1_clock(&mut self, clock: Sai1Sel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.sai1sel().variant(clock.into()));
    }

    pub fn clock_48(&mut self, clock: Clk48Sel) {
        // TODO: PLL check necessary??
        self.rcc
            .ccipr
            .modify(|_, w| w.clk48sel().variant(clock.into()));
    }

    pub fn adc_clock(&mut self, clock: AdcSel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.adcsel().variant(clock.into()));
    }

    pub fn rng_clock(&mut self, clock: RngSel) {
        self.rcc
            .ccipr
            .modify(|_, w| w.rngsel().variant(clock.into()));
    }

    pub fn listen(&mut self, event: Event, listen: bool) {
        self.rcc.cier.modify(|_, w| match event {
            Event::LsiReady => w.lsi1rdyie().bit(listen),
            Event::LseReady => w.lserdyie().bit(listen),
            Event::MsiReady => w.msirdyie().bit(listen),
            Event::HsiReady => w.hsirdyie().bit(listen),
            Event::HseReady => w.hserdyie().bit(listen),
            Event::PllReady => w.pllrdyie().bit(listen),
            Event::Pllsai1Ready => w.pllsai1rdyie().bit(listen),
            Event::HseCSS => panic!("There is no HSECSS interrupt"),
            Event::LseCSS => w.lsecssie().bit(listen),
            Event::Hsi48Ready => w.hsi48rdyie().bit(listen),
            Event::Lsi2Ready => w.lsi2rdyie().bit(listen),
        });
    }

    pub fn clear_irq(&mut self, event: Event) {
        self.rcc.cicr.write(|w| match event {
            Event::LsiReady => w.lsi1rdyc().set_bit(),
            Event::LseReady => w.lserdyc().set_bit(),
            Event::MsiReady => w.msirdyc().set_bit(),
            Event::HsiReady => w.hsirdyc().set_bit(),
            Event::HseReady => w.hserdyc().set_bit(),
            Event::PllReady => w.pllrdyc().set_bit(),
            Event::Pllsai1Ready => w.pllsai1rdyc().set_bit(),
            Event::HseCSS => w.hsecssc().set_bit(),
            Event::LseCSS => w.lsecssc().set_bit(),
            Event::Hsi48Ready => w.hsi48rdyc().set_bit(),
            Event::Lsi2Ready => w.lsi2rdyc().set_bit(),
        });
    }

    fn pll_m_checked(src: PllSrcX, vos: Vos, pllm: Pllm) -> Result<VcoHertz, Error> {
        let pll_m_in = match src {
            PllSrcX::Msi(r) => r.hertz(),
            PllSrcX::Hsi16 => hsi16_hertz(),
            PllSrcX::Hse(pre) => hse_output_hertz(pre),
        };

        if vos == Vos::Range2 && pll_m_in > 16.MHz::<1, 1>() {
            return Err(Error::PllClkIllegalRange);
        }

        let vco_in = pll_m_in.convert() / pllm.div_factor() as u32;

        if vco_in > 16.MHz::<1, 3>() || vco_in < fugit::Rate::<u32, 1, 3>::from_raw(8_000_000) {
            return Err(Error::PllClkIllegalRange);
        }

        Ok(vco_in)
    }

    fn pll_m(src: PllSrcX, pllm: Pllm) -> VcoHertz {
        let pll_m_in = match src {
            PllSrcX::Msi(r) => r.hertz(),
            PllSrcX::Hsi16 => hsi16_hertz(),
            PllSrcX::Hse(pre) => hse_output_hertz(pre),
        };

        pll_m_in.convert() / pllm.div_factor() as u32
    }

    fn pll_n_checked(vco_in: VcoHertz, plln: Plln) -> Result<VcoHertz, Error> {
        vco_in
            .raw()
            .checked_mul(plln.get() as u32)
            .map(VcoHertz::from_raw)
            .filter(|&v| v >= VcoHertz::MHz(96) && v <= VcoHertz::MHz(344))
            .ok_or(Error::PllClkIllegalRange)
    }

    fn pll_n(vco_in: VcoHertz, plln: Plln) -> VcoHertz {
        vco_in * plln.get() as u32
    }

    fn check_pll(
        vco_in: VcoHertz,
        plln: Plln,
        pllp: Pllp,
        pllq: PllQR,
        pllr: PllQR,
    ) -> Result<(), Error> {
        let vco_out = Self::pll_n_checked(vco_in, plln)?;

        let pllp = vco_out / pllp.get() as u32;
        let pllq = vco_out / pllq.div_factor() as u32;
        let pllr = vco_out / pllr.div_factor() as u32;

        if [pllp, pllq, pllr].into_iter().max().unwrap() > VcoHertz::MHz(64) {
            return Err(Error::PllClkIllegalRange);
        }

        Ok(())
    }

    fn pllsai1_n_checked(vco_in: VcoHertz, plln: Pllsai1N) -> Result<VcoHertz, Error> {
        vco_in
            .raw()
            .checked_mul(plln.get() as u32)
            .map(VcoHertz::from_raw)
            .filter(|&v| v >= VcoHertz::MHz(64) && v <= VcoHertz::MHz(344))
            .ok_or(Error::PllClkIllegalRange)
    }

    fn check_pllsai1(
        vco_in: VcoHertz,
        plln: Pllsai1N,
        pllp: Pllp,
        pllq: PllQR,
        pllr: PllQR,
    ) -> Result<(), Error> {
        let vco_out = Self::pllsai1_n_checked(vco_in, plln)?;

        let pllp = vco_out / pllp.get() as u32;
        let pllq = vco_out / pllq.div_factor() as u32;
        let pllr = vco_out / pllr.div_factor() as u32;

        if [pllp, pllq, pllr].into_iter().max().unwrap() > VcoHertz::MHz(64) {
            return Err(Error::PllClkIllegalRange);
        }

        Ok(())
    }

    fn pll_r(&self, vco_out: VcoHertz) -> Option<VcoHertz> {
        let pllcfgr = self.rcc.pllcfgr.read();

        if pllcfgr.pllren().bit_is_clear() {
            return None;
        }

        let pllr: PllQR = pllcfgr.pllr().bits().try_into().unwrap();

        Some(vco_out / pllr.div_factor() as u32)
    }

    fn calculate_sysclk(&self, sw: SysclkX) -> Option<Hertz> {
        match sw {
            SysclkX::Msi(range) => Some(range.hertz()),
            SysclkX::Hsi16 => Some(hsi16_hertz()),
            SysclkX::Hse(pre) => Some(hse_output_hertz(pre)),
            SysclkX::Pll => {
                let pllcfgr = self.rcc.pllcfgr.read();
                let pllsrc: PllSrc = pllcfgr.pllsrc().bits().try_into().unwrap();

                let pllsrcx = match pllsrc {
                    PllSrc::NoClock => return None,
                    PllSrc::Msi => {
                        PllSrcX::Msi(self.rcc.cr.read().msirange().bits().try_into().unwrap())
                    }
                    PllSrc::Hsi16 => PllSrcX::Hsi16,
                    PllSrc::Hse => PllSrcX::Hse(self.rcc.cr.read().hsepre().bit()),
                };
                let pllm = pllcfgr.pllm().bits().try_into().unwrap();
                let plln = pllcfgr.plln().bits().try_into().unwrap();

                let vco_in = Self::pll_m(pllsrcx, pllm);
                let vco_out = Self::pll_n(vco_in, plln);

                self.pll_r(vco_out).map(|x| x.convert())
            }
        }
    }

    fn check_sysclk(
        &self,
        sysclk: Hertz,
        hpre: PreScaler,
        c2hpre: PreScaler,
        shdpre: PreScaler,
        vos: Vos,
    ) -> Result<(), Error> {
        self.check_hclk1(sysclk, hpre, vos)?;
        self.check_hclk2(sysclk, c2hpre, vos)?;
        self.check_hclk4(sysclk, shdpre, vos)?;

        Ok(())
    }

    fn calculate_hclk1(&self, sysclk: Hertz, hpre: PreScaler) -> Hertz {
        sysclk / hpre.div_scale() as u32
    }

    fn check_hclk1(&self, sysclk: Hertz, hpre: PreScaler, vos: Vos) -> Result<(), Error> {
        if self.rcc.cfgr.read().hpref().bit_is_clear() {
            return Err(Error::PrescalerNotApplied);
        }
        if vos == Vos::Range2 && sysclk > 16.MHz::<1, 1>() * hpre.div_scale() as u32 {
            return Err(Error::SysclkTooHighVosRange2);
        }

        Ok(())
    }

    fn calculate_hclk2(&self, sysclk: Hertz, c2hpre: PreScaler) -> Hertz {
        sysclk / c2hpre.div_scale() as u32
    }

    fn check_hclk2(&self, sysclk: Hertz, c2hpre: PreScaler, vos: Vos) -> Result<(), Error> {
        if self.rcc.extcfgr.read().c2hpref().bit_is_clear() {
            return Err(Error::PrescalerNotApplied);
        }
        if vos == Vos::Range2 && sysclk > 16.MHz::<1, 1>() * c2hpre.div_scale() as u32 {
            return Err(Error::SysclkTooHighVosRange2);
        }

        Ok(())
    }

    fn calculate_hclk4(&self, sysclk: Hertz, shdpre: PreScaler) -> Hertz {
        sysclk / shdpre.div_scale() as u32
    }

    fn check_hclk4(&self, sysclk: Hertz, shdpre: PreScaler, vos: Vos) -> Result<(), Error> {
        if self.rcc.extcfgr.read().shdhpref().bit_is_clear() {
            return Err(Error::PrescalerNotApplied);
        }
        if vos == Vos::Range2 && sysclk > 16.MHz::<1, 1>() * shdpre.div_scale() as u32 {
            return Err(Error::SysclkTooHighVosRange2);
        }

        Ok(())
    }

    fn is_sysclk(&self, clk: SysclkSwitch) -> bool {
        let cfgr = self.rcc.cfgr.read();

        cfgr.sw().bits() == clk.into() || cfgr.sws().bits() == clk.into()
    }

    fn is_pllclk(&self, clk: PllSrc) -> bool {
        let cr = self.rcc.cr.read();
        let pllcfgr = self.rcc.pllcfgr.read();

        (cr.pllon().bit() || cr.pllsai1on().bit()) && pllcfgr.pllsrc().bits() == clk.into()
    }

    fn check_sysclk_rdy(&self, sysclk: SysclkSwitch) -> Result<(), Error> {
        if self.sysclk_is_rdy(sysclk) {
            Ok(())
        } else {
            Err(Error::SelectedClockNotEnabled)
        }
    }

    fn sysclk_is_rdy(&self, sysclk: SysclkSwitch) -> bool {
        let cr = self.rcc.cr.read();

        match sysclk {
            SysclkSwitch::Msi => cr.msirdy().bit(),
            SysclkSwitch::Hsi16 => cr.hsirdy().bit() || cr.hsikerdy().bit(),
            SysclkSwitch::Hse => cr.hserdy().bit(),
            SysclkSwitch::Pll => cr.pllrdy().bit(),
        }
    }

    fn check_pllclk_rdy(&self, clk: PllSrc) -> Result<(), Error> {
        if self.pllclk_is_rdy(clk) {
            Ok(())
        } else {
            Err(Error::SelectedClockNotEnabled)
        }
    }

    fn pllclk_is_rdy(&self, clk: PllSrc) -> bool {
        let cr = self.rcc.cr.read();

        match clk {
            PllSrc::NoClock => false,
            PllSrc::Msi => cr.msirdy().bit(),
            PllSrc::Hsi16 => cr.hsirdy().bit() || cr.hsikerdy().bit(),
            PllSrc::Hse => cr.hserdy().bit(),
        }
    }

    fn calculate_pclk1(&self, hclk1: Hertz, ppre1: PpreScaler) -> Hertz {
        hclk1 / ppre1.div_scale() as u32
    }

    fn calculate_pclk2(&self, hclk1: Hertz, ppre2: PpreScaler) -> Hertz {
        hclk1 / ppre2.div_scale() as u32
    }
}

pub(crate) fn set_flash_latency(hclk4: Hertz) {
    // SAFETY: No safety critical accesses performed
    let pwr = unsafe { &*PWR::PTR };
    // SAFETY: No safety critical accesses performed
    let flash = unsafe { &*FLASH::PTR };

    cortex_m::interrupt::free(|_| {
        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();
        let latency = Latency::from(vos, hclk4);

        flash.acr.modify(|_, w| w.latency().variant(latency.into()));
    });
}

pub trait TryClocks<'a> {
    fn try_sysclk(&self) -> nb::Result<Hertz, Infallible>;
    fn try_hclk1(&self) -> nb::Result<Hertz, Infallible>;
    fn try_hclk2(&self) -> nb::Result<Hertz, Infallible>;
    fn try_hclk4(&self) -> nb::Result<Hertz, Infallible>;
    fn try_pclk1(&self) -> nb::Result<Hertz, Infallible>;
    fn try_pclk2(&self) -> nb::Result<Hertz, Infallible>;
    fn try_i2c1_clk(&self) -> nb::Result<Hertz, Infallible>;
    fn try_i2c3_clk(&self) -> nb::Result<Hertz, Infallible>;
}

impl TryClocks<'static> for Rcc {
    fn try_sysclk(&self) -> nb::Result<Hertz, Infallible> {
        let cfgr = self.rcc.cfgr.read();
        if cfgr.sw().bits() != cfgr.sws().bits() {
            return Err(nb::Error::WouldBlock);
        }

        let sysclk: SysclkSwitch = cfgr.sw().bits().try_into().unwrap();
        let sysclkx = match sysclk {
            SysclkSwitch::Msi => {
                SysclkX::Msi(self.rcc.cr.read().msirange().bits().try_into().unwrap())
            }
            SysclkSwitch::Hsi16 => SysclkX::Hsi16,
            SysclkSwitch::Hse => SysclkX::Hse(self.rcc.cr.read().hsepre().bit()),
            SysclkSwitch::Pll => SysclkX::Pll,
        };

        Ok(self.calculate_sysclk(sysclkx).unwrap())
    }

    fn try_hclk1(&self) -> nb::Result<Hertz, Infallible> {
        let hpre = self.rcc.cfgr.read().hpre().bits().try_into().unwrap();
        self.try_sysclk().map(|x| self.calculate_hclk1(x, hpre))
    }

    fn try_hclk2(&self) -> nb::Result<Hertz, Infallible> {
        let c2hpre = self.rcc.extcfgr.read().c2hpre().bits().try_into().unwrap();
        self.try_sysclk().map(|x| self.calculate_hclk2(x, c2hpre))
    }

    fn try_hclk4(&self) -> nb::Result<Hertz, Infallible> {
        let shdpre = self.rcc.extcfgr.read().shdhpre().bits().try_into().unwrap();
        self.try_sysclk().map(|x| self.calculate_hclk4(x, shdpre))
    }

    fn try_pclk1(&self) -> nb::Result<Hertz, Infallible> {
        let ppre1 = self.rcc.cfgr.read().ppre1().bits().try_into().unwrap();
        self.try_sysclk().map(|x| self.calculate_pclk1(x, ppre1))
    }

    fn try_pclk2(&self) -> nb::Result<Hertz, Infallible> {
        let ppre2 = self.rcc.cfgr.read().ppre2().bits().try_into().unwrap();
        self.try_sysclk().map(|x| self.calculate_pclk2(x, ppre2))
    }

    fn try_i2c1_clk(&self) -> nb::Result<Hertz, Infallible> {
        let i2c_clk: I2cSel = self.rcc.ccipr.read().i2c1sel().bits().try_into().unwrap();

        match i2c_clk {
            I2cSel::Pclk => self.try_pclk1(),
            I2cSel::Sysclk => self.try_sysclk(),
            I2cSel::Hsi16 => Ok(hsi16_hertz()),
        }
    }

    fn try_i2c3_clk(&self) -> nb::Result<Hertz, Infallible> {
        let i2c_clk: I2cSel = self.rcc.ccipr.read().i2c3sel().bits().try_into().unwrap();

        match i2c_clk {
            I2cSel::Pclk => self.try_pclk1(),
            I2cSel::Sysclk => self.try_sysclk(),
            I2cSel::Hsi16 => Ok(hsi16_hertz()),
        }
    }
}

impl<'a> TryClocks<'a> for &'a Rcc {
    fn try_sysclk(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_sysclk()
    }

    fn try_hclk1(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_hclk1()
    }

    fn try_hclk2(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_hclk2()
    }

    fn try_hclk4(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_hclk4()
    }

    fn try_pclk1(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_pclk1()
    }

    fn try_pclk2(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_pclk2()
    }

    fn try_i2c1_clk(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_i2c1_clk()
    }

    fn try_i2c3_clk(&self) -> nb::Result<Hertz, Infallible> {
        (*self).try_i2c3_clk()
    }
}

pub trait Clocks<'a> {
    fn sysclk(&self) -> Hertz;
    fn hclk1(&self) -> Hertz;
    fn hclk2(&self) -> Hertz;
    fn hclk4(&self) -> Hertz;
    fn pclk1(&self) -> Hertz;
    fn pclk2(&self) -> Hertz;
    fn i2c1_clk(&self) -> Hertz;
    fn i2c3_clk(&self) -> Hertz;
}

impl<'a, T> TryClocks<'a> for T
where
    T: Clocks<'a>,
{
    fn try_sysclk(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.sysclk())
    }

    fn try_hclk1(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.hclk1())
    }

    fn try_hclk2(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.hclk2())
    }

    fn try_hclk4(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.hclk4())
    }

    fn try_pclk1(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.pclk1())
    }

    fn try_pclk2(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.pclk2())
    }

    fn try_i2c1_clk(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.i2c1_clk())
    }

    fn try_i2c3_clk(&self) -> nb::Result<Hertz, Infallible> {
        Ok(self.i2c3_clk())
    }
}

pub struct Ccdr {
    sysclk: Hertz,
    hclk1: Hertz,
    hclk2: Hertz,
    hclk4: Hertz,
    pclk1: Hertz,
    pclk2: Hertz,
    i2c1_clk: Hertz,
    i2c3_clk: Hertz,
}

impl Clocks<'static> for Ccdr {
    fn sysclk(&self) -> Hertz {
        self.sysclk
    }

    fn hclk1(&self) -> Hertz {
        self.hclk1
    }

    fn hclk2(&self) -> Hertz {
        self.hclk2
    }

    fn hclk4(&self) -> Hertz {
        self.hclk4
    }

    fn pclk1(&self) -> Hertz {
        self.pclk1
    }

    fn pclk2(&self) -> Hertz {
        self.pclk2
    }

    fn i2c1_clk(&self) -> Hertz {
        self.i2c1_clk
    }

    fn i2c3_clk(&self) -> Hertz {
        self.i2c3_clk
    }
}

impl Clocks<'static> for &'_ Ccdr {
    fn sysclk(&self) -> Hertz {
        (*self).sysclk()
    }

    fn hclk1(&self) -> Hertz {
        (*self).hclk1()
    }

    fn hclk2(&self) -> Hertz {
        (*self).hclk2()
    }

    fn hclk4(&self) -> Hertz {
        (*self).hclk4()
    }

    fn pclk1(&self) -> Hertz {
        (*self).pclk1()
    }

    fn pclk2(&self) -> Hertz {
        (*self).pclk2()
    }

    fn i2c1_clk(&self) -> Hertz {
        (*self).i2c1_clk()
    }

    fn i2c3_clk(&self) -> Hertz {
        (*self).i2c3_clk()
    }
}

pub struct Unwrap<T>(pub T);

impl<'a, T> Clocks<'a> for Unwrap<T>
where
    T: TryClocks<'a>,
{
    fn sysclk(&self) -> Hertz {
        self.try_hclk1().unwrap()
    }

    fn hclk1(&self) -> Hertz {
        self.try_hclk1().unwrap()
    }

    fn hclk2(&self) -> Hertz {
        self.try_hclk2().unwrap()
    }

    fn hclk4(&self) -> Hertz {
        self.try_hclk4().unwrap()
    }

    fn pclk1(&self) -> Hertz {
        self.try_pclk1().unwrap()
    }

    fn pclk2(&self) -> Hertz {
        self.try_pclk2().unwrap()
    }

    fn i2c1_clk(&self) -> Hertz {
        self.try_i2c1_clk().unwrap()
    }

    fn i2c3_clk(&self) -> Hertz {
        self.try_i2c3_clk().unwrap()
    }
}

pub struct Block<T>(pub T);

impl<'a, T> Clocks<'a> for Block<T>
where
    T: TryClocks<'a>,
{
    fn sysclk(&self) -> Hertz {
        nb::block!(self.try_sysclk()).unwrap()
    }

    fn hclk1(&self) -> Hertz {
        nb::block!(self.try_hclk1()).unwrap()
    }

    fn hclk2(&self) -> Hertz {
        nb::block!(self.try_hclk2()).unwrap()
    }

    fn hclk4(&self) -> Hertz {
        nb::block!(self.try_hclk4()).unwrap()
    }

    fn pclk1(&self) -> Hertz {
        nb::block!(self.try_pclk1()).unwrap()
    }

    fn pclk2(&self) -> Hertz {
        nb::block!(self.try_pclk2()).unwrap()
    }
    fn i2c1_clk(&self) -> Hertz {
        nb::block!(self.try_i2c1_clk()).unwrap()
    }

    fn i2c3_clk(&self) -> Hertz {
        nb::block!(self.try_i2c3_clk()).unwrap()
    }
}

pub unsafe trait TrustedClocks<'a> {}

unsafe impl TrustedClocks<'static> for Ccdr {}
unsafe impl TrustedClocks<'static> for &'_ Ccdr {}
unsafe impl TrustedClocks<'static> for Rcc {}
unsafe impl<'a> TrustedClocks<'a> for &'a Rcc {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopwuck {
    Msi,
    Hsi16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RfClock {
    Hsi16,
    Hse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum MsiRange {
    /// 100 KHz
    R100K = 0b0000,
    /// 200 KHz
    R200K = 0b0001,
    /// 400 KHz
    R400K = 0b0010,
    /// 800 KHz
    R800K = 0b0011,
    /// 1 MHz
    R1M = 0b0100,
    /// 2 MHz
    R2M = 0b0101,
    /// 4 MHz
    R4M = 0b0110,
    /// 8 MHz
    R8M = 0b0111,
    /// 16 MHz
    R16M = 0b1000,
    /// 24 MHz
    R24M = 0b1001,
    /// 32 MHz
    R32M = 0b1010,
    /// 48 MHz
    R48M = 0b1011,
}

impl MsiRange {
    pub const fn hertz(self) -> Hertz {
        match self {
            Self::R100K => Hertz::Hz(100_000),
            Self::R200K => Hertz::Hz(200_000),
            Self::R400K => Hertz::Hz(400_000),
            Self::R800K => Hertz::Hz(800_000),
            Self::R1M => Hertz::Hz(1_000_000),
            Self::R2M => Hertz::Hz(2_000_000),
            Self::R4M => Hertz::Hz(4_000_000),
            Self::R8M => Hertz::Hz(8_000_000),
            Self::R16M => Hertz::Hz(16_000_000),
            Self::R24M => Hertz::Hz(24_000_000),
            Self::R32M => Hertz::Hz(32_000_000),
            Self::R48M => Hertz::Hz(48_000_000),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum McoPrescaler {
    /// MCO divided by 1
    D1 = 0b000,
    /// MCO divided by 2
    D2 = 0b001,
    /// MCO divided by 4
    D4 = 0b010,
    /// MCO divided by 8
    D8 = 0b011,
    /// MCO divided by 16
    D16 = 0b100,
}

impl McoPrescaler {
    pub fn div_scale(self) -> u8 {
        match self {
            Self::D1 => 1,
            Self::D2 => 2,
            Self::D4 => 4,
            Self::D8 => 8,
            Self::D16 => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum McoSelector {
    /// MCO output disabled, no clock on MCO
    Disabled = 0b0000,
    /// SYSCLK system clock
    Sysclk = 0b0001,
    /// MSI clock
    Msi = 0b0010,
    /// HSI16 clock
    Hsi16 = 0b0011,
    /// HSE clock (after stabilization, after HSERDY=1)
    HseAfter = 0b0100,
    /// Main PLLRCLK clock
    Pllrclk = 0b0101,
    /// LSI1 clock
    Lsi1 = 0b0110,
    /// LSI2 clock
    Lsi2 = 0b0111,
    /// LSE clock
    Lse = 0b1000,
    /// Internal HSI48 clock
    Hsi48 = 0b1001,
    /// HSE clock (before stabilization, after HSEON=1)
    HseBefore = 0b1100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum PpreScaler {
    /// Not divided
    #[default]
    D1 = 0b000,
    /// Divided by 2
    D2 = 0b100,
    /// Divided by 4
    D4 = 0b101,
    /// Divided by 8
    D8 = 0b110,
    /// Divided by 16
    D16 = 0b111,
}

impl PpreScaler {
    /// Division scale factor
    pub const fn div_scale(self) -> u8 {
        match self {
            Self::D1 => 1,
            Self::D2 => 2,
            Self::D4 => 4,
            Self::D8 => 8,
            Self::D16 => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum PreScaler {
    /// Not divided
    #[default]
    D1 = 0b0000,
    /// Divided by 3
    D3 = 0b0001,
    /// Divided by 5
    D5 = 0b0010,
    /// Divided by 6
    D6 = 0b0101,
    /// Divided by 10
    D10 = 0b0110,
    /// Divided by 32
    D32 = 0b0111,
    /// Divided by 2
    D2 = 0b1000,
    /// Divided by 4
    D4 = 0b1001,
    /// Divided by 8
    D8 = 0b1010,
    /// Divided by 16
    D16 = 0b1011,
    /// Divided by 64
    D64 = 0b1100,
    /// Divided by 128
    D128 = 0b1101,
    /// Divided by 256
    D256 = 0b1110,
    /// Divided by 512
    D512 = 0b1111,
}

impl PreScaler {
    /// Division scale factor
    pub const fn div_scale(self) -> u16 {
        match self {
            Self::D1 => 1,
            Self::D3 => 3,
            Self::D5 => 5,
            Self::D6 => 6,
            Self::D10 => 10,
            Self::D32 => 32,
            Self::D2 => 2,
            Self::D4 => 4,
            Self::D8 => 8,
            Self::D16 => 16,
            Self::D64 => 64,
            Self::D128 => 128,
            Self::D256 => 256,
            Self::D512 => 512,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum SysclkSwitch {
    Msi = 0b00,
    Hsi16 = 0b01,
    Hse = 0b10,
    Pll = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum PllSrc {
    NoClock = 0b00,
    Msi = 0b01,
    Hsi16 = 0b10,
    Hse = 0b11,
}

/// Division factor for the main PLL and audio PLLSAI1 input clock
///
/// The software has to set these bits to ensure that the VCO input frequency
/// ranges from 2.66 to 16 MHz
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Pllm {
    /// Not divided
    D1 = 0b000,
    /// Divided by 2
    D2 = 0b001,
    /// Divided by 3
    D3 = 0b010,
    /// Divided by 4
    D4 = 0b011,
    /// Divided by 5
    D5 = 0b100,
    /// Divided by 6
    D6 = 0b101,
    /// Divided by 7
    D7 = 0b110,
    /// Divided by 8
    D8 = 0b111,
}

impl Pllm {
    pub const fn div_factor(self) -> u8 {
        match self {
            Self::D1 => 1,
            Self::D2 => 2,
            Self::D3 => 3,
            Self::D4 => 4,
            Self::D5 => 5,
            Self::D6 => 6,
            Self::D7 => 7,
            Self::D8 => 8,
        }
    }
}

//
// MAIN PLL
//

/// Main PLL multiplication factor for VCO
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plln(u8);

impl Plln {
    pub fn new(x: u8) -> Result<Self, ValueError> {
        if x < 6 || x > 127 {
            return value_error!("PLLN must be in range of [6, 127]");
        }

        Ok(Self(x))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Plln {
    type Error = ValueError;

    fn try_from(x: u8) -> Result<Self, Self::Error> {
        Self::new(x)
    }
}

impl From<Plln> for u8 {
    fn from(x: Plln) -> u8 {
        x.0
    }
}

pub struct Pllp(u8);

impl Pllp {
    /// Main PLL and PLLSAI1 division factor for PLLCLK and PLLSAI1PCLK
    ///
    /// Note: The software has to set these bits so that 64 MHz is not exceeded on
    /// this domain
    ///
    /// # Arguments
    ///
    /// - `x`: Desired division factor. Must be in range of [2, 32]
    pub fn new(x: u8) -> Result<Self, ValueError> {
        if x < 2 || x > 32 {
            return value_error!("Main PLL division factor must be in range of [2, 32]");
        }

        Ok(Self(x))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl From<Pllp> for u8 {
    fn from(x: Pllp) -> u8 {
        x.0 - 1
    }
}

impl TryFrom<u8> for Pllp {
    type Error = ValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value + 1)
    }
}

/// Main PLL and PLLSAI1 division factor for PLLQCLK, PLLRCLK, PLLSAI1QCLK and PLLSAI1RCLK
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum PllQR {
    /// Divided by 2
    D2 = 0b001,
    /// Divided by 3
    D3 = 0b010,
    /// Divided by 4
    D4 = 0b011,
    /// Divided by 5
    D5 = 0b100,
    /// Divided by 6
    D6 = 0b101,
    /// Divided by 7
    D7 = 0b110,
    /// Divided by 8
    D8 = 0b111,
}

impl PllQR {
    pub const fn div_factor(self) -> u8 {
        match self {
            Self::D2 => 2,
            Self::D3 => 3,
            Self::D4 => 4,
            Self::D5 => 5,
            Self::D6 => 6,
            Self::D7 => 7,
            Self::D8 => 8,
        }
    }
}

//
// PLLSAI1
//

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pllsai1N(u8);

impl Pllsai1N {
    pub fn new(x: u8) -> Result<Self, ValueError> {
        if x < 4 || x > 86 {
            return value_error!("PLLSAI1 division factor must be in range of [4, 86]");
        }

        Ok(Self(x))
    }

    pub fn get(self) -> u8 {
        self.0
    }
}

impl TryFrom<u8> for Pllsai1N {
    type Error = ValueError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Pllsai1N> for u8 {
    fn from(value: Pllsai1N) -> u8 {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Smpssel {
    Hsi16 = 0b00,
    /// Msi range must be one of the following: 16/24/32/48 MHz
    Msi = 0b01,
    Hse = 0b10,
}

/// SMPS division prescaler
///
/// Note: There is always a fixed division of 2 after the prescaler has been applied
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Smpsdiv {
    /// SMPS incoming frequency will be 8 MHz. Forbidden, if MSI clock is selected and configured for 24 MHz.
    /// The following division factors will be applied:
    ///
    /// - HSI16 (16MHz) => 1 (8 MHz)
    /// - MSI (16 MHz) => 1 (8 MHz)
    /// - MSI (24 MHz) => RESERVED
    /// - MSI (32 MHz) => 2 (8 MHz)
    /// - MSI (48 MHz) => 3 (8 MHz)
    /// - HSE (32 MHz) => 2 (8 MHz)
    S8MHz = 0b00,
    /// SMPS incoming frequency will be 4 MHz. The following division factors will be applied:
    ///
    /// - HSI16 (16 MHz) => 2 (4 MHz)
    /// - MSI (16 MHz) => 2 (4 MHz)
    /// - MSI (24 MHz) => 3 (4 MHz)
    /// - MSI (32 MHz) => 4 (4 MHz)
    /// - MSI (48 MHz) => 6 (4 MHz)
    /// - HSE (32 MHz) => 4 (4 MHz)
    S4MHz = 0b01,
}

/// RF system wakeup clock source selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Rfwkpsel {
    NoClock = 0b00,
    /// LSE oscillator clock used as RF system wakeup clock
    Lse = 0b01,
    /// HSEoscillator clock divided by 1024 used as RF system wakeup clock
    Hse = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Usart1sel {
    Pclk = 0b00,
    Sysclk = 0b01,
    Hsi16 = 0b10,
    Lse = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum I2cSel {
    Pclk = 0b00,
    Sysclk = 0b01,
    Hsi16 = 0b10,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum LptimSel {
    Pclk = 0b00,
    Lsi = 0b01,
    Hsi16 = 0b10,
    Lse = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Sai1Sel {
    PllsaiP = 0b00,
    PllP = 0b01,
    Hsi16 = 0b10,
    Ext = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Clk48Sel {
    Hsi48 = 0b00,
    PllsaiQ = 0b01,
    PllQ = 0b10,
    Msi = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum AdcSel {
    NoClock = 0b00,
    PllsaiR = 0b01,
    PllP = 0b10,
    Sysclk = 0b11,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum RngSel {
    Clk48 = 0b00,
    Lsi = 0b01,
    Lse = 0b10,
}

/// MSI Maximum frequency
pub const fn msi_max_hertz(vos: Vos) -> Hertz {
    match vos {
        Vos::Range1 => Hertz::MHz(48),
        Vos::Range2 => Hertz::MHz(16),
    }
}

/// HSI16 frequency
pub const fn hsi16_hertz() -> Hertz {
    Hertz::MHz(16)
}

/// HSI48 frequency
pub const fn hsi48_hertz() -> Hertz {
    Hertz::MHz(48)
}

/// HSE frequency
///
/// Note: If Range 2 is selected, HSEPRE must be set to divide the frequency by 2
pub const fn hse_hertz() -> Hertz {
    Hertz::MHz(32)
}

pub const fn hse_output_hertz(hsepre: bool) -> Hertz {
    match hsepre {
        false => hse_hertz(),
        true => Hertz::from_raw(hse_hertz().raw() / 2),
    }
}

/// PLL and PLLSAI1 maximum frequency
///
/// - Range 1: VCO max = 344 MHz
/// - Range 2: VCO max = 128 MHz
pub const fn pll_max_hertz(vos: Vos) -> Hertz {
    match vos {
        Vos::Range1 => Hertz::MHz(64),
        Vos::Range2 => Hertz::MHz(16),
    }
}

/// 32 kHz low speed internal RC which may drive the independent watchdog
/// and optionally the RTC used for Auto-wakeup from Stop and Standby modes
pub const fn lsi1_hertz() -> Hertz {
    Hertz::MHz(32)
}

/// 32 kHz low speed low drift internal RC which may drive the independent watchdog
/// and optionally the RTC used for Auto-wakeup from Stop and Standby modes
pub const fn lsi2_hertz() -> Hertz {
    Hertz::MHz(32)
}

/// Low speed external crystal which optionally drives the RTC used for
/// Auto-wakeup or the RF system Auto-wakeup from Stop and Standby modes, or the
/// real-time clock (RTCCLK)
pub const fn lse_hertz() -> Hertz {
    Hertz::kHz(32_768)
}
