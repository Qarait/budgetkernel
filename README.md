# budgetkernel

A small, auditable, deterministic, zero-allocation budget accounting kernel.

Declare budgets across fixed dimensions, charge them at runtime boundaries, and get a verdict:

```text
Continue
Warn(dim)
Exhausted(dim)
```

The crate is intentionally narrow. It does not read clocks, perform I/O, allocate on the hot path, refill budgets automatically, persist state, or coordinate across machines. The caller owns measurement and policy. `budgetkernel` owns bounded accounting.

## Why this exists

LLM pipelines, task runners, crawlers, quota systems, and agent loops often need to track more than one resource at once:

1. tokens
2. elapsed milliseconds
3. bytes
4. calls
5. memory
6. caller-defined custom units

Most systems do this with ad-hoc counters. `budgetkernel` provides a small kernel for that accounting with explicit semantics and a well-defined verification story.

It is not a rate limiter. It is not a metrics system. It is not a distributed quota service.

It is the deterministic core those systems can build around.

## Example

```rust
use budgetkernel::{Budget, Dim, Verdict};

fn main() -> Result<(), Box<dyn std::error::Error>> {
	let mut budget = Budget::builder()
		.limit_with_warn(Dim::Tokens, 10_000, 8_000)
		.limit_with_warn(Dim::Millis, 30_000, 27_000)
		.limit_with_warn(Dim::Calls, 5, 4)
		.build()?;

	let mut verdict = Verdict::Continue;

	verdict = verdict.worst(budget.charge(Dim::Tokens, 1_523)?);
	verdict = verdict.worst(budget.charge(Dim::Millis, 842)?);
	verdict = verdict.worst(budget.charge(Dim::Calls, 1)?);

	match verdict {
		Verdict::Continue => {
			// Keep going.
		}
		Verdict::Warn(dim) => {
			// Still allowed, but consider degrading or preempting.
			println!("warning: {} budget is getting low", dim.name());
		}
		Verdict::Exhausted(dim) => {
			// Stop and return a partial result.
			println!("exhausted: {}", dim.name());
		}
	}

	Ok(())
}
```

For complete examples, see:

1. `examples/llm_pipeline.rs`
2. `examples/task_runner.rs`
3. `examples/http_quota.rs`

## Core API

Build a budget:

```rust
let mut budget = Budget::builder()
	.limit(Dim::Calls, 50)
	.limit_with_warn(Dim::Tokens, 100_000, 80_000)
	.build()?;
```

Charge one dimension:

```rust
let verdict = budget.charge(Dim::Tokens, 1_000)?;
```

Query accounting state:

```rust
let spent = budget.spent(Dim::Tokens);
let remaining = budget.remaining(Dim::Tokens);
```

Manually reset spent counters:

```rust
budget.reset();
```

`reset()` preserves declared limits and warn thresholds. It does not perform automatic refill. The caller decides when a budget period ends.

## Dimensions

The dimension set is fixed:

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

The fixed set is deliberate. It avoids dynamic registration, string keys, hashing, allocation, and user-provided discriminants. Internally, dimensions map to dense array indexes.

The three `Custom` slots are for caller-defined units. For example, an adapter may define `Custom0` as "work units" or "retrieval depth" in its own codebase.

## Verdicts

`Budget::charge` returns:

```rust
Result<Verdict, ChargeError>
```

`ChargeError` reports structural errors, such as charging an undeclared dimension.

`Verdict` reports the state of an accepted charge:

```rust
pub enum Verdict {
	Continue,
	Warn(Dim),
	Exhausted(Dim),
}
```

### Continue

The charge was accepted and no configured warn threshold was crossed.

### Warn

The charge was accepted, but the running total is now above the configured warn threshold.

Warn is not one-shot. It fires on every charge where the current state is above the warn threshold but not exhausted. If callers want one-shot logging, they should track suppression in their adapter.

### Exhausted

The charge pushed the running total past the configured limit.

Limits are inclusive:

```text
spent == limit     => Continue or Warn
spent > limit      => Exhausted
```

Exhaustion wins over warning. If one charge crosses both the warn threshold and the limit, the verdict is `Exhausted`.

