//! On-device onboarding wizard — the step/field state machine.
//!
//! Spec: docs/v0.9-onboarding-wizard.md. The wizard runs *instead of* the
//! editor when the card is unconfigured (no usable `typoena.conf` **or** no
//! `/sd/repo` — the second half makes a power-pull between "conf written" and
//! "clone finished" resume here rather than hit the no-repo boot halt).
//!
//! Same architecture as the editor crate: this is pure logic, host-testable.
//! Keys (`keymap::Key`) come in via [`Wizard::key`], I/O requests come out as
//! [`Effect`]s that the firmware driver executes (join Wi-Fi, run the GitHub
//! device flow, list repos, clone), and the driver feeds results back via
//! [`Wizard::event`]. Rendering is [`Wizard::draw_into`] onto a
//! `display::Frame`, the `show_message` pattern — no editor loop involved.
//!
//! Steps: Wi-Fi → Sign in (device-flow QR) → Repo pick → Clone → Done. Every
//! completed step hands the driver a [`Effect::WriteConf`] so progress
//! persists atomically (unlink + tmp + rename) and a power-pull resumes at
//! the right step.

use display::Frame;
use embedded_graphics::{
    mono_font::{ascii::FONT_10X20, MonoTextStyle},
    pixelcolor::BinaryColor,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use keymap::Key;

pub mod github;

/// Repos larger than this are refused on-device: libgit2 has no partial
/// clone, so tip media would be downloaded whole (see the spec's size gate).
pub const SIZE_GATE_KB: u64 = 30 * 1024;

/// One selectable repo from the API list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoChoice {
    /// `owner/name`.
    pub full_name: String,
    /// GitHub's `size` field — kilobytes.
    pub size_kb: u64,
}

/// What the wizard asks the firmware driver to do. Returned by [`Wizard::key`]
/// / [`Wizard::event`]; the driver executes and answers with an [`Event`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    /// Scan for nearby networks so the SSID can be picked, not typed.
    ScanWifi,
    /// Join this network (bounded retry) and report back.
    TestWifi { ssid: String, pass: String },
    /// Start the GitHub device flow (POST login/device/code), then poll for
    /// the token at the server's interval until `AuthDone`/`AuthFailed`.
    StartAuth,
    /// GET the repos the app installation can reach.
    FetchRepos,
    /// Shallow-clone `full_name` to /sd/repo (init + fetch depth 1 +
    /// apply_tree_diff empty→tip), seeding defaults after.
    Clone { full_name: String },
    /// Persist this conf atomically (unlink + tmp + rename).
    WriteConf(conf::Conf),
    /// Wizard finished — fall through to the normal boot path.
    Finish,
}

/// What the firmware driver reports back into the wizard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// Scan results: SSIDs, deduped and strongest-first (driver's job).
    WifiScan(Vec<String>),
    WifiScanFailed(String),
    WifiOk,
    WifiFailed(String),
    /// Device flow started: show the QR (verification URI) + user code.
    AuthCode {
        verification_uri: String,
        user_code: String,
    },
    /// Token granted. `login`/`name`/`email` come from GET /user (name/email
    /// may be blank — fall back to the login).
    AuthDone {
        token: String,
        login: String,
        name: String,
        email: String,
    },
    AuthFailed(String),
    Repos(Vec<RepoChoice>),
    ReposFailed(String),
    /// A progress line for the clone screen ("downloading…", "12/340 files").
    CloneProgress(String),
    CloneDone,
    CloneFailed(String),
}

