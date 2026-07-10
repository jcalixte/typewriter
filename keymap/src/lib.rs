//! Pure HID boot-keyboard decode — the logic half of `firmware/src/usb_kbd.rs`,
//! extracted so it can be built and tested on the host (the firmware crate is
//! pinned to the xtensa target and can't run `cargo test`).
//!
//! It owns nothing hardware-shaped: no USB transfers, no logging, no globals.
//! You feed it raw 8-byte boot reports and it emits decoded [`Key`] events via
//! a callback. `firmware` wires the USB interrupt endpoint to [`Decoder::feed`];
//! tests here drive it directly.
//!
//! Why this is the module worth testing: [`Decoder::feed`] is the one place
//! device-controlled bytes are parsed, and [`translate`] is the sole source of
//! `Key::Char`, whose ASCII-only guarantee the editor's byte==char indexing
//! relies on. Both invariants are pinned by the tests below. See MEMORY_AUDIT.md.

#![cfg_attr(not(test), no_std)]
#![forbid(unsafe_code)]

/// A decoded key-down event. Beyond plain characters, the decoder recognises a
/// few editing combos (resolved here so the main loop only sees intents) and a
/// dual-role Caps Lock: held it acts as Ctrl, tapped it emits `Escape`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Enter,
    Backspace,
    /// Ctrl+Backspace or Ctrl+W — delete the word before the caret.
    DeleteWord,
    /// Cmd/GUI+Backspace — delete back to the start of the current line.
    DeleteLine,
    /// Caps Lock tapped on its own. A no-op for now; groundwork for a future
    /// vim-style normal mode.
    Escape,
}

/// Caps Lock usage ID — repurposed as a dual-role Ctrl/Escape key.
const CAPS: u8 = 0x39;

/// Edge-detecting boot-report decoder. Holds the previous report's key slots
/// (for key-down edge detection) and the Caps dual-role state. Construct once
/// per attached keyboard; call [`reset`](Decoder::reset) on detach.
#[derive(Debug, Clone)]
pub struct Decoder {
    /// Keycodes held in the previous report.
    prev: [u8; 6],
    /// Set while Caps is held once any other key is pressed, so releasing Caps
    /// only emits `Escape` on a clean tap.
    caps_used: bool,
}

impl Default for Decoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Decoder {
    pub const fn new() -> Self {
        Self { prev: [0; 6], caps_used: false }
    }

    /// Clear all state (call when the keyboard is unplugged so a stale "held"
    /// slot from the old device can't suppress the first key of the next one).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Edge-detect key-downs in an 8-byte boot report and emit translated keys.
    /// Layout: `[modifiers, reserved, key1..key6]`; `0` means "no key". Robust
    /// to any slice length — a short report (< 3 bytes) is ignored, and extra
    /// bytes past the six key slots are simply processed too, never indexed
    /// out of range.
    pub fn feed(&mut self, report: &[u8], mut emit: impl FnMut(Key)) {
        if report.len() < 3 {
            return;
        }
        let mods = report[0];
        let shift = mods & 0x22 != 0; // LShift 0x02 | RShift 0x20
        let cmd = mods & 0x88 != 0; // LGUI 0x08 | RGUI 0x80
        let current = &report[2..];

        // Caps Lock is a normal key in the boot report (not a modifier bit), so
        // we track its down/up edges here. Held, it acts as Ctrl; tapped alone,
        // it emits Escape.
        let caps_now = current.contains(&CAPS);
        let caps_before = self.prev.contains(&CAPS);
        let ctrl = mods & 0x11 != 0 || caps_now; // LCtrl 0x01 | RCtrl 0x10, or Caps
        // Any other key down while Caps is held means it was used as Ctrl — so
        // its release must not fire Escape.
        if caps_now && current.iter().any(|&k| k != 0 && k != CAPS) {
            self.caps_used = true;
        }

        for &k in current {
            if k == 0 || k == CAPS || self.prev.contains(&k) {
                continue; // empty slot, the Caps key itself, or already held
            }
            if let Some(key) = translate(k, shift, ctrl, cmd) {
                emit(key);
            }
        }

        // Caps released as a clean tap (nothing else pressed while it was down)
        // → Escape. Reset the used-flag on both the press and release edges.
        if caps_before && !caps_now {
            if !core::mem::replace(&mut self.caps_used, false) {
                emit(Key::Escape);
            }
        } else if caps_now && !caps_before {
            self.caps_used = false;
        }

        let mut next = [0u8; 6];
        for (slot, &k) in next.iter_mut().zip(current.iter()) {
            *slot = k;
        }
        self.prev = next;
    }
}

