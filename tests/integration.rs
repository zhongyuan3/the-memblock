//! End-to-end tests exercising the public memblock API, modeled on a boot
//! sequence: register physical memory, reserve the kernel image, allocate
//! blocks, and hand memory back.

use the_memblock::error::Error;
use the_memblock::flags::MemblockFlags;
use the_memblock::memblock::Memblock;

const NONE: MemblockFlags = MemblockFlags::NONE;

#[test]
fn readme_example() {
    let mut mb = Memblock::<u64, 16>::new();

    mb.add(0x8000_0000, 0x4000_0000, NONE).unwrap();
    mb.reserve_kern(0x8000_0000, 0x20_0000).unwrap();

    let base = mb.phys_alloc(0x1000, 0x1000, NONE).unwrap();
    assert_eq!(base, 0xBFFF_F000);
}

#[test]
fn boot_lifecycle() {
    let mut mb = Memblock::<usize, 16>::new();

    // Two memory banks with a hole between them.
    mb.add(0x0, 0x1000_0000, NONE).unwrap();
    mb.add(0x2000_0000, 0x2000_0000, NONE).unwrap();
    assert_eq!(mb.memory().count(), 2);
    assert_eq!(mb.phys_mem_size(), 0x3000_0000);
    assert_eq!(mb.memory_base(), Some(0));
    assert_eq!(mb.memory_end(), Some(0x4000_0000));

    // Reserve the "kernel image" at the start of the first bank.
    mb.reserve_kern(0x0, 0x1000).unwrap();
    assert!(mb.is_reserved(0x0));
    assert!(!mb.is_reserved(0x1000));

    // Allocate a boot page table; top-down by default.
    let pt = mb.phys_alloc(0x1000, 0x1000, NONE).unwrap();
    assert_eq!(pt, 0x3FFF_F000);
    assert!(mb.is_reserved(pt));
    assert_eq!(mb.reserved_size(), 0x2000);

    // Allocate a stack in the low bank by restricting the range.
    let stack = mb
        .phys_alloc_range(0x1000, 0x1000, 0x1000, 0x1000_0000, NONE)
        .unwrap();
    assert_eq!(stack, 0x0FFF_F000);

    // Free the page table again.
    mb.phys_free(pt, 0x1000).unwrap();
    assert!(!mb.is_reserved(pt));
    // Kernel image + stack remain reserved.
    assert_eq!(mb.reserved_size(), 0x2000);
}

#[test]
fn allocation_strategy_bottom_up_vs_top_down() {
    let mut top_down = Memblock::<usize, 8>::new();
    top_down.add(0x1000, 0x1000, NONE).unwrap();
    let p = top_down.phys_alloc(0x100, 0x100, NONE).unwrap();
    assert_eq!(p, 0x1f00);

    let mut bottom_up = Memblock::<usize, 8>::new();
    bottom_up.add(0x1000, 0x1000, NONE).unwrap();
    bottom_up.set_bottom_up(true);
    assert!(bottom_up.bottom_up());
    let p = bottom_up.phys_alloc(0x100, 0x100, NONE).unwrap();
    assert_eq!(p, 0x1000);
}

#[test]
fn current_limit_bounds_allocations() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, NONE).unwrap();
    mb.set_current_limit(0x1500);
    assert_eq!(mb.current_limit(), 0x1500);

    // Top-down, capped at 0x1500.
    let p = mb.phys_alloc(0x100, 0x100, NONE).unwrap();
    assert_eq!(p, 0x1400);

    // The limit applies even when the caller passes a larger end.
    let p = mb
        .phys_alloc_range(0x100, 0x100, 0, usize::MAX, NONE)
        .unwrap();
    assert_eq!(p, 0x1300);

    // Raising the limit allows higher allocations again. Top-down picks the
    // highest free block, which is back above the old limit.
    mb.set_current_limit(usize::MAX);
    let p = mb.phys_alloc(0x100, 0x100, NONE).unwrap();
    assert_eq!(p, 0x1f00);
}

