# Changelog

## 0.1.2 - 2026-04-26

Public presentation polish.

### Fixed

- Fixed the GitHub Actions CI badge URL in `README.md`.
- Clarified zero-allocation wording as zero heap allocation on the hot path.
- Added a rustdoc-visible link to the security model.
- Ensured branch-specific public links point to `master`.

### Changed

- Updated the package description to use more precise hot-path allocation wording.

## 0.1.1 - 2026-04-26

Documentation cleanup and release polish.

### Fixed

- Updated README status now that the crate is published.
- Clarified `Verdict::Warn` documentation around inclusive limits.
- Updated fixed-map safety prose to match the current direct-indexing guard style.
- Renamed the exhausted benchmark case to clarify that it measures the already-exhausted steady-state path.

### Changed

- Added docs.rs metadata to `Cargo.toml`.
- Expanded CI example runtime coverage to include `safe-map`.

## 0.1.0 - 2026-04-25

Initial release.

### Added

- Fixed eight-dimension budget model via `Dim`.
- Mutable `Budget` with builder-based declaration.
- Single-dimension `charge()` API returning `Result<Verdict, ChargeError>`.
- Verdict states: `Continue`, `Warn(Dim)`, `Exhausted(Dim)`.
- `Verdict::worst` for reducing sequential charge results.
- Manual `reset()` for caller-controlled budget reuse.
- `remaining()` and `spent()` accounting queries.
- `no_std` support.
- `safe-map` feature for a fully safe internal map implementation.
- Unit tests, property tests, examples, benchmarks, design docs, and security model.