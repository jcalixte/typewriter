//! Baked alternate body fonts and the `font` pref → [`MonoFont`] registry.
//!
//! Axis A of the font feature: additional families rendered into the **same**
//! 10×20 cell as the built-in `FONT_10X20`, so the editor's fixed character grid
//! (`editor::{CW,CH}`) never moves. Each family is baked offline by
//! `display/tools/fontgen.py` into a 1-bit atlas that reuses eg's public
//! `mapping::ISO_8859_15`; only the ~5 KB atlas ships to flash — there is no
//! on-device rasterizer.

use embedded_graphics::mono_font::{iso_8859_15::FONT_10X20, MonoFont};

pub mod cascadia_mono;
pub mod courier_prime;
pub mod dejavu_sans_mono;
pub mod fira_code;
pub mod hack;
pub mod ibm_plex_mono;
pub mod jetbrains_mono;
pub mod mononoki;
pub mod space_mono;

/// The `font` pref values the palette cycles, in order. The head is the built-in
/// Misc Fixed (`"default"`); every other entry must be resolvable by
/// [`body_font`]. Kept here, beside [`body_font`], so the cyclable list and the
/// resolver can't drift apart.
pub const FONT_OPTIONS: [&str; 10] = [
    "default",
    "jetbrains-mono",
    "dejavu-sans-mono",
    "cascadia-mono",
    "mononoki",
    "fira-code",
    "courier-prime",
    "ibm-plex-mono",
    "space-mono",
    "hack",
];

/// Resolve a `font` pref value to its 10×20 body [`MonoFont`], falling back to
/// the built-in Misc Fixed `FONT_10X20` for `"default"` or any unrecognized value
/// (so a hand-typed `.typoena.toml` font name can never leave the editor without
/// a body font).
pub fn body_font(name: &str) -> &'static MonoFont<'static> {
    match name {
        "jetbrains-mono" => &jetbrains_mono::JETBRAINS_MONO_10X20,
        "dejavu-sans-mono" => &dejavu_sans_mono::DEJAVU_SANS_MONO_10X20,
        "cascadia-mono" => &cascadia_mono::CASCADIA_MONO_10X20,
        "mononoki" => &mononoki::MONONOKI_10X20,
        "fira-code" => &fira_code::FIRA_CODE_10X20,
        "courier-prime" => &courier_prime::COURIER_PRIME_10X20,
        "ibm-plex-mono" => &ibm_plex_mono::IBM_PLEX_MONO_10X20,
        "space-mono" => &space_mono::SPACE_MONO_10X20,
        "hack" => &hack::HACK_10X20,
        _ => &FONT_10X20,
    }
}