#[test]
fn allocation_returns_out_of_memory() {
    let mut mb = Memblock::<usize, 8>::new();
    assert!(matches!(
        mb.phys_alloc(0x100, 0x100, NONE),
        Err(Error::OutOfMemory)
    ));

    mb.add(0x1000, 0x100, NONE).unwrap();
    assert!(matches!(
        mb.phys_alloc(0x200, 0x100, NONE),
        Err(Error::OutOfMemory)
    ));
}

#[test]
fn region_array_can_overflow() {
    let mut mb = Memblock::<usize, 2>::new();
    mb.add(0x1000, 0x100, NONE).unwrap();
    mb.add(0x2000, 0x100, NONE).unwrap();
    // A third, non-mergeable region cannot fit.
    assert!(matches!(
        mb.add(0x3000, 0x100, NONE),
        Err(Error::OverCapacity)
    ));
}

#[test]
fn free_mem_ranges_public_api() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, NONE).unwrap();
    mb.reserve(0x1800, 0x100).unwrap();

    let free: Vec<_> = mb.free_mem_ranges(NONE).collect();
    assert_eq!(free, vec![(0x1000, 0x1800), (0x1900, 0x2000)]);

    let free: Vec<_> = mb.free_mem_ranges(NONE).rev().collect();
    assert_eq!(free, vec![(0x1900, 0x2000), (0x1000, 0x1800)]);
}

#[test]
fn mem_pfn_ranges_public_api() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, NONE).unwrap();
    mb.add(0x4000, 0x2000, NONE).unwrap();

    let pfns: Vec<_> = mb.mem_pfn_ranges(0x1000).collect();
    assert_eq!(pfns, vec![(1, 2), (4, 6)]);
}

#[test]
fn region_attribute_lifecycle() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x0, 0x100, NONE).unwrap();

    mb.mark_nomap(0x40, 0x80).unwrap();
    // NOMAP regions are skipped by default allocation/free iteration.
    assert!(
        mb.free_mem_ranges(NONE)
            .all(|(b, e)| b >= 0xc0 || e <= 0x40)
    );

    mb.clear_nomap(0x40, 0x80).unwrap();
    let free: Vec<_> = mb.free_mem_ranges(NONE).collect();
    assert_eq!(free, vec![(0x0, 0x100)]);

    mb.mark_hotplug(0x0, 0x100).unwrap();
    assert!(
        mb.memory().regions()[0]
            .flags()
            .contains(MemblockFlags::HOTPLUG)
    );
    mb.clear_hotplug(0x0, 0x100).unwrap();

    mb.mark_mirror(0x0, 0x100).unwrap();
    assert!(
        mb.memory().regions()[0]
            .flags()
            .contains(MemblockFlags::MIRROR)
    );
    mb.clear_mirror(0x0, 0x100).unwrap();
}

#[test]
fn reserved_attribute_lifecycle() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.reserve(0x0, 0x100).unwrap();

    mb.reserved_mark_kern(0x0, 0x100).unwrap();
    assert!(
        mb.reserved().regions()[0]
            .flags()
            .contains(MemblockFlags::RSRV_KERN)
    );
    mb.reserved_clear_kern(0x0, 0x100).unwrap();

    mb.reserved_mark_noinit(0x0, 0x100).unwrap();
    assert!(
        mb.reserved().regions()[0]
            .flags()
            .contains(MemblockFlags::RSRV_NOINIT)
    );
    mb.reserved_clear_noinit(0x0, 0x100).unwrap();
    assert_eq!(mb.reserved().regions()[0].flags(), NONE);
}

#[test]
fn allocation_filters_nomap_memory() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, MemblockFlags::NOMAP).unwrap();
    mb.add(0x2000, 0x1000, NONE).unwrap();

    // Default: NOMAP memory is not used.
    let p = mb.phys_alloc(0x100, 0x100, NONE).unwrap();
    assert_eq!(p, 0x2f00);

    // Explicitly requesting NOMAP allows allocating from it.
    let mut mb2 = Memblock::<usize, 8>::new();
    mb2.add(0x1000, 0x1000, MemblockFlags::NOMAP).unwrap();
    let p = mb2.phys_alloc(0x100, 0x100, MemblockFlags::NOMAP).unwrap();
    assert_eq!(p, 0x1f00);
}

