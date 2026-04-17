//! Budget dimensions.
//!
//! A dimension is a named, integer-valued resource that can be budgeted
//! and charged. The dimension set is fixed at compile time. Five
//! dimensions are pre-named for conventional use; three `Custom` slots
//! are reserved for domain-specific budgets the caller assigns meaning
//! to via documentation in their own codebase.
//!
//! ## Why fixed?
//!
//! A closed dimension set is load-bearing for this crate's guarantees.
//! It lets the internal map use array-indexed lookup (`O(1)`, no hashing),
//! makes discriminants compiler-verified unique, and keeps the code path
//! auditable in a single afternoon by a reviewer. If you need a ninth
//! dimension, you likely want a second ledger, not a larger one.

/// The maximum number of distinct dimensions a budget may contain.
///
/// This is equal to the total number of [`Dim`] variants. It is fixed
/// at compile time and exposed so callers can size their own scratch
/// arrays consistently with the kernel.
pub const MAX_DIMS: usize = 8;

/// A budget dimension.
///
/// The discriminant assigned to each variant is stable, documented,
/// and used internally for `O(1)` array-indexed lookup. Do not rely on
/// specific numeric values outside of this crate; treat the enum as
/// opaque.
///
/// ## Conventional meanings
///
/// The five named dimensions carry conventional meanings. The kernel
/// itself treats all dimensions as opaque `u64` counters; meaning lives
/// in caller documentation and adapter code.
///
/// - [`Dim::Tokens`] — LLM tokens, database rows, message units.
/// - [`Dim::Millis`] — wall-clock milliseconds elapsed.
/// - [`Dim::Bytes`] — bytes transferred, read, or written.
/// - [`Dim::Calls`] — discrete operation count.
/// - [`Dim::Memory`] — peak bytes of memory in use.
///
/// The three `Custom` slots carry no kernel-level meaning and are
/// available for domain-specific budgets.
#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq, Debug, Hash)]
pub enum Dim {
    /// LLM tokens, database rows, message units. Caller-defined meaning.
    Tokens = 0,
    /// Wall-clock milliseconds elapsed. Caller supplies elapsed values.
    Millis = 1,
    /// Bytes transferred, read, or written.
    Bytes = 2,
    /// Discrete operation count.
    Calls = 3,
    /// Peak bytes of memory in use.
    Memory = 4,
    /// User-assigned custom dimension, slot 0.
    Custom0 = 5,
    /// User-assigned custom dimension, slot 1.
    Custom1 = 6,
    /// User-assigned custom dimension, slot 2.
    Custom2 = 7,
}

impl Dim {
    /// All dimensions, in discriminant order. Useful for iteration in
    /// diagnostic code (never on the hot path — charging targets
    /// specific dimensions by value).
    pub const ALL: [Self; MAX_DIMS] = [
        Self::Tokens,
        Self::Millis,
        Self::Bytes,
        Self::Calls,
        Self::Memory,
        Self::Custom0,
        Self::Custom1,
        Self::Custom2,
    ];

    /// The stable discriminant as a `usize`, suitable for indexing a
    /// `[_; MAX_DIMS]` array. Guaranteed to be in `0..MAX_DIMS`.
    #[inline]
    #[must_use]
    pub const fn index(self) -> usize {
        self as u8 as usize
    }

    /// A short, static, human-readable name. Intended for error
    /// messages and diagnostics. Never allocates.
    #[inline]
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Tokens => "tokens",
            Self::Millis => "millis",
            Self::Bytes => "bytes",
            Self::Calls => "calls",
            Self::Memory => "memory",
            Self::Custom0 => "custom0",
            Self::Custom1 => "custom1",
            Self::Custom2 => "custom2",
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    #[test]
    fn index_matches_discriminant() {
        for dim in Dim::ALL {
            assert!(dim.index() < MAX_DIMS);
        }
    }

    #[test]
    fn indices_are_unique_and_dense() {
        let mut seen = [false; MAX_DIMS];
        for dim in Dim::ALL {
            assert!(!seen[dim.index()], "duplicate index for {dim:?}");
            seen[dim.index()] = true;
        }
        assert!(seen.iter().all(|&b| b), "indices are not dense 0..MAX_DIMS");
    }

    #[test]
    fn all_length_matches_max_dims() {
        assert_eq!(Dim::ALL.len(), MAX_DIMS);
    }

    #[test]
    fn names_are_unique() {
        let names: [&str; MAX_DIMS] = [
            Dim::Tokens.name(),
            Dim::Millis.name(),
            Dim::Bytes.name(),
            Dim::Calls.name(),
            Dim::Memory.name(),
            Dim::Custom0.name(),
            Dim::Custom1.name(),
            Dim::Custom2.name(),
        ];
        for i in 0..MAX_DIMS {
            for j in (i + 1)..MAX_DIMS {
                assert_ne!(names[i], names[j]);
            }
        }
    }
}
