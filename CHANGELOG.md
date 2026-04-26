# Changelog

## 0.1.1 - 2026-04-26

Minor release polishing and registry documentation improvements.

### Fixed

- Fixed stale doc comment regarding `std` feature implementation status in `lib.rs`.
- Corrected relative links in `README.md` to absolute URLs for better `crates.io` rendering.

### Added

- Added CI status and registry badges to `README.md`.
- Added project status indicators for release readiness.



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