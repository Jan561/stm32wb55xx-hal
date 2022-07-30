#![no_std]
#![feature(never_type)]

#[macro_use]
mod macros;

pub mod cpu;
pub mod flash;
pub mod i2c;
pub mod prelude;
pub mod pwr;
pub mod rcc;
pub mod signature;

use stm32wb::stm32wb55 as pac;
