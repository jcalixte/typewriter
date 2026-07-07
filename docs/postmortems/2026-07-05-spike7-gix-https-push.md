# Spike 7 (git push) — the ADR-004 kill-switch fired: gix can't push over HTTPS

> Date: 2026-07-05 (on-device push completed 2026-07-06)
> Status: **DONE** — gix ruled out for the push path; pivoted to `libgit2`
> (`git2`), proved the git mechanics on desktop, then landed the full
> `init → commit → push` over mbedTLS HTTPS **on hardware** (2026-07-06). See
> "On-device push COMPLETE" below.
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

The util-layer `arpa/inet.h` include is unconditional (not gated on the transport
backend), so path 1/2/3 is needed before even a transport-less build links.

### Path 1 attempted — 2026-07-05: confirmed a dead end

Injected esp-idf's lwIP + generated-config include dirs via
`CFLAGS_xtensa_esp32s3_espidf` and rebuilt the probe. `arpa/inet.h` **resolved**
— then the build immediately hit the next wall: `lwipopts.h → sys/ioctl.h: No
such file`, a header from a *different* esp-idf component (vfs/newlib), not lwIP.
That is the whole problem in one line: **path 1 peels the esp-idf component
include graph one component at a time**, with fragile absolute `-I` paths into
build-output dirs (the config-dir hash even changed between two builds). It does
not converge without effectively reconstructing esp-idf's entire per-component
include environment by hand.

### Decision: go straight to path 2 (libgit2 as an esp-idf component)

Path 2 isn't just the robust include fix — it **solves the TLS backend at the
same time**. libgit2 the C library *does* support mbedTLS (`USE_HTTPS=mbedTLS`);
only the `libgit2-sys` Rust wrapper lacks it. Building libgit2 as an esp-idf
CMake component lets us (a) inherit every component's includes + link (kills the
cascade) and (b) set `USE_HTTPS=mbedTLS` against esp-idf's own mbedtls — which
would make a **custom Rust subtransport unnecessary**. Two birds. Sketch:

- Add libgit2 via esp-idf-sys's extra-components mechanism (a `components/`
  dir + `CMakeLists.txt` declaring `REQUIRES lwip mbedtls pthread newlib vfs`,
  wrapping libgit2's own CMake with `USE_HTTPS=mbedTLS`, `USE_SSH=OFF`).
- Bind to it from Rust — either `libgit2-sys` in *system* mode pointing at the
  component-built lib, or hand-rolled bindings for the handful of calls the
  `git` module needs.

This is a real, multi-step chunk (component CMake + bindings + link), not a
flag-flip — scoped as the next work item, gated behind nothing now that PSRAM is
up.

## Follow-ups

- [x] Enable PSRAM (`CONFIG_SPIRAM`) — **done + hardware-verified 2026-07-05**
      (octal, USE_MALLOC): 8 MB detected, memory-tested, added to heap; editor
      still runs.
- [x] On-device Spike 7 libgit2 build — **probed + path 1 attempted 2026-07-05**:
      the C cross-compiles; the include cascade does not converge via CFLAGS
      injection (path 1 dead end). Decision: **path 2** (libgit2 as an esp-idf
      component with `USE_HTTPS=mbedTLS`).
- [x] Path 2: add libgit2 as an esp-idf component — **compiles AND links
      2026-07-05** (Gate A + Gate B). libgit2 1.9.4 built as a component with
      `REQUIRES mbedtls lwip pthread vfs newlib`; the include cascade vanished as
      predicted. mbedTLS wired directly (`GIT_MBEDTLS` + `GIT_SHA1/256_MBEDTLS`).
      A `git_smoke` bin calling `git_libgit2_init/version/shutdown` links clean:
      538 `git_*` functions in the ELF, +514 KB text. See "Path 2 result" below.
- [x] Flash the PSRAM build and confirm the SPIRAM heap region — **done
      2026-07-05**: 8192K pool added to heap, memory test OK.
- [x] Run the desktop spike against a real GitHub test repo — **done 2026-07-05**
      (`jcalixte/typoena-test`, fine-grained PAT): HTTPS handshake + PAT auth +
      push confirmed. Still open: the `push_update_reference` rejection path over
      HTTPS (needs a non-fast-forward against a real remote to trigger it).
