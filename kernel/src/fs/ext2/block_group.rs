use static_assertions::assert_eq_size;

pub struct BlockGroup {
    pub block_usage_bitmap_block: u32,
    pub inode_usage_bitmap_block: u32,
    pub inode_table_block: u32,
    pub unallocated_blocks_count: u16,
    pub unallocated_inodes_count: u16,
    pub directories_count: u16,
    _unused: [u8; 14],
}

assert_eq_size!(BlockGroup, [u8; 32]);
