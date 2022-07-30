use crate::signature::FlashSize;
use crate::{pac::FLASH, pwr::Vos};
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};

/// Total number of pages, indexed 0 to 255
pub const NUM_PAGES: usize = 256;
/// Flash Page Size in Bytes (4 KiB)
pub const PAGE_SIZE: usize = 0x0000_1000;
/// See RM0434 Rev 9 p.75
pub const FLASH_BASE_ADDR: usize = 0x0800_0000;

pub fn flash_end() -> usize {
    FLASH_BASE_ADDR + FlashSize::get().bytes() - 1
}

#[derive(Debug)]
#[repr(C)]
pub struct FlashUid {
    // 0x1FFF_7580
    reg_1: u32,
    // 0x1FFF_7584
    reg_2: u32,
}

define_ptr_type!(FlashUid, 0x1FFF_7580);

impl FlashUid {
    pub fn uid(&self) -> u32 {
        self.reg_1
    }

    pub fn dev_id(&self) -> u8 {
        (self.reg_2 & 0xFF) as u8
    }

    pub fn manufacturer(&self) -> u32 {
        (self.reg_2 & 0xFFFF_FF00) >> 8
    }

    pub fn uid64(&self) -> u64 {
        ((self.reg_1 as u64) << 32) | self.reg_2 as u64
    }
}

pub enum Error {
    /// Program / Erase operation suspended (PESD)
    OperationSuspended,
    /// CPU1 attempts to read/write from/to secured pages
    SecureFlashError,
    /// Error with custom status
    Status(Status),
}

pub struct Status {
    #[cfg(feature = "cm4")]
    r: crate::pac::flash::sr::R,
    #[cfg(feature = "cm0p")]
    r: crate::pac::flash::c2sr::R,
}

impl Status {
    /// Status register
    #[cfg(feature = "cm4")]
    pub fn r(&self) -> &crate::pac::flash::sr::R {
        &self.r
    }

    /// Status register
    #[cfg(feature = "cm0p")]
    pub fn r(&self) -> &crate::pac::flash::c2sr::R {
        &self.r
    }

    /// Programming Error occured
    pub fn prog_err(&self) -> bool {
        self.r.sizerr().bit_is_set()
            || self.r.miserr().bit_is_set()
            || self.r.fasterr().bit_is_set()
            || self.r.wrperr().bit_is_set()
            || self.r.pgaerr().bit_is_set()
            || self.r.pgserr().bit_is_set()
            || self.r.progerr().bit_is_set()
    }
}

pub trait FlashExt {
    fn constrain(self) -> Flash;
}

impl FlashExt for FLASH {
    fn constrain(self) -> Flash {
        Flash { flash: self }
    }
}

pub struct Flash {
    flash: FLASH,
}

impl Flash {
    pub fn new(flash: FLASH) -> Self {
        Self { flash }
    }

    pub const fn address(&self) -> usize {
        FLASH_BASE_ADDR
    }

    pub fn len(&self) -> usize {
        FlashSize::get().bytes()
    }

    pub fn unlock(&mut self) -> UnlockedFlash {
        unlock(&self.flash);

        UnlockedFlash { flash: self }
    }

    pub fn page(&self, offset: usize) -> Option<u8> {
        if offset >= self.len() {
            return None;
        }

        u8::try_from(offset / PAGE_SIZE).ok()
    }

    pub fn uid(&self) -> u64 {
        FlashUid::get().uid64()
    }

    pub fn load_option_bytes(&mut self) {
        while self.flash.sr.read().bsy().bit_is_set() {}

        self.flash.cr.modify(|_, w| w.obl_launch().set_bit());
    }

    pub fn acr<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&AcrR, &'w mut AcrW) -> &'w mut AcrW,
    {
        let r = AcrR::read_from(&self.flash);
        let mut wc = AcrW(r.0);

        op(&r, &mut wc);

        self.flash.acr.modify(|_, w| {
            w.latency()
                .variant(wc._latency())
                .prften()
                .bit(wc._prften())
                .icen()
                .bit(wc._icen())
                .icrst()
                .bit(wc._icrst())
                .dcen()
                .bit(wc._dcen())
                .dcrst()
                .bit(wc._dcrst())
                .pes()
                .bit(wc._pes())
                .empty()
                .bit(wc._empty())
        });
    }

