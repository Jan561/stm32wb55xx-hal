use super::PacketHeader;
use crate::ipcc::Ipcc;
use core::mem::MaybeUninit;

#[repr(C, packed)]
pub struct CsEvt {
    status: u8,
    numcmd: u8,
    cmdcode: u16,
}

#[repr(C, packed)]
pub struct CcEvt {
    numcmd: u8,
    cmdcode: u16,
    payload: [u8; 1],
}

#[repr(C, packed)]
pub struct AsynchEvt {
    subevtcode: u16,
    payload: [u8; 1],
}

#[repr(C, packed)]
pub struct Evt {
    evtcode: u8,
    plen: u8,
    payload: [u8; 1],
}

#[repr(C, packed)]
pub struct EvtSerial {
    kind: u8,
    evt: Evt,
}

#[repr(C, packed)]
pub struct EvtPacket {
    header: PacketHeader,
    evtserial: EvtSerial,
}

impl EvtPacket {
    pub fn kind(&self) -> u8 {
        self.evtserial.kind
    }

    pub fn evt(&self) -> &Evt {
        &self.evtserial.evt
    }
}

pub struct EvtBox(*mut EvtPacket);

impl EvtBox {
    pub fn evt(&self) -> EvtPacket {
        let mut evt = MaybeUninit::uninit();
        unsafe {
            self.0.copy_to(evt.as_mut_ptr(), 1);
            evt.assume_init()
        }
    }
}

impl Drop for EvtBox {
    fn drop(&mut self) {
        unsafe {
            let rb = crate::pac::Peripherals::steal().IPCC;
            let mut ipcc = Ipcc { rb };

            super::mm::evt_drop(self.0, &mut ipcc);
        }
    }
}
