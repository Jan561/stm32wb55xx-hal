pub trait SetAlternate<const A: u8, OType> {
    fn set_alt_mode(&mut self);
    fn restore_mode(&mut self);
}

pub trait PinA<PIN, PER> {
    type A;
}

macro_rules! pin {
    ($(<$PIN:ty, $PER:ty> for [$($PX:ident<$A:literal>),*]),*) => {};
}
