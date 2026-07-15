//! "Sign in with GitHub" — the GitHub App **device flow**, which replaces
//! hand-creating a PAT. We ask GitHub for a one-time user code, the user types
//! it at github.com/login/device and authorizes the Typoena app, and GitHub
//! hands back a `ghu_` user token that speaks the exact same HTTPS basic-auth
//! wire protocol as a PAT — so the firmware and the clone path need no changes.
//!
//! HTTP goes through the system `curl` (same shell-out philosophy as git /
//! diskutil — no HTTP-client crate). GitHub's OAuth endpoints answer in
//! `application/x-www-form-urlencoded` unless asked for JSON, which parses
//! with a few lines and zero deps.

use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

/// The Typoena GitHub App's client id. Public by design (it names the app,
/// like a username — the device flow needs no secret on the client).
pub const CLIENT_ID: &str = "Iv23liwgnE86ITDpBdnn";

/// Where the user grants the app access to a repo. Signing in (^G) only proves
/// identity — an app token can't touch a repo until the app is *installed* on
/// it, which is the classic 403 footgun the SD step hints about.
pub const APP_INSTALL_URL: &str = "https://github.com/apps/typoena/installations/new";

const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";

/// Progress of the background sign-in worker, drained on the UI thread.
pub enum AuthEvent {
    /// GitHub issued a code; show it and wait for the user to authorize.
    Code {
        user_code: String,
        verification_uri: String,
    },
    Done(Result<Token, String>),
}

pub struct Token {
    pub access_token: String,
    /// Set when the app has "expire user authorization tokens" enabled — the
    /// token dies after this many seconds, worth surfacing to the user.
    pub expires_in: Option<u64>,
}

/// Worker entry point: request a code, report it, then poll until the user
/// authorizes (or cancels / the code expires). Always ends with a `Done`.
pub fn run_device_flow(tx: Sender<AuthEvent>, cancel: Arc<AtomicBool>) {
    let result = device_flow(&tx, &cancel);
    let _ = tx.send(AuthEvent::Done(result));
}

fn device_flow(tx: &Sender<AuthEvent>, cancel: &AtomicBool) -> Result<Token, String> {
    let fields = post_form(DEVICE_CODE_URL, &[("client_id", CLIENT_ID)])?;
    if let Some(e) = form_error(&fields) {
        return Err(e);
    }
    let device_code =
        get(&fields, "device_code").ok_or("GitHub sent no device code — try again")?;
    let user_code = get(&fields, "user_code").ok_or("GitHub sent no user code — try again")?;
    let verification_uri = get(&fields, "verification_uri")
        .unwrap_or_else(|| "https://github.com/login/device".to_string());
    let mut interval = num(&fields, "interval").unwrap_or(5);
    let deadline = Instant::now() + Duration::from_secs(num(&fields, "expires_in").unwrap_or(900));

    let _ = tx.send(AuthEvent::Code {
        user_code,
        verification_uri: verification_uri.clone(),
    });
    open_browser(&verification_uri);

    while Instant::now() < deadline {
        if !sleep_unless_cancelled(Duration::from_secs(interval), cancel) {
            return Err("cancelled".to_string());
        }
        let fields = post_form(
            TOKEN_URL,
            &[
                ("client_id", CLIENT_ID),
                ("device_code", &device_code),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ],
        )?;
        if let Some(token) = get(&fields, "access_token") {
            return Ok(Token {
                access_token: token,
                expires_in: num(&fields, "expires_in"),
            });
        }
        match get(&fields, "error").as_deref() {
            // Not authorized yet — keep polling at GitHub's requested pace.
            Some("authorization_pending") => {}
            Some("slow_down") => interval = num(&fields, "interval").unwrap_or(interval + 5),
            Some(_) => return Err(form_error(&fields).unwrap_or_else(|| "GitHub refused".into())),
            None => return Err("unexpected response from GitHub".to_string()),
        }
    }
    Err("the code expired before it was entered — sign in again for a fresh one".to_string())
}

/// POST urlencoded params via curl; parse the form-encoded reply. curl without
/// `--fail` still prints the body on a 4xx, so OAuth errors come back as
/// parseable `error=` fields rather than an opaque exit code.
fn post_form(url: &str, params: &[(&str, &str)]) -> Result<Vec<(String, String)>, String> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sS", "--max-time", "15"]);
    for (k, v) in params {
        cmd.arg("--data-urlencode").arg(format!("{k}={v}"));
    }
    let out = cmd
        .arg(url)
        .output()
        .map_err(|e| format!("couldn't run curl: {e}"))?;
    let body = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if body.is_empty() {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(if err.is_empty() {
            "no response from GitHub — check your connection".to_string()
        } else {
            err
        });
    }
    Ok(parse_form(&body))
}

/// Re-open the verification page (also fired automatically when the code
/// arrives). Best-effort; the URL is on screen either way.
pub fn open_browser(uri: &str) {
    let _ = Command::new("open").arg(uri).spawn();
}

/// Can the token see the target repo? Asked proactively at Configure so the
/// authorization-≠-installation 403 (which otherwise every first-time ^G user
/// hits at the clone) surfaces as a fixable flag instead of a failure.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RepoAccess {
    Granted,
    /// The API said 401/403/404 — for a ^G token that means "app not
    /// installed on this repo"; for a PAT, "no access granted".
    Missing,
    /// Couldn't tell (non-GitHub host, network trouble) — never flagged.
    Unknown,
}

