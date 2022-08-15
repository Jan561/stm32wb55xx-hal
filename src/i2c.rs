use crate::pac::{I2C1, I2C3};
use crate::rcc::{rec, Clocks, TrustedClocks};
use crate::time::Hertz;
use core::cmp::max;
use core::marker::PhantomData;
use embedded_hal::i2c::blocking::Operation;
use embedded_hal::i2c::{NoAcknowledgeSource, SevenBitAddress, TenBitAddress};
use fugit::RateExtU32;
use paste::paste;
use sealed::sealed;

#[sealed]
pub trait Pins<I2C> {}

#[sealed]
pub trait SclPin<I2C> {}

#[sealed]
pub trait SdaPin<I2C> {}

#[sealed]
pub trait SmbaPin<I2C> {}

#[sealed]
impl<I2C, SCL, SDA> Pins<I2C> for (SCL, SDA)
where
    SCL: SclPin<I2C>,
    SDA: SdaPin<I2C>,
{
}

const SEVEN_BIT_ADDR_MODE: bool = false;
const TEN_BIT_ADDR_MODE: bool = true;
const RD_WRN_WRITE: bool = false;
const RD_WRN_READ: bool = true;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Start {
    Start,
    Restart,
    Reload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stop {
    Software,
    Reload,
    Automatic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    Bus,
    Arbitration,
    NoAcknowledge(NoAcknowledgeSource),
}

impl embedded_hal::i2c::Error for Error {
    fn kind(&self) -> embedded_hal::i2c::ErrorKind {
        match *self {
            Error::Bus => embedded_hal::i2c::ErrorKind::Bus,
            Error::Arbitration => embedded_hal::i2c::ErrorKind::ArbitrationLoss,
            Error::NoAcknowledge(nack) => embedded_hal::i2c::ErrorKind::NoAcknowledge(nack),
        }
    }
}

#[derive(Debug)]
pub enum ConfigError {
    I2cClockDisabled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Event {
    Transmit,
    Receive,
    AddressMatch,
    NotAcknowledge,
    Stop,
    TransferComplete,
    Errors,
}

pub trait I2cExt: Sized {
    fn i2c<'a, PINS>(
        self,
        pins: PINS,
        clocks: impl Clocks + TrustedClocks<'a>,
        frequency: Hertz,
    ) -> Result<I2c<'a, Self, PINS>, ConfigError>
    where
        PINS: Pins<Self>;
}

pub struct I2c<'a, I2C, PINS> {
    _phantom: PhantomData<&'a ()>,
    i2c: I2C,
    pins: PINS,
}

pub type I2c1<'a, PINS> = I2c<'a, I2C1, PINS>;
pub type I2c3<'a, PINS> = I2c<'a, I2C3, PINS>;

impl<I2C, PINS> I2c<'_, I2C, PINS> {
    fn timings(i2cclk: Hertz, frequency: Hertz) -> [u8; 5] {
        let ratio = (i2cclk + frequency - 1.Hz()) / frequency;

        // 8192 = 16 * (256 + 256) is the highest scale factor we can achieve
        assert!(ratio <= 8192);

        macro_rules! presc_reg {
            ($($ratio:expr, $ticks:expr);*) => {
                [$(
                    if $ratio != 0 {
                        (($ratio - 1) / $ticks) as u8
                    } else {
                        0
                    }
                ),*].into_iter().max().unwrap()
            };
        }

        macro_rules! timing {
            ($min:expr, $tf:expr, $tr:expr, $su:expr, $ticks:expr, $l_weight:expr, $h_weight:expr, $scll_min:expr, $sclh_min:expr) => {{
                assert!(i2cclk >= $min.MHz::<1, 1>());

                // t_sync1 and t_sync2 insert an additional delay of > 2 additional i2cclk cycles and > 50 ns for AF each
                // To account for the first, we subtract 2*2, to account for the second, we subtract 2*50ns*i2cclk
                let scl_ratio = ratio - 4 - i2cclk.to_kHz() / 10_000;

                let scll_min_ratio = (i2cclk.to_kHz() * $scll_min + 999_999) / 1_000_000;
                let sclh_min_ratio = (i2cclk.to_kHz() * $sclh_min + 999_999) / 1_000_000;

                let sdadel_ratio = ($tf - 50u32)
                    .checked_sub(3_000_000 / i2cclk.to_kHz())
                    .map(|x| (x * i2cclk.to_kHz() + 999_999) / 1_000_000)
                    .unwrap_or(0);

                let scldel_ratio = (i2cclk.to_kHz() * ($tr + $su) + 999_999) / 1_000_000;

                let presc_reg = presc_reg!(scl_ratio, $ticks; scldel_ratio, 16; sdadel_ratio, 15);
                let presc = (presc_reg + 1) as u32;

                let scll = ((scl_ratio * $l_weight - 1) / (presc * ($l_weight + $h_weight))) as u8;
                let scll = max(scll, ((scll_min_ratio - 1) / presc) as u8);

                // Maybe we should do a checked subtraction here but tests are ok
                let sclh = ((scl_ratio - presc - 1) / presc - scll as u32) as u8;
                let sclh = max(sclh, ((sclh_min_ratio - 1) / presc) as u8);

                let sdadel = ((sdadel_ratio + presc - 1) / presc) as u8;
                let scldel = ((scldel_ratio - 1) / presc) as u8;

                [presc_reg, scll, sclh, sdadel, scldel]
            }};
        }

        if frequency > 400.kHz::<1, 1>() {
            timing!(19, 120, 120, 50, 384, 2, 1, 500, 260)
        } else if frequency > 100.kHz::<1, 1>() {
            timing!(9, 300, 300, 100, 384, 2, 1, 1300, 600)
        } else {
            timing!(2, 300, 1000, 250, 512, 1, 1, 4700, 4000)
        }
    }
}

