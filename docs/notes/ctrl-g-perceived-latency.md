# Durability before delivery

> Why a 10-second keystroke on a typewriter I'm building doesn't have to feel slow.

I'm building a small device called **Typoena**: an e-ink panel, a mechanical keyboard, an ESP32-S3, and a single purpose. You open the lid, you write Markdown, you press a key to publish it to GitHub. There is no browser, no notification tray, no second app — the hardware enforces focus the way software can't.

The whole product surface is two user-facing actions:

- **Save** (`Ctrl-S`) — write the buffer to the SD card. ~200 ms.
- **Publish** (`Ctrl-G`) — ship the working copy to the git remote. **5–10 seconds.**

The same kind of keystroke, but one takes 50× longer than the other. The physical keypress is ~150 ms either way — key down, debounce, USB report, key up. On `Ctrl-S` that's _most_ of the perceived time; on `Ctrl-G` it's barely the first sliver of a long process. And the human perception threshold for "instant" is around 100 ms, so the gap isn't just a number on paper.

Sitting with that, here's the concern I couldn't shake:

> "I'm concerned about the fact that Ctrl-G is a 150ms action to do, but what it triggers can take >5-10s. Compared to the same quick action Ctrl-S for instance that will have a order of magnitude even lower than the pressing key action."

The reframing question: **what is the user actually waiting for?** For `Ctrl-S`, the moment that matters is "my work is saved" — and the SD card completes the write in 50–200 ms. Save = safe. Same instant.

For `Ctrl-G`, the equivalent moment isn't "push complete." It's "commit landed locally" — which happens at ~0.2 seconds, well before the push even starts. From that moment on, your work is preserved across power loss, SD removal, the apocalypse — everything except remote delivery. The remaining 5–10 seconds is _transport of an already-safe thing_.

Surface that moment in the status line at ~0.2 seconds (`✓ committed abc1234 · pushing…`) and the perceived latency of `Ctrl-G` collapses from 10 seconds to roughly 200 milliseconds. The gap with `Ctrl-S` disappears.

**Durability before delivery.** The moment that matters to the user is the moment durability is achieved, not the moment delivery completes. Once you see that, the slow operations stop feeling slow.