    pub fn acr_2<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Acr2R, &'w mut Acr2W) -> &'w mut Acr2W,
    {
        let r = Acr2R::read_from(&self.flash);
        let mut wc = Acr2W(r.0);

        op(&r, &mut wc);

        self.flash.c2acr.modify(|_, w| {
            w.prften()
                .bit(wc._prfen())
                .icen()
                .bit(wc._icen())
                .icrst()
                .bit(wc._icrst())
                .pes()
                .bit(wc._pes())
        })
    }
}

pub struct UnlockedFlash<'a> {
    flash: &'a mut Flash,
}

impl Drop for UnlockedFlash<'_> {
    fn drop(&mut self) {
        lock(&self.flash.flash);
    }
}

/*macro_rules! clear_sr {
    ($sr:ident, $misserr:ident) => {
        fn clear_sr(&self) {
            self.reg().$sr.modify(|_, w| {
                w.eop()
                    .set_bit()
                    .fasterr()
                    .set_bit()
                    .$misserr()
                    .set_bit()
                    .operr()
                    .set_bit()
                    .pgaerr()
                    .set_bit()
                    .pgserr()
                    .set_bit()
                    .progerr()
                    .set_bit()
                    .rderr()
                    .set_bit()
                    .sizerr()
                    .set_bit()
                    .wrperr()
                    .set_bit()
            });
        }
    };
}*/

impl<'a> UnlockedFlash<'a> {
    fn reg(&self) -> &FLASH {
        &self.flash.flash
    }

    fn address(&self) -> usize {
        self.flash.address()
    }

    /// Erase a single page
    ///
    /// # SAFETY
    ///
    /// Make sure you don't erase your code
    //
    // See RM0434 Rev9 p. 82
    pub unsafe fn page_erase(&mut self, page: u8) -> Result<(), Error> {
        let sr = &c1_c2!(self.reg().sr, self.reg().c2sr);
        let cr = &c1_c2!(self.reg().cr, self.reg().c2cr);

        while sr.read().bsy().bit_is_set() {}

        if sr.read().pesd().bit_is_set() {
            return Err(Error::OperationSuspended);
        }

        self.clear_sr();

        cr.modify(|_, w| {
            w
                // Set page number
                .pnb()
                .variant(page)
                // No mass erase
                .mer()
                .clear_bit()
                // Erase that page
                .per()
                .set_bit()
                // No programming
                .pg()
                .clear_bit()
                // No fast programming
                .fstpg()
                .clear_bit()
                // Start
                .strt()
                .set_bit()
        });

        while sr.read().bsy().bit_is_set() {}

        Ok(())
    }

    /// CPU2: Complete flash will be wiped
    ///
    /// # SAFETY
    ///
    /// This must be executed from SRAM
    //
    // See RM0434 Rev 9 p. 83
    #[cfg(feature = "cm0p")]
    pub unsafe fn mass_erase(&mut self) -> Result<(), Error> {
        while self.flash.flash.c2sr.read().bsy().bit_is_set() {}

        self.clear_sr();

        self.flash.flash.c2cr.modify(|_, w| {
            w
                // Mass Erase
                .mer()
                .set_bit()
                // No page erase
                .per()
                .clear_bit()
                // No programming
                .pg()
                .clear_bit()
                // No fast programming
                .fstpg()
                .clear_bit()
                // Start
                .strt()
                .set_bit()
        });

        while self.flash.flash.c2sr.read().bsy().bit_is_set() {}

        Ok(())
    }

    /// Normal programming of data into flash
    ///
    /// - `offset` must be multiple of 8
    /// - size of `data` must be multiple of 64 bits
    //
    // See RM0434 Rev 9 p. 84
    pub fn program(&mut self, offset: usize, data: &[u8]) -> Result<(), Error> {
        if data.len() % 8 != 0 || offset % 8 != 0 {
            panic!("Size of `data` and offset must be a multiple of 64 bit");
        }

        self.clear_sr();

        let cr = &c1_c2!(self.reg().cr, self.reg().c2cr);
        let sr = &c1_c2!(self.reg().sr, self.reg().c2sr);

        cr.modify(|_, w| {
            w
                // Programming Mode
                .pg()
                .set_bit()
                // No page erase
                .per()
                .clear_bit()
                // No mass erase
                .mer()
                .clear_bit()
                // No fast programming
                .fstpg()
                .clear_bit()
        });

        let mut ptr = self.address() as *mut u32;
        // SAFETY: offset is in bounds of flash
        // offset / 4 bytes
        ptr = unsafe { ptr.add(offset >> 2) };

        for chunk in data.chunks_exact(8) {
            let w1 = u32::from_le_bytes(chunk[0..4].try_into().unwrap());
            let w2 = u32::from_le_bytes(chunk[4..].try_into().unwrap());

            // SAFETY: RM0434 Rev 9 p. 84 - Standard Programming - Step 4
            unsafe {
                core::ptr::write_volatile(ptr, w1);
                ptr = ptr.add(1);
                core::ptr::write_volatile(ptr, w2);
                ptr = ptr.add(1);
            }

            while sr.read().bsy().bit_is_set() {}

            if sr.read().eop().bit_is_set() {
                sr.modify(|_, w| w.eop().clear_bit());
            } else {
                return Err(Error::Status(Status { r: sr.read() }));
            }
        }

        cr.modify(|_, w| w.pg().clear_bit());

        Ok(())
    }

