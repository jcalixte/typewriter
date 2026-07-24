//! Host-only visual check for baked fonts (Axis A). Renders sample lines with the
//! built-in `FONT_10X20` and every entry of `FONT_OPTIONS` (via `body_font`) onto
//! a real `Frame`, then dumps the raw 1-bit framebuffer for off-device inspection
//! (a companion script turns it into a PNG). Not shipped — a dev harness.
//!
//!   cargo run --example font_preview -- /tmp/font_preview.fb

use display::{body_font, Frame, FONT_OPTIONS, FB_BYTES, HEIGHT, WIDTH};
use embedded_graphics::mono_font::iso_8859_15::FONT_10X20;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::pixelcolor::BinaryColor;
use embedded_graphics::prelude::*;
use embedded_graphics::text::{Baseline, Text};
use std::io::Write;

const SAMPLE: &str = "The quick brown fox 0O o1lI| {}[] — \u{20ac}\u{153}\u{160}";

fn line(f: &mut Frame, label: &str, font_name: &str, y: i32) {
    let font = if font_name == "default" { &FONT_10X20 } else { body_font(font_name) };
    let style = MonoTextStyle::new(font, BinaryColor::On);
    let text = format!("[{label}] {SAMPLE}");
    Text::with_baseline(&text, Point::new(4, y), style, Baseline::Top)
        .draw(f)
        .unwrap();
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "/tmp/font_preview.fb".into());
    let mut f = Frame::new_white();

    // Fit every FONT_OPTIONS entry in the fixed device-height frame: keep the
    // roomy 30px pitch when there are few families, tighten it as more are added.
    let pitch = 30.min((HEIGHT as i32 - 4) / FONT_OPTIONS.len() as i32);
    let mut y = 4;
    for name in FONT_OPTIONS {
        line(&mut f, name, name, y);
        y += pitch;
    }

    std::fs::File::create(&out).unwrap().write_all(f.bytes()).unwrap();
    eprintln!("wrote {out} ({FB_BYTES} bytes, {WIDTH}x{HEIGHT})");
}
