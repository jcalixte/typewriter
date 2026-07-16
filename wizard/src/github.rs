//! GitHub device-flow protocol — the pure half.
//!
//! Everything string-in/string-out so it host-tests without a network: the
//! firmware driver (`wizard_io`) owns transport (EspHttpConnection over the
//! esp-idf cert bundle) and feeds bodies through these parsers. Ported from
//! `installer/src/auth.rs`, the proven host implementation of the same flow.
//!
//! GitHub's OAuth endpoints answer `application/x-www-form-urlencoded` unless
//! asked otherwise — a few lines to parse, no JSON dependency on the flow
//! itself. Identity (`GET /user`) is JSON, parsed with serde_json.

/// The Typoena GitHub App's client id. Public by design (it names the app,
/// like a username — the device flow needs no secret on the client).
pub const CLIENT_ID: &str = "Iv23liwgnE86ITDpBdnn";

pub const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
pub const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
pub const USER_URL: &str = "https://api.github.com/user";

/// Where the user grants the app access to repos. Signing in only proves
/// identity — the token can't see a repo until the app is *installed* on it.
pub const APP_INSTALL_URL: &str = "https://github.com/apps/typoena/installations/new";

/// The app installations the signed-in user can see (device-flow user token).
pub const INSTALLATIONS_URL: &str = "https://api.github.com/user/installations?per_page=100";

/// Repos one installation grants the user. Per-installation (not `/user/repos`)
/// so the list is exactly what the app can actually reach — what the clone can
/// use. `per_page=100` covers any realistic grant (the app is installed on a
/// handful of hand-picked repos), truncating silently past that.
pub fn installation_repos_url(id: u64) -> String {
    format!("https://api.github.com/user/installations/{id}/repositories?per_page=100")
}

/// Body for `POST login/device/code`. All values are URL-safe literals.
pub fn device_code_body() -> String {
    format!("client_id={CLIENT_ID}")
}

/// Body for `POST login/oauth/access_token` while polling.
pub fn poll_body(device_code: &str) -> String {
    format!(
        "client_id={CLIENT_ID}&device_code={device_code}&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
    )
}

/// The `login/device/code` grant: what to show, what to poll with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Poll no faster than this (seconds).
    pub interval_secs: u64,
    /// The code dies this many seconds from issue.
    pub expires_in_secs: u64,
}

pub fn parse_device_code(body: &str) -> Result<DeviceCode, String> {
    let fields = parse_form(body);
    if let Some(e) = form_error(&fields) {
        return Err(e);
    }
    Ok(DeviceCode {
        device_code: get(&fields, "device_code")
            .ok_or("GitHub sent no device code - try again")?,
        user_code: get(&fields, "user_code").ok_or("GitHub sent no user code - try again")?,
        verification_uri: get(&fields, "verification_uri")
            .unwrap_or_else(|| "https://github.com/login/device".to_string()),
        interval_secs: num(&fields, "interval").unwrap_or(5),
        expires_in_secs: num(&fields, "expires_in").unwrap_or(900),
    })
}

/// One poll of the token endpoint.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Poll {
    /// Authorized — the `ghu_` user token.
    Token(String),
    /// Not approved yet; keep the current pace.
    Pending,
    /// GitHub asks for a slower pace (seconds).
    SlowDown(u64),
    /// Terminal: expired code, access denied, or anything unrecognized.
    Failed(String),
}

pub fn parse_poll(body: &str) -> Poll {
    let fields = parse_form(body);
    if let Some(token) = get(&fields, "access_token") {
        return Poll::Token(token);
    }
    match get(&fields, "error").as_deref() {
        Some("authorization_pending") => Poll::Pending,
        Some("slow_down") => Poll::SlowDown(num(&fields, "interval").unwrap_or(10)),
        Some(_) => Poll::Failed(form_error(&fields).unwrap_or_else(|| "GitHub refused".into())),
        None => Poll::Failed("unexpected response from GitHub".into()),
    }
}

/// Identity from `GET /user` (JSON): `(login, name, email)`. `name`/`email`
/// are often null for privacy — returned blank; the wizard falls back to the
/// login / noreply address.
pub fn parse_user(json: &str) -> Result<(String, String, String), String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("bad /user reply: {e}"))?;
    let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    let login = s("login");
    if login.is_empty() {
        return Err("GitHub /user reply carried no login".into());
    }
    Ok((login, s("name"), s("email")))
}

/// Installation ids from `GET /user/installations`.
pub fn parse_installation_ids(json: &str) -> Result<Vec<u64>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("bad /user/installations reply: {e}"))?;
    let arr = v
        .get("installations")
        .and_then(|x| x.as_array())
        .ok_or("no installations in reply")?;
    Ok(arr
        .iter()
        .filter_map(|i| i.get("id").and_then(|x| x.as_u64()))
        .collect())
}

