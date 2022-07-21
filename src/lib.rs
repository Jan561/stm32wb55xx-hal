#![no_std]

#[macro_use]
mod macros;
pub mod cpu;
pub mod flash;
pub mod signature;

use stm32wb::stm32wb55 as pac;
