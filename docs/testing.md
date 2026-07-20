# Testing conventions

> How Rust tests are laid out across the host-testable crates (`editor`,
> `keymap`, `display`). All of these build and run off the xtensa target —
> the firmware itself is exercised on hardware, not `cargo test`.
>
> Project overview: [`../README.md`](../README.md) · doc index:
> [`README.md`](README.md).

## Two shapes, chosen by what the test exercises

**Unit tests live in the source file.** A `#[cfg(test)] mod tests` block at the
bottom of the `.rs` file. It compiles only under `cargo test` (zero cost in the
shipped binary — which matters on a 16 MB part), and it gets private access to
the module it sits in. This is the default. `keymap/src/lib.rs` is the worked
example: 28 tests inline under the decoder they cover.

**Behavioural tests live in a `src/tests/` submodule.** When a test drives the
_whole_ `Editor` through `Key` events and asserts on private state — rather than
calling one function in isolation — it isn't really a unit test of any single
module. Those go in a `#[cfg(test)] mod tests;` **directory**, one file per
theme. The `editor` crate is laid out this way.

## The `editor` crate's `src/tests/`

- [`mod.rs`](../editor/src/tests/mod.rs) is the sole `#[cfg(test)] mod tests;`
  (declared at the bottom of [`lib.rs`](../editor/src/lib.rs)). It holds **every
  shared helper** (`typed`, `command`, `kinds`, `over`, `palette_editor`, …) and
  the fixture consts, and it re-exports the crate root with
  `pub(crate) use crate::*;` so each theme file reaches `Editor`, `Key`,
  `Effect`, … through a single `use super::*;`.
- Each theme file (`editing.rs`, `visual.rs`, `palette.rs`, `search.rs`, …) is
  just `#[test]` fns plus that one `use super::*;`. No file re-imports the crate
  root directly.

Adding a test:

1. Put it in the theme file matching the surface it exercises. A test that spans
   surfaces (e.g. edit → format → push) goes with its _primary_ intent.
2. Need a helper used by more than one theme? Add it to `mod.rs`. A helper used
   by a single theme may stay in that theme file.
3. New theme? Add `mod <name>;` to `mod.rs` and a `//!` one-liner to the file.

## What we deliberately don't use

- **The top-level `tests/` integration directory.** That dir only sees a
  crate's public API. The editor's tests read private state (`e.text`,
  `e.caret`, `e.mode()`), so they must stay in-crate under `#[cfg(test)]`.
  Reach for `tests/` only to pin a crate's _public_ contract.
- **Doctests** for the embedded core. Fine for documenting a public API, but
  slow and awkward here; the behaviour is covered by the tests above.

## Running them

No cargo workspace ties the crates together, so run per crate from its own
directory:

```sh
cd editor && cargo test     # 224 tests
cd keymap && cargo test     # 28 tests
```
