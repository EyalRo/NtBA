#!/usr/bin/env python3
"""Render a short demo animation from the WIP player sprite board (v2).

Cuts the run / dribble / shoot frames out of assets/sprites/wip/players-board-v2.png, aligns them
on a shared ground line, and plays them on a simple court: walk left, walk up, dribble right, shoot.

The board is a 1536x1024 presentation image (not a packable sheet), so frames are segmented by
colour keying against the navy panel background using hand-measured column cuts.

Usage: python3 scripts/render_wip_board_demo.py [out.gif]
Requires: pip install Pillow numpy scipy
"""
import sys
from pathlib import Path

import numpy as np
from scipy import ndimage as ndi
from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parents[1]
BOARD = ROOT / "assets" / "sprites" / "wip" / "players-board-v2.png"
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "assets" / "sprites" / "previews" / "wip_board_v2_demo.gif"

W, H, SCALE = 320, 180, 3  # logical stage size (sprites stay 1:1) and GIF upscale
FRAME_MS = 83  # ~12 fps

# name: (cut x-boundaries, y0, y1, label-text zone (x0, x1, y1) to blank out). Boundaries are board pixels.
SPANS = {
    "side_idle": (list(range(20, 251, 46)), 313, 402, None),
    "side_run": ([255 + round(i * 54.4) for i in range(9)], 318, 402, None),
    "side_dribble": ([700, 772, 835, 889, 945, 1008, 1090], 315, 402, None),
    "side_shoot": ([400, 447, 489, 530, 578, 637, 681, 724, 775], 418, 530, (380, 440, 430)),
    "end_run": ([300 + round(i * 51.9) for i in range(9)], 606, 712, None),
}


BG = np.array([1, 16, 33])  # the panel background is a near-uniform navy
BG_TOL = 26


def sprite_mask(img_crop):
    """Foreground = everything not reachable from the crop border through background-coloured pixels.

    Flood-filling (rather than thresholding) keeps dark hair, shadows and trim that sit inside the sprite.
    """
    a = np.asarray(img_crop).astype(int)
    bgish = np.sqrt(((a - BG) ** 2).sum(-1)) < BG_TOL
    h, w = bgish.shape
    outside = np.zeros_like(bgish)
    stack = [(y, x) for y in range(h) for x in (0, w - 1)] + [(y, x) for x in range(w) for y in (0, h - 1)]
    while stack:
        y, x = stack.pop()
        if 0 <= y < h and 0 <= x < w and bgish[y, x] and not outside[y, x]:
            outside[y, x] = True
            stack += [(y + 1, x), (y - 1, x), (y, x + 1), (y, x - 1)]
    return ~outside


def cut_frames(img, name):
    seeds, y0, y1, label = SPANS[name]
    n = len(seeds) - 1
    row = img.crop((seeds[0], y0, seeds[-1], y1))
    if label:  # paint the board's caption text out before keying
        lx0, lx1, ly1 = label
        ImageDraw.Draw(row).rectangle((lx0 - seeds[0], 0, lx1 - seeds[0], ly1 - y0), fill=tuple(BG))
    rm = sprite_mask(row)
    # Neighbouring poses overlap horizontally (trailing feet, loose balls), so split by connected blob:
    # dilate a little to keep a ball with its hand, then give each blob to the frame its mass sits in.
    blobs, k = ndi.label(ndi.binary_dilation(rm, iterations=2))
    bounds = np.array(seeds[1:-1]) - seeds[0]
    owner = np.zeros(rm.shape, int)
    for i in range(1, k + 1):
        ys, xs = np.where((blobs == i) & rm)
        if len(xs) < 12:
            continue  # speck
        owner[ys, xs] = np.searchsorted(bounds, np.median(xs)) + 1
    frames = []
    for f in range(n):
        m = owner == f + 1
        ys, xs = np.where(m)
        t, bt, l, rr = ys.min(), ys.max(), xs.min(), xs.max()
        rgba = np.zeros((m.shape[0], rr - l + 1, 4), np.uint8)
        rgba[..., :3] = np.asarray(row)[:, l:rr + 1]
        rgba[..., 3] = m[:, l:rr + 1] * 255
        # hip anchor: median x of the torso band, so arm/ball swings do not shift the body
        band = (ys > t + 0.30 * (bt - t)) & (ys < t + 0.55 * (bt - t))
        ax = int(np.median(xs[band])) - l if band.any() else (rr - l) // 2
        frames.append({"img": Image.fromarray(rgba), "ax": ax, "bottom": bt})
    ground = max(f["bottom"] for f in frames)  # airborne frames sit above the shared ground line
    for f in frames:
        f["ay"] = ground  # row in the crop that lies on the shared ground line
    return frames


