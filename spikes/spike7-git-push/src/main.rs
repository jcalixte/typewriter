//! Spike 7 (desktop half) — `libgit2` add → commit → push over HTTPS + PAT.
//!
//! ## Why libgit2 and not gitoxide
//!
//! [ADR-004] chose `gix` and named Spike 7 its kill-switch: "if [gix smart-HTTP
//! push] fails on the device, we fall back to `libgit2-sys`." Before writing a
//! line of gix code, gitoxide's own crate-status doc settles it — gix has
//! send-pack *plumbing* but supports push only over `file://` and `ssh://`;
//! **push over HTTP(S) is not implemented**. Since [ADR-005] fixes auth as
//! HTTPS + PAT, the kill-switch condition is met at the library level, so this
//! spike proves the fallback: `libgit2` via the `git2` crate.
//!
//! ## What this proves (desktop)
//!
//! The exact sequence the on-device `git` module will run (v0.1 technical →
//! `git` module), against a real repository:
//!
//!   1. open the working copy
//!   2. stage everything — `git add --all` semantics, so deletions propagate
//!      too (matters for v0.5 file-delete → next Push's staged set)
//!   3. short-circuit when nothing is staged ("nothing to push")
//!   4. commit with the configured author, message = an ISO-8601 timestamp
//!   5. push HEAD to `origin/<branch>` over HTTPS with a PAT in the credential
//!      callback (never logged)
//!   6. on push rejection (remote moved): fetch + `pull --no-edit`
//!      (fast-forward or a clean merge), then retry the push once
//!
//! ## What it does NOT prove
//!
//! The real remaining risk moved with the kill-switch: **can libgit2
//! cross-compile to `xtensa-esp32s3-espidf` against esp-idf's mbedtls?** That
//! is the next on-device gate — the very cross-compile pain gix was picked to
//! avoid. This desktop pass de-risks the *API and the git mechanics*; the
//! device build is a separate spike.
//!
//! ## Run
//!
//! Local remote (no credentials — proves the mechanics end to end):
//!   cargo run -- /path/to/working-copy      # origin is a file:// bare repo
//!
//! Real GitHub repo (proves HTTPS + PAT):
//!   set -a; . ./.env; set +a
//!   cargo run -- "$TW_REPO_PATH"
//!
//! [ADR-004]: ../../docs/adr.md
//! [ADR-005]: ../../docs/adr.md

use std::cell::RefCell;
use std::rc::Rc;

use anyhow::{bail, Context, Result};
use chrono::Utc;
use git2::{
    build::CheckoutBuilder, Commit, Cred, CredentialType, FetchOptions, IndexAddOption,
    PushOptions, RemoteCallbacks, Repository, Signature,
};

/// Runtime config, mirroring the build-time `TW_*` env the firmware bakes in.
struct Config {
    repo_path: String,
    /// GitHub username + PAT for HTTPS basic auth. Both `None` → a local
    /// (`file://`) remote that needs no credentials.
    gh_user: Option<String>,
    pat: Option<String>,
    author_name: String,
    author_email: String,
    /// Optional override for `origin`'s URL (the HTTPS remote baked in at build
    /// time). Unset → use the working copy's existing `origin` as-is.
    remote_url: Option<String>,
}

impl Config {
    fn from_env_and_args() -> Result<Self> {
        let repo_path = std::env::args()
            .nth(1)
            .or_else(|| non_empty("TW_REPO_PATH"))
            .context("no working copy: pass a path as argv[1] or set TW_REPO_PATH")?;
        Ok(Self {
            repo_path,
            gh_user: non_empty("TW_GH_USER"),
            pat: non_empty("TW_PAT"),
            author_name: non_empty("TW_AUTHOR_NAME").unwrap_or_else(|| "Typoena".into()),
            author_email: non_empty("TW_AUTHOR_EMAIL")
                .unwrap_or_else(|| "typoena@example.com".into()),
            remote_url: non_empty("TW_REMOTE_URL"),
        })
    }

    fn signature(&self) -> Result<Signature<'static>> {
        Signature::now(&self.author_name, &self.author_email).context("building commit signature")
    }
}

/// An env var, treated as absent when empty (build.rs emits "" for unset vars,
/// so the firmware uses the same convention).
fn non_empty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}

