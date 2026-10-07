//! Pure domain logic: no IO, no async, no clocks. Everything here is deterministic
//! and unit/property testable. See `docs/architecture.md`.

pub mod imaging;
pub mod settings;
pub mod units;
