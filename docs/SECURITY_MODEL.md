# budgetkernel security model

`budgetkernel` is not a cryptographic library and does not authenticate, authorize, encrypt, or isolate processes.

Its security goal is narrower:

> Provide deterministic, bounded, panic-resistant budget accounting that can be safely embedded in larger systems.

This document describes what the crate guarantees, what it does not guarantee, and how the unsafe boundary is controlled.

## Threat model

The crate assumes callers may pass unfavorable values:

- very large charges
- `u64::MAX`
- zero charges
- unknown dimensions
- budgets that reach or exceed limits
- repeated calls after exhaustion

These inputs must not cause undefined behavior, allocation failures, integer overflow panics, indexing panics, or hidden I/O.

The crate does not defend against malicious code running in the same process. Rust code with access to `&mut Budget` can call public methods repeatedly. That is expected usage, not an attack boundary.

## Caller-visible guarantees

For valid API usage, the crate aims to guarantee:

- no heap allocation on the hot path
- no clocks
- no I/O
- no syscalls
- no caller-triggerable panics
- saturating arithmetic
- deterministic results
- bounded work
- explicit errors for unknown dimensions
- no unsafe code outside the internal fixed map module

## Panics

The crate distinguishes caller-triggerable panics from internal invariant assertions.

Caller-triggerable panics are forbidden. Examples include:

- panicking on integer overflow
- panicking on an undeclared dimension
- panicking because a budget is exhausted
- panicking from unchecked indexing in public logic
- panicking from `unwrap()` or `expect()` in production code

Internal `debug_assert!` checks are allowed. They guard invariants that should only be violated by bugs in the kernel itself. They compile out in release builds and are not part of normal caller-facing control flow.

## Unsafe boundary

The only current unsafe boundary is the default fixed map implementation.

The fixed map stores `u64` values in an array of `MaybeUninit<u64>` and tracks initialized slots with a parallel presence array.

The safe-map feature replaces this with fully initialized `[u64; MAX_DIMS]` storage and removes unsafe from that implementation.

## Fixed map invariants

The unsafe implementation relies on these invariants.

### 1. Slot initialization discipline

For every index `i`:

- if `present[i] == true`, then `slots[i]` contains an initialized valid `u64`
- if `present[i] == false`, then `slots[i]` must not be read

The only unsafe read is `assume_init()` after checking the presence bit.

### 2. Index bounds

All map access is keyed by `Dim`.

The index comes from `Dim::index()`, which maps the fixed enum variants into `0..MAX_DIMS`.

The map does not accept arbitrary caller-provided `usize` indexes.

### 3. No aliasing

The fixed map is an owned value. It has no interior mutability.

Mutation requires `&mut self`, so Rust's borrow checker enforces exclusive mutable access.

### 4. No drop hazards

The stored value type is `u64`.

`u64` is `Copy` and has no destructor. Dropping a map with uninitialized slots is safe because no destructor needs to run for uninitialized values.

If the map ever stores a non-`Copy` or destructor-bearing type, this module must be redesigned.

### 5. Deterministic stack storage

The map allocates no heap memory.

Its storage size is fixed by `MAX_DIMS`.

## Safe-map feature

The `safe-map` feature exists for audits and policy environments that prefer no unsafe code in the internal map.

It uses:

```rust
slots: [u64; MAX_DIMS]
present: [bool; MAX_DIMS]
```

All slots are initialized to zero.

The public behavior is identical to the default implementation. The test matrix runs the unit and property tests under both default and safe-map.

## Lint posture

The crate denies important panic-prone patterns in production code:

- `clippy::unwrap_used`
- `clippy::expect_used`
- `clippy::panic`
- `clippy::indexing_slicing`
- `unsafe_op_in_unsafe_fn`
- `missing_docs`

There are narrow exceptions:

- test modules may allow indexing because test panics are acceptable test failures
- the fixed map module allows `clippy::redundant_pub_crate` with a rationale: item-level `pub(crate)` is used as defense in depth
- localized indexing allows may appear near unsafe fixed-map access with explicit bounds reasoning

These exceptions should remain narrow and documented.

## Verification matrix

The current verification matrix is:

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

- deterministic unit tests
- public API property tests
- doctests
- tests under `safe-map`
- tests under `no_std`
- Miri for library tests

Miri is especially important because it exercises the unsafe fixed map path.

## Property tests

Property tests target the public API, not the internal fixed map.

They cover behavioral guarantees such as:

- accumulation matches saturating sums
- spent values do not decrease without reset
- dimensions accumulate independently
- unknown dimensions do not mutate declared dimensions
- reset restores initial accounting state
- inclusive limits exhaust only after the limit
- warn thresholds fire before exhaustion

The internal map is covered by unit tests and Miri.

## Benchmarks

Benchmarks are performance baselines, not security guarantees.

The benchmark suite measures:

- continuing single-dimension charge
- warning single-dimension charge
- exhausted single-dimension charge
- three sequential charges as a checkpoint pattern

The benchmark results should not be overclaimed. They are local measurements and may vary by CPU, compiler, target, optimization level, and feature configuration.

## Non-goals

This crate does not protect against:

- misuse of budget policy by the host application
- incorrect token/time/byte measurements supplied by the caller
- malicious code with process access
- distributed race conditions
- persistence failures
- clock skew
- rate-limit policy mistakes
- side-channel attacks
- denial-of-service outside the bounded work performed by this crate

The kernel is intentionally small. Adapters are responsible for measurement, persistence, distribution, logging, clocks, and policy.