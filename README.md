# the-memblock

A `no_std` reimplementation of the Linux kernel's [memblock] early-boot
memory allocator.

Memblock tracks physical memory as an ordered list of regions split into two
types: `memory` (memory available to the kernel) and `reserved` (memory set
aside for allocations). It provides primitives to add and remove ranges,
query the current layout, allocate aligned blocks of free memory (top-down
or bottom-up), and set per-region attributes such as `NOMAP` or `MIRROR`.

This repository is a [Cargo workspace] with two crates:

| Crate | Description |
| --- | --- |
| [`the-memblock`](the-memblock/) | The allocator library, published to crates.io |
| [`the-memblock-derive`](the-memblock-derive/) | The `#[derive(PhysAddr)]` proc-macro, re-exported by the main crate |

[memblock]: https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html
[Cargo workspace]: https://doc.rust-lang.org/cargo/reference/workspaces.html

## Quick start

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

See the [`the-memblock`](the-memblock/) README for the full API, features,
and the `#[derive(PhysAddr)]` custom address type example, and
[`CHANGELOG.md`](the-memblock/CHANGELOG.md) for the release history.

## License

MIT. See [LICENSE](LICENSE).