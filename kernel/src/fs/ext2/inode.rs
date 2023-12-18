use bitflags::bitflags;
use static_assertions::assert_eq_size;

pub struct Inode {
    pub tp_and_perm: TypeAndPermission,
    pub user_id: u16,
    pub size_lo: u32,
    pub last_access_time: u32,
    pub creation_time: u32,
    pub last_modification_time: u32,
    pub deletion_time: u32,
    pub group_id: u16,
    pub hard_link_count: u16,
    pub disk_sectors: u32,
    pub flags: InodeFlags,
    pub os_specific_value_1: u32,
    pub direct_block_pointers: [u32; 12],
    pub singly_indirect_block_pointer: u32,
    pub doubly_indirect_block_pointer: u32,
    pub triply_indirect_block_pointer: u32,
    pub generation_number: u32,
    pub extended_attribute_block: u32,
    pub size_hi: u32,
    pub fragment_block_address: u32,
    pub os_specific_value_2: [u32; 3],
}

assert_eq_size!(Inode, [u8; 128]);

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct TypeAndPermission {
    value: u16,
}

bitflags! {
    pub struct Type: u16 {
        const FIFO = 0x1000;
        const CHARACTER_DEVICE = 0x2000;
        const DIRECTORY = 0x4000;
        const BLOCK_DEVICE = 0x6000;
        const REGULAR_FILE = 0x8000;
        const SYMBOLIC_LINK = 0xA000;
        const SOCKET = 0xC000;
    }
}

bitflags! {
    pub struct Permission: u16 {
        const OTHER_EXECUTE = 0x01;
        const OTHER_WRITE = 0x02;
        const OTHER_READ = 0x04;
        const GROUP_EXECUTE = 0x10;
        const GROUP_WRITE = 0x20;
        const GROUP_READ = 0x40;
        const USER_EXECUTE = 0x100;
        const USER_WRITE = 0x200;
        const USER_READ = 0x400;
        const STICKY = 0x1000;
        const SET_GROUP_ID = 0x2000;
        const SET_USER_ID = 0x4000;
    }
}

bitflags! {
    pub struct InodeFlags: u32 {
        const SECURE_DELETION = 0x0000_0001;
        const KEEP_COPY_ON_DELETE = 0x0000_0002;
        const FILE_COMPRESSION = 0x0000_0004;
        const SYNC_CHANGES = 0x0000_0008;
        const IMMUTABLE_FILE = 0x0000_0010;
        const APPEND_ONLY = 0x0000_0020;
        const NO_DUMP = 0x0000_0040;
        const LAST_ACCESS_TIME = 0x0000_0080;
        const HASH_INDEXED_DIRECTORY = 0x0001_0000;
        const AFS_DIRECTORY = 0x0002_0000;
        const JOURNAL_FILE_DATA = 0x0004_0000;
    }
}
