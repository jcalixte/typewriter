//! Wizard state: which step we're on, the results each step produces, and
//! step-aware key handling (nav steps, the Configure form, and the SD-card step
//! each behave differently).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::auth::{self, AuthEvent};
use crate::config::{Config, Field, keychain_wifi_password};
use crate::preflight::Preflight;
use crate::sdcard::{self, Card, CardInspect, SdEvent};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Preflight,
    Configure,
    SdCard,
    Done,
}

impl Step {
    pub const ALL: [Step; 4] = [Step::Preflight, Step::Configure, Step::SdCard, Step::Done];

    pub fn title(self) -> &'static str {
        match self {
            Step::Preflight => "Preflight",
            Step::Configure => "Configure",
            Step::SdCard => "SD card",
            Step::Done => "Done",
        }
    }

    pub fn index(self) -> usize {
        Step::ALL.iter().position(|&s| s == self).unwrap_or(0)
    }

    fn next(self) -> Step {
        Step::ALL[(self.index() + 1).min(Step::ALL.len() - 1)]
    }

    fn prev(self) -> Step {
        Step::ALL[self.index().saturating_sub(1)]
    }
}

/// The "Sign in with GitHub" device flow (started with ^G on Configure). While
/// it's not Idle the Configure step is modal: the sign-in panel owns the keys
/// (Esc cancels, o reopens the browser) so the user can't half-edit the form
/// while a background worker is about to overwrite the token field.
pub enum AuthState {
    Idle,
    /// Asking GitHub for a one-time code.
    Starting,
    /// Code issued — the user is off authorizing in the browser; we poll.
    Waiting {
        user_code: String,
        verification_uri: String,
    },
}

/// The proactive repo-access probe (does the token see the notes repo?). Each
/// verdict remembers which remote it judged, so editing the field silently
/// retires a stale flag instead of mislabeling the new URL.
pub enum RepoCheck {
    NotRun,
    Checking { remote: String },
    Granted { remote: String },
    Missing { remote: String },
}

pub enum SdState {
    Idle,
    /// The selected card already holds a repo; awaiting an explicit `y` to wipe.
    ConfirmWipe(CardInspect),
    Running,
    Done,
    Failed(String),
}

/// A background computation currently owning the UI. Each variant carries the
/// caption shown next to the spinner while the work runs off the UI thread; the
/// point is that a shell-out (diskutil, git, the Keychain prompt) never freezes
/// the render loop mid-wizard.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    None,
    Preflight,
    DetectingCards,
    PreparingCard,
    Keychain,
}

impl Busy {
    /// Spinner caption, or None when idle.
    pub fn label(self) -> Option<&'static str> {
        match self {
            Busy::None => None,
            Busy::Preflight => Some("Checking your Mac…"),
            Busy::DetectingCards => Some("Scanning for SD cards…"),
            Busy::PreparingCard => Some("Reading the card…"),
            Busy::Keychain => Some("Asking Keychain…"),
        }
    }
}

/// The output of a background task, applied on the UI thread when it lands.
enum TaskResult {
    Preflight(Preflight),
    Cards(Vec<Card>),
    Prepared {
        has_repo: bool,
        inspect: Option<CardInspect>,
    },
    Keychain {
        ssid: String,
        pw: Option<String>,
    },
}

