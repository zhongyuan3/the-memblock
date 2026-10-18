# Changelog

All notable changes to this project are documented in this file.

## [Unreleased]

### Changed

- **The address traits moved to `the-mm-core`.** `PhysAddr` and its
  overflow-safe arithmetic helpers (`saturating_add`, `cap_size`) now live in
  the shared `the-mm-core` crate. `the_memblock::addr::PhysAddr` is a
  re-export, so existing code and `#[derive(PhysAddr)]` keep working
  unchanged.
- **PFN conversions moved to `PageFrame`.** `pfn_up`/`pfn_down`/
  `pfn_to_phys` are no longer part of `PhysAddr`: `PageFrame` now provides
  `phys_to_pfn` (floor), `pfn_to_phys` and `pfn_ceil` (exact ceil).
  `pfn_down` is replaced by `phys_to_pfn`, and `pfn_up` is renamed
  `pfn_ceil`. `Memblock::mem_pfn_ranges` and `PfnRangeIter` now require
  `T: PageFrame`; all other memblock APIs still need only `PhysAddr`.
  `the-memblock` re-exports `PageFrame` for convenience.
- **Iterator types no longer expose the region capacity.** `FreeMemRangeIter`
  and `PfnRangeIter` are now `FreeMemRangeIter<'a, T, F>` and
  `PfnRangeIter<'a, T, F>`; they borrow region slices instead of
  `MemblockType<T, N, F>`. The constructors remain crate-internal.
- `find_in_range`'s documentation now correctly states that it still applies
  the `RegionFlags::should_skip` policy with the empty flag set (the default
  flags skip `NOMAP` and `DRIVER_MANAGED` regions).
- **`find_in_range_node` renamed.** The method has no NUMA node selector, so
  it is now `Memblock::find_in_range_with_flags`. The kernel reference in its
  documentation still names `memblock_find_in_range_node`.
- **`RegionFlags` has explicit set operations.** The trait no longer requires
  `BitOr`/`BitAnd`/`Not`; implementors provide `union`, `intersection` and
  `difference`, and `contains` is derived from `intersection`. This lowers
  the boilerplate for custom flag types and decouples the trait from one
  particular bitwise representation. `MemblockFlags` implements the new
  operations on top of `bitflags`.
- Memblock depends only on `the-mm-core`; it has no dependency on the buddy
  or slab allocators. The `the-mm-core` crate is also re-exported at the
  crate root.

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

[`Memblock::free_mem_ranges`]: https://docs.rs/the-memblock/latest/the_memblock/memblock/struct.Memblock.html
[`Memblock::mem_pfn_ranges`]: https://docs.rs/the-memblock/latest/the_memblock/memblock/struct.Memblock.html
[`PhysAddr`]: https://docs.rs/the-memblock/latest/the_memblock/addr/trait.PhysAddr.html
