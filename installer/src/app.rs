//! Wizard state: which step we're on, the results each step produces, and
//! step-aware key handling (nav steps, the Configure form, and the SD-card step
//! each behave differently).

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Instant;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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
            remote: self.config.remote_url.clone(),
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
