# Postmortem — Onboarding wizard Wi-Fi bring-up boot loop (LoadProhibited)

**Date:** 2026-07-20
**Firmware:** 0.7.9
**Component:** `firmware/src/infrastructure/wizard_io.rs` (`run`), toolchain codegen
**Severity:** High — every card entering onboarding boot-looped; the device was unusable for any first-time / BYO-card user.
**Status:** Fixed and verified on device.

## Summary

Any card that entered the onboarding wizard — blank card, foreign card, bring-your-own card, a card with an incomplete `typoena.conf`, or an explicit `:setup` — sent the device into a reboot loop. The crash was a `LoadProhibited` Guru Meditation the instant the wizard tried to bring up the Wi-Fi radio, immediately after the "card erased — dedicated to Typoena" step.

The root cause was **not** in our logic and **not** in the Xtensa argument ABI, though it spent a long time looking like both. It was a **backend miscompilation of the wizard's enormous `run` function at `opt-level = "s"`**: the size optimizer emitted code that read a value out of a stack slot it never wrote. The fix is a one-line, surgically-scoped compiler setting — compile the `firmware` package at `opt-level = 2` — not any source change.

## Impact

- **Who:** every first-time user, and anyone re-running `:setup`. Already-provisioned cards (which skip the wizard) were unaffected, so this never showed up in day-to-day use or in `:gp`/`:gl` sync.
- **Failure mode:** hard boot loop (`rst:0xc RTC_SW_CPU_RST` — the panic handler resetting into the same crash), not a graceful failure. The device never reached the Wi-Fi network list.
- **Data:** none lost. On BYO cards the consent gate + card wipe worked correctly; the crash was strictly *after* that, at radio bring-up.

## The symptom

```
Guru Meditation Error: Core 0 panic'd (LoadProhibited). Exception was unhandled.
EXCCAUSE: 0x1c   EXCVADDR: 0xa5a5a5a5
```

Two facts made this diagnosable:

1. **`0xa5a5a5a5` is not random garbage.** `0xa5` is FreeRTOS's `tskSTACK_FILL_BYTE`, the pattern it paints fresh task stacks with. A load *from* (or *of*) `0xa5a5a5a5` means we dereferenced a pointer/value that lives in a stack slot that was allocated but **never written** — an uninitialized-stack read.
2. **It always landed at the same place:** the first thing the wizard does that touches a real object — cloning one of the esp-idf singletons on the way into `EspWifi::new`.

`EspSystemEventLoop` and `EspDefaultNvsPartition` are `Arc`-backed. `.clone()` on one is an `Arc::clone` that dereferences the `ArcInner` `NonNull` pointer. If that pointer was read from an unwritten stack slot, it *is* `0xa5a5a5a5`, and the atomic refcount bump faults. (The `Modem`/`WifiModem` peripheral is a ZST — cloning/reborrowing it never dereferences anything — which is why the modem argument *never* crashed even when its slot was poisoned. That was a red herring for most of the investigation.)

## Root cause

At `opt-level = "s"`, the Xtensa/LLVM backend miscompiled `wizard_io::run`. `run` is an **enormous** function (the entire wizard event loop, inlined helpers and all). Somewhere in its stack-slot allocation for argument spills and by-value struct copies, the optimizer emitted a read from a slot that no store ever populated. Whatever singleton landed in that slot was then `0xa5a5a5a5`, and the first `.clone()` of it faulted.

The size optimizer is exercised hardest on exactly the largest functions, and `run` is the largest in the crate. This is a backend defect, not undefined behavior in our Rust.

## The chase — why it took six device flashes

Each hypothesis below was a build → flash → `addr2line` cycle. The trap was that the crash **moved with every codegen change but never cleared** — which, in hindsight, was itself the diagnosis.

| # | Hypothesis | Change | Result |
|---|-----------|--------|--------|
| 1 | 84-byte by-value `Conf` (7 `String`s) spills `nvs`/`sys_loop` into unwritten Xtensa arg-spill slots | `start: &conf::Conf` (8 bytes, one slot) | Crash **moved** to `sys_loop.clone()`, did not clear |
| 2 | `esp_wifi_init` can't get a contiguous internal DMA block (largest free ~63 KB with the 96 KB clone worker resident) | `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP=y` | **No change** — a 0xa5 load is a stack read, not a failed alloc. Disproven as the cause. |
| 3 | Threading singleton *references* through the deep inlined loop materializes the bad slot | Own singletons by value; `#[inline(never)] build_radio` helper (mirrors the working `wifi_tls` spike); build radio lazily | Crash **moved** to `nvs.clone()` at the `build_radio` call (with `run` inlined into `main`) |
| 4 | Need *all* the levers: `#[inline(never)]` on `run` **and** `&Conf` **and** by-ref singletons | all three at once | Crash **moved earlier** — a compiler-generated `memcpy` from `0xa5` at loop entry (PC `0x42013e13`), *before any keypress*. `#[inline(never)]` **introduced a fresh** miscompilation. |

