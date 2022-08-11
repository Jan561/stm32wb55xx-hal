//! PWR Power Control
//!
//! By default, the system is in Run mode after a system or power reset and at least
//! one CPU is in CRun mode executing code. Low power modes are available to save power
//! when the CPU does not need to be kept running, for example when it is waiting for an
//! external event.
//!
//! The individual CPUs feature two low-power modes, entered by the CPU when executing
//! WFI, WFE or on return from an exception handler when SLEEPONEXIR is enabled
//!
//! - CSleep mode: when the CPU enters low-power mode and SLEEPDEEP is disabled, ARM "sleep mode"
//! - CStop mode: when the CPU enters low-power mode and SLEEPDEEP is enabled, ARM "sleepdeep mode"
//!
//! The following Low-Power modes are available:
//!
//! - Sleep Mode: CPU clock off, all peripherals including CPU core peripherals (NVIC, SysTick, ...)
//! can run and wake up the CPU when an interrupt or an event occurs
//! - Low-Power Run mode (LPRun): This mode is achieved when the system clock frequency is reduced below 2 MHz.
//! The code is executed from the SRAM or the FLASH memory. The regulator is in low-power mode to minimize the operating current
//! - Low-Power Sleep mode (LPSleep): This mode is entered from the LPRun mode: CPU is off
//! - Stop0, Stop1, Stop2 mode: the content of SRAM1, SRAM2 and of all
//! registers is retained. All clocks in the V CORE domain are stopped, the PLL, the MSI, the
//! HSI16 and the HSE are disabled. The LSI and the LSE can be kept running.
//! The RTC can remain active (Stop mode with RTC, Stop mode without RTC).
//! Some peripherals with the wakeup capability can enable the HSI16 RC during Stop
//! mode to detect their wakeup condition.
//! In Stop2 mode, most of the VCORE domain is put in a lower leakage mode. Stop1 offers
//! the largest number of active peripherals and wakeup sources, a smaller wakeup time
//! but a higher consumption compared with Stop2. In Stop0 mode, the main regulator
//! remains ON, resulting in the fastest wakeup time but with much higher consumption.
//! The active peripherals and wakeup sources are the same as in Stop1 mode.
//! The system clock, when exiting from Stop0, Stop1 or Stop2 mode, can be either MSI up
//! to 48 MHz or HSI16, depending on the software configuration
//! - Standby Mode: V_{CORE} domain is powered off. However, it is possible to preserve the SRAM2a contents,
//! by setting the RRS bit in the PWR_CR3 register. All clocks in the V_{CORE} domain are stopped, the PLL,
//! the MSI, the HSI16 and the HSE are disabled. The LSI and the LSE can be kept running.
//! The RTC can remain active (Standby mode with RTC, Standby mode without RTC).
//! The system clock, when exiting Standby modes, is HSI16
//! - Shutdown Mode: V_{CORE} domain is powered off. All clocks in the V_{CORE} domain are
//! stopped, the PLL, the MSI, the HSI16, the LSI and the HSE are disabled. The LSE can
//! be kept running. The system clock, when exiting Shutdown mode, is MSI at 4 MHz. In
//! this mode, the supply voltage monitoring is disabled and the product behavior is not
//! guaranteed in case of a power voltage drop
//!
//! Note: Stop, Standby and Shutdown Modes are only entered, when both CPUs are in CStop mode

pub mod pxcr;

use crate::pac::pwr::{sr1, sr2};
use crate::pac::{PWR, RCC};
use crate::rcc::{self, Clocks};
use cortex_m::peripheral::SCB;
use fugit::RateExtU32;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

use self::pxcr::Pxcr;

#[derive(Debug)]
pub enum Error {
    SysclkTooHighVos,
    SysclkTooHighLpr,
    LPRunToStop2Illegal,
}

pub trait PwrExt {
    fn constrain(self) -> Pwr;
}

impl PwrExt for PWR {
    fn constrain(self) -> Pwr {
        Pwr { pwr: self }
    }
}

pub struct Pwr {
    pwr: PWR,
}

impl Pwr {
    pub fn pxcr(&self) -> Pxcr {
        Pxcr(self)
    }

