//! Rendering. Theme-agnostic: no hard-coded background (the terminal's own),
//! bold/reverse for emphasis, and the conventional green/yellow/red for status
//! so it reads on any color scheme.

use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Gauge, List, ListItem, Paragraph, Wrap},
};

use crate::app::{App, AuthState, Busy, RepoCheck, SdState, Step};
use crate::config::Field;
use crate::preflight::Status;

/// One frame of the braille spinner for the given tick.
pub(crate) fn spinner(tick: u64) -> char {
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
        Constraint::Length(EINK_BOX_H + 2), // device-screen box + 1 row margin each side
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
/// Tagline, lifted verbatim from typoena.dev so the two read as one product.
/// One caret types the name, then continues into this.
const TAGLINE: &str = "A distraction-free writing machine.";
/// Milliseconds between revealed letters — comfortably above the 100 ms render
/// tick so the typewriter reads as a deliberate, unhurried keystroke rhythm.
const KEY_MS: u128 = 150;
/// How long the caret blinks after both lines are typed, before it settles.
const BLINK_MS: u128 = 10_000;

/// The device's e-ink panel is GDEY0579T93, 792×272 px — a landscape strip of
/// ~2.9:1. A monospace cell is roughly twice as tall as it is wide, so to
/// reproduce that shape on screen the box's cols:rows must be ~2× wider, ≈5.8:1.
/// 41×7 lands at ~2.93:1 and leaves the 35-char tagline room to breathe.
const EINK_BOX_W: u16 = 41;
const EINK_BOX_H: u16 = 7;

/// One centred line of the header: `text[..shown]` in `style`, a caret cell at
/// the cursor position (reverse-video when `caret` is lit), then padding out to
/// the full text width so the centred line never shifts as it fills.
fn typed_line(text: &str, shown: usize, style: Style, caret: bool) -> Line<'static> {
    let mut spans = vec![
        Span::styled(text[..shown].to_string(), style), // text is ASCII: byte == char
        if caret {
            Span::styled(" ", Style::new().add_modifier(Modifier::REVERSED))
        } else {
            Span::raw(" ")
        },
    ];
    if shown < text.len() {
        spans.push(Span::raw(" ".repeat(text.len() - shown)));
    }
    Line::from(spans)
}

