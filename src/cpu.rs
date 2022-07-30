use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Cpu {
    /// Normal, non-secure CPU (Cortex-M4)
    C1,
    /// Secure CPU (Cortex-M0+)
    C2,
}

impl Cpu {
    // See PM0223 Rev 5 p. 88 and PM0214 Rev 10 p. 224
    const PTR: *const u32 = 0xE000_ED00 as *const _;
    const M0P: u16 = 0xC60;
    const M4: u16 = 0xC24;

    pub const CPU: Self = c1_c2!(Self::C1, Self::C2);

    pub fn from_device() -> Self {
        mask_u32!(MASK, OFFSET, [15:4]);

        // SAFETY: See reference of PTR
        let reg = unsafe { core::ptr::read_volatile(Self::PTR) };
        let val = get_u32!(u16, reg, MASK, OFFSET);

        if val == Self::M0P {
            Self::C2
        } else if val == Self::M4 {
            Self::C1
        } else {
            unreachable!();
        }
    }
}
