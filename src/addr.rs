//! The physical address type and address arithmetic helpers.
//!
//! Defines [`PhysAddr`], the generic physical address type used throughout
//! memblock, plus free-standing helpers for physical address and page frame
//! number (PFN) arithmetic, mirroring the kernel's
//! `PFN_UP`/`PFN_DOWN`/`PFN_PHYS` macros (`include/linux/pfn.h`), the
//! `ALIGN`/`ALIGN_DOWN` macros, and memblock's
//! `memblock_addrs_overlap`/`memblock_cap_size`.
//!
//! All helpers are generic over [`PhysAddr`] and overflow-safe: results that
//! would exceed [`PhysAddr::MAX`] saturate instead of wrapping around.

use core::ops::Add;
use core::ops::BitAnd;
use core::ops::Div;
use core::ops::Mul;
use core::ops::Not;
use core::ops::Rem;
use core::ops::Sub;

/// Physical address type used throughout memblock.
///
/// Implementors must be a plain copyable unsigned integer-like type. All
/// range arithmetic inside memblock is overflow-safe: incoming sizes are
/// clamped against [`PhysAddr::MAX`] (mirroring the kernel's
/// `memblock_cap_size`) and the remaining computations either cannot
/// overflow or saturate via the crate-internal `saturating_add` helper.
///
/// `Div`/`Mul`/`Rem` are required for page frame number (PFN)
/// computations, and `BitAnd`/`Not` are required by the internal alignment
/// helpers.
///
/// # Contract
///
/// The overflow-safety guarantees of this crate hold only for
/// implementations with standard unsigned integer semantics:
///
/// - `ZERO`, `ONE`, `MAX` are the additive identity, the multiplicative
///   identity, and the greatest representable value, with
///   `ZERO <= v <= MAX` for every value `v`.
/// - `Add`/`Sub`/`Mul` agree with mathematical integer arithmetic whenever
///   the result is `<= MAX`; memblock guards every operation that could
///   exceed `MAX` or underflow, so wrapping is never triggered.
/// - `Div`/`Rem` are truncating division and remainder, with `v / ONE == v`
///   and `v % ONE == ZERO`.
/// - `BitAnd`/`Not` are two's-complement bitwise operations, with
///   `!ZERO == MAX`.
///
/// Implementations for all unsigned primitives (`u8`..`u128`, `usize`)
/// are provided by this crate.
pub trait PhysAddr:
    Copy
    + Ord
    + Add<Output = Self>
    + Sub<Output = Self>
    + Div<Output = Self>
    + Mul<Output = Self>
    + Rem<Output = Self>
    + BitAnd<Output = Self>
    + Not<Output = Self>
{
    /// The maximum address representable by `Self`.
    const MAX: Self;
    /// The zero address.
    const ZERO: Self;
    /// The address value one.
    const ONE: Self;
}

macro_rules! impl_phys_addr {
    ($($t:ty),+ $(,)?) => {
        $(
            impl PhysAddr for $t {
                const MAX: Self = <$t>::MAX;
                const ZERO: Self = 0;
                const ONE: Self = 1;
            }
        )+
    };
}

impl_phys_addr!(u8, u16, u32, u64, u128, usize);

/// Returns `true` if the ranges `[base1, base1 + size1)` and
/// `[base2, base2 + size2)` overlap.
///
/// Mirrors the kernel's `memblock_addrs_overlap`. Ranges are clamped to
/// [`PhysAddr::MAX`] before comparison, so ranges touching the top of the
/// address space never wrap around.
pub fn addrs_overlap<T: PhysAddr>(base1: T, size1: T, base2: T, size2: T) -> bool {
    base1 < saturating_add(base2, size2) && base2 < saturating_add(base1, size1)
}

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

/// Returns `true` if `v` is a non-zero power of two.
fn is_power_of_two<T: PhysAddr>(v: T) -> bool {
    v != PhysAddr::ZERO && (v & (v - T::ONE)) == PhysAddr::ZERO
}

/// Rounds `addr` up to the next multiple of `alignment`.
///
/// Mirrors the kernel's `ALIGN`. The result saturates to
/// [`PhysAddr::MAX`] if it would overflow.
///
/// # Panics
///
/// Panics if `alignment` is zero or not a power of two.
pub fn align_up<T: PhysAddr>(addr: T, alignment: T) -> T {
    assert!(
        is_power_of_two(alignment),
        "alignment must be a non-zero power of two"
    );
    let mask = alignment - T::ONE;
    if addr > T::MAX - mask {
        T::MAX
    } else {
        (addr + mask) & !mask
    }
}

/// Rounds `addr` down to the previous multiple of `alignment`.
///
/// Mirrors the kernel's `ALIGN_DOWN`.
///
/// # Panics
///
/// Panics if `alignment` is zero or not a power of two.
pub fn align_down<T: PhysAddr>(addr: T, alignment: T) -> T {
    assert!(
        is_power_of_two(alignment),
        "alignment must be a non-zero power of two"
    );
    addr & !(alignment - T::ONE)
}

