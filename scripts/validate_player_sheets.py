#!/usr/bin/env python3
"""Validate NtBA player sheets against issue #140's indexed-PNG contract.

Usage: python3 scripts/validate_player_sheets.py
Requires: pip install Pillow
"""
from pathlib import Path
import json
import sys
from PIL import Image

ROOT = Path(__file__).resolve().parents[1] / "assets" / "sprites"
SHEETS = {"players_side": ((60, 64), (30, 56), 20),
          "players_end": ((48, 56), (24, 46), 14)}
ROLES = {"skin": (6, 9), "hair": (34, 37), "jersey": (14, 17),
         "trim": (18, 21), "shorts": (58, 61), "socks": (22, 25),
         "shoes": (26, 29)}
EXPECTED = {
    "players_side": {"idle": (0, 1), "run": (1, 8), "jump": (9, 2),
                     "dribble_idle": (11, 1), "dribble_run": (12, 8)},
    "players_end": {"front_idle": (0, 1), "front_run": (1, 4),
                    "front_jump": (5, 1), "front_dribble": (6, 1),
                    "back_idle": (7, 1), "back_run": (8, 4),
                    "back_jump": (12, 1), "back_dribble": (13, 1)}
}

def palette():
    colors = []
    for line in (ROOT / "ntba.gpl").read_text().splitlines():
        parts = line.split()
        if len(parts) >= 3 and all(p.isdecimal() for p in parts[:3]):
            colors.append(tuple(map(int, parts[:3])))
    if len(colors) != 89:
        raise ValueError(f"Expected 89 palette colors, got {len(colors)}")
    return colors

def validate(name, colors):
    errors = []
    def check(condition, message):
        if not condition:
            errors.append(message)
    meta = json.loads((ROOT / (name + ".json")).read_text())
    frame, origin, count = SHEETS[name]
    check(tuple(meta["frame"]) == frame, "frame size mismatch")
    check(tuple(meta["origin"]) == origin, "origin mismatch")
    check(len(meta["frames"]) == count, "frame count mismatch")
    check(meta.get("transparent") == 255, "transparent index must be 255")
    for role, (lo, hi) in ROLES.items():
        check(meta.get("roleRamps", {}).get(role) == list(range(lo, hi + 1)),
              f"{role} ramp mismatch")
    for clip, (start, length) in EXPECTED[name].items():
        entry = meta.get("clips", {}).get(clip, {})
        check((entry.get("start"), entry.get("len")) == (start, length),
              f"{clip} clip range mismatch")
    try:
        with Image.open(ROOT / (name + ".png")) as im:
            check(im.format == "PNG", "not a PNG")
            check(im.mode == "P", "PNG must be 8-bit indexed (mode P)")
            check(im.size == (frame[0] * count, frame[1]), "sheet dimensions mismatch")
            check(im.info.get("transparency") == 255, "transparency must be palette index 255")
            pal = im.getpalette()
            check(pal is not None and len(pal) >= 89 * 3, "missing palette")
            if pal:
                for i, rgb in enumerate(colors):
                    if tuple(pal[i*3:i*3+3]) != rgb:
                        errors.append(f"palette entry {i} differs from ntba.gpl")
                        break
            pixels = im.load()
            used = set(im.getdata())
            check(used <= set(range(89)) | {255},
                  f"illegal pixel indices: {sorted(used - (set(range(89)) | {255}))}")
            if im.size == (frame[0] * count, frame[1]):
                w, h = frame
                for n in range(count):
                    x0 = n * w
                    for x in range(w):
                        if pixels[x0+x, 0] != 255 or pixels[x0+x, h-1] != 255:
                            errors.append(f"frame {n}: top/bottom transparent margin violated")
                            break
                    for y in range(h):
                        if pixels[x0, y] != 255 or pixels[x0+w-1, y] != 255:
                            errors.append(f"frame {n}: left/right transparent margin violated")
                            break
    except (OSError, ValueError) as exc:
        errors.append(str(exc))
    return errors

def main():
    colors = palette()
    failures = 0
    for name in SHEETS:
        errors = validate(name, colors)
        print(f"{name}: " + ("PASS" if not errors else "FAIL"))
        for error in errors:
            print("  - " + error)
        failures += bool(errors)
    return 1 if failures else 0

if __name__ == "__main__":
    sys.exit(main())
