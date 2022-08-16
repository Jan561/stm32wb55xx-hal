use crate::signature::FlashSize;
use crate::time::Hertz;
use crate::{pac::FLASH, pwr::Vos};
use fugit::RateExtU32;
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

pub enum ConfigError {
    CacheEnabled,
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
            || c1_c2!(self.r.miserr().bit_is_set(), self.r.misserr().bit_is_set())
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

    /// Unlocks the Flash and returns a handle to the unlocked flash
    /// 
    /// The Flash is locked automatically after dropping the handle
    pub fn unlocked(&mut self) -> UnlockedFlash {
        if self.flash.cr.read().lock().bit_is_set() {
            unlock(&self.flash);
        }

        UnlockedFlash { flash: self, autolock: true }
    }

    /// Unlocks the Flash
    /// 
    /// Note: The caller is responsible for locking the flash manually afterwards
    pub fn unlock(&mut self) {
        unlock(&self.flash);
    }

    /// Returns a handle to the unlocked flash
    /// 
    /// The caller must ensure that the flash is indeed unlocked.
    /// The Flash is **not** locked automatically after dropping the handle
    /// 
    /// # Panic
    /// 
    /// Panics if the flash is locked
    pub fn as_unlocked(&mut self) -> UnlockedFlash {
        assert!(self.flash.cr.read().lock().bit_is_clear());

        UnlockedFlash { flash: self, autolock: false }
    }

    /// Locks the flash
    /// 
    /// Not necessary to call if the flash got unlocked with `unlocked`
    pub fn lock(&mut self) {
        lock(&self.flash);
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

    pub fn prefetch_enable(&mut self, en: bool) {
        let acr = &c1_c2!(self.flash.acr, self.flash.c2acr);

        acr.modify(|_, w| w.prften().bit(en));
    }

    pub fn instruction_cache_enable(&mut self, en: bool) {
        let acr = &c1_c2!(self.flash.acr, self.flash.c2acr);

        acr.modify(|_, w| w.icen().bit(en));
    }

    pub fn instruction_cache_reset(&mut self) -> Result<(), ConfigError> {
        let acr = &c1_c2!(self.flash.acr, self.flash.c2acr);

        if acr.read().icen().bit_is_set() {
            return Err(ConfigError::CacheEnabled);
        }

        acr.modify(|_, w| w.icrst().set_bit());
        acr.modify(|_, w| w.icrst().clear_bit());

        Ok(())
    }

    #[cfg(feature = "cm4")]
    pub fn data_cache_enable(&mut self, en: bool) {
        self.flash.acr.modify(|_, w| w.dcen().bit(en));
    }

    #[cfg(feature = "cm4")]
    pub fn data_cache_reset(&mut self) -> Result<(), ConfigError> {
        if self.flash.acr.read().dcen().bit_is_set() {
            return Err(ConfigError::CacheEnabled);
        }

        self.flash.acr.modify(|_, w| w.dcrst().set_bit());
        self.flash.acr.modify(|_, w| w.dcrst().clear_bit());

        Ok(())
    }

    pub fn suspend_programming_erase(&mut self, suspend: bool) {
        let acr = &c1_c2!(self.flash.acr, self.flash.c2acr);

        acr.modify(|_, w| w.pes().bit(suspend));
    }

    pub fn is_empty(&self) -> bool {
        self.flash.acr.read().empty().bit()
    }

    pub fn latency(&self) -> Latency {
        self.flash.acr.read().latency().bits().try_into().unwrap()
    }
}

pub struct UnlockedFlash<'a> {
    flash: &'a mut Flash,
    autolock: bool,
}

impl Drop for UnlockedFlash<'_> {
    fn drop(&mut self) {
        if self.autolock {
            lock(&self.flash.flash);
        }
    }
}

impl<'a> UnlockedFlash<'a> {
    fn reg(&self) -> &FLASH {
        &self.flash.flash
    }

    fn address(&self) -> usize {
        self.flash.address()
    }

    /// Erase a single page
    ///
    /// # Safety
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