    /// CPU2: Perform fast programming
    ///
    /// Note:
    /// - A mass erase is performed before
    /// - Flash Memory Clock Frequency (HCLK4) must be at least 8 MHz
    ///
    /// # SAFETY
    ///
    /// This must be executed from SRAM
    //
    // See RM0434 Rev 9 p. 85
    #[cfg(feature = "cm0p")]
    pub unsafe fn fast_program(&mut self, offset: usize, data: &[u8]) -> Result<(), Error> {
        if data.len() % 512 != 0 || offset % 512 != 0 {
            panic!("Size of `data` and offset must be a multiple of 512 Bytes");
        }

        self.mass_erase()?;

        while self.flash.flash.c2sr.read().bsy().bit_is_set() {}

        self.clear_sr();

        self.flash.flash.c2cr.modify(|_, w| {
            w
                // Fast Programming
                .fstpg()
                .set_bit()
                // No normal programming
                .pg()
                .clear_bit()
                // No mass erase
                .mer()
                .clear_bit()
                // No page erase
                .per()
                .clear_bit()
        });

        let mut ptr = self.flash.address() as *mut u32;
        // offset / 4 bytes
        ptr = ptr.add(offset >> 2);

        for chunk in data.chunks_exact(512) {
            for word in chunk
                .chunks_exact(4)
                .map(|x| u32::from_le_bytes(x.try_into().unwrap()))
            {
                core::ptr::write_volatile(ptr, word);
                ptr = ptr.add(1);
            }

            while self.flash.flash.c2sr.read().bsy().bit_is_set() {}

            if self.flash.flash.c2sr.read().eop().bit_is_set() {
                self.flash.flash.c2sr.modify(|_, w| w.eop().clear_bit());
            } else {
                return Err(Error::Status(Status {
                    r: self.flash.flash.c2sr.read(),
                }));
            }
        }

        self.flash.flash.c2cr.modify(|_, w| w.fstpg().clear_bit());

        Ok(())
    }

    pub fn options_unlocked(&mut self) -> OptionsUnlocked<'_, 'a> {
        unlock_options(&self.flash.flash);

        OptionsUnlocked { flash: self }
    }

    /*#[cfg(feature = "cm4")]
    clear_sr!(sr, miserr);

    #[cfg(feature = "cm0p")]
    clear_sr!(c2sr, misserr);*/

    fn clear_sr(&self) {
        // Register is cleared by writing 1 to enabled flags so we can just write the old value
        c1_c2!(self.reg().sr, self.reg().c2sr).modify(|_, w| w);
    }
}

pub struct OptionsUnlocked<'a, 'b> {
    flash: &'a mut UnlockedFlash<'b>,
}

impl Drop for OptionsUnlocked<'_, '_> {
    fn drop(&mut self) {
        lock_options(&self.flash.flash.flash);
    }
}

impl OptionsUnlocked<'_, '_> {
    fn reg(&self) -> &FLASH {
        self.flash.reg()
    }

