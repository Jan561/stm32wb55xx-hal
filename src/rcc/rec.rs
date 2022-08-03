//! Reset / Enable Control

use crate::pac::RCC;
use core::marker::PhantomData;
use paste::paste;

macro_rules! bus {
    ($($(#[ $meta:meta ])? $bus:ident =>
        $([$(E $AXBnENR:ident)? $(R $AXBnRSTR:ident)? $(S $AXBnSMENR:ident)?])?
        $([A $AXBn:ident $($n:literal)?])?
    ),*) => {
        paste! {
            $(
                $(#[$meta])?
                pub struct $bus {
                    _marker: PhantomData<*const ()>,
                }

                $(#[$meta])?
                $(
                    impl $bus {
                        fn enr() -> &'static crate::pac::rcc::[<$AXBn:upper ENR $($n)?>] {
                            let rcc = unsafe { &*RCC::PTR };
                            &rcc.[<$AXBn:lower enr $($n)?>]
                        }

                        fn rst() -> &'static crate::pac::rcc::[<$AXBn:upper RSTR $($n)?>] {
                            let rcc = unsafe { &*RCC::PTR };
                            &rcc.[<$AXBn:lower rstr $($n)?>]
                        }

                        fn smenr() -> &'static crate::pac::rcc::[<$AXBn:upper SMENR $($n)?>] {
                            let rcc = unsafe { &*RCC::PTR };
                            &rcc.[<$AXBn:lower smenr $($n)?>]
                        }
                    }
                )?

                $(
                    impl $bus {
                        $(
                            fn enr() -> &'static crate::pac::rcc::$AXBnENR {
                                let rcc = unsafe { &*RCC::PTR };
                                &rcc.[<$AXBnENR:lower>]
                            }
                        )?

                        $(
                            fn rst() -> &'static crate::pac::rcc::$AXBnRSTR {
                                let rcc = unsafe { &*RCC::PTR };
                                &rcc.[<$AXBnRSTR:lower>]
                            }
                        )?

                        $(
                            fn smenr() -> &'static crate::pac::rcc::$AXBnSMENR {
                                let rcc = unsafe { &*RCC::PTR };
                                &rcc.[<$AXBnSMENR:lower>]
                            }
                        )?
                    }
                )?
            )*
        }
    };
}

bus! {
    #[cfg(feature = "cm4")]
    AHB1 => [A AHB1],
    #[cfg(feature = "cm0p")]
    AHB1 => [E C2AHB1ENR R AHB1RSTR S C2AHB1SMENR],
    #[cfg(feature = "cm4")]
    AHB2 => [A AHB2],
    #[cfg(feature = "cm0p")]
    AHB2 => [E C2AHB2ENR R AHB2RSTR S C2AHB2SMENR],
    #[cfg(feature = "cm4")]
    AHB3 => [A AHB3],
    #[cfg(feature = "cm0p")]
    AHB3 => [E C2AHB3ENR R AHB3RSTR S C2AHB3SMENR],
    #[cfg(feature = "cm4")]
    APB1_1 => [A APB1 1],
    #[cfg(feature = "cm0p")]
    APB1_1 => [E C2APB1ENR1 R APB1RSTR1 S C2APB1SMENR1],
    #[cfg(feature = "cm4")]
    APB1_2 => [A APB1 2],
    #[cfg(feature = "cm0p")]
    APB1_2 => [E C2APB1ENR2 R APB1RSTR2 S C2APB1SMENR2],
    #[cfg(feature = "cm4")]
    APB2 => [A APB2],
    #[cfg(feature = "cm0p")]
    APB2 => [E C2APB2ENR R APB2RSTR S C2APB2SMENR],
    #[cfg(feature = "cm4")]
    APB3 => [R APB3RSTR],
    #[cfg(feature = "cm0p")]
    APB3 => [E C2APB3ENR R APB3RSTR S C2APB3SMENR],
    #[cfg(feature = "cm0p")]
    APB3_SHARED => [R APB3RSTR]
}

macro_rules! enable {
    ($p:ident => ($AXBn:ident, $f:ident)) => {
        impl $p {
            pub fn enable() {
                let r = $AXBn::enr();
                r.modify(|_, w| w.$f().set_bit());
            }

            pub fn disable() {
                let r = $AXBn::enr();
                r.modify(|_, w| w.$f().clear_bit());
            }
        }
    };
}

macro_rules! sm_enable {
    ($p:ident => ($AXBn:ident, $f:ident)) => {
        impl $p {
            pub fn sm_enable() {
                let r = $AXBn::smenr();
                r.modify(|_, w| w.$f().set_bit());
            }

            pub fn sm_disable() {
                let r = $AXBn::smenr();
                r.modify(|_, w| w.$f().clear_bit());
            }
        }
    };
}

