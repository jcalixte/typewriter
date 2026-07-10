//! Shared library surface for the Typoena firmware crate.
//!
//! The editor binary (`src/main.rs`) and the spike binaries under `src/bin/`
//! are each separate crate roots; anything they need to share lives here and is
//! reached as `firmware::…`. Currently that is just the resilient Wi-Fi
//! bring-up, extracted from three duplicated `connect_wifi` copies so the retry
//! logic lives in exactly one place and the eventual editor integration reuses
//! it instead of growing a fourth copy.

pub mod net;