/// `GET /repos/{owner}/{repo}` with the token. GitHub answers 404 (not 403)
/// for a repo the credential can't see, so any auth-shaped status maps to
/// `Missing`; transport failures stay `Unknown` so a flaky network never
/// paints a false warning.
pub fn check_repo_access(token: &str, remote: &str) -> RepoAccess {
    let Some(url) = repo_api_url(remote) else {
        return RepoAccess::Unknown;
    };
    let out = Command::new("curl")
        .args([
            "-sS",
            "-o",
            "/dev/null",
            "-w",
            "%{http_code}",
            "--max-time",
            "10",
        ])
        .args(["-H", &format!("Authorization: Bearer {token}")])
        .args(["-H", "X-GitHub-Api-Version: 2022-11-28"])
        .arg(&url)
        .output();
    match out {
        Ok(o) if o.status.success() => match String::from_utf8_lossy(&o.stdout).trim() {
            "200" => RepoAccess::Granted,
            "401" | "403" | "404" => RepoAccess::Missing,
            _ => RepoAccess::Unknown,
        },
        _ => RepoAccess::Unknown,
    }
}

/// The API endpoint for a github.com remote, or None for hosts we can't
/// introspect (the check silently stands down there).
fn repo_api_url(remote: &str) -> Option<String> {
    let rest = remote.strip_prefix("https://github.com/")?;
    let path = rest.strip_suffix(".git").unwrap_or(rest);
    let (owner, repo) = path.split_once('/')?;
    if owner.is_empty() || repo.is_empty() || repo.contains('/') {
        return None;
    }
    Some(format!("https://api.github.com/repos/{owner}/{repo}"))
}

/// Sleep in short ticks so a cancel takes effect promptly. Returns false when
/// cancelled.
fn sleep_unless_cancelled(total: Duration, cancel: &AtomicBool) -> bool {
    let deadline = Instant::now() + total;
    while Instant::now() < deadline {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    !cancel.load(Ordering::Relaxed)
}

fn parse_form(body: &str) -> Vec<(String, String)> {
    body.split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            (percent_decode(k), percent_decode(v))
        })
        .collect()
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = |b: u8| (b as char).to_digit(16);
                match (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                    (Some(hi), Some(lo)) => {
                        out.push((hi * 16 + lo) as u8);
                        i += 3;
                    }
                    _ => {
                        out.push(b'%');
                        i += 1;
                    }
                }
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn get(fields: &[(String, String)], key: &str) -> Option<String> {
    fields
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
        .filter(|v| !v.is_empty())
}

fn num(fields: &[(String, String)], key: &str) -> Option<u64> {
    get(fields, key)?.parse().ok()
}

fn form_error(fields: &[(String, String)]) -> Option<String> {
    let code = get(fields, "error")?;
    Some(match get(fields, "error_description") {
        Some(d) => format!("{d} ({code})"),
        None => code,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_device_code_reply() {
        let f = parse_form(
            "device_code=abc123&expires_in=899&interval=5&user_code=WDJB-MJHT\
             &verification_uri=https%3A%2F%2Fgithub.com%2Flogin%2Fdevice",
        );
        assert_eq!(get(&f, "user_code").as_deref(), Some("WDJB-MJHT"));
        assert_eq!(
            get(&f, "verification_uri").as_deref(),
            Some("https://github.com/login/device"),
            "percent-encoded URI must decode"
        );
        assert_eq!(num(&f, "interval"), Some(5));
        assert!(form_error(&f).is_none());
    }

    #[test]
    fn parses_an_oauth_error_with_plus_spaces() {
        let f = parse_form("error=access_denied&error_description=The+user+denied+the+request");
        let e = form_error(&f).expect("error field must surface");
        assert!(e.contains("The user denied the request"));
        assert!(e.contains("access_denied"));
    }

    #[test]
    fn a_token_reply_yields_token_and_expiry() {
        let f = parse_form("access_token=ghu_abc&expires_in=28800&token_type=bearer&scope=");
        assert_eq!(get(&f, "access_token").as_deref(), Some("ghu_abc"));
        assert_eq!(num(&f, "expires_in"), Some(28800));
        assert!(
            get(&f, "scope").is_none(),
            "empty values read as absent, not empty strings"
        );
    }

    #[test]
    fn malformed_percent_sequences_pass_through() {
        assert_eq!(percent_decode("100%zz"), "100%zz");
        assert_eq!(percent_decode("a%2"), "a%2");
    }

    #[test]
    fn github_remotes_map_to_the_api() {
        assert_eq!(
            repo_api_url("https://github.com/you/notes.git").as_deref(),
            Some("https://api.github.com/repos/you/notes")
        );
        assert_eq!(
            repo_api_url("https://github.com/you/notes").as_deref(),
            Some("https://api.github.com/repos/you/notes")
        );
    }

    #[test]
    fn uncheckable_remotes_stand_down() {
        assert!(repo_api_url("https://git.apoena.dev/you/notes.git").is_none());
        assert!(repo_api_url("https://github.com/just-an-owner").is_none());
        assert!(repo_api_url("https://github.com/a/b/c.git").is_none());
    }
}
