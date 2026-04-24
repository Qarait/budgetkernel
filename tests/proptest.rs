use budgetkernel::{Budget, ChargeError, Dim, Verdict, MAX_DIMS};
use proptest::prelude::*;

fn dim_strategy() -> impl Strategy<Value = Dim> {
    prop_oneof![
        Just(Dim::Tokens),
        Just(Dim::Millis),
        Just(Dim::Bytes),
        Just(Dim::Calls),
        Just(Dim::Memory),
        Just(Dim::Custom0),
        Just(Dim::Custom1),
        Just(Dim::Custom2),
    ]
}

fn limit_and_optional_warn_strategy() -> impl Strategy<Value = (u64, Option<u64>)> {
    (1u64..=1_000_000u64).prop_flat_map(|limit| {
        prop_oneof![
            Just((limit, None)),
            (0u64..limit).prop_map(move |warn| (limit, Some(warn))),
        ]
    })
}

fn limit_and_warn_strategy() -> impl Strategy<Value = (u64, u64)> {
    (1u64..=1_000_000u64).prop_flat_map(|limit| (Just(limit), 0u64..limit))
}

fn build_one_dim(dim: Dim, limit: u64, warn: Option<u64>) -> Budget {
    let builder = match warn {
        Some(warn) => Budget::builder().limit_with_warn(dim, limit, warn),
        None => Budget::builder().limit(dim, limit),
    };

    match builder.build() {
        Ok(budget) => budget,
        Err(err) => panic!("valid generated budget failed to build: {err}"),
    }
}

fn build_all_dims(limit: u64) -> Budget {
    let mut builder = Budget::builder();

    for dim in Dim::ALL {
        builder = builder.limit(dim, limit);
    }

    match builder.build() {
        Ok(budget) => budget,
        Err(err) => panic!("valid generated all-dim budget failed to build: {err}"),
    }
}

fn expected_verdict(dim: Dim, spent: u64, limit: u64, warn: Option<u64>) -> Verdict {
    if spent > limit {
        Verdict::Exhausted(dim)
    } else if let Some(warn) = warn {
        if spent > warn {
            Verdict::Warn(dim)
        } else {
            Verdict::Continue
        }
    } else {
        Verdict::Continue
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn single_dimension_charges_match_state_function(
        (limit, warn) in limit_and_optional_warn_strategy(),
        charges in prop::collection::vec(any::<u64>(), 0..128),
    ) {
        let dim = Dim::Tokens;
        let mut budget = build_one_dim(dim, limit, warn);
        let mut expected_spent = 0u64;

        for amount in charges {
            expected_spent = expected_spent.saturating_add(amount);

            let expected = expected_verdict(dim, expected_spent, limit, warn);
            let actual = budget.charge(dim, amount);

            prop_assert_eq!(actual, Ok(expected));
            prop_assert_eq!(budget.spent(dim), Some(expected_spent));
            prop_assert_eq!(budget.remaining(dim), Some(limit.saturating_sub(expected_spent)));
        }
    }

    #[test]
    fn spent_never_decreases_without_reset(
        (limit, warn) in limit_and_optional_warn_strategy(),
        charges in prop::collection::vec(any::<u64>(), 0..128),
    ) {
        let dim = Dim::Tokens;
        let mut budget = build_one_dim(dim, limit, warn);
        let mut previous = 0u64;

        for amount in charges {
            let _ = budget.charge(dim, amount);
            let current = budget.spent(dim);

            prop_assert!(current.is_some());
            let current = current.unwrap_or(0);

            prop_assert!(current >= previous);
            previous = current;
        }
    }

    #[test]
    fn all_dimensions_accumulate_independently(
        charges in prop::collection::vec((dim_strategy(), any::<u64>()), 0..128),
    ) {
        let mut budget = build_all_dims(u64::MAX);
        let mut expected = [0u64; MAX_DIMS];

        for (dim, amount) in charges {
            let idx = dim.index();
            expected[idx] = expected[idx].saturating_add(amount);

            let actual = budget.charge(dim, amount);
            prop_assert!(actual.is_ok());

            for check_dim in Dim::ALL {
                prop_assert_eq!(budget.spent(check_dim), Some(expected[check_dim.index()]));
            }
        }
    }

    #[test]
    fn unknown_dimension_charges_do_not_mutate_declared_dimensions(
        limit in 1u64..=1_000_000u64,
        declared_amounts in prop::collection::vec(any::<u64>(), 0..64),
        unknown_amount in any::<u64>(),
    ) {
        let mut budget = build_one_dim(Dim::Tokens, limit, None);

        for amount in declared_amounts {
            let _ = budget.charge(Dim::Tokens, amount);
        }

        let spent_before = budget.spent(Dim::Tokens);
        let remaining_before = budget.remaining(Dim::Tokens);

        let actual = budget.charge(Dim::Millis, unknown_amount);

        prop_assert_eq!(actual, Err(ChargeError::UnknownDimension(Dim::Millis)));
        prop_assert_eq!(budget.spent(Dim::Tokens), spent_before);
        prop_assert_eq!(budget.remaining(Dim::Tokens), remaining_before);
        prop_assert_eq!(budget.spent(Dim::Millis), None);
        prop_assert_eq!(budget.remaining(Dim::Millis), None);
    }

    #[test]
    fn reset_restores_initial_accounting_state(
        (limit, warn) in limit_and_optional_warn_strategy(),
        charges in prop::collection::vec(any::<u64>(), 0..128),
    ) {
        let dim = Dim::Tokens;
        let mut budget = build_one_dim(dim, limit, warn);

        for amount in charges {
            let _ = budget.charge(dim, amount);
        }

        budget.reset();

        prop_assert_eq!(budget.spent(dim), Some(0));
        prop_assert_eq!(budget.remaining(dim), Some(limit));
        prop_assert_eq!(budget.charge(dim, 0), Ok(Verdict::Continue));
    }

    #[test]
    fn inclusive_limit_exhausts_only_after_limit(
        limit in 1u64..=1_000_000u64,
    ) {
        let dim = Dim::Tokens;
        let mut budget = build_one_dim(dim, limit, None);

        prop_assert_eq!(budget.charge(dim, limit), Ok(Verdict::Continue));
        prop_assert_eq!(budget.spent(dim), Some(limit));

        prop_assert_eq!(budget.charge(dim, 1), Ok(Verdict::Exhausted(dim)));
        prop_assert_eq!(budget.spent(dim), Some(limit.saturating_add(1)));
    }

    #[test]
    fn warn_fires_before_exhaustion_for_any_valid_threshold(
        (limit, warn) in limit_and_warn_strategy(),
    ) {
        let dim = Dim::Tokens;
        let mut budget = build_one_dim(dim, limit, Some(warn));

        prop_assert_eq!(budget.charge(dim, warn), Ok(Verdict::Continue));
        prop_assert_eq!(budget.charge(dim, 1), Ok(Verdict::Warn(dim)));
        prop_assert_eq!(budget.spent(dim), Some(warn.saturating_add(1)));
        prop_assert_eq!(budget.remaining(dim), Some(limit.saturating_sub(warn.saturating_add(1))));
    }
}
