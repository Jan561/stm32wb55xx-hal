use super::PacketHeader;

#[repr(C, packed)]
pub struct Cmd {
    cmdcode: u16,
    plen: u8,
    payload: [u8; 255],
}

#[repr(C, packed)]
pub struct CmdSerial {
    pub kind: u8,
    pub cmd: Cmd,
}

#[repr(C, packed)]
pub struct CmdPacket {
    pub header: PacketHeader,
    pub cmdserial: CmdSerial,
}
