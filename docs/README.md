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
| [`v0.1-mvp-product.md`](v0.1-mvp-product.md)     | v0.1 product design — boot, type one file, `Ctrl-S` to save, `Ctrl-G` to publish.                                    |
| [`v0.1-mvp-technical.md`](v0.1-mvp-technical.md) | v0.1 technical design — single Rust binary on `esp-idf-rs`, modules, threads, bring-up order.                        |
| [`macroplan.md`](macroplan.md)                   | Version-by-version plan; each release is a usable artifact, not a checkpoint.                                        |
| [`typoena-toml.md`](typoena-toml.md)             | `.typoena.toml` reference — the git-tracked editor preferences (auto-save, format-on-save, line numbers, auto-sync). |
| [`hardware.md`](hardware.md)                     | Part choices for the bench build and the rationale behind them.                                                      |

## Quality method

| Doc                                                | What's in it                                                                                                                            |
| -------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------- |
| [`qfd.md`](qfd.md)                                 | Quality Function Deployment — turns user-facing requirements into technical HOWs; §3 is the filled 14 WHATs × 14 HOWs House of Quality. |
| [`quality-house-empty.md`](quality-house-empty.md) | The House chassis, blank — for re-scoring from scratch.                                                                                 |

## Bench work

| Area                                            | What's in it                                                                                                                                    |
| ----------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------- |
| [`spikes.md`](spikes.md)                        | Rendering & UX spikes — display/UX risks proved outside the hardware stack.                                                                     |
| [`postmortems/`](postmortems/README.md)         | Bring-up debugging write-ups: what broke, the root cause, and the decisions that came out of it.                                                |
| [`notes/`](notes/README.md)                     | Longer-form essays on the thinking behind specific choices — e.g. where the ~16 s cold [`:sync`](notes/sync-latency.md) goes.                   |
| [`tradeoff-curves/`](tradeoff-curves/README.md) | Cost-vs-knob curves behind chosen defaults — energy, latency, memory.                                                                           |
| [`kaizen/`](kaizen/README.md)                   | Six-step kaizen write-ups — the problem→analysis→fix story behind an improvement, e.g. the real-repo [`:sync` brick](kaizen/real-repo-sync.md). |
