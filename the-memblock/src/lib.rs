//! A `no_std` reimplementation of the Linux kernel's [memblock] early-boot
//! memory allocator.
//!
//! Memblock tracks physical memory as an ordered list of
//! [`MemblockRegion`](crate::region::MemblockRegion)s split into two types:
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
//! [memblock]: https://www.kernel.org/doc/html/latest/core-api/boot-time-mm.html

#![no_std]

pub use crate::addr::PhysAddr;
pub use the_memblock_derive::PhysAddr;

pub mod addr;
pub mod error;
pub mod flags;
pub mod iter;
pub mod memblock;
pub mod region;