/// The wizard's current screen.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Screen {
    /// Waiting on `ScanWifi` (the first screen on a blank card).
    WifiScanning,
    /// Picking an SSID from the scan. Typing filters; Enter selects (or
    /// rescans when nothing matches); Esc drops to manual entry for a hidden
    /// or missed network, seeded with whatever was typed.
    WifiPick {
        networks: Vec<String>,
        filter: String,
        sel: usize,
    },
    /// Editing one Wi-Fi field (0 = SSID, 1 = password). Reached for the
    /// password after a pick, or for a manual SSID (field 0) via `WifiPick`.
    WifiEdit { field: usize },
    /// Waiting on `TestWifi`.
    WifiTesting,
    /// Waiting on `StartAuth`'s `AuthCode`.
    AuthStarting,
    /// Showing the QR + code, driver is polling for the token.
    AuthCodeShown {
        verification_uri: String,
        user_code: String,
    },
    /// Sign-in failed (transport, denied, expired). Waits for the user —
    /// auto-retrying here would spin the radio and the panel forever on a
    /// dead network. Enter retries; Backspace goes back to Wi-Fi.
    AuthRetry,
    /// Waiting on `FetchRepos`.
    RepoLoading,
    /// Repo listing failed; Enter retries the fetch.
    RepoRetry,
    /// Picking from the (filtered) list. `refused` holds a size-gate message
    /// for the last attempted pick.
    RepoPick {
        repos: Vec<RepoChoice>,
        filter: String,
        sel: usize,
        refused: Option<String>,
    },
    /// Waiting on the clone; `progress` is the latest driver line.
    Cloning { progress: String },
    /// Terminal screen; any key finishes.
    AllSet,
}

/// A transient error shown on the current screen (join failed, auth failed…).
/// Cleared on the next keystroke.
type Notice = Option<String>;

pub struct Wizard {
    /// The conf being built. Prefilled in `:setup` mode / on resume.
    conf: conf::Conf,
    screen: Screen,
    notice: Notice,
    /// Whether the Wi-Fi password is shown in cleartext (Tab toggles). Defaults
    /// to shown: entering a long random key blind under `*` is the real setup
    /// pain, the device is held still during a one-time setup, and its secrets
    /// live in cleartext on the card anyway (physical custody is the control).
    show_pass: bool,
}

impl Wizard {
    /// First boot on a blank card — everything empty, start at Wi-Fi.
    pub fn first_boot() -> Wizard {
        Wizard::resume(conf::Conf::default())
    }

    /// The conf as built so far — the driver reads this on `Effect::Finish`
    /// to hand the completed config to the normal boot path.
    pub fn conf(&self) -> &conf::Conf {
        &self.conf
    }

    /// Start from an existing (possibly partial) conf: the resume-after-
    /// power-pull entry, and the base of the future `:setup` re-entry. Skips
    /// to the first step the conf doesn't already satisfy: blank SSID → Wi-Fi,
    /// blank token → sign-in, else → repo pick (a conf whose repo cloned fine
    /// never enters the wizard; reaching here with a full conf means the
    /// clone is missing).
    pub fn resume(c: conf::Conf) -> Wizard {
        let screen = if c.wifi_ssid.trim().is_empty() {
            Screen::WifiScanning
        } else if c.token.trim().is_empty() {
            Screen::AuthStarting
        } else {
            Screen::RepoLoading
        };
        Wizard {
            conf: c,
            screen,
            notice: None,
            show_pass: true,
        }
    }

    /// The effect the wizard needs executed *right now* to leave its current
    /// waiting screen. The driver calls this once after construction (resume
    /// may land on a waiting screen) and after every `key`/`event` batch.
    pub fn pending(&self) -> Option<Effect> {
        match &self.screen {
            Screen::WifiScanning => Some(Effect::ScanWifi),
            Screen::WifiTesting => Some(Effect::TestWifi {
                ssid: self.conf.wifi_ssid.clone(),
                pass: self.conf.wifi_pass.clone(),
            }),
            Screen::AuthStarting => Some(Effect::StartAuth),
            Screen::RepoLoading => Some(Effect::FetchRepos),
            _ => None,
        }
    }

