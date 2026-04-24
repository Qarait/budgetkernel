use budgetkernel::{Budget, Dim};
use criterion::{criterion_group, criterion_main, Criterion};
use std::hint::black_box;

#[cfg(feature = "safe-map")]
const BENCH_GROUP: &str = "charge/safe-map";

#[cfg(not(feature = "safe-map"))]
const BENCH_GROUP: &str = "charge/default-map";

fn build_continue_budget() -> Budget {
    match Budget::builder()
        .limit(Dim::Tokens, black_box(u64::MAX))
        .build()
    {
        Ok(budget) => budget,
        Err(_) => std::process::abort(),
    }
}

fn build_warn_budget() -> Budget {
    match Budget::builder()
        .limit_with_warn(Dim::Tokens, black_box(u64::MAX), black_box(0))
        .build()
    {
        Ok(budget) => budget,
        Err(_) => std::process::abort(),
    }
}

fn build_exhausted_budget() -> Budget {
    let mut budget = match Budget::builder().limit(Dim::Tokens, black_box(1)).build() {
        Ok(budget) => budget,
        Err(_) => std::process::abort(),
    };

    let _ = budget.charge(Dim::Tokens, black_box(2));
    budget
}

fn build_three_dim_budget() -> Budget {
    match Budget::builder()
        .limit(Dim::Tokens, black_box(u64::MAX))
        .limit(Dim::Millis, black_box(u64::MAX))
        .limit(Dim::Calls, black_box(u64::MAX))
        .build()
    {
        Ok(budget) => budget,
        Err(_) => std::process::abort(),
    }
}

fn charge_benches(c: &mut Criterion) {
    let mut group = c.benchmark_group(BENCH_GROUP);

    // Black-box the mutable budget reference so the optimizer cannot
    // treat the internal spent counters as unobservable benchmark state.
    group.bench_function("continue_single_dim", |b| {
        let mut budget = build_continue_budget();

        b.iter(|| {
            let budget = black_box(&mut budget);
            let verdict = budget.charge(black_box(Dim::Tokens), black_box(1));
            let _ = black_box(verdict);
        });
    });

    group.bench_function("warn_single_dim", |b| {
        let mut budget = build_warn_budget();

        b.iter(|| {
            let budget = black_box(&mut budget);
            let verdict = budget.charge(black_box(Dim::Tokens), black_box(1));
            let _ = black_box(verdict);
        });
    });

    group.bench_function("exhausted_single_dim", |b| {
        let mut budget = build_exhausted_budget();

        b.iter(|| {
            let budget = black_box(&mut budget);
            let verdict = budget.charge(black_box(Dim::Tokens), black_box(1));
            let _ = black_box(verdict);
        });
    });

    group.bench_function("three_dimension_checkpoint", |b| {
        let mut budget = build_three_dim_budget();

        b.iter(|| {
            let budget = black_box(&mut budget);

            let tokens = budget.charge(black_box(Dim::Tokens), black_box(1_523));
            let millis = budget.charge(black_box(Dim::Millis), black_box(842));
            let calls = budget.charge(black_box(Dim::Calls), black_box(1));

            let _ = black_box((tokens, millis, calls));
        });
    });

    group.finish();
}

criterion_group!(benches, charge_benches);
criterion_main!(benches);