    pub fn user_options<F>(&self, op: F) -> Result<(), Error>
    where
        F: for<'w> FnOnce(&UserOptionsR, &'w mut UserOptionsW) -> &'w mut UserOptionsW,
    {
        let r = UserOptionsR::read_from(self.reg());
        let mut wc = UserOptionsW(r.0);

        op(&r, &mut wc);

        self.reg().optr.modify(|_, w| {
            w.rdp()
                .variant(wc._rdp())
                .ese()
                .bit(wc._ese())
                .bor_lev()
                .variant(wc._bor_level())
                .n_rst_stop()
                .bit(wc._n_rst_stop())
                .n_rst_stdby()
                .bit(wc._n_rst_stdby())
                .n_rst_shdw()
                .bit(wc._n_rst_shdw())
                .idwg_sw()
                .bit(wc._idwg_sw())
                .iwdg_stop()
                .bit(wc._iwdg_stop())
                .iwdg_stdby()
                .bit(wc._iwdg_stdby())
                .wwdg_sw()
                .bit(wc._wwdg_sw())
                .n_boot1()
                .bit(wc._n_boot_1())
                .sram2_pe()
                .bit(wc._sram2_pe())
                .sram2_rst()
                .bit(wc._sram2_rst())
                .n_swboot0()
                .bit(wc._n_swboot_0())
                .n_boot0()
                .bit(wc._n_boot_0())
                .agc_trim()
                .variant(wc._agc_trim())
        });

        while self.reg().sr.read().bsy().bit_is_set() {}

        if self.reg().sr.read().pesd().bit_is_set() || self.reg().c2sr.read().pesd().bit_is_set() {
            return Err(Error::OperationSuspended);
        }

        self.reg().cr.modify(|_, w| w.optstrt().set_bit());

        while self.reg().sr.read().bsy().bit_is_set() {}

        Ok(())
    }

    pub fn pcrop1a_strt<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Pcrop1aStrtR, &'w mut Pcrop1aStrtW) -> &'w mut Pcrop1aStrtW,
    {
        let r = Pcrop1aStrtR::read_from(self.reg());
        let mut wc = Pcrop1aStrtW(r.0);

        op(&r, &mut wc);

        self.reg()
            .pcrop1asr
            .modify(|_, w| w.pcrop1a_strt().variant(wc._pcrop1a_strt()));
    }

    pub fn pcrop1a_end<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Pcrop1aEndR, &'w mut Pcrop1aEndW) -> &'w mut Pcrop1aEndW,
    {
        let r = Pcrop1aEndR::read_from(self.reg());
        let mut wc = Pcrop1aEndW(r.0);

        op(&r, &mut wc);

        self.reg().pcrop1aer.modify(|_, w| {
            w.pcrop1a_end()
                .variant(wc._pcrop1a_end())
                .pcrop_rdp()
                .bit(wc._pcrop_rdp())
        });
    }

    pub fn wrp1a<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Wrp1AR, &'w mut Wrp1AW) -> &'w mut Wrp1AW,
    {
        let r = Wrp1AR::read_from(self.reg());
        let mut wc = Wrp1AW(r.0);

        op(&r, &mut wc);

        self.reg().wrp1ar.modify(|_, w| {
            w.wrp1a_strt()
                .variant(wc._wrp1a_strt())
                .wrp1a_end()
                .variant(wc._wrp1a_end())
        });
    }

    pub fn wrp1b<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Wrp1BR, &'w mut Wrp1BW) -> &'w mut Wrp1BW,
    {
        let r = Wrp1BR::read_from(self.reg());
        let mut wc = Wrp1BW(r.0);

        op(&r, &mut wc);

        self.reg().wrp1br.modify(|_, w| {
            w.wrp1b_strt()
                .variant(wc._wrp1b_strt())
                .wrp1b_end()
                .variant(wc._wrp1b_end())
        })
    }

    pub fn pcrop1b_strt<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Pcrop1bStrtR, &'w mut Pcrop1bStrtW) -> &'w mut Pcrop1bStrtW,
    {
        let r = Pcrop1bStrtR::read_from(self.reg());
        let mut wc = Pcrop1bStrtW(r.0);

        op(&r, &mut wc);

        self.reg()
            .pcrop1bsr
            .modify(|_, w| w.pcrop1b_strt().variant(wc._pcrop1b_strt()));
    }

    pub fn pcrop1b_end<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&Pcrop1bEndR, &'w mut Pcrop1bEndW) -> &'w mut Pcrop1bEndW,
    {
        let r = Pcrop1bEndR::read_from(self.reg());
        let mut wc = Pcrop1bEndW(r.0);

        op(&r, &mut wc);

        self.reg()
            .pcrop1ber
            .modify(|_, w| w.pcrop1b_end().variant(wc._pcrop1b_end()));
    }

    /// Secure flash options (SFR)
    ///
    /// This register can only be written to by CPU2.
    /// This register can be read by both CPUs.
    pub fn secure_flash_options<F>(&self, op: F)
    where
        F: for<'w> FnOnce(
            &SecureFlashOptionsR,
            &'w mut SecureFlashOptionsW,
        ) -> &'w mut SecureFlashOptionsW,
    {
        let r = SecureFlashOptionsR::read_from(self.reg());
        let mut wc = SecureFlashOptionsW(r.0);

        op(&r, &mut wc);

        if r.0 != wc.0 {
            self.reg().sfr.modify(|_, w| {
                w.sfsa()
                    .variant(wc._sfsa())
                    .fsd()
                    .bit(wc._fsd())
                    .dds()
                    .bit(wc._dds())
            });
        }
    }

    pub fn secure_sram2_options<F>(&self, op: F)
    where
        F: for<'w> FnOnce(
            &SecureSRAM2OptionsR,
            &'w mut SecureSRAM2OptionsW,
        ) -> &'w mut SecureSRAM2OptionsW,
    {
        let r = SecureSRAM2OptionsR::read_from(self.reg());
        let mut wc = SecureSRAM2OptionsW(r.0);

        op(&r, &mut wc);

        if r.0 != wc.0 {
            self.reg().srrvr.modify(|_, w| {
                w.sbrv()
                    .variant(wc._sbrv())
                    .sbrsa()
                    .variant(wc._sbrsa())
                    .brsd()
                    .bit(wc._brsd())
                    .snbrsa()
                    .variant(wc._snbrsa())
                    .nbrsd()
                    .bit(wc._nbrsd())
                    .c2opt()
                    .bit(wc._c2opt())
            });
        }
    }

    pub fn ipcc<F>(&self, op: F)
    where
        F: for<'w> FnOnce(&IpccR, &'w mut IpccW) -> &'w mut IpccW,
    {
        let r = IpccR::read_from(self.reg());
        let mut wc = IpccW(r.0);

        op(&r, &mut wc);

        self.reg()
            .ipccbr
            .modify(|_, w| w.ipccdba().variant(wc._ipccdba()));
    }
}

