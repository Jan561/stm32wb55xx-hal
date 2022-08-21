pub mod c1 {
    use crate::ipcc::IpccChannel;

    pub const IPCC_BLE_CMD_CHANNEL: IpccChannel = IpccChannel::Channel1;
    pub const IPCC_SYSTEM_CMD_RSP_CHANNEL: IpccChannel = IpccChannel::Channel2;
    pub const IPCC_THREAD_OT_CMD_RSP_CHANNEL: IpccChannel = IpccChannel::Channel3;
    pub const IPCC_MM_RELEASE_BUFFER_CHANNEL: IpccChannel = IpccChannel::Channel4;
    pub const IPCC_HCI_ACL_DATA_CHANNEL: IpccChannel = IpccChannel::Channel6;
}

pub mod c2 {
    use crate::ipcc::IpccChannel;

    pub const IPCC_BLE_EVENT_CHANNEL: IpccChannel = IpccChannel::Channel1;
    pub const IPCC_SYSTEM_EVENT_CHANNEL: IpccChannel = IpccChannel::Channel2;
    pub const IPCC_THREAD_NOTIFICATION_ACK_CHANNEL: IpccChannel = IpccChannel::Channel3;
    pub const IPCC_TRACES_CHANNEL: IpccChannel = IpccChannel::Channel4;
    pub const IPCC_THREAD_CLI_NOTIFICATION_ACK_CHANNEL: IpccChannel = IpccChannel::Channel5;
}