- [x] On-device `init → commit → push` over mbedTLS HTTPS — **DONE +
      hardware-verified 2026-07-06** (see "On-device push COMPLETE").
- [x] Revise the `git` module section of the technical doc — **DONE 2026-07-07**
      (commit `2f2f122`): gix → libgit2/git2, transport settled, 96 KB stack,
      persistent-clone flow.
- [x] Real cert trust-store (drop the `certificate_check` bypass) — **DONE +
      hardware-verified 2026-07-06** (commit `2519ed8`; see "Shortcuts — status").
- [x] Settle the product sync transport — **DECIDED 2026-07-06: HTTPS + PAT**
      (on-device libgit2 is HTTPS-only; no libssh2 port).
- [~] Retire the last shortcut: no PAT-in-flash — **decision documented, deferred**
      as [ADR-011](../adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest)
      (open): on-device paste → eFuse-encrypted NVS + a per-device fine-grained PAT.
      Gates the first non-dev distribution; nothing to implement until then.
- [~] Fold the push into the editor's `git` module (persistent clone +
      fast-forward) over HTTPS+PAT — **increment A DONE + hardware-verified
      2026-07-07** (`git_sync.rs`, commit `afa61de`): `clone` + persistent `open`
      + fast-forward push proven on device. Remaining: **B** = divergence/merge
      path + the AM_RDO-clearing unlink shim (fetch/repack need read-only delete
      on FAT — POSIX chmod can't clear it, verified); **C** = lift the logic into
      a reusable `git` module wired to the editor's `Ctrl-G`. Storage caveat: the
      real notes repo is 3.9 GB / 562 MiB pack — needs shallow+sparse or a
      dedicated small repo (ADR-007), can't clone whole.
- [x] Move git to a dedicated large-stack task so the shared main-task stack (and
      the editor build) can drop back — **DONE + hardware-verified 2026-07-06**.
      `git_publish` now runs on its own `std::thread` (`GIT_STACK = 96 KB` via
      `Builder::stack_size`; main joins it), and `CONFIG_ESP_MAIN_TASK_STACK_SIZE`
      dropped 98304 → **12288** (the Spike-6 value proven with the editor +
      TLS-on-main). On-device push succeeded off-main — no panic/overflow, no
      ENOMEM on the spawn — retiring the "time()-only-works-on-main" misdiagnosis.

## Path 2 result — libgit2 compiles and links on xtensa (Gate A + Gate B)

The bet paid off. libgit2 **1.9.4** (the exact version `libgit2-sys 0.18.5`
vendors, chosen so `git2`'s safe Rust API can bind it in system mode later)
builds as an esp-idf component and links into a real image.

**Why a component beat Path 1.** Registering libgit2 with
`REQUIRES mbedtls lwip pthread vfs newlib` makes it inherit those components'
include + link graph. Path 1's manual CFLAGS injection died because resolving
one component's headers exposes the next (`arpa/inet.h` → `sys/ioctl.h` → …).
The component model walks that graph for us — the cascade never appeared.

**mbedTLS, not OpenSSL.** The libgit2-sys wrapper only offers
openssl/securetransport/winhttp, but the C library has an mbedTLS backend
(`streams/mbedtls.c`, `hash/mbedtls.c`). A hand-written `git2_features.h` selects
`GIT_HTTPS` + `GIT_MBEDTLS` + `GIT_SHA1_MBEDTLS` + `GIT_SHA256_MBEDTLS`, so TLS
and hashing reuse the mbedtls esp-idf already ships (and Spike 6 validated).

**The port surface was small** — four shims, libgit2 sources untouched (so we
never fork 1.9.4):

| Gap on esp-idf (picolibc + VFS) | Shim |
|---|---|
| no top-level `<poll.h>` (only `<sys/poll.h>`) | forwarding `poll.h` on the include path |
| `lstat` absent (no symlinks) | `#define lstat stat`, force-included via `esp_port.h` |
| `<sys/mman.h>` absent | `esp_map.c` — `p_mmap` via `git__malloc` + `read` (pack pages land in PSRAM) |
| `getuid`/`geteuid`/`getgid`/`getppid`/`getpgid`/`getsid`/`getpwuid_r`/`readlink`/`utimes` declared but not implemented | `esp_stubs.c` — single-root-user, no-user-db, no-symlink answers |

Also: gcc 14 promoted `-Wimplicit-function-declaration` /
`-Wincompatible-pointer-types` to hard errors; this pre-gcc14 C trips them
benignly, so the component downgrades them to warnings. `unix/process.c`
(fork/`sys/wait.h`) is excluded — only the SSH-exec transport we don't enable
uses it.

**Verification.** A throwaway `git_smoke` bin (`git_libgit2_init` /
`_version` / `_shutdown` via three hand externs) links with **zero undefined
references**: `nm` shows **538 `git_*` text symbols** in the ELF (`git_index_*`,
`git_repository_*`, `git_commit_*`, `git_remote_*`), the four shims present,
+514 KB `.text` (negligible against 16 MB flash).

**Gate C — RAN ON HARDWARE 2026-07-05.** Flashed `git_smoke` to the S3; the
linked library reports `1.9.4`, `git_libgit2_init() -> 1` (global init ran —
registers the mbedTLS stream + HTTP transport + hash backends),
`git_libgit2_shutdown() -> 0`, clean. No crash/assert/hang. So libgit2 +
mbedTLS **compiles, links, and executes** on the ESP32-S3 — the full Path 2
de-risk. Still unproven: an actual `repository_init` → `commit` → `push` over
mbedTLS HTTPS (needs Wi-Fi/SNTP from Spike 6 + a working-copy location).

**Build mechanics learned.** The component is wired via
`[[package.metadata.esp-idf-sys.extra_components]]` `component_dirs`, pointed at
on-disk source through a `LIBGIT2_SRC` env var (probe stage — not yet vendored).
esp-idf-sys emits no `rerun-if-*`, so editing the *root* Cargo.toml or the
component doesn't retrigger its build script once it has succeeded; forcing a
reconfigure means `rm -rf target/**/.fingerprint/esp-idf-sys-*` (cheap — the
159 MB cmake cache in the OUT_DIR persists, so only the changed component
recompiles).

**Gate D — `git2` safe-API binding LINKS 2026-07-05.** Replaced the hand
externs with the real path: the `git2` crate (default-features off, so no
openssl-sys/libssh2-sys) bound to our component via `libgit2-sys` in **system
mode** (`LIBGIT2_NO_VENDOR=1`). The trick: we don't want libgit2-sys to build
*or* link anything — esp-idf already links `liblibgit2.a` inside its component
group (verified in `build.ninja`: `esp-idf/libgit2/liblibgit2.a` sits in the
`libespidf.elf` `LINK_LIBRARIES`, and the group is repeated ~6× so libgit2's
refs to mbedtls/lwip resolve). So a **fake pkg-config with empty `Libs`**
(`firmware/pkgconfig/{libgit2,zlib}.pc`, found via `PKG_CONFIG_LIBDIR` +
`PKG_CONFIG_ALLOW_CROSS=1`) makes both libgit2-sys's and libz-sys's probes
succeed while emitting nothing; the symbols come from the component. `git_smoke`
now uses `git2::Version` + `Oid::hash_object` and links with zero undefined refs
— `nm` confirms `git_odb_hash`, `git_oid_tostr`, `git_error_last`, and
`mbedtls_sha1_starts` all **defined** in the ELF.

**Build gotcha (important):** esp-idf-sys forwards the app's link args only when
its build script reruns, and emits no `rerun-if-*`. After the component set
changes, the forwarded args go stale — `rm -rf
target/**/.fingerprint/esp-idf-sys-*` before building forces a fresh forward
(that was why the first git2 link failed with undefined `git_*`).

**Build-gating done:** `git2` is an optional dep behind the `git` feature, and
`git_smoke` has `required-features = ["git"]`, so the editor build never pulls
libgit2-sys/pkg-config. The component's CMake now registers *empty* when
`LIBGIT2_SRC` is unset, so `just build` (no env) still works.

**Open decisions before commit** (deliberately not done yet):

1. **Vendoring** — the component points at `~/.cargo`'s unpacked source via
   `LIBGIT2_SRC`; not reproducible. Needs a submodule pinned to `v1.9.4` (or a
   vendored copy). This also lets the `just flash-git` recipe drop the env vars.
2. **Component build burden** — `extra_components` still compiles all ~200
   libgit2 files on a clean build even for the editor (cached after; Rust side
   is already gated). Accept, or gate the C compile too.
3. ~~Runtime (Gate D on HW)~~ — **DONE 2026-07-05.** `just flash-git` on the S3:
   `git2 crate is talking to libgit2 1.9.4`, then `sha1(blob "hello") =
   b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0` + "hash matches" — i.e. git2 →
   libgit2 → mbedTLS SHA1 all ran correctly on device. Full chain proven.
4. ~~**The real thing** — `repository_init` → `commit` → `push` over mbedTLS
   HTTPS~~ — **DONE + hardware-verified 2026-07-06** (flash-FAT working copy);
   see "On-device push COMPLETE" below.

## On-device push COMPLETE — 2026-07-06

The real thing runs on hardware. `just flash-git-push` (bin
`firmware/src/bin/git_push.rs`, build `@a15789a`) did the whole loop on the
ESP32-S3 and pushed to GitHub over HTTPS:

```
init OK at /spiflash/wc-1783370910
wrote device.md
staged + tree written
committed to master
origin set; pushing refs/heads/master:refs/heads/device/1783370910
cert-check BYPASSED for github.com
push accepted by remote
✅ Spike 7 complete — pushed master → origin/device/1783370910 over mbedTLS HTTPS
```

Verified from both ends: the device logged `push accepted`, and `git ls-remote
https://github.com/jcalixte/typoena-test.git` independently shows
`refs/heads/device/1783370910` at commit `a96a7996`. So Wi-Fi/SNTP → mount
flash-FAT → `repository_init` → `add_all` → `commit` → smart-HTTP push + pack
upload + PAT auth all work on-device through libgit2 + esp-idf mbedTLS.

**Heap:** started at 8.44 MB free, min-ever **6.85 MB** — the whole TLS handshake
+ packfile build cost ~1.6 MB, all served from PSRAM; internal DRAM was never
stressed. **Timing:** first-boot FATFS format of the working-copy dir ~7.7 s,
commit sub-second, TLS handshake→accept ~6 s.

### Three bugs stood between "links + runs" and "pushes"

Gate D proved SHA1 on device; getting from there to a real push took three fixes
on the flash-FAT + FATFS path, each found on hardware. All are committed (5
microcommits through `a15789a`).

1. **Main task stack 12 KB → 96 KB.** libgit2 is stack-hungry: nearly every
   function puts a `char path[GIT_PATH_MAX]` (4 KB) buffer on the stack, and the
   `repository_init → config-write → FATFS → wear-leveling` chain nests ~10 of
   them — a *trivial config write* measured ~67 KB of stack. At 48 KB it
   overflowed and smashed an adjacent newlib lock handle → `LoadProhibited` in
   `xQueueGenericSend`. **This corrected an earlier misdiagnosis:** the "`time()`
   only works on the main task, not a std::thread" conclusion from the first
   on-device attempt was wrong — that thread had the *default 4 KB* stack, so the
   same deep chain just overflowed sooner. It was always stack depth, not
   thread-vs-main. (This stack has since moved: `sdkconfig.defaults` is shared
   with the editor build, so git was later given its OWN 96 KB `std::thread` and
   the main-task stack dropped back to 12 KB — see the follow-ups. The misdiagnosis
   is now doubly retired: the push runs fine off the main task.)

2. **`p_rename` = remove-then-rename** (`esp_stubs.c`). FATFS `f_rename` fails
   `EEXIST` if the target exists and FAT has no hardlinks, so libgit2's own
   `p_rename` (link-then-rename in `posix.c`) can't overwrite the
   `config`/`refs`/`HEAD`/`index` files its lock→commit sequence depends on. Ours
   drops the target then renames; `posix.c`'s original is compiled under a
   throwaway name via a file-scoped CMake `COMPILE_DEFINITIONS`, so ours is the
   `p_rename` every caller links. (Not crash-atomic, but FAT offers no atomic
   replace — acceptable for the working copy.) Verified on hardware: cleared the
   `failed to rename lockfile to '.git/config'` error.

3. **`utimes` existence-gate — the killer.** This one silently defeated every
   object write. Our first `utimes` stub returned `0` unconditionally ("VFS can't
   set times; ignore"). But libgit2's `git_futils_touch()` → `p_utimes()` is how
   the loose ODB's `freshen` probe answers *"does this object already exist?"*,
   and `git_odb_write()` (`odb.c:1629`) **skips the write entirely** when freshen
   succeeds. So a blanket `return 0` made freshen always report "exists" →
   libgit2 believed every object was already on disk → **every** blob/tree/commit
   write was silently dropped. `.git/objects/` stayed empty (only `info/` +
   `pack/`), and `write_tree` failed with `invalid object specified - device.md`.
   Fix: `stat`-gate the stub — present → `0` (setting the time is a cosmetic
   no-op we skip), absent → `-1`/`ENOENT`, so freshen correctly reports "not
   found" and the real write proceeds.

   Diagnosed with an in-binary A/B/C/D ODB probe — write an in-memory blob, a
   file blob, run `add_all`, then walk `.git/objects` — which showed `exists =
   false` for every OID and an empty objects dir, isolating it to the *write*
   path (not read, not mmap, not the index). The vendored `odb.c` /
   `odb_loose.c` / `futils.c` source then pinned it to the freshen→touch→utimes
   chain. **Lesson:** a "harmless" no-op POSIX stub is actively dangerous when a
   caller reads its return value as a semantic signal.

### Shortcuts — status

- **Cert verification — DONE, hardware-verified 2026-07-06 (commit `2519ed8`).**
  Was: `certificate_check` blanket-accepted the peer cert (MITM-open). Now: embed
  GitHub's roots (`firmware/src/bin/github_roots.pem` — USERTrust ECC/RSA +
  DigiCert G2/Global Root CA, extracted from the macOS root store), write them to
  `/spiflash/ca.pem`, and load them via `git2::opts::set_ssl_cert_file`
  (`GIT_OPT_SET_SSL_CERT_LOCATIONS`; `CONFIG_MBEDTLS_FS_IO=y` lets mbedtls fopen
  the file). The callback returns `CertificatePassthrough`, which the transport
  maps to `is_valid ? 0 : -1` (`httpclient.c:805`) → **fail-closed**. The push
  still landed, proving the chain validates against the embedded USERTrust ECC
  root on-device. Caveat: roots must be refreshed if GitHub rotates CAs; a product
  would prefer esp-idf's bundle via a custom subtransport (it can't reach
  libgit2's private mbedtls config without touching libgit2 sources).
- **PAT baked into flash — STILL STANDING** (ADR-005 spike shortcut). `build.rs`
  embeds `TW_PAT` in the git_push/git_sync image via `env!()` — plaintext in
  flash, extractable with `esptool read_flash`. Fine for the dev's bench unit,
  not for a shipped one. The provisioning + at-rest-hardening decision is now
  tracked as an **open** ADR: [ADR-011](../adr.md#adr-011-credential-provisioning--how-the-pat-reaches-the-device-and-is-protected-at-rest)
  (likely on-device paste → eFuse-encrypted NVS + a per-device fine-grained PAT).
- **Product sync transport — DECIDED 2026-07-06: HTTPS + PAT.** On-device libgit2
  is HTTPS-only (mbedTLS build; no ssh client, libssh2 unported), and the proven
  path is HTTPS+PAT, so the product keeps ADR-005 rather than porting SSH. The
  real project remote (`git@github.com:jcalixte/typewriter.git`, SSH) stays for
  desktop/human use; the device publishes over an HTTPS remote + token (stored
  securely, not in flash). No libssh2 port needed.

## Artifacts (this session)

- `spikes/spike7-git-push/` — the desktop spike crate (`src/main.rs`,
  `Cargo.toml`, `README.md`, `.env.example`).
- `firmware/components/libgit2/` — the esp-idf component (uncommitted probe):
  `CMakeLists.txt`, `git2_features.h`, `poll.h`, `esp_port.h`, `esp_map.c`,
  `esp_stubs.c`.
- `firmware/src/bin/git_smoke.rs` + Cargo.toml `[[bin]]`/`extra_components`
  (uncommitted probe wiring).
- ADR-004 — outcome note appended (kill-switch fired → libgit2).
- `docs/v0.1-mvp-technical.md` — risk-table row updated (gix push → libgit2).
