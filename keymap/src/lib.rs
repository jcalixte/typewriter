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
//! *ASCII* `Key::Char`, whose byte==char guarantee the editor's indexing relies
//! on. [`Composer`] adds US-International dead-key accent folding downstream; it
//! is the one deliberate source of *non-ASCII* (Latin-9) `Key::Char`, and so
//! must not reach the editor until the buffer is UTF-8-correct (see its docs).
//! All three invariants are pinned by the tests below. See MEMORY_AUDIT.md.

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
    /// Ctrl+D — scroll down half a screen (vim `Ctrl-d`).
    HalfPageDown,
    /// Ctrl+U — scroll up half a screen (vim `Ctrl-u`).
    HalfPageUp,
    /// Ctrl+R — redo (vim `Ctrl-r`); the inverse of `u`. Meaningful in Normal;
    /// ignored elsewhere.
    Redo,
    /// Cmd+P — open the file palette (fuzzy open, v0.5), VS Code "Go to File"
    /// style. Available from **every** mode (Insert/Visual/View/Command included,
    /// each first bailing out as Esc would); **inside** the palette the same
    /// chord closes it (toggle). Esc also closes.
    Palette,
    /// Ctrl+N — move down: one line in Normal/View (vim `CTRL-N` ≡ `j`), or one
    /// row in the file palette. Ignored in Insert.
    Down,
    /// Ctrl+P — move up: one line in Normal/View (vim `CTRL-P` ≡ `k`), or one row
    /// in the file palette. Ignored in Insert.
    Up,
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
        0x07 if ctrl => return Some(Key::HalfPageDown), // Ctrl+D, half-page down
        0x18 if ctrl => return Some(Key::HalfPageUp), // Ctrl+U, half-page up
        0x15 if ctrl => return Some(Key::Redo),       // Ctrl+R, redo
        0x13 if ctrl => return Some(Key::Up),   // Ctrl+P, move up (vim CTRL-P)
        0x13 if cmd => return Some(Key::Palette), // Cmd+P, file palette
        0x11 if ctrl => return Some(Key::Down), // Ctrl+N, move down (vim CTRL-N)
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
        // The physical Esc key (0x29) is repurposed to type backtick / tilde:
        // Escape comes from a Caps tap instead, which frees this key to reach
        // `~ — and their grave/tilde dead-key accents, and Markdown code fences —
        // without a Fn layer on 60% boards.
        0x29 => Key::Char(if shift { '~' } else { '`' }),
        0x36 => Key::Char(if shift { '<' } else { ',' }),
        0x37 => Key::Char(if shift { '>' } else { '.' }),
        0x38 => Key::Char(if shift { '?' } else { '/' }),
        _ => return None,
    };
    Some(key)
}

/// The five US-International dead keys, as the characters the QWERTY decoder
/// produces for them: acute `'`, grave `` ` ``, circumflex `^`, diaeresis `"`,
/// tilde `~`. Typing one arms the [`Composer`]; the next key resolves it.
const DEAD_KEYS: [char; 5] = ['\'', '`', '^', '"', '~'];

fn is_dead(c: char) -> bool {
    DEAD_KEYS.contains(&c)
}

