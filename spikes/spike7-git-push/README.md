# Spike 7 — git push (desktop half)

> Bring-up spike 7 in
> [`../../docs/v0.1-mvp-technical.md`](../../docs/v0.1-mvp-technical.md#hardware-bring-up-order).
> Decision context: [ADR-004](../../docs/adr.md#adr-004-git-implementation--gitoxide-gix)
> (git impl) and [ADR-005](../../docs/adr.md#adr-005-auth--https--github-personal-access-token)
> (auth). Full write-up:
> [`../../docs/postmortems/2026-07-05-spike7-gix-https-push.md`](../../docs/postmortems/2026-07-05-spike7-gix-https-push.md).

Spike 7 proves the `add → commit → push` sequence the on-device `git` module
will run. Per the technical doc it's **desktop-Rust first, then on device** —
this crate is the desktop half. It is a **host** program (plain `stable`
toolchain), deliberately kept out of the xtensa-pinned `firmware/` crate.

## Headline finding: the ADR-004 kill-switch fired

Spike 7 is the documented kill-switch for [ADR-004](../../docs/adr.md): *"if
[gix smart-HTTP push] fails on the device, we fall back to `libgit2-sys`."*
It fires at the **library level**, before any device work: gitoxide's own
crate-status doc states `gix` supports push only over `file://` and `ssh://` —
**push over HTTP(S) is not implemented** (only clone/fetch are). Since
[ADR-005](../../docs/adr.md) fixes auth as HTTPS + PAT, `gix` cannot satisfy the
push path today. So this spike uses the fallback the risk table names:
`libgit2` via the [`git2`](https://docs.rs/git2) crate.

## What it does

Mirrors the v0.1 `git` module contract:

1. open the working copy
2. stage with `git add --all` semantics (**deletions propagate** — needed for
   v0.5 file-delete)
3. short-circuit when nothing is staged → "nothing to publish"
4. commit; author from config, message = an ISO-8601 timestamp (the time *is*
   the message)
5. push `HEAD` to `origin/<branch>` over HTTPS, PAT in the credential callback
   (**never logged**)
6. on push rejection (remote moved): fetch + `pull --no-edit` (fast-forward or a
   clean merge), then retry the push once; merge conflicts are fatal (surfaced,
   never auto-resolved)

## Verified (2026-07-05)

Run live against a local `file://` bare remote (no credentials):

- **first commit + push** from an unborn `HEAD` → lands in origin ✅
- **nothing to publish** short-circuits when the index matches `HEAD` ✅
- **divergence**: a second clone advances origin → push rejected → `pull
  --no-edit` merges cleanly → retry push succeeds, origin gets a two-parent
  merge commit ✅

- **real HTTPS + PAT push to github.com** — confirmed 2026-07-05 against
  `jcalixte/typoena-test`: `committed → push accepted by remote`, the commit
  landed on GitHub. The `git2` build links `openssl-sys` for the TLS transport.

Still not exercised: a **non-fast-forward rejection over HTTPS** (the
`push_update_reference` callback path) — the local `file://` transport surfaced
rejection as a `push()` error instead, and the GitHub push above was a clean
fast-forward. The callback path is coded for but unproven live.

## Not proven here (the next gate)

The risk moved *with* the kill-switch: **can `libgit2` cross-compile to
`xtensa-esp32s3-espidf` against esp-idf's mbedtls?** — the exact C cross-compile
pain gix was chosen to avoid. That is the next on-device spike, and it also
needs PSRAM (`CONFIG_SPIRAM`) enabled and a working SD card (Spike 3) for the
`/sd/repo` working copy. This desktop pass de-risks the API + git mechanics
only.

## Run

Local remote — proves the mechanics, no secrets:

```sh
mkdir -p /tmp/s7 && cd /tmp/s7
git init -q --bare origin.git && git clone -q origin.git work
echo hi > work/notes.md
cargo run --manifest-path <this-crate>/Cargo.toml -- "$PWD/work"
```

Real GitHub repo — proves HTTPS + PAT (use a throwaway repo + a `repo`-scoped
fine-grained PAT):

```sh
cp .env.example .env    # fill TW_GH_USER / TW_PAT / TW_REPO_PATH
set -a; . ./.env; set +a
cargo run -- "$TW_REPO_PATH"
```

`TW_REPO_PATH` must be a clone whose `origin` is the HTTPS URL (or set
`TW_REMOTE_URL` to point `origin` there). The PAT is passed to libgit2's
credential callback and never printed.