    /// Feed one key. Returns the effects for the driver (0, 1, or 2 — a step
    /// completion is `WriteConf` + the next step's request).
    pub fn key(&mut self, k: Key) -> Vec<Effect> {
        self.notice = None;
        // Tab toggles password visibility while editing Wi-Fi. Handled here,
        // ahead of the `&mut self.screen` match, so it can touch `show_pass`
        // without fighting that borrow. Tab was already dropped as a control
        // char on every other screen, so this is a no-op there.
        if k == Key::Char('\t') {
            if matches!(self.screen, Screen::WifiEdit { .. }) {
                self.show_pass = !self.show_pass;
            }
            return vec![];
        }
        match &mut self.screen {
            Screen::WifiEdit { field } => {
                let f = if *field == 0 {
                    conf::Field::WifiSsid
                } else {
                    conf::Field::WifiPass
                };
                match k {
                    Key::Char(c) if !c.is_control() => {
                        self.conf.get_mut(f).push(c);
                    }
                    Key::Backspace => {
                        let v = self.conf.get_mut(f);
                        if v.pop().is_none() {
                            if *field == 1 {
                                // Backspace past an empty password → back to SSID.
                                self.screen = Screen::WifiEdit { field: 0 };
                            } else {
                                // Backspace past an empty manual SSID → the list.
                                self.screen = Screen::WifiScanning;
                                return self.pending().into_iter().collect();
                            }
                        }
                    }
                    Key::DeleteWord => delete_word(self.conf.get_mut(f)),
                    Key::DeleteLine => self.conf.get_mut(f).clear(),
                    Key::Enter => {
                        if *field == 0 {
                            if !self.conf.wifi_ssid.trim().is_empty() {
                                self.screen = Screen::WifiEdit { field: 1 };
                            }
                        } else {
                            // Empty password = open network, allowed.
                            self.screen = Screen::WifiTesting;
                            return self.pending().into_iter().collect();
                        }
                    }
                    _ => {}
                }
                vec![]
            }
            Screen::WifiPick {
                networks,
                filter,
                sel,
            } => {
                match k {
                    Key::Char(c) if !c.is_control() => {
                        filter.push(c);
                        *sel = 0;
                    }
                    Key::Backspace => {
                        filter.pop();
                        *sel = 0;
                    }
                    Key::DeleteWord | Key::DeleteLine => {
                        filter.clear();
                        *sel = 0;
                    }
                    Key::Down => {
                        let n = filtered_ssids(networks, filter).len();
                        if n > 0 {
                            *sel = (*sel + 1).min(n - 1);
                        }
                    }
                    Key::Up => *sel = sel.saturating_sub(1),
                    Key::Enter => {
                        let shown = filtered_ssids(networks, filter);
                        if let Some(ssid) = shown.get(*sel) {
                            self.conf.wifi_ssid = (*ssid).clone();
                            self.conf.wifi_pass.clear();
                            self.screen = Screen::WifiEdit { field: 1 };
                        } else {
                            // Empty list / filter matches nothing → rescan.
                            self.screen = Screen::WifiScanning;
                            return self.pending().into_iter().collect();
                        }
                    }
                    Key::Escape => {
                        // Type a hidden or missed network by hand, seeded with
                        // whatever was typed into the filter.
                        self.conf.wifi_ssid = filter.clone();
                        self.conf.wifi_pass.clear();
                        self.screen = Screen::WifiEdit { field: 0 };
                    }
                    _ => {}
                }
                vec![]
            }
            // Waiting screens ignore keys (the driver owns the outcome)…
            Screen::WifiScanning
            | Screen::WifiTesting
            | Screen::AuthStarting
            | Screen::RepoLoading => vec![],
            Screen::AuthRetry => match k {
                Key::Enter => {
                    self.screen = Screen::AuthStarting;
                    self.pending().into_iter().collect()
                }
                Key::Backspace => {
                    self.screen = Screen::WifiEdit { field: 0 };
                    vec![]
                }
                _ => vec![],
            },
            Screen::RepoRetry => match k {
                Key::Enter => {
                    self.screen = Screen::RepoLoading;
                    self.pending().into_iter().collect()
                }
                _ => vec![],
            },
            // …except the QR screen: Escape restarts the flow (a new code) if
            // the phone step went sideways.
            Screen::AuthCodeShown { .. } => {
                if k == Key::Escape {
                    self.screen = Screen::AuthStarting;
                    self.pending().into_iter().collect()
                } else {
                    vec![]
                }
            }
            Screen::RepoPick {
                repos,
                filter,
                sel,
                refused,
            } => {
                match k {
                    Key::Char(c) if !c.is_control() => {
                        filter.push(c);
                        *sel = 0;
                        *refused = None;
                    }
                    Key::Backspace => {
                        filter.pop();
                        *sel = 0;
                        *refused = None;
                    }
                    Key::DeleteWord | Key::DeleteLine => {
                        filter.clear();
                        *sel = 0;
                        *refused = None;
                    }
                    Key::Down => {
                        let n = filtered(repos, filter).len();
                        if n > 0 {
                            *sel = (*sel + 1).min(n - 1);
                        }
                    }
                    Key::Up => *sel = sel.saturating_sub(1),
                    Key::Enter => {
                        let shown = filtered(repos, filter);
                        if let Some(r) = shown.get(*sel) {
                            if r.size_kb > SIZE_GATE_KB {
                                *refused = Some(format!(
                                    "{} is {} MB - too large to set up from the device. \
                                     Pick or create a smaller repo, or seed the card \
                                     from a computer once (typoena.dev).",
                                    r.full_name,
                                    r.size_kb / 1024
                                ));
                            } else {
                                let full_name = r.full_name.clone();
                                self.conf.remote_url =
                                    conf::expand_remote_url(&format!("github.com/{full_name}"));
                                self.screen = Screen::Cloning {
                                    progress: String::from("starting clone"),
                                };
                                return vec![
                                    Effect::WriteConf(self.conf.clone()),
                                    Effect::Clone { full_name },
                                ];
                            }
                        }
                    }
                    _ => {}
                }
                vec![]
            }
            Screen::Cloning { .. } => vec![],
            Screen::AllSet => vec![Effect::Finish],
        }
    }

