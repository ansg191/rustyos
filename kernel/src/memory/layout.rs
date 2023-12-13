use x86_64::VirtAddr;

macro_rules! s_lit {
    ($i:expr, TiB) => {
        ($i * 1024 * 1024 * 1024 * 1024)
    };
    ($i:expr, GiB) => {
        ($i * 1024 * 1024 * 1024)
    };
    ($i:expr, MiB) => {
        ($i * 1024 * 1024)
    };
    ($i:expr, KiB) => {
        ($i * 1024)
    };
    ($i:expr, B) => {
        ($i)
    };
}

macro_rules! memory_layout {
    {
        $(
            $(#[$attr:meta])*
            $name:ident = $start:expr => $size:expr;
        )*
    } => {
        $(
            paste::paste! {
                $(#[$attr])*
                #[allow(clippy::identity_op)]
                pub const [<$name _START>]: VirtAddr = VirtAddr::new_truncate($start);
                $(#[$attr])*
                #[allow(clippy::identity_op)]
                pub const [<$name _END>]: VirtAddr = VirtAddr::new_truncate($start + $size - 1);
            }
        )*
    };
}

memory_layout! {
    /// Userspace (128 TiB)
    USERSPACE = 0x0000_0000_0000_0000 => s_lit!(128, TiB);
    /// Guard Hole (7.5 TiB)
    GUARD_HOLE = 0xffff_8000_0000_0000 => s_lit!(7_680, GiB);
    /// Direct Physical Memory Mapping (64 TiB)
    PHYSICAL_MEM = GUARD_HOLE_END.as_u64() + 1 => s_lit!(64, TiB);
    /// Unused Hole (0.5 TiB)
    UNUSED_HOLE1 = PHYSICAL_MEM_END.as_u64() + 1 => s_lit!(512, GiB);
    /// Bitmap Frame Allocator (1 TiB)
    BITMAP_FRAME_ALLOCATOR = UNUSED_HOLE1_END.as_u64() + 1 => s_lit!(1, TiB);
    /// Allocator (31 TiB)
    ALLOCATOR = BITMAP_FRAME_ALLOCATOR_END.as_u64() + 1 => s_lit!(31, TiB);
}
