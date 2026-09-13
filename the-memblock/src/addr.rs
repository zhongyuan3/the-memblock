//! The physical address type and address arithmetic helpers.
//!
//! Defines [`PhysAddr`], the generic physical address type used throughout
//! memblock, together with the address and page frame number (PFN)
//! arithmetic it exposes. These mirror the kernel's
//! `PFN_UP`/`PFN_DOWN`/`PFN_PHYS` macros (`include/linux/pfn.h`) and the
//! `ALIGN`/`ALIGN_DOWN` macros. The crate-internal `saturating_add` and
//! `cap_size` helpers mirror memblock's `memblock_cap_size`.
//!
//! The arithmetic is overflow-safe: results that would exceed
//! [`PhysAddr::MAX`] saturate instead of wrapping around.

use core::ops::Add;
use core::ops::Sub;

/// Physical address type used throughout memblock.
///
/// Implementors must be a plain copyable unsigned integer-like type. All
/// range arithmetic inside memblock is overflow-safe: incoming sizes are
/// clamped against [`PhysAddr::MAX`] (mirroring the kernel's
/// `memblock_cap_size`) and the remaining computations either cannot
/// overflow or saturate via the crate-internal `saturating_add` helper.
///
/// # Contract
///
/// The overflow-safety guarantees of this crate hold only for
/// implementations with standard unsigned integer semantics:
///
/// - `ZERO` and `MAX` are the additive identity and the greatest
///   representable value, with `ZERO <= v <= MAX` for every value `v`.
/// - `Add`/`Sub` agree with mathematical integer arithmetic whenever the
///   result is in `[ZERO, MAX]`; memblock guards every operation that could
///   exceed `MAX` or underflow, so wrapping is never triggered.
/// - The alignment and PFN methods perform truncating division, remainder
///   and two's-complement bitwise operations with `v / ONE == v` and
///   `v % ONE == ZERO`.
///
/// Implementations for all unsigned primitives (`u8`..`u128`, `usize`)
/// are provided by this crate.
///
/// To use a custom newtype as an address, derive the implementation with
/// [`#[derive(PhysAddr)]`](crate::PhysAddr).
pub trait PhysAddr: Copy + Ord + Add<Output = Self> + Sub<Output = Self> {
    /// The maximum address representable by `Self`.
    const MAX: Self;
    /// The zero address.
    const ZERO: Self;

    /// Rounds `addr` up to the next multiple of `alignment`.
    ///
    /// Mirrors the kernel's `ALIGN`. The result saturates to [`Self::MAX`]
    /// if it would overflow.
    ///
    /// # Panics
    ///
    /// Panics if `alignment` is zero or not a power of two.
    fn align_up(addr: Self, alignment: Self) -> Self;

    /// Rounds `addr` down to the previous multiple of `alignment`.
    ///
    /// Mirrors the kernel's `ALIGN_DOWN`.
    ///
    /// # Panics
    ///
    /// Panics if `alignment` is zero or not a power of two.
    fn align_down(addr: Self, alignment: Self) -> Self;

    /// Returns the smallest page frame number containing `addr`, i.e.
    /// `ceil(addr / page_size)`.
    ///
    /// Mirrors the kernel's `PFN_UP`. The computation cannot overflow, so
    /// addresses near [`Self::MAX`] are handled correctly.
    ///
    /// # Panics
    ///
    /// Panics if `page_size` is zero.
    fn pfn_up(addr: Self, page_size: Self) -> Self;

    /// Returns the largest page frame number fully below `addr`, i.e.
    /// `floor(addr / page_size)`.
    ///
    /// Mirrors the kernel's `PFN_DOWN`.
    ///
    /// # Panics
    ///
    /// Panics if `page_size` is zero.
    fn pfn_down(addr: Self, page_size: Self) -> Self;

    /// Returns the start address of the page numbered `pfn`.
    ///
    /// Mirrors the kernel's `PFN_PHYS`. The result saturates to
    /// [`Self::MAX`] if `pfn * page_size` would overflow.
    ///
    /// # Panics
    ///
    /// Panics if `page_size` is zero.
    fn pfn_to_phys(pfn: Self, page_size: Self) -> Self;
}