    /// Feed one driver event. Returns follow-up effects like `key`.
    pub fn event(&mut self, e: Event) -> Vec<Effect> {
        match e {
            Event::WifiScan(networks) => {
                self.screen = Screen::WifiPick {
                    networks,
                    filter: String::new(),
                    sel: 0,
                };
                vec![]
            }
            Event::WifiScanFailed(reason) => {
                // Show the picker empty with the reason: Enter rescans, Esc
                // types manually — both escape a flaky or dead scan.
                self.notice = Some(format!("scan failed: {reason}"));
                self.screen = Screen::WifiPick {
                    networks: Vec::new(),
                    filter: String::new(),
                    sel: 0,
                };
                vec![]
            }
            Event::WifiOk => {
                // Wi-Fi verified — persist, then sign in (or skip straight to
                // repos when a resume already carries a token).
                self.screen = if self.conf.token.trim().is_empty() {
                    Screen::AuthStarting
                } else {
                    Screen::RepoLoading
                };
                std::iter::once(Effect::WriteConf(self.conf.clone()))
                    .chain(self.pending())
                    .collect()
            }
            Event::WifiFailed(reason) => {
                self.notice = Some(format!("could not join: {reason}"));
                self.screen = Screen::WifiEdit { field: 1 };
                vec![]
            }
            Event::AuthCode {
                verification_uri,
                user_code,
            } => {
                self.screen = Screen::AuthCodeShown {
                    verification_uri,
                    user_code,
                };
                vec![]
            }
            Event::AuthDone {
                token,
                login,
                name,
                email,
            } => {
                self.conf.token = token;
                self.conf.gh_user = login.clone();
                self.conf.author_name = if name.trim().is_empty() {
                    login.clone()
                } else {
                    name
                };
                self.conf.author_email = if email.trim().is_empty() {
                    format!("{login}@users.noreply.github.com")
                } else {
                    email
                };
                self.screen = Screen::RepoLoading;
                std::iter::once(Effect::WriteConf(self.conf.clone()))
                    .chain(self.pending())
                    .collect()
            }
            Event::AuthFailed(reason) => {
                self.notice = Some(format!("sign-in failed: {reason}"));
                self.screen = Screen::AuthRetry;
                vec![]
            }
            Event::Repos(repos) => {
                self.screen = Screen::RepoPick {
                    repos,
                    filter: String::new(),
                    sel: 0,
                    refused: None,
                };
                vec![]
            }
            Event::ReposFailed(reason) => {
                self.notice = Some(format!("listing repos failed: {reason}"));
                self.screen = Screen::RepoRetry;
                vec![]
            }
            Event::CloneProgress(line) => {
                if let Screen::Cloning { progress } = &mut self.screen {
                    *progress = line;
                }
                vec![]
            }
            Event::CloneDone => {
                self.screen = Screen::AllSet;
                vec![Effect::WriteConf(self.conf.clone())]
            }
            Event::CloneFailed(reason) => {
                // Back to the pick list so a different (e.g. smaller) repo can
                // be chosen; the failed half-clone is the driver's to clean.
                self.notice = Some(format!("clone failed: {reason}"));
                self.screen = Screen::RepoLoading;
                self.pending().into_iter().collect()
            }
        }
    }

