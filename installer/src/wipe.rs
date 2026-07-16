//! Full-card reformat TUI (`--wipe`): pick a removable card, confirm exactly
//! what's about to be destroyed (device, size, and any unpublished device
//! edits), then `diskutil eraseVolume` it to a blank FAT32 volume and eject —
//! the "blank slate for first-boot-wizard testing" the firmware `just wipe`
//! recipe used to do behind a bare bash y/N prompt.
//!
//! This is distinct from the wizard's SD "wipe", which only erases `repo/`
//! before a re-clone; this reformats the whole volume. It only ever targets a
//! card from `detect_cards()` (removable-only, never the internal disk), and
//! re-checks that the device is still removable inside the worker before it
//! erases anything.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::time::Duration;

use anyhow::{Context, bail};
use ratatui::{
    Frame,
    crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Paragraph, Wrap},
};

use crate::sdcard::{self, Card};
use crate::ui::spinner;

/// Everything the confirm screen shows about the card about to be erased.
/// Gathered off-thread (a diskutil call + a git peek over the SD bus) so the UI
/// never freezes on the read.
struct Target {
    volume: PathBuf,
    device: String,
    size: String,
    fs: String,
    // Present only when the card already holds a Typoena working copy — so the
    // user sees what they're wiping, including any never-published edits.
    origin: Option<String>,
    head: Option<String>,
    dirty: usize,
}

/// Result of the off-thread card read (diskutil info + optional git inspect).
enum PrepResult {
    Ok(Box<Target>),
    Err(String),
}

/// Streamed from the erase worker to the UI.
enum WipeEvent {
    Log(String),
    Done(Result<(), String>),
}

enum Phase {
    /// Choosing among removable cards.
    Select,
    /// Reading the selected card's details off-thread.
    Preparing,
    /// Details gathered — the destructive-confirm screen; `y` commits.
    Confirm(Box<Target>),
    /// The worker is erasing + ejecting; the log streams in.
    Running,
    Done,
    Failed(String),
}

struct WipeApp {
    /// FAT32 volume label the card is reformatted to.
    label: String,
    cards: Vec<Card>,
    sel: usize,
    phase: Phase,
    /// Scrolling worker log (erase phases + eject).
    log: Vec<String>,
    /// Transient one-line feedback on the Select screen.
    status: Option<String>,
    /// Frame counter, bumped once per render, animating the spinner.
    tick: u64,
    prep_rx: Option<Receiver<PrepResult>>,
    wipe_rx: Option<Receiver<WipeEvent>>,
    should_quit: bool,
}

/// Entry point for `--wipe`. Detects cards up front (a quick, synchronous
/// diskutil scan, before the alternate screen so there's no frozen first
/// frame), pre-selecting `want_volume` if it names one, then runs the loop.
pub fn run(want_volume: Option<String>, label: String) -> anyhow::Result<()> {
    let cards = sdcard::detect_cards();
    let sel = want_volume
        .as_deref()
        .and_then(|w| {
            cards.iter().position(|c| {
                c.name == w
                    || c.volume.to_string_lossy() == w
                    || c.volume == Path::new("/Volumes").join(w)
            })
        })
        .unwrap_or(0);
    let mut app = WipeApp {
        label,
        cards,
        sel,
        phase: Phase::Select,
        log: Vec::new(),
        status: None,
        tick: 0,
        prep_rx: None,
        wipe_rx: None,
        should_quit: false,
    };
    let mut terminal = ratatui::init();
    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut WipeApp) -> anyhow::Result<()> {
    while !app.should_quit {
        app.poll();
        app.tick = app.tick.wrapping_add(1);
        terminal.draw(|frame| render(frame, app))?;
        // Poll so worker progress / spinner repaint even without a keypress.
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
    }
    Ok(())
}

impl WipeApp {
    fn selected_card(&self) -> Option<&Card> {
        if self.cards.is_empty() {
            return None;
        }
        self.cards.get(self.sel.min(self.cards.len() - 1))
    }

