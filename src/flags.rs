//! Per-region attribute flags.

use bitflags::bitflags;

bitflags! {
    /// Memory region flags, modeled after Linux kernel's `enum memblock_flags`.
    #[repr(transparent)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct MemblockFlags: u8 {
        /// No special request
        const NONE           = 0x0;
        /// hotpluggable region
        const HOTPLUG        = 0x1;
        /// mirrored region
        const MIRROR         = 0x2;
        /// don't add to kernel direct mapping
        const NOMAP          = 0x4;
        /// always detected via a driver
        const DRIVER_MANAGED = 0x8;
        /// don't initialize struct pages
        const RSRV_NOINIT    = 0x10;
        /// memory reserved for kernel use
        const RSRV_KERN      = 0x20;
        /// scratch memory for kexec handover
        const KHO_SCRATCH    = 0x40;
    }
}
