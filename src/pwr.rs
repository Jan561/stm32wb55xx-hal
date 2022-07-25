use num_enum::{IntoPrimitive, TryFromPrimitive};

#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Vos {
    /// High performance Range 1 (1.2V)
    Range1 = 0b01,
    /// Low power Range 2 (1.0V)
    Range2 = 0b10,
}
