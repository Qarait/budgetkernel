# budgetkernel design

`budgetkernel` is a small, deterministic budget accounting kernel.

It answers one question:

> Given a declared budget and a sequence of charges, should the caller continue, degrade, or stop?

The crate is intentionally narrow. It does not measure time, read clocks, perform I/O, persist state, coordinate across machines, or refill budgets automatically. The caller owns those policies. The kernel owns only bounded accounting.

## Core guarantees

The design is built around these guarantees:

- **No heap allocation on the hot path.** Budget state is stored in fixed-size arrays keyed by `Dim`.
- **No clocks, no I/O, no syscalls.** The caller supplies elapsed time, token counts, byte counts, call counts, or any other measured value.
- **Deterministic behavior.** Same inputs and same starting state produce the same outputs.
- **No caller-triggerable panics.** Valid API usage returns `Result` or `Verdict`; it does not panic on overflow, missing dimensions, or exhausted budgets.
- **Saturating arithmetic.** Charge accumulation uses `u64::saturating_add`; remaining budget uses `u64::saturating_sub`.
- **Bounded work.** Public operations are `O(MAX_DIMS)` or better, with `MAX_DIMS = 8`.
- **`no_std` compatibility.** The core crate works without `std`; the `std` feature adds `std::error::Error` implementations.

`debug_assert!` is allowed for internal invariant checks. The no-panic guarantee means no caller-triggerable panics from valid API usage. A `debug_assert!` firing indicates a bug inside the kernel, not an expected runtime outcome.

## Dimensions

Dimensions are fixed:

```rust
pub enum Dim {
	Tokens,
	Millis,
	Bytes,
	Calls,
	Memory,
	Custom0,
	Custom1,
	Custom2,
}
```

There are exactly eight dimensions.

This is deliberate. Earlier designs considered generic or user-defined dimensions, but that would require user-provided discriminants or hashing. User-provided discriminants can alias accidentally, and hashing introduces unnecessary machinery. A fixed enum gives us:

dense array indexing
compiler-owned discriminants
no hashing
no allocation
easy auditability
stable behavior across feature configurations

The three Custom dimensions are the escape hatch for domain-specific units. They are still fixed slots; callers assign meaning in their own adapter code.

## Budget construction

Budgets are built with `Budget::builder()`:

```rust
let budget = Budget::builder()
	.limit_with_warn(Dim::Tokens, 10_000, 8_000)
	.limit(Dim::Calls, 50)
	.build()?;
```

The builder validates structural mistakes:

- duplicate dimensions
- zero limits
- warn thresholds greater than or equal to limits
- empty budgets

Duplicate declarations are rejected. After a duplicate is recorded, the builder's internal stored values are unspecified because `build()` will fail regardless. Callers must not rely on which duplicate declaration is retained internally.

## Mutable Budget in v0.1

The original design considered a split between an immutable policy and a mutable ledger:

```rust
let policy = BudgetPolicy::builder().build()?;
let mut ledger = policy.ledger();
```

v0.1 intentionally collapses this into one mutable `Budget`:

```rust
let mut budget = Budget::builder().build()?;
budget.charge(Dim::Tokens, 100)?;
```

This is simpler for the target use case: one request, one budget, one mutable accounting state.

One plausible future path is to introduce an immutable policy plus mutable ledger for shared-policy or multi-tenant use cases. That can be done without breaking v0.1 users by preserving `Budget::builder()` and making `Budget` a compatibility alias or wrapper around the mutable ledger shape.

## Charging

The core operation is:

```rust
pub fn charge(&mut self, dim: Dim, amount: u64) -> Result<Verdict, ChargeError>
```

The algorithm is:

1. Look up the declared limit for `dim`.
2. Return `ChargeError::UnknownDimension(dim)` if the dimension was not declared.
3. Read current spent, defaulting to zero.
4. Compute `new_spent = current.saturating_add(amount)`.
5. Store `new_spent`.
6. Return `Exhausted(dim)` if `new_spent > limit`.
7. Return `Warn(dim)` if a warn threshold exists and `new_spent > warn`.
8. Otherwise return `Continue`.

Spent is updated even when the budget becomes exhausted. This preserves diagnostic information about how far the caller attempted to go.

## Inclusive limits

Limits are inclusive.

If the limit is 100, then reaching exactly 100 is allowed:

```text
spent == limit     => Continue or Warn
spent > limit      => Exhausted
```

This is why exhaustion checks use `>` rather than `>=`.

## Warn semantics

Warn thresholds are strict and repeated.

If the warn threshold is 80, then:

```text
spent <= 80          => Continue
80 < spent <= limit  => Warn
spent > limit        => Exhausted
```

Warn is not one-shot. It fires on every charge where the current accounting state is above the warn threshold but not exhausted.

This avoids hidden suppression state. If callers want one-shot logging, they track that in their adapter.

## Exhaustion priority

Exhaustion wins over warning.

If one charge crosses both the warn threshold and the limit, the verdict is `Exhausted`, not `Warn`.

## Zero-amount charges

Charging zero is valid.

```rust
budget.charge(dim, 0)?;
```

A zero charge reports the verdict for the current state without increasing the reported spent value. It can be used as a state poll.

Because `Warn` is based on current state, a zero charge can return `Warn` or `Exhausted` if the budget was already warning or exhausted.

## `warn = 0`

A warn threshold of zero is valid.

It means any positive spend enters the warning state:

```text
spent == 0         => Continue
spent > 0          => Warn, unless exhausted
```

This is useful when callers want to observe the first nonzero spend.

## `Verdict::worst`

v0.1 does not provide atomic batch charging. Callers who want to charge multiple dimensions at one checkpoint perform several sequential `charge()` calls.

`Verdict::worst` helps reduce those sequential verdicts:

```rust
let mut verdict = Verdict::Continue;

verdict = verdict.worst(budget.charge(Dim::Tokens, tokens)?);
verdict = verdict.worst(budget.charge(Dim::Millis, millis)?);
verdict = verdict.worst(budget.charge(Dim::Calls, 1)?);
```

Severity order is:

- Exhausted
- Warn
- Continue

For equal severity, `worst` is left-biased. This keeps reduction deterministic.

This does not make multiple charges atomic. Batch charging remains a possible future API.

## Manual reset

`Budget::reset()` clears spent counters and preserves limits and warn thresholds.

This does not conflict with the non-goal of automatic refill. The distinction is:

- `reset()` is explicit, synchronous, caller-triggered, and clock-free.
- Automatic refill is time-based policy and belongs in a rate limiter or adapter.

The kernel provides the primitive. The caller decides when a budget period ends.

## Internal storage

Internally, `Budget` uses three fixed maps:

- limits
- warn thresholds
- spent counters

The fixed map is array-backed and keyed by `Dim::index()`.

The default implementation uses `MaybeUninit<u64>` plus presence bits. The `safe-map` feature uses fully initialized `[u64; MAX_DIMS]` storage instead.

Both variants expose the same internal API and pass the same tests.

## `safe-map` feature

`safe-map` removes unsafe code from the internal map implementation.

The default map avoids eager slot initialization. The `safe-map` variant initializes every slot to zero. In current local microbenchmarks, both variants have similar hot-path behavior. Do not rely on one feature configuration being universally faster than the other.

Use `safe-map` when you want a fully safe implementation for audit, policy, or confidence reasons.

## Non-goals

`budgetkernel` does not provide:

- automatic time-based refill
- rate limiting
- background tasks
- persistence
- distributed coordination
- async APIs
- dynamic dimension registration
- pricing tables
- clocks
- I/O
- logging

Adapters can build those behaviors around the kernel.