fn main() {
    match run() {
        Ok(summary) => println!("✅ Spike 7 (desktop) complete — {summary}"),
        Err(e) => {
            eprintln!("❌ Spike 7 (desktop) failed: {e:?}");
            std::process::exit(1);
        }
    }
}

fn run() -> Result<String> {
    let cfg = Config::from_env_and_args()?;
    let repo = Repository::open(&cfg.repo_path)
        .with_context(|| format!("opening working copy at {}", cfg.repo_path))?;
    log(&format!("opened {}", cfg.repo_path));

    if let Some(url) = &cfg.remote_url {
        ensure_origin(&repo, url)?;
    }

    // Stage + commit, or short-circuit.
    let branch = match commit_if_changes(&repo, &cfg)? {
        Some(b) => b,
        None => return Ok("nothing to push (index matches HEAD)".into()),
    };
    let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

    // Push, with one pull-and-retry on rejection (the remote-moved case).
    match push_once(&repo, &cfg, &refspec) {
        Ok(()) => Ok(format!("pushed {branch} to origin")),
        Err(first) => {
            log(&format!(
                "push rejected ({first}); pull --no-edit then retry once"
            ));
            pull_no_edit(&repo, &cfg, &branch).context("pull --no-edit after rejected push")?;
            push_once(&repo, &cfg, &refspec).context("retry push after pull")?;
            Ok(format!("pushed {branch} to origin (after pull + merge)"))
        }
    }
}

/// Point `origin` at `url`, creating the remote if the working copy has none.
fn ensure_origin(repo: &Repository, url: &str) -> Result<()> {
    match repo.find_remote("origin") {
        Ok(_) => repo
            .remote_set_url("origin", url)
            .context("remote_set_url origin")?,
        Err(_) => {
            repo.remote("origin", url)
                .context("creating origin remote")?;
        }
    }
    log(&format!("origin → {url}"));
    Ok(())
}

/// Stage everything and commit, or return `None` if the index already matches
/// HEAD (nothing to push). Returns the branch shorthand on commit.
fn commit_if_changes(repo: &Repository, cfg: &Config) -> Result<Option<String>> {
    // `add_all(["*"], …)` is libgit2's `git add --all <pathspec>`: it stages
    // new + modified + **deleted** paths, which plain `git add .` would miss on
    // removals. The git module needs this for v0.5 file-delete.
    let mut index = repo.index().context("opening index")?;
    index
        .add_all(["*"], IndexAddOption::DEFAULT, None)
        .context("staging (add --all)")?;
    index.write().context("writing index")?;
    let tree_oid = index.write_tree().context("writing tree from index")?;

    // Parent = current HEAD, or None on an unborn branch (fresh repo).
    let parent: Option<Commit> = match repo.head() {
        Ok(h) => Some(h.peel_to_commit().context("resolving HEAD to a commit")?),
        Err(_) => None,
    };

    // Nothing to push: index tree == HEAD tree (or empty tree, unborn repo).
    match &parent {
        Some(p) if p.tree_id() == tree_oid => return Ok(None),
        None if repo.find_tree(tree_oid)?.is_empty() => return Ok(None),
        _ => {}
    }

    let tree = repo.find_tree(tree_oid)?;
    let sig = cfg.signature()?;
    let message = Utc::now().to_rfc3339(); // the ISO-8601 timestamp *is* the message
    let parents: Vec<&Commit> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &message,
        &tree,
        parents.as_slice(),
    )
    .context("committing")?;

    let branch = repo
        .head()?
        .shorthand()
        .context("HEAD has no branch shorthand (detached?)")?
        .to_string();
    log(&format!("committed to {branch}: \"{message}\""));
    Ok(Some(branch))
}

