//! Error type returned by fallible memblock operations.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// The fixed-size region array has no free slots left.
    OverCapacity,
    /// An invariant was violated; indicates a bug in the implementation.
    InternalError,
    /// No suitable free memory could be found for an allocation.
    OutOfMemory,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let s = match self {
            Error::OverCapacity => "Over capacity",
            Error::InternalError => "Internal error",
            Error::OutOfMemory => "Out of memory",
        };
        write!(f, "{s}")
    }
}

impl core::error::Error for Error {}
