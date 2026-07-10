# Memory-safety audit — firmware Rust

> **Snapshot.** Audited at commit `371b542` on 2026-07-10. This is a
> point-in-time review of the `unsafe`/FFI surface; line numbers and the
> soundness arguments below are only valid against that tree. Re-run it after
> any change to `usb_kbd.rs`, the SD/git FFI, or an `esp-idf-sys` bump.

## Scope & method

Memory-unsafety in Rust can only originate in `unsafe`, so the audit focuses
there. The entire `unsafe` surface is FFI into ESP-IDF / libgit2, concentrated
in:

- `firmware/src/usb_kbd.rs` — by far the largest, and the only place with C
  callbacks, raw transfer buffers, and `slice::from_raw_parts`. **Highest
  risk.**
- `firmware/src/bin/{sd_fat,git_push,git_sync,wifi_tls}.rs` — descriptor-struct
  zeroing plus simple FFI calls.
- `firmware/src/epd.rs`, `firmware/src/editor.rs`,
  `spikes/spike7-git-push/` — **100% safe Rust, zero `unsafe`.** They cannot
  cause UB; their failure mode is panic→abort, not corruption.

Bottom line: the FFI code is careful, with real SAFETY reasoning and one
genuinely good defensive clamp. The audit found **one plausible true-UB path**
(conditional, ordering-dependent) plus a set of latent footguns and non-UB
robustness gaps. Nothing looks like a slam-dunk exploitable bug in normal
operation.

## Score: 8 / 10 (memory safety only)

- The design leans on safe Rust — only ~40 lines of genuine `unsafe`, all thin
  FFI wrappers, and the safe core (editor, framebuffer, layout) can't produce
  UB at all. Right architecture; most of the score.
- Every `unsafe` site carries real SAFETY reasoning, and the one place
  untrusted device data sizes a raw slice (`report_cb`) is correctly clamped.
- Not a 9–10 because finding #1 is closed by an *assumed* event ordering rather
  than by construction, and findings #2–#3 are latent, dependency-sensitive
  risks. Real memory safety means the invariant is *enforced*, not hoped for.
- Closing #1 so the in-flight invariant is explicit → 9. A 10 on FFI this heavy
  needs the structural guards in the "Regression testing" section.

This is a *memory-safety* score. Robustness (leaks on hot-plug) and correctness
would score separately and slightly lower.

## Findings

### 1. Possible use-after-free freeing the interrupt transfer on unplug — `usb_kbd.rs:176-182` (highest)

On `DEV_GONE`, `client_loop` frees `report_xfer` and closes the device:

```rust
if !report_xfer.is_null() {
    unsafe { usb_host_transfer_free(report_xfer) };   // line 177
    report_xfer = ptr::null_mut();
}
```

The interrupt-IN transfer is resubmitted on every completion (`report_cb`,
line 224), so it is **in-flight** most of the time. `report_cb` only fires from
inside `usb_host_client_handle_events` (line 159); the free happens *after* that
call returns. The code implicitly relies on the transfer's final
canceled-completion callback having already run in the same `handle_events`
batch that delivered `DEV_GONE`.

If the library delivers the `DEV_GONE` client event **before** the transfer's
cancellation callback, then either:

- `usb_host_transfer_free` refuses an in-flight transfer
  (`ESP_ERR_INVALID_STATE` — its return value is **ignored** here → silent
  leak), or
- a later `usb_host_client_handle_events` iteration invokes `report_cb` on the
  freed transfer → `let t = unsafe { &mut *transfer };` (line 219) is a
  **use-after-free**.

Ordering-dependent, so medium confidence rather than a definite always-fires
bug — but it's the one path in the codebase that can reach real UB, and it's
exactly the teardown race ESP-IDF's USB Host contract warns about (free only
when not in-flight). Verify against the library semantics rather than assuming
the batch ordering holds.

**Fix.** Halt/dequeue the endpoint and wait for the last completion callback
before freeing — or track an in-flight flag set on submit and cleared in
`report_cb`, and only free once it's clear (loop `handle_events` until then). At
minimum, check the return value of `usb_host_transfer_free` and don't null the
pointer / proceed to `device_close` while it reports the transfer busy.

### 2. `mem::zeroed()` / `MaybeUninit::zeroed().assume_init()` on bindgen structs is a latent footgun — `usb_kbd.rs:110,143`, `sd_fat.rs:138,173,192`

**Sound today**: every field of the zeroed descriptors is valid at all-zero (C
fn pointers are `Option<extern fn>` → `None`; floats → `0.0`; enums are `u32`
aliases; bools → `false`).

The risk is that this soundness is **invisible and unenforced**. `esp-idf-sys`
is pinned to a git branch (`[patch.crates-io]` in `Cargo.toml`), so a bindgen
regen that introduces a field where zero is an invalid bit pattern (a reference,
`NonNull`, or a niche enum) turns `assume_init()` into instant UB with no
compiler warning — the classic `zeroed`-on-FFI trap.

**Fix.** These structs are fully overwritten in their meaningful fields anyway.
Prefer keeping them as `MaybeUninit` and writing fields via `addr_of_mut!`, or
at least add a static/compile-time assertion (or a test) that pins the
zero-is-valid assumption so a dependency bump fails loudly.

### 3. Resource leaks on re-attach and on submit error — not UB — `usb_kbd.rs:163-168, 417/436, 449`

- A second keyboard attaching while one is open makes `setup_keyboard` overwrite
  `open_dev`/`report_xfer` (lines 164-167) without freeing/closing the previous
  ones → leaked transfer + device handle.