impl<I2C, PINS> embedded_hal::i2c::ErrorType for I2c<'_, I2C, PINS> {
    type Error = Error;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Address {
    SevenBit(SevenBitAddress),
    TenBit(TenBitAddress),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NextOp {
    Start,
    Restart,
    Reload,
    Stop,
}

impl NextOp {
    fn start(self) -> Start {
        match self {
            Self::Start => Start::Start,
            Self::Restart => Start::Restart,
            Self::Reload => Start::Reload,
            Self::Stop => unreachable!(),
        }
    }

    fn stop(self) -> Stop {
        match self {
            Self::Start => unreachable!(),
            Self::Restart => Stop::Software,
            Self::Reload => Stop::Reload,
            Self::Stop => Stop::Automatic,
        }
    }
}

macro_rules! next_op {
    ($op:expr, $next:expr) => {{
        if $next.is_none() {
            NextOp::Stop
        } else if matches!($op, Operation::Read(_)) == matches!($next, Some(Operation::Read(_))) {
            NextOp::Reload
        } else {
            NextOp::Restart
        }
    }};
}

macro_rules! flush_txdr {
    ($i2c:expr) => {{
        if $i2c.isr.read().txis().bit() {
            $i2c.txdr.write(|w| w.txdata().variant(0x0));
        }

        if !$i2c.isr.read().txe().bit() {
            $i2c.isr.write(|w| w.txe().set_bit());
        }
    }};
}

macro_rules! busy_wait {
    ($i2c:expr, $flag:ident, $variant:ident, $nack:expr) => {{
        let isr = $i2c.isr.read();

        while !isr.$flag().$variant() {
            if isr.berr().bit() {
                $i2c.icr.write(|w| w.berrcf().set_bit());
                return Err(Error::Bus);
            } else if isr.arlo().bit() {
                $i2c.icr.write(|w| w.arlocf().set_bit());
                return Err(Error::Arbitration);
            } else if isr.nackf().bit() {
                $i2c.icr.write(|w| w.nackcf().set_bit().stopcf().set_bit());
                flush_txdr!($i2c);
                return Err(Error::NoAcknowledge($nack));
            }
        }
    }};
}

macro_rules! i2c {
    ($($I2Cx:ident),* $(,)?) => {
        paste! {
            $(
                impl<'a, PINS> I2c<'a, $I2Cx, PINS> {
                    pub fn new(i2c: $I2Cx, pins: PINS, clocks: impl Clocks + TrustedClocks<'a>, frequency: Hertz) -> Result<Self, ConfigError>
                    where
                        PINS: Pins<$I2Cx>,
                    {
                        if frequency > 1.MHz::<1, 1>() {
                            panic!("Maximum allowed frequency is 1 MHz");
                        }

                        if 4 * clocks.pclk1() < 3 * frequency {
                            panic!("PCLK1 frequency must be at least 3/4 of SCL frequency");
                        }

                        rec::$I2Cx::enable();
                        rec::$I2Cx::reset();

                        let i2cclk = match clocks.[<$I2Cx:lower _clk>]() {
                            Some(x) => x,
                            None => return Err(ConfigError::I2cClockDisabled),
                        };

                        let [presc, scll, sclh, sdadel, scldel] = Self::timings(i2cclk, frequency);

                        i2c.timingr.modify(|_, w| {
                            w.presc()
                                .variant(presc)
                                .scll()
                                .variant(scll)
                                .sclh()
                                .variant(sclh)
                                .sdadel()
                                .variant(sdadel)
                                .scldel()
                                .variant(scldel)
                        });

                        i2c.cr1.modify(|_, w| {
                            w.anfoff().clear_bit().pe().set_bit()
                        });

                        Ok(Self {
                            _phantom: PhantomData,
                            i2c,
                            pins,
                        })
                    }

                    pub fn listen(&mut self, event: Event) {
                        match event {
                            Event::Transmit => self.i2c.cr1.modify(|_, w| w.txie().set_bit()),
                            Event::Receive => self.i2c.cr1.modify(|_, w| w.rxie().set_bit()),
                            Event::AddressMatch => self.i2c.cr1.modify(|_, w| w.addrie().set_bit()),
                            Event::NotAcknowledge => self.i2c.cr1.modify(|_, w| w.nackie().set_bit()),
                            Event::Stop => self.i2c.cr1.modify(|_, w| w.stopie().set_bit()),
                            Event::TransferComplete => self.i2c.cr1.modify(|_, w| w.tcie().set_bit()),
                            Event::Errors => self.i2c.cr1.modify(|_, w| w.errie().set_bit()),
                        }
                    }

                    pub fn unlisten(&mut self, event: Event) {
                        match event {
                            Event::Transmit => self.i2c.cr1.modify(|_, w| w.txie().clear_bit()),
                            Event::Receive => self.i2c.cr1.modify(|_, w| w.rxie().clear_bit()),
                            Event::AddressMatch => self.i2c.cr1.modify(|_, w| w.addrie().clear_bit()),
                            Event::NotAcknowledge => self.i2c.cr1.modify(|_, w| w.nackie().clear_bit()),
                            Event::Stop => self.i2c.cr1.modify(|_, w| w.stopie().clear_bit()),
                            Event::TransferComplete => self.i2c.cr1.modify(|_, w| w.tcie().clear_bit()),
                            Event::Errors => self.i2c.cr1.modify(|_, w| w.errie().clear_bit()),
                        }
                    }

                    pub fn clear_irq(&mut self, event: Event) {
                        match event {
                            Event::AddressMatch => self.i2c.icr.write(|w| w.addrcf().set_bit()),
                            Event::NotAcknowledge => self.i2c.icr.write(|w| w.nackcf().set_bit()),
                            Event::Stop => self.i2c.icr.write(|w| w.stopcf().set_bit()),
                            Event::Errors => self.i2c.icr.write(|w| {
                                w.berrcf().set_bit()
                                    .arlocf().set_bit()
                                    .ovrcf().set_bit()
                            }),
                            _ => (),
                        }
                    }

                    pub fn rx_dma(&mut self, en: bool) {
                        self.i2c.cr1.modify(|_, w| w.rxdmaen().bit(en));
                    }

                    pub fn tx_dma(&mut self, en: bool) {
                        self.i2c.cr1.modify(|_, w| w.txdmaen().bit(en));
                    }

                    pub fn free(self) -> ($I2Cx, PINS) {
                        (self.i2c, self.pins)
                    }
                }

                /// Master controller methods
                impl<PINS> I2c<'_, $I2Cx, PINS> {
                    pub fn master_read(&mut self, addr: Address, len: usize, stop: Stop) {
                        assert!(len < 256);

                        let (addr, add10) = match addr {
                            Address::SevenBit(x) => {
                                assert!(x < 128);
                                ((x as u16) << 1, SEVEN_BIT_ADDR_MODE)
                            }
                            Address::TenBit(x) => {
                                assert!(x < 1024);
                                (x, TEN_BIT_ADDR_MODE)
                            }
                        };

                        while self.i2c.cr2.read().start().bit_is_set() {}

                        self.i2c.cr2.modify(|_, w| {
                            w.sadd()
                                .variant(addr)
                                .add10()
                                .bit(add10)
                                .head10r()
                                .clear_bit()
                                .rd_wrn()
                                .bit(RD_WRN_READ)
                                .nbytes()
                                .variant(len as u8)
                                .reload()
                                .bit(stop == Stop::Reload)
                                .start()
                                .set_bit()
                                .autoend()
                                .bit(stop == Stop::Automatic)
                        });
                    }

                    pub fn master_write(&mut self, addr: Address, len: usize, stop: Stop) {
                        assert!(len < 256);

                        let (addr, add10) = match addr {
                            Address::SevenBit(x) => {
                                assert!(x < 128);
                                ((x as u16) << 1, SEVEN_BIT_ADDR_MODE)
                            }
                            Address::TenBit(x) => {
                                assert!(x < 1024);
                                (x, TEN_BIT_ADDR_MODE)
                            }
                        };

                        while self.i2c.cr2.read().start().bit_is_set() {}

                        self.i2c.cr2.modify(|_, w| {
                            w.sadd()
                                .variant(addr)
                                .add10()
                                .bit(add10)
                                .head10r()
                                .clear_bit()
                                .rd_wrn()
                                .bit(RD_WRN_WRITE)
                                .nbytes()
                                .variant(len as u8)
                                .reload()
                                .bit(stop == Stop::Reload)
                                .start()
                                .set_bit()
                                .autoend()
                                .bit(stop == Stop::Automatic)
                        });
                    }

                    pub fn master_restart(&mut self, len: usize, stop: Stop) -> Result<(), Error> {
                        assert!(len < 256);

                        let rw = !self.i2c.cr2.read().rd_wrn().bit();
                        let head10r = rw == RD_WRN_READ && self.i2c.cr2.read().add10().bit_is_set() == TEN_BIT_ADDR_MODE;

                        busy_wait!(self.i2c, tc, bit_is_set, NoAcknowledgeSource::Data);

                        self.i2c.cr2.modify(|_, w| {
                            w.head10r()
                                .bit(head10r)
                                .rd_wrn()
                                .bit(rw)
                                .nbytes()
                                .variant(len as u8)
                                .reload()
                                .bit(stop == Stop::Reload)
                                .start()
                                .set_bit()
                                .autoend()
                                .bit(stop == Stop::Automatic)
                        });

                        Ok(())
                    }

                    pub fn master_reload(&mut self, len: usize, stop: Stop) -> Result<(), Error> {
                        assert!(len < 256);

                        busy_wait!(self.i2c, tcr, bit_is_set, NoAcknowledgeSource::Data);

                        self.i2c.cr2.modify(|_, w| {
                            w.nbytes()
                                .variant(len as u8)
                                .reload()
                                .bit(stop == Stop::Reload)
                                .autoend()
                                .bit(stop == Stop::Automatic)
                        });

                        Ok(())
                    }

                    pub fn master_stop(&mut self) {
                        self.i2c.cr2.modify(|_, w| w.stop().set_bit());
                    }

                    pub fn master_write_bytes(&mut self, addr: Address, bytes: &[u8], start: Start, stop: Stop) -> Result<(), Error> {
                        let mut begin = true;
                        let mut rem = bytes.len();
                        let mut iter = bytes.chunks(255);

                        for chunk in &mut iter {
                            let stp = if rem > 255 {
                                Stop::Reload
                            } else {
                                stop
                            };

                            rem -= chunk.len();

                            if begin {
                                match start {
                                    Start::Start => self.master_write(addr, chunk.len(), stp),
                                    Start::Restart => {
                                        assert!(self.i2c.cr2.read().rd_wrn().bit() == RD_WRN_READ);

                                        self.master_restart(chunk.len(), stp)?;
                                    }
                                    Start::Reload => {
                                        assert!(self.i2c.cr2.read().rd_wrn().bit() == RD_WRN_WRITE);

                                        self.master_reload(chunk.len(), stp)?;
                                    }
                                }
                            } else {
                                self.master_reload(chunk.len(), stp)?;
                            }

                            self.write_bytes(chunk, begin)?;
                            begin = false;
                        }

                        if stop == Stop::Automatic {
                            busy_wait!(self.i2c, tc, bit_is_set, NoAcknowledgeSource::Data);
                        }

                        Ok(())
                    }

                    pub fn master_write_bytes_iter<B>(&mut self, addr: Address, bytes: B, start: Start, stop: Stop) -> Result<(), Error>
                    where
                        B: IntoIterator<Item = u8>,
                    {
                        let mut iter = bytes.into_iter();
                        let mut begin = true;
                        let mut next = iter.next();

                        while let Some(current) = next {
                            next = iter.next();

                            let stp = if next.is_some() {
                                Stop::Reload
                            } else {
                                stop
                            };

                            if begin {
                                begin = false;
                                self.master_write_bytes(addr, &[current], start, stp)?;
                            } else {
                                self.master_write_bytes(addr, &[current], Start::Reload, stp)?;
                            }
                        }

                        Ok(())
                    }

                    fn write_bytes<'a, B>(&mut self, bytes: B, mut first: bool) -> Result<(), Error>
                    where
                        B: IntoIterator<Item = &'a u8>,
                    {
                        for byte in bytes {
                            let nack = if first {
                                first = false;
                                NoAcknowledgeSource::Address
                            } else {
                                NoAcknowledgeSource::Data
                            };

                            busy_wait!(self.i2c, txis, bit_is_set, nack);

                            self.i2c.txdr.write(|w| w.txdata().variant(*byte));
                        }

                        Ok(())
                    }

                    pub fn master_read_bytes(&mut self, addr: Address, buffer: &mut [u8], start: Start, stop: Stop) -> Result<(), Error> {
                        let mut begin = true;
                        let mut rem = buffer.len();
                        let mut iter = buffer.chunks_mut(255);

                        for chunk in &mut iter {
                            let stp = if rem > 255 {
                                Stop::Reload
                            } else {
                                stop
                            };

                            rem -= chunk.len();

                            if begin {
                                match start {
                                    Start::Start => self.master_read(addr, chunk.len(), stp),
                                    Start::Restart => {
                                        assert!(self.i2c.cr2.read().rd_wrn().bit() == RD_WRN_WRITE);

                                        self.master_restart(chunk.len(), stp)?;
                                    }
                                    Start::Reload => {
                                        assert!(self.i2c.cr2.read().rd_wrn().bit() == RD_WRN_READ);

                                        self.master_reload(chunk.len(), stp)?;
                                    }
                                }
                            } else {
                                self.master_reload(chunk.len(), stp)?;
                            }

                            self.read_bytes(chunk, begin)?;
                            begin = false;
                        }

                        if stop == Stop::Automatic {
                            busy_wait!(self.i2c, tc, bit_is_set, NoAcknowledgeSource::Data);
                        }

                        Ok(())
                    }

                    fn read_bytes<'a, B>(&mut self, buffer: B, mut first: bool) -> Result<(), Error>
                    where
                        B: IntoIterator<Item = &'a mut u8>,
                    {
                        for byte in buffer {
                            let nack = if first {
                                first = false;
                                NoAcknowledgeSource::Address
                            } else {
                                NoAcknowledgeSource::Data
                            };

                            busy_wait!(self.i2c, rxne, bit_is_set, nack);

                            *byte = self.i2c.rxdr.read().rxdata().bits();
                        }

                        Ok(())
                    }
                }

                macro_rules! hal {
                    ($addr:ty, $variant:ident) => {
                        impl<PINS> embedded_hal::i2c::blocking::I2c<$addr> for I2c<'_, $I2Cx, PINS> {
                            fn read(&mut self, addr: $addr, buffer: &mut [u8]) -> Result<(), Self::Error> {
                                self.master_read_bytes(Address::$variant(addr), buffer, Start::Start, Stop::Automatic)
                            }

                            fn write(&mut self, addr: $addr, bytes: &[u8]) -> Result<(), Self::Error> {
                                self.master_write_bytes(Address::$variant(addr), bytes, Start::Start, Stop::Automatic)
                            }

                            fn write_iter<B>(&mut self, addr: $addr, bytes: B) -> Result<(), Self::Error>
                            where
                                B: core::iter::IntoIterator<Item = u8>,
                            {
                                self.master_write_bytes_iter(Address::$variant(addr), bytes, Start::Start, Stop::Automatic)
                            }

                            fn write_read(&mut self, addr: $addr, bytes: &[u8], buffer: &mut [u8]) -> Result<(), Self::Error> {
                                let addr = Address::$variant(addr);

                                self.master_write_bytes(addr, bytes, Start::Start, Stop::Software)?;

                                self.master_read_bytes(addr, buffer, Start::Restart, Stop::Automatic)?;

                                Ok(())
                            }

                            fn write_iter_read<B>(&mut self, addr: $addr, bytes: B, buffer: &mut [u8]) -> Result<(), Self::Error>
                            where
                                B: core::iter::IntoIterator<Item = u8>,
                            {
                                let addr = Address::$variant(addr);

                                self.master_write_bytes_iter(addr, bytes, Start::Restart, Stop::Software)?;

                                self.master_read_bytes(addr, buffer, Start::Restart, Stop::Automatic)?;

                                Ok(())
                            }

                            fn transaction<'a>(&mut self, addr: $addr, operations: &mut [Operation<'a>]) -> Result<(), Self::Error> {
                                let len = operations.len();
                                let addr = Address::$variant(addr);

                                if len == 0 {
                                    return Ok(());
                                }

                                let mut next_op = NextOp::Start;

                                for i in 0..len {
                                    let current_op = next_op;
                                    let next = if i + 1 < len {
                                        Some(&operations[i + 1])
                                    } else {
                                        None
                                    };

                                    next_op = next_op!(operations[i], next);

                                    match &mut operations[i] {
                                        Operation::Read(buf) => {
                                            self.master_read_bytes(addr, buf, current_op.start(), next_op.stop())?;
                                        }
                                        Operation::Write(bytes) => {
                                            self.master_write_bytes(addr, bytes, current_op.start(), next_op.stop())?;
                                        }
                                    }
                                }

                                Ok(())
                            }

                            fn transaction_iter<'a, O>(&mut self, addr: $addr, operations: O) -> Result<(), Self::Error>
                            where
                                O: core::iter::IntoIterator<Item = Operation<'a>>,
                            {
                                let addr = Address::$variant(addr);
                                let mut iter = operations.into_iter();
                                let mut next = iter.next();
                                let mut next_op = NextOp::Start;

                                while let Some(current) = next {
                                    next = iter.next();
                                    let current_op = next_op;
                                    next_op = next_op!(current, next);

                                    match current {
                                        Operation::Read(buf) => {
                                            self.master_read_bytes(addr, buf, current_op.start(), next_op.stop())?;
                                        }
                                        Operation::Write(bytes) => {
                                            self.master_write_bytes(addr, bytes, current_op.start(), next_op.stop())?;
                                        }
                                    }
                                }

                                Ok(())
                            }
                        }
                    }
                }

                hal! { SevenBitAddress, SevenBit }
                hal! { TenBitAddress, TenBit }

                impl I2cExt for $I2Cx {
                    fn i2c<'a, PINS>(self, pins: PINS, clocks: impl Clocks + TrustedClocks<'a>, frequency: Hertz) -> Result<I2c<'a, $I2Cx, PINS>, ConfigError>
                    where
                        PINS: Pins<$I2Cx>,
                    {
                        I2c::<$I2Cx, _>::new(self, pins, clocks, frequency)
                    }
                }
            )*
        }
    };
}