macro_rules! impl_phys_addr {
    ($($t:ty),+ $(,)?) => {
        $(
            impl PhysAddr for $t {
                const MAX: Self = <$t>::MAX;
                const ZERO: Self = 0;

                fn align_up(addr: Self, alignment: Self) -> Self {
                    assert!(
                        alignment.is_power_of_two(),
                        "alignment must be a non-zero power of two"
                    );
                    let mask = alignment - 1;
                    if addr > Self::MAX - mask {
                        Self::MAX
                    } else {
                        (addr + mask) & !mask
                    }
                }

                fn align_down(addr: Self, alignment: Self) -> Self {
                    assert!(
                        alignment.is_power_of_two(),
                        "alignment must be a non-zero power of two"
                    );
                    addr & !(alignment - 1)
                }

                fn pfn_up(addr: Self, page_size: Self) -> Self {
                    assert!(page_size != 0, "page_size must be non-zero");
                    let q = addr / page_size;
                    if addr % page_size != 0 {
                        q + 1
                    } else {
                        q
                    }
                }

                fn pfn_down(addr: Self, page_size: Self) -> Self {
                    assert!(page_size != 0, "page_size must be non-zero");
                    addr / page_size
                }

                fn pfn_to_phys(pfn: Self, page_size: Self) -> Self {
                    assert!(page_size != 0, "page_size must be non-zero");
                    if pfn > Self::MAX / page_size {
                        Self::MAX
                    } else {
                        pfn * page_size
                    }
                }
            }
        )+
    };
}

impl_phys_addr!(u8, u16, u32, u64, u128, usize);

/// Returns `lhs + rhs`, saturating to [`PhysAddr::MAX`] instead of
/// overflowing.
///
/// Used wherever an exclusive end address is computed from `base + size`,
/// so that ranges touching the top of the address space clamp instead of
/// wrapping around.
pub(crate) fn saturating_add<T: PhysAddr>(lhs: T, rhs: T) -> T {
    if lhs > T::MAX - rhs {
        T::MAX
    } else {
        lhs + rhs
    }
}

