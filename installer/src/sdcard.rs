//! SD-card provisioning: pick the card, clone the notes repo onto it, seed the
//! git-tracked prefs, write `typoena.conf`, and eject. Ports the safety
//! behaviours of the `just init`/`load` recipes; the repo copy is a fresh clone
//! from the remote (no rsync / .gitignore excludes / repack — see DESIGN.md).

use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::Sender;

use anyhow::{Context, bail};
use base64::{Engine as _, engine::general_purpose::STANDARD};

/// A candidate removable volume.
pub struct Card {
    pub volume: PathBuf,
    pub name: String,
    pub fs: String,
    pub fat: bool,
}

/// Read-only summary of an already-provisioned card, shown on the wipe-confirm
/// screen so the user sees exactly what they're about to erase.
pub struct CardInspect {
    pub origin: Option<String>,
    pub head: Option<String>,
    pub dirty: usize,
}

pub struct Plan {
    pub remote: String,
    pub pat: String,
    pub card_volume: PathBuf,
    pub conf_body: String,
    /// Erase an existing `repo/` + dirty journal before cloning.
    pub wipe: bool,
}

impl Plan {
    fn repo_dir(&self) -> PathBuf {
        self.card_volume.join("repo")
    }
    fn conf_path(&self) -> PathBuf {
        self.card_volume.join("typoena.conf")
    }
}

pub enum SdEvent {
    Log(String),
    /// A parsed git-progress tick (phase name + 0..=100 percent) driving the gauge.
    Progress {
        phase: String,
        pct: u16,
    },
    Done(Result<(), String>),
}

/// Detect removable/SD volumes under /Volumes (via diskutil). Mirrors the
/// justfile `_card` heuristics; the internal boot disk never matches.
pub fn detect_cards() -> Vec<Card> {
    let Ok(rd) = std::fs::read_dir("/Volumes") else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|entry| {
            let vol = entry.path();
            if !vol.is_dir() {
                return None;
            }
            let info = match Command::new("diskutil").arg("info").arg(&vol).output() {
                Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
                _ => return None,
            };
            if !is_removable(&info) {
                return None;
            }
            let fs = field(&info, "File System Personality").unwrap_or_default();
            let up = fs.to_uppercase();
            Some(Card {
                name: vol
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                fat: up.contains("FAT") || up.contains("MS-DOS"),
                fs,
                volume: vol,
            })
        })
        .collect()
}

pub(crate) fn is_removable(info: &str) -> bool {
    // Test the VALUE of each key, not the whole line: the label "Removable
    // Media" itself contains "Removable", so a line-substring test matches every
    // disk — including the internal boot volume (found on real hardware, 07-14).
    let val = |k| field(info, k).unwrap_or_default();
    val("Protocol").contains("Secure Digital")
        || val("Removable Media").contains("Removable")
        || val("Ejectable") == "Yes"
        || val("Device Location") == "External"
}

pub(crate) fn field(info: &str, key: &str) -> Option<String> {
    info.lines().find_map(|l| {
        let rest = l.trim().strip_prefix(key)?.trim_start();
        let val = rest.strip_prefix(':')?.trim();
        (!val.is_empty()).then(|| val.to_string())
    })
}

/// True if the card already carries a working copy at `repo/`.
pub fn card_has_repo(vol: &Path) -> bool {
    vol.join("repo").exists()
}

/// Read-only inspection of an existing card (origin, HEAD, unpushed-edit count).
pub fn inspect_card(vol: &Path) -> CardInspect {
    let repo = vol.join("repo");
    let git = |args: &[&str]| -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&repo)
            .args(args)
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!v.is_empty()).then_some(v)
    };
    CardInspect {
        origin: git(&["remote", "get-url", "origin"]),
        head: git(&["rev-parse", "--short", "HEAD"]),
        dirty: std::fs::read_to_string(vol.join(".typoena-dirty"))
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count())
            .unwrap_or(0),
    }
}

/// Run the full provision on a worker thread, streaming events to `tx`.
pub fn run_provision(plan: Plan, tx: Sender<SdEvent>) {
    let mut emit = |e: SdEvent| {
        let _ = tx.send(e);
    };
    let res = provision(&plan, &mut emit).map_err(|e| format!("{e:#}"));
    let _ = tx.send(SdEvent::Done(res));
}