i2c! { I2C1, I2C3 }

macro_rules! pins {
    ($($I2Cx:ty: (
        SCL: [
            $($scl:ident),*
        ]
        SDA: [
            $($sda:ident),*
        ]
    )),*) => {
        $(
            $(
                #[sealed]
                impl SclPin<$I2Cx> for crate::gpio::$scl<crate::gpio::Alternate<4, crate::gpio::OpenDrain>> {}
            )*
            $(
                #[sealed]
                impl SdaPin<$I2Cx> for crate::gpio::$sda<crate::gpio::Alternate<4, crate::gpio::OpenDrain>> {}
            )*
        )*
    };
}

pins! {
    I2C1: (
        SCL: [
            PA9, PB6, PB8
        ]
        SDA: [
            PA10, PB7, PB9
        ]
    ),
    I2C3: (
        SCL: [
            PA7, PB10, PB13, PC0
        ]
        SDA: [
            PB4, PB11, PB14, PC1
        ]
    )
}

#[cfg(test)]
mod test {
    use super::I2c;
    use fugit::RateExtU32;

    /// Runs a timing testcase over PCLK and I2C clock ranges
    fn i2c_timing_testcase<F>(f: F)
    where
        F: Fn(u32, u32),
    {
        let i2c_timing_tests = [
            // (i2c_clk, range of bus frequencies to test)
            (2_000_000, (1_000..=100_000)),
            (9_000_000, (2_000..=400_000)),
            (16_000_000, (2_000..=400_000)),
            (19_000_000, (4_000..=1_000_000)),
            (24_000_000, (4_000..=1_000_000)),
            (32_000_000, (4_000..=1_000_000)),
            (48_000_000, (6_000..=1_000_000)),
            (64_000_000, (8_000..=1_000_000)),
        ];

        for (clock, freq_range) in i2c_timing_tests.iter() {
            for freq in freq_range.clone().step_by(1_000) {
                f(*clock, freq)
            }
        }
    }

