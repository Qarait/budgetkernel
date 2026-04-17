//! # budgetkernel
//!
//! A small, auditable, deterministic, zero-allocation budget accounting
//! kernel. Declare budgets across fixed dimensions (tokens, milliseconds,
//! bytes, calls, memory, and three user-assignable custom slots), charge
//! them at runtime boundaries, and receive a verdict: `Continue`, `Warn`,
//! or `Exhausted`.
//!
//! ## Design discipline
//!
//! - **No heap allocation on the hot path.** All state lives on the stack
//!   in fixed-size arrays sized by [`MAX_DIMS`].
//! - **No clock, no I/O, no syscalls.** The caller supplies elapsed time
//!   as a `u64` when charging the time dimension. Determinism follows.
//! - **No panics.** Every fallible operation returns a `Result` or a
//!   `Verdict` variant. Arithmetic is saturating throughout.
//! - **Bounded termination.** Every public function is `O(MAX_DIMS)` or
//!   better, with `MAX_DIMS` fixed at compile time.
//! - **`no_std` compatible.** The `std` feature enables convenience
//!   impls but is not required for core use.
//!
//! ## Features
//!
//! - `std` (default): enables `std::error::Error` impls for error types
//!   (wired in a later phase).
//! - `safe-map`: replaces the `MaybeUninit`-based internal map with a
//!   fully-safe variant. Identical semantics, slightly higher per-call
//!   initialization cost. No `unsafe` anywhere in the crate when this
//!   feature is active.
//!
//! ## Non-goals
//!
//! This crate does not and will not:
//!
//! - Refill budgets over time (that is a rate limiter's job).
//! - Persist ledger state (the host owns durability).
//! - Coordinate across processes or machines.
//! - Provide async APIs.
//! - Allow dynamic dimension registration (the dimension set is
//!   compile-time fixed; see [`Dim`]).

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(
    clippy::unwrap_used,
    clippy::panic,
    clippy::expect_used,
    clippy::indexing_slicing,
    unsafe_op_in_unsafe_fn,
    missing_docs
)]
#![warn(clippy::pedantic, clippy::nursery)]

pub mod dim;
pub(crate) mod fixed_map;

pub(crate) use fixed_map::FixedMap;

pub use dim::{Dim, MAX_DIMS};
