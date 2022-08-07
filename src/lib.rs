#![cfg_attr(not(test), no_std)]
#![feature(never_type)]
#![feature(associated_const_equality)]

#[macro_use]
mod macros;

pub mod cpu;
pub mod flash;
pub mod gpio;
pub mod i2c;
pub mod prelude;
pub mod pwr;
pub mod rcc;
pub mod signature;
pub mod time;

use stm32wb::stm32wb55 as pac;