    // See RM0434 Rev 10 p. 146
    pub fn set_power_range<'a>(
        &mut self,
        range: Vos,
        clocks: impl Clocks<'a>,
    ) -> Result<(), Error> {
        if range == Vos::Range2 && clocks.sysclk() > 2.MHz::<1, 1>() {
            return Err(Error::SysclkTooHighVos);
        }

        let old_vos: Vos = self.pwr.cr1.read().vos().bits().try_into().unwrap();

        if old_vos == Vos::Range1 && range == Vos::Range2 {
            let rcc = unsafe { &*RCC::PTR };
            rcc::set_flash_latency(rcc, clocks.sysclk().to_Hz());
        }

        self.pwr.cr1.modify(|_, w| w.vos().variant(range.into()));

        if old_vos == Vos::Range2 && range == Vos::Range1 {
            while self.pwr.sr2.read().vosf().bit_is_set() {}
        }

        Ok(())
    }

    pub fn shutdown(&self, scb: &mut SCB) -> ! {
        let _ = self.enter_low_power_mode(Lpms::Shutdown, scb);

        // Technically unreachable
        loop {}
    }

    /// Enter low power mode with enabled flash
    ///
    /// After calling the function, the clock speed must not be increased
    /// above 2 MHz.
    pub fn enter_low_power_run<'a>(&mut self, clocks: impl Clocks<'a>) -> Result<(), Error> {
        if clocks.sysclk() > 2.MHz::<1, 1>() {
            return Err(Error::SysclkTooHighLpr);
        }

        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        #[cfg(feature = "cm4")]
        cr1.modify(|_, w| w.fpdr().clear_bit().lpr().set_bit());

        #[cfg(feature = "cm0p")]
        {
            cr1.modify(|_, w| w.fpdr().clear_bit());
            self.pwr.cr1.modify(|_, w| w.lpr().set_bit());
        }

        Ok(())
    }

    /// Exit low power run mode
    pub fn exit_low_power_run(&self) {
        self.pwr.cr1.modify(|_, w| w.lpr().clear_bit());

        while self.pwr.sr2.read().reglpf().bit_is_set() {}
    }

    /// Enter low power mode
    ///
    /// This function doesn't return when no error occured
    pub fn enter_low_power_mode(&self, mode: Lpms, scb: &mut SCB) -> Result<(), Error> {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        if self.pwr.cr1.read().lpr().bit_is_set() && mode == Lpms::Stop2 {
            return Err(Error::LPRunToStop2Illegal);
        }

        cr1.modify(|_, w| w.lpms().variant(mode.into()));

        scb.set_sleepdeep();

        cortex_m::asm::dsb();
        cortex_m::asm::wfi();

        Ok(())
    }

    pub fn enter_low_power_mode_sleeponexit(&self, mode: Lpms, scb: &mut SCB) -> Result<(), Error> {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        if self.pwr.cr1.read().lpr().bit_is_set() && mode == Lpms::Stop2 {
            return Err(Error::LPRunToStop2Illegal);
        }

        cr1.modify(|_, w| w.lpms().variant(mode.into()));

        scb.set_sleepdeep();
        scb.set_sleeponexit();

        Ok(())
    }

    pub fn enter_low_power_sleep_mode(&self, flash_powered: bool, scb: &mut SCB) {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        cr1.modify(|_, w| w.fpds().bit(flash_powered));

        scb.clear_sleepdeep();

        cortex_m::asm::dsb();
        cortex_m::asm::wfi();
    }

    pub fn enter_low_power_sleep_mode_sleeponexit(&self, flash_powered: bool, scb: &mut SCB) {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        cr1.modify(|_, w| w.fpds().bit(flash_powered));

        scb.clear_sleepdeep();
        scb.set_sleeponexit();
    }

    /// Low-Power mode selection
    pub fn lp_mode(&self) -> Lpms {
        c1_c2!(self.pwr.cr1, self.pwr.c2cr1)
            .read()
            .lpms()
            .bits()
            .try_into()
            .unwrap()
    }

    /// Flash memory power down mode during LPRun for CPUx
    ///
    /// Selects whether the flash memory is in power down mode or idle mode when in LPRun mode. (flash memory
    /// can only be in power down mode when code is executed from SRAM). Flash memory is set
    /// in power down mode only when the system is in LPRun mode, and the FPDR
    /// bit from the other CPU too allows so
    ///
    /// - `false`: Flash memory in idle mode when system is in LPRun mode
    /// - `true`: Flash memory in power down mode when system is in LPRun mode
    pub fn lp_run_flash_powerdown(&self) -> bool {
        c1_c2!(self.pwr.cr1, self.pwr.c2cr1).read().fpdr().bit()
    }

    pub fn set_lp_run_flash_powerdown(&mut self, powerdown: bool) {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        if powerdown {
            const WRITE_KEY: u32 = 0xC1B0;

            // SAFETY: See RM0434 Rev 10 p. 173
            // This doesn't actually overwrite the register values
            unsafe {
                cr1.write(|w| w.bits(WRITE_KEY));
            }
        }

        cr1.modify(|_, w| w.fpdr().bit(powerdown));
    }

    /// Flash memory power down mode during LPSleep for CPUx
    ///
    /// This bit selects whether the flash memory is in power down mode or idle mode when both
    /// CPUs are in Sleep mode. flash memory is set in power down mode only when the system is
    /// in LPSleep mode and the FPDS bit of the other CPU also allows this
    ///
    /// - `false`: Flash memory in Idle mode when system is in LPSleep mode
    /// - `true`: Flash memory in power down mode when system is in LPSleep mode
    pub fn lp_sleep_flash_powerdown(&self) -> bool {
        c1_c2!(self.pwr.cr1, self.pwr.c2cr1).read().fpds().bit()
    }

    pub fn set_lp_sleep_flash_powerdown(&mut self, powerdown: bool) {
        c1_c2!(self.pwr.cr1, self.pwr.c2cr1).modify(|_, w| w.fpds().bit(powerdown));
    }

    pub fn dbp(&self) -> bool {
        self.pwr.cr1.read().dbp().bit()
    }

    pub fn power_range(&self) -> Vos {
        self.pwr.cr1.read().vos().bits().try_into().unwrap()
    }

    pub fn lp_run(&self) -> bool {
        self.pwr.cr1.read().lpr().bit()
    }

    #[cfg(feature = "cm0p")]
    pub fn blee_wakeup(&self) -> bool {
        self.pwr.c2cr1.read().bleewkup().bit()
    }

    #[cfg(feature = "cm0p")]
    pub fn set_blee_wakeup(&mut self, wkup: bool) {
        self.pwr.c2cr1.modify(|_, w| w.bleewkup().bit(wkup));
    }

    #[cfg(feature = "cm0p")]
    pub fn i802e_wakeup(&self) -> bool {
        self.pwr.c2cr1.read()._802ewkup().bit()
    }

    #[cfg(feature = "cm0p")]
    pub fn set_i802e_wakeup(&mut self, wkup: bool) {
        self.pwr.c2cr1.modify(|_, w| w._802ewkup().bit(wkup));
    }

    /// Disable backup domain write protection\n\n\
    ///
    /// - `false`: Access to RTC and Backup registers disabled
    /// - `true`: Access to RTC and Backup registers enabled
    pub fn set_dbp(&self, dbp: bool) {
        self.pwr.cr1.modify(|_, w| w.dbp().bit(dbp));
    }

    pub fn enable_power_voltage_detector(&mut self, level: Pls) {
        self.pwr
            .cr2
            .modify(|_, w| w.pls().variant(level.into()).pvde().set_bit());
    }

    pub fn disable_power_voltage_detector(&mut self) {
        self.pwr.cr2.modify(|_, w| w.pvde().clear_bit());
    }

    /// Peripheral voltage monitoring 1 enable: V_{DDUSB} vs 1.2 V
    pub fn peripheral_voltage_monitoring_1(&mut self, en: bool) {
        self.pwr.cr2.modify(|_, w| w.pvme1().bit(en));
    }

    /// Peripheral voltafe monitoring 3 enable: V_{DDA} vs 1.62 V
    pub fn peripheral_voltage_monitoring_3(&mut self, en: bool) {
        self.pwr.cr2.modify(|_, w| w.pvme3().bit(en));
    }

    /// V_{DDUSB} USB supply valid
    pub fn usb_supply_valid(&self) -> bool {
        self.pwr.cr2.read().usv().bit()
    }

    pub fn set_usb_supply_valid(&mut self, val: bool) {
        self.pwr.cr2.modify(|_, w| w.usv().bit(val));
    }

    /// Enable wakeup pin WKUPx for CPUx
    ///
    /// When this bit is set, the external wakeup pin WKUP5 is enabled and triggers an interrupt and
    /// wakeup from Stop, Standby or Shutdown event when a rising or a falling edge occurs to
    /// CPUx. The active edge is configured via the WP5 bit in the PWR control register 4
    /// (PWR_CR4)
    pub fn enable_wakeup_src(&mut self, src: WakeupSource) {
        let cr3 = &c1_c2!(self.pwr.cr3, self.pwr.c2cr3);

        match src {
            WakeupSource::Wkup1 => cr3.modify(|_, w| w.ewup1().set_bit()),
            WakeupSource::Wkup2 => cr3.modify(|_, w| w.ewup2().set_bit()),
            WakeupSource::Wkup3 => cr3.modify(|_, w| w.ewup3().set_bit()),
            WakeupSource::Wkup4 => cr3.modify(|_, w| w.ewup4().set_bit()),
            WakeupSource::Wkup5 => cr3.modify(|_, w| w.ewup5().set_bit()),
            #[cfg(feature = "cm0p")]
            WakeupSource::Ble => cr3.modify(|_, w| w.eblewup().set_bit()),
            #[cfg(feature = "cm0p")]
            WakeupSource::_802 => cr3.modify(|_, w| w.e802wup().set_bit()),
        }
    }

    pub fn wakeup_polarity(&mut self, pin: WakeupSource, polarity: Polarity) {
        match pin {
            WakeupSource::Wkup1 => self
                .pwr
                .cr4
                .modify(|_, w| w.wp1().bit(polarity == Polarity::Low)),
            WakeupSource::Wkup2 => self
                .pwr
                .cr4
                .modify(|_, w| w.wp2().bit(polarity == Polarity::Low)),
            WakeupSource::Wkup3 => self
                .pwr
                .cr4
                .modify(|_, w| w.wp3().bit(polarity == Polarity::Low)),
            WakeupSource::Wkup4 => self
                .pwr
                .cr4
                .modify(|_, w| w.wp4().bit(polarity == Polarity::Low)),
            WakeupSource::Wkup5 => self
                .pwr
                .cr4
                .modify(|_, w| w.wp5().bit(polarity == Polarity::Low)),
            #[cfg(feature = "cm0p")]
            _ => panic!("Only wakeup pins have an polarity"),
        }
    }

    pub fn disable_wakeup_src(&mut self, src: WakeupSource) {
        let cr3 = &c1_c2!(self.pwr.cr3, self.pwr.c2cr3);

        match src {
            WakeupSource::Wkup1 => cr3.modify(|_, w| w.ewup1().clear_bit()),
            WakeupSource::Wkup2 => cr3.modify(|_, w| w.ewup2().clear_bit()),
            WakeupSource::Wkup3 => cr3.modify(|_, w| w.ewup3().clear_bit()),
            WakeupSource::Wkup4 => cr3.modify(|_, w| w.ewup4().clear_bit()),
            WakeupSource::Wkup5 => cr3.modify(|_, w| w.ewup5().clear_bit()),
            #[cfg(feature = "cm0p")]
            WakeupSource::Ble => cr3.modify(|_, w| w.eblewup().clear_bit()),
            #[cfg(feature = "cm0p")]
            WakeupSource::_802 => cr3.modify(|_, w| w.e802wup().clear_bit()),
        }
    }

    pub fn clear_wakeup_flag(&mut self, src: WakeupSource) {
        match src {
            WakeupSource::Wkup1 => self.pwr.scr.write(|w| w.cwuf1().set_bit()),
            WakeupSource::Wkup2 => self.pwr.scr.write(|w| w.cwuf2().set_bit()),
            WakeupSource::Wkup3 => self.pwr.scr.write(|w| w.cwuf3().set_bit()),
            WakeupSource::Wkup4 => self.pwr.scr.write(|w| w.cwuf4().set_bit()),
            WakeupSource::Wkup5 => self.pwr.scr.write(|w| w.cwuf5().set_bit()),
            #[cfg(feature = "cm0p")]
            WakeupSource::Ble => self.pwr.scr.write(|w| w.cblewuf().set_bit()),
            #[cfg(feature = "cm0p")]
            WakeupSource::_802 => self.pwr.scr.write(|w| w.c802wuf().set_bit()),
        }
    }

    pub fn sram2a_retention(&mut self, rrs: bool) {
        self.pwr.cr3.modify(|_, w| w.rrs().bit(rrs));
    }

    pub fn apply_pull_mode_cfg(&mut self, val: bool) {
        c1_c2!(self.pwr.cr3, self.pwr.c2cr3).modify(|_, w| w.apc().bit(val));
    }

    #[cfg(feature = "cm4")]
    pub fn listen(&mut self, event: Event) {
        match event {
            Event::BorhSmpsStepDownInBypass => self.pwr.cr3.modify(|_, w| w.eborhsdfb().set_bit()),
            Event::CriticalRadioPhaseEOA => self.pwr.cr3.modify(|_, w| w.ecrpe().set_bit()),
            Event::BleEOA => self.pwr.cr3.modify(|_, w| w.eblea().set_bit()),
            Event::_802EOA => self.pwr.cr3.modify(|_, w| w.e802a().set_bit()),
            Event::Cpu2Hold => self.pwr.cr3.modify(|_, w| w.ec2h().set_bit()),
        }
    }

    #[cfg(feature = "cm4")]
    pub fn unlisten(&mut self, event: Event) {
        match event {
            Event::BorhSmpsStepDownInBypass => {
                self.pwr.cr3.modify(|_, w| w.eborhsdfb().clear_bit())
            }
            Event::CriticalRadioPhaseEOA => self.pwr.cr3.modify(|_, w| w.ecrpe().clear_bit()),
            Event::BleEOA => self.pwr.cr3.modify(|_, w| w.eblea().clear_bit()),
            Event::_802EOA => self.pwr.cr3.modify(|_, w| w.e802a().clear_bit()),
            Event::Cpu2Hold => self.pwr.cr3.modify(|_, w| w.ec2h().clear_bit()),
        }
    }

    #[cfg(feature = "cm4")]
    pub fn clear_event_flag(&mut self, event: Event) {
        match event {
            Event::BorhSmpsStepDownInBypass => self.pwr.scr.write(|w| w.csmpsfbf().set_bit()),
            Event::CriticalRadioPhaseEOA => self.pwr.scr.write(|w| w.ccrpef().set_bit()),
            Event::BleEOA => self.pwr.scr.write(|w| w.cbleaf().set_bit()),
            Event::_802EOA => self.pwr.scr.write(|w| w.c802af().set_bit()),
            Event::Cpu2Hold => self.pwr.scr.write(|w| w.cc2hf().set_bit()),
        }
    }

    pub fn internal_wakeup(&mut self, en: bool) {
        c1_c2!(self.pwr.cr3, self.pwr.c2cr3).modify(|_, w| w.eiwul().bit(en));
    }

    pub fn charge_bat(&mut self, bat: BatteryCharging) {
        match bat {
            BatteryCharging::Disabled => self.pwr.cr4.modify(|_, w| w.vbe().clear_bit()),
            BatteryCharging::R1_5 => self
                .pwr
                .cr4
                .modify(|_, w| w.vbe().set_bit().vbrs().bit(true)),
            BatteryCharging::R5 => self
                .pwr
                .cr4
                .modify(|_, w| w.vbe().set_bit().vbrs().bit(false)),
        }
    }

    /// Boot CPU2 after reset or wakeup from stop or standby modes
    pub fn c2boot(&mut self, val: bool) {
        self.pwr.cr4.modify(|_, w| w.c2boot().bit(val));
    }

    pub fn smpsvos_factory() -> u8 {
        SmpsVos::get().factory()
    }

    /// SMPS step-down converter voltage output scaling
    ///
    /// These bits are initialized after Option byte loading with factory trimmed value to reach 1.5 V,
    /// and can subsequently be overwritten by firmware.
    ///
    /// SMPS step down output voltage step size is 50 mV.
    ///
    /// If factory trimmed value - 0x8 gives 1.50 V on VFBSMSPS, to get 1.40 V 0x2 must be
    /// subtracted from this value
    ///
    /// - 0x0 = minimum voltage level
    /// - 0xF = maximum voltage level
    pub fn smps_vos(&mut self, val: u8) {
        assert!(val < 16);

        self.pwr.cr5.modify(|_, w| w.sdvos().variant(val));
    }

    /// SMPS step-down converter supply startup current selection
    ///
    /// Startup current is limited to maximum 80 mA + SMPSSC x 20 mA
    pub fn smps_sc(&mut self, val: u8) {
        assert!(val < 8);

        self.pwr.cr5.modify(|_, w| w.sdsc().variant(val));
    }

    pub fn borh(&mut self, borh: Borh) {
        self.pwr
            .cr5
            .modify(|_, w| w.borhc().bit(borh == Borh::SmpsBypass));
    }

    pub fn smps_enable(&mut self, en: bool) {
        self.pwr.cr5.modify(|_, w| w.sdeb().bit(en));
    }

    pub fn sr1(&self) -> sr1::R {
        self.pwr.sr1.read()
    }

    pub fn sr2(&self) -> sr2::R {
        self.pwr.sr2.read()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatteryCharging {
    Disabled,
    R1_5,
    R5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg(feature = "cm4")]
pub enum Event {
    BorhSmpsStepDownInBypass,
    CriticalRadioPhaseEOA,
    BleEOA,
    _802EOA,
    Cpu2Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeupSource {
    Wkup1,
    Wkup2,
    Wkup3,
    Wkup4,
    Wkup5,
    #[cfg(feature = "cm0p")]
    Ble,
    #[cfg(feature = "cm0p")]
    _802,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Polarity {
    /// Falling Edge
    Low,
    /// Rising Edge
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Borh {
    /// BORH generates a system reset
    SystemReset,
    /// BORH forces SMPS step-down converter bypass mode
    ///
    /// BORL still generates a system reset
    SmpsBypass,
}

/// Low-Power mode selection for CPU1
///
/// Note: If LPR is set, Stop2 cannot be selected and Stop1 mode must be entered
/// instead of Stop2
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Lpms {
    Stop0 = 0b000,
    Stop1 = 0b001,
    Stop2 = 0b010,
    Standby = 0b011,
    #[default]
    Shutdown = 0b100,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Vos {
    /// High performance Range 1 (1.2V)
    Range1 = 0b01,
    /// Low power Range 2 (1.0V)
    Range2 = 0b10,
}

/// Programmable voltage detector level selection
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Pls {
    /// V_{PVD0} ~ 2.0 V
    PVD0 = 0b000,
    /// V_{PVD1} ~ 2.2 V
    PVD1 = 0b001,
    /// V_{PVD2} ~ 2.4 V
    PVD2 = 0b010,
    /// V_{PVD3} ~ 2.5 V
    PVD3 = 0b011,
    /// V_{PVD4} ~ 2.6 V
    PVD4 = 0b100,
    /// V_{PVD5} ~ 2.8 V
    PVD5 = 0b101,
    /// V_{PVD6} ~ 2.9 V
    PVD6 = 0b110,
    /// External input analog voltage PVD_IN (compared to V_{REFINT})
    /// The I/O used as PVD_IN must be configured in analog mode in
    /// GPIO register
    PVDIn = 0b111,
}

pub struct SmpsVos(u32);

// See RM0434 Rev 10 p. 181
define_ptr_type!(SmpsVos, 0x1FFF_7558);

impl SmpsVos {
    /// Get the factory value
    pub fn factory(&self) -> u8 {
        mask_u32!(MASK, OFFSET, [11:8]);

        get_u32!(u8, self.0, MASK, OFFSET)
    }
}