    #[test]
    /// Test the SCL frequency is within the expected range
    fn i2c_frequency() {
        i2c_timing_testcase(|i2c_clk: u32, freq: u32| {
            let [presc_reg, scll, sclh, _, _] = I2c::<(), ()>::timings(i2c_clk.Hz(), freq.Hz());

            // Timing parameters
            let presc = (presc_reg + 1) as f32;
            let t_i2c_clk = 1. / (i2c_clk as f32);
            let freq = freq as f32;

            // Estimate minimum sync times. Analog filter on, 2 i2c_clk cycles
            let t_af_min = 50e-9_f32; // Analog filter 50ns. From WB55 Datasheet
            let t_sync1 = t_af_min + 2. * t_i2c_clk;
            let t_sync2 = t_af_min + 2. * t_i2c_clk;

            // See RM0434 Rev 9 Section 32.4.9
            let t_high_low = sclh as f32 + 1. + scll as f32 + 1.;
            let t_scl = t_sync1 + t_sync2 + (t_high_low * presc * t_i2c_clk);
            let f_scl = 1. / t_scl;

            let error = (freq - f_scl) / freq;
            println!(
                "Clock = {}: Set SCL = {} Actual = {} Error {:.1}%",
                i2c_clk,
                freq,
                f_scl,
                100. * error
            );

            // We must generate a bus frequency less than or equal to that
            // specified. Tolerate a 2% error
            assert!(f_scl <= 1.02 * freq);

            // But it should not be too much less than specified
            assert!(f_scl > 0.9 * freq);
        });
    }

