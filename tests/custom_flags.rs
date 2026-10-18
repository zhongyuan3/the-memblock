//! End-to-end tests for a custom flag set driving the generic memblock API.

use core::ops::BitAnd;
use core::ops::BitOr;
use core::ops::Not;

use the_memblock::error::Error;
use the_memblock::flags::RegionFlags;
use the_memblock::memblock::Memblock;
use the_memblock::region::MemblockRegion;

/// A hand-rolled custom attribute set, showing that custom flags need not be
/// built on `bitflags!`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MyFlags(u8);

impl MyFlags {
    /// Not to be allocated from unless explicitly requested.
    const EXCLUDED: Self = Self(0x1);
    /// Only iterated when explicitly requested.
    const SPECIAL: Self = Self(0x2);
    /// Applied to regions created by the allocation APIs.
    const TAGGED: Self = Self(0x4);
}

impl BitOr for MyFlags {
    type Output = Self;
    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl BitAnd for MyFlags {
    type Output = Self;
    fn bitand(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }
}

impl Not for MyFlags {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}

impl RegionFlags for MyFlags {
    const NONE: Self = Self(0);
    const ALLOC: Self = Self::TAGGED;

    fn should_skip(region: Self, requested: Self) -> bool {
        // `EXCLUDED` regions are skipped unless requested.
        if region.contains(Self::EXCLUDED) && !requested.contains(Self::EXCLUDED) {
            return true;
        }
        // `SPECIAL` regions are only iterated when explicitly requested.
        if requested.contains(Self::SPECIAL) && !region.contains(Self::SPECIAL) {
            return true;
        }
        false
    }
}

fn assert_regions(mb: &Memblock<usize, 8, MyFlags>, expected: &[(usize, usize, u8)]) {
    let got: Vec<_> = mb
        .memory()
        .regions()
        .iter()
        .map(|r| (r.base(), r.size(), r.flags().0))
        .collect();
    assert_eq!(&got[..], expected);
}

#[test]
fn add_merges_adjacent_same_flags() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x100, 0x100, MyFlags::NONE).unwrap();
    mb.add(0x200, 0x100, MyFlags::NONE).unwrap();
    assert_regions(&mb, &[(0x100, 0x200, 0)]);
}

#[test]
fn add_keeps_adjacent_regions_with_different_flags_split() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x100, 0x100, MyFlags::NONE).unwrap();
    mb.add(0x200, 0x100, MyFlags::EXCLUDED).unwrap();
    assert_regions(&mb, &[(0x100, 0x100, 0), (0x200, 0x100, 0x1)]);
}

#[test]
fn alloc_tags_with_alloc_flag() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x1000, 0x1000, MyFlags::NONE).unwrap();
    let p = mb.phys_alloc(0x100, 0x100, MyFlags::NONE).unwrap();
    assert_eq!(p, 0x1f00);
    assert_eq!(mb.reserved_size(), 0x100);
    assert!(mb.reserved().regions()[0].flags().contains(MyFlags::TAGGED));
}

#[test]
fn alloc_free_roundtrip() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x1000, 0x1000, MyFlags::NONE).unwrap();
    let p = mb.phys_alloc(0x100, 0x100, MyFlags::NONE).unwrap();
    assert_eq!(p, 0x1f00);
    mb.phys_free(p, 0x100).unwrap();
    assert!(mb.reserved().is_empty());
}

#[test]
fn should_skip_filters_excluded_by_default() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x1000, 0x100, MyFlags::EXCLUDED).unwrap();
    mb.add(0x2000, 0x100, MyFlags::NONE).unwrap();

    let free: Vec<_> = mb.free_mem_ranges(MyFlags::NONE).collect();
    assert_eq!(free, vec![(0x2000, 0x2100)]);

    let free: Vec<_> = mb.free_mem_ranges(MyFlags::EXCLUDED).collect();
    assert_eq!(free, vec![(0x1000, 0x1100), (0x2000, 0x2100)]);
}

#[test]
fn special_only_iterated_when_requested() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x1000, 0x100, MyFlags::SPECIAL).unwrap();
    mb.add(0x2000, 0x100, MyFlags::NONE).unwrap();

    // Requesting `SPECIAL` skips regions that lack it.
    let free: Vec<_> = mb.free_mem_ranges(MyFlags::SPECIAL).collect();
    assert_eq!(free, vec![(0x1000, 0x1100)]);

    // Requesting nothing iterates `SPECIAL` regions too.
    let free: Vec<_> = mb.free_mem_ranges(MyFlags::NONE).collect();
    assert_eq!(free, vec![(0x1000, 0x1100), (0x2000, 0x2100)]);
}

#[test]
fn allocation_from_excluded_when_requested() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x1000, 0x1000, MyFlags::EXCLUDED).unwrap();

    assert!(matches!(
        mb.phys_alloc_range(0x100, 0x100, 0, usize::MAX, MyFlags::NONE),
        Err(Error::OutOfMemory)
    ));

    let p = mb
        .phys_alloc_range(0x100, 0x100, 0, usize::MAX, MyFlags::EXCLUDED)
        .unwrap();
    assert_eq!(p, 0x1f00);
}

#[test]
fn mark_flags_and_clear_flags() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x0, 0x100, MyFlags::NONE).unwrap();
    mb.mark_flags(0x40, 0x80, MyFlags::EXCLUDED).unwrap();
    assert_regions(
        &mb,
        &[(0x0, 0x40, 0), (0x40, 0x80, 0x1), (0xc0, 0x40, 0)],
    );
    mb.clear_flags(0x40, 0x80, MyFlags::EXCLUDED).unwrap();
    assert_regions(&mb, &[(0x0, 0x100, 0)]);
}

#[test]
fn reserved_mark_and_clear_flags() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.reserve(0x0, 0x100).unwrap();
    mb.reserved_mark_flags(0x0, 0x100, MyFlags::TAGGED).unwrap();
    assert!(mb.reserved().regions()[0].flags().contains(MyFlags::TAGGED));
    mb.reserved_clear_flags(0x0, 0x100, MyFlags::TAGGED)
        .unwrap();
    assert_eq!(mb.reserved().regions()[0].flags(), MyFlags::NONE);
}

#[test]
fn custom_region_constructed_via_with_flags() {
    let r = MemblockRegion::with_flags(0x1000usize, 0x100, MyFlags::SPECIAL);
    assert_eq!(r.base(), 0x1000);
    assert_eq!(r.flags(), MyFlags::SPECIAL);

    // `MemblockRegion::new` is only available for the default `MemblockFlags`
    // set; custom sets construct empty-flagged regions with `with_flags`.
    let r = MemblockRegion::with_flags(0x2000usize, 0x100, MyFlags::NONE);
    assert_eq!(r.flags(), MyFlags::NONE);
}

#[test]
fn clone_is_independent() {
    let mut mb = Memblock::<usize, 8, MyFlags>::new();
    mb.add(0x1000, 0x1000, MyFlags::NONE).unwrap();
    let mut copy = mb.clone();
    copy.reserve(0x1800, 0x100).unwrap();
    assert!(mb.reserved().is_empty());
    assert_eq!(copy.reserved_size(), 0x100);
}