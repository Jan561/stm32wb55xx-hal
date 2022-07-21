use core::str::from_utf8_unchecked;

#[derive(Debug)]
#[repr(C)]
pub struct Uid {
    x: u16,
    y: u16,
    waf_lot: [u8; 8],
}

impl Uid {
    pub fn x(&self) -> u16 {
        self.x
    }

    pub fn y(&self) -> u16 {
        self.y
    }

    pub fn waf_num(&self) -> u8 {
        self.waf_lot[0]
    }

    pub fn lot_num(&self) -> &str {
        // SAFETY: Register filled with ASCII chars, see RM0434 Rev 9 p. 1512
        unsafe { from_utf8_unchecked(&self.waf_lot[1..]) }
    }
}

// See RM0434 Rev 9 p. 1511
define_ptr_type!(Uid, 0x1FFF_7590);

#[derive(Debug)]
#[repr(C)]
pub struct FlashSize(u16);

impl FlashSize {
    pub fn kilo_bytes(&self) -> u16 {
        self.0
    }

    pub fn bytes(&self) -> usize {
        self.kilo_bytes() as usize * 1024
    }
}

// See RM0434 Rev 9 p. 1512
define_ptr_type!(FlashSize, 0x1FFF_75E0);