    #[test]
    /// Test that the low period of SCL is greater than the minimum specification
    fn i2c_scl_low() {
        i2c_timing_testcase(|i2c_clk: u32, freq: u32| {
            let [presc_reg, scll, _, _, _] = I2c::<(), ()>::timings(i2c_clk.Hz(), freq.Hz());

            // Timing parameters
            let presc = (presc_reg + 1) as f32;
            let t_i2c_clk = 1. / (i2c_clk as f32);
            let freq = freq as f32;
            let t_scll = (scll as f32 + 1.) * presc * t_i2c_clk;

            // From RM0434 Rev 9 Table 192
            let t_scll_minimum = match freq {
                x if x <= 100_000. => 4.7e-6, // Standard mode (Sm)
                x if x <= 400_000. => 1.3e-6, // Fast mode (Fm)
                _ => 0.5e-6,                  // Fast mode Plus (Fm+)
            };

            println!("Clock = {}: Target {} Hz; SCLL {}", i2c_clk, freq, scll);
            println!("T SCL LOW {:.2e}; MINIMUM {:.2e}", t_scll, t_scll_minimum);
            assert!(t_scll >= t_scll_minimum);
        });
    }

    #[test]
    /// Test that the high period of SCL is greater than the minimum specification
    fn i2c_scl_high() {
        i2c_timing_testcase(|i2c_clk: u32, freq: u32| {
            let [presc_reg, _, sclh, _, _] = I2c::<(), ()>::timings(i2c_clk.Hz(), freq.Hz());

            // Timing parameters
            let presc = (presc_reg + 1) as f32;
            let t_i2c_clk = 1. / (i2c_clk as f32);
            let freq = freq as f32;
            let t_sclh = (sclh as f32 + 1.) * presc * t_i2c_clk;

            // From RM0434 Rev 9 Table 192
            let t_sclh_minimum = match freq {
                x if x <= 100_000. => 4e-6,   // Standard mode (Sm)
                x if x <= 400_000. => 0.6e-6, // Fast mode (Fm)
                _ => 0.26e-6,                 // Fast mode Plus (Fm+)
            };

            println!("Clock = {}: Target {} Hz; SCLH {}", i2c_clk, freq, sclh);
            println!("T SCL HIGH {:.2e}; MINIMUM {:.2e}", t_sclh, t_sclh_minimum);
            assert!(t_sclh >= t_sclh_minimum);
        });
    }

