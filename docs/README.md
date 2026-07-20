# Docs

> The design record for Typoena — the decisions, specs, and bench write-ups
> behind the writing appliance. Start with the [ADRs](adr.md) for the
> load-bearing choices, or the [v0.1 specs](v0.1-mvp-product.md) for what the
> first release actually does.
>
> Project overview: [`../README.md`](../README.md).

## Decisions & specs

| Doc                                              | What's in it                                                                                                         |
| ------------------------------------------------ | -------------------------------------------------------------------------------------------------------------------- |
| [`adr.md`](adr.md)                               | Architecture Decision Records — the load-bearing technical choices and why.                                          |
| [`v0.1-mvp-product.md`](v0.1-mvp-product.md)     | v0.1 product design — boot, type one file, `Ctrl-S` to save, `Ctrl-G` to push.                                       |
| [`v0.1-mvp-technical.md`](v0.1-mvp-technical.md) | v0.1 technical design — single Rust binary on `esp-idf-rs`, modules, threads, bring-up order.                        |
| [`macroplan.md`](macroplan.md)                   | Version-by-version plan; each release is a usable artifact, not a checkpoint.                                        |
| [`typoena-toml.md`](typoena-toml.md)             | `.typoena.toml` reference — the git-tracked editor preferences (auto-save, format-on-save, line numbers, auto-sync). |
| [`hardware.md`](hardware.md)                     | Part choices for the bench build and the rationale behind them.                                                      |

## Conventions

| Doc                        | What's in it                                                                                                            |
| -------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| [`testing.md`](testing.md) | Where Rust tests live — unit tests in-file vs the `editor` crate's `src/tests/` behavioural submodule; how to run them. |

## Quality method

| Doc                                                | What's in it                                                                                                                            |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| [`qfd.md`](qfd.md)                                 | Quality Function Deployment **hub** — what-matters-now headlines, the page index, and the keep-honest rules. Start here.                |
| [`qfd-house-1.md`](qfd-house-1.md)                 | House 1 (WHATs × HOWs, 16×16) with §1 requirements, §2 characteristics, §3 reading, §4 roof.                                           |
| [`qfd-perception.md`](qfd-perception.md)           | Competitive perception — five products scored 0–5 per WHAT, measured benchmarks, caveats.                                              |
| [`qfd-house-2.md`](qfd-house-2.md)                 | House 2 (HOWs × components) — §5 cascade tree, component ranking, shared-pool budget matrix.                                           |
| [`qfd-houses-3-4.md`](qfd-houses-3-4.md)           | Houses 3 & 4 under the pipeline reading — processes P1–P9 × controls Q1–Q8.                                                            |
| [`qfd-budget.md`](qfd-budget.md)                   | §6 critical performance budget — ranked targets, verdicts, and the named fallback per row.                                            |
| [`qfd-tradeoffs.md`](qfd-tradeoffs.md)             | §7 tradeoffs T1–T15 and the tensions left deliberately unresolved, each with its trigger.                                             |
| [`qfd-changelog.md`](qfd-changelog.md)             | §8 ledger — every inconsistency spotted between the houses and reality, and its fix.                                                  |
| [`quality-house-empty.md`](quality-house-empty.md) | The House chassis, blank — for re-scoring from scratch.                                                                                 |
| [`house-vs-product.md`](house-vs-product.md)       | Standing challenges between the scored houses and the real product — open disputes with evidence and resolution triggers.               |

## Bench work

| Area                                            | What's in it                                                                                                                                    |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| [`spikes.md`](spikes.md)                        | Rendering & UX spikes — display/UX risks proved outside the hardware stack.                                                                     |
| [`postmortems/`](postmortems/README.md)         | Bring-up debugging write-ups: what broke, the root cause, and the decisions that came out of it.                                                |
| [`notes/`](notes/README.md)                     | Longer-form essays on the thinking behind specific choices — e.g. where the ~16 s cold [`:sync`](notes/sync-latency.md) goes.                   |
| [`tradeoff-curves/`](tradeoff-curves/README.md) | Cost-vs-knob curves behind chosen defaults — energy, latency, memory.                                                                           |
| [`kaizen/`](kaizen/README.md)                   | Six-step kaizen write-ups — the problem→analysis→fix story behind an improvement, e.g. the real-repo [`:sync` brick](kaizen/real-repo-sync.md). |