    pub fn read_protection(&mut self, rdp: RdpLevel) {
        self.reg().optr.modify(|_, w| w.rdp().variant(rdp.into()));
    }

    pub fn system_security_enabled(&mut self, en: bool) {
        self.reg().optr.modify(|_, w| w.ese().bit(en));
    }

    pub fn bor_level(&mut self, level: BorResetLevel) {
        self.reg()
            .optr
            .modify(|_, w| w.bor_lev().variant(level.into()));
    }

    pub fn reset_on_stop(&mut self, rst: bool) {
        self.reg().optr.modify(|_, w| w.n_rst_stop().bit(!rst));
    }

    pub fn reset_on_standby(&mut self, rst: bool) {
        self.reg().optr.modify(|_, w| w.n_rst_stdby().bit(!rst));
    }

    pub fn reset_on_shutdown(&mut self, rst: bool) {
        self.reg().optr.modify(|_, w| w.n_rst_shdw().bit(!rst));
    }

    pub fn independent_watchdog(&mut self, wd: Watchdog) {
        self.reg()
            .optr
            .modify(|_, w| w.idwg_sw().bit(wd == Watchdog::Software));
    }

    pub fn independent_watchdog_counter_stop(&mut self, cnt: WatchdogCounter) {
        self.reg()
            .optr
            .modify(|_, w| w.iwdg_stop().bit(cnt == WatchdogCounter::Running));
    }

    pub fn independent_watchdog_counter_standby(&mut self, cnt: WatchdogCounter) {
        self.reg()
            .optr
            .modify(|_, w| w.iwdg_stdby().bit(cnt == WatchdogCounter::Running));
    }

    pub fn window_watchdog(&mut self, wd: Watchdog) {
        self.reg()
            .optr
            .modify(|_, w| w.wwdg_sw().bit(wd == Watchdog::Software));
    }

    pub fn boot_1(&mut self, boot: bool) {
        self.reg().optr.modify(|_, w| w.n_boot1().bit(!boot));
    }

    pub fn sram2_parity_check_enable(&mut self, en: bool) {
        self.reg().optr.modify(|_, w| w.sram2_pe().bit(!en));
    }

    pub fn sram2_erased_on_reset(&mut self, erased: bool) {
        self.reg().optr.modify(|_, w| w.sram2_rst().bit(!erased));
    }

    pub fn boot0(&mut self, boot: Boot0) {
        match boot {
            Boot0::Software(boot0) => self
                .reg()
                .optr
                .modify(|_, w| w.n_swboot0().clear_bit().n_boot0().bit(boot0)),
            Boot0::Pin => self.reg().optr.modify(|_, w| w.n_swboot0().set_bit()),
        }
    }

    pub fn agc_trim(&mut self, trim: u8) {
        assert!(trim < 8);

        self.reg().optr.modify(|_, w| w.agc_trim().variant(trim));
    }

    pub fn pcrop_erase_on_rdp_decrease(&mut self) {
        self.reg().pcrop1aer.modify(|_, w| w.pcrop_rdp().set_bit());
    }

    pub fn pcrop1a(&mut self, start_hp: HalfPage, end_hp: HalfPage) {
        self.reg()
            .pcrop1asr
            .modify(|_, w| w.pcrop1a_strt().variant(start_hp.into()));
        self.reg()
            .pcrop1aer
            .modify(|_, w| w.pcrop1a_end().variant(end_hp.into()));
    }

    pub fn pcrop1b(&mut self, start_hp: HalfPage, end_hp: HalfPage) {
        self.reg()
            .pcrop1bsr
            .modify(|_, w| w.pcrop1b_strt().variant(start_hp.into()));
        self.reg()
            .pcrop1ber
            .modify(|_, w| w.pcrop1b_end().variant(end_hp.into()));
    }

    pub fn wrp1a(&mut self, start_p: Page, end_p: Page) {
        self.reg().wrp1ar.modify(|_, w| {
            w.wrp1a_strt()
                .variant(start_p.into())
                .wrp1a_end()
                .variant(end_p.into())
        });
    }

