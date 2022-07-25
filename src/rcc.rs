use crate::pac::RCC;
use crate::pwr::Vos;
use num_enum::{FromPrimitive, IntoPrimitive, TryFromPrimitive};
use sealed::sealed;

pub trait RccExt {
    fn constrain(self) -> Rcc;
}

impl RccExt for RCC {
    fn constrain(self) -> Rcc {
        Rcc {
            w: CfgrW::read_from(&self),
            rcc: self,
        }
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

pub struct Rcc {
    rcc: RCC,
    w: CfgrW,
}

impl Rcc {
    pub fn cfg<F>(&mut self, op: F)
    where
        F: for<'w> FnOnce(&CfgrR, &'w mut CfgrW) -> &'w mut CfgrW,
    {
        let r = CfgrR::read_from(&self.rcc);

        op(&r, &mut self.w);
    }

    pub fn freeze(self) -> Clocks {
        self.rcc.cfgr.modify(|_, w| {
            w.sw()
                .variant(self.w._sw())
                .hpre()
                .variant(self.w._hpre())
                .ppre1()
                .variant(self.w._ppre1())
                .ppre2()
                .variant(self.w._ppre2())
                .stopwuck()
                .bit(self.w._stopwuck())
                .mcosel()
                .variant(self.w._mcosel())
                .mcopre()
                .variant(self.w._mcopre())
        });

        unimplemented!()
    }
}

pub struct Clocks {}

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