- `control_request`: if `usb_host_transfer_submit_control` errors, the `?` at
  line 430 returns before `usb_host_transfer_free(xfer)` (line 436) → leaked
  64-byte transfer. A submit failure in `start_report_polling` (line 458) leaks
  similarly.

Not memory-unsafe — worst case is heap exhaustion over many hot-plug cycles,
which matters for an always-powered appliance. Guard the re-attach case
(`if !open_dev.is_null()` → tear down first) and free-on-error in
`control_request`.

### 4. USB thread stack sizing is unverified — `usb_kbd.rs:121,132`

Daemon thread = 4096 B, client thread = 8192 B. The client thread runs
`report_cb → handle_report → enqueue → log::info!`, and formatting/logging is
stack-hungry; a FreeRTOS stack overflow is silent memory corruption unless the
canary/MPU check catches it. `git_push.rs` already reasons carefully about this
(96 KB, with a comment block on why); the USB threads deserve the same
measured-headroom treatment. Low confidence it's actually too small — measure
the high-water mark, don't change blindly.

### 5. `report_cb` bounds clamp — done right (noted, not a defect) — `usb_kbd.rs:221-222`

```rust
let n = (t.actual_num_bytes as usize).min(BOOT_REPORT_LEN);
let report = unsafe { core::slice::from_raw_parts(t.data_buffer, n) };
```

The one place device-controlled data sizes a raw slice. `.min(BOOT_REPORT_LEN)`
correctly clamps even a negative/garbage `actual_num_bytes` (the `i32 as usize`
blows up huge, `.min(8)` reins it back), and `handle_report` re-guards with
`report.len() < 3`. A malicious/broken keyboard can't overread the 8-byte
buffer here.

## Safe modules — no UB possible by construction

`editor.rs`, `epd.rs`, and the spike are safe Rust. Two invariants confirmed
rather than assumed:

- **`editor.rs` byte-indexing invariant holds.** It slices the buffer by byte
  offset treating it as a char index (`self.text[..self.caret]`,
  `text.as_bytes()[..]`). Valid only because the buffer is pure ASCII — and it
  is: the only source of `Key::Char` is `translate()` (`usb_kbd.rs:298`), which
  emits ASCII exclusively, and every internal insert (TAB, list markers, table
  formatting) is ASCII too. So byte == char holds and those slices can't hit a
  char-boundary panic. **When the v0.2 UTF-8 work lands, this invariant breaks
  into panics** — add a `debug_assert`/comment at the insert boundary then.
- **`epd.rs` slicing is bounded by its asserts.** `display_frame*` assert
  `fb.len() == FB_BYTES` and `y0 + h <= HEIGHT`, the row math stays within
  `FB_BYTES`, and the u16 arithmetic (`x+w-1`, `y0+h`) doesn't overflow given
  those bounds.

`git_push.rs`/`git_sync.rs`: the `RemoteCallbacks` closures capture
`Rc<RefCell<…>>` and run synchronously on the git thread during `remote.push` —
never sent across threads, so no `Send`/aliasing hazard.
`git2::opts::set_ssl_cert_file` (line 267) is `unsafe` because it sets a
process-global; called once in single-threaded setup before the git thread
spawns — sound.

## Regression testing

The honest constraint first: **the on-target binary can't be run under
Miri/ASAN** (`target_os = "espidf"`, all `unsafe` is FFI). So the strategy is
split by what's reachable where, ranked by leverage.

1. **Make the pure logic host-testable (highest leverage).** The functions that
   take untrusted input or do the slicing are FFI-free: `translate`,
   `handle_report`'s decode, the editor text ops, `changed_rows` /
   `only_adds_ink` in `main.rs`, the `epd` row math. Pull them into a
   no-esp-deps module/crate (workspace member or `#[cfg]`-gated) so `cargo test`
   runs on host. Then:
   - **Fuzz `handle_report` on host under Miri or ASAN** — the single most
     valuable test. It's the exact path where a broken/malicious keyboard's
     bytes meet `from_raw_parts` + slicing; feed arbitrary `&[u8]` and Miri
     catches any OOB the clamp fails to prevent. Guards finding #5.
   - Unit-test that `translate` never emits a non-ASCII `char`, pinning the
     invariant `editor.rs` byte-indexing depends on.
2. **Compile-time guards for the `zeroed()` assumption (#2).** Static assertions
   (or a test constructing the struct and checking a sentinel field is
   `None`/`0`) so an `esp-idf-sys` bump fails loudly instead of going silently
   UB.
3. **`clippy` as a ratchet.** `#![warn(clippy::undocumented_unsafe_blocks)]` +
   `clippy::multiple_unsafe_ops_per_block`, deny-warnings in CI. Forces every
   new `unsafe` to carry a SAFETY comment — keeps the existing discipline from
   eroding.
4. **On-device tests for what only exists on device (#1, #3, #4).**
   - **Hot-plug stress loop**: attach/detach ~100× on a bench script, log
     `esp_get_free_heap_size` each cycle. A downward trend proves the leaks
     (#3); a crash/`LoadProhibited` on the freed transfer proves the UAF (#1).
   - **Stack high-water mark**: `uxTaskGetStackHighWaterMark` on the USB
     threads, asserted in a debug build, guards #4.
5. **Fix #1 by making it impossible, not just tested.** The best defense is an
   in-flight flag set on submit and cleared in `report_cb`, with a
   `debug_assert!(!in_flight)` before the free. Any future change that
   reintroduces the race trips the assert in the hot-plug loop instead of
   corrupting memory in the field.

Priority if you only do some: **1 (fuzz `handle_report` under Miri) + 5
(in-flight flag)** cover the two real memory-safety concerns; 2 and 3 are cheap
insurance against dependency bumps; 4 is the only way to regression-test the
on-device races and is worth it for an always-powered appliance.
