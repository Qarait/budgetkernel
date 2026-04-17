//! Internal fixed-capacity map from [`Dim`] to `u64`.
//!
//! This file provides two implementations of [`FixedMap`], selected at
//! compile time by the `safe-map` feature flag:
//!
//! - **Default** (`cfg(not(feature = "safe-map"))`): backed by
//!   `[MaybeUninit<u64>; MAX_DIMS]`. Contains `unsafe` for reads; every
//!   `unsafe` block is guarded by the five invariants in the safety model
//!   below.
//! - **`safe-map`** (`cfg(feature = "safe-map")`): backed by
//!   `[u64; MAX_DIMS]`. Zero `unsafe`. Identical external behaviour;
//!   slots default to `0` before first insert.
//!
//! Both variants share the same test suite at the bottom of this file.
//!
//! ## Safety model (unsafe variant)
//!
//! 1. **Presence guards reads.** `present[i]` is the sole authority on
//!    whether `slots[i]` may be read. Every `assume_init` call is preceded
//!    by a `.get(idx).copied().unwrap_or(false)` guard in the same function
//!    body. No code path reads a slot without that guard in place.
//!
//! 2. **Bounded indexing.** All slot access uses [`Dim::index()`] as the
//!    subscript, which the enum's exhaustive discriminant assignment holds
//!    to `0..MAX_DIMS`. No raw `usize` is accepted at the public boundary.
//!    `.get(idx)` / `.get_mut(idx)` are used throughout; direct `arr[idx]`
//!    indexing does not appear in this file.
//!
//! 3. **Exclusive mutation.** Plain owned struct, no interior mutability.
//!    All writes go through `&mut self`; the borrow checker guarantees no
//!    concurrent access to the same slot.
//!
//! 4. **Trivial drop.** `u64: Copy`; forgetting an initialised slot causes
//!    no resource leak. If the value type ever becomes non-`Copy`, this
//!    module must gain a `Drop` impl that drops every slot where
//!    `present[i]` is `true`.
//!
//! 5. **Zero allocation, constant footprint.** Stack only. Every method is
//!    `O(1)`; `len` is `O(MAX_DIMS)` with `MAX_DIMS` fixed at compile time.

// We use `pub(crate)` on items in this module even though the module
// itself is `pub(crate)`. The item-level visibility is defense in depth:
// if the module is ever promoted to `pub` during a refactor, our internal
// primitive will not silently leak.
#![allow(clippy::redundant_pub_crate)]

#[cfg(not(feature = "safe-map"))]
use core::mem::MaybeUninit;

use crate::dim::{Dim, MAX_DIMS};

// ── Unsafe variant ────────────────────────────────────────────────────────────

/// A fixed-capacity map from every [`Dim`] variant to a `u64` value —
/// `MaybeUninit`-backed unsafe variant.
///
/// A slot may only be read when the matching `present` flag is `true`.
/// See the module-level safety model for the full set of invariants.
#[cfg(not(feature = "safe-map"))]
pub(crate) struct FixedMap {
    slots: [MaybeUninit<u64>; MAX_DIMS],
    present: [bool; MAX_DIMS],
}

#[cfg(not(feature = "safe-map"))]
impl FixedMap {
    /// Create an empty map. Every slot starts absent.
    ///
    /// `[MaybeUninit::uninit(); MAX_DIMS]` is the standard idiom for
    /// initialising an array of `MaybeUninit` without `unsafe` code.
    pub(crate) const fn new() -> Self {
        Self {
            slots: [MaybeUninit::uninit(); MAX_DIMS],
            present: [false; MAX_DIMS],
        }
    }

    /// Return the value stored for `dim`, or `None` if absent.
    pub(crate) fn get(&self, dim: Dim) -> Option<u64> {
        let idx = dim.index();
        if self.present.get(idx).copied().unwrap_or(false) {
            let mu = self.slots.get(idx).copied();
            // SAFETY: Invariant 1 — the guard above confirmed present[idx]
            // is true, so slots[idx] was written by a prior insert call and
            // holds a valid, initialised u64.
            // Invariant 2 — idx is Dim::index(), in 0..MAX_DIMS, so
            // slots.get(idx) returns Some and mu is Some.
            mu.map(|s| unsafe { s.assume_init() })
        } else {
            None
        }
    }

    /// Store `value` for `dim`, returning the previous value if one existed.
    ///
    /// Writing `MaybeUninit::new(value)` into a slot is always safe — it
    /// initialises the slot regardless of prior state. The only `unsafe`
    /// block reads the *previous* value, which requires the presence guard.
    pub(crate) fn insert(&mut self, dim: Dim, value: u64) -> Option<u64> {
        let idx = dim.index();
        let prev = if self.present.get(idx).copied().unwrap_or(false) {
            let mu = self.slots.get(idx).copied();
            // SAFETY: Invariant 1 — present[idx] is true, so slots[idx]
            // holds a valid, initialised u64 from a prior insert call.
            // Invariant 2 — idx is Dim::index(), in 0..MAX_DIMS.
            mu.map(|s| unsafe { s.assume_init() })
        } else {
            None
        };
        if let Some(slot) = self.slots.get_mut(idx) {
            *slot = MaybeUninit::new(value);
        }
        if let Some(flag) = self.present.get_mut(idx) {
            *flag = true;
        }
        prev
    }

    /// Return the value for `dim`, or `default` if absent.
    pub(crate) fn get_or(&self, dim: Dim, default: u64) -> u64 {
        self.get(dim).unwrap_or(default)
    }

    /// Return `true` if a value has been stored for `dim`.
    pub(crate) fn contains(&self, dim: Dim) -> bool {
        self.present.get(dim.index()).copied().unwrap_or(false)
    }

