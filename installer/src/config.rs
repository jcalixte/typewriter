//! The runtime config the device reads from the card (`typoena.conf`), plus the
//! derive ladder that pre-fills it from this Mac. Mirrors the `_write-conf`
//! recipe in `firmware/justfile`.

use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    WifiSsid,
    WifiPass,
    GhUser,
    Pat,
    RemoteUrl,
    AuthorName,
    AuthorEmail,
}

impl Field {
    // The remote URL comes after the GitHub user + token on purpose: a bare repo
    // name (`notes`) is completed against the username, so it must be filled
    // first (^G fills both) for the shorthand to resolve.
    pub const ALL: [Field; 7] = [
        Field::WifiSsid,
        Field::WifiPass,
        Field::GhUser,
        Field::Pat,
        Field::RemoteUrl,
        Field::AuthorName,
        Field::AuthorEmail,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::WifiSsid => "Wi-Fi SSID",
            Field::WifiPass => "Wi-Fi password",
            Field::RemoteUrl => "Git remote URL",
            Field::GhUser => "GitHub username",
            Field::Pat => "GitHub token",
            Field::AuthorName => "Commit author name",
            Field::AuthorEmail => "Commit author email",
        }
    }

    pub fn secret(self) -> bool {
        matches!(self, Field::WifiPass | Field::Pat)
    }

    /// The `typoena.conf` key this field writes.
    pub fn conf_key(self) -> &'static str {
        match self {
            Field::WifiSsid => "TW_WIFI_SSID",
            Field::WifiPass => "TW_WIFI_PASS",
            Field::RemoteUrl => "TW_REMOTE_URL",
            Field::GhUser => "TW_GH_USER",
            Field::Pat => "TW_TOKEN",
            Field::AuthorName => "TW_AUTHOR_NAME",
            Field::AuthorEmail => "TW_AUTHOR_EMAIL",
        }
    }

    /// Required to ship a working device (a blank one bricks). Matches the
    /// four-var guard in `firmware/build.rs` minus the runtime-defaulted author.
    pub fn required(self) -> bool {
        matches!(self, Field::WifiSsid | Field::RemoteUrl | Field::Pat)
    }
}

#[derive(Default)]
pub struct Config {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub remote_url: String,
    pub gh_user: String,
    pub pat: String,
    pub author_name: String,
    pub author_email: String,
    /// The SSID couldn't be read from the live connection (macOS 15+ redacts it
    /// without Location Services) and was guessed from the preferred-network
    /// list — the UI flags it so the user confirms rather than trusts it.
    pub wifi_ssid_guessed: bool,
}

impl Config {
    /// Pre-fill from this Mac (the derive ladder). The GitHub token is never
    /// derived — a broad token on plaintext removable media defeats the
    /// scoped-token model; it comes from the ^G sign-in or is pasted.
    pub fn derived() -> Self {
        let dev = wifi_device().unwrap_or_else(|| "en0".to_string());
        let (wifi_ssid, wifi_ssid_guessed) = match active_wifi_ssid(&dev) {
            Some(s) => (s, false),
            None => (preferred_wifi_ssid(&dev).unwrap_or_default(), true),
        };
        Config {
            wifi_ssid_guessed: wifi_ssid_guessed && !wifi_ssid.is_empty(),
            wifi_ssid,
            wifi_pass: String::new(),
            remote_url: String::new(),
            gh_user: gh_login().unwrap_or_default(),
            pat: String::new(),
            author_name: git_config("user.name").unwrap_or_default(),
            author_email: git_config("user.email").unwrap_or_default(),
        }
    }

    pub fn get(&self, f: Field) -> &str {
        match f {
            Field::WifiSsid => &self.wifi_ssid,
            Field::WifiPass => &self.wifi_pass,
            Field::RemoteUrl => &self.remote_url,
            Field::GhUser => &self.gh_user,
            Field::Pat => &self.pat,
            Field::AuthorName => &self.author_name,
            Field::AuthorEmail => &self.author_email,
        }
    }

