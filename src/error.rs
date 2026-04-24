//! Error types.

use crate::Dim;

/// Errors returned by [`BudgetBuilder::build`](crate::BudgetBuilder::build).
///
/// All variants represent structural problems with a budget declaration
/// that the builder refuses to construct. None of these are recoverable
/// at runtime — they indicate a bug in the caller's declaration code.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum BuilderError {
    /// The same dimension was declared more than once. Ambiguous: which
    /// limit should win? The builder refuses to guess.
    DuplicateDimension(Dim),

    /// A warn threshold was configured that equals or exceeds the
    /// dimension's limit. A warn threshold must fire strictly before
    /// exhaustion or it serves no purpose.
    WarnNotBelowLimit(Dim),

    /// A limit of zero was configured. A zero-limit dimension can never
    /// accept any charge, which is almost certainly a bug in caller code.
    ZeroLimit(Dim),

    /// The builder had no dimensions declared when `build` was called.
    /// An empty budget returns [`Verdict::Continue`](crate::Verdict::Continue)
    /// for every charge, which is indistinguishable from having no
    /// budget at all. Refuse to construct.
    NoDimensions,
}

/// Errors returned by [`Budget::charge`](crate::Budget::charge).
///
/// Charge errors are structural — they indicate the caller asked the
/// budget to do something outside its declared shape. They are distinct
/// from [`Verdict`](crate::Verdict), which reports the state of an
/// accepted charge against the budget.
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum ChargeError {
    /// The dimension was not declared in this budget. Callers must
    /// declare every dimension they intend to charge at build time.
    UnknownDimension(Dim),
}

impl core::fmt::Display for BuilderError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::DuplicateDimension(dim) => {
                write!(f, "dimension '{}' declared more than once", dim.name())
            }
            Self::WarnNotBelowLimit(dim) => {
                write!(
                    f,
                    "warn threshold for '{}' must be strictly less than its limit",
                    dim.name()
                )
            }
            Self::ZeroLimit(dim) => {
                write!(f, "limit for '{}' must be non-zero", dim.name())
            }
            Self::NoDimensions => {
                write!(f, "budget must declare at least one dimension")
            }
        }
    }
}

impl core::fmt::Display for ChargeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownDimension(dim) => {
                write!(
                    f,
                    "dimension '{}' was not declared in this budget",
                    dim.name()
                )
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for BuilderError {}

#[cfg(feature = "std")]
impl std::error::Error for ChargeError {}
