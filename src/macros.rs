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

macro_rules! get_u32 {
    ($uxx:ty, $reg:expr, $mask:expr, $offset:expr) => {
        crate::macros::R::<$uxx>::r(($reg >> $offset) & $mask)
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