fn provision(plan: &Plan, emit: &mut dyn FnMut(SdEvent)) -> anyhow::Result<()> {
    if plan.wipe {
        wipe_card(&plan.card_volume, emit)?;
    }
    clone(&plan.remote, &plan.repo_dir(), &plan.pat, emit)?;
    emit(SdEvent::Log("seeding .typoena.toml (if absent)…".into()));
    seed_prefs(&plan.repo_dir())?;
    emit(SdEvent::Log(format!(
        "writing {}",
        plan.conf_path().display()
    )));
    std::fs::write(plan.conf_path(), &plan.conf_body).context("writing typoena.conf")?;
    emit(SdEvent::Log("stripping AppleDouble ._ files…".into()));
    dot_clean(&plan.card_volume);
    emit(SdEvent::Log("ejecting…".into()));
    match eject(&plan.card_volume) {
        Ok(()) => emit(SdEvent::Log(
            "card ejected — remove it and insert into Typoena.".into(),
        )),
        Err(e) => emit(SdEvent::Log(format!(
            "⚠ could not eject ({e}); eject from Finder before removing."
        ))),
    }
    Ok(())
}

/// Erase an existing working copy before a re-provision. Only ever removes
/// `repo/` and the `.typoena-dirty` journal — never the volume itself, `ca.pem`,
/// or `/local`. The path guard rejects a bogus (root/empty) volume.
fn wipe_card(vol: &Path, emit: &mut dyn FnMut(SdEvent)) -> anyhow::Result<()> {
    if !vol.is_dir() || vol.parent().is_none() {
        bail!(
            "refusing to wipe: '{}' is not a mounted volume",
            vol.display()
        );
    }
    let repo = vol.join("repo");
    emit(SdEvent::Log(format!("wiping {} …", repo.display())));
    if repo.exists() {
        std::fs::remove_dir_all(&repo).with_context(|| format!("removing {}", repo.display()))?;
    }
    let _ = std::fs::remove_file(vol.join(".typoena-dirty"));
    Ok(())
}

/// Clone `remote` into `dest` with the system git. The PAT (if any) rides in an
/// HTTP Authorization header, so it never lands in the cloned repo's origin URL
/// — origin stays the clean HTTPS URL the device authenticates against.
fn clone(
    remote: &str,
    dest: &Path,
    pat: &str,
    emit: &mut dyn FnMut(SdEvent),
) -> anyhow::Result<()> {
    if dest.exists() {
        bail!(
            "{} already exists — wipe the card first, or use a fresh one",
            dest.display()
        );
    }
    emit(SdEvent::Log(format!(
        "cloning {remote} → {}",
        dest.display()
    )));
    let mut cmd = Command::new("git");
    if !pat.is_empty() {
        let token = STANDARD.encode(format!("x-access-token:{pat}"));
        cmd.arg("-c")
            .arg(format!("http.extraHeader=Authorization: Basic {token}"));
    }
    cmd.arg("clone")
        .arg("--progress")
        .arg(remote)
        .arg(dest)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().context("spawning git (is it installed?)")?;
    // git writes progress to stderr, updating a line in place with `\r`; only
    // phase transitions get a `\n`. Splitting on both carriage return and
    // newline surfaces the live "Receiving objects: 45%" ticks that a
    // line-buffered reader would swallow until the phase ends.
    let mut denied = false;
    if let Some(err) = child.stderr.take() {
        split_cr_lf(err, &mut |seg: &str| {
            let seg = seg.trim();
            if seg.is_empty() {
                return;
            }
            denied = denied || is_access_denied(seg);
            match parse_progress(seg) {
                Some((phase, pct)) => {
                    emit(SdEvent::Progress { phase, pct });
                    // Keep the scrolling log readable: only the phase-final
                    // "…, done." lines land there; the gauge shows the rest.
                    if seg.ends_with("done.") {
                        emit(SdEvent::Log(seg.to_string()));
                    }
                }
                None => emit(SdEvent::Log(seg.to_string())),
            }
        });
    }
    let status = child.wait().context("waiting for git clone")?;
    if !status.success() {
        if denied {
            bail!(
                "GitHub refused access (403). A ^G sign-in proves who you are, but the \
                 Typoena app must also be INSTALLED on this repo — grant it at \
                 {} (pick your account → Only select repositories → this repo), \
                 then press Enter to retry. No new sign-in needed.",
                crate::auth::APP_INSTALL_URL
            );
        }
        bail!("git clone failed (exit {:?})", status.code());
    }
    Ok(())
}

/// A git-clone stderr segment that means the credential authenticated but the
/// repo refused it — for an app token, that's "the app isn't installed here".
/// Observed on real hardware 2026-07-15: `remote: Write access to repository
/// not granted.` followed by `fatal: unable to access '…': The requested URL
/// returned error: 403`.
fn is_access_denied(seg: &str) -> bool {
    seg.contains("error: 403") || seg.contains("not granted")
}