The by-value → by-ref swap in hypothesis #1 producing "crash moved, didn't clear" was already the tell. It took three more rounds to accept that no rearrangement of the source feeding the optimizer would fix an optimizer that mis-allocates stack slots.

The evidence that finally reframed it: the fault relocates in lock-step with **argument order, argument count, by-ref vs by-value, and inline hints** — every one of these is a codegen knob, none of them is a correctness knob for well-defined Rust. When behavior tracks the codegen path and not the program's meaning, the codegen path is broken.

## The fix

Stop compiling this crate at `opt-level = "s"`:

```toml
# firmware/Cargo.toml
[profile.release.package.firmware]
opt-level = 2
```

A per-package profile override recompiles the `firmware` crate (lib + bin) with a codegen path that emits the same straightforward source correctly, while the workspace crates and the bench bins (`sd_bench`, `wifi_tls`, `qc`) keep the size-tuned `"s"`. Cost: **+~55 KB** flashed image (2,259,776 → 2,314,736 B, 14.13% of the factory partition — negligible headroom impact).

Kept as belt-and-suspenders (and just better code): the lean signature — `start: &conf::Conf`, by-ref `sys_loop`/`nvs`, and the lazy `build_radio` helper. Removed: `#[inline(never)]` on `run` (only ever a workaround attempt; it *introduced* variant #4).

## Verification

Flashed 2026-07-20 onto an unconfigured card:

```
wizard: card erased — dedicated to Typoena
wizard: bringing up the Wi-Fi radio — internal DRAM free 165791 B (largest block 63488 B)
wifi:mode : sta (94:a9:90:d1:74:78)
wizard: scan found 5 network(s)
```

No Guru Meditation. The radio came up on the main task and the scan reached the network list — the point six argument-shuffling attempts never got to.

## What went well

- **`debug = 2` + `addr2line -f -i -C`** pinned every crash to an exact inlined source line. Without per-crash line resolution, the "it moves but never clears" pattern would have been invisible — this is what ultimately cracked it.
- **Recognizing the UX cost mid-chase:** an early eager-radio-build variant delayed the first wizard paint ~1–2 s; switched to a lazy build (radio comes up only on the first effect that needs it) so the fix didn't regress onboarding feel.

## What went wrong

- **Four hypotheses / six flashes chasing the ABI** before concluding "backend bug." The relocation after hypothesis #1 was sufficient evidence to pivot; instead three more ABI variants were generated.
- **`CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP` was added under a wrong causal story** (a 0xa5 load can never be a failed allocation). The flag is *kept* — internal DRAM is genuinely tight at radio bring-up (~63 KB largest free block) and routing Wi-Fi/LWIP to PSRAM is legitimate headroom — but its comment is corrected to say so.

## Lessons

1. **`LoadProhibited` on exactly `0xa5a5a5a5` = uninitialized-stack read** (FreeRTOS `tskSTACK_FILL_BYTE`). Treat that specific value as a diagnostic, not generic corruption.
2. **A fault that relocates predictably with every codegen change but never clears is a compiler-backend bug, not an ABI mistake.** The relocation *is* the evidence: you're moving where the optimizer parks a bad read, not fixing what feeds it. Stop rearranging source; change the codegen path.
3. **Enormous functions are miscompilation surface at aggressive size-opt.** The size optimizer is stressed hardest on the biggest functions — and `run` is the biggest.
4. **ZST peripherals are red herrings in an ABI hunt.** `Modem` never faulted from its poisoned slot because nothing dereferences a ZST; "the modem arg is fine" did not mean "the slot is fine."
5. **Per-package `opt-level` is the surgical lever** — fix one crate's codegen without paying size across the workspace.

## Action items

- [x] `[profile.release.package.firmware] opt-level = 2` — the fix.
- [x] Removed the temporary `debug = 2` from `[profile.release]` (crash resolved; addr2line no longer needed).
- [x] Corrected the `CONFIG_SPIRAM_TRY_ALLOCATE_WIFI_LWIP` comment in `sdkconfig.defaults` (kept the flag, fixed the story).
- [ ] **Run the rest of onboarding on device** — join network → device-flow auth → clone. The crash is fixed *to the scan*; the full flow past the network list is not yet re-verified end to end.
- [ ] **Recognition note for future size-opt miscompiles.** If a second one appears, consider `opt-level = 2` workspace-wide and/or a minimal repro filed upstream (rust-lang / esp-rs, Xtensa LLVM).
- [ ] **A wizard smoke test** (bench-qc fixture is the natural home): "enters wizard + radio scans" as a go/no-go would have caught this before a user hit the boot loop.

### Note on splitting `run`

Decomposing `run` into smaller functions is tempting for readability, but **it is not the fix and would not have prevented this** — new function boundaries *reintroduce* the argument-spill surface that hypotheses #1/#3 chased. The `opt-level = 2` override, not decomposition, is the correctness guarantee. Refactor `run` for clarity if desired, but keep the override.