def place(canvas, fr, x, y, flip=False, shadow=True):
    """Draw frame with its ground anchor at (x, y); `flip` mirrors it for left-facing."""
    img, ax = fr["img"], fr["ax"]
    if flip:
        img, ax = img.transpose(Image.FLIP_LEFT_RIGHT), img.width - 1 - ax
    if shadow:
        sh = Image.new("RGBA", canvas.size, (0, 0, 0, 0))
        ImageDraw.Draw(sh).ellipse((x - 17, y - 4, x + 17, y + 4), fill=(0, 0, 0, 90))
        canvas.alpha_composite(sh)
    canvas.alpha_composite(img, (int(x - ax), int(y - fr["ay"])))


LINE = (222, 228, 240, 255)


def court():
    im = Image.new("RGBA", (W, H), (46, 58, 84, 255))
    d = ImageDraw.Draw(im)
    for y in range(0, H, 8):  # plank seams
        d.line((0, y, W, y), fill=(40, 51, 76, 255))
    d.rectangle((4, 4, W - 5, H - 5), outline=LINE)
    d.rectangle((W - 100, 46, W - 5, 134), outline=LINE, fill=(74, 52, 120, 255))  # key
    d.arc((W - 170, 24, W - 30, 156), 90, 270, fill=LINE)
    return im


def hoop(canvas):
    d = ImageDraw.Draw(canvas)
    d.rectangle((W - 26, 26, W - 23, 66), fill=(235, 235, 240, 255))  # backboard
    d.line((W - 23, 57, W - 36, 57), fill=(255, 120, 20, 255), width=2)  # rim
    for i in range(4):
        d.line((W - 36 + i * 4, 58, W - 34 + i * 3, 68), fill=(235, 235, 240, 200))  # net
    return (W - 31, 57)


def main():
    img = Image.open(BOARD).convert("RGB")
    clips = {n: cut_frames(img, n) for n in SPANS}
    # the ball in flight: the loose ball right of the last dribble frame
    bx0, by0 = 1070, 355
    bcrop = img.crop((bx0, by0, bx0 + 16, by0 + 16))
    ball = Image.fromarray(np.dstack([np.asarray(bcrop), sprite_mask(bcrop).astype(np.uint8) * 255]))
    rim_x, rim_y = hoop(court())

    seq = []  # (clip, frame index, x, y, flip, ball_xy)
    x, y = 215.0, 150.0
    for i in range(6):  # idle, facing left
        seq.append(("side_idle", i % 5, x, y, True, None))
    for i in range(16):  # walk left
        x -= 5
        seq.append(("side_run", i % 8, x, y, True, None))
    for i in range(12):  # turn: walk up (back view)
        y -= 2.6
        seq.append(("end_run", i % 8, x, y, False, None))
    for i in range(18):  # turn: dribble right
        x += 5
        seq.append(("side_dribble", i % 6, x, y, False, None))
    shot = [(0, 3), (1, 2), (2, 2), (3, 3)]  # gather, rise, rise, release (frame, ticks)
    for idx, ticks in shot:
        seq += [("side_shoot", idx, x, y, False, None)] * ticks
    rel = (x + 12, y - 66)
    flight = 12
    tail = [("side_shoot", 4)] * 4 + [("side_shoot", 7)] * 4 + [("side_idle", 0)] * 4  # follow-through, land, stand
    for i in range(flight):
        t = (i + 1) / flight
        bxy = (rel[0] + (rim_x - rel[0]) * t, rel[1] + (rim_y - rel[1]) * t - 40 * 4 * t * (1 - t))
        seq.append((*tail[i], x, y, False, bxy))
    seq += [("side_idle", 0, x, y, False, None)] * 8
    out = []
    for clip, idx, px, py, flip, bxy in seq:
        c = court()
        hoop(c)
        place(c, clips[clip][idx], px, py, flip)
        if bxy:
            c.alpha_composite(ball, (int(bxy[0] - 8), int(bxy[1] - 8)))
        out.append(c.convert("RGB").resize((W * SCALE, H * SCALE), Image.NEAREST))
    pal = out[0].quantize(256, method=Image.MEDIANCUT)
    frames = [f.quantize(palette=pal, dither=Image.NONE) for f in out]
    OUT.parent.mkdir(parents=True, exist_ok=True)
    frames[0].save(OUT, save_all=True, append_images=frames[1:], duration=FRAME_MS, loop=0, optimize=False)
    print(f"wrote {OUT} ({len(frames)} frames)")


if __name__ == "__main__":
    main()
