//! The core memblock data structures and algorithms.

use core::cmp::max;
use core::cmp::min;

use crate::addr::PhysAddr;
use crate::addr::align_down;
use crate::addr::align_up;
use crate::addr::cap_size;
use crate::addr::saturating_add;
use crate::error::Error;
use crate::flags::MemblockFlags;
use crate::iter;
use crate::iter::pfn;
use crate::iter::range;
use crate::region::MemblockRegion;

/// An ordered, fixed-capacity collection of memory regions.
///
/// Regions are kept sorted by `base` and non-overlapping; adjacent regions
/// with identical flags are merged automatically. `total_size` tracks the
/// sum of all region sizes.
#[derive(Clone, Debug)]
pub struct MemblockType<T: PhysAddr, const N: usize> {
    regions: [MemblockRegion<T>; N],
    cnt: usize,
    total_size: T,
}

impl<T: PhysAddr, const N: usize> PartialEq for MemblockType<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.regions[..self.cnt] == other.regions[..other.cnt]
            && self.total_size == other.total_size
    }
}

impl<T: PhysAddr, const N: usize> Eq for MemblockType<T, N> {}

/// The memblock allocator state, mirroring the kernel's `struct memblock`.
///
/// Holds two [`MemblockType`]s: `memory` (physical memory available to the
/// kernel) and `reserved` (memory already set aside), plus the allocation
/// policy: `bottom_up` selects the search direction and `current_limit` caps
/// the upper bound used by [`Memblock::phys_alloc`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Memblock<T: PhysAddr, const N: usize> {
    memory: MemblockType<T, N>,
    reserved: MemblockType<T, N>,
    bottom_up: bool,
    current_limit: T,
}

impl<T: PhysAddr, const N: usize> MemblockType<T, N> {
    fn move_regions(&mut self, src: usize, dst: usize, count: usize) -> Result<(), Error> {
        if count == 0 || src == dst {
            return Ok(());
        }
        if src + count > self.regions.len() || dst + count > self.regions.len() {
            return Err(Error::InternalError);
        }
        self.regions.copy_within(src..src + count, dst);
        Ok(())
    }

    fn merge_regions(&mut self) {
        if self.cnt <= 1 {
            return;
        }

        let mut i = 0;
        while i + 1 < self.cnt {
            let curr = self.regions[i];
            let next = self.regions[i + 1];

            if curr.end() == next.base() && curr.flags() == next.flags() {
                let merged = MemblockRegion::with_flags(
                    curr.base(),
                    curr.size() + next.size(),
                    curr.flags(),
                );
                self.move_regions(i + 1, i, self.cnt - i - 1).unwrap();
                self.regions[i] = merged;
                self.cnt -= 1;
                self.regions[self.cnt] = MemblockRegion::EMPTY;
            } else {
                i += 1;
            }
        }
    }

    fn insert_region(&mut self, pos: usize, region: MemblockRegion<T>) -> Result<(), Error> {
        if self.cnt >= self.regions.len() {
            return Err(Error::OverCapacity);
        }
        if pos > self.cnt {
            return Err(Error::InternalError);
        }
        if pos < self.cnt {
            self.move_regions(pos, pos + 1, self.cnt - pos).unwrap();
        }
        self.regions[pos] = region;
        self.cnt += 1;
        self.total_size = self.total_size + region.size();
        Ok(())
    }

    fn remove_region(&mut self, r: usize) {
        self.total_size = self.total_size - self.regions[r].size();
        self.regions.copy_within(r + 1..self.cnt, r);
        self.cnt -= 1;
        self.regions[self.cnt] = MemblockRegion::EMPTY;
    }

    /// Adds the range `[base, base + size)` to this type, tagged with `flags`.
    ///
    /// Overlapping parts of existing regions are left untouched; only the
    /// non-overlapping gaps are added. Adjacent regions with the same flags
    /// are merged afterwards. A zero `size` is a no-op.
    ///
    /// Mirrors the kernel's `memblock_add_range`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OverCapacity`] if the fixed region array has no room
    /// left.
    pub fn add(&mut self, base: T, size: T, flags: MemblockFlags) -> Result<(), Error> {
        let size = cap_size(base, size);
        if size == PhysAddr::ZERO {
            return Ok(());
        }

        if self.cnt == 0 {
            self.insert_region(0, MemblockRegion::with_flags(base, size, flags))?;
            return Ok(());
        }

        let end = base + size;

        let mut do_insert = false;
        if self.regions.len() > self.cnt * 2 {
            do_insert = true;
        }

        loop {
            let mut i = 0;
            let mut nr_new = 0;
            let mut cur_base = base;

            while i < self.cnt {
                let rbase = self.regions[i].base();
                let rend = self.regions[i].end();

                if end <= rbase {
                    break;
                }

                if cur_base >= rend {
                    i += 1;
                    continue;
                }

                if cur_base < rbase {
                    nr_new += 1;
                    if do_insert {
                        let sz = rbase - cur_base;
                        self.insert_region(i, MemblockRegion::with_flags(cur_base, sz, flags))?;
                        i += 1;
                    }
                }

                cur_base = min(end, rend);
                i += 1;
            }

            if end > cur_base {
                nr_new += 1;
                if do_insert {
                    let sz = end - cur_base;
                    self.insert_region(i, MemblockRegion::with_flags(cur_base, sz, flags))?;
                }
            }

            if !do_insert {
                if nr_new <= self.regions.len() - self.cnt {
                    do_insert = true;
                } else {
                    return Err(Error::OverCapacity);
                }
            } else {
                self.merge_regions();
                break;
            }
        }

        Ok(())
    }

