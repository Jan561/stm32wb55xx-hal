use super::PacketHeader;

#[repr(C, packed)]
pub struct Cmd {
    cmdcode: u16,
    plen: u8,
    payload: [u8; 255],
}

#[repr(C, packed)]
pub struct CmdSerial {
    kind: u8,
    cmd: Cmd,
}

#[repr(C, packed)]
pub struct CmdPacket {
    header: PacketHeader,
    cmdserial: CmdSerial,
}
