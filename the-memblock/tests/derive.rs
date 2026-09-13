//! End-to-end tests for the `#[derive(PhysAddr)]` macro: a custom wrapper
//! type drives the full memblock API just like a primitive address.

use the_memblock::PhysAddr;
use the_memblock::error::Error;
use the_memblock::flags::MemblockFlags;
use the_memblock::memblock::Memblock;

const NONE: MemblockFlags = MemblockFlags::NONE;

#[derive(PhysAddr)]
#[repr(transparent)]
struct Addr(usize);

#[derive(PhysAddr)]
struct U64Addr(u64);

#[test]
fn derive_keeps_new_const() {
    const MB: Memblock<Addr, 2> = Memblock::new();
    assert!(MB.memory().is_empty());
    assert_eq!(MB.current_limit(), Addr::MAX);
}

#[test]
fn derived_phys_addr_constants() {
    assert_eq!(Addr::MAX, Addr(usize::MAX));
    assert_eq!(Addr::ZERO, Addr(0));
    assert_eq!(U64Addr::MAX, U64Addr(u64::MAX));
    assert_eq!(U64Addr::ZERO, U64Addr(0));
}

#[test]
fn derived_arithmetic_methods_delegate() {
    assert_eq!(Addr::align_up(Addr(0x1), Addr(0x1000)), Addr(0x1000));
    assert_eq!(Addr::align_down(Addr(0x1fff), Addr(0x1000)), Addr(0x1000));
    assert_eq!(Addr::pfn_up(Addr(0x1), Addr(0x1000)), Addr(1));
    assert_eq!(Addr::pfn_down(Addr(0xfff), Addr(0x1000)), Addr(0));
    assert_eq!(Addr::pfn_to_phys(Addr(2), Addr(0x1000)), Addr(0x2000));
    // Saturation near the top of the address space is preserved.
    assert_eq!(
        Addr::align_up(Addr(usize::MAX - 0x7f), Addr(0x100)),
        Addr(usize::MAX)
    );
}

#[test]
fn derived_supertrait_impls() {
    // Copy + Clone.
    let a = Addr(5);
    let b = a;
    assert_eq!(a, b);

    // PartialEq + Eq + PartialOrd + Ord.
    assert_eq!(Addr(1), Addr(1));
    assert_ne!(Addr(1), Addr(2));
    assert!(Addr(1) < Addr(2));
    assert!(Addr(3) >= Addr(3));
    assert_eq!(core::cmp::min(Addr(3), Addr(1)), Addr(1));

    // Add + Sub.
    assert_eq!(Addr(1) + Addr(2), Addr(3));
    assert_eq!(Addr(3) - Addr(1), Addr(2));

    // Debug.
    assert_eq!(format!("{:?}", Addr(5)), "Addr(5)");
}

#[test]
fn derived_type_allocates_top_down() {
    let mut mb = Memblock::<Addr, 8>::new();
    mb.add(Addr(0x1000), Addr(0x1000), NONE).unwrap();
    let p = mb.phys_alloc(Addr(0x100), Addr(0x100), NONE).unwrap();
    assert_eq!(p, Addr(0x1f00));
    assert!(mb.is_reserved(p));
    assert_eq!(mb.reserved_size(), Addr(0x100));
}

#[test]
fn derived_type_allocates_bottom_up() {
    let mut mb = Memblock::<Addr, 8>::new();
    mb.add(Addr(0x1000), Addr(0x1000), NONE).unwrap();
    mb.set_bottom_up(true);
    let p = mb.phys_alloc(Addr(0x100), Addr(0x100), NONE).unwrap();
    assert_eq!(p, Addr(0x1000));
}

#[test]
fn derived_type_free_roundtrip() {
    let mut mb = Memblock::<Addr, 8>::new();
    mb.add(Addr(0x1000), Addr(0x1000), NONE).unwrap();
    let p = mb.phys_alloc(Addr(0x100), Addr(0x100), NONE).unwrap();
    mb.phys_free(p, Addr(0x100)).unwrap();
    assert!(mb.reserved().is_empty());
    assert!(mb.phys_alloc(Addr(0x100), Addr(0x100), NONE).is_ok());
}

#[test]
fn derived_type_near_max() {
    let mut mb = Memblock::<Addr, 8>::new();
    let top = Addr::MAX - Addr(0x1ff);
    mb.add(top, Addr(0x200), NONE).unwrap();
    mb.set_bottom_up(true);
    let p = mb.phys_alloc(Addr(0x80), Addr(0x100), NONE).unwrap();
    assert_eq!(p, top);
    // Sizes past the top of the address space are clamped.
    assert_eq!(mb.memory().regions()[0].end(), Addr::MAX);
}

#[test]
fn derived_type_iterators() {
    let mut mb = Memblock::<Addr, 8>::new();
    mb.add(Addr(0x1000), Addr(0x1000), NONE).unwrap();
    mb.add(Addr(0x4000), Addr(0x1000), NONE).unwrap();
    mb.reserve(Addr(0x1800), Addr(0x100)).unwrap();

    let free: Vec<_> = mb.free_mem_ranges(NONE).collect();
    assert_eq!(
        free,
        vec![
            (Addr(0x1000), Addr(0x1800)),
            (Addr(0x1900), Addr(0x2000)),
            (Addr(0x4000), Addr(0x5000))
        ]
    );

    let pfns: Vec<_> = mb.mem_pfn_ranges(Addr(0x1000)).collect();
    assert_eq!(pfns, vec![(Addr(1), Addr(2)), (Addr(4), Addr(5))]);
}

#[test]
fn derived_type_queries_and_flags() {
    let mut mb = Memblock::<Addr, 8>::new();
    mb.add(Addr(0x1000), Addr(0x100), NONE).unwrap();
    mb.mark_nomap(Addr(0x1040), Addr(0x20)).unwrap();
    assert!(
        mb.memory().regions()[1]
            .flags()
            .contains(MemblockFlags::NOMAP)
    );
    assert!(mb.is_memory(Addr(0x1050)));
    assert!(!mb.is_memory(Addr(0x1100)));
    mb.clear_nomap(Addr(0x1040), Addr(0x20)).unwrap();
    assert_eq!(mb.memory().count(), 1);
}

#[test]
fn derived_type_out_of_memory() {
    let mut mb = Memblock::<Addr, 8>::new();
    mb.add(Addr(0x1000), Addr(0x100), NONE).unwrap();
    assert!(matches!(
        mb.phys_alloc(Addr(0x200), Addr(0x100), NONE),
        Err(Error::OutOfMemory)
    ));
}

#[test]
fn derived_type_over_u64() {
    let mut mb = Memblock::<U64Addr, 8>::new();
    mb.add(U64Addr(0x8000_0000), U64Addr(0x4000_0000), NONE)
        .unwrap();
    mb.reserve_kern(U64Addr(0x8000_0000), U64Addr(0x20_0000))
        .unwrap();
    let p = mb
        .phys_alloc(U64Addr(0x1000), U64Addr(0x1000), NONE)
        .unwrap();
    assert_eq!(p, U64Addr(0xBFFF_F000));
}

#[test]
fn derived_type_works_in_containers() {
    let mut a = Addr(1);
    let mut b = Addr(2);
    core::mem::swap(&mut a, &mut b);
    assert_eq!((a, b), (Addr(2), Addr(1)));

    let arr = [Addr(3), Addr(1), Addr(2)];
    let mut sorted = arr;
    sorted.sort();
    assert_eq!(sorted, [Addr(1), Addr(2), Addr(3)]);
}
