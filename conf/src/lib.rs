//! `typoena.conf` — the device runtime config on the SD card.
//!
//! One flat `KEY=value` file at `/sd/typoena.conf` carrying the seven `TW_*`
//! values (Wi-Fi, git remote, GitHub user + token, commit author). Written by
//! the host installer or the on-device wizard; read by the firmware at boot.
//! Plaintext secrets on removable media by design — physical custody of the
//! card is the control (see installer/DESIGN.md).
//!
//! This crate is the schema's single source of truth: field list,
//! required-ness, parse, render, and the remote-URL shorthand expansion. It
//! was ported from `installer/src/config.rs` (the host-side derive ladder and
//! macOS lookups stayed behind); the installer should eventually consume this
//! crate so the two writers cannot drift.

/// One config field. Order is the file order and the wizard's edit order.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    WifiSsid,
    WifiPass,
    RemoteUrl,
    GhUser,
    Token,
    AuthorName,
    AuthorEmail,
}

impl Field {
    pub const ALL: [Field; 7] = [
        Field::WifiSsid,
        Field::WifiPass,
        Field::RemoteUrl,
        Field::GhUser,
        Field::Token,
        Field::AuthorName,
        Field::AuthorEmail,
    ];

    /// The `typoena.conf` key this field reads/writes.
    pub fn conf_key(self) -> &'static str {
        match self {
            Field::WifiSsid => "TW_WIFI_SSID",
            Field::WifiPass => "TW_WIFI_PASS",
            Field::RemoteUrl => "TW_REMOTE_URL",
            Field::GhUser => "TW_GH_USER",
            // Historic spelling, kept for card compatibility (the installer
            // and firmware/.env both use it). The VALUE is a GitHub App user
            // token from the device flow; a fine-grained PAT also still works.
            Field::Token => "TW_PAT",
            Field::AuthorName => "TW_AUTHOR_NAME",
            Field::AuthorEmail => "TW_AUTHOR_EMAIL",
        }
    }

    /// Human label (wizard / installer prompt text).
    pub fn label(self) -> &'static str {
        match self {
            Field::WifiSsid => "Wi-Fi network",
            Field::WifiPass => "Wi-Fi password",
            Field::RemoteUrl => "Git remote URL",
            Field::GhUser => "GitHub user",
            Field::Token => "GitHub token",
            Field::AuthorName => "Commit author name",
            Field::AuthorEmail => "Commit author email",
        }
    }

    /// Masked when displayed.
    pub fn secret(self) -> bool {
        matches!(self, Field::WifiPass | Field::Token)
    }

    /// Required for a working device. TW_WIFI_PASS may be legitimately empty
    /// (open network); TW_AUTHOR_* have runtime defaults.
    pub fn required(self) -> bool {
        matches!(
            self,
            Field::WifiSsid | Field::RemoteUrl | Field::GhUser | Field::Token
        )
    }
}

/// A parsed (or under-construction) `typoena.conf`.
#[derive(Default, Clone, PartialEq, Eq, Debug)]
pub struct Conf {
    pub wifi_ssid: String,
    pub wifi_pass: String,
    pub remote_url: String,
    pub gh_user: String,
    pub token: String,
    pub author_name: String,
    pub author_email: String,
}

impl Conf {
    pub fn get(&self, f: Field) -> &str {
        match f {
            Field::WifiSsid => &self.wifi_ssid,
            Field::WifiPass => &self.wifi_pass,
            Field::RemoteUrl => &self.remote_url,
            Field::GhUser => &self.gh_user,
            Field::Token => &self.token,
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
            Field::Token => &mut self.token,
            Field::AuthorName => &mut self.author_name,
            Field::AuthorEmail => &mut self.author_email,
        }
    }

    /// Required fields still blank.
    pub fn missing_required(&self) -> Vec<Field> {
        Field::ALL
            .iter()
            .copied()
            .filter(|f| f.required() && self.get(*f).trim().is_empty())
            .collect()
    }

    /// Parse a `typoena.conf` body. Unknown keys and malformed lines are
    /// ignored (forward compatibility — an older firmware must not choke on a
    /// newer installer's file), `#` comments and blank lines skipped. Values
    /// are taken verbatim after the first `=` (a Wi-Fi password may contain
    /// `=`, spaces, anything); only a trailing `\r` is stripped so a
    /// CRLF-edited card still parses.
    pub fn parse(body: &str) -> Conf {
        let mut c = Conf::default();
        for line in body.lines() {
            let line = line.strip_suffix('\r').unwrap_or(line);
            if line.trim_start().starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            for f in Field::ALL {
                if f.conf_key() == key {
                    *c.get_mut(f) = value.to_string();
                }
            }
        }
        c
    }