    fn isolate_range(&mut self, base: T, size: T) -> Result<(usize, usize), Error> {
        let size = cap_size(base, size);
        if size == PhysAddr::ZERO {
            return Ok((0, 0));
        }
        if self.cnt + 2 > self.regions.len() {
            return Err(Error::OverCapacity);
        }

        let end = base + size;
        let mut start_rgn = 0;
        let mut end_rgn = 0;

        let mut idx = 0;
        while idx < self.cnt {
            let r = self.regions[idx];
            let rbase = r.base();
            let rend = r.end();
            let rflags = r.flags();

            if rbase >= end {
                break;
            }
            if rend <= base {
                idx += 1;
                continue;
            }

            if rbase < base {
                self.regions[idx] = MemblockRegion::with_flags(base, rend - base, rflags);
                self.total_size = self.total_size - (base - rbase);
                self.insert_region(idx, MemblockRegion::with_flags(rbase, base - rbase, rflags))?;
                idx += 1;
            } else if rend > end {
                self.regions[idx] = MemblockRegion::with_flags(end, rend - end, rflags);
                self.total_size = self.total_size - (end - rbase);
                self.insert_region(idx, MemblockRegion::with_flags(rbase, end - rbase, rflags))?;
            } else {
                if end_rgn == 0 {
                    start_rgn = idx;
                }
                end_rgn = idx + 1;
                idx += 1;
            }
        }

        Ok((start_rgn, end_rgn))
    }

    /// Removes the range `[base, base + size)` from this type.
    ///
    /// The range may span multiple regions or overlap them partially; regions
    /// are split at the boundaries as needed. Removing a range that is not
    /// present is a no-op.
    ///
    /// Mirrors the kernel's `memblock_remove_range`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OverCapacity`] if splitting regions would overflow the
    /// fixed region array.
    pub fn remove(&mut self, base: T, size: T) -> Result<(), Error> {
        let (start_rgn, end_rgn) = self.isolate_range(base, size)?;
        for i in (start_rgn..end_rgn).rev() {
            self.remove_region(i);
        }
        Ok(())
    }

    /// Returns the number of regions currently stored.
    pub fn count(&self) -> usize {
        self.cnt
    }

    /// Returns `true` if this type holds no regions.
    pub fn is_empty(&self) -> bool {
        self.cnt == 0
    }

    /// Returns the fixed capacity of the region array.
    pub fn capacity(&self) -> usize {
        self.regions.len()
    }

    /// Returns the currently stored regions, sorted by `base`.
    pub fn regions(&self) -> &[MemblockRegion<T>] {
        &self.regions[..self.cnt]
    }

    /// Returns the total size of all stored regions.
    pub fn total_size(&self) -> T {
        self.total_size
    }

    /// Returns the index of the region containing `addr`, if any.
    pub fn search(&self, addr: T) -> Option<usize> {
        let mut lo = 0;
        let mut hi = self.cnt;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let r = &self.regions[mid];
            if addr < r.base() {
                hi = mid;
            } else if addr >= r.end() {
                lo = mid + 1;
            } else {
                return Some(mid);
            }
        }
        None
    }

    /// Returns `true` if the range `[base, base + size)` overlaps any stored
    /// region. A zero `size` never overlaps.
    pub fn overlaps_region(&self, base: T, size: T) -> bool {
        let size = cap_size(base, size);
        if size == PhysAddr::ZERO {
            return false;
        }
        let end = base + size;
        for i in 0..self.cnt {
            let r = &self.regions[i];
            if r.base() >= end {
                break;
            }
            if base < r.end() {
                return true;
            }
        }
        false
    }

    pub(crate) fn set_flag(
        &mut self,
        base: T,
        size: T,
        flag: MemblockFlags,
        set: bool,
    ) -> Result<(), Error> {
        let (start_rgn, end_rgn) = self.isolate_range(base, size)?;
        for i in start_rgn..end_rgn {
            let r = self.regions[i];
            let new_flags = if set {
                r.flags() | flag
            } else {
                r.flags() & !flag
            };
            self.regions[i] = MemblockRegion::with_flags(r.base(), r.size(), new_flags);
        }
        self.merge_regions();
        Ok(())
    }
}

impl<T: PhysAddr, const N: usize> Memblock<T, N> {
    /// Creates an empty memblock with no `memory` or `reserved` regions.
    pub const fn new() -> Self {
        Self {
            memory: MemblockType {
                regions: [MemblockRegion::EMPTY; N],
                cnt: 0,
                total_size: PhysAddr::ZERO,
            },
            reserved: MemblockType {
                regions: [MemblockRegion::EMPTY; N],
                cnt: 0,
                total_size: PhysAddr::ZERO,
            },
            bottom_up: false,
            current_limit: PhysAddr::MAX,
        }
    }
}

impl<T: PhysAddr, const N: usize> Default for Memblock<T, N> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: PhysAddr, const N: usize> Memblock<T, N> {
    /// Adds the range `[base, base + size)` to `memory`.
    ///
    /// See [`MemblockType::add`].
    pub fn add(&mut self, base: T, size: T, flags: MemblockFlags) -> Result<(), Error> {
        self.memory.add(base, size, flags)
    }

    /// Removes the range `[base, base + size)` from `memory`.
    ///
    /// See [`MemblockType::remove`].
    pub fn remove(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory.remove(base, size)
    }