pub struct App {
    pub step: Step,
    pub preflight: Preflight,
    pub config: Config,
    /// Focused field index within the Configure form.
    pub focus: usize,
    /// Transient one-line feedback (e.g. the Keychain-lookup result).
    pub status: Option<String>,
    // ── SD-card step ──
    pub cards: Vec<Card>,
    pub card_sel: usize,
    pub sd: SdState,
    pub sd_log: Vec<String>,
    /// Latest git-progress tick (phase, 0..=100), driving the SD-step gauge.
    pub sd_progress: Option<(String, u16)>,
    sd_rx: Option<Receiver<SdEvent>>,
    // ── GitHub sign-in (device flow) ──
    pub auth: AuthState,
    auth_rx: Option<Receiver<AuthEvent>>,
    /// Raised to stop the polling worker when the user cancels.
    auth_cancel: Option<Arc<AtomicBool>>,
    /// Proactive "can this token see the repo" probe (advisory — never blocks).
    pub repo_check: RepoCheck,
    check_rx: Option<Receiver<(String, auth::RepoAccess)>>,
    // ── background computation ──
    /// The off-thread work currently running (spinner + locked input), if any.
    pub busy: Busy,
    /// Frame counter, bumped once per render loop, animating the spinner.
    pub tick: u64,
    /// When the UI came up — the wall-clock origin the header's typewriter
    /// intro plays against, so its pace is independent of the render cadence.
    pub started: Instant,
    task_rx: Option<Receiver<TaskResult>>,
    pub should_quit: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            step: Step::Preflight,
            // Filled by the startup task (see `begin_startup`) so launch paints
            // immediately instead of blocking on the diskutil scan.
            preflight: Preflight { checks: Vec::new() },
            config: Config::derived(),
            focus: 0,
            status: None,
            cards: Vec::new(),
            card_sel: 0,
            sd: SdState::Idle,
            sd_log: Vec::new(),
            sd_progress: None,
            sd_rx: None,
            auth: AuthState::Idle,
            auth_rx: None,
            auth_cancel: None,
            repo_check: RepoCheck::NotRun,
            check_rx: None,
            busy: Busy::Preflight,
            tick: 0,
            started: Instant::now(),
            task_rx: None,
            should_quit: false,
        }
    }

    /// Kick the first environment scan off the UI thread. Call once, right after
    /// construction — `new()` leaves `busy = Preflight` so the first frame already
    /// shows the spinner. (Kept out of `new()` so tests stay thread-free.)
    pub fn begin_startup(&mut self) {
        self.begin_preflight();
    }

    pub fn on_key(&mut self, key: KeyEvent) {
        // Ctrl-C always quits, on any step (even mid-typing / mid-run).
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        // A background computation owns the UI — ignore input until it lands
        // (Ctrl-C above is the one escape). Tasks are short shell-outs.
        if self.busy != Busy::None {
            return;
        }
        self.status = None;
        // Ctrl-N / Ctrl-P jump a whole step from anywhere, independent of the
        // in-step field/card focus (Tab only spills at the ends). Back is always
        // safe; forward respects the same gate as the per-step forward keys — so
        // on the SD step (forward is earned by writing the card) ^N refuses and
        // says why via the status snackbar. Suppressed while the SD step owns a
        // modal (a running write, or the wipe-confirm), where leaving is wrong —
        // those keys fall through to the SD handler, which ignores them.
        let sd_modal = self.step == Step::SdCard
            && matches!(self.sd, SdState::Running | SdState::ConfirmWipe(_));
        // The GitHub sign-in panel is modal the same way: leaving Configure
        // mid-flow would strand the code screen, so ^N/^P fall through to the
        // Configure handler, which routes them to the panel's own keys.
        let auth_modal = self.step == Step::Configure && !matches!(self.auth, AuthState::Idle);
        if !sd_modal && !auth_modal && key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('p') => {
                    self.prev();
                    return;
                }
                KeyCode::Char('n') => {
                    match self.step {
                        Step::SdCard => {
                            self.status =
                                Some("write the card to finish this step — ^P steps back".into());
                        }
                        Step::Done => {} // last step — nowhere forward to go
                        _ => self.next(),
                    }
                    return;
                }
                _ => {}
            }
        }
        match self.step {
            Step::Configure => self.on_key_configure(key),
            Step::SdCard => self.on_key_sdcard(key),
            _ => self.on_key_nav(key),
        }
    }

    /// Non-form steps: single-key navigation. Arrows, Tab/Shift-Tab, and vim
    /// h/j/k/l all move between steps so no pointer or arrow key is required.
    fn on_key_nav(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Char('r') if self.step == Step::Preflight => self.begin_preflight(),
            KeyCode::Enter
            | KeyCode::Tab
            | KeyCode::Down
            | KeyCode::Right
            | KeyCode::Char('j')
            | KeyCode::Char('l') => self.next(),
            KeyCode::Up
            | KeyCode::BackTab
            | KeyCode::Left
            | KeyCode::Char('k')
            | KeyCode::Char('h') => self.prev(),
            _ => {}
        }
    }

    /// Configure form: typing edits the focused field; field navigation spills
    /// over into step navigation at the ends.
    fn on_key_configure(&mut self, key: KeyEvent) {
        // The sign-in panel owns the keys while the device flow runs. Modified
        // chars (^N/^P step jumps land here too) are deliberately inert — only
        // a plain Esc/n/q cancels, so a reflexive ^N can't kill the flow.
        if !matches!(self.auth, AuthState::Idle) {
            match key.code {
                _ if key.modifiers != KeyModifiers::NONE => {}
                KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('q') => self.cancel_device_flow(),
                KeyCode::Char('o') => {
                    if let AuthState::Waiting {
                        verification_uri, ..
                    } = &self.auth
                    {
                        auth::open_browser(verification_uri);
                    }
                }
                _ => {}
            }
            return;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let last = Field::ALL.len() - 1;
        match key.code {
            KeyCode::Esc => self.should_quit = true,
            // Shift-Tab / ↑ walk fields up; past the first, back a step.
            KeyCode::Up | KeyCode::BackTab => {
                if self.focus > 0 {
                    self.focus -= 1;
                } else {
                    self.prev();
                }
            }
            KeyCode::Down if self.focus < last => self.focus += 1,
            // Tab walks fields down; past the last, on to the next step — so a
            // pure Tab/Shift-Tab rhythm carries you through the whole wizard.
            KeyCode::Tab => {
                if self.focus < last {
                    self.focus += 1;
                } else {
                    self.next();
                }
            }
            KeyCode::Enter => {
                if self.focus < last {
                    self.focus += 1;
                } else {
                    self.next();
                }
            }
            KeyCode::Char('u') if ctrl => self.config.get_mut(self.focused_field()).clear(),
            KeyCode::Char('k') if ctrl => self.fill_wifi_from_keychain(),
            KeyCode::Char('g') if ctrl => self.begin_device_flow(),
            // ^O: open the app-install page and watch for access. (^I would be
            // the mnemonic but Ctrl-I *is* Tab on a terminal.)
            KeyCode::Char('o') if ctrl => self.install_app(),
            KeyCode::Backspace => {
                self.config.get_mut(self.focused_field()).pop();
            }
            KeyCode::Char(c) if !ctrl => self.config.get_mut(self.focused_field()).push(c),
            _ => {}
        }
    }

    /// SD-card step: pick a card, then start (or confirm-wipe-then-start) the
    /// provision.
    fn on_key_sdcard(&mut self, key: KeyEvent) {
        match self.sd {
            SdState::Running => return, // input locked while the worker runs
            SdState::ConfirmWipe(_) => {
                match key.code {
                    KeyCode::Char('y') | KeyCode::Char('Y') => self.start_provision(true),
                    KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                        self.sd = SdState::Idle
                    }
                    _ => {}
                }
                return;
            }
            _ => {}
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            // ↑/k/Shift-Tab walk the card list up; past the top, back a step.
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
                if self.card_sel > 0 {
                    self.card_sel -= 1;
                } else {
                    self.prev();
                }
            }
            KeyCode::Down | KeyCode::Char('j') if self.card_sel + 1 < self.cards.len() => {
                self.card_sel += 1;
            }
            KeyCode::Tab if self.card_sel + 1 < self.cards.len() => self.card_sel += 1,
            // ←/h steps back; forward is deliberately gated behind writing a card.
            KeyCode::Left | KeyCode::Char('h') => self.prev(),
            KeyCode::Char('r') => {
                self.sd = SdState::Idle;
                self.sd_log.clear();
                self.sd_progress = None;
                self.begin_detect_cards();
            }
            KeyCode::Enter => self.attempt_provision(),
            _ => {}
        }
    }

    pub fn focused_field(&self) -> Field {
        Field::ALL[self.focus.min(Field::ALL.len() - 1)]
    }

    /// Whether the current step's forward gate is satisfied, so Enter/Tab may
    /// advance. Drives the sidebar's "ready / finish this step" affordance.
    pub fn forward_open(&self) -> bool {
        match self.step {
            Step::Preflight => true, // advisory — never blocks
            Step::Configure => self.config.missing_required().is_empty(),
            Step::SdCard => matches!(self.sd, SdState::Done),
            Step::Done => false,
        }
    }

    /// The step a forward move would land on, if any (None on the last step).
    pub fn next_step(&self) -> Option<Step> {
        (self.step != Step::Done).then(|| self.step.next())
    }

    fn fill_wifi_from_keychain(&mut self) {
        let ssid = self.config.wifi_ssid.clone();
        if ssid.trim().is_empty() {
            self.status = Some("set the Wi-Fi SSID first, then ^K".into());
            return;
        }
        // The `security` lookup pops a macOS auth dialog and blocks until it's
        // dismissed — run it off-thread so the wizard doesn't freeze behind it.
        self.spawn(Busy::Keychain, move || {
            let pw = keychain_wifi_password(&ssid);
            TaskResult::Keychain { ssid, pw }
        });
    }

    /// ^G: start the GitHub sign-in. The worker requests a device code, reports
    /// it (we render it + GitHub opens in the browser), then polls until the
    /// user authorizes; the token lands in the GitHub-token field.
    fn begin_device_flow(&mut self) {
        let (tx, rx) = std::sync::mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));
        self.auth_rx = Some(rx);
        self.auth_cancel = Some(cancel.clone());
        self.auth = AuthState::Starting;
        std::thread::spawn(move || auth::run_device_flow(tx, cancel));
    }

    /// ^O: the fix for a Missing access flag — open the install page in the
    /// browser and keep probing so the flag flips to green by itself once the
    /// user grants the repo (token access is evaluated live by GitHub).
    fn install_app(&mut self) {
        if self.config.pat.trim().is_empty() {
            self.status = Some("sign in first (^G) or paste a token, then ^O".into());
            return;
        }
        if self.config.remote().trim().is_empty() {
            self.status = Some("set the Git remote first — ^O checks access to that repo".into());
            return;
        }
        auth::open_browser(auth::APP_INSTALL_URL);
        self.status = Some("browser opened — grant your notes repo; watching for access…".into());
        self.spawn_repo_check(true);
    }

    /// Probe repo access off-thread. `watch: true` keeps probing (every 5 s,
    /// ~2 min) until access appears — used after ^O so the flag goes green the
    /// moment the user finishes installing in the browser.
    fn spawn_repo_check(&mut self, watch: bool) {
        let token = self.config.pat.clone();
        let remote = self.config.remote();
        if token.trim().is_empty() || remote.trim().is_empty() {
            return;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        self.check_rx = Some(rx); // replaces (and thereby retires) any prior probe
        self.repo_check = RepoCheck::Checking {
            remote: remote.clone(),
        };
        std::thread::spawn(move || {
            let rounds = if watch { 24 } else { 1 };
            for i in 0..rounds {
                if i > 0 {
                    std::thread::sleep(std::time::Duration::from_secs(5));
                }
                let access = auth::check_repo_access(&token, &remote);
                let granted = access == auth::RepoAccess::Granted;
                if tx.send((remote.clone(), access)).is_err() || granted {
                    break; // probe retired by a newer one, or job done
                }
            }
        });
    }

    fn drain_check(&mut self) {
        let Some(rx) = self.check_rx.take() else {
            return;
        };
        let mut open = true;
        loop {
            match rx.try_recv() {
                Ok((remote, access)) => {
                    self.repo_check = match access {
                        auth::RepoAccess::Granted => RepoCheck::Granted { remote },
                        auth::RepoAccess::Missing => RepoCheck::Missing { remote },
                        // Can't tell — show nothing rather than guess.
                        auth::RepoAccess::Unknown => RepoCheck::NotRun,
                    };
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    open = false;
                    // A watcher that timed out mid-Checking never got a verdict.
                    if matches!(self.repo_check, RepoCheck::Checking { .. }) {
                        self.repo_check = RepoCheck::NotRun;
                    }
                    break;
                }
            }
        }
        if open {
            self.check_rx = Some(rx);
        }
    }

    fn cancel_device_flow(&mut self) {
        if let Some(c) = self.auth_cancel.take() {
            c.store(true, Ordering::Relaxed);
        }
        self.auth_rx = None; // the worker's late sends land nowhere
        self.auth = AuthState::Idle;
        self.status = Some("sign-in cancelled — ^G restarts it, or paste a PAT".into());
    }

    fn begin_preflight(&mut self) {
        self.spawn(Busy::Preflight, || TaskResult::Preflight(Preflight::run()));
    }

    fn begin_detect_cards(&mut self) {
        self.spawn(Busy::DetectingCards, || {
            TaskResult::Cards(sdcard::detect_cards())
        });
    }

    /// Spawn `f` on a worker thread, showing `busy`'s spinner until it lands.
    /// Only one task runs at a time — input is locked while `busy` is set, so a
    /// second can't start before the first is drained.
    fn spawn<F>(&mut self, busy: Busy, f: F)
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::channel();
        self.task_rx = Some(rx);
        self.busy = busy;
        std::thread::spawn(move || {
            let _ = tx.send(f());
        });
    }

    fn selected_volume(&self) -> Option<std::path::PathBuf> {
        self.cards
            .get(self.card_sel.min(self.cards.len().saturating_sub(1)))
            .map(|c| c.volume.clone())
    }

    /// Enter on a card: validate, then either start a fresh provision or, if the
    /// card already holds a repo, drop into the wipe-confirm screen.
    fn attempt_provision(&mut self) {
        if self.cards.is_empty() {
            self.status = Some("no card detected — insert one and press r".into());
            return;
        }
        if !self.config.missing_required().is_empty() {
            self.status = Some("fill the required fields on the Configure step first".into());
            return;
        }
        let Some(vol) = self.selected_volume() else {
            return;
        };
        // `card_has_repo` + `inspect_card` read git over the SD bus — run them
        // off-thread; `apply_task` then routes to wipe-confirm or straight to the
        // provision.
        self.spawn(Busy::PreparingCard, move || {
            let has_repo = sdcard::card_has_repo(&vol);
            let inspect = has_repo.then(|| sdcard::inspect_card(&vol));
            TaskResult::Prepared { has_repo, inspect }
        });
    }

    fn start_provision(&mut self, wipe: bool) {
        let Some(card_volume) = self.selected_volume() else {
            return;
        };
        let plan = sdcard::Plan {
            remote: self.config.remote(),
            pat: self.config.pat.clone(),
            card_volume,
            conf_body: self.config.to_conf(),
            wipe,
        };
        let (tx, rx) = std::sync::mpsc::channel();
        self.sd_rx = Some(rx);
        self.sd = SdState::Running;
        self.sd_log.clear();
        self.sd_progress = None;
        std::thread::spawn(move || sdcard::run_provision(plan, tx));
    }

    /// Pull worker progress into the log; advance to Done on success.
    pub fn drain_worker(&mut self) {
        let Some(rx) = self.sd_rx.take() else {
            return;
        };
        let mut done = None;
        loop {
            match rx.try_recv() {
                Ok(SdEvent::Log(l)) => self.sd_log.push(l),
                Ok(SdEvent::Progress { phase, pct }) => self.sd_progress = Some((phase, pct)),
                Ok(SdEvent::Done(r)) => {
                    done = Some(r);
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = Some(Err("worker thread stopped unexpectedly".into()));
                    break;
                }
            }
        }
        match done {
            Some(Ok(())) => {
                self.sd = SdState::Done;
                self.step = Step::Done;
            }
            Some(Err(e)) => self.sd = SdState::Failed(e),
            None => self.sd_rx = Some(rx),
        }
    }

    /// Called once per render loop: pull in any finished background work (the
    /// clone worker and the general task worker).
    pub fn poll_background(&mut self) {
        self.drain_worker();
        self.drain_task();
        self.drain_auth();
        self.drain_check();
    }

    fn drain_auth(&mut self) {
        let Some(rx) = self.auth_rx.take() else {
            return;
        };
        loop {
            match rx.try_recv() {
                Ok(AuthEvent::Code {
                    user_code,
                    verification_uri,
                }) => {
                    self.auth = AuthState::Waiting {
                        user_code,
                        verification_uri,
                    };
                }
                Ok(AuthEvent::Done(result)) => {
                    self.auth = AuthState::Idle;
                    self.auth_cancel = None;
                    self.status = Some(match result {
                        Ok(t) => {
                            self.config.pat = t.access_token;
                            // Fill the username from the signed-in account so a
                            // bare repo name (`notes`) completes to `<login>/notes`
                            // — the sign-in already knows who you are, so you
                            // shouldn't have to retype it. Empty only if the login
                            // lookup failed; then keep whatever was derived/typed.
                            if !t.login.trim().is_empty() {
                                self.config.gh_user = t.login;
                            }
                            // Probe access right away (no-op until the remote is
                            // set) so the not-installed case flags at Configure
                            // instead of 403ing at the clone.
                            self.spawn_repo_check(false);
                            match t.expires_in {
                                // Expiry ON in the app settings: still works, but
                                // worth flagging — the device's push dies with it.
                                Some(secs) => format!(
                                    "signed in ✓ — note: this token expires in {}h (app setting)",
                                    secs.div_ceil(3600)
                                ),
                                None => "signed in ✓ — GitHub token filled".into(),
                            }
                        }
                        Err(e) => format!("sign-in failed: {e}"),
                    });
                    return; // auth rx drops: the flow is over
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.auth = AuthState::Idle;
                    self.auth_cancel = None;
                    self.status = Some("sign-in stopped unexpectedly — ^G to retry".into());
                    return;
                }
            }
        }
        self.auth_rx = Some(rx);
    }

    fn drain_task(&mut self) {
        let Some(rx) = self.task_rx.take() else {
            return;
        };
        match rx.try_recv() {
            Ok(result) => {
                self.busy = Busy::None;
                self.apply_task(result);
            }
            Err(TryRecvError::Empty) => self.task_rx = Some(rx),
            // The worker panicked (dropped the sender): clear busy so the UI
            // unlocks rather than spinning forever.
            Err(TryRecvError::Disconnected) => self.busy = Busy::None,
        }
    }

    fn apply_task(&mut self, result: TaskResult) {
        match result {
            TaskResult::Preflight(pf) => self.preflight = pf,
            TaskResult::Cards(cards) => {
                self.cards = cards;
                self.card_sel = 0;
            }
            TaskResult::Prepared { has_repo, inspect } => {
                if has_repo {
                    // `inspect` is Some whenever `has_repo` (see attempt_provision).
                    self.sd = SdState::ConfirmWipe(inspect.unwrap_or(CardInspect {
                        origin: None,
                        head: None,
                        dirty: 0,
                    }));
                } else {
                    self.start_provision(false);
                }
            }
            TaskResult::Keychain { ssid, pw } => {
                self.status = Some(match pw {
                    Some(p) => {
                        self.config.wifi_pass = p;
                        format!("filled Wi-Fi password for “{ssid}” from Keychain")
                    }
                    None => "no Keychain password found (or the lookup was cancelled)".into(),
                });
            }
        }
    }

    fn next(&mut self) {
        self.step = self.step.next();
        self.on_enter();
    }

    fn prev(&mut self) {
        self.step = self.step.prev();
        self.on_enter();
    }

    fn on_enter(&mut self) {
        if self.step == Step::SdCard && matches!(self.sd, SdState::Idle) {
            self.begin_detect_cards();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    /// An app parked past startup, so no background task owns input.
    fn idle_app() -> App {
        let mut app = App::new();
        app.busy = Busy::None;
        app
    }

    #[test]
    fn ctrl_n_advances_a_whole_step() {
        let mut app = idle_app();
        assert!(app.step == Step::Preflight);
        app.on_key(ctrl('n'));
        assert!(app.step == Step::Configure);
    }

    #[test]
    fn ctrl_p_steps_back_ignoring_field_focus() {
        let mut app = idle_app();
        app.step = Step::Configure;
        app.focus = 2; // mid-form: a whole-step jump must not walk fields first
        app.on_key(ctrl('p'));
        assert!(app.step == Step::Preflight);
    }

    #[test]
    fn ctrl_n_on_sd_warns_and_holds() {
        let mut app = idle_app();
        app.step = Step::SdCard; // set directly: no on_enter, so no card scan spawns
        app.sd = SdState::Idle;
        app.on_key(ctrl('n'));
        assert!(
            app.step == Step::SdCard,
            "the write-gated step must not advance"
        );
        assert!(app.status.is_some(), "a snackbar warning should be shown");
    }

    #[test]
    fn ctrl_p_ignored_while_card_is_writing() {
        let mut app = idle_app();
        app.step = Step::SdCard;
        app.sd = SdState::Running;
        app.on_key(ctrl('p'));
        assert!(app.step == Step::SdCard, "must not leave a running write");
    }

    /// An app parked on Configure with the sign-in panel up.
    fn signing_in_app() -> App {
        let mut app = idle_app();
        app.step = Step::Configure;
        app.auth = AuthState::Waiting {
            user_code: "WDJB-MJHT".into(),
            verification_uri: "https://github.com/login/device".into(),
        };
        app
    }

    #[test]
    fn esc_cancels_the_sign_in_instead_of_quitting() {
        let mut app = signing_in_app();
        app.on_key(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE));
        assert!(!app.should_quit, "Esc must cancel the flow, not the app");
        assert!(matches!(app.auth, AuthState::Idle));
        assert!(app.status.is_some(), "cancelling should say how to retry");
    }

    #[test]
    fn step_jumps_are_held_while_signing_in() {
        let mut app = signing_in_app();
        app.on_key(ctrl('n'));
        app.on_key(ctrl('p'));
        assert!(app.step == Step::Configure, "the sign-in panel is modal");
        assert!(matches!(app.auth, AuthState::Waiting { .. }));
    }

    #[test]
    fn ctrl_o_without_a_token_explains_instead_of_probing() {
        let mut app = idle_app();
        app.step = Step::Configure;
        app.on_key(ctrl('o'));
        assert!(matches!(app.repo_check, RepoCheck::NotRun), "no probe yet");
        assert!(app.status.is_some(), "should say to sign in first");
    }

    #[test]
    fn a_stale_access_flag_is_hidden_after_editing_the_remote() {
        // The verdict names the remote it judged; the UI only shows it while
        // that still matches. Simulate a verdict then a remote edit.
        let mut app = idle_app();
        app.step = Step::Configure;
        app.config.remote_url = "you/notes".into();
        app.repo_check = RepoCheck::Missing {
            remote: app.config.remote(),
        };
        app.config.remote_url = "you/other".into();
        let RepoCheck::Missing { remote } = &app.repo_check else {
            panic!("state itself is untouched");
        };
        assert_ne!(
            *remote,
            app.config.remote(),
            "mismatch is what the UI keys the hide on"
        );
    }

    #[test]
    fn sign_in_fills_the_github_username_for_bare_repo_completion() {
        use crate::auth::{AuthEvent, Token};
        let mut app = signing_in_app();
        // The user never typed a username; a bare repo name is waiting.
        app.config.gh_user.clear();
        app.config.remote_url = "notes".into();
        let (tx, rx) = std::sync::mpsc::channel();
        app.auth_rx = Some(rx);
        tx.send(AuthEvent::Done(Ok(Token {
            access_token: "ghu_abc".into(),
            login: "octocat".into(),
            expires_in: None,
        })))
        .unwrap();
        app.drain_auth();
        assert_eq!(app.config.gh_user, "octocat", "sign-in fills the username");
        assert_eq!(
            app.config.remote(),
            "https://github.com/octocat/notes.git",
            "so the bare repo name now completes against it"
        );
    }

    #[test]
    fn a_failed_login_lookup_keeps_the_derived_username() {
        use crate::auth::{AuthEvent, Token};
        let mut app = signing_in_app();
        app.config.gh_user = "derived-name".into();
        let (tx, rx) = std::sync::mpsc::channel();
        app.auth_rx = Some(rx);
        // Token arrives but the /user lookup came back empty (network blip).
        tx.send(AuthEvent::Done(Ok(Token {
            access_token: "ghu_abc".into(),
            login: String::new(),
            expires_in: None,
        })))
        .unwrap();
        app.drain_auth();
        assert_eq!(
            app.config.gh_user, "derived-name",
            "an empty login must not clobber what was already there"
        );
    }

    #[test]
    fn typing_does_not_edit_fields_while_signing_in() {
        let mut app = signing_in_app();
        app.focus = 0;
        app.on_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert!(
            !app.config.wifi_ssid.contains('x'),
            "form editing is suspended under the sign-in panel"
        );
    }
}