    fn on_key(&mut self, key: KeyEvent) {
        // Ctrl-C always quits — including mid-erase (the worker is a detached
        // diskutil; aborting the UI leaves it to finish, which is safe).
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }
        match &self.phase {
            // Busy: input locked until the worker lands (Ctrl-C above escapes).
            Phase::Preparing | Phase::Running => {}
            Phase::Confirm(_) => match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => self.begin_wipe(),
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => self.phase = Phase::Select,
                _ => {}
            },
            Phase::Done => {
                if matches!(key.code, KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter) {
                    self.should_quit = true;
                }
            }
            Phase::Failed(_) => match key.code {
                KeyCode::Char('r') => self.rescan(),
                KeyCode::Char('q') | KeyCode::Esc | KeyCode::Enter => self.should_quit = true,
                _ => {}
            },
            Phase::Select => self.on_key_select(key),
        }
    }

    fn on_key_select(&mut self, key: KeyEvent) {
        self.status = None;
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => self.should_quit = true,
            KeyCode::Up | KeyCode::BackTab | KeyCode::Char('k') => {
                self.sel = self.sel.saturating_sub(1);
            }
            KeyCode::Down | KeyCode::Tab | KeyCode::Char('j')
                if self.sel + 1 < self.cards.len() =>
            {
                self.sel += 1;
            }
            KeyCode::Char('r') => self.rescan(),
            KeyCode::Enter => self.begin_prepare(),
            _ => {}
        }
    }

    fn rescan(&mut self) {
        self.cards = sdcard::detect_cards();
        self.sel = self.sel.min(self.cards.len().saturating_sub(1));
        self.log.clear();
        self.status = None;
        self.phase = Phase::Select;
    }

    /// Enter on a card: read its details off-thread, then drop into Confirm.
    fn begin_prepare(&mut self) {
        let Some(card) = self.selected_card() else {
            self.status = Some("no card detected — insert one and press r".into());
            return;
        };
        let (volume, fs) = (card.volume.clone(), card.fs.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.prep_rx = Some(rx);
        self.phase = Phase::Preparing;
        std::thread::spawn(move || {
            let _ = tx.send(prepare(volume, fs));
        });
    }

    fn begin_wipe(&mut self) {
        let Phase::Confirm(t) = &self.phase else {
            return;
        };
        let (device, volume, label) = (t.device.clone(), t.volume.clone(), self.label.clone());
        let (tx, rx) = std::sync::mpsc::channel();
        self.wipe_rx = Some(rx);
        self.log.clear();
        self.phase = Phase::Running;
        std::thread::spawn(move || run_wipe(device, label, volume, tx));
    }

    /// Pull in finished background work once per frame.
    fn poll(&mut self) {
        if let Some(rx) = self.prep_rx.take() {
            match rx.try_recv() {
                Ok(PrepResult::Ok(t)) => self.phase = Phase::Confirm(t),
                Ok(PrepResult::Err(e)) => {
                    self.phase = Phase::Select;
                    self.status = Some(e);
                }
                Err(TryRecvError::Empty) => self.prep_rx = Some(rx),
                Err(TryRecvError::Disconnected) => {
                    self.phase = Phase::Select;
                    self.status = Some("card read failed — press r to rescan".into());
                }
            }
        }
        if let Some(rx) = self.wipe_rx.take() {
            let mut done = None;
            loop {
                match rx.try_recv() {
                    Ok(WipeEvent::Log(l)) => self.log.push(l),
                    Ok(WipeEvent::Done(r)) => {
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
                Some(Ok(())) => self.phase = Phase::Done,
                Some(Err(e)) => self.phase = Phase::Failed(e),
                None => self.wipe_rx = Some(rx),
            }
        }
    }
}

/// Off-thread card read: disk device/size (diskutil) + any Typoena repo state
/// (git over the SD bus). Repo inspection is best-effort — a card with no
/// working copy just reports no origin/HEAD/dirty.
fn prepare(volume: PathBuf, fs: String) -> PrepResult {
    let Some((device, size)) = volume_info(&volume) else {
        return PrepResult::Err(format!(
            "couldn't read disk info for {}",
            volume.display()
        ));
    };
    let (origin, head, dirty) = if sdcard::card_has_repo(&volume) {
        let i = sdcard::inspect_card(&volume);
        (i.origin, i.head, i.dirty)
    } else {
        (None, None, 0)
    };
    PrepResult::Ok(Box::new(Target {
        volume,
        device,
        size,
        fs,
        origin,
        head,
        dirty,
    }))
}

/// The volume's device node (e.g. /dev/disk4s1) and human disk size, via
/// `diskutil info` — the same two fields the bash `wipe` recipe grepped for.
fn volume_info(vol: &Path) -> Option<(String, String)> {
    let out = Command::new("diskutil").arg("info").arg(vol).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let info = String::from_utf8_lossy(&out.stdout);
    let device = sdcard::field(&info, "Device Node")?;
    let size = sdcard::field(&info, "Disk Size").unwrap_or_else(|| "?".into());
    Some((device, size))
}

/// Erase the volume to a blank FAT32 and eject, streaming each step to `tx`.
fn run_wipe(device: String, label: String, volume: PathBuf, tx: Sender<WipeEvent>) {
    let mut emit = |e: WipeEvent| {
        let _ = tx.send(e);
    };
    let res = do_wipe(&device, &label, &volume, &mut emit).map_err(|e| format!("{e:#}"));
    let _ = tx.send(WipeEvent::Done(res));
}

fn do_wipe(
    device: &str,
    label: &str,
    volume: &Path,
    emit: &mut dyn FnMut(WipeEvent),
) -> anyhow::Result<()> {
    // Defense in depth: the device came from detect_cards (removable-only), but
    // re-verify right before the irreversible erase — a wrong target here
    // reformats the wrong disk.
    let info = Command::new("diskutil")
        .arg("info")
        .arg(device)
        .output()
        .context("running diskutil info")?;
    if !info.status.success() || !sdcard::is_removable(&String::from_utf8_lossy(&info.stdout)) {
        bail!("refusing to erase {device}: not a removable card");
    }

    emit(WipeEvent::Log(format!(
        "erasing {} ({device}) → blank FAT32 '{label}' …",
        volume.display()
    )));
    let mut child = Command::new("diskutil")
        .arg("eraseVolume")
        .arg("MS-DOS FAT32")
        .arg(label)
        .arg(device)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawning diskutil eraseVolume")?;
    // diskutil prints its phase lines (Unmounting / Erasing / Mounting /
    // Finished) to stdout; stream them into the log as they land.
    if let Some(out) = child.stdout.take() {
        sdcard::split_cr_lf(out, &mut |seg: &str| {
            let seg = seg.trim();
            if !seg.is_empty() {
                emit(WipeEvent::Log(seg.to_string()));
            }
        });
    }
    // Errors go to stderr (small — read after stdout drains, no deadlock risk).
    let mut errbuf = String::new();
    if let Some(mut err) = child.stderr.take() {
        let _ = err.read_to_string(&mut errbuf);
    }
    let status = child.wait().context("waiting for diskutil eraseVolume")?;
    if !status.success() {
        bail!(
            "diskutil eraseVolume failed (exit {:?}): {}",
            status.code(),
            errbuf.trim()
        );
    }

    // Flush + eject the whole media by its device node — the mount point moved
    // with the new label, but the device node is stable across the reformat.
    emit(WipeEvent::Log("flushing & ejecting…".into()));
    let _ = Command::new("sync").status();
    let out = Command::new("diskutil")
        .arg("eject")
        .arg(device)
        .output()
        .context("running diskutil eject")?;
    if out.status.success() {
        emit(WipeEvent::Log("card ejected.".into()));
    } else {
        emit(WipeEvent::Log(format!(
            "⚠ eject failed ({}) — eject from Finder before removing",
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(())
}

// ── rendering ────────────────────────────────────────────────────────────────

fn render(frame: &mut Frame, app: &WipeApp) {
    let [title, body, footer] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0), Constraint::Length(1)])
            .areas(frame.area());
    render_title(frame, title);
    render_body(frame, body, app);
    render_footer(frame, footer, app);
}

fn render_title(frame: &mut Frame, area: Rect) {
    let line = Line::from(vec![
        Span::raw(" "),
        Span::styled("typoena", Style::new().add_modifier(Modifier::BOLD)),
        Span::styled("  ·  wipe card", Style::new().fg(Color::DarkGray)),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn render_body(frame: &mut Frame, area: Rect, app: &WipeApp) {
    let block = Block::bordered().title(" wipe card ");
    match &app.phase {
        Phase::Running | Phase::Done | Phase::Failed(_) => render_progress(frame, area, app, block),
        _ => {
            let lines = body_lines(app);
            frame.render_widget(
                Paragraph::new(Text::from(lines))
                    .block(block)
                    .wrap(Wrap { trim: false }),
                area,
            );
        }
    }
}

fn body_lines(app: &WipeApp) -> Vec<Line<'static>> {
    let dim = Style::new().fg(Color::DarkGray);
    match &app.phase {
        Phase::Preparing => vec![
            Line::styled(
                format!("{} Reading the card…", spinner(app.tick)),
                Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
            ),
            Line::from(""),
            Line::styled("Checking the device and any Typoena working copy.", dim),
        ],
        Phase::Confirm(t) => confirm_lines(t, &app.label),
        // Select (default).
        _ => select_lines(app),
    }
}

fn select_lines(app: &WipeApp) -> Vec<Line<'static>> {
    let dim = Style::new().fg(Color::DarkGray);
    let mut lines = Vec::new();
    if app.cards.is_empty() {
        lines.push(Line::styled(
            "No removable card detected.",
            Style::new().fg(Color::Yellow),
        ));
        lines.push(Line::from(""));
        lines.push(Line::styled(
            "Insert an SD card, then press r to rescan.",
            dim,
        ));
    } else {
        lines.push(Line::styled(
            "Pick the card to reformat (↑/↓), then Enter:",
            dim,
        ));
        lines.push(Line::from(""));
        for (i, c) in app.cards.iter().enumerate() {
            let sel = i == app.sel;
            let marker = if sel { "▸ " } else { "  " };
            let style = if sel {
                Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED)
            } else {
                Style::new()
            };
            lines.push(Line::styled(format!("{marker}{}  [{}]", c.name, c.fs), style));
        }
    }
    if let Some(msg) = &app.status {
        lines.push(Line::from(""));
        lines.push(Line::styled(msg.clone(), Style::new().fg(Color::Cyan)));
    }
    lines
}

/// The destructive-confirm screen: the whole point of the "validation" ask —
/// show precisely what's about to be erased before a single byte is touched.
fn confirm_lines(t: &Target, label: &str) -> Vec<Line<'static>> {
    let dim = Style::new().fg(Color::DarkGray);
    let red_bold = Style::new().fg(Color::Red).add_modifier(Modifier::BOLD);
    let mut lines = vec![
        Line::styled(
            format!("⚠  ERASE EVERYTHING on {}", t.volume.display()),
            red_bold,
        ),
        Line::from(""),
        Line::styled(format!("   device    {}", t.device), dim),
        Line::styled(format!("   size      {}", t.size), dim),
        Line::styled(format!("   current   {}", t.fs), dim),
        Line::styled(
            format!("   becomes   empty FAT32 volume labelled '{label}'"),
            Style::new().fg(Color::Green),
        ),
    ];
    if t.origin.is_some() || t.head.is_some() || t.dirty > 0 {
        lines.push(Line::from(""));
        lines.push(Line::styled("   This card holds a Typoena working copy:", dim));
        if let Some(o) = &t.origin {
            lines.push(Line::styled(format!("     origin  {o}"), dim));
        }
        if let Some(h) = &t.head {
            lines.push(Line::styled(format!("     HEAD    {h}"), dim));
        }
        if t.dirty > 0 {
            lines.push(Line::styled(
                format!(
                    "   {} unpublished device edit(s) will be LOST (never pushed).",
                    t.dirty
                ),
                red_bold,
            ));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::styled("This cannot be undone.", red_bold));
    lines.push(Line::styled(
        "Press y to erase · n to cancel.",
        Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
    ));
    lines
}

/// Running / Done / Failed: a status header plus a tail of the worker log.
fn render_progress(frame: &mut Frame, area: Rect, app: &WipeApp, block: Block) {
    let status = match &app.phase {
        Phase::Failed(e) => Line::styled(format!("Failed: {e}"), Style::new().fg(Color::Red)),
        Phase::Done => Line::styled(
            format!(
                "Card erased ✓ — blank FAT32 '{}', ejected. Remove it.",
                app.label
            ),
            Style::new().fg(Color::Green),
        ),
        _ => Line::styled(
            format!("{} Erasing the card…  (Ctrl-C aborts)", spinner(app.tick)),
            Style::new().fg(Color::Yellow),
        ),
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let [top, log] =
        Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(status).wrap(Wrap { trim: false }), top);
    let start = app.log.len().saturating_sub(log.height as usize);
    let log_lines: Vec<Line> = app.log[start..]
        .iter()
        .map(|l| Line::styled(l.clone(), Style::new().fg(Color::DarkGray)))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(log_lines)).wrap(Wrap { trim: false }),
        log,
    );
}

fn render_footer(frame: &mut Frame, area: Rect, app: &WipeApp) {
    let key = |k: &str| {
        Span::styled(
            format!(" {k} "),
            Style::new().add_modifier(Modifier::REVERSED),
        )
    };
    let lbl = |l: &'static str| Span::styled(l, Style::new().fg(Color::DarkGray));
    let sep = || Span::raw("  ");
    let spans = match &app.phase {
        Phase::Select => vec![
            key("↑↓ / j k"),
            lbl(" card"),
            sep(),
            key("r"),
            lbl(" rescan"),
            sep(),
            key("Enter"),
            lbl(" erase"),
            sep(),
            key("q"),
            lbl(" quit"),
        ],
        Phase::Preparing => vec![key("^C"), lbl(" quit")],
        Phase::Confirm(_) => vec![
            key("y"),
            lbl(" erase"),
            sep(),
            key("n"),
            lbl(" cancel"),
            sep(),
            key("^C"),
            lbl(" quit"),
        ],
        Phase::Running => vec![key("^C"), lbl(" abort")],
        Phase::Failed(_) => vec![
            key("r"),
            lbl(" rescan"),
            sep(),
            key("q"),
            lbl(" quit"),
        ],
        Phase::Done => vec![key("q"), lbl(" quit")],
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::{Terminal, backend::TestBackend};

    fn app_with(phase: Phase) -> WipeApp {
        WipeApp {
            label: "TYPOENA".into(),
            cards: vec![Card {
                volume: "/Volumes/TYPOENA".into(),
                name: "TYPOENA".into(),
                fs: "MS-DOS FAT32".into(),
                fat: true,
            }],
            sel: 0,
            phase,
            log: Vec::new(),
            status: None,
            tick: 0,
            prep_rx: None,
            wipe_rx: None,
            should_quit: false,
        }
    }

    fn screen(app: &WipeApp) -> String {
        let mut terminal = Terminal::new(TestBackend::new(90, 24)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        terminal.backend().to_string()
    }

    fn sample_target() -> Box<Target> {
        Box::new(Target {
            volume: "/Volumes/TYPOENA".into(),
            device: "/dev/disk4s1".into(),
            size: "31.9 GB".into(),
            fs: "MS-DOS FAT32".into(),
            origin: Some("https://github.com/you/notes.git".into()),
            head: Some("abc1234".into()),
            dirty: 2,
        })
    }

    #[test]
    fn every_phase_renders_without_panicking() {
        for phase in [
            Phase::Select,
            Phase::Preparing,
            Phase::Confirm(sample_target()),
            Phase::Running,
            Phase::Done,
            Phase::Failed("boom".into()),
        ] {
            let _ = screen(&app_with(phase));
        }
    }

    #[test]
    fn confirm_screen_shows_the_target_and_warns() {
        let s = screen(&app_with(Phase::Confirm(sample_target())));
        assert!(s.contains("ERASE EVERYTHING"), "must warn loudly:\n{s}");
        assert!(s.contains("/dev/disk4s1"), "must name the device:\n{s}");
        assert!(s.contains("31.9 GB"), "must show the size:\n{s}");
        assert!(
            s.contains("unpublished"),
            "must flag unpublished device edits:\n{s}"
        );
        assert!(s.contains("cannot be undone"), "must be explicit:\n{s}");
    }

    #[test]
    fn y_on_confirm_starts_the_erase() {
        let mut app = app_with(Phase::Confirm(sample_target()));
        app.on_key(KeyEvent::new(KeyCode::Char('y'), KeyModifiers::NONE));
        assert!(matches!(app.phase, Phase::Running), "y must commit");
        assert!(app.wipe_rx.is_some(), "a worker channel should be live");
    }

    #[test]
    fn n_on_confirm_cancels_back_to_select() {
        let mut app = app_with(Phase::Confirm(sample_target()));
        app.on_key(KeyEvent::new(KeyCode::Char('n'), KeyModifiers::NONE));
        assert!(matches!(app.phase, Phase::Select), "n must back out");
    }

    #[test]
    fn keys_are_inert_while_erasing() {
        let mut app = app_with(Phase::Running);
        app.on_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.should_quit, "q must not quit mid-erase");
        // Ctrl-C is the one escape hatch.
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit, "Ctrl-C must still quit");
    }

    #[test]
    fn done_screen_reports_success() {
        let s = screen(&app_with(Phase::Done));
        assert!(s.contains("erased"), "done must confirm the erase:\n{s}");
        assert!(s.contains("ejected"), "done must say it ejected:\n{s}");
    }
}
