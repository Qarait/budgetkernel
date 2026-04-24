//! The budget itself.
//!
//! A [`Budget`] is constructed via [`Budget::builder`] and mutated by
//! repeated [`charge`](Budget::charge) calls. Each charge returns a
//! [`Verdict`] describing the outcome.
//!
//! Internally, a `Budget` holds three internal fixed-capacity maps: one
//! for configured limits, one for configured warn thresholds, and one
//! for running spent totals. Presence in the limits map is the
//! canonical indicator that a dimension is declared in this budget.

use crate::fixed_map::FixedMap;
use crate::{BuilderError, ChargeError, Dim, Verdict};

/// A declared budget with running spent counters.
///
/// Construct via [`Budget::builder`]. See module docs for the full
/// lifecycle.
#[derive(Debug)]
pub struct Budget {
    /// Per-dimension limits. Presence == dimension is declared.
    pub(crate) limits: FixedMap,

    /// Per-dimension warn thresholds. Presence == this dimension has a
    /// warn threshold configured (not all declared dimensions must).
    pub(crate) warn_thresholds: FixedMap,

    /// Per-dimension running totals. Presence == this dimension has
    /// been charged at least once. Absence means "never charged";
    /// `get_or(dim, 0)` gives the effective value on the hot path.
    pub(crate) spent: FixedMap,
}

/// Builder for a [`Budget`].
///
/// Accumulates per-dimension configuration and validates it in
/// [`BudgetBuilder::build`]. Duplicate dimensions, zero limits, warn
/// thresholds that do not fire strictly before exhaustion, and empty
/// budgets are all rejected structurally.
///
/// If the same dimension is declared more than once, the builder
/// records the duplicate and [`build`](Self::build) will return
/// [`BuilderError::DuplicateDimension`]. The internal state of the
/// builder after a duplicate declaration is unspecified - callers
/// must not rely on which declaration's values are stored, because
/// `build` will not produce a valid budget from a duplicate
/// declaration regardless.
#[derive(Debug)]
pub struct BudgetBuilder {
    limits: FixedMap,
    warn_thresholds: FixedMap,
    /// Set on any attempt to declare a dimension that was already
    /// declared. Reported by `build` if present. We defer the error
    /// until `build` so the builder type can remain a simple
    /// fluent-chain shape without `Result` at every step.
    duplicate: Option<Dim>,
}

impl BudgetBuilder {
    /// Internal constructor. Use [`Budget::builder`].
    const fn new() -> Self {
        Self {
            limits: FixedMap::new(),
            warn_thresholds: FixedMap::new(),
            duplicate: None,
        }
    }

    /// Declare a dimension with a limit and no warn threshold.
    ///
    /// Charging the dimension will return [`Verdict::Continue`] for
    /// every charge where the running total stays at or below `limit`,
    /// and [`Verdict::Exhausted`] once the total exceeds it.
    ///
    /// Declaring the same dimension twice is a structural error
    /// reported by [`build`](Self::build).
    #[must_use]
    pub fn limit(mut self, dim: Dim, limit: u64) -> Self {
        if self.limits.contains(dim) && self.duplicate.is_none() {
            self.duplicate = Some(dim);
        }
        // Discard: duplicate tracking is handled explicitly above; the
        // previous stored value is not part of the builder's public contract.
        let _ = self.limits.insert(dim, limit);
        self
    }

    /// Declare a dimension with a limit and an absolute warn threshold.
    ///
    /// `warn` must be strictly less than `limit` or [`build`](Self::build)
    /// will return [`BuilderError::WarnNotBelowLimit`]. Charging the
    /// dimension returns [`Verdict::Warn`] on any charge where the
    /// running total exceeds `warn` but has not yet exceeded `limit`.
    ///
    /// A `warn` threshold of zero is valid: it causes [`Verdict::Warn`]
    /// to be returned on any charge where `spent` becomes positive.
    /// A zero-amount charge against a fresh `warn = 0` budget returns
    /// [`Verdict::Continue`] because the comparison is strict
    /// (`spent > warn`). After any positive spend, a zero-amount charge
    /// reports [`Verdict::Warn`] because the current spent value remains
    /// above the threshold.
    ///
    /// Declaring the same dimension twice is a structural error
    /// reported by [`build`](Self::build).
    #[must_use]
    pub fn limit_with_warn(mut self, dim: Dim, limit: u64, warn: u64) -> Self {
        if self.limits.contains(dim) && self.duplicate.is_none() {
            self.duplicate = Some(dim);
        }
        // Discard: duplicate tracking is handled explicitly above; the
        // previous stored value is not part of the builder's public contract.
        let _ = self.limits.insert(dim, limit);
        // Discard: duplicate tracking is handled by the limit declaration;
        // a duplicate builder cannot produce a valid budget.
        let _ = self.warn_thresholds.insert(dim, warn);
        self
    }

