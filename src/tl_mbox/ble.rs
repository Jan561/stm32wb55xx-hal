use super::{
    channel,
    consts::{TL_ACL_DATA_PKT_TYPE, TL_BLECMD_PKT_TYPE},
    evt::{EvtBox, EvtPacket},
    unsafe_linked_list::{is_empty, remove_head},
    HeaplessEvtQueue, TL_REF_TABLE,
};
use super::{
    unsafe_linked_list::init_head, BleTable, BLE_CMD_BUFFER, CS_BUFFER, EVT_QUEUE,
    HCI_ACL_DATA_BUFFER, TL_BLE_TABLE,
};
use crate::ipcc::Ipcc;
use aligned::Aligned;
use core::marker::PhantomData;
use core::mem::MaybeUninit;

pub struct Ble {
    _marker: PhantomData<*const ()>,
}

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

        Self {
            _marker: PhantomData,
        }
    }

    pub fn send_cmd(&mut self, ipcc: &mut Ipcc, buf: &[u8]) {
        unsafe {
            let p_cmd_buffer = (*TL_BLE_TABLE.as_mut_ptr()).pcmd_buffer;
            let p_cmd_serial: *mut _ = &mut (*p_cmd_buffer).cmdserial;

            core::ptr::copy(buf.as_ptr(), p_cmd_serial.cast(), buf.len());

            let cmd_packet = &mut *(*TL_BLE_TABLE.as_mut_ptr()).pcmd_buffer;
            cmd_packet.cmdserial.kind = TL_BLECMD_PKT_TYPE;
        }

        ipcc.c1_set_flag_channel(channel::c1::IPCC_BLE_CMD_CHANNEL);
    }

    pub fn send_acl_data(&mut self, ipcc: &mut Ipcc, buf: &[u8]) {
        unsafe {
            let acl_buffer = (*TL_BLE_TABLE.as_mut_ptr()).phci_acl_data_buffer;
            let acl_serial: *mut _ = &mut (*acl_buffer).acl_data_serial;

            core::ptr::copy(buf.as_ptr(), acl_serial.cast(), buf.len());

            let mut cmd_packet = &mut *(*TL_BLE_TABLE.as_mut_ptr()).phci_acl_data_buffer;
            cmd_packet.acl_data_serial.kind = TL_ACL_DATA_PKT_TYPE;
        }

        ipcc.c1_set_flag_channel(channel::c1::IPCC_HCI_ACL_DATA_CHANNEL);
        ipcc.c1_set_tx_channel(channel::c1::IPCC_HCI_ACL_DATA_CHANNEL, true);
    }

    pub(super) fn acl_data_evt_handler(&mut self, ipcc: &mut Ipcc) {
        ipcc.c1_set_tx_channel(channel::c1::IPCC_HCI_ACL_DATA_CHANNEL, false);

        // TODO send ack
    }

    pub(super) fn evt_handler(&mut self, ipcc: &mut Ipcc, queue: &mut HeaplessEvtQueue) {
        unsafe {
            let mut node_ptr = core::ptr::null_mut();
            let node_ptr_ptr: *mut _ = &mut node_ptr;

            while !is_empty(EVT_QUEUE.as_mut_ptr()) {
                remove_head(EVT_QUEUE.as_mut_ptr(), node_ptr_ptr);

                let event: *mut EvtPacket = node_ptr.cast();
                let event = EvtBox::new(event);

                queue
                    .enqueue(event)
                    .unwrap_or_else(|_| panic!("Queue is full"));
            }
        }

        ipcc.c1_clear_flag_channel(channel::c2::IPCC_BLE_EVENT_CHANNEL);
    }
}