    /// Render the `typoena.conf` body. The remote is written expanded
    /// (`expand_remote_url`) — the firmware clones/pushes exactly what's
    /// here, and its libgit2 speaks HTTPS only.
    pub fn render(&self) -> String {
        let mut s = String::new();
        s.push_str("# Typoena runtime config.\n");
        s.push_str("# Plaintext secrets on removable media: keep the card safe. TW_PAT is a\n");
        s.push_str("# GitHub-App user token (Sign in with GitHub) or a fine-grained PAT scoped\n");
        s.push_str("# to contents:write on just the notes repo.\n");
        for f in Field::ALL {
            s.push_str(f.conf_key());
            s.push('=');
            match f {
                Field::RemoteUrl => s.push_str(&expand_remote_url(&self.remote_url)),
                _ => s.push_str(self.get(f)),
            }
            s.push('\n');
        }
        s
    }
}

/// Expand remote-URL shorthand to the canonical HTTPS clone URL. The device's
/// libgit2 speaks HTTPS only, so everything funnels there:
///
/// - `you/notes`             → `https://github.com/you/notes.git`
/// - `you/notes.git`         → `https://github.com/you/notes.git`
/// - `github.com/you/notes`  → `https://github.com/you/notes.git` (any host)
/// - `git@host:you/notes`    → `https://host/you/notes.git` (SSH paste — an
///   SSH origin on the card is a known device-push blocker)
/// - `ssh://git@host[:port]/you/notes` → `https://host/you/notes.git` (the ssh
///   port is dropped; it isn't the web port)
/// - full `http(s)://…` URLs pass through untouched.
///
/// Inputs with no `/` at all can't be a repo path — returned as typed so the
/// clone fails loudly rather than guessing.
pub fn expand_remote_url(input: &str) -> String {
    let s = input.trim().trim_end_matches('/');
    if s.starts_with("https://") || s.starts_with("http://") {
        return s.to_string();
    }
    if let Some(rest) = s.strip_prefix("git@") {
        if let Some((host, path)) = rest.split_once(':') {
            return format!("https://{host}/{}", ensure_dot_git(path));
        }
    }
    if let Some(rest) = s.strip_prefix("ssh://") {
        let rest = rest.strip_prefix("git@").unwrap_or(rest);
        if let Some((host_port, path)) = rest.split_once('/') {
            let host = host_port.split(':').next().unwrap_or(host_port);
            return format!("https://{host}/{}", ensure_dot_git(path));
        }
    }
    if !s.contains('/') {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_render_round_trip() {
        let c = Conf {
            wifi_ssid: "Freebox-1234".into(),
            wifi_pass: "s3cret with spaces=and=equals".into(),
            remote_url: "https://github.com/typoena/notes.git".into(),
            gh_user: "jcalixte".into(),
            token: "ghu_abc".into(),
            author_name: "Julien Calixte".into(),
            author_email: "j@example.com".into(),
        };
        assert_eq!(Conf::parse(&c.render()), c);
    }

    #[test]
    fn parse_skips_comments_blanks_unknown_and_malformed() {
        let body = "# header\n\nTW_WIFI_SSID=net\nTW_FUTURE_KEY=x\nnot a kv line\nTW_GH_USER=me\n";
        let c = Conf::parse(body);
        assert_eq!(c.wifi_ssid, "net");
        assert_eq!(c.gh_user, "me");
        assert_eq!(c.wifi_pass, "");
    }

    #[test]
    fn parse_value_verbatim_after_first_equals() {
        let c = Conf::parse("TW_WIFI_PASS= pass=with=equals \n");
        assert_eq!(c.wifi_pass, " pass=with=equals ");
    }

    #[test]
    fn parse_strips_trailing_cr_only() {
        let c = Conf::parse("TW_WIFI_SSID=net\r\nTW_GH_USER=me\r\n");
        assert_eq!(c.wifi_ssid, "net");
        assert_eq!(c.gh_user, "me");
    }

    #[test]
    fn missing_required_reports_blank_required_fields() {
        let mut c = Conf::default();
        assert_eq!(
            c.missing_required(),
            vec![Field::WifiSsid, Field::RemoteUrl, Field::GhUser, Field::Token]
        );
        c.wifi_ssid = "net".into();
        c.remote_url = "you/notes".into();
        c.gh_user = "you".into();
        c.token = "tok".into();
        assert!(c.missing_required().is_empty());
        // Author + wifi pass are never required.
        assert_eq!(c.author_name, "");
    }

    #[test]
    fn expand_remote_url_shorthands() {
        for (input, want) in [
            ("you/notes", "https://github.com/you/notes.git"),
            ("you/notes.git", "https://github.com/you/notes.git"),
            ("github.com/you/notes", "https://github.com/you/notes.git"),
            ("git@github.com:you/notes", "https://github.com/you/notes.git"),
            (
                "ssh://git@github.com:22/you/notes",
                "https://github.com/you/notes.git",
            ),
            (
                "https://github.com/you/notes.git",
                "https://github.com/you/notes.git",
            ),
            ("http://gitea.local/you/notes", "http://gitea.local/you/notes"),
            ("nonsense", "nonsense"),
            ("you/notes/", "https://github.com/you/notes.git"),
        ] {
            assert_eq!(expand_remote_url(input), want, "input: {input}");
        }
    }
}
