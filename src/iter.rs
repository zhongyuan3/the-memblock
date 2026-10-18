//! Iterators over region ranges.
//!
//! - [`FreeMemRangeIter`] mirrors the kernel's `__for_each_mem_range`: it
//!   iterates the regions of one memblock type that are not covered by
//!   another (e.g. `memory - reserved`). It implements
//!   [`DoubleEndedIterator`], so reverse iteration is obtained with
//!   [`Iterator::rev`] (e.g. for top-down allocation), just like any Rust
//!   iterator.
//! - [`PfnRangeIter`] mirrors the kernel's `for_each_mem_pfn_range`: it
//!   iterates `memory` regions as page frame number ranges.

use crate::addr::PhysAddr;
use crate::flags::MemblockFlags;
use crate::flags::RegionFlags;
use crate::memblock::MemblockType;
use crate::memblock::should_skip_region;

use core::cmp::max;
use core::cmp::min;
use core::iter::DoubleEndedIterator;

/// Iterator over `memory` regions converted to page frame number (PFN)
/// ranges.
///
/// For each `memory` region it yields `[start_pfn, end_pfn)` where
/// `start_pfn = ceil(base / page_size)` (i.e. `PFN_UP`) and
/// `end_pfn = floor(end / page_size)` (i.e. `PFN_DOWN`). Regions that
/// contain no full page are skipped.
///
/// Mirrors the kernel's `for_each_mem_pfn_range`.
///
/// # Panics
///
/// The associated [`Memblock::mem_pfn_ranges`](crate::memblock::Memblock::mem_pfn_ranges)
/// constructor panics if `page_size` is zero.
pub struct PfnRangeIter<'a, T: PhysAddr, const N: usize, F: RegionFlags = MemblockFlags> {
    idx: usize,
    page_size: T,
    mem: &'a MemblockType<T, N, F>,
}

impl<'a, T: PhysAddr, const N: usize, F: RegionFlags> PfnRangeIter<'a, T, N, F> {
    pub(crate) fn new(mem: &'a MemblockType<T, N, F>, page_size: T) -> Self {
        Self {
            idx: 0,
            page_size,
            mem,
        }
    }
}

impl<'a, T: PhysAddr, const N: usize, F: RegionFlags> Iterator for PfnRangeIter<'a, T, N, F> {
    type Item = (T, T);

    fn next(&mut self) -> Option<Self::Item> {
        let mem = self.mem.regions();
        while self.idx < mem.len() {
            let r = mem[self.idx];
            self.idx += 1;
            let start_pfn = T::pfn_up(r.base(), self.page_size);
            let end_pfn = T::pfn_down(r.end(), self.page_size);
            if start_pfn < end_pfn {
                return Some((start_pfn, end_pfn));
            }
        }
        None
    }
}

/// Forward iterator over regions of `type_a` not covered by `type_b`,
/// sorted ascending.
///
/// If `type_b` is `None`, all regions of `type_a` are yielded. This is
/// the Rust counterpart of the kernel's `__for_each_mem_range`, from
/// which `for_each_mem_range` (`type_b = None`) and
/// `for_each_free_mem_range` (`type_b = reserved`) are derived.
///
/// Regions whose attributes are excluded by `flags` are skipped according
/// to [`RegionFlags::should_skip`]; with the default [`MemblockFlags`] set,
/// e.g. `NOMAP` regions are skipped unless `flags` contains
/// [`MemblockFlags::NOMAP`].
///
/// This is a [`DoubleEndedIterator`]: it can be iterated from both ends,
/// and `.rev()` yields the free ranges in descending order.
///
/// [`MemblockFlags::NOMAP`]: crate::flags::MemblockFlags::NOMAP
pub struct FreeMemRangeIter<'a, T: PhysAddr, const N: usize, F: RegionFlags = MemblockFlags> {
    flags: F,
    type_a: &'a MemblockType<T, N, F>,
    type_b: Option<&'a MemblockType<T, N, F>>,
    /// Index of the next `memory` region for forward iteration.
    m_lo: usize,
    /// Index of the next `memory` region for backward iteration
    /// (`isize`, `-1` when exhausted).
    m_hi: isize,
    /// Reserved-gap cursor for forward iteration. Gap `i` lies between
    /// `res[i - 1].end()` and `res[i].base()`, with sentinels at `0`
    /// (from `PhysAddr::ZERO`) and `res.len()` (up to `PhysAddr::MAX`).
    r_lo: usize,
    /// Reserved-gap cursor for backward iteration (`isize`, `-1` when
    /// exhausted).
    r_hi: isize,
    /// End of the highest piece consumed by the forward end; backward
    /// candidates below it are already taken.
    fwd_end: T,
    /// Base of the lowest piece consumed by the backward end; forward
    /// candidates above it are already taken.
    bwd_base: T,
}

