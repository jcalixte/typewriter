# Typewriter

A single-purpose writing appliance: e-ink + mechanical keyboard + ESP32-S3. The
user opens the lid, writes Markdown, and (when they choose) publishes to a git
remote. This glossary fixes the language of that workflow.

**Related docs:**
[`README.md`](README.md) — project overview, hardware, macro roadmap.
[`docs/adr.md`](docs/adr.md) — load-bearing decisions; **ADR-010** is the
formal record of the **Publish** UX defined below.
[`docs/qfd.md`](docs/qfd.md) — requirements ↔ functions ↔ components, ranked
by user-facing weight. References the terms in this file as canonical.
[`docs/v0.1-mvp-product.md`](docs/v0.1-mvp-product.md) — the v0.1 product
surface, expressed in this vocabulary.
[`docs/v0.1-mvp-technical.md`](docs/v0.1-mvp-technical.md) — how v0.1 is
built.
[`docs/roadmap.md`](docs/roadmap.md) — per-version scope, where new terms
(e.g. multi-file **Buffer** concepts at v0.5) will enter this glossary.

## Language

### File scopes

**Tracked**:
A file that lives in the device's git working copy and can be published to the
remote. Lives under `/sd/repo/`.
_Avoid_: synced, public, remote, committable.

**Local**:
A file that exists only on the device and can never be published. A
permanently-private scope, not a draft staging area — files are born Local and
stay Local for their lifetime. Lives under `/sd/local/`.
_Avoid_: draft, private, untracked, scratch (these all imply impermanence or
promotability, which is not the model).

### User-facing actions

**Save**:
The act of durably writing the current buffer to the SD card. Triggered by
`Ctrl-S`. Applies to both **Tracked** and **Local** files.
_Avoid_: write, flush, persist (use them only in implementation talk).

**Publish**:
The atomic act of pushing the current state of the entire **Tracked** working
copy to the git remote. Workspace-scoped, not buffer-scoped: a **Publish**
ships every dirty **Tracked** file on the device, not just the one the user is
viewing. Triggered by `Ctrl-G`. Internally: stage all → commit with a
timestamp message → push → on push failure, pull (merge, no-edit) → push
again. Unavailable in **Local**.
_Avoid_: push, commit, sync, upload, git-push (these leak transport details
into user-facing language).

> **Commit** is deliberately *not* a user-facing term. The device authors all
> commit messages itself (ISO-8601 timestamp); the user never sees a commit
> prompt. A **Publish** is the only user-observable unit of "shipping work";
> internal commits are an implementation detail of that.

## Relationships

- A **File** belongs to exactly one scope (**Tracked** or **Local**), fixed at
  creation. There is no operation that moves a file between scopes.
- **Save** applies to any **File**; **Publish** applies only to **Tracked**.
- A single **Publish** is atomic from the user's view: a Wi-Fi failure or
  remote divergence surfaces as a single retry-able outcome, not as a multi-
  step progression the user has to reason about.

## Example dialogue

> **Dev:** "If I'm in a **Local** file and I press `Ctrl-G`, what happens?"
> **Domain expert:** "Nothing — **Publish** is unavailable in **Local**. The
> status line says so. There is no path from **Local** to the remote."
> **Dev:** "So if I want to publish something that started as a journal entry,
> I have to copy-paste it into a **Tracked** file?"
> **Domain expert:** "Yes, deliberately. Promotion is a manual gesture, not a
> built-in operation."
> **Dev:** "And if the remote has changed since I last pulled — does
> **Publish** fail?"
> **Domain expert:** "It pulls (merge, no edit) and pushes again. From the
> user's view it's one action with one outcome — success or retry."

## Principles

- **The device is a writing tool, not a sync engine.** Every git operation is
  the direct, in-session consequence of a `Ctrl-G` the user pressed. The
  device does not auto-publish, auto-pull, retry-on-boot, or otherwise
  reconcile remote state in the background. If a previous **Publish** ended
  mid-flight and left a local commit unpushed, the next user-initiated
  **Publish** picks it up; until then, the device is silent about it.
- **No state the user didn't ask for.** No banners about pending work, no
  prompts about divergence, no "did you mean to publish" warnings. The status
  line reflects the *current* action's outcome, nothing else.

## Flagged ambiguities

- "Local" was initially ambiguous between (a) a draft pen that gets promoted,
  (b) a permanently-private scope, (c) a second git repo, (d) `.gitignore`'d
  files inside the working copy. Resolved: (b). Each **File**'s scope is fixed
  at creation; there is no promotion operation.
- "Commit" was used loosely across early docs as if it were a user-facing
  action. Resolved: it is not. The user has **Save** and **Publish**. Commits
  are an internal unit inside **Publish**, never authored or named by the
  user.
