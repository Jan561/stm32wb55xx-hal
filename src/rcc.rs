use crate::flash::Latency;
use crate::pac::{FLASH, PWR, RCC};
use crate::pwr::Vos;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};
use sealed::sealed;
use stm32wb::stm32wb55::rcc::cifr;

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

#[sealed]
pub trait RccBus {
    type Bus;
}

pub trait BusClock {
    // TODO
}

pub trait Enable: RccBus {
    fn enable(rcc: &RCC);
    fn disable(rcc: &RCC);
}

pub trait LPEnable: RccBus {
    fn low_power_enable(rcc: &RCC);
    fn low_power_disable(rcc: &RCC);
}

pub trait Reset: RccBus {
    fn reset(rcc: &RCC);
}

pub trait Sysclk {
    fn current_hertz(self) -> u32;
}

pub struct Rcc {
    rcc: RCC,
}

impl Rcc {
    pub fn cr<F>(&mut self, op: F) -> Result<(), Error>
    where
        F: for<'w> FnOnce(&CrR, &'w mut CrW) -> &'w mut CrW,
    {
        let r = CrR::read_from(&self.rcc);
        let mut wc = CrW(r.0);

        op(&r, &mut wc);

        let cfgr = self.cfg_read();
        let pllcfgr = self.pllcfgr_read();

        let pwr = unsafe { &*PWR::PTR };
        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

        let pll_rising = !r.pllon() && wc._pllon();
        let pllsai1_rising = !r.pllsai1on() && wc._pllsai1on();
        let msi_range_changed = wc._msirange() != r.msirange().into();
        let hse_pre_changed = wc._hsepre() != r.hsepre();

        // MSI

        // Check if clock shall be disabled but is currently used by sysclk or pll
        if !wc._msion()
            && (cfgr.sws() == SysclkSwitch::Msi || (r.pllon() && pllcfgr.pllsrc() == PllSrc::Msi))
        {
            return Err(Error::ClockInUse);
        }

        // Check if PLL shall be enabled when selected clock source is disabled
        if pll_rising && cfgr.sws() == SysclkSwitch::Msi && !wc._msion() {
            return Err(Error::SelectedClockNotEnabled);
        }

        // HSI16

        // Check if clock shall be disabled but is currently used by sysclk or pll
        if !wc._hsion()
            && (cfgr.sws() == SysclkSwitch::Hsi16
                || (r.pllon() && pllcfgr.pllsrc() == PllSrc::Hsi16))
        {
            return Err(Error::ClockInUse);
        }

        // Check if PLL shall be enabled when selected clock source is disabled
        if pll_rising && cfgr.sws() == SysclkSwitch::Hsi16 && !wc._hsion() {
            return Err(Error::SelectedClockNotEnabled);
        }

        // HSE

        // Check if clock shall be disabled but is currently used by sysclk or pll
        if !wc._hseon()
            && (cfgr.sws() == SysclkSwitch::Hse || (r.pllon() && pllcfgr.pllsrc() == PllSrc::Hse))
        {
            return Err(Error::ClockInUse);
        }

        // Check if PLL shall be enabled when selected clock source is disabled
        if pll_rising && cfgr.sws() == SysclkSwitch::Hse && !wc._hseon() {
            return Err(Error::SelectedClockNotEnabled);
        }

        // PLL

        // Check if clock shall be disabled but is currently used by sysclk
        if !wc._pllon() && cfgr.sws() == SysclkSwitch::Pll {
            return Err(Error::ClockInUse);
        }

        // Prevent enabling the PLL when no clock is selected
        if (pll_rising || pllsai1_rising) && pllcfgr.pllsrc() == PllSrc::NoClock {
            return Err(Error::PllNoClockSelected);
        }

        let check_pll = pll_rising || (r.pllon() && (msi_range_changed || hse_pre_changed));
        let check_pllsai1 =
            pllsai1_rising || (r.pllsai1on() && (msi_range_changed || hse_pre_changed));

        let pll_m_in = match pllcfgr.pllsrc() {
            PllSrc::NoClock => unreachable!(),
            PllSrc::Msi => MsiRange::try_from(wc._msirange()).unwrap().hertz(),
            PllSrc::Hsi16 => hsi16_hertz(),
            PllSrc::Hse => hse_output_hertz(wc._hsepre()),
        };

        // Check PLL M input clock when in Range 2
        if (check_pll || check_pllsai1) && vos == Vos::Range2 {
            if pll_m_in <= 16_000_000 {
                return Err(Error::PllClkIllegalRange);
            }
        }

        let vco_in_times_3 = pll_m_in * 3 / pllcfgr.pllm().div_factor() as u32;

        // Check PLL VCO input frequency (after PLL M)
        if (check_pll || check_pllsai1)
            && (vco_in_times_3 > 48_000_000 || vco_in_times_3 < 8_000_000)
        {
            return Err(Error::PllClkIllegalRange);
        }

        let vco_out_times_3_main = match vco_in_times_3.checked_mul(pllcfgr.plln().get() as u32) {
            Some(x) => x,
            None => return Err(Error::PllClkIllegalRange),
        };
        let pllsai1cfgr = self.pllsai1cfgr_read();
        let vco_out_times_3_sai1 = match vco_in_times_3.checked_mul(pllsai1cfgr.plln().get() as u32)
        {
            Some(x) => x,
            None => return Err(Error::PllClkIllegalRange),
        };

        // Check PLL VCO output frequency (after PLL N) (for PLL MAIN and PLLSAI1)
        if check_pll
            && (vco_out_times_3_main > 344_000_000 * 3 || vco_out_times_3_main < 96_000_000 * 3)
        {
            return Err(Error::PllClkIllegalRange);
        }

        if check_pllsai1
            && (vco_out_times_3_sai1 > 344_000_000 * 3 || vco_out_times_3_sai1 < 64_000_000 * 3)
        {
            return Err(Error::PllClkIllegalRange);
        }

        // Check PLLP, PLLQ, PLLR enabled outputs for PLL MAIN
        let pllp_times_3_main = vco_out_times_3_main / pllcfgr.pllp().get() as u32;
        if check_pll && pllp_times_3_main > 64_000_000 * 3 {
            return Err(Error::PllClkIllegalRange);
        }

        let pllq_times_3_main = vco_out_times_3_main / pllcfgr.pllq().div_factor() as u32;
        if check_pll && pllq_times_3_main > 64_000_000 * 3 {
            return Err(Error::PllClkIllegalRange);
        }

        let pllr_times_3_main = vco_out_times_3_main / pllcfgr.pllr().div_factor() as u32;
        if check_pll && pllr_times_3_main > 64_000_000 * 3 {
            return Err(Error::PllClkIllegalRange);
        }

        // Check PLLP, PLLQ, PLLR enabled outputs for PLLSAI1
        let pllp_times_3_sai1 = vco_out_times_3_sai1 / pllsai1cfgr.pllp().get() as u32;
        if check_pllsai1 && pllp_times_3_sai1 > 64_000_000 * 3 {
            return Err(Error::PllClkIllegalRange);
        }

        let pllq_times_3_sai1 = vco_out_times_3_sai1 / pllsai1cfgr.pllq().div_factor() as u32;
        if check_pllsai1 && pllq_times_3_sai1 > 64_000_000 * 3 {
            return Err(Error::PllClkIllegalRange);
        }

        let pllr_times_3_sai1 = vco_out_times_3_sai1 / pllsai1cfgr.pllr().div_factor() as u32;
        if check_pllsai1 && pllr_times_3_sai1 > 64_000_000 * 3 {
            return Err(Error::PllClkIllegalRange);
        }

        // MSIRANGE shall not be changed when MSI is on but not ready
        if r.msion() && !r.msirdy() && wc._msirange() != r.msirange().into() {
            return Err(Error::MsiNotReady);
        }

        // New MSIRANGE must be within the VOS limits if used as sysclk
        if cfgr.sws() == SysclkSwitch::Msi
            && vos == Vos::Range2
            && MsiRange::R16M < wc._msirange().try_into().unwrap()
        {
            return Err(Error::SysclkTooHighVosRange2);
        }

        // HSEPRE flag must be set if HSE is used as sysclk in VOS Range 2
        if cfgr.sws() == SysclkSwitch::Hse && vos == Vos::Range2 && !wc._hsepre() {
            return Err(Error::SysclkTooHighVosRange2);
        }

        self.rcc.cr.modify(|_, w| {
            w.msion()
                .bit(wc._msion())
                .msipllen()
                .bit(wc._msipllen())
                .msirange()
                .variant(wc._msirange())
                .hsion()
                .bit(wc._hsion())
                .hsikeron()
                .bit(wc._hsikeron())
                .hsiasfs()
                .bit(wc._hsiasfs())
                .hseon()
                .bit(wc._hseon())
                .csson()
                .bit(wc._csson())
                .hsepre()
                .bit(wc._hsepre())
                .pllon()
                .bit(wc._pllon())
                .pllsai1on()
                .bit(wc._pllsai1on())
        });

        while wc._msion() && self.rcc.cr.read().msirdy().bit_is_clear() {}
        while wc._hsion() && self.rcc.cr.read().hsirdy().bit_is_clear() {}
        while wc._hseon() && self.rcc.cr.read().hserdy().bit_is_clear() {}
        while wc._hsikeron() && self.rcc.cr.read().hsikerdy().bit_is_clear() {}
        while wc._pllon() && self.rcc.cr.read().pllrdy().bit_is_clear() {}
        while wc._pllsai1on() && self.rcc.cr.read().pllsai1rdy().bit_is_clear() {}

        Ok(())
    }

    pub fn cr_read(&self) -> CrR {
        CrR::read_from(&self.rcc)
    }

    pub fn cfg<F>(&mut self, op: F) -> Result<(), Error>
    where
        F: for<'w> FnOnce(&CfgrR, &'w mut CfgrW) -> &'w mut CfgrW,
    {
        let cfgr_r = CfgrR::read_from(&self.rcc);
        let mut cfgr_w = CfgrW(cfgr_r.0);

        op(&cfgr_r, &mut cfgr_w);

        if cfgr_r.0 == cfgr_w.0 {
            return Ok(());
        }

        let cr_r = CrR::read_from(&self.rcc);
        let pllcfgr = PllCfgrR::read_from(&self.rcc);

        let enabled = match cfgr_w._sw().try_into().unwrap() {
            SysclkSwitch::Msi => cr_r.msion() && cr_r.msirdy(),
            SysclkSwitch::Hsi16 => cr_r.hsion() && cr_r.hsirdy(),
            SysclkSwitch::Hse => cr_r.hseon() && cr_r.hserdy(),
            SysclkSwitch::Pll => cr_r.pllon() && cr_r.pllrdy(),
        };

        if !enabled {
            return Err(Error::SelectedClockNotEnabled);
        }

        let current_sysclk = Self::sysclk_hertz(cfgr_r.sws(), &cr_r, &pllcfgr);
        let new_sysclk = Self::sysclk_hertz(CfgrR(cfgr_w.0).sw(), &cr_r, &pllcfgr);

        // Check vos
        let pwr = unsafe { &*PWR::PTR };
        if new_sysclk > 16_000_000 && pwr.cr1.read().vos().bits() == Vos::Range2.into() {
            return Err(Error::SysclkTooHighVosRange2);
        }

        if current_sysclk < new_sysclk {
            // Increase CPU frequency
            self.set_flash_latency(new_sysclk);
        }

        self.rcc.cfgr.modify(|_, w| {
            w.sw()
                .variant(cfgr_w._sw())
                .hpre()
                .variant(cfgr_w._hpre())
                .ppre1()
                .variant(cfgr_w._ppre1())
                .ppre2()
                .variant(cfgr_w._ppre2())
                .stopwuck()
                .bit(cfgr_w._stopwuck())
                .mcosel()
                .variant(cfgr_w._mcosel())
                .mcopre()
                .variant(cfgr_w._mcopre())
        });

        while self.rcc.cfgr.read().sws().bits() != cfgr_w._sw()
            || self.rcc.cfgr.read().hpref().bit_is_set()
            || self.rcc.cfgr.read().ppre1f().bit_is_set()
            || self.rcc.cfgr.read().ppre2f().bit_is_set()
        {}

        if current_sysclk > new_sysclk {
            // Decrease CPU frequency
            self.set_flash_latency(new_sysclk);
        }

        Ok(())
    }

    pub fn cfg_read(&self) -> CfgrR {
        CfgrR::read_from(&self.rcc)
    }

    pub fn pllcfgr<F>(&self, op: F) -> Result<(), Error>
    where
        F: for<'w> FnOnce(&PllCfgrR, &'w mut PllCfgrW) -> &'w mut PllCfgrW,
    {
        let r = PllCfgrR::read_from(&self.rcc);
        let mut wc = PllCfgrW(r.0);

        op(&r, &mut wc);

        if (wc._pllsrc() != r.pllsrc().into() || wc._pllm() != r.pllm().into())
            && (self.rcc.cr.read().pllon().bit_is_set()
                || self.rcc.cr.read().pllsai1on().bit_is_set())
        {
            return Err(Error::PllEnabled);
        }

        if (wc._plln() != r.plln().into()
            || wc._pllp() != r.pllp().into()
            || wc._pllq() != r.pllq().into()
            || wc._pllr() != r.pllr().into())
            && self.rcc.cr.read().pllon().bit_is_set()
        {
            return Err(Error::PllEnabled);
        }

        self.rcc.pllcfgr.modify(|_, w| {
            w.pllsrc()
                .variant(wc._pllsrc())
                .pllm()
                .variant(wc._pllm())
                .plln()
                .variant(wc._plln())
                .pllpen()
                .bit(wc._pllpen())
                .pllp()
                .variant(wc._pllp())
                .pllqen()
                .bit(wc._pllqen())
                .pllq()
                .variant(wc._pllq())
                .pllren()
                .bit(wc._pllren())
                .pllr()
                .variant(wc._pllr())
        });

        Ok(())
    }

    pub fn pllcfgr_read(&self) -> PllCfgrR {
        PllCfgrR::read_from(&self.rcc)
    }

    pub fn pllsai1cfgr_read(&self) -> Pllsai1CfgrR {
        Pllsai1CfgrR::read_from(&self.rcc)
    }

    pub fn ext_cfg<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&ExtCfgrR, &'w mut ExtCfgrW) -> &'w mut ExtCfgrW,
    {
        let r = ExtCfgrR::read_from(&self.rcc);
        let mut wc = ExtCfgrW::read_from(&self.rcc);

        op(&r, &mut wc);

        self.rcc.extcfgr.modify(|_, w| {
            w.shdhpre()
                .variant(wc._shdhpre())
                .c2hpre()
                .variant(wc._c2hpre())
        });

        while self.rcc.extcfgr.read().shdhpref().bit_is_set()
            || self.rcc.extcfgr.read().c2hpref().bit_is_set()
        {}
    }

    pub fn ext_cfg_read(&self) -> ExtCfgrR {
        ExtCfgrR::read_from(&self.rcc)
    }

    pub fn cier<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&CierR, &'w mut CierW) -> &'w mut CierW,
    {
        let r = CierR::read_from(&self.rcc);
        let mut wc = CierW(r.0);

        op(&r, &mut wc);

        self.rcc.cier.modify(|_, w| {
            w.lsi1rdyie()
                .bit(wc._lsi1rdyie())
                .lserdyie()
                .bit(wc._lserdyie())
                .msirdyie()
                .bit(wc._msirdyie())
                .hsirdyie()
                .bit(wc._hsirdyie())
                .hserdyie()
                .bit(wc._hserdyie())
                .pllrdyie()
                .bit(wc._pllrdyie())
                .pllsai1rdyie()
                .bit(wc._pllsai1rdyie())
                .lsecssie()
                .bit(wc._lsecssie())
                .hsi48rdyie()
                .bit(wc._hsi48rdyie())
                .lsi2rdyie()
                .bit(wc._lsi2rdyie())
        });
    }

    pub fn cier_read(&self) -> CierR {
        CierR::read_from(&self.rcc)
    }

    pub fn cifr_read(&self) -> cifr::R {
        self.rcc.cifr.read()
    }

    pub fn cicr<F>(&self, op: F)
    where
        F: FnOnce(&mut Cicr) -> &mut Cicr,
    {
        let mut c = Cicr::new();

        op(&mut c);

        self.rcc.cicr.write(|w| unsafe { w.bits(c.0) });
    }

    pub fn csr<F>(&mut self, op: F) -> Result<(), Error>
    where
        F: for<'w> FnOnce(&CsrR, &'w mut CsrW) -> &'w mut CsrW,
    {
        let r = CsrR::read_from(&self.rcc);
        let mut wc = CsrW(r.0);

        op(&r, &mut wc);

        if wc._lsi2trim() != r.lsi2trim() && r.lsi2on() {
            return Err(Error::ClockInUse);
        }

        self.rcc.csr.modify(|_, w| unsafe { w.bits(wc.0) });

        Ok(())
    }

    pub fn csr_read(&self) -> CsrR {
        CsrR::read_from(&self.rcc)
    }

    pub fn smps_cr<F>(&self, op: F) -> Result<(), Error>
    where
        F: for<'w> FnOnce(&SmpsCrR, &'w mut SmpsCrW) -> &'w mut SmpsCrW,
    {
        let r = SmpsCrR::read_from(&self.rcc);
        let mut wc = SmpsCrW(r.0);

        op(&r, &mut wc);

        if wc._smpssel() == Smpssel::Msi.into() {
            let msi_range = self.cr_read().msirange();

            match msi_range {
                MsiRange::R24M => {
                    if wc._smpsdiv() == Smpsdiv::S4MHz.into() {
                        return Err(Error::SmpsMsi24MhzTo4MhzIllegal);
                    }
                }
                MsiRange::R16M | MsiRange::R32M | MsiRange::R48M => (),
                _ => return Err(Error::SmpsMsiUnsupportedRange),
            }
        }

        self.rcc.smpscr.modify(|_, w| {
            w.smpssel()
                .variant(wc._smpssel())
                .smpsdiv()
                .variant(wc._smpsdiv())
        });

        while self.rcc.smpscr.read().smpssws().bits() != wc._smpssel() {}

        Ok(())
    }

    pub fn smps_cr_read(&self) -> SmpsCrR {
        SmpsCrR::read_from(&self.rcc)
    }

    fn set_flash_latency(&self, clk: u32) {
        // SAFETY: No safety critical accesses performed
        let pwr = unsafe { &*PWR::PTR };
        // SAFETY: No safety critical accesses performed
        let flash = unsafe { &*FLASH::PTR };

        let vos: Vos = pwr.cr1.read().vos().bits().try_into().unwrap();

        let hclk4 = Self::hclk4(clk, &ExtCfgrR::read_from(&self.rcc));
        let latency = Latency::from(vos, hclk4);

        flash.acr.modify(|_, w| w.latency().variant(latency.into()));
    }

    pub fn current_sysclk_hertz(&self) -> u32 {
        Self::sysclk_hertz(self.cfg_read().sws(), &self.cr_read(), &self.pllcfgr_read())
    }

    fn sysclk_hertz(sw: SysclkSwitch, cr_r: &CrR, pllcfgr: &PllCfgrR) -> u32 {
        match sw {
            SysclkSwitch::Msi => Self::msi_hertz(cr_r),
            SysclkSwitch::Hsi16 => hsi16_hertz(),
            SysclkSwitch::Hse => hse_output_hertz(cr_r.hsepre()),
            SysclkSwitch::Pll => {
                let src = match pllcfgr.pllsrc() {
                    PllSrc::NoClock => 0,
                    PllSrc::Msi => Self::msi_hertz(cr_r),
                    PllSrc::Hsi16 => hsi16_hertz(),
                    PllSrc::Hse => hse_hertz(),
                } as u64;

                let pllm = pllcfgr.pllm().div_factor() as u64;
                let plln = pllcfgr.plln().get() as u64;
                let pllr = pllcfgr.pllr().div_factor() as u64;

                let voc = src * plln / pllm;
                (voc / pllr) as u32
            }
        }
    }

    pub fn current_hclk2(&self) -> u32 {
        let sysclk = self.current_sysclk_hertz();

        Self::hclk2(sysclk, &ExtCfgrR::read_from(&self.rcc))
    }

    fn hclk2(sysclk: u32, ext: &ExtCfgrR) -> u32 {
        let prescaler = u8::from(ext.c2hpre()) as u32;

        sysclk / prescaler
    }

    pub fn current_hclk4(&self) -> u32 {
        let sysclk = self.current_sysclk_hertz();

        Self::hclk4(sysclk, &ExtCfgrR::read_from(&self.rcc))
    }

    fn hclk4(sysclk: u32, ext: &ExtCfgrR) -> u32 {
        let prescaler = u8::from(ext.shdhpre()) as u32;

        sysclk / prescaler
    }

    fn msi_hertz(cr_r: &CrR) -> u32 {
        cr_r.msirange().hertz()
    }
}

