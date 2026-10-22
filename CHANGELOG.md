# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Changed

- **Removed non-essential `Debug` implementations.** `Memblock`,
  `MemblockType`, `MemblockRegion` and `MemblockFlags` no longer implement
  `Debug`; only `Error` keeps it for error handling. The tests compare values
  directly instead of formatting them. This is a breaking change for code
  that relied on the derived formatting. The `PhysAddr` derive macro no longer
  generates `Debug` either; add it to the derive list explicitly if needed.
- **Flattened the module layout.** `Error`, `Memblock`, `MemblockType`,
  `MemblockRegion`, `MemblockFlags` and the range iterators are now exported
  only from the crate root, and the implementation modules are private. The
  `addr`, `error` and `flags` modules were removed; `PhysAddr` remains
  available as `the_memblock::PhysAddr`.
- **Aligned `add` and allocation signatures with the kernel.** `Memblock::add`,
  `Memblock::phys_alloc` and `Memblock::phys_alloc_range` no longer take a
  `flags` parameter, matching `memblock_add`, `memblock_phys_alloc` and
  `memblock_phys_alloc_range`. They use `MemblockFlags::NONE`. The explicit
  attribute variants are `add_with_flags`, `phys_alloc_with_flags` and
  `phys_alloc_range_with_flags`.
- **Renamed the DRAM boundary accessors.** `Memblock::memory_base` and
  `Memblock::memory_end` are now `start_of_dram` and `end_of_dram`, matching
  the kernel's `memblock_start_of_DRAM` and `memblock_end_of_DRAM`. The
  return type remains `Option<T>`, so an empty memblock is still
  representable.
- **Renamed the `MemblockType` range operations.** `MemblockType::add` and
  `MemblockType::remove` are now `add_range` and `remove_range`, matching
  `memblock_add_range` and `memblock_remove_range`. `Memblock::add` and
  `Memblock::remove` keep their names because they mirror `memblock_add` and
  `memblock_remove`.
- The crate-private flag helper `MemblockType::set_flag` was renamed to
  `setclr_flag` to mirror the kernel's `memblock_setclr_flag`.
- **The address traits moved to `the-mm-core`.** `PhysAddr` and its
  overflow-safe arithmetic helpers (`saturating_add`, `cap_size`) now live in
  the shared `the-mm-core` crate. `the_memblock::PhysAddr` is a re-export,
  so existing code and `#[derive(PhysAddr)]` keep working unchanged.
- **PFN conversions moved to `PageFrame`.** `pfn_up`/`pfn_down`/
  `pfn_to_phys` are no longer part of `PhysAddr`: `PageFrame` now provides
  `phys_to_pfn` (floor), `pfn_to_phys` and `pfn_ceil` (exact ceil).
  `pfn_down` is replaced by `phys_to_pfn`, and `pfn_up` is renamed
  `pfn_ceil`. `Memblock::mem_pfn_ranges` and `PfnRangeIter` now require
  `T: PageFrame`; all other memblock APIs still need only `PhysAddr`.
  `the-memblock` re-exports `PageFrame` for convenience.
- **Iterator types no longer expose the region capacity.** `FreeMemRangeIter`
  and `PfnRangeIter` are now `FreeMemRangeIter<'a, T>` and
  `PfnRangeIter<'a, T>`; they borrow region slices instead of
  `MemblockType<T, N>`. The constructors remain crate-internal.
- `find_in_range`'s documentation now correctly states that it still applies
  the attribute filter with the empty flag set (the default flags skip
  `NOMAP` and `DRIVER_MANAGED` regions).
- **`find_in_range_node` renamed.** The method has no NUMA node selector, so
  it is now `Memblock::find_in_range_with_flags`. The kernel reference in its
  documentation still names `memblock_find_in_range_node`.
- Memblock depends only on `the-mm-core`; it has no dependency on the buddy
  or slab allocators. The `the-mm-core` crate is also re-exported at the
  crate root.

### Removed

- **Removed the `RegionFlags` trait and the `F` type parameter.**
  `Memblock`, `MemblockType`, `MemblockRegion`, `FreeMemRangeIter` and
  `PfnRangeIter` now always use the concrete `MemblockFlags` type. Custom
  flag sets and their filtering policy are gone; new Linux flags should be
  added directly to `MemblockFlags`. This also removes the
  `RegionFlags::ALLOC` indirection: all allocations now tag the reserved
  region with `MemblockFlags::RSRV_KERN`, matching the kernel. The custom
  flag tests and the README section describing them were removed.

## [0.2.1]

### Added

- **Custom region flag sets.** The new [`RegionFlags`] trait makes the
  per-region attribute set generic. `Memblock`, `MemblockType`,
  `MemblockRegion` and the range iterators gained a defaulted type parameter
  `F: RegionFlags = MemblockFlags`, so existing code is unchanged while
  custom flag sets can be used as `Memblock<T, N, MyFlags>`. The default
  [`MemblockFlags`] set keeps the Linux kernel semantics (`ALLOC` =
  `RSRV_KERN` and the `should_skip_region` filtering policy).
- Generic attribute mutation on `Memblock`: `mark_flags`, `clear_flags`,
  `reserved_mark_flags` and `reserved_clear_flags`, usable with any flag
  set. The Linux-specific `mark_*`/`clear_*`/`reserve_kern` methods are now
  provided only for the default `MemblockFlags` set.
- `RegionFlags` is re-exported at the crate root.

### Compatibility

The Linux-specific API (`MemblockFlags`, `reserve_kern`, `mark_*`,
`clear_*`, `MemblockRegion::new`) is source-compatible for the default flag
set. Custom flag sets construct regions with `MemblockRegion::with_flags`
instead of `MemblockRegion::new`.

## [0.2.0]

### Breaking changes

- **Renamed iterator types** in `crate::iter`:
  - `range::Iter` is now `FreeMemRangeIter` (constructed via
    [`Memblock::free_mem_ranges`]).
  - `pfn::Iter` is now `PfnRangeIter` (constructed via
    [`Memblock::mem_pfn_ranges`]).
- **Removed free functions** in `crate::addr`:
  - `addrs_overlap` — use `MemblockType::overlaps_region` instead.
  - `align_up`, `align_down`, `pfn_up`, `pfn_down`, `pfn_to_phys` — these are
    now methods on the [`PhysAddr`] trait.
- **`PhysAddr` trait** gained the required methods `align_up`, `align_down`,
  `pfn_up`, `pfn_down`, `pfn_to_phys`, and dropped the `ONE` associated
  constant. External implementors of the trait must be updated.

### Changed

- `PhysAddr` methods now only require `Copy + Ord + Add + Sub` bounds; the
  division and bitwise operators used internally are no longer part of the
  public trait contract.
- The alignment and PFN helpers validate their parameters again and panic on
  invalid input (zero or non-power-of-two alignment, zero page size).

### Added

- `#[derive(PhysAddr)]` macro, provided by the new `the-memblock-derive`
  crate and re-exported at the crate root alongside the `PhysAddr` trait. It
  implements `PhysAddr` and its required supertraits for single-field tuple
  structs wrapping another `PhysAddr` implementor, e.g.
  `#[derive(PhysAddr)] struct Addr(usize);`.

[`Memblock::free_mem_ranges`]: https://docs.rs/the-memblock/latest/the_memblock/struct.Memblock.html
[`Memblock::mem_pfn_ranges`]: https://docs.rs/the-memblock/latest/the_memblock/struct.Memblock.html
[`PhysAddr`]: https://docs.rs/the-memblock/latest/the_memblock/trait.PhysAddr.html
