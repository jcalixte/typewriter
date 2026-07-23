#!/usr/bin/env python3
"""Bake Typo (o tucano, the companion character) from Julien's reference drawing.

Base truth is a 1-bit threshold of `typo_ref.png` — the sprite is Julien's own
line art pixelated, never a redrawn interpretation (three hand-drawn attempts
were rejected before this pipeline; the thresholded mark was approved on sight).
Faces are the same grid MIRRORED (so Typo watches the writing column from the
side panel) plus small per-mood pixel overlays; the body stays unmirrored, as
drawn, for the boot splash.

No PIL: macOS `sips` does the downsampling and PNG→BMP conversion, and the BMP
is parsed by hand. Threshold sum(r,g,b) < 700 catches the reference's light pink
strokes on white.

  Regenerate:
    python3 display/tools/typogen.py [--preview /tmp/typo_preview.png]

Outputs, next to the crate source:
    display/src/typo/sprites.rs   (the packed row arrays; the Sprite/Mood API
                                   lives hand-written in typo/mod.rs)
And, with --preview, a contact-sheet PNG (needs rsvg-convert) for eyeballing.
"""
import argparse
import json
import os
import struct
import subprocess
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REF = os.path.join(HERE, 'typo_ref.png')
OUT_RS = os.path.join(HERE, '..', 'src', 'typo', 'sprites.rs')