impl Sysclk for &'_ Rcc {
    fn current_hertz(self) -> u32 {
        self.current_sysclk_hertz()
    }
}

config_reg_u32! {
    R, CfgrR, RCC, cfgr, [
        sw => (SysclkSwitch, u8, [1:0], "System clock switch"),
        sws => (SysclkSwitch, u8, [3:2], "System clock switch status"),
        hpre => (PreScaler, u8, [7:4], "HCLK1 prescaler (CPU1, AHB1, AHB2, AHB3, SRAM1)"),
        ppre1 => (PpreScaler, u8, [10:8], "PCLK1 low-speed prescaler (APB1)"),
        ppre2 => (PpreScaler, u8, [13:11], "PCLK2 high-speed prescaler (APB2)"),
        stopwuck => (bool, bool, [15:15], "Wakeup from Stop and CSS backup clock selection\n\n\
            - `false`: MSI\n\
            - `true`: HSI16
        "),
        hpref => (bool, bool, [16:16], "HCLK1 prescaler flag applied (CPU1, AHB1, AHB2, AHB3, SRAM1)"),
        ppre1f => (bool, bool, [17:17], "PCLK1 prescaler flag applied (APB1)"),
        ppre2f => (bool, bool, [18:18], "PCLK2 prescaler flag applied (APB2)"),
        mcosel => (McoSelector, u8, [27:24], "Microcontroller clock output"),
        mcopre => (McoPrescaler, u8, [30:28], "Microcontroller clock output prescaler"),
    ]
}

config_reg_u32! {
    W, CfgrW, RCC, cfgr, [
        sw => (_sw, SysclkSwitch, u8, [1:0], "System clock switch"),
        hpre => (_hpre, PreScaler, u8, [7:4], "HCLK1 prescaler (CPU1, AHB1, AHB2, AHB3, SRAM1)"),
        ppre1 => (_ppre1, PpreScaler, u8, [10:8], "PCLK1 low-speed prescaler (APB1)"),
        ppre2 => (_ppre2, PpreScaler, u8, [13:11], "PCLK2 high-speed prescaler (APB2)"),
        stopwuck => (_stopwuck, bool, bool, [15:15], "Wakeup from Stop and CSS backup clock selection\n\n\
            - `false`: MSI\n\
            - `true`: HSI16
        "),
        mcosel => (_mcosel, McoSelector, u8, [27:24], "Microcontroller clock output"),
        mcopre => (_mcopre, McoPrescaler, u8, [30:28], "Microcontroller clock output prescaler"),
    ]
}

config_reg_u32! {
    R, CrR, RCC, cr, [
        msion => (bool, bool, [0:0], "MSI clock enable"),
        msirdy => (bool, bool, [1:1], "MSI clock ready flag"),
        msipllen => (bool, bool, [2:2], "MSI clock PLL enable"),
        msirange => (MsiRange, u8, [7:4], "MSI clock range"),
        hsion => (bool, bool, [8:8], "HSI16 clock enable"),
        hsikeron => (bool, bool, [9:9], "HSI16 always enable for peripheral kernel clocks"),
        hsirdy => (bool, bool, [10:10], "HSI16 clock ready flag"),
        hsiasfs => (bool, bool, [11:11], "HSI16 automatic start from Stop"),
        hsikerdy => (bool, bool, [12:12], "HSI16 kernel clock ready flag for peripheral requests"),
        hseon => (bool, bool, [16:16], "HSE clock enable"),
        hserdy => (bool, bool, [17:17], "HSE clock ready flag"),
        csson => (bool, bool, [19:19], "HSE clock security system enable"),
        hsepre => (bool, bool, [20:20], "HSE system clock and PLL M divider prescale\n\n\
            - `false`: SYSCLK and PLL M divider input clocks are not divided (HSE)\n\
            - `true`: SYSCLK and PLL M divider input clocks are divided by 2 (HSE/2)
        "),
        pllon => (bool, bool, [24:24], "System PLL enable"),
        pllrdy => (bool, bool, [25:25], "System PLL clock ready flag"),
        pllsai1on => (bool, bool, [26:26], "SAI PLL enable"),
        pllsai1rdy => (bool, bool, [27:27], "SAI PLL clock ready flag\n\n\
            - `false`: PLLSAI1 unlocked\n\
            - `true`: PLLSAI1 locked
        "),
    ]
}

config_reg_u32! {
    W, CrW, RCC, cr, [
        msion => (_msion, bool, bool, [0:0], "MSI clock enable"),
        msipllen => (_msipllen, bool, bool, [2:2], "MSI clock PLL enable"),
        msirange => (_msirange, MsiRange, u8, [7:4], "MSI clock range"),
        hsion => (_hsion, bool, bool, [8:8], "HSI16 clock enable"),
        hsikeron => (_hsikeron, bool, bool, [9:9], "HSI16 always enable for peripheral kernel clocks"),
        hsiasfs => (_hsiasfs, bool, bool, [11:11], "HSI16 automatic start from Stop"),
        hseon => (_hseon, bool, bool, [16:16], "HSE clock enable"),
        csson => (_csson, bool, bool, [19:19], "HSE clock security system enable"),
        hsepre => (_hsepre, bool, bool, [20:20], "HSE system clock and PLL M divider prescale\n\n\
            - `false`: SYSCLK and PLL M divider input clocks are not divided (HSE)\n\
            - `true`: SYSCLK and PLL M divider input clocks are divided by 2 (HSE/2)
        "),
        pllon => (_pllon, bool, bool, [24:24], "System PLL enable"),
        pllsai1on => (_pllsai1on, bool, bool, [26:26], "SAI PLL enable"),
    ]
}

config_reg_u32! {
    RW, PllCfgrR, PllCfgrW, RCC, pllcfgr, [
        pllsrc => (_pllsrc, PllSrc, u8, [1:0], "Main PLL and audio PLLSAI1 clock source"),
        pllm => (_pllm, Pllm, u8, [6:4], "Division factor for the main PLL and audio PLLSAI1 input clock"),
        plln => (_plln, Plln, u8, [14:8], "Main PLL multiplication factor for VCO"),
        pllpen => (_pllpen, bool, bool, [16:16], "Main PLL PLLPCLK output clock enable"),
        pllp => (_pllp, Pllp, u8, [21:17], "Main PLL division factor for PLLPCLK"),
        pllqen => (_pllqen, bool, bool, [24:24], "Main PLL PLLQCLK output clock enable"),
        pllq => (_pllq, PllQR, u8, [27:25], "Main PLL division factor for PLLQCLK"),
        pllren => (_pllren, bool, bool, [28:28], "Main PLL PLLRCLK output clock enable"),
        pllr => (_pllr, PllQR, u8, [31:29], "Main PLL division factor for PLLRCLK"),
    ]
}

config_reg_u32! {
    R, ExtCfgrR, RCC, extcfgr, [
        shdhpre => (PreScaler, u8, [3:0], "HCLK4 shared prescaler (AHB4, Flash memory and SRAM2)\n\n\
            Set and cleared by software to control the division factor of the Shared HCLK4 clock (AHB4,
            Flash memory and SRAM2).
            The SHDHPREF flag can be checked to know if the programmed SHDHPRE prescaler value
            is applied
        "),
        c2hpre => (PreScaler, u8, [7:4], "HCLK2 prescaler (CPU2)\n\n\
            Set and cleared by software to control the division factor of the HCLK2 clock (CPU2).
            The C2HPREF flag can be checked to know if the programmed C2HPRE prescaler value is
            applied
        "),
        shdhpref => (bool, bool, [16:16], "HCLK4 shared prescaler flag (AHB4, Flash memory and SRAM2)"),
        c2hpref => (bool, bool, [17:17], "HCLK2 prescaler flag (CPU2)"),
        rfcss => (bool, bool, [20:20], "Radio system HCLK5 and APB3 selected clock source indication\n\n\
            Set and reset by hardware to indicate which clock source is selected for the Radio system
            HCLK5 and APB3 clock\n\n\
            - `false`: HSI16 used for Radio system HCLK5 and APB3 clock\n\
            - `true`: HSE divided by 2 used for Radio system HCLK5 and APB3 clock
        "),
    ]
}

config_reg_u32! {
    W, ExtCfgrW, RCC, extcfgr, [
        shdhpre => (_shdhpre, PreScaler, u8, [3:0], "HCLK4 shared prescaler (AHB4, Flash memory and SRAM2)\n\n\
            Set and cleared by software to control the division factor of the Shared HCLK4 clock (AHB4, \
            Flash memory and SRAM2). \n\
            The SHDHPREF flag can be checked to know if the programmed SHDHPRE prescaler value \
            is applied
        "),
        c2hpre => (_c2hpre, PreScaler, u8, [7:4], "HCLK2 prescaler (CPU2)\n\n\
            Set and cleared by software to control the division factor of the HCLK2 clock (CPU2).\n\
            The C2HPREF flag can be checked to know if the programmed C2HPRE prescaler value is \
            applied
        "),
    ]
}

config_reg_u32! {
    RW, Pllsai1CfgrR, Pllsai1CfgrW, RCC, pllsai1cfgr, [
        plln => (_plln, Pllsai1N, u8, [14:8], "Audio PLLSAI1 multiplication factor for VCO\n\n\
            These bits can be written only when the PLLSAI1 is disabled\n\n\
            Note: The VCO output frequency must be between 64 and 344 MHz
        "),
        pllpen => (_pllpen, bool, bool, [16:16], "Audio PLLSAI1 PLLSAI1PCLK output enable"),
        pllp => (_pllp, Pllp, u8, [21:17], "Audio PLLSAI1 division factor for PLLSAI1PCLK\n\n\
            This output can be selected for SAI1 and ADC. These bits can be written only if PLLSAI1 is disabled
        "),
        pllqen => (_pllqen, bool, bool, [24:24], "Audio PLLSAI1 PLLSAI1QCLK output enable"),
        pllq => (_pllq, PllQR, u8, [27:25], "Audio PLLSAI1 division factor for PLLSAI1QCLK\n\n\
            This output can be selected for USB and True RNG clock. These bits can be written only if PLLSAI1 is disabled
        "),
        pllren => (_pllren, bool, bool, [28:28], "Audio PLLSAI1 PLLSAI1RCLK output enable"),
        pllr => (_pllr, PllQR, u8, [31:29], "Audio PLLSAI1 division factor for PLLSAI1RCLK\n\n\
            This output can be selected as system clock. These bits can be written only if PLLSAI1 is disabled
        "),
    ]
}

config_reg_u32! {
    R, SmpsCrR, RCC, smpscr, [
        smpssel => (Smpssel, u8, [1:0], "SMPS step-down converter clock selection"),
        smpsdiv => (Smpsdiv, u8, [5:4], "SMPS step-down converter clock prescaler"),
        smpssws => (Smpssel, u8, [9:8], "SMPS step-down converter clock switch status"),
    ]
}

config_reg_u32! {
    W, SmpsCrW, RCC, smpscr, [
        smpssel => (_smpssel, Smpssel, u8, [1:0], "SMPS step-down converter clock selection"),
        smpsdiv => (_smpsdiv, Smpsdiv, u8, [5:4], "SMPS step-down converter clock prescaler"),
    ]
}

config_reg_u32! {
    RW, CierR, CierW, RCC, cier, [
        lsi1rdyie => (_lsi1rdyie, bool, bool, [0:0], "LSI1 ready interrupt enable"),
        lserdyie => (_lserdyie, bool, bool, [1:1], "LSE ready interrupt enable"),
        msirdyie => (_msirdyie, bool, bool, [2:2], "MSI ready interrupt enable"),
        hsirdyie => (_hsirdyie, bool, bool, [3:3], "HSI16 ready interrupt enable"),
        hserdyie => (_hserdyie, bool, bool, [4:4], "HSE ready interrupt enable"),
        pllrdyie => (_pllrdyie, bool, bool, [5:5], "PLL ready interrupt enable"),
        pllsai1rdyie => (_pllsai1rdyie, bool, bool, [6:6], "PLLSAI1 ready interrupt enable"),
        lsecssie => (_lsecssie, bool, bool, [9:9], "LSE clock security system interrupt enable"),
        hsi48rdyie => (_hsi48rdyie, bool, bool, [10:10], "HSI48 ready interrupt enable"),
        lsi2rdyie => (_lsi2rdyie, bool, bool, [11:11], "LSI2 ready interrupt enable"),
    ]
}

clear_status_reg_u32! {
    Cicr, [
        lsi1rdyc => (0, "LSI1 ready interrupt clear"),
        lserdyc => (1, "LSE ready interrupt clear"),
        msirdyc => (2, "MSI ready interrupt clear"),
        hsirdyc => (3, "HSI16 ready interrupt clear"),
        hserdyc => (4, "HSE ready interrupt clear"),
        pllrdyc => (5, "PLL ready interrupt clear"),
        pllsai1rdyc => (6, "PLLSAI1 ready interrupt clear"),
        lsecssc => (9, "LSE clock security system interrupt clear"),
        hsi48rdyc => (10, "HSI48 ready interrupt clear"),
        lsi2rdyc => (11, "LSI2 ready interrupt clear"),
    ]
}

config_reg_u32! {
    R, CsrR, RCC, csr, [
        lsi1on => (bool, bool, [0:0], "LSI1 oscillator enable"),
        lsi1rdy => (bool, bool, [1:1], "LSI1 oscillator ready"),
        lsi2on => (bool, bool, [2:2], "LSI2 oscillator enable and selection\n\n\
            - `false`: LSI2 oscillator off (LSI1 selected on LSI)\n\
            - `true`: LSI2 oscillator on (LSI2 when ready selected on LSI)
        "),
        lsi2rdy => (bool, bool, [3:3], "LSI2 oscillator ready"),
        lsi2trim => (u8, u8, [11:8], "LSI2 oscillator trim\n\n\
            Note: LSI2TRIM must be changed only when LSI2 is disabled
        "),
        rfwkpsel => (Rfwkpsel, u8, [15:14], "RF system wakeup clock source selection"),
        rfrsts => (bool, bool, [16:16], "Radio system BLE and 802.15.4 reset status\n\n\
            - `false`: Radio system BLE and 802.15.4 not in reset, radio system can be accessed\n\
            - `true`: Radio system BLE and 802.15.4 under reset, radio system cannot be accessed
        "),
        oblrstf => (bool, bool, [25:25], "Option byte loader reset flag\n\n\
            Cleared by writing to the RMVF bit
        "),
        pinrstf => (bool, bool, [26:26], "Pin reset flag\n\n\
            Cleared by writing to the RMVF bit
        "),
        borrstf => (bool, bool, [27:27], "BOR flag\n\n\
            Cleared by writing to the RMVF bit
        "),
        sftrstf => (bool, bool, [28:28], "Software reset flag\n\n\
            Cleared by writing to the RMVF bit
        "),
        iwdgrstf => (bool, bool, [29:29], "Independent window watchdog reset flag\n\n\
            Cleared by writing to the RMVF bit
        "),
        wwdgrstf => (bool, bool, [30:30], "Window watchdog reset flag\n\n\
            Cleared by writing to the RMVF bit
        "),
        lpwrrstf => (bool, bool, [31:31], "Low power reset flag\n\n\
            Cleared by writing to the RMVF bit\n\n\
            - `false`: No illegal mode reset occured\n\
            - `true` Illegal mode reset occured
        "),
    ]
}

config_reg_u32! {
    W, CsrW, RCC, csr, [
        lsi1on => (_lsi1on, bool, bool, [0:0], "LSI1 oscillator enable"),
        lsi2on => (_lsi2on, bool, bool, [2:2], "LSI2 oscillator enable and selection\n\n\
            - `false`: LSI2 oscillator off (LSI1 selected on LSI)\n\
            - `true`: LSI2 oscillator on (LSI2 when ready selected on LSI)
        "),
        lsi2trim => (_lsi2trim, u8, u8, [11:8], "LSI2 oscillator trim\n\n\
            Note: LSI2TRIM must be changed only when LSI2 is disabled
        "),
        rfwkpsel => (_rfwkpsel, Rfwkpsel, u8, [15:14], "RF system wakeup clock source selection"),
        rmvw => (_rmvw, bool, bool, [23:23], "Remove reset flag"),
    ]
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
    pub const fn hertz(self) -> u32 {
        match self {
            Self::R100K => 100_000,
            Self::R200K => 200_000,
            Self::R400K => 400_000,
            Self::R800K => 800_000,
            Self::R1M => 1_000_000,
            Self::R2M => 2_000_000,
            Self::R4M => 4_000_000,
            Self::R8M => 8_000_000,
            Self::R16M => 16_000_000,
            Self::R24M => 24_000_000,
            Self::R32M => 32_000_000,
            Self::R48M => 48_000_000,
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

/// MSI Maximum frequency
pub const fn msi_max_hertz(vos: Vos) -> u32 {
    match vos {
        Vos::Range1 => 48_000_000,
        Vos::Range2 => 16_000_000,
    }
}

/// HSI16 frequency
pub const fn hsi16_hertz() -> u32 {
    16_000_000
}

/// HSI48 frequency
pub const fn hsi48_hertz() -> u32 {
    48_000_000
}

/// HSE frequency
///
/// Note: If Range 2 is selected, HSEPRE must be set to divide the frequency by 2
pub const fn hse_hertz() -> u32 {
    32_000_000
}

pub const fn hse_output_hertz(hsepre: bool) -> u32 {
    match hsepre {
        false => hse_hertz(),
        true => hse_hertz() / 2,
    }
}

/// PLL and PLLSAI1 maximum frequency
///
/// - Range 1: VCO max = 344 MHz
/// - Range 2: VCO max = 128 MHz
pub const fn pll_max_hertz(vos: Vos) -> u32 {
    match vos {
        Vos::Range1 => 64_000_000,
        Vos::Range2 => 16_000_000,
    }
}

/// 32 kHz low speed internal RC which may drive the independent watchdog
/// and optionally the RTC used for Auto-wakeup from Stop and Standby modes
pub const fn lsi1_hertz() -> u32 {
    32_000_000
}

/// 32 kHz low speed low drift internal RC which may drive the independent watchdog
/// and optionally the RTC used for Auto-wakeup from Stop and Standby modes
pub const fn lsi2_hertz() -> u32 {
    32_000_000
}

/// Low speed external crystal which optionally drives the RTC used for
/// Auto-wakeup or the RF system Auto-wakeup from Stop and Standby modes, or the
/// real-time clock (RTCCLK)
pub const fn lse_hertz() -> u32 {
    32_768_000
}
