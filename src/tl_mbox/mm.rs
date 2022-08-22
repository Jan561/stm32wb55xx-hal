use crate::ipcc::Ipcc;
use core::marker::PhantomData;

use super::{
    channel::c1::IPCC_MM_RELEASE_BUFFER_CHANNEL,
    evt::EvtPacket,
    unsafe_linked_list::{init_head, insert_tail, is_empty, remove_head},
    MemManagerTable, BLE_SPARE_EVT_BUF, EVT_POOL, FREE_BUF_QUEUE, LOCAL_FREE_BUF_QUEUE, POOL_SIZE,
    SYS_SPARE_EVT_BUF, TL_MEM_MANAGER_TABLE,
};
use core::mem::MaybeUninit;

use aligned::Aligned;

pub struct MemoryManager {
    _marker: PhantomData<*const ()>,
}

impl MemoryManager {
    pub(super) fn new() -> Self {
        unsafe {
            init_head(FREE_BUF_QUEUE.as_mut_ptr());
            init_head(LOCAL_FREE_BUF_QUEUE.as_mut_ptr());

            TL_MEM_MANAGER_TABLE = Aligned(MaybeUninit::new(MemManagerTable {
                spare_ble_buffer: BLE_SPARE_EVT_BUF.as_ptr().cast(),
                spare_sys_buffer: SYS_SPARE_EVT_BUF.as_ptr().cast(),
                blepool: EVT_POOL.as_ptr().cast(),
                blepoolsize: POOL_SIZE as u32,
                pevt_free_buffer_queue: FREE_BUF_QUEUE.as_mut_ptr(),
                traces_evt_pool: core::ptr::null(),
                tracespoolsize: 0,
            }));
        }

        Self {
            _marker: PhantomData,
        }
    }
}

pub fn evt_drop(evt: *mut EvtPacket, ipcc: &mut Ipcc) {
    unsafe {
        let list_node = evt.cast();

        insert_tail(LOCAL_FREE_BUF_QUEUE.as_mut_ptr(), list_node);
    }

    let channel_is_busy = ipcc.c1_is_active_flag(IPCC_MM_RELEASE_BUFFER_CHANNEL);

    // Postpone event buffer freeing to IPCC interrupt handler
    if channel_is_busy {
        ipcc.c1_set_tx_channel(IPCC_MM_RELEASE_BUFFER_CHANNEL, true);
    } else {
        send_free_buf();
        ipcc.c1_set_flag_channel(IPCC_MM_RELEASE_BUFFER_CHANNEL);
    }
}

pub fn send_free_buf() {
    unsafe {
        let mut node_ptr = core::ptr::null_mut();

        while !is_empty(LOCAL_FREE_BUF_QUEUE.as_mut_ptr()) {
            remove_head(LOCAL_FREE_BUF_QUEUE.as_mut_ptr(), &mut node_ptr);
            insert_tail(
                (*TL_MEM_MANAGER_TABLE.as_mut_ptr()).pevt_free_buffer_queue,
                node_ptr,
            );
        }
    }
}

pub fn free_buf_handler(ipcc: &mut Ipcc) {
    ipcc.c1_set_tx_channel(IPCC_MM_RELEASE_BUFFER_CHANNEL, false);
    send_free_buf();
    ipcc.c1_set_flag_channel(IPCC_MM_RELEASE_BUFFER_CHANNEL);
}