/// A centred brand header with a blank line of margin above and below. A single
/// caret types the name, then continues down into the tagline; once both lines
/// are written it blinks for `BLINK_MS` (the machine, waiting for you) and then
/// settles so the header goes quiet.
fn render_header(frame: &mut Frame, area: Rect, app: &App) {
    let elapsed = app.started.elapsed().as_millis();

    // One running counter drives both lines: the first NAME.len() keys fill the
    // name, the rest spill into the tagline.
    let typed = (elapsed / KEY_MS) as usize;
    let name_shown = typed.min(NAME.len());
    let tag_shown = typed.saturating_sub(NAME.len()).min(TAGLINE.len());
    let type_done = (NAME.len() + TAGLINE.len()) as u128 * KEY_MS;

    // Caret: solid while typing, then a ~530 ms blink for BLINK_MS, then gone.
    let lit = if elapsed < type_done {
        true
    } else if elapsed < type_done + BLINK_MS {
        (elapsed / 530).is_multiple_of(2)
    } else {
        false
    };
    // It sits on whichever line is still being written (the tagline once the
    // name is complete), so there's only ever one caret on screen.
    let on_name = typed < NAME.len();

    // Center a panel-proportioned box in the header band. Clamp to the band so a
    // narrow terminal frames a smaller strip rather than overflowing.
    let w = EINK_BOX_W.min(area.width);
    let h = EINK_BOX_H.min(area.height);
    let screen = Rect {
        x: area.x + area.width.saturating_sub(w) / 2,
        y: area.y + area.height.saturating_sub(h) / 2,
        width: w,
        height: h,
    };
    // The bezel: a rounded frame, softer than the plain-bordered panels below,
    // so the header reads as the device's screen rather than another pane.
    let bezel = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(Color::DarkGray));
    let inner = bezel.inner(screen);
    frame.render_widget(bezel, screen);

    // The name and tagline, vertically centered inside the screen — the two
    // lines the device itself would show, typed out one caret at a time.
    let content = vec![
        typed_line(
            NAME,
            name_shown,
            Style::new().add_modifier(Modifier::BOLD),
            lit && on_name,
        ),
        Line::from(""),
        typed_line(
            TAGLINE,
            tag_shown,
            Style::new().fg(Color::DarkGray),
            lit && !on_name,
        ),
    ];
    let top_pad = (inner.height as usize).saturating_sub(content.len()) / 2;
    let mut lines = vec![Line::from(""); top_pad];
    lines.extend(content);
    frame.render_widget(
        Paragraph::new(Text::from(lines)).alignment(Alignment::Center),
        inner,
    );
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
        Line::styled("^N/^P step", dim),
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
    // The GitHub sign-in takes over the step while it runs (it's modal — the
    // form comes back when the flow ends or is cancelled).
    match &app.auth {
        AuthState::Starting => {
            let lines = vec![
                busy_line(app, "Contacting GitHub…"),
                Line::from(""),
                Line::styled(
                    "Requesting a one-time sign-in code.",
                    Style::new().fg(Color::DarkGray),
                ),
            ];
            frame.render_widget(paragraph(lines, block), area);
            return;
        }
        AuthState::Waiting {
            user_code,
            verification_uri,
        } => {
            render_auth_waiting(frame, area, app, block, user_code, verification_uri);
            return;
        }
        AuthState::Idle => {}
    }
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
    // The repo-access verdict, shown only while it matches the current remote
    // (editing the field retires a stale flag). Missing is the big one: every
    // first-time ^G user needs the app installed on their repo.
    match &app.repo_check {
        RepoCheck::Checking { remote } if *remote == app.config.remote() => {
            lines.push(busy_line(app, "Checking repo access…"));
        }
        RepoCheck::Granted { remote } if *remote == app.config.remote() => {
            lines.push(Line::styled(
                format!("✓ your token can access {remote}"),
                Style::new().fg(Color::Green),
            ));
        }
        RepoCheck::Missing { remote } if *remote == app.config.remote() => {
            lines.push(Line::styled(
                format!("⚠ your token can't see {remote} yet."),
                Style::new().fg(Color::Yellow).add_modifier(Modifier::BOLD),
            ));
            lines.push(Line::styled(
                "Signed in with ^G? The Typoena app must be installed on that repo — \
                 ^O opens the page (this turns green once granted). \
                 Pasted a PAT? Give it contents:write there.",
                Style::new().fg(Color::Yellow),
            ));
        }
        _ => {}
    }
    if app.focused_field() == Field::WifiSsid && app.config.wifi_ssid_guessed {
        lines.push(Line::styled(
            "Best guess — macOS hides the active network; confirm it's the one Typoena will use.",
            Style::new().fg(Color::Yellow),
        ));
    } else if app.focused_field() == Field::RemoteUrl
        && !app.config.remote_url.trim().is_empty()
        && app.config.remote() != app.config.remote_url.trim()
    {
        // Shorthand in play — show live what it expands to, so there's never a
        // surprise about what actually lands on the card.
        lines.push(Line::styled(
            format!("→ will use {}", app.config.remote()),
            Style::new().fg(Color::Green),
        ));
    } else if let Some(hint) = field_hint(app.focused_field()) {
        lines.push(Line::styled(hint, Style::new().fg(Color::DarkGray)));
    }

    frame.render_widget(paragraph(lines, block), area);
}