    /// Finalize the builder into a [`Budget`].
    ///
    /// Validation order is fixed and documented: duplicate declarations
    /// first, then the no-dimensions check, then per-dimension checks
    /// in [`Dim::ALL`] order (zero-limit before warn-not-below-limit).
    ///
    /// # Errors
    ///
    /// Returns [`BuilderError`] on any structural problem with the
    /// declaration. All errors are non-recoverable; they indicate a
    /// bug in the caller's declaration code.
    pub fn build(self) -> Result<Budget, BuilderError> {
        if let Some(dim) = self.duplicate {
            return Err(BuilderError::DuplicateDimension(dim));
        }
        if self.limits.is_empty() {
            return Err(BuilderError::NoDimensions);
        }
        debug_assert!(
            Dim::ALL
                .iter()
                .all(|d| !self.warn_thresholds.contains(*d) || self.limits.contains(*d)),
            "warn_thresholds invariant violated: a warn threshold exists for an undeclared dimension"
        );
        for dim in Dim::ALL {
            if let Some(limit) = self.limits.get(dim) {
                if limit == 0 {
                    return Err(BuilderError::ZeroLimit(dim));
                }
                if let Some(warn) = self.warn_thresholds.get(dim) {
                    if warn >= limit {
                        return Err(BuilderError::WarnNotBelowLimit(dim));
                    }
                }
            }
        }
        Ok(Budget {
            limits: self.limits,
            warn_thresholds: self.warn_thresholds,
            spent: FixedMap::new(),
        })
    }
}

impl Budget {
    /// Begin constructing a [`Budget`].
    #[must_use]
    pub const fn builder() -> BudgetBuilder {
        BudgetBuilder::new()
    }

    /// Charge `amount` against `dim` and return the resulting verdict.
    ///
    /// # Semantics
    ///
    /// - **Inclusive limits.** A charge that brings `spent` exactly to
    ///   `limit` returns [`Verdict::Continue`] (or [`Verdict::Warn`] if
    ///   a warn threshold is configured and crossed). A charge that
    ///   brings `spent` to `limit + 1` or beyond returns
    ///   [`Verdict::Exhausted`].
    /// - **Saturating arithmetic.** If `current_spent + amount` would
    ///   overflow `u64`, the running total saturates at `u64::MAX`.
    ///   Saturation never panics.
    /// - **Exhaustion wins over warn.** If a single charge crosses both
    ///   the warn threshold and the limit, [`Verdict::Exhausted`] is
    ///   returned; `Warn` is not reported for that charge.
    /// - **Spent is always updated.** Even on exhaustion, `spent` is
    ///   updated to reflect the attempted charge (saturated). This lets
    ///   callers query [`remaining`](Self::remaining) or a snapshot to
    ///   diagnose overruns.
    ///
    /// ## Zero-amount charges
    ///
    /// Charging zero units is valid. It returns the verdict for the
    /// current accounting state without increasing the reported spent
    /// value. This is useful as a state poll: `charge(dim, 0)` tells
    /// callers whether the budget is currently continuing, warning, or
    /// exhausted without consuming additional budget.
    ///
    /// # Errors
    ///
    /// Returns [`ChargeError::UnknownDimension`] if `dim` was not
    /// declared in this budget.
    pub fn charge(&mut self, dim: Dim, amount: u64) -> Result<Verdict, ChargeError> {
        let Some(limit) = self.limits.get(dim) else {
            return Err(ChargeError::UnknownDimension(dim));
        };
        let current = self.spent.get_or(dim, 0);
        let new_spent = current.saturating_add(amount);
        // Discard: the previous spent value was already read above via
        // `get_or`; this write commits the new saturated total.
        let _ = self.spent.insert(dim, new_spent);

        if new_spent > limit {
            return Ok(Verdict::Exhausted(dim));
        }
        if let Some(warn) = self.warn_thresholds.get(dim) {
            if new_spent > warn {
                return Ok(Verdict::Warn(dim));
            }
        }
        Ok(Verdict::Continue)
    }

    /// Return the remaining headroom for `dim`, i.e. `limit - spent`
    /// saturating at zero. Returns `None` if the dimension was not
    /// declared.
    #[must_use]
    pub fn remaining(&self, dim: Dim) -> Option<u64> {
        let limit = self.limits.get(dim)?;
        let spent = self.spent.get_or(dim, 0);
        Some(limit.saturating_sub(spent))
    }