/// Push `refspec` to `origin`. Errors both on transport failure and on a
/// server-side ref rejection (non-fast-forward) — the caller pulls and retries.
fn push_once(repo: &Repository, cfg: &Config, refspec: &str) -> Result<()> {
    let mut remote = repo
        .find_remote("origin")
        .context("remote 'origin' not found — is the working copy a clone?")?;

    // Server-side per-ref status arrives via this callback, NOT as a push()
    // error, so record any non-None status and treat it as a failure below.
    let rejections = Rc::new(RefCell::new(Vec::<String>::new()));
    let mut cbs = RemoteCallbacks::new();
    bind_credentials(&mut cbs, cfg);
    {
        let rejections = rejections.clone();
        cbs.push_update_reference(move |refname, status| {
            if let Some(msg) = status {
                rejections.borrow_mut().push(format!("{refname}: {msg}"));
            }
            Ok(())
        });
    }

    let mut opts = PushOptions::new();
    opts.remote_callbacks(cbs);
    remote
        .push(&[refspec], Some(&mut opts))
        .context("push transport")?;

    let rej = rejections.borrow();
    if !rej.is_empty() {
        bail!("remote rejected: {}", rej.join("; "));
    }
    log("push accepted by remote");
    Ok(())
}

/// `git pull --no-edit`: fetch `origin/<branch>`, then fast-forward or make a
/// merge commit. Bails on conflicts — the device surfaces that, never auto-
/// resolves.
fn pull_no_edit(repo: &Repository, cfg: &Config, branch: &str) -> Result<()> {
    let mut remote = repo.find_remote("origin")?;
    let mut cbs = RemoteCallbacks::new();
    bind_credentials(&mut cbs, cfg);
    let mut fo = FetchOptions::new();
    fo.remote_callbacks(cbs);
    remote
        .fetch(&[branch], Some(&mut fo), None)
        .context("fetch origin")?;

    let fetch_head = repo
        .find_reference("FETCH_HEAD")
        .context("no FETCH_HEAD after fetch")?;
    let fetched = repo.reference_to_annotated_commit(&fetch_head)?;
    let (analysis, _) = repo.merge_analysis(&[&fetched])?;

    if analysis.is_up_to_date() {
        log("already up to date with origin");
        return Ok(());
    }

    if analysis.is_fast_forward() {
        let refname = format!("refs/heads/{branch}");
        repo.find_reference(&refname)?
            .set_target(fetched.id(), "spike7: fast-forward")?;
        repo.set_head(&refname)?;
        repo.checkout_head(Some(CheckoutBuilder::new().force()))?;
        log("fast-forwarded to origin");
        return Ok(());
    }

    // True divergence → merge. Conflicts are fatal here (no auto-resolve).
    repo.merge(&[&fetched], None, None).context("merge")?;
    let mut idx = repo.index()?;
    if idx.has_conflicts() {
        repo.cleanup_state().ok();
        bail!("merge conflicts on pull — resolve manually; Push will not auto-resolve");
    }
    let tree = repo.find_tree(idx.write_tree()?)?;
    let sig = cfg.signature()?;
    let head_commit = repo.head()?.peel_to_commit()?;
    let their_commit = repo.find_commit(fetched.id())?;
    repo.commit(
        Some("HEAD"),
        &sig,
        &sig,
        &format!("Merge origin/{branch}"),
        &tree,
        &[&head_commit, &their_commit],
    )
    .context("creating merge commit")?;
    repo.cleanup_state()?;
    log("merged origin (no conflicts)");
    Ok(())
}

/// Wire the credential callback: HTTPS → PAT (user/pass plaintext), local
/// remotes → default. The PAT is handed to libgit2 but never printed.
fn bind_credentials<'a>(cbs: &mut RemoteCallbacks<'a>, cfg: &'a Config) {
    cbs.credentials(move |_url, username_from_url, allowed| {
        // GitHub over HTTPS asks for USER_PASS_PLAINTEXT; the PAT is the
        // password (any non-empty username works, we send the configured one).
        if allowed.contains(CredentialType::USER_PASS_PLAINTEXT) {
            if let (Some(user), Some(pat)) = (cfg.gh_user.as_deref(), cfg.pat.as_deref()) {
                return Cred::userpass_plaintext(user, pat);
            }
        }
        // file:// / already-authenticated transports need nothing.
        if allowed.contains(CredentialType::DEFAULT) {
            return Cred::default();
        }
        if let Some(user) = username_from_url {
            return Cred::username(user);
        }
        Err(git2::Error::from_str(
            "no usable credentials — set TW_GH_USER and TW_PAT for an HTTPS remote",
        ))
    });
}

/// Step log to stdout. Deliberately plain (this is a bench tool, not firmware);
/// it never touches `cfg.pat`.
fn log(msg: &str) {
    println!("spike7: {msg}");
}