    /// Return the number of dimensions that have a stored value.
    ///
    /// Iterates the presence array; no `unsafe` code required.
    pub(crate) fn len(&self) -> usize {
        self.present.iter().filter(|&&b| b).count()
    }
}

// ── Safe variant ──────────────────────────────────────────────────────────────

/// A fixed-capacity map from every [`Dim`] variant to a `u64` value —
/// plain-array, fully-safe variant.
///
/// Enabled with `--features safe-map`. Externally identical to the default
/// variant. Unset slots hold `0`; a slot is only returned by `get` once
/// `insert` has been called for that dimension.
#[cfg(feature = "safe-map")]
pub(crate) struct FixedMap {
    slots: [u64; MAX_DIMS],
    present: [bool; MAX_DIMS],
}

#[cfg(feature = "safe-map")]
impl FixedMap {
    /// Create an empty map. Every slot starts absent; values default to `0`.
    pub(crate) const fn new() -> Self {
        Self {
            slots: [0u64; MAX_DIMS],
            present: [false; MAX_DIMS],
        }
    }

    /// Return the value stored for `dim`, or `None` if absent.
    pub(crate) fn get(&self, dim: Dim) -> Option<u64> {
        let idx = dim.index();
        if self.present.get(idx).copied().unwrap_or(false) {
            self.slots.get(idx).copied()
        } else {
            None
        }
    }

    /// Store `value` for `dim`, returning the previous value if one existed.
    pub(crate) fn insert(&mut self, dim: Dim, value: u64) -> Option<u64> {
        let idx = dim.index();
        let prev = if self.present.get(idx).copied().unwrap_or(false) {
            self.slots.get(idx).copied()
        } else {
            None
        };
        if let Some(slot) = self.slots.get_mut(idx) {
            *slot = value;
        }
        if let Some(flag) = self.present.get_mut(idx) {
            *flag = true;
        }
        prev
    }

    /// Return the value for `dim`, or `default` if absent.
    pub(crate) fn get_or(&self, dim: Dim, default: u64) -> u64 {
        self.get(dim).unwrap_or(default)
    }

    /// Return `true` if a value has been stored for `dim`.
    pub(crate) fn contains(&self, dim: Dim) -> bool {
        self.present.get(dim.index()).copied().unwrap_or(false)
    }

    /// Return the number of dimensions that have a stored value.
    ///
    /// Iterates the presence array; no `unsafe` code anywhere in this variant.
    pub(crate) fn len(&self) -> usize {
        self.present.iter().filter(|&&b| b).count()
    }
}

// ── Shared tests ──────────────────────────────────────────────────────────────
//
// No cfg gate on this module — it compiles against whichever FixedMap variant
// is active. Both variants must pass all tests.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dim::Dim;

    #[test]
    fn new_map_is_empty() {
        let m = FixedMap::new();
        assert_eq!(m.len(), 0);
        for dim in Dim::ALL {
            assert!(!m.contains(dim));
            assert!(m.get(dim).is_none());
        }
    }

    #[test]
    fn insert_and_get_roundtrip() {
        let mut m = FixedMap::new();
        assert!(m.insert(Dim::Tokens, 42).is_none());
        assert_eq!(m.get(Dim::Tokens), Some(42));
        assert!(m.contains(Dim::Tokens));
    }

    #[test]
    fn insert_returns_previous_value() {
        let mut m = FixedMap::new();
        m.insert(Dim::Tokens, 10);
        let prev = m.insert(Dim::Tokens, 20);
        assert_eq!(prev, Some(10));
        assert_eq!(m.get(Dim::Tokens), Some(20));
    }

    #[test]
    fn get_or_returns_default_when_absent() {
        let m = FixedMap::new();
        assert_eq!(m.get_or(Dim::Millis, 99), 99);
    }

    #[test]
    fn get_or_returns_stored_value_when_present() {
        let mut m = FixedMap::new();
        m.insert(Dim::Millis, 7);
        assert_eq!(m.get_or(Dim::Millis, 99), 7);
    }

    #[test]
    fn len_tracks_distinct_dimensions() {
        let mut m = FixedMap::new();
        assert_eq!(m.len(), 0);
        m.insert(Dim::Tokens, 1);
        assert_eq!(m.len(), 1);
        m.insert(Dim::Bytes, 2);
        assert_eq!(m.len(), 2);
        // Re-inserting the same dim must not increment len.
        m.insert(Dim::Tokens, 3);
        assert_eq!(m.len(), 2);
    }

    #[test]
    fn dimensions_are_independent() {
        let mut m = FixedMap::new();
        m.insert(Dim::Tokens, 100);
        m.insert(Dim::Bytes, 200);
        assert_eq!(m.get(Dim::Tokens), Some(100));
        assert_eq!(m.get(Dim::Bytes), Some(200));
        assert!(m.get(Dim::Millis).is_none());
    }

    #[test]
    fn all_dims_can_be_inserted_and_read() {
        let mut m = FixedMap::new();
        for (i, dim) in Dim::ALL.iter().enumerate() {
            m.insert(*dim, i as u64);
        }
        assert_eq!(m.len(), MAX_DIMS);
        for (i, dim) in Dim::ALL.iter().enumerate() {
            assert_eq!(m.get(*dim), Some(i as u64));
        }
    }

    #[test]
    fn insert_overwrites_without_growing_len() {
        let mut m = FixedMap::new();
        m.insert(Dim::Calls, 1);
        m.insert(Dim::Calls, 2);
        m.insert(Dim::Calls, 3);
        assert_eq!(m.len(), 1);
        assert_eq!(m.get(Dim::Calls), Some(3));
    }
}