macro_rules! reset {
    ($p:ident => ($AXBn:ident, $f:ident)) => {
        impl $p {
            pub fn reset() {
                let r = $AXBn::rst();
                r.modify(|_, w| w.$f().set_bit());
                r.modify(|_, w| w.$f().clear_bit());
            }
        }
    };
}

macro_rules! rec {
    ($($(#[$meta:meta])? $p:ident => $AXBn:ident),* $(,)?) => {
        paste! {
            $(
                $(#[$meta])?
                pub struct $p;

                $(#[$meta])?
                enable!($p => ($AXBn, [<$p:lower en>]));

                $(#[$meta])?
                sm_enable!($p => ($AXBn, [<$p:lower smen>]));

                $(#[$meta])?
                reset!($p => ($AXBn, [<$p:lower rst>]));
            )*
        }
    };
}

rec! {
    DMA1 => AHB1,
    DMA2 => AHB1,
    // DMAMUX1 => AHB1,
    // SRAM1 => AHB1
    CRC => AHB1,
    TSC => AHB1,
    GPIOA => AHB2,
    GPIOB => AHB2,
    GPIOC => AHB2,
    GPIOD => AHB2,
    GPIOE => AHB2,
    GPIOH => AHB2,
    // ADC => AHB2,
    AES1 => AHB2,
    #[cfg(feature = "cm4")]
    QSPI => AHB3,
    PKA => AHB3,
    AES2 => AHB3,
    RNG => AHB3,
    // HSEM => AHB3,
    // IPCC => AHB3,
    FLASH => AHB3,
    TIM2 => APB1_1,
    LCD => APB1_1,
    SPI2 => APB1_1,
    I2C1 => APB1_1,
    I2C3 => APB1_1,
    // CRS => APB1_1,
    // USB => APB1_1,
    LPTIM1 => APB1_1,
    LPUART1 => APB1_2,
    LPTIM2 => APB1_2,
    TIM1 => APB2,
    SPI1 => APB2,
    USART1 => APB2,
    TIM16 => APB2,
    TIM17 => APB2,
    SAI1 => APB2,
    // RF => APB3,
    // #[cfg(feature = "cm0p")]
    // BLE => APB3,
}

pub struct DMAMUX1;
enable!(DMAMUX1 => (AHB1, dmamuxen));
sm_enable!(DMAMUX1 => (AHB1, dmamuxsmen));
reset!(DMAMUX1 => (AHB1, dmamuxrst));

pub struct SRAM1;
sm_enable!(SRAM1 => (AHB1, sram1smen));

pub struct ADC;
enable!(ADC => (AHB2, adcen));
sm_enable!(ADC => (AHB2, adcfssmen));
reset!(ADC => (AHB2, adcrst));

pub struct SRAM2;
sm_enable!(SRAM2 => (AHB3, sram2smen));

pub struct HSEM;
enable!(HSEM => (AHB3, hsemen));
reset!(HSEM => (AHB3, hsemrst));

pub struct IPCC;
enable!(IPCC => (AHB3, ipccen));
reset!(IPCC => (AHB3, ipccrst));

pub struct RTCAPB;
enable!(RTCAPB => (APB1_1, rtcapben));
sm_enable!(RTCAPB => (APB1_1, rtcapbsmen));

#[cfg(feature = "cm4")]
pub struct WWDG;
#[cfg(feature = "cm4")]
enable!(WWDG => (APB1_1, wwdgen));
#[cfg(feature = "cm4")]
sm_enable!(WWDG => (APB1_1, wwdgsmen));

pub struct CRS;
enable!(CRS => (APB1_1, crsen));
sm_enable!(CRS => (APB1_1, crsmen));
reset!(CRS => (APB1_1, crsrst));

pub struct USB;
enable!(USB => (APB1_1, usben));
sm_enable!(USB => (APB1_1, usbsmen));
reset!(USB => (APB1_1, usbfsrst));

#[cfg(feature = "cm0p")]
pub struct BLE;
#[cfg(feature = "cm0p")]
enable!(BLE => (APB3, bleen));
#[cfg(feature = "cm0p")]
sm_enable!(BLE => (APB3, blesmen));

#[cfg(feature = "cm0p")]
pub struct _802;
#[cfg(feature = "cm0p")]
enable!(_802 => (APB3, en802));
#[cfg(feature = "cm0p")]
sm_enable!(_802 => (APB3, smen802));

pub struct RF;
#[cfg(feature = "cm4")]
reset!(RF => (APB3, rfrst));
#[cfg(feature = "cm0p")]
reset!(RF => (APB3_SHARED, rfrst));
