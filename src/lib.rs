//! # budgetkernel
//!
//! Every system that consumes resources on behalf of a caller needs to
//! know when to stop. LLM agents burn tokens. Task runners burn CPU
//! time. API gateways burn request quotas. The stopping logic is always
//! the same: accumulate, compare, decide.
//!
//! `budgetkernel` isolates that decision into a deterministic,
//! zero-allocation accounting kernel. Declare budgets across fixed
//! dimensions, charge them at runtime boundaries, and receive one of
//! three verdicts: [`Verdict::Continue`], [`Verdict::Warn`], or
//! [`Verdict::Exhausted`].
//!
//! The kernel never reads a clock, never touches I/O, and never decides
//! what your program should do after a verdict. The caller owns time,
//! logging, persistence, and policy. The kernel owns exactly one thing:
//! given what has been spent and what is allowed, what should happen
//! next?
//!
//! ## Example
//!
//! ```rust
//! use budgetkernel::{Budget, Dim, Verdict};
//!
//! fn main() -> Result<(), ()> {
//!     let mut budget = Budget::builder()
//!         .limit_with_warn(Dim::Tokens, 100_000, 80_000)
//!         .limit_with_warn(Dim::Millis, 30_000, 27_000)
//!         .limit(Dim::Calls, 50)
//!         .build()
//!         .map_err(|_| ())?;
//!
//!     match budget.charge(Dim::Tokens, 12_500).map_err(|_| ())? {
//!         Verdict::Continue => {}
//!         Verdict::Warn(_dim) => {}
//!         Verdict::Exhausted(_dim) => {}
//!     }
//!
//!     Ok(())
//! }
//! ```
//!
//! No background threads. No timers. No global state. The same inputs
//! produce the same verdict on every machine, in every test, under Miri.
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
//! - **Adapter-layer philosophy.** The kernel does the accounting; the
//!   caller owns clocks, pricing, logging, and policy.
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

pub mod budget;
pub mod dim;
pub mod error;
pub(crate) mod fixed_map;
pub mod verdict;

pub use budget::{Budget, BudgetBuilder};
pub use dim::{Dim, MAX_DIMS};
pub use error::{BuilderError, ChargeError};
pub use verdict::Verdict;
