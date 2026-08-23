//! Iterators over region ranges.
//!
//! - [`range`] mirrors the kernel's `__for_each_mem_range`: [`range::Iter`]
//!   iterates the regions of one memblock type that are not covered by
//!   another (e.g. `memory - reserved`). It implements
//!   [`DoubleEndedIterator`], so reverse iteration is obtained with
//!   [`Iterator::rev`] (e.g. for top-down allocation), just like any Rust
//!   iterator.
//! - [`pfn`] mirrors the kernel's `for_each_mem_pfn_range`: [`pfn::Iter`]
//!   iterates `memory` regions as page frame number ranges.

pub mod pfn {
    use crate::addr::PhysAddr;
    use crate::addr::pfn_down;
    use crate::addr::pfn_up;
    use crate::memblock::MemblockType;

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
    pub struct Iter<'a, T: PhysAddr, const N: usize> {
        idx: usize,
        page_size: T,
        mem: &'a MemblockType<T, N>,
    }

    impl<'a, T: PhysAddr, const N: usize> Iter<'a, T, N> {
        pub(crate) fn new(mem: &'a MemblockType<T, N>, page_size: T) -> Self {
            Self {
                idx: 0,
                page_size,
                mem,
            }
        }
    }

    impl<'a, T: PhysAddr, const N: usize> Iterator for Iter<'a, T, N> {
        type Item = (T, T);

        fn next(&mut self) -> Option<Self::Item> {
            let mem = self.mem.regions();
            while self.idx < mem.len() {
                let r = mem[self.idx];
                self.idx += 1;
                let start_pfn = pfn_up(r.base(), self.page_size);
                let end_pfn = pfn_down(r.end(), self.page_size);
                if start_pfn < end_pfn {
                    return Some((start_pfn, end_pfn));
                }
            }
            None
        }
    }
}

pub mod range {
    use core::cmp::max;
    use core::cmp::min;
    use core::iter::DoubleEndedIterator;

    use crate::addr::PhysAddr;
    use crate::flags::MemblockFlags;
    use crate::memblock::MemblockType;
    use crate::memblock::should_skip_region;

    /// Forward iterator over regions of `type_a` not covered by `type_b`,
    /// sorted ascending.
    ///
    /// If `type_b` is `None`, all regions of `type_a` are yielded. This is
    /// the Rust counterpart of the kernel's `__for_each_mem_range`, from
    /// which `for_each_mem_range` (`type_b = None`) and
    /// `for_each_free_mem_range` (`type_b = reserved`) are derived.
    ///
    /// Regions whose attributes are excluded by `flags` are skipped, e.g.
    /// `NOMAP` regions are skipped unless `flags` contains
    /// [`MemblockFlags::NOMAP`].
    ///
    /// This is a [`DoubleEndedIterator`]: it can be iterated from both ends,
    /// and `.rev()` yields the free ranges in descending order.
    ///
    /// [`MemblockFlags::NOMAP`]: crate::flags::MemblockFlags::NOMAP
    pub struct Iter<'a, T: PhysAddr, const N: usize> {
        flags: MemblockFlags,
        type_a: &'a MemblockType<T, N>,
        type_b: Option<&'a MemblockType<T, N>>,
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

    impl<'a, T: PhysAddr, const N: usize> Iter<'a, T, N> {
        pub(crate) fn new(
            type_a: &'a MemblockType<T, N>,
            type_b: Option<&'a MemblockType<T, N>>,
            flags: MemblockFlags,
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

    impl<'a, T: PhysAddr, const N: usize> Iterator for Iter<'a, T, N> {
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

                        if m_end <= r_end {
                            self.m_lo += 1;
                        } else {
                            self.r_lo += 1;
                        }

                        if end > base {
                            if end <= self.bwd_base {
                                self.fwd_end = self.fwd_end.max(end);
                                return Some((base, end));
                            }
                            // Everything up to `bwd_base` was consumed from
                            // the back end; this iterator is done.
                            return None;
                        }
                    }

                    self.r_lo += 1;
                }

                self.m_lo += 1;
            }

            None
        }
    }

    impl<'a, T: PhysAddr, const N: usize> DoubleEndedIterator for Iter<'a, T, N> {
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

                        if m_base >= r_base {
                            self.m_hi -= 1;
                        } else {
                            self.r_hi -= 1;
                        }

                        if end > base {
                            if base >= self.fwd_end {
                                self.bwd_base = self.bwd_base.min(base);
                                return Some((base, end));
                            }
                            // Everything from `fwd_end` up was consumed from
                            // the front end; this iterator is done.
                            return None;
                        }
                    }

                    self.r_hi -= 1;
                }

                self.m_hi -= 1;
            }

            None
        }
    }
}