/// The "enter this code on GitHub" screen shown while the device flow polls.
/// The code is the one thing the user must carry to the browser, so it gets a
/// big reversed-video cell of its own.
fn render_auth_waiting(
    frame: &mut Frame,
    area: Rect,
    app: &App,
    block: Block,
    user_code: &str,
    verification_uri: &str,
) {
    let dim = Style::new().fg(Color::DarkGray);
    let lines = vec![
        Line::styled(
            "Sign in with GitHub",
            Style::new().add_modifier(Modifier::BOLD),
        ),
        Line::from(""),
        Line::from(vec![
            Span::styled("  1. Your browser opened  ", dim),
            Span::raw(verification_uri.to_string()),
            Span::styled("  (o reopens it)", dim),
        ]),
        Line::styled("  2. Enter this code:", dim),
        Line::from(""),
        Line::from(vec![
            Span::raw("        "),
            Span::styled(
                format!(" {user_code} "),
                Style::new().add_modifier(Modifier::BOLD | Modifier::REVERSED),
            ),
        ]),
        Line::from(""),
        Line::styled("  3. Authorize the Typoena app.", dim),
        Line::from(""),
        busy_line(app, "Waiting for you to authorize on GitHub…"),
        Line::from(""),
        Line::styled(
            "Esc cancels — you can still paste a PAT by hand instead.",
            Style::new().fg(Color::DarkGray),
        ),
    ];
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
                        "  {} unpushed edit(s) will be LOST (not yet pushed from the device).",
                        info.dirty
                    ),
                    Style::new().fg(Color::Red).add_modifier(Modifier::BOLD),
                ));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(format!(
                "This ERASES repo/ (and the dirty journal) and re-clones {} onto the card.",
                app.config.remote()
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
            "^G signs in with GitHub and fills this — or paste a fine-grained PAT \
             (contents:write on the notes repo). Masked. ^U clears.",
        ),
        Field::WifiPass => {
            Some("^K fills this from your Keychain for the current SSID (may prompt macOS).")
        }
        Field::RemoteUrl => Some(
            "Your notes repo — just the name is enough (notes → \
             https://github.com/<you>/notes.git); you/notes, full URLs, and \
             other hosts work too.",
        ),
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
            Step::Configure if !matches!(app.auth, AuthState::Idle) => vec![
                key("o"),
                lbl(" reopen browser"),
                sep(),
                key("Esc"),
                lbl(" cancel sign-in"),
                sep(),
                key("^C"),
                lbl(" quit"),
            ],
            Step::Configure => vec![
                key("Tab"),
                lbl(" field / next"),
                sep(),
                key("^G"),
                lbl(" sign in"),
                sep(),
                key("^O"),
                lbl(" install app"),
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
        // Wind the clock back past the whole intro (type + 10 s blink) so it
        // renders in its final, fully-typed, settled state.
        app.started = Instant::now()
            .checked_sub(Duration::from_secs(20))
            .unwrap_or_else(Instant::now);
        let s = screen(&app);
        assert!(s.contains("typoena"), "the name should have typed in:\n{s}");
        assert!(
            s.contains(TAGLINE),
            "the single caret should have typed the whole tagline too:\n{s}"
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
    fn sign_in_panel_shows_the_user_code() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.auth = crate::app::AuthState::Waiting {
            user_code: "WDJB-MJHT".into(),
            verification_uri: "https://github.com/login/device".into(),
        };
        let s = screen(&app);
        assert!(
            s.contains("WDJB-MJHT"),
            "the code the user must type is the whole point:\n{s}"
        );
        assert!(
            s.contains("github.com/login/device"),
            "the URL must be visible in case the browser didn't open:\n{s}"
        );
        assert!(s.contains("Esc"), "the escape hatch must be shown:\n{s}");
    }

    #[test]
    fn configure_footer_offers_the_sign_in_key() {
        let mut app = App::new();
        app.step = Step::Configure;
        let s = screen(&app);
        assert!(s.contains("^G"), "footer should advertise sign-in:\n{s}");
    }

    #[test]
    fn missing_repo_access_is_flagged_with_the_fix() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.config.remote_url = "you/notes".into();
        app.repo_check = crate::app::RepoCheck::Missing {
            remote: app.config.remote(),
        };
        let s = screen(&app);
        assert!(
            s.contains("can't see"),
            "the not-installed case must be flagged:\n{s}"
        );
        assert!(
            s.contains("^O"),
            "the flag must name the key that fixes it:\n{s}"
        );
    }

    #[test]
    fn granted_repo_access_shows_green_reassurance() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.config.remote_url = "you/notes".into();
        app.repo_check = crate::app::RepoCheck::Granted {
            remote: app.config.remote(),
        };
        let s = screen(&app);
        assert!(s.contains("can access"), "granted must be visible:\n{s}");
    }

    #[test]
    fn stale_access_flags_are_not_rendered() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.config.remote_url = "you/other".into();
        app.repo_check = crate::app::RepoCheck::Missing {
            remote: "https://github.com/you/notes.git".into(),
        };
        let s = screen(&app);
        assert!(
            !s.contains("can't see"),
            "a verdict about an old remote must not label the new one:\n{s}"
        );
    }

    #[test]
    fn remote_shorthand_shows_its_expansion_live() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.focus = Field::ALL
            .iter()
            .position(|&f| f == Field::RemoteUrl)
            .unwrap();
        app.config.remote_url = "you/notes".into();
        let s = screen(&app);
        assert!(
            s.contains("https://github.com/you/notes.git"),
            "the user must see what the shorthand becomes:\n{s}"
        );
    }

    #[test]
    fn bare_repo_name_shows_its_user_completion_live() {
        let mut app = App::new();
        app.step = Step::Configure;
        app.focus = Field::ALL
            .iter()
            .position(|&f| f == Field::RemoteUrl)
            .unwrap();
        app.config.gh_user = "you".into();
        app.config.remote_url = "notes".into();
        let s = screen(&app);
        assert!(
            s.contains("https://github.com/you/notes.git"),
            "a bare name must show the username it completes against:\n{s}"
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
            s.contains("unpushed"),
            "must flag unpushed edits:\n{s}"
        );
    }
}