config_reg_u32! {
    RW, AcrR, AcrW, FLASH, acr, [
        latency => (_latency, Latency, u8, [2:0], "Latency\n\n\
            Represents the ratio of the flash memory HCLK clock period to the flash memory access time
        "),
        prften => (_prften, bool, bool, [8:8], "CPU1 Prefetch enable\n\n\
            - `false`: CPU1 prefetch disabled\n\
            - `true`: CPU1 prefetch enabled
        "),
        icen => (_icen, bool, bool, [9:9], "CPU1 Instruction cache enable\n\n\
            - `false`: CPU1 instruction cache disabled\n\
            - `true`: CPU1 instruction cache enabled
        "),
        dcen => (_dcen, bool, bool, [10:10], "CPU1 data cache enable\n\n\
            - `false`: CPU1 data cache is disabled\n\
            - `true`: CPU1 data cache is enabled
        "),
        icrst => (_icrst, bool, bool, [11:11], "CPU1 instruction cache reset\n\n\
            This bit can be written only if the instruction cache is disabled\n\
            - `false`: CPU1 instruction cache is not reset\n\
            - `true`: CPU1 instruction cache is reset
        "),
        dcrst => (_dcrst, bool, bool, [12:12], "CPU1 data cache reset\n\n\
            This bit can be written only if the data cache is disabled\n\
            - `false`: CPU1 data cache is not reset\n\
            - `true`: CPU1 data cache is reset
        "),
        pes => (_pes, bool, bool, [15:15], "CPU1 Program / erase suspend request\n\n\
            - `false`: Flash memory program and erase operations granted\n\
            - `true`: New flash memory program and erase operations suspended until this
            bit and the same bit for CPU2 is cleared
        "),
        empty => (_empty, bool, bool, [16:16], "CPU1 Flash memory user area empty\n\n\
            When read indicates whether the first location of the User Flash memory is erased or has a 
programmed value\n\
            - `false`: User flash memory programmed\n\
            - `true`: User flash memory empty
        "),
    ]
}

config_reg_u32! {
    RW, Acr2R, Acr2W, FLASH, c2acr, [
        prfen => (_prfen, bool, bool, [8:8], "CPU2 prefetch enable\n\n\
            - `false`: CPU2 prefetch disabled\n\
            - `true`: CPU2 prefetch enabled
        "),
        icen => (_icen, bool, bool, [9:9], "CPU2 instruction cache enable\n\n\
            - `false`: CPU2 instruction cache disabled\n\
            - `true`: CPU2 instruction cache enabled
        "),
        icrst => (_icrst, bool, bool, [11:11], "CPU2 instruction cache reset\n\n\
            - `false`: CPU2 instruction cache is not reset\n\
            - `true`: CPU2 instruction cache is reset
        "),
        pes => (_pes, bool, bool, [15:15], "CPU2 program / erase suspend request\n\n\
            - `false`: Flash memory program and erase operations granted\n\
            - `true`: New flash memory program and erase operations suspended until this
            bit and the same bit for CPU1 is cleared
        "),
    ]
}

config_reg_u32! {
    RW, UserOptionsR, UserOptionsW, FLASH, optr, [
        rdp => (_rdp, RdpLevel, u8, [7:0], "Read Protection Level"),
        ese => (_ese, bool, bool, [8:8], "System Security enabled flag"),
        bor_level => (_bor_level, BorResetLevel, u8, [11:9], "BOR Reset Level"),
        n_rst_stop => (_n_rst_stop, bool, bool, [12:12], "No Reset in stop mode"),
        n_rst_stdby => (_n_rst_stdby, bool, bool, [13:13], "No Reset in standby mode"),
        n_rst_shdw => (_n_rst_shdw, bool, bool, [14:14], "No Reset in shutdown mode"),
        idwg_sw => (_idwg_sw, bool, bool, [16:16], "Independent watchdog selection\n\n\
            - `false`: Hardware independent watchdog\n\
            - `true`: Software independent watchdog
        "),
        iwdg_stop => (_iwdg_stop, bool, bool, [17:17], "Independent watchdog counter freeze in stop mode\n\n\
            - `false`: Independent watchdog counter is frozen in standby mode\n\
            - `true`: Independent watchdog counter is running in standby mode
        "),
        iwdg_stdby => (_iwdg_stdby, bool, bool, [18:18], "Independent watchdog counter freeze in standby mode"),
        wwdg_sw => (_wwdg_sw, bool, bool, [19:19], "Window watchdog selection\n\n\
            - `false`: Hardware window watchdog\n\
            - `true`: Software window watchdog
        "),
        n_boot_1 => (_n_boot_1, bool, bool, [23:23], "Boot configuration\n\n\
            Together with BOOT0 pin or option bit nBOOT0 (depending on nSWBOOT0 option bit configuration), \
            this bit selects boot mode from the user flash memory, SRAM1 or the System Memory
        "),
        sram2_pe => (_sram2_pe, bool, bool, [24:24], "SRAM2 parity check enable"),
        sram2_rst => (_sram2_rst, bool, bool, [25:25], "SRAM2 and PKA RAM erase when system reset"),
        n_swboot_0 => (_n_swboot_0, bool, bool, [26:26], "Software BOOT0 selection\n\n\
            - `false`: BOOT0 taken from the option bit nBOOT0\n\
            - `true`: BOOT0 taken from the PH3/BOOT0 pin
        "),
        n_boot_0 => (_n_boot_0, bool, bool, [27:27], "nBOOT0 option bit"),
        agc_trim => (_agc_trim, u8, u8, [31:29], "Radio automatic gain control trimming"),
    ]
}

config_reg_u32! {
    RW, Pcrop1aStrtR, Pcrop1aStrtW, FLASH, pcrop1asr, [
        pcrop1a_strt => (_pcrop1a_strt, u16, u16, [8:0], "PCROP1A area start offset\n\n\
            Unit: Half Page (2 KiB). Size: 9 Bit (0-511)
        "),
    ]
}

config_reg_u32! {
    RW, Pcrop1aEndR, Pcrop1aEndW, FLASH, pcrop1aer, [
        pcrop1a_end => (_pcrop1a_end, u16, u16, [8:0], "PCROP1A area end offset\n\n\
            Unit: Half Page (2 KiB). Size: 9 Bit (0-511)
        "),
        pcrop_rdp => (_pcrop_rdp, bool, bool, [31:31], "PCROP area preserved when RDP level decreased\n\n\
            This bit is set only\n\
            - `false`: PCROP area is not erased when the RDP level is decreased from Level 1 to Level 0\n\
            - `true`: PCROP area is erased when the RDP level is decreased from Level 1 to Level 0
        "),
    ]
}

config_reg_u32! {
    RW, Wrp1AR, Wrp1AW, FLASH, wrp1ar, [
        wrp1a_strt => (_wrp1a_strt, u8, u8, [7:0], "WRP first area 'A' start offset\n\n\
            Unit: Page (4 KiB). Size: 8 Bit (0-255)
        "),
        wrp1a_end => (_wrp1a_end, u8, u8, [23:16], "WRP first area 'A' end offset\n\n\
            Unit: Page (4 KiB). Size: 8 Bit (0-255)
        "),
    ]
}

config_reg_u32! {
    RW, Wrp1BR, Wrp1BW, FLASH, wrp1br, [
        wrp1b_strt => (_wrp1b_strt, u8, u8, [7:0], "WRP second area 'B' start offset\n\n\
            Unit: Page (4 KiB). Size: 8 Bit (0-255)
        "),
        wrp1b_end => (_wrp1b_end, u8, u8, [23:16], "WRP second area 'B' end offset\n\n\
            Unit: Page (4 KiB). Size: 8 Bit (0-255)
        "),
    ]
}

config_reg_u32! {
    RW, Pcrop1bStrtR, Pcrop1bStrtW, FLASH, pcrop1bsr, [
        pcrop1b_strt => (_pcrop1b_strt, u16, u16, [8:0], "PCROP1B area start offset\n\n\
            Unit: Half Page (2 KiB). Size: 9 Bit (0-511)
        "),
    ]
}

config_reg_u32! {
    RW, Pcrop1bEndR, Pcrop1bEndW, FLASH, pcrop1ber, [
        pcrop1b_end => (_pcrop1b_end, u16, u16, [8:0], "PCROP1B area end offset\n\n\
            Unit: Half Page (2 KiB). Size: 9 Bit (0-511)
        "),
    ]
}

config_reg_u32! {
    RW, SecureFlashOptionsR, SecureFlashOptionsW, FLASH, sfr, [
        sfsa => (_sfsa, u8, u8, [7:0], "Secure flash memory start address\n\n\
            Unit: Page (4 KiB). Size: 8 Bit (0-255)
        "),
        fsd => (_fsd, bool, bool, [8:8], "Flash memory security disabled\n\n\
            Start address given by SFSA
        "),
        dds => (_dds, bool, bool, [12:12], "Disable CPU2 debug access"),
    ]
}

config_reg_u32! {
    RW, IpccR, IpccW, FLASH, ipccbr, [
        ipccdba => (_ipccdba, u16, u16, [13:0], "IPCC mailbox data buffer base address offset\n\n\
            Contains the first double word offset of the IPCC mailbox data buffer area in SRAM2\n\
            - Unit: Double Word (8 Byte)\n\
            - Size: 14 Bit
        ")
    ]
}

config_reg_u32! {
    RW, SecureSRAM2OptionsR, SecureSRAM2OptionsW, FLASH, srrvr, [
        sbrv => (_sbrv, u32, u32, [17:0], "CPU2 boot reset vector\n\n\
            Contains the world aligned CPU2 boot reset start address offset within the selected \
            memory area by C2OPT
        "),
        sbrsa => (_sbrsa, u8, u8, [22:18], "Secure backup SRAM2a start address\n\n\
            SBRSA contains the start address of the first 1 KiB page of the secure backup SRAM2a area\n\
            - Size: 5 Bits (0-31)
        "),
        brsd => (_brsd, bool, bool, [23:23], "Backup SRAM2a security disable\n\n\
            - `false`: SRAM2a is secure. SBRSA contains the start address of the first 1 KiB page of the secure backup SRAM2a area\n\
            - `true`: SRAM2a is not secure
        "),
        snbrsa => (_snbrsa, u8, u8, [29:25], "Secure non-backup SRAM2b start address\n\n\
            SNBRSA contains the start address of the first 1 KiB page of the secure non-backup SRAM2b area
        "),
        nbrsd => (_nbrsd, bool, bool, [30:30], "Non-backup security disable\n\n\
            - `false`: SRAM2b is secure. SNBRSA contains the start address of the first 1 KiB page \
            of the secure non-backup SRAM2b area\n\
            - `true`: SRAM2b is not secure
        "),
        c2opt => (_c2opt, bool, bool, [31:31], "CPU2 boot reset vector memory selection\n\n\
            - `false`: SBRV offset addresses SRAM1 or SRAM2, from start address 0x2000_0000 \
            (SBRV value must be kept within the SRAM area)\n\
            - `true`: SBRV offset addresses Flash memory, from start address 0x0800_0000
        "),
    ]
}

/// Latency
///
/// Represents the ratio of the flash memory HCLK clock period to the flash memory access time
///
/// # Note
///
/// The chip has two power modes, selectable via power control (PWR):
/// - Range 1: High-performance range / overvolted, f <= 64 MHz
/// - Range 2: Low-power range / undervolted, f <= 16 MHz
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Latency {
    /// Zero wait states
    ///
    /// Apply, when:
    /// - Range 1: f <= 18 MHz
    /// - Range 2: f <= 6 MHz
    W0 = 0b000,
    /// One wait state
    ///
    /// Apply, when:
    /// - Range 1: f <= 36 MHz
    /// - Range 2: f <= 12 MHz
    W1 = 0b001,
    /// Two wait states
    ///
    /// Apply, when:
    /// - Range 1: f <= 54 MHz
    /// - Range 2: f <= 16 MHz
    W2 = 0b010,
    /// Three wait states
    ///
    /// Apply, when:
    /// - Range 1: f <= 64 MHz
    /// - Range 2: N/A
    W3 = 0b011,
}

impl Latency {
    pub fn from(vos: Vos, sysclk: u32) -> Self {
        match vos {
            Vos::Range1 => {
                if sysclk <= 18_000_000 {
                    Self::W0
                } else if sysclk <= 36_000_000 {
                    Self::W1
                } else if sysclk <= 54_000_000 {
                    Self::W2
                } else {
                    Self::W3
                }
            }
            Vos::Range2 => {
                if sysclk <= 6_000_000 {
                    Self::W0
                } else if sysclk <= 12_000_000 {
                    Self::W1
                } else {
                    Self::W2
                }
            }
        }
    }
}

/// Read Protection
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum RdpLevel {
    /// Level 0 - No Protection
    L0 = 0xAA,
    /// Level 1 - Read Protection
    #[default]
    L1 = 0x00,
    /// Level 2 - No Debug
    ///
    /// This setting offers maximum protection
    ///
    /// # WARNING
    ///
    /// - Once this value is set, it can't be unset anymore
    /// - The debug ports will be closed forever
    /// - The options register will be permanently frozen
    /// - Only a custom boot loader will be able to access the flash main memory
    /// - *This can't be undone.* If you need to disable RDP in the future,
    /// you need to physically replace the MCU. Not even ST can help you with that
    #[cfg(feature = "flash_rdp_l2")]
    L2 = 0xCC,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum BorResetLevel {
    /// Threshold ~ 1.7 V
    L0 = 0b000,
    /// Threshold ~ 2.0 V
    L1 = 0b001,
    /// Threshold ~ 2.2 V
    L2 = 0b010,
    /// Threshold ~ 2.5 V
    L3 = 0b011,
    /// Threshold ~ 2.8 V
    L4 = 0b100,
}

/// See RM0434 Rev 9 p. 81
fn unlock(flash: &FLASH) {
    /// See RM0434 Rev 9 p. 81
    const WRITE_KEY_1: u32 = 0x4567_0123;
    const WRITE_KEY_2: u32 = 0xCDEF_89AB;

    // SAFETY: Passing bits as documented in RM
    flash.keyr.write(|w| unsafe { w.keyr().bits(WRITE_KEY_1) });
    // SAFETY: Passing bits as documented in RM
    flash.keyr.write(|w| unsafe { w.keyr().bits(WRITE_KEY_2) });
}

/// See RM0434 Rev 9 p. 81
fn lock(flash: &FLASH) {
    flash.cr.modify(|_, w| w.lock().set_bit());
}

fn unlock_options(flash: &FLASH) {
    // See RM0434 Rev 9 p. 96
    const OPTIONS_KEY_1: u32 = 0x0819_2A3B;
    const OPTIONS_KEY_2: u32 = 0x4C5D_6E7F;

    flash.optkeyr.write(|w| w.optkeyr().variant(OPTIONS_KEY_1));
    flash.optkeyr.write(|w| w.optkeyr().variant(OPTIONS_KEY_2));
}

fn lock_options(flash: &FLASH) {
    flash.cr.modify(|_, w| w.optlock().set_bit());
}
