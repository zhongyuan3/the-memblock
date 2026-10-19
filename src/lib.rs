//! A `no_std` reimplementation of the Linux kernel's [memblock] early-boot
//! memory allocator.
//!
//! Memblock tracks physical memory as an ordered list of
//! [`MemblockRegion`]s split into two types:
//! `memory` (memory available to the kernel) and `reserved` (memory set
//! aside for allocations). It provides primitives to add and remove ranges,
//! query the current layout, allocate aligned blocks of free memory
//! (top-down or bottom-up), and set per-region attributes such as `NOMAP` or
//! `MIRROR`. The [`PhysAddr`] address arithmetic lives in [`the_mm_core`] and
//! is shared with the other allocator crates.
//! The [`PhysAddr`] trait is implemented for every unsigned primitive; the
//! [`#[derive(PhysAddr)]`](crate::PhysAddr) macro transparently implements it
//! for single-field wrapper types such as `struct Addr(usize)`.
//!
//! Page frame number (PFN) conversions live in [`PageFrame`] (also from
//! [`the_mm_core`]), because they are only meaningful for page-frame-capable
//! address types. The PFN iterator [`Memblock::mem_pfn_ranges`] therefore
//! requires `T: PageFrame`; the rest of the memblock API only needs
//! [`PhysAddr`].
//!
//! Per-region attributes are modeled by [`MemblockFlags`], which implements
//! the Linux kernel's `enum memblock_flags` semantics. Allocation APIs tag
//! the regions they reserve with [`MemblockFlags::RSRV_KERN`].
//!
//! The main entry points — [`Memblock`], [`MemblockType`],
//! [`MemblockRegion`], [`MemblockFlags`], [`Error`], [`PhysAddr`],
//! [`PageFrame`] and the range iterators [`FreeMemRangeIter`]/
//! [`PfnRangeIter`] — are re-exported at the crate root.
//!
//! [memblock]: https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html

#![no_std]

pub use crate::iter::FreeMemRangeIter;
pub use crate::iter::PfnRangeIter;
pub use crate::memblock::Error;
pub use crate::memblock::Memblock;
pub use crate::memblock::MemblockType;
pub use crate::region::MemblockFlags;
pub use crate::region::MemblockRegion;
pub use the_memblock_derive::PhysAddr;
pub use the_mm_core;
pub use the_mm_core::addr::PhysAddr;
pub use the_mm_core::PageFrame;

mod iter;
mod memblock;
mod region;
