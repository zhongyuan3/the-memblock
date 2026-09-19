//! A `no_std` reimplementation of the Linux kernel's [memblock] early-boot
//! memory allocator.
//!
//! Memblock tracks physical memory as an ordered list of
//! [`MemblockRegion`]s split into two types:
//! `memory` (memory available to the kernel) and `reserved` (memory set
//! aside for allocations). It provides primitives to add and remove ranges,
//! query the current layout, allocate aligned blocks of free memory
//! (top-down or bottom-up), and set per-region attributes such as `NOMAP` or
//! `MIRROR`. The [`PhysAddr`] address type and the address
//! and page frame number (PFN) arithmetic it provides live in [`addr`].
//! The [`PhysAddr`] trait is implemented for every unsigned primitive; the
//! [`#[derive(PhysAddr)]`](crate::PhysAddr) macro transparently implements it
//! for single-field wrapper types such as `struct Addr(usize)`.
//!
//! Per-region attributes are generic: the [`RegionFlags`] trait describes a
//! flag set, and the default [`MemblockFlags`]
//! type implements the Linux kernel's `enum memblock_flags` semantics. Use a
//! custom flag type with `Memblock<T, N, F>` to drive the allocator with
//! your own attributes (see [`RegionFlags`]).
//!
//! The main entry points — [`Memblock`], [`MemblockType`],
//! [`MemblockRegion`], [`MemblockFlags`], [`Error`] and the range iterators
//! [`FreeMemRangeIter`]/[`PfnRangeIter`] — are re-exported at the crate
//! root; the same items are also available under their module paths.
//!
//! [memblock]: https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html

#![no_std]

pub use crate::addr::PhysAddr;
pub use crate::error::Error;
pub use crate::flags::MemblockFlags;
pub use crate::flags::RegionFlags;
pub use crate::iter::FreeMemRangeIter;
pub use crate::iter::PfnRangeIter;
pub use crate::memblock::Memblock;
pub use crate::memblock::MemblockType;
pub use crate::region::MemblockRegion;
pub use the_memblock_derive::PhysAddr;

pub mod addr;
pub mod error;
pub mod flags;
pub mod iter;
pub mod memblock;
pub mod region;
