//! Infrastructure — adapters that fulfil the `app` ports over libraries and
//! protocols.
//!
//! SD/FAT storage ([`storage_sd`], `app::Storage`), the background palette file
//! index ([`file_index`], `app::FileIndex`), the git push transport plus its
//! `app::NetService` ([`net`]), and the onboarding wizard's hardware I/O
//! ([`wizard_io`]). Mirrors the `infrastructure/` tier of the C
//! `../typing-machine` reference.

pub mod file_index;
pub mod storage_sd;

// The net transport is feature-gated: it pulls libgit2, which the standalone
// bench bins (`sd_bench`, `wifi_tls`) build without. The firmware bin always
// sets `full`, so it always links it.
#[cfg(feature = "full")]
pub mod net;

// Over-the-air firmware update (the `:update` transport). Rides the same
// radio-owning git thread as sync — no libgit2, just esp-idf's HTTPS + OTA — so
// it is gated with the full build and dispatched through `net`.
#[cfg(feature = "full")]
pub mod ota;

// The onboarding wizard's end state is a clone, so it only exists in the git
// build (see the `wizard` optional dep in Cargo.toml).
#[cfg(feature = "full")]
pub mod wizard_io;