    /// Render the current screen. Full-frame, FONT_10X20, the `show_message`
    /// posture (title + body + a bottom hint line).
    pub fn draw_into(&self, f: &mut Frame) {
        f.clear_white();
        let ink = MonoTextStyle::new(&FONT_10X20, BinaryColor::On);
        let inv = MonoTextStyle::new(&FONT_10X20, BinaryColor::Off);
        let w = display::WIDTH as i32;
        let h = display::HEIGHT as i32;
        let line = |f: &mut Frame, row: i32, s: &str, style| {
            let _ = Text::with_baseline(s, Point::new(10, 8 + row * 24), style, Baseline::Top)
                .draw(f);
        };
        let hint = |f: &mut Frame, s: &str| {
            let _ = Text::with_baseline(s, Point::new(10, h - 24), ink, Baseline::Top).draw(f);
        };

        match &self.screen {
            Screen::WifiEdit { field } => {
                line(f, 0, "Welcome to Typoena - Wi-Fi", ink);
                let pw = if self.show_pass {
                    self.conf.wifi_pass.clone()
                } else {
                    "*".repeat(self.conf.wifi_pass.chars().count())
                };
                let (a, b) = (
                    format!("  Network:  {}", self.conf.wifi_ssid),
                    format!("  Password: {pw}"),
                );
                line(f, 2, &a, ink);
                line(f, 3, &b, ink);
                // Caret on the active field.
                let (row, len) = if *field == 0 {
                    (2, self.conf.wifi_ssid.chars().count())
                } else {
                    (3, self.conf.wifi_pass.chars().count())
                };
                let x = 10 + (12 + len as i32) * 10;
                let _ = Rectangle::new(
                    Point::new(x, 8 + row * 24),
                    Size::new(10, 20),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(f);
                let hint_text = if *field == 0 {
                    "type SSID - Enter next - Backspace: back to list".to_string()
                } else {
                    let tab = if self.show_pass { "Tab hides pw" } else { "Tab shows pw" };
                    format!("type - Enter joins - {tab} - empty = open network")
                };
                hint(f, &hint_text);
            }
            Screen::WifiScanning => {
                line(f, 0, "Welcome to Typoena - Wi-Fi", ink);
                line(f, 2, "  scanning for networks...", ink);
            }
            Screen::WifiPick {
                networks,
                filter,
                sel,
            } => {
                line(f, 0, &format!("Choose your Wi-Fi  ({})", networks.len()), ink);
                line(f, 1, &format!("  filter: {filter}"), ink);
                let shown = filtered_ssids(networks, filter);
                if shown.is_empty() {
                    line(f, 3, "  no networks found - Enter to rescan", ink);
                } else {
                    // Rows 3..9 — a 6-row window scrolled to keep sel visible.
                    let win = 6usize;
                    let top = sel.saturating_sub(win - 1);
                    for (i, ssid) in shown.iter().enumerate().skip(top).take(win) {
                        let row = 3 + (i - top) as i32;
                        let text = format!("  {ssid}");
                        if i == *sel {
                            let _ = Rectangle::new(
                                Point::new(0, 8 + row * 24),
                                Size::new(w as u32, 20),
                            )
                            .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                            .draw(f);
                            line(f, row, &text, inv);
                        } else {
                            line(f, row, &text, ink);
                        }
                    }
                }
                hint(f, "type filters - Ctrl-N/P move - Enter selects - Esc types it");
            }
            Screen::WifiTesting => {
                line(f, 0, "Joining Wi-Fi...", ink);
                line(f, 2, &format!("  {}", self.conf.wifi_ssid), ink);
            }
            Screen::AuthStarting => {
                line(f, 0, "Sign in with GitHub", ink);
                line(f, 2, "  contacting github.com...", ink);
            }
            Screen::AuthCodeShown {
                verification_uri,
                user_code,
            } => {
                line(f, 0, "Sign in with GitHub", ink);
                line(f, 2, "  on your phone, open:", ink);
                line(f, 3, &format!("  {verification_uri}"), ink);
                line(f, 5, "  and enter this code:", ink);
                // The code, inverted for weight; the QR (the verification URI)
                // on the right — scan, tap approve, done.
                let code = format!(" {user_code} ");
                let cw = (code.chars().count() as i32) * 10;
                let _ = Rectangle::new(Point::new(28, 8 + 6 * 24), Size::new(cw as u32, 20))
                    .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                    .draw(f);
                let _ = Text::with_baseline(&code, Point::new(28, 8 + 6 * 24), inv, Baseline::Top)
                    .draw(f);
                draw_qr(f, verification_uri, w - 200, 40, 200);
                hint(f, "waiting for the approval - Esc for a fresh code");
            }
            Screen::AuthRetry => {
                line(f, 0, "Sign in with GitHub", ink);
                line(f, 2, "  the sign-in did not complete.", ink);
                hint(f, "Enter retries - Backspace edits Wi-Fi");
            }
            Screen::RepoLoading => {
                line(f, 0, "Pick your notes repo", ink);
                line(f, 2, "  listing your repos...", ink);
            }
            Screen::RepoRetry => {
                line(f, 0, "Pick your notes repo", ink);
                line(f, 2, "  the repo list did not load.", ink);
                hint(f, "Enter retries");
            }
            Screen::RepoPick {
                repos,
                filter,
                sel,
                refused,
            } => {
                line(f, 0, &format!("Pick your notes repo  ({})", repos.len()), ink);
                line(f, 1, &format!("  filter: {filter}"), ink);
                let shown = filtered(repos, filter);
                // A scrolled window over the list. When a size-gate refusal is
                // showing, shrink it so the wrapped message fits below.
                let win = if refused.is_some() { 4usize } else { 6 };
                let top = sel.saturating_sub(win - 1);
                for (i, r) in shown.iter().enumerate().skip(top).take(win) {
                    let row = 3 + (i - top) as i32;
                    let mb = r.size_kb.div_ceil(1024);
                    let text = format!("  {}  ({} MB)", r.full_name, mb);
                    if i == *sel {
                        let _ = Rectangle::new(
                            Point::new(0, 8 + row * 24),
                            Size::new(w as u32, 20),
                        )
                        .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                        .draw(f);
                        line(f, row, &text, inv);
                    } else {
                        line(f, row, &text, ink);
                    }
                }
                if let Some(msg) = refused {
                    // Wrap to the panel width (FONT_10X20 → ~78 chars at x=10;
                    // keep the 2-space indent, so ~74 usable) so nothing clips.
                    let base = 3 + win as i32;
                    for (k, l) in wrap_words(msg, 74).iter().take(3).enumerate() {
                        line(f, base + k as i32, &format!("  {l}"), ink);
                    }
                }
                hint(f, "type to filter - Ctrl-N/Ctrl-P move - Enter picks");
            }
            Screen::Cloning { progress } => {
                line(f, 0, "Setting up your repo", ink);
                line(f, 2, &format!("  {progress}"), ink);
                hint(f, "this is one-time - a big tree can take minutes");
            }
            Screen::AllSet => {
                line(f, 0, "All set.", ink);
                line(f, 2, "  Your Typoena is ready - press any key to write.", ink);
            }
        }
        if let Some(n) = &self.notice {
            let _ = Text::with_baseline(
                n,
                Point::new(10, h - 48),
                ink,
                Baseline::Top,
            )
            .draw(f);
        }
    }
}

/// Draw `text` as a QR code filling a `box_px`-sized square at (x0, y0),
/// centered, with the mandatory 4-module quiet zone (the surrounding white
/// frame a scanner needs). The 1-bit panel is a natural QR surface; modules
/// are `scale`-px filled rects. Encoding a short URI can't fail; if it ever
/// does (or the box can't fit the version), the box stays blank and the
/// user code + URL text beside it still carry the flow.
fn draw_qr(f: &mut Frame, text: &str, x0: i32, y0: i32, box_px: i32) {
    let Ok(qr) = qrcodegen::QrCode::encode_text(text, qrcodegen::QrCodeEcc::Medium) else {
        return;
    };
    let size = qr.size(); // modules per side
    let scale = box_px / (size + 8); // 4-module quiet zone each side
    if scale < 2 {
        return; // too dense to scan at this box size — text fallback remains
    }
    let total = size * scale;
    let ox = x0 + (box_px - total) / 2;
    let oy = y0 + (box_px - total) / 2;
    for my in 0..size {
        for mx in 0..size {
            if qr.get_module(mx, my) {
                let _ = Rectangle::new(
                    Point::new(ox + mx * scale, oy + my * scale),
                    Size::new(scale as u32, scale as u32),
                )
                .into_styled(PrimitiveStyle::with_fill(BinaryColor::On))
                .draw(f);
            }
        }
    }
}

/// Case-insensitive substring filter over `owner/name`.
fn filtered<'a>(repos: &'a [RepoChoice], filter: &str) -> Vec<&'a RepoChoice> {
    let q = filter.to_lowercase();
    repos
        .iter()
        .filter(|r| q.is_empty() || r.full_name.to_lowercase().contains(&q))
        .collect()
}

