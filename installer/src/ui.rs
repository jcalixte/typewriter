//! Rendering. Theme-agnostic: no hard-coded background (the terminal's own),
//! bold/reverse for emphasis, and the conventional green/yellow/red for status
//! so it reads on any color scheme.

use ratatui::{
    Frame,
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, Gauge, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, Busy, SdState, Step};
use crate::config::Field;
use crate::preflight::Status;

/// One frame of the braille spinner for the given tick.
fn spinner(tick: u64) -> char {
    const FRAMES: [char; 10] = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
    FRAMES[(tick as usize) % FRAMES.len()]
}

/// A spinner + caption line, shown while a background computation runs.
fn busy_line(app: &App, label: &str) -> Line<'static> {
    Line::styled(
        format!("{} {label}", spinner(app.tick)),
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD),
    )
}

pub fn render(frame: &mut Frame, app: &App) {
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    render_header(frame, header, app);
    let [steps, main] =
        Layout::horizontal([Constraint::Length(22), Constraint::Min(0)]).areas(body);
    render_steps(frame, steps, app);
    render_main(frame, main, app);
    render_footer(frame, footer, app);
}

/// The product name, typed out one letter at a time by the header intro.
const NAME: &str = "typoena";
/// Milliseconds between revealed letters — a hair over the 100 ms render tick,
/// so each repaint lands about one new key.
const KEY_MS: u128 = 110;
/// Tagline, lifted verbatim from typoena.dev so the two read as one product.
const TAGLINE: &str = "A distraction-free writing machine.";

/// A two-line brand header: the name types itself in with a trailing block
/// caret that keeps blinking once it's done (the machine, waiting for you),
/// and the tagline lands beneath once the name is complete.
fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed = app.started.elapsed().as_millis();
    let shown = ((elapsed / KEY_MS) as usize).min(NAME.len()); // NAME is ASCII
    let done = shown == NAME.len();

    // ~530 ms half-period: a calm, cursor-like blink at the idle render rate.
    let caret = if (elapsed / 530) % 2 == 0 {
        Span::styled(" ", Style::new().add_modifier(Modifier::REVERSED))
    } else {
        Span::raw(" ")
    };
    let title = Line::from(vec![
        Span::styled(
            NAME[..shown].to_string(),
            Style::new().add_modifier(Modifier::BOLD),
        ),
        caret,
    ]);

    let subtitle = if done {
        Line::styled(TAGLINE, Style::new().fg(Color::DarkGray))
    } else {
        Line::from("")
    };

    frame.render_widget(Paragraph::new(Text::from(vec![title, subtitle])), area);
}

fn render_steps(frame: &mut Frame, area: Rect, app: &App) {
    // The list needs 4 rows + a border; the rest goes to the movement legend.
    let [list_area, legend_area] =
        Layout::vertical([Constraint::Length(6), Constraint::Min(0)]).areas(area);

    let cur = app.step.index();
    let items: Vec<ListItem> = Step::ALL
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            // ✓ done (revisitable) · ▸ current · dim = not yet reached.
            let (marker, style) = if s == app.step {
                (
                    "▸",
                    Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED),
                )
            } else if i < cur {
                ("✓", Style::new().fg(Color::Green))
            } else {
                (" ", Style::new().fg(Color::DarkGray))
            };
            ListItem::new(Line::styled(
                format!("{marker} {}. {}", i + 1, s.title()),
                style,
            ))
        })
        .collect();
    frame.render_widget(
        List::new(items).block(Block::bordered().title(" steps ")),
        list_area,
    );

    render_steps_legend(frame, legend_area, app);
}

/// The left-column legend: how to move, and whether the current step lets you
/// advance yet. Answers "how do I go back / when can I go on / which is next".
fn render_steps_legend(frame: &mut Frame, area: Rect, app: &App) {
    let dim = Style::new().fg(Color::DarkGray);
    let mut lines = vec![
        Line::styled("Tab   next", dim),
        Line::styled("⇧Tab  back", dim),
        Line::from(""),
    ];
    match (app.forward_open(), app.next_step()) {
        (true, Some(next)) => lines.push(Line::styled(
            format!("→ {}", next.title()),
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        )),
        (false, Some(_)) => {
            let why = match app.step {
                Step::Configure => "fill required",
                Step::SdCard => "write card first",
                _ => "finish this step",
            };
            lines.push(Line::styled(why, Style::new().fg(Color::Yellow)));
        }
        (_, None) => lines.push(Line::styled("all done", Style::new().fg(Color::Green))),
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .block(Block::bordered().title(" move "))
            .wrap(Wrap { trim: false }),
        area,
    );
}

