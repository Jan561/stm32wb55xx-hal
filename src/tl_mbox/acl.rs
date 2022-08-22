use super::PacketHeader;

#[repr(C, packed)]
pub struct AclDataSerial {
    pub kind: u8,
    pub handle: u16,
    pub length: u16,
    pub acl_data: [u8; 1],
}

#[repr(C, packed)]
pub struct AclDataPacket {
    pub header: PacketHeader,
    pub acl_data_serial: AclDataSerial,
}