impl<'a, T: PhysAddr, const N: usize, F: RegionFlags> FreeMemRangeIter<'a, T, N, F> {
    pub(crate) fn new(
        type_a: &'a MemblockType<T, N, F>,
        type_b: Option<&'a MemblockType<T, N, F>>,
        flags: F,
    ) -> Self {
        Self {
            m_lo: 0,
            m_hi: type_a.regions().len() as isize - 1,
            r_lo: 0,
            r_hi: type_b.map_or(0, |t| t.regions().len() as isize),
            flags,
            type_a,
            type_b,
            fwd_end: PhysAddr::ZERO,
            bwd_base: PhysAddr::MAX,
        }
    }
}

impl<'a, T: PhysAddr, const N: usize, F: RegionFlags> Iterator for FreeMemRangeIter<'a, T, N, F> {
    type Item = (T, T);

    fn next(&mut self) -> Option<Self::Item> {
        let mem = self.type_a.regions();
        let res = match self.type_b {
            Some(t) => t.regions(),
            None => &[],
        };

        while (self.m_lo as isize) <= self.m_hi {
            let m = mem[self.m_lo];
            let m_base = m.base();
            let m_end = m.end();

            if should_skip_region(m, self.flags) {
                self.m_lo += 1;
                continue;
            }

            if res.is_empty() {
                self.m_lo += 1;
                if m_base < self.bwd_base {
                    self.fwd_end = self.fwd_end.max(m_end);
                    return Some((m_base, m_end));
                }
                // Collided with pieces taken from the back end.
                return None;
            }

            while self.r_lo < res.len() + 1 {
                let r_base = if self.r_lo == 0 {
                    PhysAddr::ZERO
                } else {
                    res[self.r_lo - 1].end()
                };

                let r_end = if self.r_lo < res.len() {
                    res[self.r_lo].base()
                } else {
                    PhysAddr::MAX
                };

                if r_base >= m_end {
                    break;
                }

                if m_base < r_end {
                    let base = max(m_base, r_base);
                    let end = min(m_end, r_end);

                    if end > base {
                        if m_end <= r_end {
                            self.m_lo += 1;
                        } else {
                            self.r_lo += 1;
                        }

                        if end <= self.bwd_base {
                            self.fwd_end = self.fwd_end.max(end);
                            return Some((base, end));
                        }
                        // Everything up to `bwd_base` was consumed from
                        // the back end; this iterator is done.
                        return None;
                    }
                }

                // A zero-width overlap (adjacent reserved regions that
                // are not merged because of differing attributes) yields
                // nothing; the trailing increment moves past it.
                self.r_lo += 1;
            }

            self.m_lo += 1;
        }

        None
    }
}

