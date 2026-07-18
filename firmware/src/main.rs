#[cfg(feature = "git")]
mod wizard_io;

use std::rc::Rc;

use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::gpio::{AnyIOPin, PinDriver, Pull};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::config::{Config, DriverConfig};
use esp_idf_svc::hal::spi::{Dma, SpiBusDriver, SpiDriver};
use esp_idf_svc::hal::units::FromValueType;

use app::{file_stem, FileIndex, Panel, Runtime};
use display::Frame;
use editor::{Editor, Prefs, Scope, Snippets, LOCAL_DIR, PREFS_PATH, SNIPPETS_PATH};
use firmware::adapters::{EspClock, EspFileWalk, SdStorage};
#[cfg(feature = "git")]
use firmware::adapters::{EspSystem, GitSyncService};
#[cfg(not(feature = "git"))]
use firmware::adapters::{NullSyncService, NullSystem};
use firmware::epd::Epd;
use firmware::persistence::{Storage, CONF_PATH, NOTES};
use firmware::usb_kbd;

/// Injected by build.rs so serial output identifies the exact build.
const BUILD_TAG: &str = concat!("build ", env!("BUILD_TIME"), " @", env!("BUILD_GIT"));

fn main() -> anyhow::Result<()> {
    // Required once before any esp-idf-svc call; some runtime patches
    // only link if this symbol is referenced. See esp-idf-template#71.
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    log::info!("Typoena — modal editor (vim modes), {BUILD_TAG}");

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    // GDEY0579T93 on S3-safe GPIOs (Spike 2 wiring):
    //   SCK 12 · DIN/MOSI 11 · CS 7 · DC 6 · RST 5 · BUSY 4
    let spi = SpiDriver::new(
        peripherals.spi2,
        pins.gpio12,
        pins.gpio11,
        None::<AnyIOPin>,
        &DriverConfig::new().dma(Dma::Auto(4096)),
    )?;
    // EPD SPI clock. Was 4 MHz; the panel (SSD1683) takes 10–20 MHz, and this
    // clock only affects the pixel clock-out, not the waveform BUSY time — so it
    // trims the pre-kick band write (~43 ms full-area at 4 MHz) off perceived
    // latency on the erase/caret/scroll path. Sweep higher (16/20 MHz) only
    // while watching the panel for signal-integrity glitches (garbled/missing
    // bands). See docs/tradeoff-curves/epd-refresh-latency.md.
    let bus = SpiBusDriver::new(spi, &Config::new().baudrate(20.MHz().into()))?;
    let cs = PinDriver::output(pins.gpio7)?;
    let dc = PinDriver::output(pins.gpio6)?;
    let rst = PinDriver::output(pins.gpio5)?;
    let busy = PinDriver::input(pins.gpio4, Pull::Down)?;
    let mut epd = Epd::new(bus, dc, rst, cs, busy);

    log::info!("EPD reset + init…");
    epd.reset()?;
    epd.init()?;
    // Boot splash (Spike 9): the Typoena mark, kicked off *async* — the ~2.2 s
    // full-refresh waveform runs while the SD mounts and the note loads below,
    // so the splash starts painting as early as the app can drive it and its
    // wait overlaps the mandatory boot work instead of preceding it. Its full
    // refresh doubles as the baseline the old white clear used to establish
    // (writes both RAM banks); the first editor render further down implicitly
    // waits it out (`wait_ready`) and then replaces it.
    epd.display_frame_async(Frame::splash().bytes())?;

    // Mount the SD and load the saved note. We bring the SD up *after* the EPD —
    // the doc's boot order is SD-first, but a dead panel can't explain a missing
    // card — and treat a missing card / repo / unreadable note as fatal: a
    // writing appliance that silently started empty would clobber the note on
    // the next `:w`. See docs/v0.1-mvp-technical.md, boot sequence.
    let storage = boot_storage(&mut epd);

    // The light build has no wizard (it can't clone), so it keeps the old
    // no-repo halt; the git build's repo check happens in the wizard gate
    // below, where a missing repo *enters setup* instead of halting.
    #[cfg(not(feature = "git"))]
    if !storage.repo_present() {
        let _ = CONF_PATH; // conf is consumed by the git build only
        boot_halt(
            &mut epd,
            "No repo on the SD card",
            "Provision it on your computer (just init) and reboot.",
        );
    }

    // Bring up the USB keyboard in the background; keys arrive via next_key().
    // Before the wizard gate — first-boot setup types on this keyboard.
    usb_kbd::start()?;

    // Device runtime config + the first-boot wizard gate (v0.9 onboarding).
    // The card's typoena.conf overrides the .env-baked TW_* per field
    // (slice 0). If the effective config is incomplete or the repo is missing,
    // the wizard runs *instead of* the editor (slice 2) and hands back the
    // completed conf; either way the result is installed before the git
    // thread spawns. Secrets stay out of the log — only which keys exist.
    #[cfg(feature = "git")]
    let (sys_loop, nvs, modem) = {
        use esp_idf_svc::eventloop::EspSystemEventLoop;
        use esp_idf_svc::nvs::EspDefaultNvsPartition;

        let sys_loop = EspSystemEventLoop::take()?;
        let nvs = EspDefaultNvsPartition::take()?;
        let mut modem = peripherals.modem;

        let card = match std::fs::read_to_string(CONF_PATH) {
            Ok(body) => conf::Conf::parse(&body),
            Err(_) => conf::Conf::default(),
        };
        let provided: Vec<&str> = conf::Field::ALL
            .iter()
            .filter(|f| !card.get(**f).trim().is_empty())
            .map(|f| f.conf_key())
            .collect();
        log::info!(
            "typoena.conf on card provides: {}",
            if provided.is_empty() { "nothing".into() } else { provided.join(", ") }
        );

        let effective = firmware::git_sync::effective_conf_from(&card);
        let unconfigured = !effective.missing_required().is_empty() || !storage.repo_present();
        // `:setup` reboots into the wizard prefilled (the running editor can't
        // reclaim the radio from the git thread). One-shot: clear the marker on
        // read so a power-pull mid-setup boots the editor, not setup again.
        let setup_requested = storage.setup_requested();
        if setup_requested {
            storage.clear_setup_request();
        }
        let final_conf = if unconfigured || setup_requested {
            if unconfigured {
                log::info!("unconfigured card (conf incomplete or repo missing) — entering the onboarding wizard");
            } else {
                log::info!(":setup requested — reopening the wizard prefilled from the card conf");
            }
            // The gate above asks "is the device usable?" (baked dev values can
            // answer yes). The wizard provisions the *card*, so it resumes from
            // the card's own state — never the baked fallback, which would skip
            // the very steps a blank card needs (and, on the author's device,
            // mask the whole flow by jumping straight to the repo step). `:setup`
            // (configured card, marker set) opens the reset menu instead.
            match wizard_io::run(&mut epd, &storage, card, setup_requested && !unconfigured, &sys_loop, &nvs, &mut modem) {
                Ok(c) => c,
                Err(e) => boot_halt(&mut epd, "Setup stopped", &format!("{e:#}")),
            }
        } else {
            card
        };
        firmware::git_sync::set_card_conf(final_conf);
        (sys_loop, nvs, modem)
    };

    // Editor preferences (.typoena.toml, git-tracked). Read before the boot
    // buffer is chosen (`open_last_on_boot` decides which file that is) and
    // before the first render (`line_numbers` shapes the opening frame). A
    // missing / unreadable / partial file falls back to defaults, so a fresh
    // card just works.
    let prefs = match storage.load_path(PREFS_PATH) {
        Ok(src) => Prefs::parse(&src),
        Err(_) => Prefs::default(),
    };
    log::info!("prefs: {prefs:?}");
    // Apply the configured timezone before anything reads the wall clock, so
    // `localtime_r` — and thus the `:inbox` note's dated name/title — reflects the
    // local calendar day. Empty (the default) leaves the ESP clock at UTC.
    if !prefs.timezone.is_empty() {
        apply_timezone(&prefs.timezone);
    }
    let (boot_path, boot_scope, saved) = boot_note(&mut epd, &storage, &prefs);

    // Spawn the dedicated git thread — the `:gp` publish transport. It owns
    // the Wi-Fi stack (brought up lazily on the first `:gp`, so the radio
    // stays off until you publish) and parks on `git_tx` until signalled; the
    // push runs off the UI loop, and its outcome returns on `git_rx` for the
    // snackbar. Behind the `git` feature so a light build carries no libgit2.
    #[cfg(feature = "git")]
    let (git_tx, git_rx) = {
        use firmware::git_sync::{run_git_service, GitOutcome, GitRequest, GIT_STACK};

        // sys_loop / nvs / modem come from the wizard-gate block above — the
        // wizard borrows the modem for its join test, then the git thread
        // owns all three for good.
        let (req_tx, req_rx) = std::sync::mpsc::channel::<GitRequest>();
        let (res_tx, res_rx) = std::sync::mpsc::channel::<GitOutcome>();
        std::thread::Builder::new()
            .name("git".into())
            .stack_size(GIT_STACK)
            .spawn(move || run_git_service(modem, sys_loop, nvs, req_rx, res_tx))?;
        log::info!(
            "git thread up ({} KB stack); Wi-Fi comes up on the first :gp",
            GIT_STACK / 1024
        );
        (req_tx, res_rx)
    };

    // Seed the editor from the boot note (`boot_note` above: the default
    // `/sd/repo/notes.md`, or the resumed last file when `open_last_on_boot`
    // is set). Boots in Normal mode with the caret on the last character (the
    // resume point) — press `i`/`a`/`o` to write.
    let mut ed = Editor::with_file(boot_path.clone(), boot_scope, saved);
    // Confirm the boot-load on the panel (no serial console in normal use):
    // "loaded <name>" using the note's filename without its suffix (notes.md ->
    // notes). Cleared by the first keystroke, like any snackbar.
    ed.set_notice(format!("loaded {}", file_stem(&boot_path)));
    ed.set_prefs(prefs);
    // Snippet library (.typoena.snippets.json, git-tracked). Parsed with
    // serde_json in the editor crate; a missing / unreadable / malformed file is
    // non-fatal — the editor simply has no snippets and runs unchanged.
    let snippets = match storage.load_path(SNIPPETS_PATH) {
        Ok(src) => match Snippets::parse(&src) {
            Ok(s) => s,
            Err(e) => {
                log::warn!("snippets parse FAILED ({e}); none loaded");
                Snippets::default()
            }
        },
        Err(_) => Snippets::default(),
    };
    log::info!("snippets: {} loaded", snippets.0.len());
    ed.set_snippets(snippets);

    // Keyboard attach/detach state drives the panel's disconnect flag; seed it
    // (and the word-count snapshot) before the first render. The loop's own
    // bookkeeping (last-file marker, idle-save window, focus timer) now lives in
    // the [`Runtime`].
    ed.set_keyboard_present(usb_kbd::keyboard_present());
    ed.refresh_stats();

    // First editor render — the moment the splash disappears. Everything
    // mandatory is ready here: SD mounted, note loaded, prefs applied, input
    // running (the palette walk continues in the background). `Panel::new` draws
    // the opening frame and paints it as a full-area *partial* (~630 ms) that
    // first waits out the splash's waveform (which the boot work above
    // overlapped), so the splash→editor swap rides the partial instead of a
    // second full refresh — shaving ~1.3 s off cold boot. From here the panel
    // owns the EPD and both reused framebuffers (a repaint never allocates — a
    // background `:gp` can take the heap to the floor); every repaint goes
    // through it. See [`app::Panel`].
    let panel = Panel::new(epd, &mut ed)?;

    // Boot-time measurement (the ≤ 5 s v0.1 / ≤ 3 s v1.0 target). Two clocks, and
    // they disagree by ~1.4 s here, so report both. `esp_log_timestamp()` counts
    // from ~power-on (same value as this line's own log prefix) → the real
    // cold-boot number. `esp_timer_get_time()` only starts ~1.4 s in, after the
    // 2nd-stage bootloader + the ~0.74 s PSRAM memtest, so it captures just the
    // app-side init, not total boot. "Cursor ready" = first editor frame on the
    // panel, input loop below about to poll.
    let total_ms = unsafe { esp_idf_svc::sys::esp_log_timestamp() };
    let app_ms = (unsafe { esp_idf_svc::sys::esp_timer_get_time() } / 1000) as u32;
    log::info!("boot: cursor ready — {total_ms} ms since power-on ({app_ms} ms app-side)");

    // The palette's background file index (Ctrl-P). Kick the first walk now —
    // AFTER the first editor frame is on the panel — so the seconds-long readdir
    // over SPI doesn't starve the boot-critical SD reads or delay the first
    // paint. The idle loop feeds finished walks to the editor (recents-only until
    // then); a pull re-walks the same way. Runs on the walk thread the port owns.
    let files = EspFileWalk::new();
    files.request_rewalk();

    // Share the mounted card across the storage / sync / system adapters — all on
    // this single UI task, so `Rc` (not `Arc`) is enough. The git build's sync +
    // system adapters reach the same dirty journal and setup marker through it.
    let card = Rc::new(storage);

    // Choose the sync + system adapters by build: a full build drives git; a
    // light editor build injects the no-op pair (publish/pull skipped, `:setup`
    // reports it needs the full firmware). The [`Runtime`] is identical either way.
    #[cfg(feature = "git")]
    let sync: Box<dyn app::SyncService> =
        Box::new(GitSyncService::new(card.clone(), git_tx, git_rx));
    #[cfg(not(feature = "git"))]
    let sync: Box<dyn app::SyncService> = Box::new(NullSyncService);
    #[cfg(feature = "git")]
    let system: Box<dyn app::System> = Box::new(EspSystem(card.clone()));
    #[cfg(not(feature = "git"))]
    let system: Box<dyn app::System> = Box::new(NullSystem);

    // Assemble the run loop and drive it forever. The editor is seeded, the panel
    // holds the first frame, and every hardware / infrastructure dependency is
    // now behind a port; the loop that used to live inline here is the
    // host-tested [`Runtime`]. The only exits are `:reboot`/`:setup`, which
    // restart the device — so `run` never returns.
    let mut runtime = Runtime::new(
        ed,
        panel,
        Box::new(usb_kbd::UsbKeyboard),
        Box::new(SdStorage(card.clone())),
        sync,
        Box::new(EspClock),
        system,
        Box::new(files),
    );
    runtime.run()
}