/// Translate a HID keyboard usage ID to a key event using a US QWERTY layout.
/// Editing combos (Ctrl/Cmd chords) resolve to intents here and take priority
/// over character insertion; other keys with Ctrl or Cmd held are swallowed.
///
/// Every `Key::Char` this returns is ASCII — the editor depends on it (a byte
/// offset into its buffer is also a char index). The `translate_only_emits_ascii`
/// test pins this for all 256 usage IDs × modifier combinations.
fn translate(usage: u8, shift: bool, ctrl: bool, cmd: bool) -> Option<Key> {
    match usage {
        0x2a => {
            // Backspace: Cmd = delete line, Ctrl = delete word, else one char.
            return Some(if cmd {
                Key::DeleteLine
            } else if ctrl {
                Key::DeleteWord
            } else {
                Key::Backspace
            });
        }
        0x1a if ctrl => return Some(Key::DeleteWord), // Ctrl+W, readline-style
        _ => {}
    }

    // With Ctrl or Cmd held and no combo matched above, insert nothing — so
    // Caps+J or Cmd+S don't type a stray character.
    if ctrl || cmd {
        return None;
    }

    let key = match usage {
        0x04..=0x1d => {
            let base = b'a' + (usage - 0x04);
            Key::Char(if shift { base.to_ascii_uppercase() } else { base } as char)
        }
        0x1e..=0x27 => {
            const UNSHIFTED: [char; 10] = ['1', '2', '3', '4', '5', '6', '7', '8', '9', '0'];
            const SHIFTED: [char; 10] = ['!', '@', '#', '$', '%', '^', '&', '*', '(', ')'];
            let i = (usage - 0x1e) as usize;
            Key::Char(if shift { SHIFTED[i] } else { UNSHIFTED[i] })
        }
        0x28 => Key::Enter,
        0x2a => Key::Backspace,
        0x2b => Key::Char('\t'),
        0x2c => Key::Char(' '),
        0x2d => Key::Char(if shift { '_' } else { '-' }),
        0x2e => Key::Char(if shift { '+' } else { '=' }),
        0x2f => Key::Char(if shift { '{' } else { '[' }),
        0x30 => Key::Char(if shift { '}' } else { ']' }),
        0x31 => Key::Char(if shift { '|' } else { '\\' }),
        0x33 => Key::Char(if shift { ':' } else { ';' }),
        0x34 => Key::Char(if shift { '"' } else { '\'' }),
        0x35 => Key::Char(if shift { '~' } else { '`' }),
        0x36 => Key::Char(if shift { '<' } else { ',' }),
        0x37 => Key::Char(if shift { '>' } else { '.' }),
        0x38 => Key::Char(if shift { '?' } else { '/' }),
        _ => return None,
    };
    Some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build an 8-byte boot report: modifier byte, reserved 0, then up to six
    /// key slots (zero-padded).
    fn report(mods: u8, keys: &[u8]) -> Vec<u8> {
        let mut r = vec![mods, 0];
        r.extend_from_slice(keys);
        r.resize(8, 0);
        r
    }

    fn feed(dec: &mut Decoder, report: &[u8]) -> Vec<Key> {
        let mut out = Vec::new();
        dec.feed(report, |k| out.push(k));
        out
    }

    // ---- translate: the ASCII invariant the editor relies on ----

    #[test]
    fn translate_only_emits_ascii() {
        for usage in 0u8..=255 {
            for &shift in &[false, true] {
                for &ctrl in &[false, true] {
                    for &cmd in &[false, true] {
                        if let Some(Key::Char(c)) = translate(usage, shift, ctrl, cmd) {
                            assert!(
                                c.is_ascii(),
                                "usage {usage:#04x} (shift={shift} ctrl={ctrl} cmd={cmd}) \
                                 produced non-ASCII {c:?} — breaks editor byte==char indexing"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn translate_letters_and_shift() {
        assert_eq!(translate(0x04, false, false, false), Some(Key::Char('a')));
        assert_eq!(translate(0x04, true, false, false), Some(Key::Char('A')));
        assert_eq!(translate(0x1d, false, false, false), Some(Key::Char('z')));
        assert_eq!(translate(0x1d, true, false, false), Some(Key::Char('Z')));
    }

    #[test]
    fn translate_digits_and_symbols() {
        assert_eq!(translate(0x1e, false, false, false), Some(Key::Char('1')));
        assert_eq!(translate(0x1e, true, false, false), Some(Key::Char('!')));
        assert_eq!(translate(0x27, false, false, false), Some(Key::Char('0')));
        assert_eq!(translate(0x27, true, false, false), Some(Key::Char(')')));
    }

    #[test]
    fn translate_backspace_variants() {
        assert_eq!(translate(0x2a, false, false, false), Some(Key::Backspace));
        assert_eq!(translate(0x2a, false, true, false), Some(Key::DeleteWord)); // Ctrl
        assert_eq!(translate(0x2a, false, false, true), Some(Key::DeleteLine)); // Cmd
        assert_eq!(translate(0x1a, false, true, false), Some(Key::DeleteWord)); // Ctrl+W
    }

    #[test]
    fn translate_ctrl_or_cmd_swallows_plain_chars() {
        assert_eq!(translate(0x04, false, true, false), None); // Ctrl+a
        assert_eq!(translate(0x04, false, false, true), None); // Cmd+a
    }

    // ---- Decoder: edge detection ----

    #[test]
    fn key_down_emits_once_then_hold_is_silent() {
        let mut d = Decoder::new();
        assert_eq!(feed(&mut d, &report(0, &[0x04])), vec![Key::Char('a')]);
        // Same key still held → no repeat.
        assert_eq!(feed(&mut d, &report(0, &[0x04])), vec![]);
    }

    #[test]
    fn release_then_press_again_re_emits() {
        let mut d = Decoder::new();
        feed(&mut d, &report(0, &[0x04]));
        assert_eq!(feed(&mut d, &report(0, &[])), vec![]); // release
        assert_eq!(feed(&mut d, &report(0, &[0x04])), vec![Key::Char('a')]); // re-press
    }

    #[test]
    fn multiple_new_keys_in_one_report() {
        let mut d = Decoder::new();
        // 'a' (0x04) and 'b' (0x05) newly down in the same report.
        assert_eq!(
            feed(&mut d, &report(0, &[0x04, 0x05])),
            vec![Key::Char('a'), Key::Char('b')]
        );
    }

    // ---- Decoder: Caps Lock dual role ----

    #[test]
    fn caps_tap_emits_escape() {
        let mut d = Decoder::new();
        assert_eq!(feed(&mut d, &report(0, &[CAPS])), vec![]); // Caps down, nothing
        assert_eq!(feed(&mut d, &report(0, &[])), vec![Key::Escape]); // clean release
    }

    #[test]
    fn caps_held_as_ctrl_suppresses_escape() {
        let mut d = Decoder::new();
        feed(&mut d, &report(0, &[CAPS])); // Caps down
        // Caps + Backspace → Ctrl+Backspace = DeleteWord.
        assert_eq!(feed(&mut d, &report(0, &[CAPS, 0x2a])), vec![Key::DeleteWord]);
        // Releasing Caps must NOT emit Escape (it was used as Ctrl).
        assert_eq!(feed(&mut d, &report(0, &[])), vec![]);
    }

    #[test]
    fn modifier_ctrl_and_cmd_backspace() {
        let mut d = Decoder::new();
        assert_eq!(feed(&mut d, &report(0x01, &[0x2a])), vec![Key::DeleteWord]); // LCtrl
        feed(&mut d, &report(0, &[])); // release
        assert_eq!(feed(&mut d, &report(0x08, &[0x2a])), vec![Key::DeleteLine]); // LGUI
    }

    // ---- Decoder: robustness on malformed / untrusted input ----

    #[test]
    fn short_report_is_ignored() {
        let mut d = Decoder::new();
        assert_eq!(feed(&mut d, &[]), vec![]);
        assert_eq!(feed(&mut d, &[0x00]), vec![]);
        assert_eq!(feed(&mut d, &[0x00, 0x00]), vec![]);
    }

    #[test]
    fn never_panics_on_arbitrary_input() {
        // The FFI layer clamps reports to 8 bytes, but the decoder must not
        // panic on anything — feed it every length 0..=16, every fill byte, a
        // full sweep of single-key usages, and a deterministic pseudo-random
        // stream. A panic here fails the test.
        let mut d = Decoder::new();

        for len in 0..=16usize {
            for fill in 0u8..=255 {
                let buf = vec![fill; len];
                d.feed(&buf, |_| {});
            }
        }

        // Every usage ID as the sole key in a well-formed report.
        for usage in 0u8..=255 {
            d.feed(&report(0xff, &[usage]), |_| {});
        }

        // Deterministic LCG so the stream is reproducible without a rand dep.
        let mut state = 0x1234_5678u32;
        for _ in 0..10_000 {
            state = state.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            let len = (state >> 28) as usize; // 0..=15
            let buf: Vec<u8> = (0..len)
                .map(|i| (state.rotate_left(i as u32 * 3) & 0xff) as u8)
                .collect();
            d.feed(&buf, |_| {});
        }
    }

    #[test]
    fn reset_clears_held_state() {
        let mut d = Decoder::new();
        feed(&mut d, &report(0, &[0x04])); // 'a' held
        d.reset();
        // After reset the same key reads as a fresh down, not a held slot.
        assert_eq!(feed(&mut d, &report(0, &[0x04])), vec![Key::Char('a')]);
    }
}
