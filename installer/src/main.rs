mod app;
mod auth;
mod config;
mod preflight;
mod sdcard;
mod ui;
mod wipe;

use std::path::PathBuf;
use std::time::Duration;

use app::App;
use config::{Config, Field};
use preflight::{Preflight, Status};
use ratatui::crossterm::event::{self, Event, KeyEventKind};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().collect();

    // Headless preflight + derived config (scriptable, no TTY needed).
    if args.iter().any(|a| a == "--check") {
        return run_check();
    }
    // Read-only: list the removable cards the SD step would offer.
    if args.iter().any(|a| a == "--list-cards") {
        return list_cards();
    }
    // Full-card reformat TUI: pick a card, confirm, erase to blank FAT32, eject.
    // Usage: --wipe [volume] [--label NAME]   (backs the firmware `just wipe`).
    if args.iter().any(|a| a == "--wipe") {
        let mut label = "TYPOENA".to_string();
        let mut volume: Option<String> = None;
        let mut it = args.iter().skip(1);
        while let Some(a) = it.next() {
            match a.as_str() {
                "--label" => {
                    if let Some(v) = it.next() {
                        label = v.clone();
                    }
                }
                s if s.starts_with("--") => {} // --wipe and any other flags
                s if volume.is_none() => volume = Some(s.to_string()),
                _ => {}
            }
        }
        return wipe::run(volume, label);
    }
    // Verify the (optionally wipe +) clone + config-write path without a card
    // (clones to a temp dir, no eject).
    // Usage: --dry-run-sd <remote-url> [dest-dir] [--wipe]
    if args.iter().any(|a| a == "--dry-run-sd") {
        let wipe = args.iter().any(|a| a == "--wipe");
        // positional args (flags stripped): [0] = remote, [1] = optional dest
        let positionals: Vec<String> = args
            .iter()
            .skip(1)
            .filter(|a| !a.starts_with("--"))
            .cloned()
            .collect();
        let remote = config::expand_remote_url(&positionals.first().cloned().unwrap_or_default());
        if remote.is_empty() {
            anyhow::bail!("usage: --dry-run-sd <remote-url> [dest-dir] [--wipe]");
        }
        let dest = positionals
            .get(1)
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::temp_dir().join("typoena-dryrun"));
        println!(
            "dry-run SD provision{}: clone {remote} → {}/repo",
            if wipe { " (wipe first)" } else { "" },
            dest.display()
        );
        return sdcard::dry_run(&remote, &dest, wipe);
    }

    let mut terminal = ratatui::init();
    let result = run(&mut terminal);
    ratatui::restore();
    result
}

fn run(terminal: &mut ratatui::DefaultTerminal) -> anyhow::Result<()> {
    let mut app = App::new();
    // Kick the first environment scan off the UI thread so the window paints
    // immediately instead of blocking on the diskutil scan.
    app.begin_startup();
    while !app.should_quit {
        app.poll_background();
        // Bump the frame counter so the spinner animates while work runs.
        app.tick = app.tick.wrapping_add(1);
        terminal.draw(|frame| ui::render(frame, &app))?;
        // Poll so worker progress / spinner can repaint even without a keypress.
        if event::poll(Duration::from_millis(100))?
            && let Event::Key(key) = event::read()?
            && key.kind == KeyEventKind::Press
        {
            app.on_key(key);
        }
    }
    Ok(())
}

fn run_check() -> anyhow::Result<()> {
    let pf = Preflight::run();
    for c in &pf.checks {
        let tag = match c.status {
            Status::Ok => "OK  ",
            Status::Warn => "WARN",
            Status::Missing => "MISS",
        };
        println!("[{tag}] {:<16} {}", c.label, c.detail);
    }
    println!("ready: {}", pf.ready());

    println!("--- derived config (secrets hidden) ---");
    let cfg = Config::derived();
    for f in Field::ALL {
        let v = cfg.get(f);
        let shown = if f.secret() {
            if v.is_empty() { "(unset)" } else { "(set)" }
        } else if v.is_empty() {
            "(unset)"
        } else {
            v
        };
        println!("  {:<22} {}", f.label(), shown);
    }
    Ok(())
}

fn list_cards() -> anyhow::Result<()> {
    let cards = sdcard::detect_cards();
    if cards.is_empty() {
        println!("no removable card detected under /Volumes");
        return Ok(());
    }
    for c in cards {
        let fat = if c.fat {
            "FAT"
        } else {
            "NOT FAT — device may not mount"
        };
        println!("{}  [{}] ({})", c.name, c.fs, fat);
        println!("  {}", c.volume.display());
    }
    Ok(())
}