impl<'a, T: PhysAddr, const N: usize, F: RegionFlags> DoubleEndedIterator
    for FreeMemRangeIter<'a, T, N, F>
{
    fn next_back(&mut self) -> Option<Self::Item> {
        let mem = self.type_a.regions();
        let res = match self.type_b {
            Some(t) => t.regions(),
            None => &[],
        };

        while (self.m_lo as isize) <= self.m_hi {
            let m = mem[self.m_hi as usize];
            let m_base = m.base();
            let m_end = m.end();

            if should_skip_region(m, self.flags) {
                self.m_hi -= 1;
                continue;
            }

            if res.is_empty() {
                self.m_hi -= 1;
                if m_end > self.fwd_end {
                    self.bwd_base = self.bwd_base.min(m_base);
                    return Some((m_base, m_end));
                }
                // Collided with pieces taken from the front end.
                return None;
            }

            while self.r_hi >= 0 {
                let ri = self.r_hi as usize;
                let r_base = if ri == 0 {
                    PhysAddr::ZERO
                } else {
                    res[ri - 1].end()
                };

                let r_end = if ri < res.len() {
                    res[ri].base()
                } else {
                    PhysAddr::MAX
                };

                if r_end <= m_base {
                    break;
                }

                if m_end > r_base {
                    let base = max(m_base, r_base);
                    let end = min(m_end, r_end);

                    if end > base {
                        if m_base >= r_base {
                            self.m_hi -= 1;
                        } else {
                            self.r_hi -= 1;
                        }

                        if base >= self.fwd_end {
                            self.bwd_base = self.bwd_base.min(base);
                            return Some((base, end));
                        }
                        // Everything from `fwd_end` up was consumed from
                        // the front end; this iterator is done.
                        return None;
                    }
                }

                // A zero-width overlap (adjacent reserved regions that
                // are not merged because of differing attributes) yields
                // nothing; the trailing decrement moves past it.
                self.r_hi -= 1;
            }

            self.m_hi -= 1;
        }

        None
    }
}

#[cfg(test)]
mod tests {
    extern crate alloc;
    use alloc::vec;
    use alloc::vec::Vec;

    use super::*;
    use crate::memblock::Memblock;

    fn mb_with(regions: &[(usize, usize, MemblockFlags)]) -> Memblock<usize, 8> {
        let mut mb = Memblock::<usize, 8>::new();
        for &(base, size, flags) in regions {
            mb.add(base, size, flags).unwrap();
        }
        mb
    }

    fn reserve(mb: &mut Memblock<usize, 8>, base: usize, size: usize) {
        mb.reserve(base, size).unwrap();
    }

    #[test]
    fn pfn_iter_basic() {
        let mb = mb_with(&[
            (0x1000, 0x1000, MemblockFlags::NONE),
            (0x4000, 0x2000, MemblockFlags::NONE),
        ]);
        let pfns: Vec<_> = PfnRangeIter::new(mb.memory(), 0x1000).collect();
        assert_eq!(pfns, vec![(1, 2), (4, 6)]);
    }

    #[test]
    fn pfn_iter_skips_regions_without_full_pages() {
        let mb = mb_with(&[
            (0x800, 0x2000, MemblockFlags::NONE),
            (0x4000, 0x800, MemblockFlags::NONE),
        ]);
        let pfns: Vec<_> = PfnRangeIter::new(mb.memory(), 0x1000).collect();
        assert_eq!(pfns, vec![(1, 2)]);
    }

    #[test]
    fn pfn_iter_empty() {
        let mb = Memblock::<usize, 8>::new();
        assert!(PfnRangeIter::new(mb.memory(), 0x1000).next().is_none());
    }

    #[test]
    fn pfn_iter_non_standard_page_size() {
        let mb = mb_with(&[(0x2000, 0x4000, MemblockFlags::NONE)]);
        let pfns: Vec<_> = PfnRangeIter::new(mb.memory(), 0x2000).collect();
        assert_eq!(pfns, vec![(1, 3)]);
    }

    #[test]
    fn free_mem_range_type_b_none_yields_all() {
        let mb = mb_with(&[
            (0x1000, 0x100, MemblockFlags::NONE),
            (0x2000, 0x100, MemblockFlags::NONE),
        ]);
        let ranges: Vec<_> =
            FreeMemRangeIter::new(mb.memory(), None, MemblockFlags::NONE).collect();
        assert_eq!(ranges, vec![(0x1000, 0x1100), (0x2000, 0x2100)]);
    }