#[test]
fn query_apis_work_together() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x100, NONE).unwrap();
    mb.add(0x2000, 0x200, NONE).unwrap();
    mb.reserve(0x1050, 0x20).unwrap();

    assert_eq!(mb.phys_mem_size(), 0x300);
    assert_eq!(mb.reserved_size(), 0x20);
    assert_eq!(mb.region_size(0x1000), 0x100);
    assert_eq!(mb.region_size(0x2100), 0x200);

    assert!(mb.is_memory(0x1050));
    assert!(!mb.is_memory(0x500));
    assert!(mb.is_reserved(0x1060));
    assert!(mb.is_region_memory(0x1000, 0x100));
    assert!(mb.is_region_reserved(0x1040, 0x20));
    assert!(!mb.is_region_reserved(0x1070, 0x20));
}

#[test]
fn works_across_address_widths() {
    let mut mb8 = Memblock::<u8, 4>::new();
    mb8.add(0x10, 0x40, NONE).unwrap();
    // Top-down: end is 0x50, so the aligned block starts at 0x50 - 0x10.
    assert_eq!(mb8.phys_alloc(0x10, 0x10, NONE).unwrap(), 0x40);

    let mut mb16 = Memblock::<u16, 4>::new();
    mb16.add(0x100, 0x400, NONE).unwrap();
    assert_eq!(mb16.phys_alloc(0x100, 0x100, NONE).unwrap(), 0x400);

    let mut mb32 = Memblock::<u32, 4>::new();
    mb32.add(0x1000, 0x1000, NONE).unwrap();
    assert_eq!(mb32.phys_alloc(0x100, 0x100, NONE).unwrap(), 0x1f00);

    let mut mb64 = Memblock::<u64, 4>::new();
    mb64.add(0x1000, 0x1000, NONE).unwrap();
    assert_eq!(mb64.phys_alloc(0x100, 0x100, NONE).unwrap(), 0x1f00);

    let mut mb128 = Memblock::<u128, 4>::new();
    mb128.add(0x1000, 0x1000, NONE).unwrap();
    assert_eq!(mb128.phys_alloc(0x100, 0x100, NONE).unwrap(), 0x1f00);
}

#[test]
fn clone_is_independent() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, NONE).unwrap();

    let mut copy = mb.clone();
    copy.set_bottom_up(true);
    copy.reserve(0x1800, 0x100).unwrap();

    assert!(!mb.bottom_up());
    assert!(mb.reserved().is_empty());
    assert_eq!(copy.reserved_size(), 0x100);
    assert_eq!(mb, mb.clone());
}

#[test]
fn adjacency_with_different_flags_stays_split() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x100, NONE).unwrap();
    mb.add(0x1100, 0x100, MemblockFlags::NOMAP).unwrap();
    assert_eq!(mb.memory().count(), 2);

    // Clearing the flag lets them merge.
    mb.clear_nomap(0x1100, 0x100).unwrap();
    assert_eq!(mb.memory().count(), 1);
    assert_eq!(
        mb.memory().regions()[0],
        the_memblock::region::MemblockRegion::new(0x1000, 0x200)
    );
}

#[test]
fn allocate_in_allocated_memory_is_rejected() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x100, NONE).unwrap();
    let p = mb.phys_alloc(0x100, 0x100, NONE).unwrap();
    assert_eq!(p, 0x1000);

    // The only memory is now fully reserved.
    assert!(matches!(
        mb.phys_alloc(0x100, 0x100, NONE),
        Err(Error::OutOfMemory)
    ));
}

#[test]
fn find_in_range_returns_without_reserving() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, NONE).unwrap();

    let (base, end) = mb.find_in_range(0, usize::MAX, 0x100, 0x100).unwrap();
    assert_eq!((base, end), (0x1f00, 0x2000));
    assert!(mb.reserved().is_empty());

    // Combining find + reserve behaves like an allocation.
    mb.reserve(base, end - base).unwrap();
    assert!(mb.is_reserved(0x1f00));
}