/// Clamps `size` so that `base + size <= PhysAddr::MAX`.
///
/// Mirrors the kernel's `memblock_cap_size`: ranges extending past the top
/// of the address space are truncated instead of wrapping around.
pub(crate) fn cap_size<T: PhysAddr>(base: T, size: T) -> T {
    if size > T::MAX - base {
        T::MAX - base
    } else {
        size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn align_up_basic() {
        assert_eq!(usize::align_up(0x0, 0x1000), 0x0);
        assert_eq!(usize::align_up(0x1, 0x1000), 0x1000);
        assert_eq!(usize::align_up(0x1000, 0x1000), 0x1000);
        assert_eq!(usize::align_up(0x1001, 0x1000), 0x2000);
        assert_eq!(usize::align_up(0x1234, 1), 0x1234);
    }

    #[test]
    fn align_up_near_max_saturates() {
        let aligned = usize::MAX & !0xff;
        // The next multiple does not fit; saturate instead of wrapping.
        assert_eq!(usize::align_up(aligned + 1, 0x100), usize::MAX);
        assert_eq!(usize::align_up(usize::MAX, 0x1), usize::MAX);
    }

    #[test]
    fn align_down_basic() {
        assert_eq!(usize::align_down(0x0, 0x1000), 0x0);
        assert_eq!(usize::align_down(0xfff, 0x1000), 0x0);
        assert_eq!(usize::align_down(0x1000, 0x1000), 0x1000);
        assert_eq!(usize::align_down(0x1fff, 0x1000), 0x1000);
        assert_eq!(usize::align_down(0x1234, 1), 0x1234);
    }

    #[test]
    fn align_down_near_max() {
        let aligned = usize::MAX & !0xff;
        assert_eq!(usize::align_down(usize::MAX, 0x100), aligned);
        assert_eq!(usize::align_up(aligned, 0x100), aligned);
    }

    #[test]
    fn align_works_for_all_widths() {
        assert_eq!(u8::align_up(0x21, 0x20), 0x40);
        assert_eq!(u8::align_down(0x3f, 0x20), 0x20);
        assert_eq!(u16::align_up(0x100, 0x1000), 0x1000);
        assert_eq!(u32::align_up(0x1234_5678, 0x1000), 0x1234_6000);
        assert_eq!(
            u64::align_down(0x1234_5678_9abc_def1, 0x1000),
            0x1234_5678_9abc_d000
        );
        assert_eq!(u128::align_up(0x1, 0x10), 0x10);
    }

    #[test]
    fn align_roundtrip_is_idempotent() {
        for a in [0x100usize, 0x1000, 0x1_0000] {
            assert_eq!(
                usize::align_up(usize::align_up(0x1234, a), a),
                usize::align_up(0x1234, a)
            );
            assert_eq!(
                usize::align_down(usize::align_down(0x1234, a), a),
                usize::align_down(0x1234, a)
            );
        }
    }

    #[test]
    #[should_panic(expected = "alignment must be a non-zero power of two")]
    fn align_up_rejects_zero_alignment() {
        usize::align_up(0x1000, 0);
    }

    #[test]
    #[should_panic(expected = "alignment must be a non-zero power of two")]
    fn align_up_rejects_non_power_of_two() {
        usize::align_up(0x1000, 3);
    }

    #[test]
    #[should_panic(expected = "alignment must be a non-zero power of two")]
    fn align_down_rejects_non_power_of_two() {
        usize::align_down(0x1000, 6);
    }

    #[test]
    fn pfn_up_down_basic() {
        assert_eq!(usize::pfn_up(0x0, 0x1000), 0);
        assert_eq!(usize::pfn_up(0x1, 0x1000), 1);
        assert_eq!(usize::pfn_up(0x1000, 0x1000), 1);
        assert_eq!(usize::pfn_up(0x1001, 0x1000), 2);
        assert_eq!(usize::pfn_down(0xfff, 0x1000), 0);
        assert_eq!(usize::pfn_down(0x1000, 0x1000), 1);
        assert_eq!(usize::pfn_down(0x1001, 0x1000), 1);
    }

    #[test]
    fn pfn_helpers_at_address_space_end() {
        let max = usize::MAX;
        assert_eq!(usize::pfn_up(max, 0x1000), max / 0x1000 + 1);
        assert_eq!(usize::pfn_down(max, 0x1000), max / 0x1000);
    }

    #[test]
    fn pfn_to_phys_basic() {
        assert_eq!(usize::pfn_to_phys(0, 0x1000), 0);
        assert_eq!(usize::pfn_to_phys(1, 0x1000), 0x1000);
        assert_eq!(usize::pfn_to_phys(2, 0x1000), 0x2000);
    }

    #[test]
    fn pfn_to_phys_saturates_at_max() {
        assert_eq!(
            usize::pfn_to_phys(usize::MAX / 0x1000 + 1, 0x1000),
            usize::MAX
        );
        assert_eq!(usize::pfn_to_phys(usize::MAX, 0x1000), usize::MAX);
    }

    #[test]
    fn pfn_roundtrip_is_identity_for_full_pages() {
        let base = 0x1234_0000usize;
        let pfn = usize::pfn_down(base, 0x1000);
        assert_eq!(usize::pfn_to_phys(pfn, 0x1000), base);
    }

    #[test]
    #[should_panic(expected = "page_size must be non-zero")]
    fn pfn_up_rejects_zero_page_size() {
        usize::pfn_up(0x1000, 0);
    }

    #[test]
    #[should_panic(expected = "page_size must be non-zero")]
    fn pfn_down_rejects_zero_page_size() {
        usize::pfn_down(0x1000, 0);
    }

    #[test]
    #[should_panic(expected = "page_size must be non-zero")]
    fn pfn_to_phys_rejects_zero_page_size() {
        usize::pfn_to_phys(1, 0);
    }

    #[test]
    fn pfn_works_for_all_widths() {
        assert_eq!(u8::pfn_up(0xff, 0x10), 16);
        assert_eq!(u8::pfn_down(0xff, 0x10), 15);
        assert_eq!(u32::pfn_to_phys(0x1000, 0x1000), 0x100_0000);
        assert_eq!(
            u64::pfn_down(0xffff_ffff_ffff_f000, 0x1000),
            0x000f_ffff_ffff_ffff
        );
    }

    #[test]
    fn saturating_add_clamps_at_max() {
        assert_eq!(saturating_add(0x1000usize, 0x100), 0x1100);
        assert_eq!(saturating_add(usize::MAX - 0xf, 0x100), usize::MAX);
        assert_eq!(saturating_add(usize::MAX, 1), usize::MAX);
    }

    #[test]
    fn cap_size_clamps_at_address_space_end() {
        assert_eq!(cap_size(0x1000usize, 0x100), 0x100);
        assert_eq!(cap_size(usize::MAX - 0xf, 0x100), 0xf);
        assert_eq!(cap_size(usize::MAX, 0x100), 0x0);
    }
}
