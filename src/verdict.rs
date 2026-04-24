//! The outcome of a charge.
//!
//! A [`Verdict`] has exactly three shapes: the charge was accepted and
//! the budget has headroom ([`Verdict::Continue`]); the charge was
//! accepted but a dimension has crossed its warn threshold
//! ([`Verdict::Warn`]); the charge pushed a dimension past its limit
//! ([`Verdict::Exhausted`]).
//!
//! ## Firing semantics
//!
//! [`Verdict::Warn`] is returned on every charge where a dimension's
//! running total exceeds its warn threshold but has not yet reached its
//! limit. It is **not** one-shot. The kernel deliberately holds no
//! suppression state: if it did, `Verdict` would depend on prior calls
//! and break determinism. Callers who want one-shot warning behavior
//! (warn once per run, then suppress) track that in their adapter.
//!
//! ## Priority
//!
//! When a charge simultaneously crosses a warn threshold and exhausts a
//! limit, [`Verdict::Exhausted`] wins. The kernel checks exhaustion
//! first and never reports `Warn` for a charge that also exhausted.

use crate::Dim;

/// The outcome of a single [`charge`](crate::Budget::charge) call.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum Verdict {
    /// The charge was accepted and no threshold was crossed.
    Continue,

    /// The charge was accepted, but the named dimension's running total
    /// now exceeds its configured warn threshold. The budget is not
    /// exhausted — the caller may continue, but should consider this
    /// a preemption signal.
    Warn(Dim),

    /// The charge pushed the named dimension's running total past its
    /// configured limit. Inclusive: a charge that brings `spent` exactly
    /// to `limit` returns [`Verdict::Continue`] (or [`Verdict::Warn`]);
    /// a charge that brings `spent` to `limit + 1` or beyond returns
    /// `Exhausted`.
    Exhausted(Dim),
}

impl Verdict {
    /// `true` if this verdict indicates the budget has headroom
    /// (i.e., not `Exhausted`). Convenience for the common
    /// `while budget.charge(...).is_continuing() { ... }` pattern.
    #[must_use]
    pub const fn is_continuing(self) -> bool {
        !matches!(self, Self::Exhausted(_))
    }

    /// `true` if this verdict is `Exhausted`.
    #[must_use]
    pub const fn is_exhausted(self) -> bool {
        matches!(self, Self::Exhausted(_))
    }

    /// The dimension that triggered the verdict, if any. `Continue`
    /// returns `None`; `Warn` and `Exhausted` return the dimension.
    #[must_use]
    pub const fn dimension(self) -> Option<Dim> {
        match self {
            Self::Continue => None,
            Self::Warn(d) | Self::Exhausted(d) => Some(d),
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn continue_is_continuing_and_not_exhausted() {
        let v = Verdict::Continue;
        assert!(v.is_continuing());
        assert!(!v.is_exhausted());
        assert_eq!(v.dimension(), None);
    }

    #[test]
    fn warn_is_continuing_and_not_exhausted() {
        let v = Verdict::Warn(Dim::Tokens);
        assert!(v.is_continuing());
        assert!(!v.is_exhausted());
        assert_eq!(v.dimension(), Some(Dim::Tokens));
    }

    #[test]
    fn exhausted_is_not_continuing() {
        let v = Verdict::Exhausted(Dim::Millis);
        assert!(!v.is_continuing());
        assert!(v.is_exhausted());
        assert_eq!(v.dimension(), Some(Dim::Millis));
    }
}
