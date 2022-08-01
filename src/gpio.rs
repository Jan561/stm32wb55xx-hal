//! GPIO

// Type States

use core::marker::PhantomData;

use num_enum::{IntoPrimitive, TryFromPrimitive};

pub struct Alternate<const A: u8>;

pub struct OpenDrain;
pub struct PushPull;

/// Analog pin (type state)
pub struct Analog;

pub struct Input;
pub struct Output<MODE = PushPull> {
    _mode: PhantomData<MODE>,
}

/// Pull setting for an input
#[derive(Debug, Clone, Copy, PartialEq, Eq, TryFromPrimitive, IntoPrimitive)]
#[repr(u8)]
pub enum Pull {
    Floating = 0b00,
    Up = 0b01,
    Down = 0b10,
}

pub struct Pin<const P: char, const N: u8, MODE = Analog> {
    _mode: PhantomData<MODE>,
}
