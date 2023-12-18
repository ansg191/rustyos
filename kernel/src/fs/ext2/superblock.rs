use bitflags::bitflags;
use static_assertions::assert_eq_size;

pub struct SuperBlock {
    pub inode_count: u32,
    pub block_count: u32,
    pub reserved_block_count: u32,
    pub unallocated_block_count: u32,
    pub unallocated_inode_count: u32,
    pub superblock_block_number: u32,
    pub block_size: u32,
    pub fragment_size: u32,
    pub blocks_per_group: u32,
    pub fragments_per_group: u32,
    pub inodes_per_group: u32,
    pub last_mount_time: u32,
    pub last_write_time: u32,
    pub mount_count: u16,
    pub max_mount_count: u16,
    pub magic: u16,
    pub state: FileSystemState,
    pub errors: ErrorHandlingMethod,
    pub minor_version: u16,
    pub last_check_time: u32,
    pub check_interval: u32,
    pub creator_os: u32,
    pub major_version: u32,
    pub reserved_blocks_uid: u16,
    pub reserved_blocks_gid: u16,

    // Extended Superblock, only if major_version >= 1
    pub first_non_reserved_inode: u32,
    pub inode_size: u16,
    pub block_group_number: u16,
    pub optional_features: OptFeatures,
    pub required_features: RequiredFeatures,
    pub readonly_features: ReadOnlyFeatures,
    pub filesystem_id: u128,
    pub volume_name: [u8; 16],
    pub path_to_last_mounted: [u8; 64],
    pub compression_algorithms: u32,
    pub block_preallocations_for_files: u8,
    pub block_preallocations_for_directories: u8,
    _unused: u16,
    pub journal_id: u128,
    pub journal_inode: u32,
    pub journal_device: u32,
    pub orphan_inode_list_head: u32,
    _unused2: [u8; 788],
}

assert_eq_size!(SuperBlock, [u8; 1024]);

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u16)]
pub enum FileSystemState {
    Clean = 1,
    Error = 2,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[repr(u16)]
pub enum ErrorHandlingMethod {
    Ignore = 1,
    RemountAsReadOnly = 2,
    KernelPanic = 3,
}

bitflags! {
    pub struct OptFeatures: u32 {
        /// Preallocate some number of blocks for files
        const PREALLOCATE_BLOCKS_FOR_DIRECTORIES = 0x0001;
        /// AFS server inodes exist
        const AFS_SERVER_INODES_EXIST = 0x0002;
        /// File system has a journal (Ext3)
        const HAS_JOURNAL = 0x0004;
        /// Inodes have extended attributes
        const INODES_HAVE_EXTENDED_ATTRIBUTES = 0x0008;
        /// File system can resize itself for larger partitions
        const CAN_RESIZE = 0x0010;
        /// Directories use hash index
        const DIRECTORIES_USE_HASH_INDEX = 0x0020;
    }
}

bitflags! {
    pub struct RequiredFeatures: u32 {
        /// Compression is used
        const COMPRESSION = 0x0001;
        /// Directory entries contain a type field
        const DIRECTORY_TYPE_FIELD = 0x0002;
        /// File system needs to replay its journal
        const NEEDS_JOURNAL_REPLAY = 0x0004;
        /// File system uses journal device
        const USES_JOURNAL_DEVICE = 0x0008;
    }
}

bitflags! {
    pub struct ReadOnlyFeatures: u32 {
        /// Sparse superblocks and group descriptors
        const SPARSE_SUPERBLOCKS_AND_GROUP_DESCRIPTORS = 0x0001;
        /// File system uses a 64-bit file size
        const USES_64_BIT_FILE_SIZE = 0x0002;
        /// Directory contents are stored in the form of a Binary Tree
        const HAS_BINARY_TREES = 0x0004;
    }
}
