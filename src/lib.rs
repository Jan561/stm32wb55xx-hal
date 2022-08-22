#![no_std]
#![feature(never_type)]
#![feature(associated_const_equality)]

#[macro_use]
mod macros;

pub mod cpu;
// pub mod delay;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod ipcc;
pub mod prelude;
pub mod pwr;
pub mod rcc;
pub mod signature;
pub mod time;
pub mod tl_mbox;

pub use stm32wb::stm32wb55 as pac;
