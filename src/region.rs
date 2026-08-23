//! A single contiguous physical memory range.

use crate::addr::PhysAddr;
use crate::addr::saturating_add;
use crate::flags::MemblockFlags;

/// A contiguous range of physical memory `[base, base + size)`.
///
/// Regions are never allowed to overlap within a [`MemblockType`] and are
/// kept sorted by `base`.
///
/// [`MemblockType`]: crate::memblock::MemblockType
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemblockRegion<T: PhysAddr> {
    base: T,
    size: T,
    flags: MemblockFlags,
}

impl<T: PhysAddr> MemblockRegion<T> {
    /// An empty region used to fill unused array slots.
    pub const EMPTY: MemblockRegion<T> = MemblockRegion {
        base: PhysAddr::ZERO,
        size: PhysAddr::ZERO,
        flags: MemblockFlags::NONE,
    };

    /// Creates a region with no special flags.
    pub const fn new(base: T, size: T) -> Self {
        Self {
            base,
            size,
            flags: MemblockFlags::NONE,
        }
    }

    /// Creates a region with the given [`flags`].
    ///
    /// [`flags`]: MemblockFlags
    pub const fn with_flags(base: T, size: T, flags: MemblockFlags) -> Self {
        Self { base, size, flags }
    }

    /// Returns the start address of the region.
    pub const fn base(self) -> T {
        self.base
    }

    /// Returns the size of the region.
    pub const fn size(self) -> T {
        self.size
    }

    /// Returns the exclusive end address (`base + size`) of the region.
    ///
    /// Saturates to [`PhysAddr::MAX`] instead of wrapping around for
    /// regions touching the top of the address space.
    pub fn end(self) -> T {
        saturating_add(self.base, self.size)
    }

    /// Returns the attributes of the region.
    pub const fn flags(self) -> MemblockFlags {
        self.flags
    }
}
