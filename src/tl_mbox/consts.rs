use super::{evt::CsEvt, PacketHeader};

// Packets

pub const TL_BLECMD_PKT_TYPE: u8 = 0x01;
pub const TL_ACL_DATA_PKT_TYPE: u8 = 0x02;
pub const TL_BLEEVT_PKT_TYPE: u8 = 0x04;
pub const TL_OTCMD_PKT_TYPE: u8 = 0x08;
pub const TL_OTRSP_PKT_TYPE: u8 = 0x09;
pub const TL_CLICMD_PKT_TYPE: u8 = 0x0A;
pub const TL_OTNOT_PKT_TYPE: u8 = 0x0C;
pub const TL_OTACK_PKT_TYPE: u8 = 0x0D;
pub const TL_CLINOT_PKT_TYPE: u8 = 0x0E;
pub const TL_CLIACK_PKT_TYPE: u8 = 0x0F;
pub const TL_SYSCMD_PKT_TYPE: u8 = 0x10;
pub const TL_SYSRSP_PKT_TYPE: u8 = 0x11;
pub const TL_SYSEVT_PKT_TYPE: u8 = 0x12;
pub const TL_CLIRESP_PKT_TYPE: u8 = 0x15;
pub const TL_M0CMD_PKT_TYPE: u8 = 0x16;
pub const TL_LOCCMD_PKT_TYPE: u8 = 0x20;
pub const TL_LOCRSP_PKT_TYPE: u8 = 0x21;
pub const TL_TRACES_APP_PKT_TYPE: u8 = 0x40;
pub const TL_TRACES_WL_PKT_TYPE: u8 = 0x41;

// Other consts

pub const TL_CMD_HDR_SIZE: usize = 4;
pub const TL_EVT_HDR_SIZE: usize = 3;
pub const TL_EVT_CS_PAYLOAD_SIZE: usize = 4;

pub const TL_BLEEVT_CC_OPCODE: u8 = 0x0E;
pub const TL_BLEEVT_CS_OPCODE: u8 = 0x0F;
pub const TL_BLEEVT_VS_OPCODE: u8 = 0xFF;

pub const TL_CS_EVT_SIZE: usize = core::mem::size_of::<CsEvt>();
pub const TL_BLEEVT_CS_PACKET_SIZE: usize = (TL_EVT_HDR_SIZE + TL_CS_EVT_SIZE);
pub const TL_PACKET_HEADER_SIZE: usize = core::mem::size_of::<PacketHeader>();
pub const TL_BLEEVT_CS_BUFFER_SIZE: usize = TL_PACKET_HEADER_SIZE + TL_BLEEVT_CS_PACKET_SIZE;
