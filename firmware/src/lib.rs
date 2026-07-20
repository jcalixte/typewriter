//! Shared library surface for the Typoena firmware crate.
//!
//! The editor binary (`src/main.rs`) and the spike binaries under `src/bin/`
//! are each separate crate roots; anything they share lives here, organised into
//! the clean-architecture tiers the C `../typing-machine` reference documents:
//!
//! - [`drivers`] — esp-idf implementations of the hardware ports: the SSD1683
//!   panel, the USB-host keyboard, Wi-Fi bring-up, and the esp clock/system
//!   adapters.
//! - [`infrastructure`] — adapters that fulfil the `app` ports over libraries
//!   and protocols: SD/FAT storage, the git push transport, the background
//!   file index, and the onboarding wizard's hardware I/O.
//!
//! The hardware and application frontiers themselves are the separate `hal` and
//! `app` crates (compiler-enforced, host-testable); the render engine
//! ([`app::Panel`]) and run loop ([`app::Runtime`]) live there. The modules
//! below only *implement* those ports and are wired together at composition in
//! `main.rs`.

pub mod drivers;
pub mod infrastructure;