    /// Return the amount already charged against `dim`. Returns `None`
    /// if the dimension was not declared, and `Some(0)` if the dimension
    /// was declared but never charged.
    #[must_use]
    pub fn spent(&self, dim: Dim) -> Option<u64> {
        if self.limits.contains(dim) {
            Some(self.spent.get_or(dim, 0))
        } else {
            None
        }
    }

    /// Zero all spent counters, preserving the declared limits and warn
    /// thresholds. Useful for per-request budget reuse across a long-
    /// lived connection or worker.
    pub fn reset(&mut self) {
        self.spent = FixedMap::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_ok(builder: BudgetBuilder) -> Option<Budget> {
        let result = builder.build();
        assert!(result.is_ok());
        result.ok()
    }

    fn build_err(builder: BudgetBuilder) -> Option<BuilderError> {
        let result = builder.build();
        assert!(result.is_err());
        result.err()
    }

    #[test]
    fn build_rejects_empty_budget() {
        assert_eq!(
            build_err(Budget::builder()),
            Some(BuilderError::NoDimensions)
        );
    }

    #[test]
    fn build_rejects_zero_limit() {
        assert_eq!(
            build_err(Budget::builder().limit(Dim::Tokens, 0)),
            Some(BuilderError::ZeroLimit(Dim::Tokens))
        );
    }

    #[test]
    fn build_rejects_warn_equal_to_limit() {
        assert_eq!(
            build_err(Budget::builder().limit_with_warn(Dim::Tokens, 100, 100)),
            Some(BuilderError::WarnNotBelowLimit(Dim::Tokens))
        );
    }

    #[test]
    fn build_rejects_warn_above_limit() {
        assert_eq!(
            build_err(Budget::builder().limit_with_warn(Dim::Tokens, 100, 101)),
            Some(BuilderError::WarnNotBelowLimit(Dim::Tokens))
        );
    }

    #[test]
    fn build_rejects_duplicate_declaration() {
        assert_eq!(
            build_err(
                Budget::builder()
                    .limit(Dim::Tokens, 100)
                    .limit(Dim::Tokens, 200),
            ),
            Some(BuilderError::DuplicateDimension(Dim::Tokens))
        );
    }

    #[test]
    fn build_accepts_single_dimension() {
        let Some(budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.remaining(Dim::Tokens), Some(100));
    }

    #[test]
    fn build_accepts_all_dimensions() {
        let mut builder = Budget::builder();
        for dim in Dim::ALL {
            builder = builder.limit(dim, 1);
        }
        let Some(mut budget) = build_ok(builder) else {
            return;
        };
        for dim in Dim::ALL {
            assert_eq!(budget.charge(dim, 1), Ok(Verdict::Continue));
        }
    }

    #[test]
    fn charge_unknown_dim_returns_error() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Bytes, 1),
            Err(ChargeError::UnknownDimension(Dim::Bytes))
        );
    }

    #[test]
    fn charge_below_limit_returns_continue() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.charge(Dim::Tokens, 50), Ok(Verdict::Continue));
    }

    #[test]
    fn charge_exactly_to_limit_returns_continue() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.charge(Dim::Tokens, 100), Ok(Verdict::Continue));
    }

    #[test]
    fn charge_one_past_limit_returns_exhausted() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, 101),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
    }

    #[test]
    fn charge_incremental_to_exact_limit_returns_continue() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.charge(Dim::Tokens, 40), Ok(Verdict::Continue));
        assert_eq!(budget.charge(Dim::Tokens, 60), Ok(Verdict::Continue));
    }

    #[test]
    fn charge_incremental_past_limit_returns_exhausted() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.charge(Dim::Tokens, 40), Ok(Verdict::Continue));
        assert_eq!(
            budget.charge(Dim::Tokens, 61),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
    }

    #[test]
    fn charge_crosses_warn_returns_warn() {
        let Some(mut budget) = build_ok(Budget::builder().limit_with_warn(Dim::Tokens, 100, 80))
        else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, 81),
            Ok(Verdict::Warn(Dim::Tokens))
        );
    }

    #[test]
    fn warn_fires_every_call_above_threshold() {
        let Some(mut budget) = build_ok(Budget::builder().limit_with_warn(Dim::Tokens, 100, 80))
        else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, 81),
            Ok(Verdict::Warn(Dim::Tokens))
        );
        assert_eq!(
            budget.charge(Dim::Tokens, 1),
            Ok(Verdict::Warn(Dim::Tokens))
        );
    }

    #[test]
    fn charge_past_limit_reports_exhausted_not_warn() {
        let Some(mut budget) = build_ok(Budget::builder().limit_with_warn(Dim::Tokens, 100, 80))
        else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, 101),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
    }

    #[test]
    fn charge_saturates_at_u64_max() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, u64::MAX - 1)) else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, u64::MAX - 1),
            Ok(Verdict::Continue)
        );
        assert_eq!(
            budget.charge(Dim::Tokens, 10),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
        assert_eq!(budget.spent(Dim::Tokens), Some(u64::MAX));
    }

    #[test]
    fn charge_independent_across_dimensions() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100).limit_with_warn(
            Dim::Bytes,
            1000,
            500,
        )) else {
            return;
        };
        assert_eq!(budget.charge(Dim::Tokens, 100), Ok(Verdict::Continue));
        assert_eq!(
            budget.charge(Dim::Bytes, 600),
            Ok(Verdict::Warn(Dim::Bytes))
        );
        assert_eq!(budget.spent(Dim::Tokens), Some(100));
        assert_eq!(budget.spent(Dim::Bytes), Some(600));
    }

    #[test]
    fn remaining_reports_headroom() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.charge(Dim::Tokens, 30), Ok(Verdict::Continue));
        assert_eq!(budget.remaining(Dim::Tokens), Some(70));
    }

    #[test]
    fn remaining_saturates_at_zero_on_exhaustion() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, 200),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
        assert_eq!(budget.remaining(Dim::Tokens), Some(0));
    }

    #[test]
    fn remaining_none_for_undeclared_dim() {
        let Some(budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.remaining(Dim::Bytes), None);
    }

    #[test]
    fn spent_reports_zero_before_first_charge() {
        let Some(budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.spent(Dim::Tokens), Some(0));
    }

    #[test]
    fn spent_none_for_undeclared_dim() {
        let Some(budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        assert_eq!(budget.spent(Dim::Bytes), None);
    }

    #[test]
    fn reset_zeros_spent_preserves_limits() {
        let Some(mut budget) = build_ok(Budget::builder().limit_with_warn(Dim::Tokens, 100, 80))
        else {
            return;
        };
        assert_eq!(
            budget.charge(Dim::Tokens, 90),
            Ok(Verdict::Warn(Dim::Tokens))
        );
        budget.reset();
        assert_eq!(budget.spent(Dim::Tokens), Some(0));
        assert_eq!(budget.remaining(Dim::Tokens), Some(100));
        assert_eq!(
            budget.charge(Dim::Tokens, 100),
            Ok(Verdict::Warn(Dim::Tokens))
        );
    }

    #[test]
    fn reset_on_fresh_budget_is_noop() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };
        budget.reset();
        assert_eq!(budget.spent(Dim::Tokens), Some(0));
        assert_eq!(budget.remaining(Dim::Tokens), Some(100));
    }

    #[test]
    fn charge_zero_polls_state_without_consuming_budget() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };

        assert_eq!(budget.charge(Dim::Tokens, 50), Ok(Verdict::Continue));
        assert_eq!(budget.charge(Dim::Tokens, 0), Ok(Verdict::Continue));
        assert_eq!(budget.spent(Dim::Tokens), Some(50));
    }

    #[test]
    fn charge_zero_reports_exhausted_when_already_exhausted() {
        let Some(mut budget) = build_ok(Budget::builder().limit(Dim::Tokens, 100)) else {
            return;
        };

        assert_eq!(
            budget.charge(Dim::Tokens, 200),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
        assert_eq!(
            budget.charge(Dim::Tokens, 0),
            Ok(Verdict::Exhausted(Dim::Tokens))
        );
        assert_eq!(budget.spent(Dim::Tokens), Some(200));
    }

    #[test]
    fn warn_zero_fires_on_positive_charge_and_zero_poll_afterward() {
        let Some(mut budget) = build_ok(Budget::builder().limit_with_warn(Dim::Tokens, 100, 0))
        else {
            return;
        };

        assert_eq!(budget.charge(Dim::Tokens, 0), Ok(Verdict::Continue));
        assert_eq!(
            budget.charge(Dim::Tokens, 1),
            Ok(Verdict::Warn(Dim::Tokens))
        );
        assert_eq!(
            budget.charge(Dim::Tokens, 0),
            Ok(Verdict::Warn(Dim::Tokens))
        );
    }
}
