# Durability before delivery

> Why a 10-second keystroke on a typewriter I'm building doesn't have to
> feel slow.

## Two keystrokes, very different costs

I'm building a small device called **Typoena**: an e-ink panel, a
mechanical keyboard, an ESP32-S3, and a single purpose. You open the lid,
you write Markdown, and you press a key to publish it to GitHub. There is
no browser, no notification tray, no second app. The hardware enforces
focus the way software can't.

The whole product surface is two user-facing actions:

- **Save** (`Ctrl-S`) — write the current buffer to the SD card. Always
  available. ~200 ms.
- **Publish** (`Ctrl-G`) — ship the entire tracked working copy to the git
  remote. Atomic from the user's view. **5–10 seconds typical.**

These two keystrokes _look_ symmetric. Both are modifier-letter, both
triggered the same way, both single-shot. But one of them takes 200 ms
and the other takes 5–10 seconds. That's a 50× cost gap, and it matters
because the human perception threshold for "instant" is about 100 ms.

The physical keystroke itself — key down, debounce, USB report, key up —
takes ~150 ms. On `Ctrl-S` that's _most_ of the perceived time. On
`Ctrl-G`, the keystroke is barely the first sliver of a long process.

## Where the time goes

Here's a breakdown of `Ctrl-G` on a fresh session, with the Wi-Fi radio
starting cold:

| Stage                             | Time      |
| --------------------------------- | --------- |
| Save buffer to SD                 | ~0.1 s    |
| `git add` + `git commit`          | ~0.2 s    |
| Wi-Fi associate + DHCP            | 2–5 s     |
| TLS handshake                     | ~2 s      |
| `git push` (pack + send + server) | 1–3 s     |
| **Total (critical path)**         | **5–10 s** |

Wi-Fi association dominates, and that's deliberate. Typoena's radio is
**off by default** — it only powers up when `Ctrl-G` is pressed, and it
shuts down again after a short grace window. The battery savings are
dramatic: always-on station mode would burn ~410 mAh/day on the radio
alone; on-demand drops that to ~25 mAh/day at ten Publishes per day. On a
single 18650 cell, that's the difference between days and weeks of
standby.

The price is paid on every cold Publish. A few seconds, every time. For
a device that, by design, doesn't need to be online except when shipping
work, this trade is fine. But it produces the asymmetry above: one
keystroke costs you a fifth of a second; another costs you ten.

## The question I was sitting with

After mapping all this out, I asked the question that triggered the rest
of this post:

> "I'm concerned about the fact that Ctrl-G is a 150ms action to do, but
> what it triggers can take >5-10s. Compared to the same quick action
> Ctrl-S for instance that will have a order of magnitude even lower than
> the pressing key action."

My first instinct was to optimise. TLS session resumption could shave a
second. A smaller cipher suite, another. Static IP instead of DHCP, a
few hundred milliseconds. With effort, I might cut the cold path to
4–5 seconds.

But that's still 25× the `Ctrl-S` cost, and every optimisation comes
with friction. TLS resumption requires storing session tickets across
radio power-cycles (more state, more code). Cipher tuning sacrifices
flexibility on networks I haven't tested. Static IPs are fragile when
the user moves between routers. I'd be spending design budget on a
number that, even halved, still feels slow.

So I asked a different question: **what is the user actually waiting
for?**

## A different question

When you press `Ctrl-S`, the moment you care about is "my work is
saved." The SD card writes the bytes in 50–200 ms, and that moment
lines up with the operation completing. Save = safe. Same instant.

When you press `Ctrl-G`, what's the equivalent? You'd naturally say "my
work is published" — and assume that means the push completed. But this
device authors timestamp commits _before_ it pushes. The local commit
lands at ~0.2 seconds, and from that moment on your work is preserved
across power loss, SD removal, the apocalypse — everything except remote
delivery. The remaining 5–10 seconds is _transport of an already-safe
thing_. The work isn't in flight; it's already committed to disk. The
push is just delivery to a backup location.

## Durability before delivery

This is the design principle the question pushed me toward: **the moment
that matters to the user is the moment durability is achieved, not the
moment delivery completes.** Once I named it, the implementation became
obvious.

The status line surfaces the commit-landed state at ~0.2 seconds, then
shows the push as a secondary state:

```
Bad:   "publishing… 1 of 3 ▓░░"        ← misleading, conflates safe + delivered
Good:  "✓ committed abc1234 · pushing" ← says exactly what's done and what's pending
```

Two transitions, two messages, partial-refresh on the status line only.
The user sees the `✓` within a fifth of a second of pressing the key.
They know the work is safe. They can keep typing. The radio continues
associating, the TLS handshake completes, the push lands — all of it
happens around them, none of it modal.

The _perceived_ latency of `Ctrl-G` collapses from 10 seconds to roughly
200 milliseconds. The gap with `Ctrl-S` is no longer 50×; it's barely
distinguishable.

## Why this generalises

I think this is a useful lens beyond writing appliances. Any application
that does network I/O in response to a single user action has the same
shape: a fast local operation followed by a slow remote one. The usual
responses are:

1. **Optimise the slow part until it feels fast.** Often impossible.
2. **Hide the slow part with a spinner.** Admits defeat — and on e-ink,
   with ~300 ms refresh and ghosting, you can't even spin.
3. **Quietly do the operation in the background and not tell the user.**
   This is the auto-sync trap I explicitly designed Typoena to avoid —
   the device is a writing tool, not a sync engine.

The fourth path — name the durability moment, surface it the instant it
arrives — is almost always available. It shifts the question from "how
fast is the operation" to "when is the user safe." Those are different
questions with different answers, and the second one is almost always
faster.

## What I'm not optimising

I'm not chasing a v1.0 target of `Ctrl-G` in ≤10 seconds on the cold
path. With the safety moment landing at ~200 ms, that target is no
longer the load-bearing UX metric. I'd rather spend the engineering
effort on something the user can actually feel: typing latency,
partial-refresh ghosting, keyboard wake time.

Durability before delivery. Once you see it, you can't unsee it — and
suddenly the slow operations stop feeling slow.