    pub fn get_mut(&mut self, f: Field) -> &mut String {
        match f {
            Field::WifiSsid => &mut self.wifi_ssid,
            Field::WifiPass => &mut self.wifi_pass,
            Field::RemoteUrl => &mut self.remote_url,
            Field::GhUser => &mut self.gh_user,
            Field::Pat => &mut self.pat,
            Field::AuthorName => &mut self.author_name,
            Field::AuthorEmail => &mut self.author_email,
        }
    }

    /// Required fields still blank (the SD step refuses to write a dead conf).
    pub fn missing_required(&self) -> Vec<Field> {
        Field::ALL
            .iter()
            .copied()
            .filter(|f| f.required() && self.get(*f).trim().is_empty())
            .collect()
    }

    /// The remote as the device needs it: the typed value expanded from any
    /// accepted shorthand to a full HTTPS clone URL. A bare repo name is
    /// completed against the GitHub username field.
    pub fn remote(&self) -> String {
        expand_remote_url_with_user(&self.remote_url, &self.gh_user)
    }

    /// Render the `typoena.conf` body — written to the card by the SD step.
    pub fn to_conf(&self) -> String {
        let mut s = String::new();
        s.push_str("# Typoena runtime config — generated by the installer.\n");
        s.push_str("# Plaintext secrets on removable media: keep the card safe. TW_PAT is a\n");
        s.push_str("# GitHub-App user token (Sign in with GitHub) or a fine-grained PAT scoped\n");
        s.push_str("# to contents:write on just the notes repo.\n");
        for f in Field::ALL {
            s.push_str(f.conf_key());
            s.push('=');
            match f {
                // The conf carries the expanded URL, never the shorthand — the
                // firmware clones/pushes exactly what's written here.
                Field::RemoteUrl => s.push_str(&self.remote()),
                _ => s.push_str(self.get(f)),
            }
            s.push('\n');
        }
        s
    }
}

/// Expand remote-URL shorthand to the canonical HTTPS clone URL, with no known
/// GitHub user — a bare repo name can't be completed and is returned as typed.
/// Prefer [`expand_remote_url_with_user`] where a username is available.
pub fn expand_remote_url(input: &str) -> String {
    expand_remote_url_with_user(input, "")
}

/// Expand remote-URL shorthand to the canonical HTTPS clone URL. The device's
/// libgit2 speaks HTTPS only, so everything funnels there:
///
/// - `notes`                 → `https://github.com/<gh_user>/notes.git` when a
///   GitHub username is known (returned as typed otherwise, so the clone fails
///   loudly rather than guessing an owner)
/// - `you/notes`             → `https://github.com/you/notes.git`
/// - `you/notes.git`         → `https://github.com/you/notes.git`
/// - `github.com/you/notes`  → `https://github.com/you/notes.git` (any host)
/// - `git@host:you/notes`    → `https://host/you/notes.git` (SSH paste — an
///   SSH origin on the card is a known device-push blocker)
/// - `ssh://git@host[:port]/you/notes` → `https://host/you/notes.git` (the ssh
///   port is dropped; it isn't the web port)
/// - full `http(s)://…` URLs pass through untouched.
pub fn expand_remote_url_with_user(input: &str, gh_user: &str) -> String {
    let s = input.trim().trim_end_matches('/');
    if s.starts_with("https://") || s.starts_with("http://") {
        return s.to_string();
    }
    if let Some(rest) = s.strip_prefix("git@")
        && let Some((host, path)) = rest.split_once(':')
    {
        return format!("https://{host}/{}", ensure_dot_git(path));
    }
    if let Some(rest) = s.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        if let Some((host_port, path)) = rest.split_once('/') {
            let host = host_port.split(':').next().unwrap_or(host_port);
            return format!("https://{host}/{}", ensure_dot_git(path));
        }
    }
    if !s.contains('/') {
        // A bare repo name: complete it against the GitHub user if we know one
        // (`notes` → `https://github.com/you/notes.git`); otherwise leave it be.
        let user = gh_user.trim();
        if !s.is_empty() && !user.is_empty() {
            return format!("https://github.com/{user}/{}", ensure_dot_git(s));
        }
        return s.to_string();
    }
    // A dotted first segment reads as a host (`github.com/you/notes`); a plain
    // one as a GitHub owner (`you/notes`).
    let first = s.split('/').next().unwrap_or("");
    if first.contains('.') {
        format!("https://{}", ensure_dot_git(s))
    } else {
        format!("https://github.com/{}", ensure_dot_git(s))
    }
}