/// Apply a POSIX `TZ` string to libc so `localtime_r` reads the local calendar
/// day (see `Prefs::timezone`). newlib carries no zoneinfo database, so `tz` must
/// be the POSIX form (`CET-1CEST,M3.5.0,M10.5.0/3`), never an IANA name
/// (`Europe/Paris`) — the latter silently stays UTC. Best-effort: an interior NUL
/// or a failed `setenv` just leaves the previous zone (UTC) in place.
fn apply_timezone(tz: &str) {
    let Ok(c_tz) = std::ffi::CString::new(tz) else {
        log::warn!("timezone {tz:?} has an interior NUL; left at UTC");
        return;
    };
    // SAFETY: both pointers are valid C strings for the call; `tzset` reads the
    // `TZ` env var we just set. `1` = overwrite any existing value.
    unsafe {
        esp_idf_svc::sys::setenv(c"TZ".as_ptr(), c_tz.as_ptr(), 1);
        esp_idf_svc::sys::tzset();
    }
    log::info!("timezone applied: TZ={tz}");
}

/// Mount the SD card, or halt with the reason on the panel. A missing CARD is
/// fatal by design (see the boot-sequence comment in `main`): the note is the
/// whole point of the appliance, so we refuse to run in a state where the next
/// save could destroy it. A missing REPO is the caller's call — the git
/// build's wizard gate enters first-boot setup, the light build halts.
fn boot_storage(epd: &mut Epd) -> Storage {
    // A git build shares this mount with the git thread, and libgit2 keeps the
    // pack + idx descriptors open across a publish — that overruns the
    // editor's tight 4-FD budget, so mount with the 16-FD one (persistence.rs,
    // MAX_FILES_GIT). The light build keeps the editor's own budget.
    #[cfg(feature = "git")]
    let mounted = Storage::mount_for_git();
    #[cfg(not(feature = "git"))]
    let mounted = Storage::mount();
    match mounted {
        Ok(s) => s,
        Err(e) => boot_halt(epd, "SD card not ready", &format!("{e:#}")),
    }
}

