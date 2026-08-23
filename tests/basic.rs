//! Integration test proving the library is usable from outside the crate:
//! `PhysAddr` has built-in implementations for primitive types, so external
//! users can instantiate `Memblock<usize, N>` directly without hitting
//! orphan-rule errors.

use the_memblock::flags::MemblockFlags;
use the_memblock::memblock::Memblock;

#[test]
fn usize_works_out_of_the_box() {
    let mut mb = Memblock::<usize, 8>::new();
    mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
    let p = mb.phys_alloc(0x100, 0x100, MemblockFlags::NONE).unwrap();
    assert_eq!(p, 0x1f00);
    mb.phys_free(p, 0x100).unwrap();
    assert!(mb.reserved().is_empty());
}

#[test]
fn u64_works_out_of_the_box() {
    let mut mb = Memblock::<u64, 4>::new();
    mb.add(0x1_0000, 0x1_0000, MemblockFlags::NONE).unwrap();
    let p = mb.phys_alloc(0x100, 0x100, MemblockFlags::NONE).unwrap();
    assert_eq!(p, 0x1_ff00);
}

#[test]
fn debug_and_clone_are_derived() {
    let mut mb = Memblock::<usize, 4>::new();
    mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
    let clone = mb.clone();
    assert_eq!(clone.memory(), mb.memory());
    let dump = format!("{mb:?}");
    assert!(dump.contains("Memblock"));
}