    #[test]
    /// Test the SDADEL value is greater than the minimum specification
    fn i2c_sdadel_minimum() {
        i2c_timing_testcase(|i2c_clk: u32, freq: u32| {
            let [presc_reg, _, _, sdadel, _] = I2c::<(), ()>::timings(i2c_clk.Hz(), freq.Hz());
            // Timing parameters
            let presc = (presc_reg + 1) as f32;
            let t_i2c_clk = 1. / (i2c_clk as f32);
            let freq = freq as f32;
            let t_sdadel = (sdadel as f32) * presc * t_i2c_clk;

            // From RM0434 Rev 9 Table 192
            let t_fall_max = match freq {
                x if x <= 100_000. => 300e-9, // Standard mode (Sm)
                x if x <= 400_000. => 300e-9, // Fast mode (Fm)
                _ => 120e-9,                  // Fast mode Plus (Fm+)
            };

            let t_af_min = 50e-9_f32; // Analog filter min 50ns. From WB55 Datasheet
            let hddat_min = 0.;

            // From RM0434 Rev 9 Section 32.4.5
            //
            // tSDADEL >= {tf + tHD;DAT(min) - tAF(min) - [(DNF + 3) x tI2CCLK]}
            let t_sdadel_minimim = t_fall_max + hddat_min - t_af_min - (3. * t_i2c_clk);

            println!("Target {} Hz; SDADEL {}", freq, sdadel);
            println!(
                "T SDA DELAY {:.2e} MINIMUM {:.2e}",
                t_sdadel, t_sdadel_minimim
            );
            assert!(sdadel <= 15);
            assert!(t_sdadel >= t_sdadel_minimim);
        });
    }