/// Feed each `\r`- or `\n`-delimited segment of `reader` to `on_segment`.
/// git's progress volume is small (kilobytes of text), so byte-wise is fine.
pub(crate) fn split_cr_lf(reader: impl Read, mut on_segment: impl FnMut(&str)) {
    let mut buf: Vec<u8> = Vec::new();
    for b in BufReader::new(reader).bytes().map_while(Result::ok) {
        if b == b'\r' || b == b'\n' {
            if !buf.is_empty() {
                on_segment(&String::from_utf8_lossy(&buf));
                buf.clear();
            }
        } else {
            buf.push(b);
        }
    }
    if !buf.is_empty() {
        on_segment(&String::from_utf8_lossy(&buf));
    }
}

/// Parse a git progress segment like `Receiving objects:  45% (2345/5210), …`
/// (optionally `remote: `-prefixed) into its phase name and 0..=100 percent.
fn parse_progress(seg: &str) -> Option<(String, u16)> {
    let seg = seg.strip_prefix("remote: ").unwrap_or(seg);
    let (phase, rest) = seg.split_once(": ")?;
    let pct: u16 = rest.split('%').next()?.trim().parse().ok()?;
    (pct <= 100).then(|| (phase.trim().to_string(), pct))
}

const PREFS_TEMPLATE: &str = "\
# Typoena editor preferences — hand-editable, git-tracked.
save_on_idle = true
format_on_save = true
line_numbers = true
theme = \"light\"
auto_sync = \"10m\"
";

/// Seed a starter `.typoena.toml` only if the cloned repo doesn't already carry
/// one (a repo with a synced prefs file keeps its own). Mirrors `_seed-configs`.
fn seed_prefs(repo_dir: &Path) -> anyhow::Result<()> {
    let p = repo_dir.join(".typoena.toml");
    if p.exists() {
        return Ok(());
    }
    std::fs::write(&p, PREFS_TEMPLATE).with_context(|| format!("seeding {}", p.display()))?;
    Ok(())
}

fn dot_clean(vol: &Path) {
    // Best-effort: strip the AppleDouble `._` companions macOS writes on FAT,
    // which otherwise corrupt the pack scan (`._pack-*.idx`). Failure never blocks.
    // Capture (don't inherit) output: dot_clean prints permission errors for the
    // SIP dirs it can't enter (`.Spotlight-V100`, `.fseventsd`) to stderr, which
    // would otherwise smear straight across the ratatui alternate screen.
    let _ = Command::new("dot_clean")
        .arg("-m")
        .arg(vol)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn eject(vol: &Path) -> anyhow::Result<()> {
    let _ = Command::new("sync")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    // Capture, don't inherit: diskutil prints "Disk … ejected" to stdout, which
    // would otherwise land on the TUI. We surface our own log line instead.
    let out = Command::new("diskutil")
        .arg("eject")
        .arg(vol)
        .output()
        .context("running diskutil eject")?;
    if !out.status.success() {
        bail!(
            "diskutil eject exited {:?}: {}",
            out.status.code(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

/// Headless verification: (optionally wipe) + clone + seed + write a sample conf
/// into `dest`, with no card selection and no eject. Backs `--dry-run-sd`.
pub fn dry_run(remote: &str, dest: &Path, wipe: bool) -> anyhow::Result<()> {
    let plan = Plan {
        remote: remote.to_string(),
        pat: String::new(),
        card_volume: dest.to_path_buf(),
        conf_body: "# sample typoena.conf (dry run)\nTW_WIFI_SSID=example\n".to_string(),
        wipe,
    };
    let mut emit = |e: SdEvent| match e {
        SdEvent::Log(m) => println!("  {m}"),
        SdEvent::Progress { phase, pct } => println!("  {phase}: {pct}%"),
        SdEvent::Done(_) => {}
    };
    if plan.wipe {
        wipe_card(&plan.card_volume, &mut emit)?;
    }
    clone(&plan.remote, &plan.repo_dir(), &plan.pat, &mut emit)?;
    emit(SdEvent::Log("seeding .typoena.toml (if absent)…".into()));
    seed_prefs(&plan.repo_dir())?;
    emit(SdEvent::Log(format!(
        "writing {}",
        plan.conf_path().display()
    )));
    std::fs::write(plan.conf_path(), &plan.conf_body).context("writing typoena.conf")?;
    emit(SdEvent::Log(
        "dry run complete (no card write, no eject).".into(),
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_observed_403_lines_are_flagged() {
        // Verbatim from the 2026-07-15 field failure.
        assert!(is_access_denied(
            "remote: Write access to repository not granted."
        ));
        assert!(is_access_denied(
            "fatal: unable to access 'https://github.com/jcalixte/notes.git/': \
             The requested URL returned error: 403"
        ));
    }

    #[test]
    fn ordinary_clone_chatter_is_not_flagged() {
        assert!(!is_access_denied("Cloning into '/Volumes/TYPOENA/repo'..."));
        assert!(!is_access_denied("Receiving objects:  45% (100/220)"));
        assert!(!is_access_denied(
            "fatal: unable to access '…': Could not resolve host: github.com"
        ));
    }
}
