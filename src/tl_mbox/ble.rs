use super::channel;
use crate::ipcc::Ipcc;
use aligned::Aligned;
use core::mem::MaybeUninit;

use super::{
    unsafe_linked_list::init_head, BleTable, BLE_CMD_BUFFER, CS_BUFFER, EVT_QUEUE,
    HCI_ACL_DATA_BUFFER, TL_BLE_TABLE,
};

pub struct Ble;

impl Ble {
    pub(super) fn new(ipcc: &mut Ipcc) -> Self {
        unsafe {
            init_head(EVT_QUEUE.as_mut_ptr());

            TL_BLE_TABLE = Aligned(MaybeUninit::new(BleTable {
                pcmd_buffer: BLE_CMD_BUFFER.as_mut_ptr(),
                pcs_buffer: CS_BUFFER.as_ptr().cast(),
                pevt_queue: EVT_QUEUE.as_ptr().cast(),
                phci_acl_data_buffer: HCI_ACL_DATA_BUFFER.as_mut_ptr().cast(),
            }));
        }

        ipcc.c1_set_rx_channel(channel::c2::IPCC_BLE_EVENT_CHANNEL, true);

        Self
    }
}