    /// Adds the range `[base, base + size)` to `reserved`.
    ///
    /// Mirrors the kernel's `memblock_reserve`, which records the region
    /// without special attributes.
    pub fn reserve(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved.add(base, size, MemblockFlags::NONE)
    }

    /// Adds the range `[base, base + size)` to `reserved` and marks it
    /// reserved for kernel use.
    ///
    /// Mirrors the kernel's `memblock_reserve_kern`; the allocation APIs
    /// use this internally so that every memblock allocation carries
    /// [`MemblockFlags::RSRV_KERN`].
    pub fn reserve_kern(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved.add(base, size, MemblockFlags::RSRV_KERN)
    }

    /// Returns the base address of the first `memory` region, if any.
    ///
    /// Mirrors the kernel's `memblock_start_of_DRAM`.
    pub fn memory_base(&self) -> Option<T> {
        if self.memory.cnt > 0 {
            Some(self.memory.regions[0].base())
        } else {
            None
        }
    }

    /// Finds a free (unreserved) region of `size` bytes aligned to `align`
    /// within `[start, end]`.
    ///
    /// The search direction follows [`Memblock::bottom_up`]. Regions are
    /// filtered by attribute: `NOMAP` and `DRIVER_MANAGED` regions are
    /// skipped unless the corresponding flag is present in `flags`, while
    /// `MIRROR` and `KHO_SCRATCH` regions are only considered when
    /// explicitly requested. The range is treated as clamped to
    /// `[start, end]`.
    ///
    /// Mirrors the kernel's `memblock_find_in_range_node` (minus the NUMA
    /// node selector). Note that unlike the allocation APIs, the result is
    /// *not* reserved; combine with [`Memblock::reserve`] to keep it.
    ///
    /// # Panics
    ///
    /// Panics if `align` is zero or not a power of two.
    pub fn find_in_range_node(
        &self,
        size: T,
        align: T,
        start: T,
        end: T,
        flags: MemblockFlags,
    ) -> Option<(T, T)> {
        if self.bottom_up {
            for (r_start, r_end) in self.free_mem_ranges(flags) {
                let this_start = min(max(r_start, start), end);
                let this_end = min(max(r_end, start), end);

                if this_start >= this_end {
                    continue;
                }

                let cand = align_up(this_start, align);
                if cand < this_end && this_end - cand >= size {
                    return Some((cand, cand + size));
                }
            }
        } else {
            for (r_start, r_end) in self.free_mem_ranges(flags).rev() {
                let this_start = min(max(r_start, start), end);
                let this_end = min(max(r_end, start), end);

                if this_end < size {
                    continue;
                }

                let cand = align_down(this_end - size, align);
                if cand >= this_start {
                    return Some((cand, cand + size));
                }
            }
        }

        None
    }

    /// Finds a free (unreserved) region of `size` bytes aligned to `align`
    /// within `[start, end]`, without attribute-based filtering.
    ///
    /// Mirrors the kernel's `memblock_find_in_range`.
    ///
    /// # Panics
    ///
    /// Panics if `align` is zero or not a power of two.
    pub fn find_in_range(&self, start: T, end: T, size: T, align: T) -> Option<(T, T)> {
        self.find_in_range_node(size, align, start, end, MemblockFlags::NONE)
    }

