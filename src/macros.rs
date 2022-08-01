use core::marker::PhantomData;

#[cfg(feature = "cm4")]
macro_rules! c1_c2 {
    ($c1:expr, $c2:expr $(,)?) => {
        $c1
    };
}

#[cfg(feature = "cm0p")]
macro_rules! c1_c2 {
    ($c1:expr, $c2:expr $(,)?) => {
        $c2
    };
}

macro_rules! define_ptr_type {
    ($name:ident, $ptr:expr) => {
        impl $name {
            pub fn ptr() -> *const Self {
                $ptr as *const _
            }

            pub fn get() -> &'static Self {
                // SAFETY: Pointer must be valid
                unsafe { &*Self::ptr() }
            }
        }
    };
}

macro_rules! mask_u32 {
    ($mask:ident, $offset:ident, [$hi:tt : $lo:tt]) => {
        const $mask: u32 = 0xFFFF_FFFF >> (31 - ($hi - $lo));
        const $offset: u32 = $lo;
    };
}

macro_rules! set_u32 {
    ($reg:expr, $x:expr, $mask:expr, $offset:expr) => {
        $reg &= !($mask << $offset);
        $reg |= ($x & $mask) << $offset
    };
}

macro_rules! get_u32 {
    ($uxx:ty, $reg:expr, $mask:expr, $offset:expr) => {
        crate::macros::R::<$uxx>::r(($reg >> $offset) & $mask)
    };
}

macro_rules! config_reg_u32 {
    (R, $ident_r:ident, $per:ty, $reg:ident, [$($field:ident => ($ty:ty, $ux:ty, [$hi:tt : $lo:tt], $doc:tt)),* $(,)?]) => {
        pub struct $ident_r(u32);

        impl $ident_r {
            #[allow(unused)]
            fn read_from(per: &<$per as core::ops::Deref>::Target) -> Self {
                let bits = per.$reg.read().bits();
                Self(bits)
            }

            $(
                #[doc=$doc]
                pub fn $field(&self) -> $ty {
                    mask_u32!(MASK, OFFSET, [$hi:$lo]);

                    let val = get_u32!($ux, self.0, MASK, OFFSET);

                    val.try_into().unwrap()
                }
            )*
        }
    };
    (W, $ident_w:ident, $per:ty, $reg:ident, [$($field:ident => ($getter:ident, $ty:ty, $ux:ty, [$hi:tt : $lo:tt], $doc:tt)),* $(,)?]) => {
        pub struct $ident_w(u32);

        impl $ident_w {
            #[allow(unused)]
            fn read_from(per: &<$per as core::ops::Deref>::Target) -> Self {
                let bits = per.$reg.read().bits();
                Self(bits)
            }

            $(
                #[doc=$doc]
                pub fn $field(&mut self, val: $ty) -> &mut Self {
                    mask_u32!(MASK, OFFSET, [$hi:$lo]);

                    let x = u32::from(<$ux>::from(val));

                    set_u32!(self.0, x, MASK, OFFSET);

                    self
                }

                fn $getter(&self) -> $ux {
                    mask_u32!(MASK, OFFSET, [$hi:$lo]);

                    get_u32!($ux, self.0, MASK, OFFSET)
                }
            )*
        }
    };
    (RW, $ident_r:ident, $ident_w:ident, $per:ty, $reg:ident, [$($field:ident => ($getter:ident, $ty:ty, $ux:ty, [$hi:tt : $lo:tt], $doc:tt)),* $(,)?]) => {
        config_reg_u32!(R, $ident_r, $per, $reg, [$($field => ($ty, $ux, [$hi:$lo], $doc)),*]);
        config_reg_u32!(W, $ident_w, $per, $reg, [$($field => ($getter, $ty, $ux, [$hi:$lo], $doc)),*]);
    };
}

pub struct R<UXX> {
    _uxx: PhantomData<UXX>,
}

impl R<bool> {
    #[inline(always)]
    #[allow(unused)]
    pub fn r(val: u32) -> bool {
        val != 0
    }
}

impl R<u8> {
    #[inline(always)]
    #[allow(unused)]
    pub fn r(val: u32) -> u8 {
        val as u8
    }
}

impl R<u16> {
    #[inline(always)]
    #[allow(unused)]
    pub fn r(val: u32) -> u16 {
        val as u16
    }
}

impl R<u32> {
    #[inline(always)]
    #[allow(unused)]
    pub fn r(val: u32) -> u32 {
        val
    }
}

macro_rules! clear_status_reg_u32 {
    ($ident:ident, [
        $($field:ident => ($bit:tt, $doc:tt)),* $(,)?
    ]) => {
        pub struct $ident(u32);

        impl $ident {
            pub fn new() -> Self {
                Self(0)
            }

            $(
                #[doc=$doc]
                pub fn $field(&mut self) -> &mut Self {
                    mask_u32!(MASK, OFFSET, [$bit:$bit]);

                    set_u32!(self.0, 1, MASK, OFFSET);

                    self
                }
            )*
        }
    };
}
