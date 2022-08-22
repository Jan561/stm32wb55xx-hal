pub mod acl;
pub mod ble;
pub mod channel;
pub mod cmd;
pub mod consts;
pub mod evt;
pub mod mm;
pub mod unsafe_linked_list;

use self::acl::AclDataPacket;
use self::ble::Ble;
use self::evt::EvtBox;
use self::mm::MemoryManager;
use self::{cmd::CmdPacket, unsafe_linked_list::ListNode};
use crate::{ipcc::Ipcc, rcc::rec};
use aligned::{Aligned, A4};
use consts::{TL_CS_EVT_SIZE, TL_EVT_HDR_SIZE, TL_PACKET_HEADER_SIZE};
use core::mem::MaybeUninit;

// From STM32_WPAN/interface/patterns/ble_thread/tl/mbox_def.h
#[repr(C, packed)]
pub struct SafeBootInfoTable {
    version: u32,
}

#[repr(C, packed)]
pub struct FusInfoTable {
    version: u32,
    memory_size: u32,
    fus_info: u32,
}

#[repr(C, packed)]
pub struct WirelessFwInfoTable {
    version: u32,
    memory_size: u32,
    info_stack: u32,
    reserved: u32,
}

#[repr(C, packed)]
pub struct DeviceInfoTable {
    safe_boot_info_table: SafeBootInfoTable,
    fus_info_table: FusInfoTable,
    wireless_fw_info_table: WirelessFwInfoTable,
}

#[repr(C, packed)]
pub struct BleTable {
    pcmd_buffer: *mut CmdPacket,
    pcs_buffer: *const u8,
    pevt_queue: *const u8,
    phci_acl_data_buffer: *mut AclDataPacket,
}

#[repr(C, packed)]
pub struct ThreadTable {
    notack_buffer: *const u8,
    clicmdrsp_buffer: *const u8,
    otcmdrsp_buffer: *const u8,
    clinot_buffer: *const u8,
}

#[repr(C, packed)]
pub struct LldTestsTable {
    clicmdrsp_buffer: *const u8,
    m0cmd_buffer: *const u8,
}

#[repr(C, packed)]
pub struct BleLldTable {
    cmdrsp_buffer: *const u8,
    m0cmd_buffer: *const u8,
}

#[repr(C, packed)]
pub struct ZigbeeTable {
    notif_m0_to_m4_buffer: *const u8,
    appli_cmd_m4_to_m0_buffer: *const u8,
    request_m0_to_m4_buffer: *const u8,
}

#[repr(C, packed)]
pub struct SysTable {
    pcmd_buffer: *mut CmdPacket,
    sys_queue: *const ListNode,
}

#[repr(C, packed)]
pub struct MemManagerTable {
    spare_ble_buffer: *const u8,
    spare_sys_buffer: *const u8,

    blepool: *const u8,
    blepoolsize: u32,

    pevt_free_buffer_queue: *mut ListNode,

    traces_evt_pool: *const u8,
    tracespoolsize: u32,
}

#[repr(C, packed)]
pub struct TracesTable {
    traces_queue: *const u8,
}

#[repr(C, packed)]
pub struct Mac802_15_4 {
    p_cmdrsp_buffer: *const u8,
    p_notack_buffer: *const u8,
    evt_queue: *const u8,
}

#[repr(C)]
pub struct RefTable {
    p_device_info_table: *const DeviceInfoTable,
    p_ble_table: *const BleTable,
    p_thread_table: *const ThreadTable,
    p_sys_table: *const SysTable,
    p_mem_manager_table: *const MemManagerTable,
    p_traces_table: *const TracesTable,
    p_mac_802_15_4_table: *const Mac802_15_4,
    p_zigbee_table: *const ZigbeeTable,
    p_lld_tests_table: *const LldTestsTable,
    p_ble_lld_table: *const BleLldTable,
}

#[link_section = "TL_REF_TABLE"]
static mut TL_REF_TABLE: MaybeUninit<RefTable> = MaybeUninit::uninit();

#[link_section = "MB_MEM1"]
static mut TL_DEVICE_INTO_TABLE: Aligned<A4, MaybeUninit<DeviceInfoTable>> =
    Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_BLE_TABLE: Aligned<A4, MaybeUninit<BleTable>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_THREAD_TABLE: Aligned<A4, MaybeUninit<ThreadTable>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_LLD_TESTS_TABLE: Aligned<A4, MaybeUninit<LldTestsTable>> =
    Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_BLE_LLD_TABLE: Aligned<A4, MaybeUninit<BleLldTable>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_SYS_TABLE: Aligned<A4, MaybeUninit<SysTable>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_MEM_MANAGER_TABLE: Aligned<A4, MaybeUninit<MemManagerTable>> =
    Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_TRACES_TABLE: Aligned<A4, MaybeUninit<TracesTable>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_MAC_802_15_4_TABLE: Aligned<A4, MaybeUninit<Mac802_15_4>> =
    Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TL_ZIGBEE_TABLE: Aligned<A4, MaybeUninit<ZigbeeTable>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut FREE_BUF_QUEUE: Aligned<A4, MaybeUninit<ListNode>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut TRACES_EVT_QUEUE: Aligned<A4, MaybeUninit<ListNode>> = Aligned(MaybeUninit::uninit());

type PacketHeader = ListNode;