/// Choose and load the boot buffer. With `open_last_on_boot` set and a marker
/// naming a still-existing file (`Storage::last_file`), resume that file;
/// otherwise the default note. Only the default note is fatal (`boot_halt`) —
/// a stale or unreadable last file falls back rather than refusing to boot.
fn boot_note(epd: &mut Epd, storage: &Storage, prefs: &Prefs) -> (String, Scope, String) {
    if prefs.open_last_on_boot {
        if let Some(path) = storage.last_file() {
            match storage.load_path(&path) {
                Ok(text) => {
                    log::info!("boot: resumed {path} ({} bytes)", text.len());
                    let scope = if path.starts_with(LOCAL_DIR) { Scope::Local } else { Scope::Tracked };
                    return (path, scope, text);
                }
                // Unreadable (e.g. grown past MAX_FILE_BYTES on a computer) —
                // the default note still boots.
                Err(e) => log::warn!("boot: can't resume {path} ({e:#}); falling back to {NOTES}"),
            }
        }
    }
    let note = match storage.load() {
        Ok(text) => text,
        Err(e) => boot_halt(epd, "Could not read your note", &format!("{e:#}")),
    };
    log::info!("boot: loaded {} bytes from {NOTES}", note.len());
    (NOTES.to_string(), Scope::Tracked, note)
}

/// Show a terminal boot error on the panel and idle forever. Rebooting into the
/// same missing card would just thrash, so we stop and explain instead.
fn boot_halt(epd: &mut Epd, headline: &str, detail: &str) -> ! {
    log::error!("boot halt — {headline}: {detail}");
    if let Err(e) = show_message(epd, &format!("{headline}\n\n{detail}\n")) {
        log::error!("(could not paint the boot error either: {e:#})");
    }
    loop {
        FreeRtos::delay_ms(1000);
    }
}

/// Render a plain full-frame message by borrowing the editor purely as a
/// text-layout engine, so boot failures surface on the panel, not a dead screen.
fn show_message(epd: &mut Epd, msg: &str) -> anyhow::Result<()> {
    let frame = Editor::with_text(msg.to_string()).draw(false);
    epd.display_frame(frame.bytes())?;
    Ok(())
}
