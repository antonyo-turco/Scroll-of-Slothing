//! Pure domain logic: no IO, no async, no clocks. Everything here is deterministic
//! and unit/property testable. See `docs/architecture.md`.

pub mod aoe;
pub mod cards;
pub mod combat;
pub mod cutout;
pub mod fonts;
pub mod imaging;
pub mod maps;
pub mod music;
pub mod pack;
pub mod settings;
pub mod stt;
pub mod token;
pub mod units;