def bmp_grid(path, cut=700):
    """Threshold a BMP to a 1-bit grid (1 = ink), trimmed of empty margins."""
    d = open(path, 'rb').read()
    off = struct.unpack_from('<I', d, 10)[0]
    w = struct.unpack_from('<i', d, 18)[0]
    h = struct.unpack_from('<i', d, 22)[0]
    step = struct.unpack_from('<H', d, 28)[0] // 8
    rb = ((w * step + 3) // 4) * 4
    g = []
    for y in range(abs(h)):
        sy = (abs(h) - 1 - y) if h > 0 else y
        g.append([1 if d[off+sy*rb+x*step]+d[off+sy*rb+x*step+1]+d[off+sy*rb+x*step+2] < cut else 0
                  for x in range(w)])
    rows = [i for i, r in enumerate(g) if any(r)]
    cols = [i for i in range(w) if any(r[i] for r in g)]
    return [r[cols[0]:cols[-1]+1] for r in g[rows[0]:rows[-1]+1]]


def resample(width, tmpdir):
    out = os.path.join(tmpdir, f'r{width}')
    subprocess.run(['sips', '--resampleWidth', str(width), REF, '--out', out + '.png'],
                   check=True, capture_output=True)
    subprocess.run(['sips', '-s', 'format', 'bmp', out + '.png', '--out', out + '.bmp'],
                   check=True, capture_output=True)
    return bmp_grid(out + '.bmp')


def flip(g):
    return [list(reversed(r)) for r in g]


def px(g, x, y, v=1):
    if 0 <= y < len(g) and 0 <= x < len(g[0]):
        g[y][x] = v


def line(g, x0, y0, x1, y1, v=1):
    dx, dy = abs(x1-x0), -abs(y1-y0)
    sx, sy = (1 if x0 < x1 else -1), (1 if y0 < y1 else -1)
    err = dx + dy
    while True:
        px(g, x0, y0, v)
        if x0 == x1 and y0 == y1:
            break
        e2 = 2 * err
        if e2 >= dy:
            err += dy; x0 += sx
        if e2 <= dx:
            err += dx; y0 += sy


def sparkle(g, x, y):
    for a, b in ((0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)):
        px(g, x + a, y + b)


def copy(g):
    return [r[:] for r in g]


def build(tmpdir):
    # ---- base grids straight from the reference -----------------------------
    base48 = resample(56, tmpdir)     # trims to ~48x48; eye at cols 13-16, rows 7-10
    compact = resample(48, tmpdir)    # smaller cut, no additions at all

    w = len(base48[0])                # flipped x = w-1-x
    ex = w - 1 - 14                   # eye centre x, flipped (~33); eye rows 7-10

    def clear_eye(g):
        for y in range(6, 12):
            for x in range(ex - 3, ex + 3):
                px(g, x, y, 0)

    def face(mood):
        """All moods are pixel overlays on the bare mirrored reference."""
        g = flip(copy(base48))
        if mood == 'neutral':
            return g
        if mood == 'frustrated':                    # pre-refresh: ghosting builds
            for y in range(6, 9):                   # half-lid: keep lowest eye rows
                for x in range(ex - 3, ex + 3):
                    px(g, x, y, 0)
            line(g, ex - 4, 4, ex + 2, 5)           # knitted brow
            for x, y in ((8, 1), (3, 12), (27, 17), (44, 4), (46, 13), (18, 20)):
                px(g, x, y)                         # ghost dust on his feathers
            return g
        # ---- the post-flash pool: one of these after every full refresh -----
        if mood == 'anticipation':
            for y in range(6, 11):                  # wide-open eye
                for x in range(ex - 2, ex + 3):
                    px(g, x, y)
            px(g, ex - 2, 7, 0)                     # catchlight
            px(g, ex - 1, 7, 0)
            sparkle(g, 2, 18)
            sparkle(g, 45, 4)
            return g
        if mood == 'wink':
            clear_eye(g)
            for x, y in ((ex - 3, 9), (ex - 2, 8), (ex - 1, 8), (ex, 8), (ex + 1, 8), (ex + 2, 9)):
                px(g, x, y)                         # happy closed arc
            sparkle(g, 44, 5)
            return g
        if mood == 'curious':                       # "?" floats behind his head
            q = ["0110", "1001", "0001", "0010", "0100", "0000", "0100"]
            for dy, row in enumerate(q):
                for dx, c in enumerate(row):
                    if c == '1':
                        px(g, 43 + dx, 1 + dy)
            return g
        if mood == 'determined':
            line(g, ex - 3, 5, ex + 2, 5)           # straight low brow, eye intact
            return g
        if mood == 'zen':
            clear_eye(g)
            line(g, ex - 3, 9, ex + 2, 9)           # softly closed eye
            return g
        if mood == 'note':                          # whistling at the fresh page
            n = ["0010", "0011", "0010", "0110", "1110"]
            for dy, row in enumerate(n):            # in the clear pocket under the tip
                for dx, c in enumerate(row):
                    if c == '1':
                        px(g, 1 + dx, 17 + dy)
            return g
        raise ValueError(mood)

    moods = ['neutral', 'frustrated', 'anticipation', 'wink', 'curious',
             'determined', 'zen', 'note']
    sprites = {'body': base48, 'mark_compact': compact}
    sprites.update({m: face(m) for m in moods})
    return sprites


def emit_rust(sprites):
    parts = [
        "//! GENERATED by display/tools/typogen.py — do not edit by hand.\n",
        "//! Thresholded from typo_ref.png (Julien's reference drawing); the mood\n",
        "//! faces are the mirrored base plus pixel overlays. Regenerate with:\n",
        "//!   python3 display/tools/typogen.py\n",
        "\n",
        "use super::Sprite;\n",
    ]
    for name, g in sprites.items():
        w, h = len(g[0]), len(g)
        parts.append(f"\npub(super) const {name.upper()}: Sprite = Sprite {{\n")
        parts.append(f"    w: {w},\n    h: {h},\n    rows: &[\n")
        for row in g:
            bits = 0
            for x, v in enumerate(row):
                if v:
                    bits |= 1 << (w - 1 - x)
            parts.append(f"        0x{bits:0{(w + 3) // 4}x},\n")
        parts.append("    ],\n};\n")
    return ''.join(parts)


def emit_preview(sprites, path):
    """Contact sheet (8x + 1x per sprite) via SVG -> rsvg-convert, to eyeball."""
    z, pad = 8, 20
    order = list(sprites)
    total_w = sum(len(sprites[k][0]) * z + pad for k in order) + pad
    total_h = max(len(sprites[k]) * z for k in order) + pad * 2 + 60
    parts = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{total_w}" height="{total_h}">',
             f'<rect width="{total_w}" height="{total_h}" fill="#e8e4da"/>']
    x = pad
    for k in order:
        parts.append(f'<text x="{x}" y="{pad-6}" font-family="monospace" font-size="13">{k}</text>')
        for yy, row in enumerate(sprites[k]):
            for xx, c in enumerate(row):
                if c:
                    parts.append(f'<rect x="{x+xx*z}" y="{pad+yy*z}" width="{z}" height="{z}"/>')
        oy = pad + len(sprites[k]) * z + 10
        for yy, row in enumerate(sprites[k]):        # 1x preview
            for xx, c in enumerate(row):
                if c:
                    parts.append(f'<rect x="{x+xx}" y="{oy+yy}" width="1" height="1"/>')
        x += len(sprites[k][0]) * z + pad
    parts.append('</svg>')
    svg = path + '.svg'
    open(svg, 'w').write(''.join(parts))
    subprocess.run(['rsvg-convert', svg, '-o', path], check=True)
    os.remove(svg)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument('--preview', help='also render a contact-sheet PNG here')
    args = ap.parse_args()
    with tempfile.TemporaryDirectory() as tmpdir:
        sprites = build(tmpdir)
    open(OUT_RS, 'w').write(emit_rust(sprites))
    print(f'wrote {os.path.relpath(OUT_RS, HERE)}:',
          json.dumps({k: f'{len(v[0])}x{len(v)}' for k, v in sprites.items()}))
    if args.preview:
        emit_preview(sprites, args.preview)
        print(f'preview: {args.preview}')


if __name__ == '__main__':
    main()
