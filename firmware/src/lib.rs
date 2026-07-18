//! Shared library surface for the Typoena firmware crate.
//!
//! The editor binary (`src/main.rs`) and the spike binaries under `src/bin/`
//! are each separate crate roots; anything they need to share lives here and is
//! reached as `firmware::…`:
//!
//! - [`net`] — resilient Wi-Fi bring-up, extracted from three duplicated
//!   `connect_wifi` copies so the retry logic lives in exactly one place.
//! - [`persistence`] — SD mount + atomic save/load, graduated from the Spike 3
//!   bench binary so the editor and the spike share one implementation.
//! - [`epd`] — the SSD1683 panel driver, shared by the editor binary and the
//!   Spike 9 boot-splash bench binary so both drive the panel through one copy.
//! - [`usb_kbd`] — the USB-host boot-keyboard bridge, shared by the editor
//!   binary and the `kbd` bench binary (`just flash-kbd`) so a bare board can
//!   exercise the keyboard with no SD card through one copy.
//!
//! The panel render engine ([`app::Panel`]) has moved to the host-testable
//! `app` crate (generic over [`hal::Screen`]); the editor binary and the `demo`
//! bin both drive it from there.

pub mod adapters;
pub mod epd;
pub mod net;
pub mod persistence;
pub mod usb_kbd;

// On-device git publish (the editor's `:gp` transport). Behind the `git`
// feature so a light build never pulls libgit2/git2 — see main.rs `publish` and
// the feature note in Cargo.toml.
#[cfg(feature = "git")]
pub mod git_sync;
