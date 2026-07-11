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

pub mod net;
pub mod persistence;