fn render_main(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::bordered().title(format!(" {} ", app.step.title()));
    match app.step {
        Step::Preflight => render_preflight(frame, area, app, block),
        Step::Configure => render_configure(frame, area, app, block),
        Step::SdCard => render_sdcard(frame, area, app, block),
        Step::Done => render_done(frame, area, block),
    }
}

fn render_preflight(frame: &mut Frame, area: Rect, app: &App, block: Block) {
    // Startup / re-check: the diskutil scan runs off-thread — show the spinner
    // rather than an empty (or stale) check list.
    if app.busy == Busy::Preflight {
        let lines = vec![
            busy_line(app, app.busy.label().unwrap_or("Working…")),
            Line::from(""),
            Line::styled(
                "Scanning removable disks and checking git.",
                Style::new().fg(Color::DarkGray),
            ),
        ];
        frame.render_widget(paragraph(lines, block), area);
        return;
    }
    let mut lines = vec![
        Line::styled(
            "Checking your Mac and the card.",
            Style::new().fg(Color::DarkGray),
        ),
        Line::from(""),
    ];
    for c in &app.preflight.checks {
        let (glyph, color) = status_glyph(c.status);
        lines.push(Line::from(vec![
            Span::styled(
                format!(" {glyph} "),
                Style::new().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<16}", c.label),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(c.detail.clone(), Style::new().fg(Color::DarkGray)),
        ]));
    }
    lines.push(Line::from(""));
    lines.push(if app.preflight.ready() {
        Line::styled(
            "Ready. Press Enter to continue.",
            Style::new().fg(Color::Green),
        )
    } else {
        Line::styled(
            "Fix the ✗ items, then press r to re-check.",
            Style::new().fg(Color::Yellow),
        )
    });
    frame.render_widget(paragraph(lines, block), area);
}

fn render_configure(frame: &mut Frame, area: Rect, app: &App, block: Block) {
    let mut lines: Vec<Line> = vec![
        Line::styled(
            "Pre-filled from this Mac where possible. Type to edit · Tab / ↑↓ move · Enter next.",
            Style::new().fg(Color::DarkGray),
        ),
        Line::from(""),
    ];

    for (i, &f) in Field::ALL.iter().enumerate() {
        let focused = i == app.focus;
        let val = app.config.get(f);
        let empty = val.trim().is_empty();
        let shown: String = if f.secret() && !empty {
            "•".repeat(val.chars().count())
        } else {
            val.to_string()
        };

        let mut spans = vec![
            Span::styled(
                if focused { "▸ " } else { "  " },
                Style::new().add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!("{:<22}", f.label()),
                if focused {
                    Style::new().add_modifier(Modifier::BOLD)
                } else {
                    Style::new()
                },
            ),
        ];
        if focused {
            spans.push(Span::raw(shown));
            spans.push(Span::styled(
                " ",
                Style::new().add_modifier(Modifier::REVERSED),
            )); // block caret
        } else if empty {
            let (text, color) = if f.required() {
                ("(required)", Color::Yellow)
            } else {
                ("(optional)", Color::DarkGray)
            };
            spans.push(Span::styled(text, Style::new().fg(color)));
        } else {
            spans.push(Span::raw(shown));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));
    if app.busy == Busy::Keychain {
        lines.push(busy_line(app, app.busy.label().unwrap_or("Working…")));
    } else if let Some(msg) = &app.status {
        lines.push(Line::styled(msg.clone(), Style::new().fg(Color::Cyan)));
    } else {
        let missing = app.config.missing_required();
        if missing.is_empty() {
            lines.push(Line::styled(
                "All required fields set — Enter on the last field goes to the SD-card step.",
                Style::new().fg(Color::Green),
            ));
        } else {
            let names: Vec<&str> = missing.iter().map(|f| f.label()).collect();
            lines.push(Line::styled(
                format!("Required still empty: {}", names.join(", ")),
                Style::new().fg(Color::Yellow),
            ));
        }
    }
    if app.focused_field() == Field::WifiSsid && app.config.wifi_ssid_guessed {
        lines.push(Line::styled(
            "Best guess — macOS hides the active network; confirm it's the one Typoena will use.",
            Style::new().fg(Color::Yellow),
        ));
    } else if let Some(hint) = field_hint(app.focused_field()) {
        lines.push(Line::styled(hint, Style::new().fg(Color::DarkGray)));
    }

    frame.render_widget(paragraph(lines, block), area);
}

