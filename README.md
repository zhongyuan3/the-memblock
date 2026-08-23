# the-memblock

A `no_std` reimplementation of the Linux kernel's [memblock] early-boot
memory allocator.

Memblock tracks physical memory as an ordered list of regions split into two
types: `memory` (memory available to the kernel) and `reserved` (memory set
aside for allocations). It provides primitives to add and remove ranges,
query the current layout, allocate aligned blocks of free memory (top-down
or bottom-up), and set per-region attributes such as `NOMAP` or `MIRROR`.

[memblock]: https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html

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
  `RSRV_NOINIT`, `RSRV_KERN`, `KHO_SCRATCH`.

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

## License

MIT. See [LICENSE-MIT](LICENSE-MIT).
