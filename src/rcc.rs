use crate::pwr::Vos;
use crate::{flash::Flash, pac::RCC};
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};
use sealed::sealed;

#[derive(Debug)]
pub struct ValueError(&'static str);

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
    fn current_hertz(&self) -> u32;
}

pub struct Rcc {
    rcc: RCC,
}

impl Rcc {
    /// # Arguments
    ///
    /// - `op`: Closure with 2 arguments r and w
    /// - `flash`: Needed when increasing or decreasing CPU frequency.
    /// If not provided in this case, the method will panic
    pub fn cfg<F>(&mut self, op: F, flash: Option<&Flash>)
    where
        F: for<'w> FnOnce(&CfgrR, &'w mut CfgrW) -> &'w mut CfgrW,
    {
        let cfgr_r = CfgrR::read_from(&self.rcc);
        let mut cfgr_w = CfgrW(cfgr_r.0);

        op(&cfgr_r, &mut cfgr_w);

        if cfgr_r.0 == cfgr_w.0 {
            return;
        }

        let cr_r = CrR::read_from(&self.rcc);
        let pllcfgr = PllCfgrR::read_from(&self.rcc);

        let current_sysclk = Self::sysclk_hertz(&cfgr_r, &cr_r, &pllcfgr);
        let new_sysclk = Self::sysclk_hertz(&CfgrR(cfgr_w.0), &cr_r, &pllcfgr);

        if current_sysclk > new_sysclk {
            // Decrease CPU frequency
        } else if current_sysclk < new_sysclk {
            // Increase CPU frequency
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
    }

    pub fn current_sysclk_hertz(&self) -> u32 {
        Self::sysclk_hertz(&self.cfg_read(), &self.cr_read(), &self.pllcfgr_read())
    }

    fn sysclk_hertz(cfgr_r: &CfgrR, cr_r: &CrR, pllcfgr: &PllCfgrR) -> u32 {
        match cfgr_r.sws() {
            SysclkSwitch::Msi => Self::msi_hertz(cr_r),
            SysclkSwitch::Hsi16 => hsi16_hertz(),
            SysclkSwitch::Hse => {
                if cr_r.hsepre() {
                    hse_hertz() / 2
                } else {
                    hse_hertz()
                }
            }
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

    pub fn cfg_read(&self) -> CfgrR {
        CfgrR::read_from(&self.rcc)
    }

    pub fn cr_read(&self) -> CrR {
        CrR::read_from(&self.rcc)
    }

    pub fn pllcfgr_read(&self) -> PllCfgrR {
        PllCfgrR::read_from(&self.rcc)
    }

    fn msi_hertz(cr_r: &CrR) -> u32 {
        cr_r.msirange().hertz()
    }
}

impl Sysclk for Rcc {
    fn current_hertz(&self) -> u32 {
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
    /// Main PLL division factor for PLLCLK
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

/// Main PLL division factor for PLLQCLK and PLLRCLK
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
