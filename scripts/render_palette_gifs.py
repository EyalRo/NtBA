#!/usr/bin/env python3
"""Render palette-swapped animated GIF previews of the player sprite sheets.

Usage: python3 scripts/render_palette_gifs.py [out_dir]
Requires: pip install Pillow
"""
import json, sys
from pathlib import Path
from PIL import Image
ROOT = Path(__file__).resolve().parents[1] / "assets" / "sprites"
OUT = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "previews"
SCALE = 5
RAMPS={"jersey":(14,17),"trim":(18,21),"shorts":(58,61),"socks":(22,25),"hair":(34,37)}
def ramp(dark,light):
    return [tuple(round(d+(l-d)*t/3) for d,l in zip(dark,light)) for t in range(4)]
THEMES={
 "bubblegum_pink":{"jersey":ramp((190,40,110),(255,190,225)),"trim":ramp((255,255,255),(255,255,255)),
    "shorts":ramp((150,30,90),(255,150,200)),"socks":ramp((200,70,130),(255,210,235)),"bg":(60,30,60)},
 "default":{"bg":(40,44,60)},
 "ocean_blue":{"jersey":ramp((10,50,130),(130,200,255)),"trim":ramp((230,230,240),(255,255,255)),
    "shorts":ramp((8,40,100),(90,160,240)),"bg":(25,40,50)},
 "lime_flash":{"jersey":ramp((40,120,10),(210,255,100)),"trim":ramp((20,20,20),(70,70,70)),
    "shorts":ramp((20,20,24),(90,90,100)),"hair":ramp((220,200,40),(255,245,140)),"bg":(30,30,40)},
}
ANIMS=[("players_side","run"),("players_side","dribble_run"),("players_side","jump"),("players_end","front_run"),("players_end","back_run")]
def render(sheet,clip,theme,t):
    meta=json.loads((ROOT/f"{sheet}.json").read_text()); im=Image.open(ROOT/f"{sheet}.png")
    pal=im.getpalette()
    for role,cols in t.items():
        if role=="bg": continue
        a,b=RAMPS[role]
        for i,c in enumerate(cols): pal[(a+i)*3:(a+i)*3+3]=c
    w,h=meta["frame"]; c=meta["clips"][clip]; frames=[]
    for n in range(c["start"],c["start"]+c["len"]):
        f=im.crop((n*w,0,n*w+w,h)); f.putpalette(pal); f.info["transparency"]=255
        rgba=f.convert("RGBA"); bg=Image.new("RGBA",f.size,t["bg"]+(255,)); bg.alpha_composite(rgba)
        frames.append(bg.convert("RGB").resize((w*SCALE,h*SCALE),Image.NEAREST))
    return frames,c.get("fps",None)
OUT.mkdir(exist_ok=True)
for tn,t in THEMES.items():
    for sheet,clip in ANIMS:
        fr,_=render(sheet,clip,t,t)
        fr[0].save(OUT/f"{tn}_{clip}.gif",save_all=True,append_images=fr[1:],duration=110 if len(fr)>2 else 220,loop=0,disposal=2)
        print(tn,clip,len(fr))