/// Returns the smallest page frame number containing `addr`, i.e.
/// `ceil(addr / page_size)`.
///
/// Mirrors the kernel's `PFN_UP`. The computation cannot overflow, so
/// addresses near [`PhysAddr::MAX`] are handled correctly.
///
/// # Panics
///
/// Panics if `page_size` is zero.
pub fn pfn_up<T: PhysAddr>(addr: T, page_size: T) -> T {
    assert!(page_size != PhysAddr::ZERO, "page_size must be non-zero");
    let q = addr / page_size;
    if addr % page_size != PhysAddr::ZERO {
        q + T::ONE
    } else {
        q
    }
}

/// Returns the largest page frame number fully below `addr`, i.e.
/// `floor(addr / page_size)`.
///
/// Mirrors the kernel's `PFN_DOWN`.
///
/// # Panics
///
/// Panics if `page_size` is zero.
pub fn pfn_down<T: PhysAddr>(addr: T, page_size: T) -> T {
    assert!(page_size != PhysAddr::ZERO, "page_size must be non-zero");
    addr / page_size
}

/// Returns the start address of the page numbered `pfn`.
///
/// Mirrors the kernel's `pfn_to_phys`. The result saturates to
/// [`PhysAddr::MAX`] if `pfn * page_size` would overflow.
///
/// # Panics
///
/// Panics if `page_size` is zero.
pub fn pfn_to_phys<T: PhysAddr>(pfn: T, page_size: T) -> T {
    assert!(page_size != PhysAddr::ZERO, "page_size must be non-zero");
    if pfn > T::MAX / page_size {
        T::MAX
    } else {
        pfn * page_size
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addrs_overlap_basic() {
        assert!(addrs_overlap(0x0usize, 0x100, 0x80, 0x100));
        assert!(!addrs_overlap(0x0usize, 0x100, 0x100, 0x100));
        assert!(!addrs_overlap(0x100usize, 0x0, 0x100, 0x100));
    }

    #[test]
    fn addrs_overlap_near_max() {
        let top = usize::MAX - 0xf;
        assert!(addrs_overlap(top, 0x10, top - 0x10, 0x20));
        assert!(!addrs_overlap(top, 0x10, 0x0, 0x100));
    }

    #[test]
    fn cap_size_clamps_at_address_space_end() {
        assert_eq!(cap_size(0x1000usize, 0x100), 0x100);
        assert_eq!(cap_size(usize::MAX - 0xf, 0x100), 0xf);
        assert_eq!(cap_size(usize::MAX, 0x100), 0x0);
    }

    #[test]
    fn align_up_basic() {
        assert_eq!(align_up(0x0usize, 0x1000), 0x0);
        assert_eq!(align_up(0x1usize, 0x1000), 0x1000);
        assert_eq!(align_up(0x1000usize, 0x1000), 0x1000);
        assert_eq!(align_up(0x1001usize, 0x1000), 0x2000);
        assert_eq!(align_up(0x1234usize, 1), 0x1234);
    }

    #[test]
    fn align_down_basic() {
        assert_eq!(align_down(0x0usize, 0x1000), 0x0);
        assert_eq!(align_down(0xfffusize, 0x1000), 0x0);
        assert_eq!(align_down(0x1000usize, 0x1000), 0x1000);
        assert_eq!(align_down(0x1fffusize, 0x1000), 0x1000);
        assert_eq!(align_down(0x1234usize, 1), 0x1234);
    }

    #[test]
    fn align_helpers_near_max() {
        let aligned = usize::MAX & !0xff;
        assert_eq!(align_down(usize::MAX, 0x100), aligned);
        assert_eq!(align_up(aligned, 0x100), aligned);
        // The next multiple does not fit; saturate instead of wrapping.
        assert_eq!(align_up(aligned + 1, 0x100), usize::MAX);
    }

    #[test]
    #[should_panic(expected = "alignment must be a non-zero power of two")]
    fn align_up_rejects_zero_alignment() {
        align_up::<usize>(0x1000, 0);
    }

    #[test]
    #[should_panic(expected = "alignment must be a non-zero power of two")]
    fn align_down_rejects_non_power_of_two() {
        align_down::<usize>(0x1000, 3);
    }

    #[test]
    fn pfn_helpers() {
        assert_eq!(pfn_up(0x0usize, 0x1000), 0);
        assert_eq!(pfn_up(0x1usize, 0x1000), 1);
        assert_eq!(pfn_up(0x1000usize, 0x1000), 1);
        assert_eq!(pfn_down(0xfffusize, 0x1000), 0);
        assert_eq!(pfn_down(0x1000usize, 0x1000), 1);
        assert_eq!(pfn_to_phys(2usize, 0x1000), 0x2000);
        assert_eq!(pfn_to_phys(0usize, 0x1000), 0);
    }

    #[test]
    fn pfn_helpers_at_address_space_end() {
        let max = usize::MAX;
        assert_eq!(pfn_up(max, 0x1000), max / 0x1000 + 1);
        assert_eq!(pfn_down(max, 0x1000), max / 0x1000);
        assert_eq!(pfn_to_phys(max / 0x1000 + 1, 0x1000), usize::MAX);
    }

    #[test]
    #[should_panic(expected = "page_size must be non-zero")]
    fn pfn_helpers_reject_zero_page_size() {
        pfn_up::<usize>(0, 0);
    }
}