    /// Allocates `size` bytes of aligned free memory within `[start, end]`.
    ///
    /// The search direction follows [`Memblock::bottom_up`] and the upper
    /// bound is clamped to [`Memblock::current_limit`], mirroring
    /// `memblock_alloc_internal`. The result is added to `reserved` with
    /// [`MemblockFlags::RSRV_KERN`].
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfMemory`] if no suitable region exists, or
    /// [`Error::OverCapacity`] if reserving it would overflow the region
    /// array.
    fn alloc_internal(
        &mut self,
        size: T,
        align: T,
        start: T,
        end: T,
        flags: MemblockFlags,
    ) -> Result<T, Error> {
        if size == PhysAddr::ZERO {
            return Err(Error::OutOfMemory);
        }
        let limit = min(end, self.current_limit);
        let (base, found_end) = self
            .find_in_range_node(size, align, start, limit, flags)
            .ok_or(Error::OutOfMemory)?;
        self.reserve_kern(base, found_end - base)?;
        Ok(base)
    }

    /// Allocates `size` bytes of free memory aligned to `align` within
    /// `[start, end]`.
    ///
    /// The search direction follows [`Memblock::bottom_up`] and the upper
    /// bound is clamped to [`Memblock::current_limit`], mirroring the
    /// kernel's `memblock_phys_alloc_range`. The result is added to
    /// `reserved` with [`MemblockFlags::RSRV_KERN`]; the `flags` argument
    /// only filters which `memory` regions may be allocated from (see
    /// [`Memblock::find_in_range_node`] for the filtering rules).
    ///
    /// # Panics
    ///
    /// Panics if `align` is zero or not a power of two.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfMemory`] if no suitable free region exists, or
    /// [`Error::OverCapacity`] if the region array is full.
    pub fn phys_alloc_range(
        &mut self,
        size: T,
        align: T,
        start: T,
        end: T,
        flags: MemblockFlags,
    ) -> Result<T, Error> {
        self.alloc_internal(size, align, start, end, flags)
    }

    /// Returns the allocation direction used by all allocation functions.
    ///
    /// Mirrors the kernel's `memblock_bottom_up`. Defaults to `false`
    /// (top-down).
    pub fn bottom_up(&self) -> bool {
        self.bottom_up
    }

    /// Sets the allocation direction used by all allocation functions.
    ///
    /// Mirrors the kernel's `memblock_set_bottom_up`.
    pub fn set_bottom_up(&mut self, bottom_up: bool) {
        self.bottom_up = bottom_up;
    }

    /// Returns the upper bound used by the allocation functions.
    ///
    /// The allocation end is clamped to this value (see
    /// [`Memblock::set_current_limit`]). Mirrors the kernel's
    /// `memblock.current_limit`. `T::MAX` (the default) means no limit.
    pub fn current_limit(&self) -> T {
        self.current_limit
    }

    /// Sets the upper bound used by the allocation functions.
    ///
    /// All allocations, including those with an explicit `end`, are clamped
    /// to this limit. Mirrors the kernel's `memblock_set_current_limit`.
    pub fn set_current_limit(&mut self, limit: T) {
        self.current_limit = limit;
    }

    /// Allocates `size` bytes of free memory aligned to `align`, using the
    /// configured [`Memblock::current_limit`] as the upper bound and
    /// [`Memblock::bottom_up`] as the search direction.
    ///
    /// Equivalent to
    /// [`phys_alloc_range(size, align, T::ZERO, T::MAX, flags)`](Memblock::phys_alloc_range).
    /// The result is added to `reserved` with [`MemblockFlags::RSRV_KERN`];
    /// the `flags` argument only filters which `memory` regions may be
    /// allocated from.
    ///
    /// Mirrors the kernel's `memblock_phys_alloc`.
    ///
    /// # Panics
    ///
    /// Panics if `align` is zero or not a power of two.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OutOfMemory`] if `size` is zero or no suitable free
    /// region exists, or [`Error::OverCapacity`] if the region array is full.
    pub fn phys_alloc(&mut self, size: T, align: T, flags: MemblockFlags) -> Result<T, Error> {
        self.alloc_internal(size, align, PhysAddr::ZERO, PhysAddr::MAX, flags)
    }

    /// Releases the reserved block `[base, base + size)`.
    ///
    /// The range is removed from `reserved`. Removing a range that is not
    /// present is a no-op.
    ///
    /// Mirrors the kernel's `memblock_phys_free`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::OverCapacity`] if splitting regions would overflow the
    /// region array.
    pub fn phys_free(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved.remove(base, size)
    }

    /// Iterates over the free memory ranges, ascending.
    ///
    /// A range is free when it belongs to `memory` but not to `reserved`,
    /// i.e. `memory - reserved`. Mirrors the kernel's
    /// `for_each_free_mem_range`. Regions whose attributes are excluded by
    /// `flags` are skipped, e.g. pass [`MemblockFlags::NOMAP`] to include NOMAP
    /// regions.
    ///
    /// [`MemblockFlags::NOMAP`]: crate::flags::MemblockFlags::NOMAP
    pub fn free_mem_ranges(&self, flags: MemblockFlags) -> iter::range::Iter<'_, T, N> {
        range::Iter::new(&self.memory, Some(&self.reserved), flags)
    }

    /// Iterates over the `memory` regions as page frame number (PFN) ranges
    /// `[start_pfn, end_pfn)`.
    ///
    /// Mirrors the kernel's `for_each_mem_pfn_range`. `page_size` is the size
    /// of a page. Regions that contain no full page are skipped.
    ///
    /// # Panics
    ///
    /// Panics if `page_size` is zero.
    pub fn mem_pfn_ranges(&self, page_size: T) -> iter::pfn::Iter<'_, T, N> {
        assert!(page_size != PhysAddr::ZERO, "page_size must be non-zero");
        pfn::Iter::new(&self.memory, page_size)
    }

    /// Returns the reserved region collection.
    pub fn reserved(&self) -> &MemblockType<T, N> {
        &self.reserved
    }

    /// Returns the memory region collection.
    pub fn memory(&self) -> &MemblockType<T, N> {
        &self.memory
    }

    /// Returns the total size of all `memory` regions.
    ///
    /// Mirrors the kernel's `memblock_phys_mem_size`.
    pub fn phys_mem_size(&self) -> T {
        self.memory.total_size
    }

    /// Returns the total size of all `reserved` regions.
    ///
    /// Mirrors the kernel's `memblock_reserved_size`.
    pub fn reserved_size(&self) -> T {
        self.reserved.total_size
    }

    /// Returns the end address of the last `memory` region, if any.
    ///
    /// Mirrors the kernel's `memblock_end_of_DRAM`.
    pub fn memory_end(&self) -> Option<T> {
        self.memory.regions().last().map(|r| r.end())
    }

    /// Returns the size of the `memory` region containing `addr`, or zero if
    /// `addr` is not in any region.
    pub fn region_size(&self, addr: T) -> T {
        match self.memory.search(addr) {
            Some(i) => self.memory.regions[i].size(),
            None => PhysAddr::ZERO,
        }
    }

    /// Returns `true` if `addr` lies within a `memory` region.
    pub fn is_memory(&self, addr: T) -> bool {
        self.memory.search(addr).is_some()
    }

    /// Returns `true` if `addr` lies within a `reserved` region.
    pub fn is_reserved(&self, addr: T) -> bool {
        self.reserved.search(addr).is_some()
    }

    /// Returns `true` if the range `[base, base + size)` is fully contained in
    /// a single `memory` region.
    pub fn is_region_memory(&self, base: T, size: T) -> bool {
        match self.memory.search(base) {
            Some(i) => saturating_add(base, size) <= self.memory.regions[i].end(),
            None => false,
        }
    }

    /// Returns `true` if the range `[base, base + size)` intersects a
    /// `reserved` region.
    ///
    /// Mirrors the kernel's `memblock_is_region_reserved`, which checks for
    /// an intersection rather than full containment.
    pub fn is_region_reserved(&self, base: T, size: T) -> bool {
        self.reserved.overlaps_region(base, size)
    }

    /// Marks the `memory` range `[base, base + size)` as hotpluggable.
    ///
    /// Mirrors the kernel's `memblock_mark_hotplug`.
    pub fn mark_hotplug(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::HOTPLUG, true)
    }

    /// Clears the hotpluggable attribute on the `memory` range
    /// `[base, base + size)`.
    ///
    /// Mirrors the kernel's `memblock_clear_hotplug`.
    pub fn clear_hotplug(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::HOTPLUG, false)
    }

    /// Marks the `memory` range `[base, base + size)` as mirrored.
    ///
    /// Mirrors the kernel's `memblock_mark_mirror`.
    pub fn mark_mirror(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::MIRROR, true)
    }

    /// Clears the mirrored attribute on the `memory` range
    /// `[base, base + size)`.
    ///
    /// Mirrors the kernel's `memblock_clear_mirror`.
    pub fn clear_mirror(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::MIRROR, false)
    }

    /// Marks the `memory` range `[base, base + size)` as not mapped into the
    /// kernel direct mapping.
    ///
    /// Mirrors the kernel's `memblock_mark_nomap`.
    pub fn mark_nomap(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory.set_flag(base, size, MemblockFlags::NOMAP, true)
    }

    /// Clears the NOMAP attribute on the `memory` range `[base, base + size)`.
    ///
    /// Mirrors the kernel's `memblock_clear_nomap`.
    pub fn clear_nomap(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::NOMAP, false)
    }

    /// Marks the `memory` range `[base, base + size)` as scratch memory for
    /// kexec handover.
    ///
    /// Mirrors the kernel's `memblock_mark_kho_scratch`.
    pub fn mark_kho_scratch(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::KHO_SCRATCH, true)
    }

    /// Clears the KHO scratch attribute on the `memory` range
    /// `[base, base + size)`.
    ///
    /// Mirrors the kernel's `memblock_clear_kho_scratch`.
    pub fn clear_kho_scratch(&mut self, base: T, size: T) -> Result<(), Error> {
        self.memory
            .set_flag(base, size, MemblockFlags::KHO_SCRATCH, false)
    }

    /// Marks the `reserved` range `[base, base + size)` so that its struct
    /// pages are not initialized.
    ///
    /// Mirrors the kernel's `memblock_reserved_mark_noinit`.
    pub fn reserved_mark_noinit(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved
            .set_flag(base, size, MemblockFlags::RSRV_NOINIT, true)
    }

    /// Clears the no-init attribute on the `reserved` range
    /// `[base, base + size)`.
    pub fn reserved_clear_noinit(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved
            .set_flag(base, size, MemblockFlags::RSRV_NOINIT, false)
    }

    /// Marks the `reserved` range `[base, base + size)` as reserved for kernel
    /// use.
    ///
    /// Mirrors the kernel's `memblock_reserved_mark_kern`.
    pub fn reserved_mark_kern(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved
            .set_flag(base, size, MemblockFlags::RSRV_KERN, true)
    }

    /// Clears the kernel-use attribute on the `reserved` range
    /// `[base, base + size)`.
    pub fn reserved_clear_kern(&mut self, base: T, size: T) -> Result<(), Error> {
        self.reserved
            .set_flag(base, size, MemblockFlags::RSRV_KERN, false)
    }
}