#[link_section = "MB_MEM2"]
static mut CS_BUFFER: Aligned<
    A4,
    MaybeUninit<[u8; TL_PACKET_HEADER_SIZE + TL_EVT_HDR_SIZE + TL_CS_EVT_SIZE]>,
> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut EVT_QUEUE: Aligned<A4, MaybeUninit<ListNode>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM1"]
static mut SYSTEM_EVT_QUEUE: Aligned<A4, MaybeUninit<ListNode>> = Aligned(MaybeUninit::uninit());

// Not in shared RAM
static mut LOCAL_FREE_BUF_QUEUE: MaybeUninit<ListNode> = MaybeUninit::uninit();

const CFG_TLBLE_EVT_QUEUE_LENGTH: usize = 5;
const CFG_TLBLE_MOST_EVENT_PAYLOAD_SIZE: usize = 255;
const TL_BLE_EVENT_FRAME_SIZE: usize = TL_EVT_HDR_SIZE + CFG_TLBLE_MOST_EVENT_PAYLOAD_SIZE;

const fn divc(x: usize, y: usize) -> usize {
    (x + y - 1) / y
}

const POOL_SIZE: usize =
    CFG_TLBLE_EVT_QUEUE_LENGTH * 4 * divc(TL_PACKET_HEADER_SIZE + TL_BLE_EVENT_FRAME_SIZE, 4);

#[link_section = "MB_MEM2"]
static mut EVT_POOL: Aligned<A4, MaybeUninit<[u8; POOL_SIZE]>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM2"]
static mut SYS_CMD_BUFFER: Aligned<A4, MaybeUninit<CmdPacket>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM2"]
static mut SYS_SPARE_EVT_BUF: Aligned<
    A4,
    MaybeUninit<[u8; TL_PACKET_HEADER_SIZE + TL_EVT_HDR_SIZE + 255]>,
> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM2"]
static mut BLE_SPARE_EVT_BUF: Aligned<
    A4,
    MaybeUninit<[u8; TL_PACKET_HEADER_SIZE + TL_EVT_HDR_SIZE + 255]>,
> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM2"]
static mut BLE_CMD_BUFFER: Aligned<A4, MaybeUninit<CmdPacket>> = Aligned(MaybeUninit::uninit());

#[link_section = "MB_MEM2"]
// Some magic numbers of ST ---------------------------------------------------------v----v
static mut HCI_ACL_DATA_BUFFER: Aligned<A4, MaybeUninit<[u8; TL_PACKET_HEADER_SIZE + 5 + 251]>> =
    Aligned(MaybeUninit::uninit());

pub type HeaplessEvtQueue = heapless::spsc::Queue<EvtBox, 32>;

pub struct TlMbox {
    ble: Ble,
    mm: MemoryManager,
}

impl TlMbox {
    pub fn tl_init(ipcc: crate::pac::IPCC, rec: &mut rec::IPCC) -> (Self, Ipcc) {
        unsafe {
            TL_REF_TABLE = MaybeUninit::new(RefTable {
                p_device_info_table: TL_DEVICE_INTO_TABLE.as_ptr(),
                p_ble_table: TL_BLE_TABLE.as_ptr(),
                p_thread_table: TL_THREAD_TABLE.as_ptr(),
                p_sys_table: TL_SYS_TABLE.as_ptr(),
                p_mem_manager_table: TL_MEM_MANAGER_TABLE.as_ptr(),
                p_traces_table: TL_TRACES_TABLE.as_ptr(),
                p_mac_802_15_4_table: TL_MAC_802_15_4_TABLE.as_ptr(),
                p_zigbee_table: TL_ZIGBEE_TABLE.as_ptr(),
                p_lld_tests_table: TL_LLD_TESTS_TABLE.as_ptr(),
                p_ble_lld_table: TL_BLE_LLD_TABLE.as_ptr(),
            });

            TL_DEVICE_INTO_TABLE = Aligned(MaybeUninit::zeroed());
            TL_BLE_TABLE = Aligned(MaybeUninit::zeroed());
            TL_THREAD_TABLE = Aligned(MaybeUninit::zeroed());
            TL_SYS_TABLE = Aligned(MaybeUninit::zeroed());
            TL_MEM_MANAGER_TABLE = Aligned(MaybeUninit::zeroed());
            TL_TRACES_TABLE = Aligned(MaybeUninit::zeroed());
            TL_MAC_802_15_4_TABLE = Aligned(MaybeUninit::zeroed());
            TL_ZIGBEE_TABLE = Aligned(MaybeUninit::zeroed());
            TL_LLD_TESTS_TABLE = Aligned(MaybeUninit::zeroed());
            TL_BLE_LLD_TABLE = Aligned(MaybeUninit::zeroed());

            LOCAL_FREE_BUF_QUEUE = MaybeUninit::zeroed();

            EVT_POOL = Aligned(MaybeUninit::zeroed());
            SYS_CMD_BUFFER = Aligned(MaybeUninit::zeroed());
            SYS_SPARE_EVT_BUF = Aligned(MaybeUninit::zeroed());
            BLE_SPARE_EVT_BUF = Aligned(MaybeUninit::zeroed());
            CS_BUFFER = Aligned(MaybeUninit::zeroed());
            BLE_CMD_BUFFER = Aligned(MaybeUninit::zeroed());
        }

        let mut ipcc = Ipcc::new(ipcc, rec);

        let ble = Ble::new(&mut ipcc);
        let mm = MemoryManager::new();

        let s = Self { ble, mm };

        (s, ipcc)
    }
}
