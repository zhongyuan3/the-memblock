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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_no_flags() {
        let r = MemblockRegion::new(0x1000usize, 0x200);
        assert_eq!(r.base(), 0x1000);
        assert_eq!(r.size(), 0x200);
        assert_eq!(r.flags(), MemblockFlags::NONE);
    }

    #[test]
    fn with_flags_sets_flags() {
        let r = MemblockRegion::with_flags(0x1000usize, 0x200, MemblockFlags::NOMAP);
        assert_eq!(r.base(), 0x1000);
        assert_eq!(r.size(), 0x200);
        assert!(r.flags().contains(MemblockFlags::NOMAP));
    }

    #[test]
    fn end_is_base_plus_size() {
        let r = MemblockRegion::new(0x1000usize, 0x200);
        assert_eq!(r.end(), 0x1200);
    }

    #[test]
    fn end_saturates_at_max() {
        let top = usize::MAX - 0xf;
        let r = MemblockRegion::new(top, 0x100);
        assert_eq!(r.end(), usize::MAX);
    }

    #[test]
    fn empty_region_is_zero_sized() {
        let r = MemblockRegion::<usize>::EMPTY;
        assert_eq!(r.base(), 0);
        assert_eq!(r.size(), 0);
        assert_eq!(r.flags(), MemblockFlags::NONE);
        assert_eq!(r, MemblockRegion::new(0, 0));
    }

    #[test]
    fn region_is_copy_and_eq() {
        let a = MemblockRegion::new(0x1000usize, 0x200);
        let b = a;
        assert_eq!(a, b);
        assert_ne!(a, MemblockRegion::new(0x1000, 0x201));
        assert_ne!(
            a,
            MemblockRegion::with_flags(0x1000, 0x200, MemblockFlags::NOMAP)
        );
    }
}