/// Fold a dead key and the following base letter into a single accented glyph,
/// for the ISO-8859-15 (Latin-9) repertoire the render font carries. Returns
/// `None` when the pair doesn't compose (e.g. `'`+`z`), so the caller can fall
/// back to emitting the accent then the letter.
fn compose(dead: char, base: char) -> Option<char> {
    Some(match (dead, base) {
        // Acute — plus ç, the roadmap's `'`+c special case.
        ('\'', 'a') => 'á', ('\'', 'e') => 'é', ('\'', 'i') => 'í',
        ('\'', 'o') => 'ó', ('\'', 'u') => 'ú', ('\'', 'y') => 'ý',
        ('\'', 'c') => 'ç',
        ('\'', 'A') => 'Á', ('\'', 'E') => 'É', ('\'', 'I') => 'Í',
        ('\'', 'O') => 'Ó', ('\'', 'U') => 'Ú', ('\'', 'Y') => 'Ý',
        ('\'', 'C') => 'Ç',
        // Grave
        ('`', 'a') => 'à', ('`', 'e') => 'è', ('`', 'i') => 'ì',
        ('`', 'o') => 'ò', ('`', 'u') => 'ù',
        ('`', 'A') => 'À', ('`', 'E') => 'È', ('`', 'I') => 'Ì',
        ('`', 'O') => 'Ò', ('`', 'U') => 'Ù',
        // Circumflex
        ('^', 'a') => 'â', ('^', 'e') => 'ê', ('^', 'i') => 'î',
        ('^', 'o') => 'ô', ('^', 'u') => 'û',
        ('^', 'A') => 'Â', ('^', 'E') => 'Ê', ('^', 'I') => 'Î',
        ('^', 'O') => 'Ô', ('^', 'U') => 'Û',
        // Diaeresis
        ('"', 'a') => 'ä', ('"', 'e') => 'ë', ('"', 'i') => 'ï',
        ('"', 'o') => 'ö', ('"', 'u') => 'ü', ('"', 'y') => 'ÿ',
        ('"', 'A') => 'Ä', ('"', 'E') => 'Ë', ('"', 'I') => 'Ï',
        ('"', 'O') => 'Ö', ('"', 'U') => 'Ü', ('"', 'Y') => 'Ÿ',
        // Tilde
        ('~', 'a') => 'ã', ('~', 'n') => 'ñ', ('~', 'o') => 'õ',
        ('~', 'A') => 'Ã', ('~', 'N') => 'Ñ', ('~', 'O') => 'Õ',
        _ => return None,
    })
}

/// US-International dead-key composer: folds a dead key plus the following letter
/// into one accented [`Key::Char`], so the editor still sees a single character.
/// Sits downstream of [`Decoder`] in the key stream — the decoder does HID
/// edge-detection + US-QWERTY translation, this does accent composition.
///
/// **Latin-9, not ASCII.** Unlike [`translate`], this is deliberately a source
/// of non-ASCII `Key::Char` (à, é, ç … the ISO-8859-15 set the render font
/// carries). Its output must therefore NOT be fed to the editor until the editor
/// buffer is UTF-8-correct — byte offsets stepped per character, not per byte
/// (the v0.2 groundwork item). Wiring it into `usb_kbd`'s decode path before
/// then would let a caret motion land mid-char and panic on the next edit, which
/// is why `Decoder` does not route through it yet.
#[derive(Debug, Clone, Default)]
pub struct Composer {
    /// The armed dead key awaiting its base letter, if any.
    pending: Option<char>,
}

impl Composer {
    pub const fn new() -> Self {
        Self { pending: None }
    }

    /// The currently-armed dead key, for the side-panel pending-accent indicator
    /// (roadmap v0.2.5). `None` when nothing is pending.
    pub fn pending(&self) -> Option<char> {
        self.pending
    }

    /// Drop any pending accent (call on keyboard detach or a mode reset, so a
    /// stale dead key can't swallow the next unrelated letter).
    pub fn reset(&mut self) {
        self.pending = None;
    }

    /// Feed one decoded key; emit zero, one, or two resolved keys.
    ///
    /// - A dead key (`'` `` ` `` `^` `"` `~`) arms and emits nothing yet.
    /// - Armed + a composing letter → the single accented char.
    /// - Armed + space → the literal dead-key char (the everyday apostrophe
    ///   path: `'` then space is a plain `'`); the space is consumed.
    /// - Armed + a non-composing char → the accent as a literal, then the char
    ///   processed fresh (so it may itself arm the next dead key).
    /// - Armed + a non-character event (Enter, Backspace, arrows, …) → flush the
    ///   accent as a literal first, then the event.
    pub fn feed(&mut self, key: Key, mut emit: impl FnMut(Key)) {
        let Some(dead) = self.pending.take() else {
            self.arm_or_emit(key, &mut emit);
            return;
        };
        match key {
            Key::Char(' ') => emit(Key::Char(dead)),
            Key::Char(c) => match compose(dead, c) {
                Some(accented) => emit(Key::Char(accented)),
                None => {
                    emit(Key::Char(dead));
                    self.arm_or_emit(key, &mut emit);
                }
            },
            other => {
                emit(Key::Char(dead));
                emit(other);
            }
        }
    }