/// Returns `true` if a `memory` region's attributes are excluded by `flags`.
///
/// `NOMAP` and `DRIVER_MANAGED` regions are skipped unless the corresponding
/// flag is present in `flags`; `MIRROR` and `KHO_SCRATCH` regions are only
/// iterated when explicitly requested.
///
/// Mirrors the kernel's `should_skip_region` applied to `memblock.memory`.
pub(crate) fn should_skip_region<T: PhysAddr>(r: MemblockRegion<T>, flags: MemblockFlags) -> bool {
    if !flags.contains(MemblockFlags::NOMAP) && r.flags().contains(MemblockFlags::NOMAP) {
        return true;
    }
    if !flags.contains(MemblockFlags::DRIVER_MANAGED)
        && r.flags().contains(MemblockFlags::DRIVER_MANAGED)
    {
        return true;
    }
    if flags.contains(MemblockFlags::MIRROR) && !r.flags().contains(MemblockFlags::MIRROR) {
        return true;
    }
    if flags.contains(MemblockFlags::KHO_SCRATCH) && !r.flags().contains(MemblockFlags::KHO_SCRATCH)
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::addr::addrs_overlap;
    use crate::flags::MemblockFlags;

    fn dump(mb: &Memblock<usize, 8>) -> [(usize, usize, u8); 8] {
        let mut out = [(0usize, 0usize, 0u8); 8];
        for (i, r) in mb.memory.regions().iter().enumerate() {
            out[i] = (r.base(), r.size(), r.flags().bits());
        }
        out
    }

    #[test]
    fn add_merges_adjacent_same_flags() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x100, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x200, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(&dump(&mb)[..1], &[(0x100, 0x200, 0)]);
    }

    #[test]
    fn add_does_not_merge_different_flags() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x0, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x100, 0x100, MemblockFlags::NOMAP).unwrap();
        assert_eq!(
            &dump(&mb)[..2],
            &[(0x0, 0x100, 0), (0x100, 0x100, MemblockFlags::NOMAP.bits())]
        );
    }

    #[test]
    fn add_gap_before_existing() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x100, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x50, 0x50, MemblockFlags::NONE).unwrap();
        assert_eq!(&dump(&mb)[..2], &[(0x50, 0x50, 0), (0x100, 0x100, 0)]);
    }

    #[test]
    fn add_wraps_existing_region() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x100, 0x100, MemblockFlags::NOMAP).unwrap();
        mb.add(0x0, 0x300, MemblockFlags::NONE).unwrap();
        assert_eq!(
            &dump(&mb)[..3],
            &[
                (0x0, 0x100, 0),
                (0x100, 0x100, MemblockFlags::NOMAP.bits()),
                (0x200, 0x100, 0)
            ]
        );
    }

    #[test]
    fn remove_middle_split() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x0, 0x1000, MemblockFlags::NONE).unwrap();
        mb.remove(0x400, 0x200).unwrap();
        assert_eq!(&dump(&mb)[..2], &[(0x0, 0x400, 0), (0x600, 0xa00, 0)]);
    }

    #[test]
    fn remove_across_regions() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x0, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x200, 0x100, MemblockFlags::NONE).unwrap();
        mb.remove(0x50, 0x200).unwrap();
        assert_eq!(&dump(&mb)[..2], &[(0x0, 0x50, 0), (0x250, 0xb0, 0)]);
    }

    #[test]
    fn remove_absent_is_noop() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.remove(0x0, 0x100).unwrap();
        assert_eq!(&dump(&mb)[..1], &[(0x1000, 0x100, 0)]);
    }

    #[test]
    fn alloc_reserves() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        let p = mb.phys_alloc(0x100, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(p, 0x1f00);
        assert_eq!(mb.memory_base(), Some(0x1000));
        assert_eq!(mb.reserved().count(), 1);
    }

    #[test]
    fn alloc_free_roundtrip() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        let p = mb.phys_alloc(0x100, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(p, 0x1f00);
        mb.phys_free(p, 0x100).unwrap();
        assert!(mb.reserved().is_empty());
        assert_eq!(mb.reserved_size(), 0);
    }

    #[test]
    fn alloc_zero_size_fails() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        assert!(matches!(
            mb.phys_alloc(0, 0x100, MemblockFlags::NONE),
            Err(Error::OutOfMemory)
        ));
    }

    #[test]
    fn phys_alloc_range_top_down() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        let p = mb
            .phys_alloc_range(0x100, 0x100, 0, usize::MAX, MemblockFlags::NONE)
            .unwrap();
        assert_eq!(p, 0x1f00);
    }

    #[test]
    fn phys_alloc_range_respects_bounds() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        let p = mb
            .phys_alloc_range(0x100, 0x100, 0, 0x1500, MemblockFlags::NONE)
            .unwrap();
        assert_eq!(p, 0x1400);
    }

    #[test]
    fn phys_alloc_range_follows_bottom_up() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.set_bottom_up(true);
        let p = mb
            .phys_alloc_range(0x100, 0x100, 0, usize::MAX, MemblockFlags::NONE)
            .unwrap();
        assert_eq!(p, 0x1000);
    }

    #[test]
    fn phys_alloc_range_respects_current_limit() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.set_current_limit(0x1500);
        let p = mb
            .phys_alloc_range(0x100, 0x100, 0, usize::MAX, MemblockFlags::NONE)
            .unwrap();
        assert_eq!(p, 0x1400);
    }

    #[test]
    fn alloc_skips_nomap_by_default() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NOMAP).unwrap();
        mb.add(0x2000, 0x1000, MemblockFlags::NONE).unwrap();
        let p = mb
            .phys_alloc_range(0x100, 0x100, 0, usize::MAX, MemblockFlags::NONE)
            .unwrap();
        assert_eq!(p, 0x2f00);
    }

    #[test]
    fn alloc_from_nomap_when_requested() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NOMAP).unwrap();
        let p = mb
            .phys_alloc_range(0x100, 0x100, 0, usize::MAX, MemblockFlags::NOMAP)
            .unwrap();
        assert_eq!(p, 0x1f00);
        assert!(matches!(
            mb.phys_alloc_range(0x100, 0x100, 0, usize::MAX, MemblockFlags::NONE),
            Err(Error::OutOfMemory)
        ));
    }

    #[test]
    fn alloc_out_of_memory() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        assert!(matches!(
            mb.phys_alloc_range(0x1000, 0x100, 0, usize::MAX, MemblockFlags::NONE),
            Err(Error::OutOfMemory)
        ));
    }

    #[test]
    fn free_mem_ranges_rev_order() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2000, 0x100, MemblockFlags::NONE).unwrap();
        let mut it = mb.free_mem_ranges(MemblockFlags::NONE).rev();
        let a = it.next().unwrap();
        let b = it.next().unwrap();
        assert_eq!((a.0, a.1), (0x2000, 0x2100));
        assert_eq!((b.0, b.1), (0x1000, 0x1100));
        assert!(it.next().is_none());
    }

    #[test]
    fn free_mem_ranges_is_double_ended() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.reserve(0x1800, 0x100).unwrap();

        // Interleaved consumption from both ends must partition the pieces.
        let mut it = mb.free_mem_ranges(MemblockFlags::NONE);
        assert_eq!(it.next(), Some((0x1000, 0x1800)));
        assert_eq!(it.next_back(), Some((0x1900, 0x2000)));
        assert_eq!(it.next(), None);
        assert_eq!(it.next_back(), None);

        // rev() yields everything, descending.
        let mut it_fwd = mb.free_mem_ranges(MemblockFlags::NONE);
        let f0 = it_fwd.next().unwrap();
        let f1 = it_fwd.next().unwrap();
        assert!(it_fwd.next().is_none());

        let mut it_rev = mb.free_mem_ranges(MemblockFlags::NONE).rev();
        assert_eq!(it_rev.next(), Some(f1));
        assert_eq!(it_rev.next(), Some(f0));
        assert!(it_rev.next().is_none());
    }

    #[test]
    fn free_mem_ranges_both_ends_meet_inside_one_region() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x0, 0x3000, MemblockFlags::NONE).unwrap();
        mb.reserve(0x100, 0x100).unwrap();
        mb.reserve(0x500, 0x100).unwrap();
        // Free pieces: [0x0,0x100), [0x200,0x500), [0x600,0x3000).

        // Purely from the back.
        let mut it = mb.free_mem_ranges(MemblockFlags::NONE);
        assert_eq!(it.next_back(), Some((0x600, 0x3000)));
        assert_eq!(it.next_back(), Some((0x200, 0x500)));
        assert_eq!(it.next_back(), Some((0x0, 0x100)));
        assert_eq!(it.next_back(), None);

        // Alternating ends must not duplicate or skip anything.
        let mut it = mb.free_mem_ranges(MemblockFlags::NONE);
        assert_eq!(it.next(), Some((0x0, 0x100)));
        assert_eq!(it.next_back(), Some((0x600, 0x3000)));
        assert_eq!(it.next(), Some((0x200, 0x500)));
        assert_eq!(it.next(), None);
        assert_eq!(it.next_back(), None);
    }

    #[test]
    fn queries() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2000, 0x200, MemblockFlags::NONE).unwrap();
        mb.reserve(0x1050, 0x20).unwrap();

        assert_eq!(mb.phys_mem_size(), 0x300);
        assert_eq!(mb.reserved_size(), 0x20);
        assert_eq!(mb.memory_base(), Some(0x1000));
        assert_eq!(mb.memory_end(), Some(0x2200));
        assert_eq!(mb.region_size(0x1000), 0x100);
        assert_eq!(mb.region_size(0x2100), 0x200);
        assert_eq!(mb.region_size(0x500), 0);

        assert!(mb.is_memory(0x1050));
        assert!(!mb.is_memory(0x500));
        assert!(mb.is_reserved(0x1060));
        assert!(!mb.is_reserved(0x1000));
        assert!(mb.is_region_memory(0x1000, 0x100));
        assert!(!mb.is_region_memory(0x1000, 0x101));
        assert!(mb.is_region_reserved(0x1050, 0x20));
        // Kernel semantics: intersects, not fully contained.
        assert!(mb.is_region_reserved(0x1040, 0x20));
        assert!(mb.is_region_reserved(0x1060, 0x20));
        assert!(!mb.is_region_reserved(0x1070, 0x20));
        assert!(mb.memory.overlaps_region(0x1000, 0x100));
        assert!(!mb.memory.overlaps_region(0x1300, 0x100));
        assert!(addrs_overlap(0x0usize, 0x100, 0x80, 0x100));
        assert!(!addrs_overlap(0x0usize, 0x100, 0x100, 0x100));
    }

    #[test]
    fn find_in_range_matches_kernel_names() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();

        let (base, end) = mb.find_in_range(0, usize::MAX, 0x100, 0x100).unwrap();
        assert_eq!((base, end), (0x1f00, 0x2000));

        mb.set_bottom_up(true);
        let (base, _) = mb
            .find_in_range_node(0x100, 0x100, 0, usize::MAX, MemblockFlags::NOMAP)
            .unwrap();
        assert_eq!(base, 0x1000);

        // find_in_range_node does not reserve; the caller does.
        assert!(mb.reserved().is_empty());
    }

    #[test]
    fn reserve_kern_sets_flag() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.reserve_kern(0x1000, 0x40).unwrap();
        assert!(
            mb.reserved().regions()[0]
                .flags()
                .contains(MemblockFlags::RSRV_KERN)
        );
    }

    #[test]
    fn mark_and_clear_nomap() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x0, 0x100, MemblockFlags::NONE).unwrap();
        mb.mark_nomap(0x40, 0x80).unwrap();
        assert_eq!(
            &dump(&mb)[..3],
            &[
                (0x0, 0x40, 0),
                (0x40, 0x80, MemblockFlags::NOMAP.bits()),
                (0xc0, 0x40, 0)
            ]
        );
        mb.clear_nomap(0x40, 0x80).unwrap();
        assert_eq!(&dump(&mb)[..1], &[(0x0, 0x100, 0)]);
    }

    #[test]
    fn mark_flags_preserved_across_remove() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x0, 0x100, MemblockFlags::NONE).unwrap();
        mb.mark_nomap(0x40, 0x80).unwrap();
        mb.remove(0x20, 0x10).unwrap();
        assert_eq!(
            &dump(&mb)[..4],
            &[
                (0x0, 0x20, 0),
                (0x30, 0x10, 0),
                (0x40, 0x80, MemblockFlags::NOMAP.bits()),
                (0xc0, 0x40, 0)
            ]
        );
    }

    #[test]
    fn total_size_after_add_remove() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        assert_eq!(mb.phys_mem_size(), 0x1000);
        mb.remove(0x1400, 0x200).unwrap();
        assert_eq!(mb.phys_mem_size(), 0xe00);
    }

    #[test]
    fn mem_pfn_ranges_basic() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.add(0x4000, 0x2000, MemblockFlags::NONE).unwrap();
        let mut it = mb.mem_pfn_ranges(0x1000);
        assert_eq!(it.next(), Some((1, 2)));
        assert_eq!(it.next(), Some((4, 6)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn mem_pfn_ranges_skips_partial_pages() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x800, 0x2000, MemblockFlags::NONE).unwrap();
        mb.add(0x4000, 0x800, MemblockFlags::NONE).unwrap();
        let mut it = mb.mem_pfn_ranges(0x1000);
        assert_eq!(it.next(), Some((1, 2)));
        assert_eq!(it.next(), None);
    }

    #[test]
    fn free_mem_ranges_subtracts_reserved() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.add(0x4000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.reserve(0x1800, 0x100).unwrap();

        let mut free = mb.free_mem_ranges(MemblockFlags::NONE);
        let a = free.next().unwrap();
        let b = free.next().unwrap();
        let c = free.next().unwrap();
        assert_eq!((a.0, a.1), (0x1000, 0x1800));
        assert_eq!((b.0, b.1), (0x1900, 0x2000));
        assert_eq!((c.0, c.1), (0x4000, 0x5000));
        assert!(free.next().is_none());
    }

    #[test]
    fn alloc_follows_bottom_up() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.set_bottom_up(true);
        assert!(mb.bottom_up());
        let p = mb.phys_alloc(0x100, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(p, 0x1000);
    }

    #[test]
    fn alloc_respects_current_limit() {
        let mut mb = Memblock::<usize, 8>::new();
        mb.add(0x1000, 0x1000, MemblockFlags::NONE).unwrap();
        mb.set_current_limit(0x1500);
        assert_eq!(mb.current_limit(), 0x1500);
        let p = mb.phys_alloc(0x100, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(p, 0x1400);
    }

    #[test]
    fn add_full_array_no_new_regions_is_noop() {
        let mut mb = Memblock::<usize, 2>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x1040, 0x10, MemblockFlags::NONE).unwrap();
        assert_eq!(mb.memory().count(), 2);
        assert_eq!(mb.memory().regions()[0].size(), 0x100);
    }

    #[test]
    fn add_full_array_adjacent_extends_last_region() {
        let mut mb = Memblock::<usize, 2>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2000, 0x100, MemblockFlags::NONE).unwrap();
        // A fully occupied array cannot provide the transient slot the
        // two-pass insertion needs before merging, mirroring the kernel
        // running out of room in memblock_double_array.
        assert!(matches!(
            mb.add(0x2100, 0x100, MemblockFlags::NONE),
            Err(Error::OverCapacity)
        ));

        let mut mb = Memblock::<usize, 3>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2100, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(mb.memory().count(), 2);
        assert_eq!(mb.memory().regions()[1], MemblockRegion::new(0x2000, 0x200));
        assert_eq!(mb.phys_mem_size(), 0x300);
    }

    #[test]
    fn add_full_array_needing_new_region_fails() {
        let mut mb = Memblock::<usize, 2>::new();
        mb.add(0x1000, 0x100, MemblockFlags::NONE).unwrap();
        mb.add(0x2000, 0x100, MemblockFlags::NONE).unwrap();
        assert!(matches!(
            mb.add(0x3000, 0x100, MemblockFlags::NONE),
            Err(Error::OverCapacity)
        ));
    }

    #[test]
    fn add_caps_size_at_address_space_end() {
        let mut mb = Memblock::<usize, 8>::new();
        let top = usize::MAX - 0xf;
        mb.add(top, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(
            mb.memory().regions()[0],
            MemblockRegion::with_flags(top, 0xf, MemblockFlags::NONE)
        );
        assert_eq!(mb.memory().regions()[0].end(), usize::MAX);
        assert_eq!(mb.phys_mem_size(), 0xf);
    }

    #[test]
    fn remove_caps_size_at_address_space_end() {
        let mut mb = Memblock::<usize, 8>::new();
        let top = usize::MAX - 0xff;
        mb.add(top, 0x100, MemblockFlags::NONE).unwrap();
        mb.remove(usize::MAX - 0xf, 0x100).unwrap();
        assert_eq!(mb.memory().regions()[0].size(), 0xf0);
    }

    #[test]
    fn is_region_memory_handles_max_range() {
        let mut mb = Memblock::<usize, 8>::new();
        let top = usize::MAX - 0xff;
        mb.add(top, 0x100, MemblockFlags::NONE).unwrap();
        assert!(mb.is_region_memory(top, 0x100));
        assert!(!mb.is_region_memory(top - 0x10, 0x101));
    }

    #[test]
    fn alloc_near_max_bottom_up() {
        let mut mb = Memblock::<usize, 8>::new();
        let top = usize::MAX - 0x1ff;
        mb.add(top, 0x200, MemblockFlags::NONE).unwrap();
        mb.set_bottom_up(true);
        let p = mb.phys_alloc(0x80, 0x100, MemblockFlags::NONE).unwrap();
        assert_eq!(p, top);
    }

    #[test]
    fn alloc_top_down_near_max() {
        let mut mb = Memblock::<usize, 8>::new();
        let top = usize::MAX - 0xff;
        mb.add(top, 0x100, MemblockFlags::NONE).unwrap();
        let p = mb.phys_alloc(0x40, 0x40, MemblockFlags::NONE).unwrap();
        assert_eq!(p, usize::MAX - 0x7f);
    }

    #[test]
    #[should_panic(expected = "page_size must be non-zero")]
    fn mem_pfn_ranges_rejects_zero_page_size() {
        let mb = Memblock::<usize, 8>::new();
        let _ = mb.mem_pfn_ranges(0);
    }
}