fn render_sdcard(frame: &mut Frame, area: Rect, app: &App, block: Block) {
    let dim = |s: String| Line::styled(s, Style::new().fg(Color::DarkGray));
    let mut lines: Vec<Line> = Vec::new();
    match &app.sd {
        SdState::ConfirmWipe(info) => {
            let vol = app
                .cards
                .get(app.card_sel.min(app.cards.len().saturating_sub(1)))
                .map(|c| c.volume.display().to_string())
                .unwrap_or_default();
            lines.push(Line::styled(
                format!("⚠  {vol} already holds a Typoena card."),
                Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::from(""));
            if let Some(o) = &info.origin {
                lines.push(dim(format!("  origin: {o}")));
            }
            if let Some(h) = &info.head {
                lines.push(dim(format!("  HEAD:   {h}")));
            }
            if info.dirty > 0 {
                lines.push(Line::styled(
                    format!(
                        "  {} unpublished edit(s) will be LOST (not yet published from the device).",
                        info.dirty
                    ),
                    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "This ERASES repo/ (and the dirty journal) and re-clones {} onto the card.",
                app.config.remote_url
            )));
            lines.push(Line::from(""));
            lines.push(Line::styled(
                "Press y to wipe and continue · n to cancel.",
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
        }
        SdState::Idle if app.busy == Busy::DetectingCards || app.busy == Busy::PreparingCard => {
            lines.push(busy_line(app, app.busy.label().unwrap_or("Working…")));
        }
        SdState::Idle => {
            if app.cards.is_empty() {
                lines.push(Line::styled(
                    "No removable card detected.",
                    Style::new().fg(Color::Yellow),
                ));
                lines.push(Line::from(""));
                lines.push(dim("Insert a FAT32 SD card, then press r to rescan.".into()));
            } else {
                lines.push(dim("Choose the card to write (↑/↓), then Enter:".into()));
                lines.push(Line::from(""));
                for (i, c) in app.cards.iter().enumerate() {
                    let sel = i == app.card_sel;
                    let marker = if sel { "▸ " } else { "  " };
                    let warn = if c.fat {
                        String::new()
                    } else {
                        "  (not FAT32 — the device may not mount it)".to_string()
                    };
                    let style = if sel {
                        Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED)
                    } else {
                        Style::new()
                    };
                    lines.push(Line::styled(
                        format!("{marker}{}  [{}]{}", c.name, c.fs, warn),
                        style,
                    ));
                }
                lines.push(Line::from(""));
                if app.config.missing_required().is_empty() {
                    lines.push(Line::styled(
                        "Enter writes the card: clone → seed config → typoena.conf → eject.",
                        Style::new().fg(Color::Green),
                    ));
                } else {
                    lines.push(Line::styled(
                        "Configure the required fields first (↑ to go back a step).",
                        Style::new().fg(Color::Yellow),
                    ));
                }
            }
            if let Some(msg) = &app.status {
                lines.push(Line::from(""));
                lines.push(Line::styled(msg.clone(), Style::new().fg(Color::Cyan)));
            }
        }
        // Running / Done / Failed render with a live progress gauge, so they
        // take their own path rather than the plain-paragraph one below.
        _ => {
            render_sd_progress(frame, area, app, block);
            return;
        }
    }
    frame.render_widget(paragraph(lines, block), area);
}

/// The provision view: a status line, a git-progress gauge while cloning, and a
/// tail of the worker log.
fn render_sd_progress(frame: &mut Frame, area: Rect, app: &App, block: Block) {
    let status = match &app.sd {
        SdState::Failed(e) => Line::styled(format!("Failed: {e}"), Style::new().fg(Color::Red)),
        SdState::Done => Line::styled(
            "Card ready ✓ — remove it and insert into Typoena.",
            Style::new().fg(Color::Green),
        ),
        _ => Line::styled(
            "Provisioning the card…  (Ctrl-C aborts)",
            Style::new().fg(Color::Yellow),
        ),
    };

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // The gauge only appears while cloning (the one phase with real percentages);
    // seed/conf/eject are near-instant and just scroll past in the log.
    match (&app.sd, &app.sd_progress) {
        (SdState::Running, Some((phase, pct))) => {
            let [top, gauge, log] = Layout::vertical([
                Constraint::Length(2),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .areas(inner);
            frame.render_widget(Paragraph::new(status), top);
            frame.render_widget(
                Gauge::default()
                    .gauge_style(Style::new().fg(Color::Green))
                    .ratio((*pct as f64 / 100.0).clamp(0.0, 1.0))
                    .label(format!("{phase}  {pct}%")),
                gauge,
            );
            render_sd_log(frame, log, app);
        }
        _ => {
            let [top, log] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(inner);
            frame.render_widget(Paragraph::new(status), top);
            render_sd_log(frame, log, app);
        }
    }
}

fn render_sd_log(frame: &mut Frame, area: Rect, app: &App) {
    let start = app.sd_log.len().saturating_sub(area.height as usize);
    let lines: Vec<Line> = app.sd_log[start..]
        .iter()
        .map(|l| Line::styled(l.clone(), Style::new().fg(Color::DarkGray)))
        .collect();
    frame.render_widget(
        Paragraph::new(Text::from(lines)).wrap(Wrap { trim: false }),
        area,
    );
}

fn render_done(frame: &mut Frame, area: Rect, block: Block) {
    let lines = vec![
        Line::styled(
            "All set.",
            Style::new().fg(Color::Green).add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from("Remove the card and insert it into your Typoena, then power on."),
        Line::styled(
            "Open lid → write → push. Nothing else runs on it.",
            Style::new().fg(Color::DarkGray),
        ),
    ];
    frame.render_widget(paragraph(lines, block), area);
}

fn field_hint(f: Field) -> Option<&'static str> {
    match f {
        Field::Pat => Some(
            "Fine-grained PAT, contents:write on the notes repo. Never derived; masked. ^U clears.",
        ),
        Field::WifiPass => {
            Some("^K fills this from your Keychain for the current SSID (may prompt macOS).")
        }
        Field::RemoteUrl => {
            Some("HTTPS URL of your notes repo, e.g. https://github.com/you/notes.git")
        }
        _ => None,
    }
}

fn status_glyph(s: Status) -> (&'static str, Color) {
    match s {
        Status::Ok => ("✓", Color::Green),
        Status::Warn => ("!", Color::Yellow),
        Status::Missing => ("✗", Color::Red),
    }
}

fn paragraph<'a>(lines: Vec<Line<'a>>, block: Block<'a>) -> Paragraph<'a> {
    Paragraph::new(Text::from(lines))
        .block(block)
        .wrap(Wrap { trim: false })
}

fn render_footer(frame: &mut Frame, area: Rect, app: &App) {
    let key = |k: &str| {
        Span::styled(
            format!(" {k} "),
            Style::new().add_modifier(Modifier::REVERSED),
        )
    };
    let lbl = |l: &'static str| Span::styled(l, Style::new().fg(Color::DarkGray));
    let sep = || Span::raw("  ");

    let spans = if matches!(app.sd, SdState::ConfirmWipe(_)) && app.step == Step::SdCard {
        vec![
            key("y"),
            lbl(" wipe & continue"),
            sep(),
            key("n"),
            lbl(" cancel"),
            sep(),
            key("^C"),
            lbl(" quit"),
        ]
    } else {
        match app.step {
            Step::Configure => vec![
                key("Tab"),
                lbl(" field / next"),
                sep(),
                key("^K"),
                lbl(" wifi pw"),
                sep(),
                key("^U"),
                lbl(" clear"),
                sep(),
                key("Esc"),
                lbl(" quit"),
            ],
            Step::SdCard => vec![
                key("↑↓ / j k"),
                lbl(" card"),
                sep(),
                key("r"),
                lbl(" rescan"),
                sep(),
                key("Enter"),
                lbl(" write"),
                sep(),
                key("q"),
                lbl(" quit"),
            ],
            _ => vec![
                key("Tab"),
                lbl(" next"),
                sep(),
                key("⇧Tab"),
                lbl(" back"),
                sep(),
                key("r"),
                lbl(" re-check"),
                sep(),
                key("q"),
                lbl(" quit"),
            ],
        }
    };
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::SdState;
    use crate::sdcard::{Card, CardInspect};
    use ratatui::{Terminal, backend::TestBackend};

    /// Render `app` to an off-screen terminal and return the visible text.
    fn screen(app: &App) -> String {
        let mut terminal = Terminal::new(TestBackend::new(90, 30)).unwrap();
        terminal.draw(|f| render(f, app)).unwrap();
        terminal.backend().to_string()
    }

    #[test]
    fn every_step_renders_without_panicking() {
        let mut app = App::new();
        for step in Step::ALL {
            app.step = step;
            let _ = screen(&app); // a layout-array or index panic would fail here
        }
    }

    #[test]
    fn header_types_the_name_then_shows_the_tagline() {
        use std::time::{Duration, Instant};
        let mut app = App::new();
        // Wind the clock back past the intro so it renders fully typed.
        app.started = Instant::now()
            .checked_sub(Duration::from_secs(3))
            .unwrap_or_else(Instant::now);
        let s = screen(&app);
        assert!(s.contains("typoena"), "the name should have typed in:\n{s}");
        assert!(
            s.contains("distraction-free"),
            "the tagline should show once the name is done:\n{s}"
        );
    }

    #[test]
    fn sidebar_shows_progress_and_movement() {
        let mut app = App::new();
        app.step = Step::SdCard; // Preflight + Configure are now behind us
        let s = screen(&app);
        assert!(s.contains('✓'), "completed steps should be ticked:\n{s}");
        assert!(s.contains("Tab"), "movement legend should name Tab:\n{s}");
        assert!(
            s.contains("back"),
            "legend should show how to go back:\n{s}"
        );
    }

    #[test]
    fn guessed_ssid_is_flagged_on_the_wifi_field() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.focus = 0; // Wi-Fi SSID
        app.config.wifi_ssid = "SomeNet".into();
        app.config.wifi_ssid_guessed = true;
        assert!(
            screen(&app).contains("Best guess"),
            "a guessed SSID must be flagged so the user confirms it"
        );
    }

    #[test]
    fn detecting_cards_shows_a_loading_caption() {
        let mut app = App::new();
        app.step = Step::SdCard;
        app.busy = Busy::DetectingCards;
        let s = screen(&app);
        assert!(
            s.contains("Scanning"),
            "the SD step should show a loading caption while detecting cards:\n{s}"
        );
    }

    #[test]
    fn clone_progress_drives_a_gauge() {
        let mut app = App::new();
        app.step = Step::SdCard;
        app.sd = SdState::Running;
        app.sd_progress = Some(("Receiving objects".into(), 42));
        let s = screen(&app);
        assert!(
            s.contains("42%"),
            "the gauge should show the clone percent:\n{s}"
        );
    }

    #[test]
    fn wipe_confirmation_warns_before_erasing() {
        let mut app = App::new();
        app.step = Step::SdCard;
        app.cards = vec![Card {
            volume: "/Volumes/TYPOENA".into(),
            name: "TYPOENA".into(),
            fs: "MS-DOS FAT32".into(),
            fat: true,
        }];
        app.card_sel = 0;
        app.sd = SdState::ConfirmWipe(CardInspect {
            origin: Some("https://github.com/you/notes.git".into()),
            head: Some("abc1234".into()),
            dirty: 2,
        });
        let s = screen(&app);
        assert!(
            s.contains("already holds"),
            "must warn the card is in use:\n{s}"
        );
        assert!(
            s.contains("unpublished"),
            "must flag unpublished edits:\n{s}"
        );
    }
}