fn ensure_dot_git(path: &str) -> String {
    let path = path.trim_start_matches('/');
    if path.ends_with(".git") {
        path.to_string()
    } else {
        format!("{path}.git")
    }
}

fn git_config(key: &str) -> Option<String> {
    non_empty(Command::new("git").args(["config", key]).output().ok()?)
}

fn gh_login() -> Option<String> {
    non_empty(
        Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .ok()?,
    )
}

/// The BSD device (enX) of the Wi-Fi hardware port. Don't assume en0 — Ethernet
/// Macs, Studios/Pros, and USB dongles put Wi-Fi elsewhere. Falls back to en0.
fn wifi_device() -> Option<String> {
    let out = Command::new("networksetup")
        .arg("-listallhardwareports")
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut in_wifi = false;
    for line in text.lines() {
        let l = line.trim();
        if let Some(port) = l.strip_prefix("Hardware Port:") {
            in_wifi = port.trim().eq_ignore_ascii_case("Wi-Fi");
        } else if in_wifi && let Some(dev) = l.strip_prefix("Device:") {
            return Some(dev.trim().to_string());
        }
    }
    None
}

/// The SSID of the network we're actually joined to, if it can be read. On
/// macOS 15+ the live SSID is redacted (returns "<redacted>") unless the caller
/// holds Location Services permission — which a `curl | sh` binary won't — so
/// this often yields nothing on newer systems; the caller then guesses.
fn active_wifi_ssid(dev: &str) -> Option<String> {
    // Pre-Sequoia: networksetup names the joined SSID directly. (On 15+ it just
    // reports "You are not associated with an AirPort network", parsed as None.)
    ns_current_ssid(dev).or_else(|| ipconfig_ssid(dev))
}

fn ns_current_ssid(dev: &str) -> Option<String> {
    let out = Command::new("networksetup")
        .args(["-getairportnetwork", dev])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Current Wi-Fi Network: "))
        .map(|s| s.trim().to_string())
        .filter(|s| is_real_ssid(s))
}

fn ipconfig_ssid(dev: &str) -> Option<String> {
    let out = Command::new("ipconfig")
        .args(["getsummary", dev])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| {
            let l = l.trim();
            let rest = l
                .strip_prefix("SSID :")
                .or_else(|| l.strip_prefix("SSID:"))?;
            Some(rest.trim().to_string())
        })
        .filter(|s| is_real_ssid(s))
}

/// Best-effort GUESS when the live SSID is unreadable. The top of the
/// preferred-network list is the most-preferred network — usually, but not
/// always, the one you're on. Never trusted silently: the UI marks it.
fn preferred_wifi_ssid(dev: &str) -> Option<String> {
    let out = Command::new("networksetup")
        .args(["-listpreferredwirelessnetworks", dev])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("Preferred networks on"))
        .map(str::to_string)
        .filter(|s| is_real_ssid(s))
}

fn is_real_ssid(s: &str) -> bool {
    !s.is_empty() && s != "<redacted>"
}

/// Look up a saved Wi-Fi password in the System keychain. Pops a macOS auth
/// dialog, so call only on explicit user action. Mirrors the justfile's
/// `security find-generic-password -wa <ssid>`.
pub fn keychain_wifi_password(ssid: &str) -> Option<String> {
    if ssid.trim().is_empty() {
        return None;
    }
    non_empty(
        Command::new("security")
            .args(["find-generic-password", "-wa", ssid])
            .output()
            .ok()?,
    )
}