    #[test]
    /// Test the SDADEL value is less than the maximum specification
    fn i2c_sdadel_maximum() {
        i2c_timing_testcase(|i2c_clk: u32, freq: u32| {
            let [presc_reg, _, _, sdadel, _] = I2c::<(), ()>::timings(i2c_clk.Hz(), freq.Hz());
            // Timing parameters
            let presc = (presc_reg + 1) as f32;
            let t_i2c_clk = 1. / (i2c_clk as f32);
            let freq = freq as f32;
            let t_sdadel = (sdadel as f32) * presc * t_i2c_clk;

            let t_hddat_max = match freq {
                x if x <= 100_000. => 3.45e-6, // Standard mode (Sm)
                x if x <= 400_000. => 0.9e-6,  // Fast mode (Fm)
                _ => 0.45e-6,                  // Fast mode Plus (Fm+)
            };
            let t_af_max = 110e-9_f32; // Analog filter max 110ns. From WB55 Datasheet

            // From RM0434 Rev 9 Section 32.4.5
            //
            // tSDADEL <= {tHD;DAT(max) - tAF(max) - [(DNF + 4) x tI2CCLK]}
            let t_sdadel_maximum = t_hddat_max - t_af_max - (4. * t_i2c_clk);

            println!("Target {} Hz; SDADEL {}", freq, sdadel);
            println!(
                "T SDA DELAY {:.2e} MAXIMUM {:.2e}",
                t_sdadel, t_sdadel_maximum
            );
            assert!(sdadel <= 15);
            assert!(t_sdadel <= t_sdadel_maximum);
        });
    }

