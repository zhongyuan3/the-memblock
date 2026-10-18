//! The physical address type and address arithmetic helpers.
//!
//! The traits and helpers live in [`the_mm_core`] so that every allocator
//! crate shares the same address model. This module re-exports them under
//! the historical `the_memblock::addr` path.
//!
//! The arithmetic is overflow-safe: results that would exceed
//! [`PhysAddr::MAX`] saturate instead of wrapping around.

pub use the_mm_core::addr::PhysAddr;
pub(crate) use the_mm_core::addr::cap_size;
pub(crate) use the_mm_core::addr::saturating_add;