/// `(full_name, size_kb)` pairs from an installation's repositories list.
/// `size` is GitHub's repo size in kilobytes — the input to the wizard's gate.
pub fn parse_repos(json: &str) -> Result<Vec<(String, u64)>, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("bad repositories reply: {e}"))?;
    let arr = v
        .get("repositories")
        .and_then(|x| x.as_array())
        .ok_or("no repositories in reply")?;
    Ok(arr
        .iter()
        .filter_map(|r| {
            let full = r.get("full_name")?.as_str()?.to_string();
            let size = r.get("size").and_then(|x| x.as_u64()).unwrap_or(0);
            Some((full, size))
        })
        .collect())
}

/// `error_description` > `error`, like the installer.
fn form_error(fields: &[(String, String)]) -> Option<String> {
    get(fields, "error_description")
        .or_else(|| get(fields, "error"))
        .map(|e| e.replace('+', " "))
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

fn parse_form(body: &str) -> Vec<(String, String)> {
    body.trim()
        .split('&')
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
                        out.push(bytes[i]);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_code_reply_parses() {
        let body = "device_code=dc123&user_code=ABCD-1234&verification_uri=https%3A%2F%2Fgithub.com%2Flogin%2Fdevice&expires_in=899&interval=5";
        let dc = parse_device_code(body).unwrap();
        assert_eq!(dc.device_code, "dc123");
        assert_eq!(dc.user_code, "ABCD-1234");
        assert_eq!(dc.verification_uri, "https://github.com/login/device");
        assert_eq!(dc.interval_secs, 5);
        assert_eq!(dc.expires_in_secs, 899);
    }

    #[test]
    fn device_code_error_surfaces_description() {
        let body = "error=unauthorized_client&error_description=The+client+is+not+authorized";
        assert_eq!(
            parse_device_code(body).unwrap_err(),
            "The client is not authorized"
        );
    }

    #[test]
    fn poll_outcomes() {
        assert_eq!(
            parse_poll("error=authorization_pending&error_description=x"),
            Poll::Pending
        );
        assert_eq!(parse_poll("error=slow_down&interval=12"), Poll::SlowDown(12));
        assert_eq!(
            parse_poll("access_token=ghu_tok&token_type=bearer&scope="),
            Poll::Token("ghu_tok".into())
        );
        assert!(matches!(parse_poll("error=expired_token"), Poll::Failed(_)));
        assert!(matches!(parse_poll("weird"), Poll::Failed(_)));
    }

    #[test]
    fn user_json_parses_with_null_fallbacks() {
        let (login, name, email) =
            parse_user(r#"{"login":"you","name":null,"email":null,"id":1}"#).unwrap();
        assert_eq!(login, "you");
        assert_eq!(name, "");
        assert_eq!(email, "");
        let (_, name, email) =
            parse_user(r#"{"login":"you","name":"You N.","email":"y@x.com"}"#).unwrap();
        assert_eq!(name, "You N.");
        assert_eq!(email, "y@x.com");
        assert!(parse_user(r#"{"id":1}"#).is_err());
        assert!(parse_user("not json").is_err());
    }

    #[test]
    fn installation_ids_parse() {
        let json = r#"{"total_count":2,"installations":[{"id":11,"x":1},{"id":22}]}"#;
        assert_eq!(parse_installation_ids(json).unwrap(), vec![11, 22]);
        assert!(parse_installation_ids("nope").is_err());
        assert!(parse_installation_ids(r#"{"x":1}"#).is_err());
    }

    #[test]
    fn repos_parse_full_name_and_size() {
        let json = r#"{"total_count":2,"repositories":[
            {"full_name":"you/notes","size":420,"private":true},
            {"full_name":"you/big","size":574000}
        ]}"#;
        assert_eq!(
            parse_repos(json).unwrap(),
            vec![
                ("you/notes".to_string(), 420),
                ("you/big".to_string(), 574000)
            ]
        );
        // A repo missing `size` defaults to 0 (never wrongly gated out).
        assert_eq!(
            parse_repos(r#"{"repositories":[{"full_name":"a/b"}]}"#).unwrap(),
            vec![("a/b".to_string(), 0)]
        );
        assert!(parse_repos("nope").is_err());
    }

    #[test]
    fn poll_body_is_form_encoded() {
        let b = poll_body("dc123");
        assert!(b.contains("client_id=Iv23liwgnE86ITDpBdnn"));
        assert!(b.contains("device_code=dc123"));
        assert!(b.contains("grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"));
    }
}
