use crate::i2c;

pub trait PinA<PIN, PER> {
    const A: u8;
}

macro_rules! pin {
    ($(<$PIN:ty, $PER:ident> for [$($PX:ident<$A:literal>),*]),* $(,)?) => {
        $(
            $(
                impl PinA<$PIN, crate::pac::$PER> for crate::gpio::$PX {
                    const A: u8 = $A;
                }
            )*
        )*
    };
}

// I2C

pin! {
    <i2c::Smba, I2C1> for [PA1<4>, PA14<4>, PB5<4>],
    <i2c::Scl, I2C1> for [PA9<4>, PB6<4>, PB8<4>],
    <i2c::Sda, I2C1> for [PA10<4>, PB7<4>, PB9<4>],
    <i2c::Smba, I2C3> for [PB2<4>, PB12<4>],
    <i2c::Scl, I2C3> for [PA7<4>, PB10<4>, PB13<4>, PC0<4>],
    <i2c::Sda, I2C3> for [PB4<4>, PB11<4>, PB14<4>, PC1<4>],
}
