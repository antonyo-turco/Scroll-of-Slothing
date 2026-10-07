//! Pure domain logic: no IO, no async, no clocks. Everything here is deterministic
//! and unit/property testable. See `docs/architecture.md`.

pub mod cards;
pub mod cutout;
pub mod fonts;
pub mod imaging;
pub mod pack;
pub mod settings;
pub mod token;
pub mod units;
