# Why we don't shrink the notes repo

> The 150 MB of images in my notes repo isn't bloat to offload — it's the
> image CDN for the web app that reads the same repo. Shrinking it for
> Typoena's sake breaks remanso. So we don't.

The question that started this: **Typoena** keeps a persistent clone of my
notes repo (`github.com/jcalixte/notes`) on its SD card and fast-forwards it
on every `Ctrl-G`. The most likely first failure is a cold clone that's too
big — so the instinct was "clone the least data possible; ignore
`node_modules`; maybe strip the media." That instinct is mostly wrong, for
reasons worth writing down before anyone force-pushes a rewritten history.

## The measured reality

| Metric (target repo, measured 2026-07-07) | Value |
| --- | --- |
| Working tree | 3.9 GB (dominated by `node_modules`) |
| `.git` (what a clone actually transfers) | **566 MB** |
| Commits / objects | 13,852 / 63,252 |
| Depth-1 snapshot (HEAD tree + blobs) | **154.7 MB** |
| — of which markdown (the notes) | **1.4 MB** |
| — of which media (png/jpg/pdf/gif/bmp) | **153 MB** |
| Media across *all* history (dedup) | 566 MB (715 PNG objects = 463 MB) |

Two early assumptions corrected:

- **`node_modules` is a non-issue.** It's gitignored and was never committed,
  so a clone never touches it. There's nothing to filter. The 3.9 GB working
  tree is a red herring; a clone transfers the 566 MB `.git`.
- **The risk isn't stack overflow, it's transfer size.** 566 MB over Wi-Fi +
  mbedTLS to an SPI SD card, with no resume, is 30+ minutes and one dropout
  away from failure. Stack/memory pressure is a symptom of asking the device
  to cold-clone half a gigabyte.

## Why shrinking is off the table

The clean fix for "device only needs 1.4 MB of text" would be a blobless
partial clone (`--filter=blob:none`) — **libgit2 does not support it**. The
fallbacks are LFS migration, a filter-repo history purge, or `git rm` of media
at HEAD. All three remove the image *blobs* from what a git client sees.

That breaks **remanso**, the web app that reads the same repo. remanso is a
frontend with no server-side git; it displays a note image by reading the
image straight out of git as a blob and inlining it as a data URI
(`useImages.hook.ts` + `repo.ts`):

1. Markdown `<img src="relative/path.png">`
2. resolve path → find the file's `sha` in the GitHub tree
3. `GET /repos/{owner}/{repo}/git/blobs/{sha}` → **base64 of the git blob**
4. `img.src = "data:image/jpeg;base64," + thatContent`

The image *is* the git blob. GitHub's Git Blobs / Trees / Contents APIs do
**not** resolve LFS (only `media.githubusercontent.com` does, which remanso
doesn't use). So after an LFS migration those endpoints return the ~130-byte
**pointer text**, remanso wraps it in `data:image/jpeg;base64,…`, and every
image renders broken. Uploads break too: remanso writes images via
`PUT /contents`, which ignores `.gitattributes`/LFS and commits a plain blob.

And it's not specific to LFS — `git rm` at HEAD or a filter-repo purge remove
the blobs from the tree, so remanso can't find them either. **Any approach
that takes the images out of the git repo breaks remanso**, because those
150 MB are load-bearing infrastructure for the web app, not offloadable bloat.

| Consumer | How it reads images | If we shrink the repo |
| --- | --- | --- |
| Typoena (libgit2) | doesn't render images; needs valid git objects | fine — would get tiny pointers |
| remanso (Blobs API → data URI) | reads image bytes straight out of git | **broken** — pointer/missing bytes render as a dead image; uploads bypass LFS |

## Decision

**Leave the notes repo untouched. Pre-seed the device SD card with a full
`git clone` from a computer.**

Repo size is only a *device* constraint when the *device* does the cold clone.
A laptop clones 566 MB in ~2 minutes onto the SD via a card reader; the SD has
GB to spare. The device then only ever takes the `open` + incremental
fetch/commit/push path (`open_or_clone` already splits on this). A *full*
pre-seed (not depth-1) also sidesteps the shallow-push sharp edge. remanso
keeps working, the device gets everything, and repo size stops being anyone's
problem.

## What happens on an ongoing pull

In the single-writer model the device usually doesn't pull at all:
`fetch_and_integrate` runs only from the rejected-push arm of
`push_with_retry`, and a sole writer always fast-forwards. "Images to pull"
only arises when a **second writer** (remanso or the desktop) pushed them.
When it does, today the device fetches (downloading the image blobs) and then
hits the divergence bail (`increment B, deferred`) — no data loss, but no
integration until the merge path exists.

Once integration lands, the costs of carrying media the device never renders:

1. **Bandwidth for unusable bytes.** No partial fetch in libgit2, so a fetch
   pulls the full new image blobs. 20 MB of pasted screenshots = a 20 MB fetch
   before a one-line note can publish.
2. **~2× SD storage.** Each image lives in `.git` *and* in the working-tree
   checkout that `checkout_head(force)` writes.
3. **Memory — the real edge.** libgit2 tends to materialize a whole blob in
   RAM for checkout. History already has a 38 MB mp3 and 16 MB PNGs. Against
   **8 MB PSRAM**, a single large image arriving in a pull is a genuine OOM
   risk on checkout. Verify on hardware (push a 20 MB image from remanso, watch
   `min_free_heap`) rather than trusting a read of libgit2 internals.

**The trap:** `publish()` stages with `add_all(["*"])`. A sparse checkout that
omitted images would make `add_all` see them as deleted → the device would
commit "delete all images" and push it → remanso loses every image. So with
the current staging, a full checkout is mandatory — which is what feeds costs
2 and 3.

## Fix for increment C (git module in the editor)

Change two things together:

- **Stage specific paths, not `add_all(["*"])`.** The editor knows which note
  file it wrote — commit that path explicitly.
- Then a **sparse checkout that excludes media** is safe: the device never
  writes images to its working tree, killing the checkout OOM and the 2×
  storage. The bytes still transit `.git` on fetch (no partial clone), but
  writing objects to disk is far lower memory risk than a checkout that
  materializes them in RAM.

## Related

- `firmware/src/bin/git_sync.rs` — the persistent-clone publish cycle analysed
  here (milestone #2A).
- ADR-010 — "writing tool, not sync engine": the principle this decision
  serves.
- remanso: `src/hooks/useImages.hook.ts`, `src/modules/repo/services/repo.ts`
  (`queryFileContent`) — the image-as-blob pipeline.
