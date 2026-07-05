# Spike 7 (git push) — the ADR-004 kill-switch fired: gix can't push over HTTPS

> Date: 2026-07-05
> Status: **turned, not failed** — gix ruled out for the push path; pivoted to
> `libgit2` (`git2`) and proved the git mechanics on desktop. On-device build is
> the next gate.
>
> Context: Spike 7 in
> [`../v0.1-mvp-technical.md`](../v0.1-mvp-technical.md#hardware-bring-up-order),
> git impl [ADR-004](../adr.md#adr-004-git-implementation--gitoxide-gix), auth
> [ADR-005](../adr.md#adr-005-auth--https--github-personal-access-token).
> Spike program: [`../../spikes/spike7-git-push/`](../../spikes/spike7-git-push/).

## Summary

Spike 7 was written as the kill-switch for [ADR-004](../adr.md): *"the
smart-HTTP path is validated in spike 7 before we commit to integration; if it
fails on the device, we fall back to `libgit2-sys`."* It never needed a device
to fire. Before writing any gix code, gitoxide's own crate-status doc settles
the question: `gix` has send-pack/receive-pack **plumbing** (report-status,
sideband, delete-refs, atomic pushes) but supports push as a **workflow** only
over `file://` and `ssh://`. **Push over HTTP(S) is not implemented** — push is
still listed under "workflows that still need plumbing." (Clone/fetch, by
contrast, are robust over HTTP(S) — which is why Spike 6's TLS GET passed but
does not carry over to push.)

Because [ADR-005](../adr.md) fixes auth as **HTTPS + PAT**, `gix` cannot satisfy
the push path today. gix *can* push over `ssh://`, but that would (a) revisit
ADR-005 and (b) still die on device — gix's SSH transport spawns the external
`ssh` program, which does not exist on the ESP32. So the kill-switch condition
is met at the library level.

**Decision:** take the fallback the risk table already names — `libgit2` via the
[`git2`](https://docs.rs/git2) crate — keeping ADR-005 (HTTPS + PAT) intact.
Proved the full `add → commit → push` sequence on desktop
([`spikes/spike7-git-push`](../../spikes/spike7-git-push/)).

## Why not the alternatives

| Option | Verdict |
| ------ | ------- |
| **gix + HTTPS** (as ADR-004 intended) | Blocked — gix has no HTTP(S) push. |
| **gix + SSH push** | gix supports it, but revisits ADR-005 *and* gix's SSH transport shells out to an `ssh` binary absent on ESP32 → dead on device. |
| **gix-protocol send-pack + custom HTTPS transport** | Pure-Rust, no ADR change, but not smoke-test-sized: hand-wiring send-pack over an mbedtls HTTP transport is real work and unproven upstream. Reconsider only if the libgit2 cross-compile (below) turns out worse. |
| **libgit2 (`git2`)** ← chosen | The ADR's named fallback. Trivial on desktop; the risk becomes the on-device cross-compile. |

## What the desktop spike proves

Run live against a local `file://` bare remote (no credentials), exercising the
exact v0.1 `git` module contract:

- **first commit + push** from an unborn `HEAD` (fresh clone of an empty repo)
  → the commit lands in origin. Message is an ISO-8601 timestamp.
- **nothing to publish** → short-circuits when the staged tree matches `HEAD`.
- **divergence** → a second clone advances origin; the first clone's push is
  rejected, `pull --no-edit` merges cleanly (different files), the retry push
  succeeds, and origin ends with a correct two-parent merge commit.

Also confirmed 2026-07-05 against a **real GitHub repo** (`jcalixte/typoena-test`)
over HTTPS with a fine-grained PAT: `committed → push accepted by remote`, the
commit landed on GitHub. So the TLS handshake + PAT auth + smart-HTTP push all
work through libgit2's vendored stack (desktop links `openssl-sys` for TLS). The
one path still unexercised live is a **non-fast-forward rejection over HTTPS**
(the `push_update_reference` callback) — the `file://` transport surfaced that as
a `push()` error instead, and the GitHub push was a clean fast-forward.

Implementation notes that carry into the real module:

- **`git add --all` semantics.** libgit2's `index.add_all(["*"], DEFAULT)` stages
  new + modified + **deleted** paths, unlike a naive `git add .`. v0.5 file-delete
  needs removals to reach the next Publish's staged set — this is that behavior.
- **Push rejection is not always a `push()` error.** A non-fast-forward can come
  back as a transport `Err` (local transport did this) *or* silently via the
  `push_update_reference` callback with a status string while `push()` returns
  `Ok` (the HTTPS/GitHub path). The spike handles both and routes either to the
  pull-and-retry. The callback path is coded for but not yet exercised live.
- **PAT hygiene.** The token is handed only to libgit2's credential callback
  (`Cred::userpass_plaintext`) and never logged — matches ADR-005.

## What it does *not* prove — the next gate

The risk moved **with** the kill-switch, and arguably got harder. ADR-004 chose
gix *specifically to avoid* libgit2's C cross-compile to xtensa; falling back to
libgit2 re-introduces exactly that. The open question is now:

> Can `libgit2` (`git2` / `libgit2-sys`) cross-compile to
> `xtensa-esp32s3-espidf` and use esp-idf's **mbedtls** as its TLS backend?

`libgit2-sys` vendors libgit2 and, on desktop, pulled `openssl-sys` for TLS —
there is no openssl on esp-idf, so the device build will need libgit2 pointed at
mbedtls (its `MbedTLS` backend) via the esp-idf sysroot, which is unproven. This
is the on-device Spike 7 and it also depends on:

- **PSRAM** (`CONFIG_SPIRAM`) enabled — still off (only ~339 KB internal heap;
  see firmware README / Spike 6 note). libgit2's pack working set needs it.
- **A working SD card** (Spike 3, currently
  [paused on a CMD59-incompatible card](2026-07-05-spike3-sd-cmd59.md)) for the
  `/sd/repo` working copy.

So the full **SD → push** loop is still not testable on hardware; this spike
retired the *library/API* risk and replaced it with a *cross-compile* risk to
tackle once PSRAM + SD are unblocked.

## On-device probe — 2026-07-05

Two moves toward the on-device gate, decoupled from the (still-blocked) SD card:

**PSRAM enabled.** `sdkconfig.defaults` gained `CONFIG_SPIRAM=y` +
`CONFIG_SPIRAM_MODE_OCT=y` (the N16R8 is *octal* PSRAM — quad mode would fail
init) + `CONFIG_SPIRAM_USE_MALLOC=y` (adds PSRAM to the heap so large Rust allocs
land there). Speed left at 40 MHz for a safe first enable. Octal PSRAM uses
GPIO 33–37; the EPD/SD pins (4–13) avoid that range, so no wiring conflict.
**Confirmed on hardware 2026-07-05:** boot log shows `Found 8MB PSRAM device`
(vendor AP, gen-3, 64 Mbit die), `SPI SRAM memory test OK`, and `Adding pool of
8192K of PSRAM memory to heap allocator` — the full 8 MB joins the heap on top of
~372 KB internal DRAM. The editor boots and types normally, so PSRAM broke
nothing. The ~1.5 MB git working-set budget now has headroom.

**libgit2 cross-compile probe.** Added `git2` (default-features off → no
openssl/ssh, isolating the C build from the TLS question) + a throwaway
`git_probe` bin, and built for `xtensa-esp32s3-espidf`. Result reframes the risk
in a *more* encouraging direction than expected:

- **The C cross-compiles.** `xtensa-esp-elf-gcc` ran on libgit2's sources and
  several files built with `exit status: 0`. The feared "cmake can't target
  xtensa / no toolchain" failure did **not** happen.
- **The wall is missing esp-idf networking headers.** libgit2's core
  `git2_util.h` does `#include <arpa/inet.h>`, and the build died with
  `arpa/inet.h: No such file or directory`. Root cause: `libgit2-sys` builds
  vendored libgit2 as a **standalone `cc` library**, so it never inherits
  esp-idf's include paths — esp-idf's BSD-socket headers (lwIP) live under the
  esp-idf component tree, not the bare toolchain sysroot the `cc` invocation
  used. (`arpa/inet.h` *does* exist in esp-idf, via lwIP's POSIX compat layer;
  it just wasn't on the `-I` path.)

So the on-device libgit2 question is **not** "impossible," it's "needs esp-idf
integration": get the vendored C build to see esp-idf's lwIP/newlib includes.
Candidate paths, roughly in order of effort:

1. **Inject esp-idf include dirs into the `cc` build** via
   `CFLAGS_xtensa-esp32s3-espidf` (point `-I` at esp-idf's lwIP POSIX-compat
   headers). Cheapest to try; risk is a *cascade* — `arpa/inet.h` is likely the
   first of several missing headers, and then missing lwIP/pthread symbols at
   final link.
2. **Build libgit2 as a proper esp-idf component** (CMake component pulled into
   the esp-idf build so it inherits all component includes/libs). The "right"
   way; more plumbing, via esp-idf-sys's extra-components mechanism.
3. **Patch/fork `libgit2-sys`** to be esp-idf-aware (read `DEP_ESP_IDF_*` and add
   the include paths). Upstreamable but the most work.

TLS is a *separate* later step regardless: `libgit2-sys` has no mbedtls backend
(https → openssl only), so the plan remains libgit2 with networking off + a
**custom Rust smart-subtransport** reusing the Spike 6 esp-idf HTTPS client. But
the util-layer `arpa/inet.h` include is unconditional (not gated on the transport
backend), so path 1/2/3 is needed before even a transport-less build links.

## Follow-ups

- [x] Enable PSRAM (`CONFIG_SPIRAM`) — **done + hardware-verified 2026-07-05**
      (octal, USE_MALLOC): 8 MB detected, memory-tested, added to heap; editor
      still runs.
- [~] On-device Spike 7 libgit2 build — **probed 2026-07-05**: the C
      cross-compiles for xtensa; blocker is esp-idf networking includes
      (`arpa/inet.h`) missing from the standalone `cc` build (see "On-device
      probe" above). Next: inject esp-idf lwIP include paths (path 1), expect a
      header/symbol cascade, escalate to an esp-idf component (path 2) if it
      sprawls.
- [ ] Custom mbedtls-backed smart-subtransport (reuse Spike 6 HTTPS) once the
      library links.
- [x] Flash the PSRAM build and confirm the SPIRAM heap region — **done
      2026-07-05**: 8192K pool added to heap, memory test OK.
- [x] Run the desktop spike against a real GitHub test repo — **done 2026-07-05**
      (`jcalixte/typoena-test`, fine-grained PAT): HTTPS handshake + PAT auth +
      push confirmed. Still open: the `push_update_reference` rejection path over
      HTTPS (needs a non-fast-forward against a real remote to trigger it).
- [ ] Revise the `git` module section of the technical doc (it still describes
      gix crates/transport) once the device path is confirmed.

## Artifacts (this session)

- `spikes/spike7-git-push/` — the desktop spike crate (`src/main.rs`,
  `Cargo.toml`, `README.md`, `.env.example`).
- ADR-004 — outcome note appended (kill-switch fired → libgit2).
- `docs/v0.1-mvp-technical.md` — risk-table row updated (gix push → libgit2).
