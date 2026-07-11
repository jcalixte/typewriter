# Notes

> Longer-form essays on the thinking behind specific Typoena choices — the
> arguments too big for an ADR and too durable for a commit message.
>
> Docs index: [`../README.md`](../README.md). Project overview:
> [`../../README.md`](../../README.md).

| Note | What it argues |
| --- | --- |
| [`ctrl-g-perceived-latency.md`](ctrl-g-perceived-latency.md) | Durability before delivery — surfacing "commit landed" at ~0.2 s makes the 5–10 s `Ctrl-G` push feel instant. |
| [`git-sync-images-and-repo-size.md`](git-sync-images-and-repo-size.md) | Why we don't shrink the notes repo — its 153 MB of media is remanso's image CDN, so rewriting history to slim the on-device clone breaks the web app. |
| [`boot-time-budget.md`](boot-time-budget.md) | Where the ~4.3 s to cursor goes — and why the ≤ 3 s v1.0 target is hard: one ~1.9 s full refresh is unavoidable at cold boot, so the splash is nearly free. |
| [`sync-latency.md`](sync-latency.md) | Where the ~16 s cold `:sync` goes — Wi-Fi + SNTP + one TLS push; why optimistic-retry cut a whole handshake, and why the rest is close to the protocol floor. |
