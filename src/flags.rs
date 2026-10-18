//! Per-region attribute flags.
//!
//! The [`RegionFlags`] trait is the generic interface memblock uses to
//! store, combine and filter per-region attributes. The default
//! implementation, [`MemblockFlags`], is modeled after the Linux kernel's
//! `enum memblock_flags`; custom flag sets can implement [`RegionFlags`] to
//! use their own attributes, e.g. on a `bitflags!` type of their own.

use bitflags::bitflags;
use core::ops::BitAnd;
use core::ops::BitOr;
use core::ops::Not;

/// Per-region attribute flags.
///
/// Implementors describe a set of region attributes: how to represent the
/// empty set ([`RegionFlags::NONE`]), how to combine flags, and the
/// filtering policy used when iterating memory ranges
/// ([`RegionFlags::should_skip`]).
///
/// [`RegionFlags::ALLOC`] is the flag applied to `reserved` regions created
/// by the allocation APIs; it defaults to [`RegionFlags::NONE`] (no tag).
///
/// # Implementation
///
/// Implement on any `Copy + Eq` type with the required bitwise operators,
/// e.g. a `bitflags!` type:
///
/// ```ignore
/// use the_memblock::flags::RegionFlags;
///
/// impl RegionFlags for MyFlags {
///     const NONE: Self = Self::empty();
///     fn should_skip(_region: Self, _requested: Self) -> bool {
///         false // no attribute-based filtering
///     }
/// }
/// ```
///
/// The default [`MemblockFlags`] set additionally overrides [`RegionFlags::ALLOC`]
/// with `RSRV_KERN` and implements the kernel's `should_skip_region` policy
/// (see [`MemblockFlags`]).
///
/// [`RegionFlags::should_skip`]: RegionFlags::should_skip
/// [`RegionFlags::ALLOC`]: RegionFlags::ALLOC
/// [`RegionFlags::NONE`]: RegionFlags::NONE
pub trait RegionFlags:
    Copy + Eq + BitOr<Output = Self> + BitAnd<Output = Self> + Not<Output = Self>
{
    /// The empty flag set.
    const NONE: Self;

    /// The flag applied to regions created by the allocation APIs.
    ///
    /// Defaults to [`RegionFlags::NONE`]; the default [`MemblockFlags`] set
    /// overrides this with `RSRV_KERN` to mirror the kernel.
    const ALLOC: Self = Self::NONE;

    /// Returns `true` if all flags in `other` are set in `self`.
    fn contains(self, other: Self) -> bool {
        self & other == other
    }

    /// Returns `true` if `self` has no flags set.
    fn is_empty(self) -> bool {
        self == Self::NONE
    }

    /// Returns `true` if a region carrying `region` attributes must be
    /// skipped when the caller requests `requested` attributes.
    ///
    /// This is the filtering policy used by
    /// [`Memblock::free_mem_ranges`](crate::memblock::Memblock::free_mem_ranges)
    /// and the allocation search. Implementations that do not need
    /// attribute-based filtering should return `false`.
    fn should_skip(region: Self, requested: Self) -> bool;
}

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

impl RegionFlags for MemblockFlags {
    const NONE: Self = MemblockFlags::NONE;
    const ALLOC: Self = MemblockFlags::RSRV_KERN;

    fn should_skip(region: Self, requested: Self) -> bool {
        if !requested.contains(Self::NOMAP) && region.contains(Self::NOMAP) {
            return true;
        }
        if !requested.contains(Self::DRIVER_MANAGED) && region.contains(Self::DRIVER_MANAGED) {
            return true;
        }
        if requested.contains(Self::MIRROR) && !region.contains(Self::MIRROR) {
            return true;
        }
        if requested.contains(Self::KHO_SCRATCH) && !region.contains(Self::KHO_SCRATCH) {
            return true;
        }
        false
    }
}