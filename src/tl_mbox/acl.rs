use super::PacketHeader;

#[repr(C, packed)]
pub struct AclDataSerial {
    kind: u8,
    handle: u16,
    length: u16,
    acl_data: [u8; 1],
}

#[repr(C, packed)]
pub struct AclDataPacket {
    header: PacketHeader,
    acl_data_serial: AclDataSerial,
}

#[repr(C)]
pub struct MmConfig {
    p_ble_spare_evt_buffer: *const u8,
    p_system_spare_evt_buffer: *const u8,
    p_asynch_evt_pool: *const u8,
    async_evt_pool_size: u32,
    p_traces_evt_pool: *const u8,
    traces_evt_pool_size: u32,
}

#[repr(C)]
pub struct BleInitConf {}
