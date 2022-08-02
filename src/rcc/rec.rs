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
                        fn enr(&self) -> &crate::pac::rcc::[<$AXBn:upper ENR $($n)?>] {
                            let rcc = unsafe { &*RCC::PTR };
                            &rcc.[<$AXBn:lower enr $($n)?>]
                        }

                        fn rst(&self) -> &crate::pac::rcc::[<$AXBn:upper RSTR $($n)?>] {
                            let rcc = unsafe { &*RCC::PTR };
                            &rcc.[<$AXBn:lower rstr $($n)?>]
                        }

                        fn smenr(&self) -> &crate::pac::rcc::[<$AXBn:upper SMENR $($n)?>] {
                            let rcc = unsafe { &*RCC::PTR };
                            &rcc.[<$AXBn:lower smenr $($n)?>]
                        }
                    }
                )?

                $(
                    impl $bus {
                        $(
                            fn enr(&self) -> &crate::pac::rcc::$AXBnENR {
                                let rcc = unsafe { &*RCC::PTR };
                                &rcc.[<$AXBnENR:lower>]
                            }
                        )?

                        $(
                            fn rst(&self) -> &crate::pac::rcc::$AXBnRSTR {
                                let rcc = unsafe { &*RCC::PTR };
                                &rcc.[<$AXBnRSTR:lower>]
                            }
                        )?

                        $(
                            fn smenr(&self) -> &crate::pac::rcc::$AXBnSMENR {
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
    APB3 => [E C2APB3ENR R APB3RSTR S C2APB3SMENR]
}

macro_rules! peripheral_reset_and_enable_control_register {
    ($AXBn:ident, $p:ident) => {
        pub struct $p {
            _marker: PhantomData<*const ()>,
        }
    };
}