    #[test]
    /// Test the SCLDEL value is greater than the minimum specification
    fn i2c_scldel_minimum() {
        i2c_timing_testcase(|i2c_clk: u32, freq: u32| {
            let [presc_reg, _, _, _, scldel_reg] = I2c::<(), ()>::timings(i2c_clk.Hz(), freq.Hz());
            let scldel = scldel_reg + 1;
            // Timing parameters
            let presc = (presc_reg + 1) as f32;
            let t_i2c_clk = 1. / (i2c_clk as f32);
            let freq = freq as f32;
            let t_scldel = (scldel as f32) * presc * t_i2c_clk;

            // From RM0434 Rev 9 Table 192
            let t_rise_max = match freq {
                x if x <= 100_000. => 1000e-9, // Standard mode (Sm)
                x if x <= 400_000. => 300e-9,  // Fast mode (Fm)
                _ => 120e-9,                   // Fast mode Plus (Fm+)
            };
            let t_sudat_min = match freq {
                x if x <= 100_000. => 250e-9, // Standard mode (Sm)
                x if x <= 400_000. => 100e-9, // Fast mode (Fm)
                _ => 50e-9,                   // Fast mode Plus (Fm+)
            };

            // From RM0434 Rev 9 Section 32.4.5
            //
            // tSCLDEL >= tr + tSU;DAT(min)
            let t_scldel_minimum = t_rise_max + t_sudat_min;

            println!("Target {} Hz; SCLDEL {}", freq, scldel);
            println!(
                "T SCL DELAY {:.2e} MINIMUM {:.2e}",
                t_scldel, t_scldel_minimum
            );
            assert!(scldel <= 16);
            assert!(t_scldel >= t_scldel_minimum);
        });
    }
}