fn non_empty(out: std::process::Output) -> Option<String> {
    if !out.status.success() {
        return None;
    }
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!v.is_empty()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owner_repo_shorthand_expands_to_github() {
        assert_eq!(
            expand_remote_url("you/notes"),
            "https://github.com/you/notes.git"
        );
        assert_eq!(
            expand_remote_url("you/notes.git"),
            "https://github.com/you/notes.git"
        );
    }

    #[test]
    fn host_paths_get_the_scheme() {
        assert_eq!(
            expand_remote_url("github.com/you/notes.git"),
            "https://github.com/you/notes.git"
        );
        assert_eq!(
            expand_remote_url("git.apoena.dev/you/notes"),
            "https://git.apoena.dev/you/notes.git"
        );
    }

    #[test]
    fn full_urls_pass_through_untouched() {
        assert_eq!(
            expand_remote_url("https://github.com/you/notes.git"),
            "https://github.com/you/notes.git"
        );
        // No forced .git on an explicit URL — respect the exact paste.
        assert_eq!(
            expand_remote_url("https://github.com/you/notes"),
            "https://github.com/you/notes"
        );
    }

    #[test]
    fn ssh_pastes_become_https() {
        assert_eq!(
            expand_remote_url("git@github.com:you/notes.git"),
            "https://github.com/you/notes.git"
        );
        assert_eq!(
            expand_remote_url("ssh://git@git.apoena.dev:22222/you/notes.git"),
            "https://git.apoena.dev/you/notes.git",
            "the ssh port must be dropped, not carried to https"
        );
    }

    #[test]
    fn bare_repo_name_is_completed_against_the_github_user() {
        assert_eq!(
            expand_remote_url_with_user("notes", "you"),
            "https://github.com/you/notes.git"
        );
        assert_eq!(
            expand_remote_url_with_user("notes.git", "you"),
            "https://github.com/you/notes.git"
        );
        // Config::remote() feeds the username in; it must complete a bare name.
        let cfg = Config {
            remote_url: "notes".into(),
            gh_user: "you".into(),
            ..Config::default()
        };
        assert_eq!(cfg.remote(), "https://github.com/you/notes.git");
    }

    #[test]
    fn bare_name_without_a_user_is_left_as_typed() {
        // No username to complete against — leave it so the clone fails loudly
        // rather than guessing an owner.
        assert_eq!(expand_remote_url_with_user("notes", ""), "notes");
        assert_eq!(expand_remote_url_with_user("notes", "  "), "notes");
        assert_eq!(expand_remote_url("notes"), "notes");
        assert_eq!(expand_remote_url(""), "");
        // An empty remote never becomes a URL, even with a user.
        assert_eq!(expand_remote_url_with_user("", "you"), "");
    }

    #[test]
    fn a_known_user_never_overrides_an_explicit_owner_or_url() {
        // The two-segment and full-URL forms are already unambiguous — the
        // username must not hijack them.
        assert_eq!(
            expand_remote_url_with_user("someone/notes", "you"),
            "https://github.com/someone/notes.git"
        );
        assert_eq!(
            expand_remote_url_with_user("https://github.com/someone/notes.git", "you"),
            "https://github.com/someone/notes.git"
        );
    }

    #[test]
    fn whitespace_and_trailing_slash_are_trimmed() {
        assert_eq!(
            expand_remote_url("  you/notes/  "),
            "https://github.com/you/notes.git"
        );
    }

    #[test]
    fn the_conf_carries_the_expanded_url() {
        let cfg = Config {
            remote_url: "you/notes".into(),
            ..Config::default()
        };
        assert!(
            cfg.to_conf()
                .contains("TW_REMOTE_URL=https://github.com/you/notes.git\n"),
            "the device must never see the shorthand"
        );
    }
}