## Sequential multi-dimension checkpoints

v0.1 intentionally ships single-dimension charging only:

```rust
budget.charge(dim, amount)?;
```

Callers who want to check several dimensions at one checkpoint can perform several sequential charges and reduce the verdicts with `Verdict::worst`:

```rust
let mut verdict = Verdict::Continue;

verdict = verdict.worst(budget.charge(Dim::Tokens, tokens)?);
verdict = verdict.worst(budget.charge(Dim::Millis, millis)?);
verdict = verdict.worst(budget.charge(Dim::Calls, 1)?);
```

This is not atomic batch charging. It is a deterministic reduction of sequential results.

Batch charging may be added later if real usage justifies it.

## Zero charges and `warn = 0`

Charging zero is valid:

```rust
budget.charge(dim, 0)?;
```

A zero charge reports the verdict for the current state without increasing the reported spent value. This can be used as a state poll.

A warn threshold of zero is also valid. It means the first positive spend enters the warning state.

## Design guarantees

The crate is designed around these guarantees:

1. no heap allocation on the hot path
2. no clocks
3. no I/O
4. no syscalls
5. deterministic behavior
6. saturating arithmetic
7. bounded work
8. no caller-triggerable panics
9. `no_std` compatibility
10. one current unsafe boundary, isolated in the internal fixed map implementation

The no-panic guarantee means no caller-triggerable panics from valid API usage. Internal `debug_assert!` checks may guard kernel invariants during debug builds.

See [docs/DESIGN.md](docs/DESIGN.md) for the full design rationale.

## Safety model

The default internal map uses `MaybeUninit<u64>` plus presence bits. This is the only current unsafe boundary.

The `safe-map` feature replaces that implementation with fully initialized arrays and removes unsafe from the map implementation.

Both variants are tested with the same unit and property tests.

See [docs/SECURITY_MODEL.md](docs/SECURITY_MODEL.md) for invariants, threat model, lint posture, and verification details.

## Feature flags

```toml
[features]
default = ["std"]
std = []
safe-map = []
```

### `std`

Enabled by default.

Adds `std::error::Error` implementations for error types.

The core accounting logic does not require `std`.

### `safe-map`

Uses a fully safe internal fixed map implementation.

This is useful for audits or policy environments that prefer no unsafe code in the internal map. The public behavior is identical.

### `no_std`

Build without default features:

```bash
cargo build --no-default-features
```

The crate remains usable without `std`. The caller is still responsible for measurement, clocks, logging, persistence, and any adapter behavior.

## Verification

Current verification matrix:

```text
cargo build
cargo build --no-default-features
cargo build --features safe-map
cargo build --no-default-features --features safe-map

cargo test
cargo test --no-default-features
cargo test --features safe-map
cargo test --no-default-features --features safe-map

cargo clippy --all-targets --all-features -- -D warnings
cargo fmt --check
cargo doc --no-deps
cargo doc --no-deps --no-default-features
cargo +nightly miri test --lib
```

The test suite includes:

1. deterministic unit tests
2. public API property tests
3. doctests
4. tests under `safe-map`
5. tests under `no_std`
6. MIRI for library tests

## Benchmarks

A Criterion benchmark suite is included:

```bash
cargo bench --bench charge
cargo bench --bench charge --features safe-map
```

It measures:

1. continuing single-dimension charge
2. warning single-dimension charge
3. exhausted single-dimension charge
4. three sequential charges as a checkpoint pattern

Benchmark numbers are local measurements and vary by CPU, compiler, target, optimization level, and feature configuration. Treat them as a baseline for your environment, not a universal guarantee.

## Non-goals

`budgetkernel` does not provide:

1. automatic time-based refill
2. rate limiting
3. background tasks
4. persistence
5. distributed coordination
6. async APIs
7. dynamic dimension registration
8. model pricing tables
9. clocks
10. I/O
11. logging

Adapters can build those behaviors around the kernel.

## Status

Work in progress toward v0.1.0.

The core kernel, examples, property tests, benchmarks, design/security docs, and README are in place. The crate is release-ready for v0.1.0. Publication to crates.io is pending.