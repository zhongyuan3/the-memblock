# the-memblock

[![crates.io](https://img.shields.io/crates/v/the-memblock)](https://crates.io/crates/the-memblock)
[![docs.rs](https://img.shields.io/docsrs/the-memblock)](https://docs.rs/the-memblock)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A `no_std` reimplementation of the Linux kernel's [memblock] early-boot
memory allocator.

Memblock tracks physical memory as an ordered list of regions split into two
types: `memory` (memory available to the kernel) and `reserved` (memory set
aside for allocations). It provides primitives to add and remove ranges,
query the current layout, allocate aligned blocks of free memory (top-down
or bottom-up), and set per-region attributes such as `NOMAP` or `MIRROR`.

This crate is part of the [the-memblock] workspace; its `#[derive(PhysAddr)]`
proc-macro lives in the [`the-memblock-derive`] crate and is re-exported
here. See [`CHANGELOG.md`](CHANGELOG.md) for the release history.

[memblock]: https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html
[the-memblock]: https://github.com/zhongyuan3/the-memblock
[`the-memblock-derive`]: https://crates.io/crates/the-memblock-derive

## Features

- Fixed-capacity, `no_std`, no allocation: suitable for boot-time and
  embedded use; region storage is a const-generic array.
- Generic over the address width via the `PhysAddr` trait, with built-in
  implementations for all unsigned primitives (`u8`..`u128`, `usize`).
- Overflow-safe range arithmetic: sizes are clamped against the top of the
  address space, mirroring the kernel's `memblock_cap_size`.
- Allocation APIs mirror the kernel: `phys_alloc`, `phys_alloc_range`,
  `phys_free`, `find_in_range`, with `bottom_up` and `current_limit` policy.
- Double-ended iterators for memory/free/PFN ranges
  (`for_each_mem_range`, `for_each_free_mem_range`,
  `for_each_mem_pfn_range` counterparts).
- Region flag management: `HOTPLUG`, `MIRROR`, `NOMAP`, `DRIVER_MANAGED`,
  `RSRV_NOINIT`, `RSRV_KERN`, `KHO_SCRATCH`, generic over the attribute set
  via the [`RegionFlags`] trait.

## Usage

```rust
use the_memblock::flags::MemblockFlags;
use the_memblock::memblock::Memblock;

let mut mb = Memblock::<u64, 16>::new();

// Register physical memory and reserve the kernel image.
mb.add(0x8000_0000, 0x4000_0000, MemblockFlags::NONE).unwrap();
mb.reserve_kern(0x8000_0000, 0x20_0000).unwrap();

// Top-down allocation of a 4 KiB aligned block.
let base = mb.phys_alloc(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
assert_eq!(base, 0xBFFF_F000);
```

## Custom address types

The `PhysAddr` trait is implemented for every unsigned primitive, but you can
wrap a primitive in your own type with the `#[derive(PhysAddr)]` macro. The
derive transparently forwards all address arithmetic to the inner type and
also provides the supertrait implementations the trait requires
(`Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Debug`,
`Add`, `Sub`):

```rust
use the_memblock::flags::MemblockFlags;
use the_memblock::memblock::Memblock;
use the_memblock::PhysAddr;

#[derive(PhysAddr)]
#[repr(transparent)]
struct Addr(usize);

let mut mb = Memblock::<Addr, 8>::new();
mb.add(Addr(0x1000), Addr(0x1000), MemblockFlags::NONE).unwrap();
let base = mb.phys_alloc(Addr(0x100), Addr(0x100), MemblockFlags::NONE).unwrap();
assert_eq!(base, Addr(0x1f00));
```

Only concrete single-field tuple structs are supported. Wrap a type that
already implements `PhysAddr` (such as a primitive).

## Custom region flags

Per-region attributes are generic through the [`RegionFlags`] trait. The
default [`MemblockFlags`] type implements the Linux kernel's
`enum memblock_flags` semantics; implement [`RegionFlags`] on your own
`Copy + Eq` flag type (e.g. a `bitflags!` type) to use it with any address
type and capacity:

```rust
use the_memblock::flags::RegionFlags;
use the_memblock::memblock::Memblock;

#[derive(Clone, Copy, PartialEq, Eq)]
struct MyFlags(u8);

// Bitwise operations + `NONE` + a `should_skip` policy must be provided.
impl core::ops::BitOr for MyFlags {
    type Output = Self;
    fn bitor(self, o: Self) -> Self { Self(self.0 | o.0) }
}
impl core::ops::BitAnd for MyFlags {
    type Output = Self;
    fn bitand(self, o: Self) -> Self { Self(self.0 & o.0) }
}
impl core::ops::Not for MyFlags {
    type Output = Self;
    fn not(self) -> Self { Self(!self.0) }
}

impl RegionFlags for MyFlags {
    const NONE: Self = Self(0);
    fn should_skip(_region: Self, _requested: Self) -> bool { false }
}

let mut mb = Memblock::<u64, 16, MyFlags>::new();
mb.add(0x8000_0000, 0x4000_0000, MyFlags::NONE).unwrap();
let base = mb.phys_alloc(0x1000, 0x1000, MyFlags::NONE).unwrap();
assert_eq!(base, 0xBFFF_F000);
```

The allocation APIs tag the regions they reserve with
[`RegionFlags::ALLOC`] (defaulting to `NONE`; the Linux set uses
`RSRV_KERN`). The generic `mark_flags`/`clear_flags` methods mutate
attributes for any flag set, while the Linux-specific `mark_*`/`clear_*`
methods and `MemblockRegion::new` are only available with the default
[`MemblockFlags`] set.

## License

MIT. See [LICENSE](LICENSE).