/// Greedy word-wrap to at most `max` chars per line. Words longer than `max`
/// (e.g. a very long repo path) are hard-split so nothing clips off-panel.
fn wrap_words(s: &str, max: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in s.split_whitespace() {
        // A single word longer than the line: emit it in `max`-char chunks.
        if word.chars().count() > max {
            if !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            let chars: Vec<char> = word.chars().collect();
            for chunk in chars.chunks(max) {
                lines.push(chunk.iter().collect());
            }
            continue;
        }
        let extra = if cur.is_empty() { 0 } else { 1 };
        if cur.chars().count() + extra + word.chars().count() > max {
            lines.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() {
            cur.push(' ');
        }
        cur.push_str(word);
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

/// Case-insensitive substring filter over scanned SSIDs.
fn filtered_ssids<'a>(networks: &'a [String], filter: &str) -> Vec<&'a String> {
    let q = filter.to_lowercase();
    networks
        .iter()
        .filter(|s| q.is_empty() || s.to_lowercase().contains(&q))
        .collect()
}

/// Command-line style Ctrl-W: drop trailing spaces, then the last word.
fn delete_word(s: &mut String) {
    while s.ends_with(' ') {
        s.pop();
    }
    while let Some(c) = s.chars().last() {
        if c == ' ' {
            break;
        }
        s.pop();
    }
}

#[cfg(test)]
mod tests;
