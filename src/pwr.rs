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

use crate::pac::pwr::{sr1, sr2};
use crate::pac::PWR;
use crate::rcc::{Rcc, Sysclk};
use cortex_m::peripheral::SCB;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

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
    #[cfg(feature = "cm4")]
    pub fn set_power_range(&self, range: Vos, sysclk: impl Sysclk) -> Result<(), Error> {
        if range == Vos::Range2 && sysclk.current_hertz() > 2_000_000 {
            return Err(Error::SysclkTooHighVos);
        }

        self.pwr.cr1.modify(|_, w| w.vos().variant(range.into()));

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
    pub fn enter_low_power_run(&self, rcc: &Rcc) -> Result<(), Error> {
        if rcc.current_sysclk_hertz() > 2_000_000 {
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

    /// Enter low power run mode with disabled flash
    ///
    /// After calling the function, the clock speed must not be increased
    /// above 2 MHz
    ///
    /// # SAFETY
    ///
    /// This method must be called from SRAM
    pub unsafe fn enter_low_power_run_no_flash(&self, rcc: &Rcc) -> Result<(), Error> {
        if rcc.current_sysclk_hertz() > 2_000_000 {
            return Err(Error::SysclkTooHighLpr);
        }

        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        if !cr1.read().fpdr().bit_is_set() {
            // We need to unlock the register first
            const WRITE_KEY: u32 = 0xC1B0;

            // SAFETY: See RM0434 Rev 10 p. 173
            // This doesn't actually overwrite the register values
            cr1.write(|w| w.bits(WRITE_KEY));
        }

        #[cfg(feature = "cm4")]
        cr1.modify(|_, w| w.fpdr().set_bit().lpr().set_bit());

        #[cfg(feature = "cm0p")]
        {
            cr1.modify(|_, w| w.fpdr().set_bit());
            self.pwr.cr1.modify(|_, w| w.lpr().set_bit());
        }

        Ok(())
    }

    /// Exit low power run mode
    pub fn exit_low_power_run(&self) {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);
        let sr2 = &c1_c2!(self.pwr.sr2, self.pwr.c2sr2);

        cr1.modify(|_, w| w.lpr().clear_bit());

        while sr2.read().reglpf().bit_is_set() {}
    }

    /// Enter low power mode
    ///
    /// This function doesn't return when no error occured
    pub fn enter_low_power_mode(&self, mode: Lpms, scb: &mut SCB) -> Result<(), Error> {
        let cr1 = &c1_c2!(self.pwr.cr1, self.pwr.c2cr1);

        if cr1.read().lpr().bit_is_set() && mode == Lpms::Stop2 {
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

        if cr1.read().lpr().bit_is_set() && mode == Lpms::Stop2 {
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

    /// Disable backup domain write protection\n\n\
    ///
    /// - `false`: Access to RTC and Backup registers disabled
    /// - `true`: Access to RTC and Backup registers enabled
    pub fn dbp(&self, dbp: bool) {
        self.pwr.cr1.modify(|_, w| w.dbp().bit(dbp));
    }

    pub fn cr1_read(&self) -> Cr1R {
        Cr1R::read_from(&self.pwr)
    }

    pub fn cr2<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Cr2R, &'w mut Cr2W) -> &'w mut Cr2W,
    {
        let r = Cr2R::read_from(&self.pwr);
        let mut wc = Cr2W(r.0);

        op(&r, &mut wc);

        if r.0 == wc.0 {
            return;
        }

        self.pwr.cr2.modify(|_, w| {
            w.pvde()
                .bit(wc._pvde())
                .pls()
                .variant(wc._pls())
                .pvme1()
                .bit(wc._pvme1())
                .pvme3()
                .bit(wc._pvme3())
                .usv()
                .bit(wc._usv())
        });
    }

    pub fn cr2_read(&self) -> Cr2R {
        Cr2R::read_from(&self.pwr)
    }

    pub fn cr3<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Cr3R, &'w mut Cr3W) -> &'w mut Cr3W,
    {
        let r = Cr3R::read_from(&self.pwr);
        let mut wc = Cr3W(r.0);

        op(&r, &mut wc);

        if r.0 == wc.0 {
            return;
        }

        self.pwr.cr3.modify(|_, w| {
            w.ewup1()
                .bit(wc._ewup1())
                .ewup2()
                .bit(wc._ewup2())
                .ewup3()
                .bit(wc._ewup3())
                .ewup4()
                .bit(wc._ewup4())
                .ewup5()
                .bit(wc._ewup5())
                .eborhsdfb()
                .bit(wc._eborhsmpsfb())
                .rrs()
                .bit(wc._rrs())
                .apc()
                .bit(wc._apc())
                .ecrpe()
                .bit(wc._ecpre())
                .eblea()
                .bit(wc._eblea())
                .e802a()
                .bit(wc._e802a())
                .ec2h()
                .bit(wc._ec2h())
                .eiwul()
                .bit(wc._eiwul())
        });
    }

    pub fn read_cr3(&self) -> Cr3R {
        Cr3R::read_from(&self.pwr)
    }

    pub fn cr4<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Cr4R, &'w mut Cr4W) -> &'w mut Cr4W,
    {
        let r = Cr4R::read_from(&self.pwr);
        let mut wc = Cr4W(r.0);

        op(&r, &mut wc);

        if r.0 == wc.0 {
            return;
        }

        self.pwr.cr4.modify(|_, w| {
            w.wp1()
                .bit(wc._wp1())
                .wp2()
                .bit(wc._wp2())
                .wp3()
                .bit(wc._wp3())
                .wp4()
                .bit(wc._wp4())
                .wp5()
                .bit(wc._wp5())
                .vbe()
                .bit(wc._vbe())
                .vbrs()
                .bit(wc._vbrs())
                .c2boot()
                .bit(wc._c2boot())
        });
    }

    pub fn cr4_read(&self) -> Cr4R {
        Cr4R::read_from(&self.pwr)
    }

    pub fn cr5<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Cr5R, &'w mut Cr5W) -> &'w mut Cr5W,
    {
        let r = Cr5R::read_from(&self.pwr);
        let mut wc = Cr5W(r.0);

        op(&r, &mut wc);

        if r.0 == wc.0 {
            return;
        }

        self.pwr.cr5.modify(|_, w| {
            w.sdvos()
                .variant(wc._smpsvos())
                .sdsc()
                .variant(wc._smpssc())
                .borhc()
                .bit(wc._borhc())
                .sdeb()
                .bit(wc._smpsen())
        });
    }

    pub fn cr5_read(&self) -> Cr5R {
        Cr5R::read_from(&self.pwr)
    }

    pub fn sr1(&self) -> sr1::R {
        self.pwr.sr1.read()
    }

    pub fn sr2(&self) -> sr2::R {
        self.pwr.sr2.read()
    }

    pub fn scr<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&'w mut Scr) -> &'w mut Scr,
    {
        let mut c = Scr::new();

        op(&mut c);

        self.pwr.scr.write(|w| unsafe { w.bits(c.0) });
    }
}

#[cfg(feature = "cm4")]
config_reg_u32! {
    R, Cr1R, PWR, cr1, [
        lpms => (Lpms, u8, [2:0], "Low-Power mode selection"),
        fpdr => (bool, bool, [4:4], "Flash memory power down mode during LPRun for CPUx\n\n\
            Selects whether the flash memory is in power down mode or idle mode when in LPRun mode. (flash memory
            can only be in power down mode when code is executed from SRAM). Flash memory is set
            in power down mode only when the system is in LPRun mode, and the FPDR
            bit from the other CPU too allows so.\n\n\
            - `false`: Flash memory in idle mode when system is in LPRun mode\n\
            - `true`: Flash memory in power down mode when system is in LPRun mode
        "),
        fpds => (bool, bool, [5:5], "Flash memory power down mode during LPSleep for CPUx\n\n\
            This bit selects whether the flash memory is in power down mode or idle mode when both
            CPUs are in Sleep mode. flash memory is set in power down mode only when the system is
            in LPSleep mode and the FPDS bit of the other CPU also allows this.\n\n\
            - `false`: Flash memory in Idle mode when system is in LPSleep mode\n\
            - `true`: Flash memory in power down mode when system is in LPSleep mode
        "),
        dbp => (bool, bool, [8:8], "Disable backup domain write protection\n\n\
                - `false`: Access to RTC and Backup registers disabled\n\
                - `true`: Access to RTC and Backup registers enabled
        "),
        vos => (Vos, u8, [10:9], "Voltage scaling range selection"),
        lpr => (bool, bool, [14:14], "Low-power run"),
    ]
}

#[cfg(feature = "cm0p")]
config_reg_u32! {
    R, Cr1R, PWR, cr1, [
        lpms => (Lpms, u8, [2:0], "Low-Power mode selection"),
        fpdr => (bool, bool, [4:4], "Flash memory power down mode during LPRun for CPUx\n\n\
            Selects whether the flash memory is in power down mode or idle mode when in LPRun mode. (flash memory
            can only be in power down mode when code is executed from SRAM). Flash memory is set
            in power down mode only when the system is in LPRun mode, and the FPDR
            bit from the other CPU too allows so.\n\n\
            - `false`: Flash memory in idle mode when system is in LPRun mode\n\
            - `true`: Flash memory in power down mode when system is in LPRun mode
        "),
        fpds => (bool, bool, [5:5], "Flash memory power down mode during LPSleep for CPUx\n\n\
            This bit selects whether the flash memory is in power down mode or idle mode when both
            CPUs are in Sleep mode. flash memory is set in power down mode only when the system is
            in LPSleep mode and the FPDS bit of the other CPU also allows this.\n\n\
            - `false`: Flash memory in Idle mode when system is in LPSleep mode\n\
            - `true`: Flash memory in power down mode when system is in LPSleep mode
        "),
        bleewkup => (bool, bool, [14:14], "BLE external wakeup\n\n\
            When set this bit forces a wakeup of the BLE controller. It is automatically reset\n\
            when BLE controller exits its sleep mode
        "),
        i802ewkup => (bool, bool, [15:15], "802.15.4 external wakeup signal\n\n\
            When set this bit forces a wakeup of the 802.15.4 controller. It is automatically reset\n\
            when 802.15.4 controller exits its sleep mode
        "),
    ]
}

#[cfg(feature = "cm4")]
config_reg_u32! {
    W, Cr1W, PWR, cr1, [
        dbp => (_dbp, bool, bool, [8:8], "Disable backup domain write protection\n\n\
            - `false`: Access to RTC and Backup registers disabled\n\
            - `true`: Access to RTC and Backup registers enabled
        "),
    ]
}

#[cfg(feature = "cm0p")]
config_reg_u32! {
    W, Cr1W, PWR, c2cr1, [
        bleewkup => (_bleewkup, bool, bool, [14:14], "BLE external wakeup\n\n\
            When set this bit forces a wakeup of the BLE controller. It is automatically reset\n\
            when BLE controller exits its sleep mode
        "),
        i802ewkup => (_802ewkup, bool, bool, [15:15], "802.15.4 external wakeup signal\n\n\
            When set this bit forces a wakeup of the 802.15.4 controller. It is automatically reset\n\
            when 802.15.4 controller exits its sleep mode
        "),
    ]
}

config_reg_u32! {
    RW, Cr2R, Cr2W, PWR, cr2, [
        pvde => (_pvde, bool, bool, [0:0], "Programmable voltage detector enable"),
        pls => (_pls, Pls, u8, [3:1], "Programmable voltage detector level selection\n\n\
            Note: These bits are write-protected once PVDL (PVDLock) is set in SYSCFG_CBR register
        "),
        pvme1 => (_pvme1, bool, bool, [4:4], "Peripheral voltage monitoring 1 enable: V_{DDUSB} vs 1.2 V"),
        pvme3 => (_pvme3, bool, bool, [6:6], "Peripheral voltafe monitoring 3 enable: V_{DDA} vs 1.62 V"),
        usv => (_usv, bool, bool, [10:10], "V_{DDUSB} USB supply valid"),
    ]
}

config_reg_u32! {
    RW, Cr3R, Cr3W, PWR, cr3, [
        ewup1 => (_ewup1, bool, bool, [0:0], "Enable wakeup pin WKUP1 for CPUx\n\n\
            When this bit is set, the external wakeup pin WKUP1 is enabled and triggers an interrupt and
            wakeup from Stop, Standby or Shutdown event when a rising or a falling edge occurs to
            CPUx. The active edge is configured via the WP1 bit in the PWR control register 4
            (PWR_CR4)
        "),
        ewup2 => (_ewup2, bool, bool, [1:1], "Enable wakeup pin WKUP2 for CPUx\n\n\
            When this bit is set, the external wakeup pin WKUP2 is enabled and triggers an interrupt and
            wakeup from Stop, Standby or Shutdown event when a rising or a falling edge occurs to
            CPUx. The active edge is configured via the WP2 bit in the PWR control register 4
            (PWR_CR4)
        "),
        ewup3 => (_ewup3, bool, bool, [2:2], "Enable wakeup pin WKUP3 for CPUx\n\n\
            When this bit is set, the external wakeup pin WKUP3 is enabled and triggers an interrupt and
            wakeup from Stop, Standby or Shutdown event when a rising or a falling edge occurs to
            CPUx. The active edge is configured via the WP3 bit in the PWR control register 4
            (PWR_CR4)
        "),
        ewup4 => (_ewup4, bool, bool, [3:3], "Enable wakeup pin WKUP4 for CPUx\n\n\
            When this bit is set, the external wakeup pin WKUP4 is enabled and triggers an interrupt and
            wakeup from Stop, Standby or Shutdown event when a rising or a falling edge occurs to
            CPUx. The active edge is configured via the WP4 bit in the PWR control register 4
            (PWR_CR4)
        "),
        ewup5 => (_ewup5, bool, bool, [4:4], "Enable wakeup pin WKUP5 for CPUx\n\n\
            When this bit is set, the external wakeup pin WKUP5 is enabled and triggers an interrupt and
            wakeup from Stop, Standby or Shutdown event when a rising or a falling edge occurs to
            CPUx. The active edge is configured via the WP5 bit in the PWR control register 4
            (PWR_CR4)
        "),
        eborhsmpsfb => (_eborhsmpsfb, bool, bool, [8:8], "Enable BORH and SMPS step-down converter forced in Bypass interrupts for CPUx"),
        rrs => (_rrs, bool, bool, [9:9], "SRAM2a retention in standby mode\n\n\
            - `false`: SRAM2a powered off in standby mode (content is lost)\n\
            - `true`: SRAM2a powered by the low power regulator in standby mode (content is kept)
        "),
        apc => (_apc, bool, bool, [10:10], "Apply pull-up and pull-down configuration from CPUx\n\n\
            When this bit for CPUx or the APC bit for the other CPU is set, the I/O pull-up and pull-
            down configurations defined in the PWR_PUCRx and PWR_PDCRx registers are applied.
            When both bits are cleared, the PWR_PUCRx and PWR_PDCRx registers are not applied to
            the I/Os
        "),
        ecpre => (_ecpre, bool, bool, [11:11], "Enable critical radio phase end of activity interrupt for CPUx"),
        eblea => (_eblea, bool, bool, [12:12], "Enable BLE end of activity interrupt for CPUx"),
        e802a => (_e802a, bool, bool, [13:13], "Enable 802.15.4 end of activity interrupt for CPUx"),
        ec2h => (_ec2h, bool, bool, [14:14], "Enable CPU2 Hold interrupt for CPUx"),
        eiwul => (_eiwul, bool, bool, [15:15], "Enable internal wakeup line for CPUx"),
    ]
}

config_reg_u32! {
    RW, Cr4R, Cr4W, PWR, cr4, [
        wp1 => (_wp1, bool, bool, [0:0], "Wakeup pin WKUP1 polarity\n\n\
            - `false`: Detection on high level (rising edge)\n\
            - `true`: Detection on low level (falling edge)
        "),
        wp2 => (_wp2, bool, bool, [1:1], "Wakeup pin WKUP2 polarity\n\n\
            - `false`: Detection on high level (rising edge)\n\
            - `true`: Detection on low level (falling edge)
        "),
        wp3 => (_wp3, bool, bool, [2:2], "Wakeup pin WKUP3 polarity\n\n\
            - `false`: Detection on high level (rising edge)\n\
            - `true`: Detection on low level (falling edge)
        "),
        wp4 => (_wp4, bool, bool, [3:3], "Wakeup pin WKUP4 polarity\n\n\
            - `false`: Detection on high level (rising edge)\n\
            - `true`: Detection on low level (falling edge)
        "),
        wp5 => (_wp5, bool, bool, [4:4], "Wakeup pin WKUP5 polarity\n\n\
            - `false`: Detection on high level (rising edge)\n\
            - `true`: Detection on low level (falling edge)
        "),
        vbe => (_vbe, bool, bool, [8:8], "V_{BAT} Battery charging enable"),
        vbrs => (_vbrs, bool, bool, [9:9], "V_{BAT} battery charging resistor selection\n\n\
            - `false`: Charge V_{BAT} through a 5 kOhm resistor\n\
            - `true`: Charge V_{BAT} through a 1.5 kOhm resistor
        "),
        c2boot => (_c2boot, bool, bool, [15:15], "Boot CPU2 after reset or wakeup from stop or standby modes"),
    ]
}

config_reg_u32! {
    RW, Cr5R, Cr5W, PWR, cr5, [
        smpsvos => (_smpsvos, u8, u8, [3:0], "SMPS step-down converter voltage output scaling\n\n\
            These bits are initialized after Option byte loading with factory trimmed value to reach 1.5 V,
            and can subsequently be overwritten by firmware.
            SMPS step down output voltage step size is 50 mV.
            If factory trimmed value - 0x8 gives 1.50 V on VFBSMSPS, to get 1.40 V 0x2 must be
            subtracted from this value.
            - 0x0 = minimum voltage level
            - 0xF = maximum voltage level
        "),
        smpssc => (_smpssc, u8, u8, [6:4], "SMPS step-down converter supply startup current selection\n\n\
            Startup current is limited to maximum 80 mA + SMPSSC * 20 mA
        "),
        borhc => (_borhc, bool, bool, [8:8], "BORH configuration selection\n\n\
            - `false`: BORH generates a system reset\n\
            - `true`:  BORH forces SMPS step-down converter Bypass mode (BORL still generates a system reset)
        "),
        smpsen => (_smpsen, bool, bool, [15:15], "Enable SMPS step-down converter SMPS mode enabled\n\n\
            This bit is reset to 0 when SMPS step-down converter switching on the fly is enabled and the
            VDD level drops below the BORH threshold
        "),
    ]
}

clear_status_reg_u32! {
    Scr, [
        cwuf1 => (0, "Clear wakeup flag 1"),
        cwuf2 => (1, "Clear wakeup flag 2"),
        cwuf3 => (2, "Clear wakeup flag 3"),
        cwuf4 => (3, "Clear wakeup flag 4"),
        cwuf5 => (4, "Clear wakeup flag 5"),
        csmpsfbf => (7, "Clear SMPS step-down converter forced in Bypass interrupt flag"),
        cborhf => (8, "Clear BORH interrupt flag"),
        cblewuf => (9, "Clear BLE wakeup interrupt flag"),
        c802wuf => (10, "Clear 802.15.4 wakeup interrupt flag"),
        ccrpef => (11, "Clear critical radio phase end of activity interrupt flag"),
        cbleaf => (12, "Clear BLE end of activity interrupt flag"),
        c802af => (13, "Clear 802.15.4 end of activity interrupt flag"),
        cc2hf => (14, "Clear CPU2 hold interrupt flag"),
    ]
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