    #[test]
    fn free_mem_range_type_b_none_reverse() {
        let mb = mb_with(&[
            (0x1000, 0x100, MemblockFlags::NONE),
            (0x2000, 0x100, MemblockFlags::NONE),
        ]);
        let ranges: Vec<_> = FreeMemRangeIter::new(mb.memory(), None, MemblockFlags::NONE)
            .rev()
            .collect();
        assert_eq!(ranges, vec![(0x2000, 0x2100), (0x1000, 0x1100)]);
    }

    #[test]
    fn free_mem_range_type_b_none_filters_flags() {
        let mb = mb_with(&[
            (0x1000, 0x100, MemblockFlags::NOMAP),
            (0x2000, 0x100, MemblockFlags::NONE),
        ]);
        let ranges: Vec<_> =
            FreeMemRangeIter::new(mb.memory(), None, MemblockFlags::NONE).collect();
        assert_eq!(ranges, vec![(0x2000, 0x2100)]);
        let ranges: Vec<_> =
            FreeMemRangeIter::new(mb.memory(), None, MemblockFlags::NOMAP).collect();
        assert_eq!(ranges, vec![(0x1000, 0x1100), (0x2000, 0x2100)]);
    }

    #[test]
    fn free_mem_range_empty_memory() {
        let mb = Memblock::<usize, 8>::new();
        assert!(
            FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE)
                .next()
                .is_none()
        );
        assert!(
            FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE)
                .next_back()
                .is_none()
        );
    }

    #[test]
    fn free_mem_range_single_reserved_split() {
        let mut mb = mb_with(&[(0x1000, 0x1000, MemblockFlags::NONE)]);
        reserve(&mut mb, 0x1800, 0x100);
        let mut it = FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE);
        assert_eq!(it.next(), Some((0x1000, 0x1800)));
        assert_eq!(it.next(), Some((0x1900, 0x2000)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn free_mem_range_double_ended_single_region() {
        let mut mb = mb_with(&[(0x0, 0x3000, MemblockFlags::NONE)]);
        reserve(&mut mb, 0x100, 0x100);
        reserve(&mut mb, 0x500, 0x100);

        let mut it = FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE);
        assert_eq!(it.next_back(), Some((0x600, 0x3000)));
        assert_eq!(it.next_back(), Some((0x200, 0x500)));
        assert_eq!(it.next_back(), Some((0x0, 0x100)));
        assert_eq!(it.next_back(), None);

        let mut it = FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE);
        assert_eq!(it.next(), Some((0x0, 0x100)));
        assert_eq!(it.next_back(), Some((0x600, 0x3000)));
        assert_eq!(it.next(), Some((0x200, 0x500)));
        assert_eq!(it.next(), None);
        assert_eq!(it.next_back(), None);
    }

    #[test]
    fn free_mem_range_partition_across_many_regions() {
        // Forward and backward consumption must partition all free pieces.
        let mut mb = Memblock::<usize, 8>::new();
        for base in [0x1000, 0x4000, 0x8000] {
            mb.add(base, 0x1000, MemblockFlags::NONE).unwrap();
        }
        reserve(&mut mb, 0x1400, 0x200);
        reserve(&mut mb, 0x4200, 0x100);

        let fwd: Vec<_> =
            FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE).collect();

        let mut it = FreeMemRangeIter::new(mb.memory(), Some(mb.reserved()), MemblockFlags::NONE);
        let mut front = Vec::new();
        let mut back = Vec::new();
        loop {
            let f = it.next();
            let b = it.next_back();
            if f.is_none() && b.is_none() {
                break;
            }
            if let Some(x) = f {
                front.push(x);
            }
            if let Some(x) = b {
                back.push(x);
            }
        }

        back.reverse();
        front.extend(back);
        assert_eq!(front, fwd);
    }
}