    /// If `key` is a dead-key character, arm it (emitting nothing); otherwise
    /// pass it straight through.
    fn arm_or_emit(&mut self, key: Key, emit: &mut impl FnMut(Key)) {
        if let Key::Char(c) = key {
            if is_dead(c) {
                self.pending = Some(c);
                return;
            }
        }
        emit(key);
    }
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
    fn esc_key_is_repurposed_to_backtick_and_tilde() {
        // 0x29 (the physical Esc key) types `/~ now; Escape comes from a Caps tap.
        assert_eq!(translate(0x29, false, false, false), Some(Key::Char('`')));
        assert_eq!(translate(0x29, true, false, false), Some(Key::Char('~')));
    }

    #[test]
    fn translate_ctrl_navigation_and_redo_chords() {
        assert_eq!(translate(0x07, false, true, false), Some(Key::HalfPageDown)); // Ctrl+D
        assert_eq!(translate(0x18, false, true, false), Some(Key::HalfPageUp)); // Ctrl+U
        assert_eq!(translate(0x15, false, true, false), Some(Key::Redo)); // Ctrl+R
        assert_eq!(translate(0x13, false, true, false), Some(Key::Up)); // Ctrl+P, up
        assert_eq!(translate(0x13, false, false, true), Some(Key::Palette)); // Cmd+P, palette
        assert_eq!(translate(0x11, false, true, false), Some(Key::Down)); // Ctrl+N, down
        assert_eq!(translate(0x11, false, false, true), None); // Cmd+N reserved (:enew, v0.5)
        // Without a modifier these are ordinary letters, not intents.
        assert_eq!(translate(0x15, false, false, false), Some(Key::Char('r')));
        assert_eq!(translate(0x13, false, false, false), Some(Key::Char('p')));
        assert_eq!(translate(0x11, false, false, false), Some(Key::Char('n')));
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

    #[test]
    fn physical_esc_key_decodes_to_backtick_not_escape() {
        // End to end: a report with usage 0x29 yields a backtick, not Escape.
        let mut d = Decoder::new();
        assert_eq!(feed(&mut d, &report(0, &[0x29])), vec![Key::Char('`')]);
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

    // ---- Composer: US-International dead-key accent folding ----

    fn ch(c: char) -> Key {
        Key::Char(c)
    }

    /// Feed a sequence of keys through a fresh composer and collect the output.
    fn compose_keys(seq: &[Key]) -> Vec<Key> {
        let mut c = Composer::new();
        let mut out = Vec::new();
        for &k in seq {
            c.feed(k, |k| out.push(k));
        }
        out
    }

    #[test]
    fn dead_key_composes_accented_letter() {
        // The roadmap's worked examples: à é ê ë ñ, and ç via `'`+c.
        assert_eq!(compose_keys(&[ch('`'), ch('a')]), vec![ch('à')]);
        assert_eq!(compose_keys(&[ch('\''), ch('e')]), vec![ch('é')]);
        assert_eq!(compose_keys(&[ch('^'), ch('e')]), vec![ch('ê')]);
        assert_eq!(compose_keys(&[ch('"'), ch('e')]), vec![ch('ë')]);
        assert_eq!(compose_keys(&[ch('~'), ch('n')]), vec![ch('ñ')]);
        assert_eq!(compose_keys(&[ch('\''), ch('c')]), vec![ch('ç')]);
    }

    #[test]
    fn dead_key_composes_uppercase() {
        assert_eq!(compose_keys(&[ch('\''), ch('E')]), vec![ch('É')]);
        assert_eq!(compose_keys(&[ch('~'), ch('N')]), vec![ch('Ñ')]);
        assert_eq!(compose_keys(&[ch('"'), ch('Y')]), vec![ch('Ÿ')]);
        assert_eq!(compose_keys(&[ch('\''), ch('C')]), vec![ch('Ç')]);
    }

    #[test]
    fn dead_key_plus_space_is_literal_diacritic() {
        // The everyday apostrophe path: `'` then space → a single `'`, space
        // consumed. Same for every dead key.
        assert_eq!(compose_keys(&[ch('\''), ch(' ')]), vec![ch('\'')]);
        assert_eq!(compose_keys(&[ch('^'), ch(' ')]), vec![ch('^')]);
        assert_eq!(compose_keys(&[ch('"'), ch(' ')]), vec![ch('"')]);
        assert_eq!(compose_keys(&[ch('`'), ch(' ')]), vec![ch('`')]);
        assert_eq!(compose_keys(&[ch('~'), ch(' ')]), vec![ch('~')]);
    }

    #[test]
    fn dead_key_plus_noncomposing_emits_accent_then_letter() {
        assert_eq!(compose_keys(&[ch('\''), ch('z')]), vec![ch('\''), ch('z')]);
        // Grave doesn't compose with 'c' (only acute does, → ç).
        assert_eq!(compose_keys(&[ch('`'), ch('c')]), vec![ch('`'), ch('c')]);
    }

    #[test]
    fn noncharacter_event_flushes_pending_accent_first() {
        assert_eq!(compose_keys(&[ch('\''), Key::Enter]), vec![ch('\''), Key::Enter]);
        assert_eq!(
            compose_keys(&[ch('^'), Key::Backspace]),
            vec![ch('^'), Key::Backspace]
        );
        assert_eq!(compose_keys(&[ch('~'), Key::Escape]), vec![ch('~'), Key::Escape]);
        assert_eq!(
            compose_keys(&[ch('"'), Key::DeleteWord]),
            vec![ch('"'), Key::DeleteWord]
        );
    }

    #[test]
    fn dead_key_twice_emits_one_then_rearms() {
        let mut c = Composer::new();
        let mut out = Vec::new();
        c.feed(ch('\''), |k| out.push(k)); // arm
        assert_eq!(out, vec![]);
        assert_eq!(c.pending(), Some('\''));
        c.feed(ch('\''), |k| out.push(k)); // second acute: flush one, re-arm
        assert_eq!(out, vec![ch('\'')]);
        assert_eq!(c.pending(), Some('\''));
        c.feed(ch('e'), |k| out.push(k)); // now composes with the re-armed acute
        assert_eq!(out, vec![ch('\''), ch('é')]);
        assert_eq!(c.pending(), None);
    }

    #[test]
    fn pending_reflects_armed_dead_key() {
        let mut c = Composer::new();
        assert_eq!(c.pending(), None);
        c.feed(ch('~'), |_| {});
        assert_eq!(c.pending(), Some('~')); // side-panel indicator would show '~'
        c.feed(ch('o'), |_| {}); // resolves
        assert_eq!(c.pending(), None);
    }

    #[test]
    fn reset_drops_pending() {
        let mut c = Composer::new();
        c.feed(ch('`'), |_| {});
        assert_eq!(c.pending(), Some('`'));
        c.reset();
        assert_eq!(c.pending(), None);
        // Next base letter is not swallowed by the dropped accent.
        let mut out = Vec::new();
        c.feed(ch('a'), |k| out.push(k));
        assert_eq!(out, vec![ch('a')]);
    }

    #[test]
    fn plain_ascii_passes_through_unchanged() {
        let seq: Vec<Key> = "hello world".chars().map(ch).collect();
        assert_eq!(compose_keys(&seq), seq);
    }

    #[test]
    fn composes_within_a_word() {
        // Keystrokes n a " i v e  →  "naïve" (the diaeresis folds into ï).
        let seq = [ch('n'), ch('a'), ch('"'), ch('i'), ch('v'), ch('e')];
        let out: String = compose_keys(&seq)
            .into_iter()
            .map(|k| match k {
                Key::Char(c) => c,
                _ => '?',
            })
            .collect();
        assert_eq!(out, "naïve");
    }

    #[test]
    fn every_composed_char_is_non_ascii() {
        // The Composer is the deliberate non-ASCII (Latin-9) source; translate
        // stays ASCII. If a mapping ever produced an ASCII char it would slip
        // past the editor's UTF-8 gate unnoticed — pin it here.
        for &dead in &DEAD_KEYS {
            for base in [
                'a', 'e', 'i', 'o', 'u', 'y', 'c', 'n', 'A', 'E', 'I', 'O', 'U', 'Y', 'C', 'N',
            ] {
                if let Some(accented) = compose(dead, base) {
                    assert!(
                        !accented.is_ascii(),
                        "compose({dead:?}, {base:?}) = {accented:?} must be non-ASCII"
                    );
                }
            }
        }
    }
}