    pub fn wrp1b(&mut self, start_p: Page, end_p: Page) {
        self.reg().wrp1br.modify(|_, w| {
            w.wrp1b_strt()
                .variant(start_p.into())
                .wrp1b_end()
                .variant(end_p.into())
        });
    }

    pub fn ipcc(&mut self, offset: usize) {
        assert!(offset < (1 << 14));

        self.reg()
            .ipccbr
            .modify(|_, w| w.ipccdba().variant(offset as u16));
    }

    #[cfg(feature = "cm0p")]
    pub fn flash_security_enable(&mut self, en: bool) {
        self.reg().sfr.modify(|_, w| w.fsd().bit(!en));
    }

    #[cfg(feature = "cm0p")]
    pub fn cpu2_debug_access_disabled(&mut self, disabled: bool) {
        self.reg().sfr.modify(|_, w| w.dds().bit(disabled));
    }

    #[cfg(feature = "cm0p")]
    pub fn secure_flash_start(&mut self, page: Page) {
        self.reg().sfr.modify(|_, w| w.sfsa().variant(page.into()));
    }

    #[cfg(feature = "cm0p")]
    pub fn cpu2_boot_reset_vector(&mut self, mem: Cpu2ResetMemory, offset: usize) {
        assert!(offset < (1 << 18));

        self.reg().srrvr.modify(|_, w| {
            w.c2opt()
                .bit(mem == Cpu2ResetMemory::Flash)
                .sbrv()
                .variant(offset as u32)
        });
    }

    #[cfg(feature = "cm0p")]
    pub fn secure_sram2a_start(&mut self, sram_page: SramPage) {
        self.reg()
            .srrvr
            .modify(|_, w| w.sbrsa().variant(sram_page.into()));
    }

    #[cfg(feature = "cm0p")]
    pub fn sram2a_security_disable(&mut self, disable: bool) {
        self.reg().srrvr.modify(|_, w| w.brsd().bit(disable));
    }

    #[cfg(feature = "cm0p")]
    pub fn secure_sram2b_start(&mut self, sram_page: SramPage) {
        self.reg()
            .srrvr
            .modify(|_, w| w.snbrsa().variant(sram_page.into()));
    }

    #[cfg(feature = "cm0p")]
    pub fn sram2b_security_disable(&mut self, disable: bool) {
        self.reg().srrvr.modify(|_, w| w.nbrsd().bit(disable));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cpu2ResetMemory {
    Sram,
    Flash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HalfPage(u16);

impl HalfPage {
    pub fn new(hp: u16) -> Self {
        assert!(hp < NUM_PAGES as u16 * 2);

        Self(hp)
    }
}

impl From<HalfPage> for u16 {
    fn from(hp: HalfPage) -> u16 {
        hp.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Page(u8);

impl Page {
    pub fn new(p: u8) -> Self {
        Self(p)
    }
}

impl From<Page> for u8 {
    fn from(p: Page) -> u8 {
        p.0
    }
}

pub struct SramPage(u8);

impl SramPage {
    pub fn new(p: u8) -> Self {
        assert!(p < 32);

        Self(p)
    }
}

impl From<SramPage> for u8 {
    fn from(p: SramPage) -> Self {
        p.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Boot0 {
    /// BOOT0 taken from the option bit nBOOT0
    Software(bool),
    /// BOOT0 taken from PH3/BOOT0 pin
    Pin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Watchdog {
    Hardware,
    Software,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogCounter {
    Frozen,
    Running,
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
    pub(crate) fn from(vos: Vos, hclk4: Hertz) -> Self {
        match vos {
            Vos::Range1 => {
                if hclk4 <= 18.MHz::<1, 1>() {
                    Self::W0
                } else if hclk4 <= 36.MHz::<1, 1>() {
                    Self::W1
                } else if hclk4 <= 54.MHz::<1, 1>() {
                    Self::W2
                } else {
                    Self::W3
                }
            }
            Vos::Range2 => {
                if hclk4 <= 6.MHz::<1, 1>() {
                    Self::W0
                } else if hclk4 <= 12.MHz::<1, 1>() {
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
