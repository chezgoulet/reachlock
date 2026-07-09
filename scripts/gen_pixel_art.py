#!/usr/bin/env python3
"""Deterministic pixel-art generator for the REACHLOCK placeholder art pass.

Generates every sprite the framework scenes load through AssetLibrary:
  - character sheets  godot/mods/reachlock/assets/npcs/<id>_sheet.png
                      (4 rows: down/up/left/right x 4 walk frames, 24x32 each)
  - single portraits  godot/mods/reachlock/assets/npcs/<id>.png (front stand)
  - the player        godot/mods/reachlock/assets/player/character{,_sheet}.png
  - floor/wall tiles  godot/mods/reachlock/assets/tiles/<name>.png (16x16)
  - furniture props   godot/mods/reachlock/assets/props/<name>.png

Everything is drawn from a shared palette so the world reads as one place
(Stardew/SoM-adjacent: warm tones, 1px outlines, top-left light). The art is
intended as a high-quality placeholder a human artist can replace file-by-file
— the loader convention (AssetLibrary) never changes.

Deterministic: same input, same bytes. Run: python3 scripts/gen_pixel_art.py
"""
from __future__ import annotations

import os
import random

from PIL import Image, ImageDraw

REPO = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ASSETS = os.path.join(REPO, "godot", "mods", "reachlock", "assets")

# --- palette ------------------------------------------------------------------

PAL = {
    "outline":      (26, 22, 30, 255),
    "hull":         (94, 99, 110, 255),
    "hull_dark":    (68, 72, 82, 255),
    "hull_light":   (126, 132, 145, 255),
    "grate":        (58, 61, 70, 255),
    "warm":         (204, 146, 72, 255),
    "warm_dark":    (150, 100, 52, 255),
    "wood":         (104, 72, 52, 255),
    "wood_dark":    (76, 52, 38, 255),
    "pale":         (196, 200, 205, 255),
    "pale_dark":    (160, 164, 172, 255),
    "glow_amber":   (255, 186, 84, 255),
    "glow_blue":    (110, 190, 255, 255),
    "glow_cyan":    (96, 226, 219, 255),
    "glow_red":     (240, 84, 68, 255),
    "glow_green":   (120, 220, 130, 255),
    "screen":       (30, 48, 66, 255),
    "screen_line":  (86, 160, 190, 255),
    "shadow":       (0, 0, 0, 70),
}


def px(d: ImageDraw.ImageDraw, x, y, c):
    d.point((x, y), fill=c)


def rect(d, x0, y0, x1, y1, c):
    d.rectangle((x0, y0, x1, y1), fill=c)


def orect(d, x0, y0, x1, y1, fill, outline=PAL["outline"]):
    d.rectangle((x0, y0, x1, y1), fill=fill, outline=outline)


def shade(c, f):
    return (max(0, min(255, int(c[0] * f))), max(0, min(255, int(c[1] * f))),
            max(0, min(255, int(c[2] * f))), c[3] if len(c) > 3 else 255)


def save(img: Image.Image, *parts):
    path = os.path.join(ASSETS, *parts)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    img.save(path)
    print("wrote", os.path.relpath(path, REPO))


# --- characters -----------------------------------------------------------------
#
# 24x32 frame. Facing rows: 0 down, 1 up, 2 left, 3 right. Frames: stand,
# step-A, stand, step-B. Anatomy: head 10px, torso 9px, legs 8px, shoes 2px.

CHARACTERS = {
    # Crew
    "tib":       dict(skin=(214, 170, 132), hair=(70, 52, 40), style="short",
                      top=(96, 74, 52), top2=(120, 96, 66), pants=(60, 62, 72),
                      shoes=(42, 38, 40), extra="jacket"),      # worn flight jacket
    "tove":      dict(skin=(198, 154, 118), hair=(38, 34, 36), style="tail",
                      top=(88, 96, 88), top2=(108, 118, 106), pants=(70, 66, 58),
                      shoes=(46, 42, 40), extra="scarf", accent=(150, 60, 48)),
    "bardo":     dict(skin=(140, 100, 72), hair=(28, 26, 28), style="short",
                      top=(122, 104, 142), top2=(142, 124, 162), pants=(66, 60, 74),
                      shoes=(50, 44, 46), extra="datapad", accent=(110, 190, 255)),
    "doc_keene": dict(skin=(120, 84, 60), hair=(20, 18, 20), style="crop",
                      top=(210, 214, 218), top2=(188, 192, 198), pants=(76, 82, 92),
                      shoes=(52, 50, 52), extra="medcoat", accent=(180, 60, 60)),
    "risc":      dict(skin=(188, 142, 110), hair=(90, 84, 78), style="buzz",
                      top=(170, 110, 60), top2=(190, 130, 74), pants=(80, 74, 64),
                      shoes=(48, 44, 42), extra="toolbelt", accent=(220, 180, 90)),
    # Droids — metal skin, visor optics, no hair
    "prudence":  dict(skin=(168, 148, 178), hair=None, style="dome",
                      top=(120, 78, 130), top2=(140, 96, 150), pants=(88, 74, 96),
                      shoes=(60, 54, 66), extra="droid", accent=(96, 226, 219)),
    "boris":     dict(skin=(140, 152, 160), hair=None, style="dome",
                      top=(96, 116, 128), top2=(112, 134, 146), pants=(74, 86, 94),
                      shoes=(54, 60, 64), extra="droid_heavy", accent=(255, 186, 84)),
    # Station & Earth
    "doss":      dict(skin=(206, 160, 124), hair=(150, 140, 130), style="bun",
                      top=(146, 116, 62), top2=(168, 136, 76), pants=(62, 58, 54),
                      shoes=(44, 42, 40), extra="coat", accent=(210, 170, 90)),
    "grissom":   dict(skin=(196, 148, 112), hair=(84, 62, 46), style="buzz",
                      top=(112, 104, 88), top2=(128, 120, 102), pants=(70, 64, 56),
                      shoes=(46, 42, 40), extra="heavy", accent=(160, 140, 90)),
    "noor":      dict(skin=(132, 94, 66), hair=(24, 22, 26), style="wrap",
                      top=(70, 110, 100), top2=(86, 130, 118), pants=(64, 68, 62),
                      shoes=(48, 46, 44), extra="scarf", accent=(210, 190, 140)),
    "vex":       dict(skin=(190, 150, 120), hair=(120, 40, 40), style="tail",
                      top=(90, 50, 50), top2=(110, 64, 62), pants=(56, 50, 54),
                      shoes=(40, 36, 40), extra="jacket", accent=(220, 90, 70)),
    # The player: a neutral spacer in Reach colors
    "player":    dict(skin=(202, 158, 122), hair=(60, 48, 44), style="short",
                      top=(74, 104, 128), top2=(90, 124, 150), pants=(64, 66, 76),
                      shoes=(46, 44, 46), extra="jacket", accent=(110, 190, 255)),
}

FW, FH = 24, 32  # frame size


def draw_character_frame(spec: dict, facing: str, frame: int) -> Image.Image:
    img = Image.new("RGBA", (FW, FH), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    heavy = spec.get("extra") in ("droid_heavy", "heavy")
    half = 6 if heavy else 5              # torso half-width
    cx = FW // 2
    skin, top, top2 = spec["skin"], spec["top"], spec["top2"]
    pants, shoes = spec["pants"], spec["shoes"]
    accent = spec.get("accent", (200, 200, 200))
    droid = spec.get("extra", "").startswith("droid")

    # Walk cycle: legs alternate; body bobs 1px on step frames.
    step = {0: (0, 0), 1: (-2, 1), 2: (0, 0), 3: (2, 1)}[frame]
    leg_off, bob = step

    # -- legs & shoes (y 22..29 shoes 28..29), two 3px legs
    ly = 22 + bob
    lgap = 1
    lw = 3
    left_x0 = cx - lgap - lw
    right_x0 = cx + lgap
    lshift = leg_off if facing in ("left", "right") else 0
    l_dy = max(0, leg_off) if facing in ("down", "up") else 0
    r_dy = max(0, -leg_off) if facing in ("down", "up") else 0
    orect(d, left_x0 - lshift, ly + l_dy, left_x0 + lw - 1 - lshift, 27 + bob, pants)
    orect(d, right_x0 + lshift, ly + r_dy, right_x0 + lw - 1 + lshift, 27 + bob, pants)
    rect(d, left_x0 - lshift, 28 + bob, left_x0 + lw - 1 - lshift, 29 + bob, shoes)
    rect(d, right_x0 + lshift, 28 + bob, right_x0 + lw - 1 + lshift, 29 + bob, shoes)

    # -- torso (y 13..22); coat variants extend to 25
    ty0, ty1 = 13 + bob, 22 + bob
    coat = spec.get("extra") in ("coat", "medcoat", "jacket")
    if spec.get("extra") in ("coat", "medcoat"):
        ty1 = 25 + bob
    orect(d, cx - half, ty0, cx + half - 1, ty1, top)
    # two-tone: light from top-left
    rect(d, cx - half + 1, ty0 + 1, cx - 1, ty1 - 1, top2)
    if coat:  # lapel line
        d.line((cx, ty0 + 1, cx, ty1 - 1), fill=shade(top, 0.7))
    if spec.get("extra") == "toolbelt":
        rect(d, cx - half, ty1 - 1, cx + half - 1, ty1, accent)
    if spec.get("extra") == "scarf":
        rect(d, cx - half + 1, ty0, cx + half - 2, ty0 + 1, accent)
    if droid:  # chest indicator
        px(d, cx - 1, ty0 + 3, accent)

    # -- arms: 2px columns beside torso, swing with walk
    a_dy = leg_off if facing in ("left", "right") else 0
    arm_c = top if not droid else spec["skin"]
    orect(d, cx - half - 2, ty0 + 1 - a_dy // 2, cx - half - 1, ty0 + 7 - a_dy // 2, arm_c)
    orect(d, cx + half, ty0 + 1 + a_dy // 2, cx + half + 1, ty0 + 7 + a_dy // 2, arm_c)
    if spec.get("extra") == "datapad" and facing != "up":
        rect(d, cx + half, ty0 + 5, cx + half + 2, ty0 + 8, PAL["screen"])
        px(d, cx + half + 1, ty0 + 6, accent)

    # -- head (y 3..12): skin block + hair/dome
    hw = 5 if not heavy else 6
    orect(d, cx - hw, 4 + bob, cx + hw - 1, 12 + bob, skin)
    if droid:
        # visor strip instead of eyes; dome crown
        rect(d, cx - hw + 1, 4 + bob, cx + hw - 2, 5 + bob, shade(skin, 0.8))
        if facing != "up":
            vx0 = cx - hw + 1 if facing != "right" else cx - 1
            vx1 = cx + hw - 2 if facing != "left" else cx
            rect(d, vx0, 8 + bob, vx1, 9 + bob, accent)
    else:
        hair = spec["hair"]
        style = spec.get("style", "short")
        rect(d, cx - hw, 3 + bob, cx + hw - 1, 6 + bob, hair)
        if facing == "up":
            rect(d, cx - hw, 3 + bob, cx + hw - 1, 10 + bob, hair)
        if style == "tail" and facing != "down":
            rect(d, cx + (hw - 1 if facing != "left" else -hw), 7 + bob,
                 cx + (hw if facing != "left" else -hw + 1), 13 + bob, hair)
        if style == "bun":
            rect(d, cx - 2, 2 + bob, cx + 1, 3 + bob, hair)
        if style == "wrap":
            rect(d, cx - hw, 3 + bob, cx + hw - 1, 8 + bob, accent)
        if style == "crop":
            rect(d, cx - hw, 3 + bob, cx + hw - 1, 5 + bob, hair)
        # eyes
        if facing == "down":
            px(d, cx - 3, 9 + bob, PAL["outline"])
            px(d, cx + 2, 9 + bob, PAL["outline"])
        elif facing == "left":
            px(d, cx - 3, 9 + bob, PAL["outline"])
        elif facing == "right":
            px(d, cx + 2, 9 + bob, PAL["outline"])
    # medcoat: white coat over shoulders reads from all sides
    if spec.get("extra") == "medcoat":
        rect(d, cx - half, ty0, cx - half + 1, ty1, (222, 226, 230, 255))
        rect(d, cx + half - 2, ty0, cx + half - 1, ty1, (222, 226, 230, 255))

    return img


def gen_characters():
    rows = ["down", "up", "left", "right"]
    for cid, spec in CHARACTERS.items():
        sheet = Image.new("RGBA", (FW * 4, FH * 4), (0, 0, 0, 0))
        for r, facing in enumerate(rows):
            for f in range(4):
                frame = draw_character_frame(spec, facing, f)
                sheet.paste(frame, (f * FW, r * FH))
        single = draw_character_frame(spec, "down", 0)
        if cid == "player":
            save(sheet, "player", "character_sheet.png")
            save(single, "player", "character.png")
        else:
            save(sheet, "npcs", f"{cid}_sheet.png")
            save(single, "npcs", f"{cid}.png")


# --- tiles ----------------------------------------------------------------------

TILE = 16


def gen_tile(name, base, seams=True, rivets=False, grate=False, planks=False,
             noise=0.05, seed=7):
    rng = random.Random(seed + hash(name) % 1000)
    img = Image.new("RGBA", (TILE, TILE), base)
    d = ImageDraw.Draw(img)
    for y in range(TILE):
        for x in range(TILE):
            if rng.random() < noise:
                px(d, x, y, shade(base, rng.choice((0.92, 1.06))))
    if seams:
        d.line((0, 0, TILE - 1, 0), fill=shade(base, 1.12))
        d.line((0, 0, 0, TILE - 1), fill=shade(base, 1.08))
        d.line((0, TILE - 1, TILE - 1, TILE - 1), fill=shade(base, 0.8))
        d.line((TILE - 1, 0, TILE - 1, TILE - 1), fill=shade(base, 0.84))
    if rivets:
        for (x, y) in ((2, 2), (13, 2), (2, 13), (13, 13)):
            px(d, x, y, shade(base, 0.7))
    if grate:
        for i in range(0, TILE, 4):
            d.line((i, 0, i, TILE - 1), fill=shade(base, 0.78))
            d.line((0, i, TILE - 1, i), fill=shade(base, 0.78))
    if planks:
        for y in (5, 10):
            d.line((0, y, TILE - 1, y), fill=shade(base, 0.72))
        px(d, 4, 2, shade(base, 0.7)); px(d, 11, 8, shade(base, 0.7))
    save(img, "tiles", f"{name}.png")


def gen_tiles():
    gen_tile("floor_deck", PAL["hull"], rivets=True)
    gen_tile("floor_grate", PAL["grate"], grate=True, seams=False)
    gen_tile("floor_med", PAL["pale"], noise=0.02)
    gen_tile("floor_galley", (122, 108, 82, 255))
    gen_tile("floor_quarters", (84, 88, 108, 255))
    gen_tile("floor_cargo", (96, 88, 72, 255), rivets=True)
    gen_tile("floor_bar", PAL["wood"], planks=True, seams=False)
    gen_tile("floor_office", (110, 96, 84, 255), noise=0.03)
    gen_tile("floor_cryo", (96, 128, 138, 255))
    # wall band: darker with a lit top edge
    img = Image.new("RGBA", (TILE, TILE), PAL["hull_dark"])
    d = ImageDraw.Draw(img)
    rect(d, 0, 0, TILE - 1, 2, shade(PAL["hull_dark"], 1.35))
    rect(d, 0, TILE - 3, TILE - 1, TILE - 1, shade(PAL["hull_dark"], 0.7))
    save(img, "tiles", "wall.png")


# --- props ----------------------------------------------------------------------

def prop(name, w, h):
    img = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    return img, ImageDraw.Draw(img), lambda: save(img, "props", f"{name}.png")


def screen_panel(d, x0, y0, x1, y1, line=PAL["screen_line"], on=True):
    orect(d, x0, y0, x1, y1, PAL["screen"])
    if on:
        for i, y in enumerate(range(y0 + 2, y1 - 1, 3)):
            d.line((x0 + 2, y, x1 - 2 - (i % 3), y), fill=line)


def gen_props():
    # -- consoles & ship stations
    img, d, done = prop("pilot_console", 34, 26)
    orect(d, 2, 10, 31, 23, PAL["hull_dark"])
    screen_panel(d, 4, 2, 29, 12, PAL["glow_cyan"])
    rect(d, 6, 16, 12, 20, PAL["hull_light"]); rect(d, 20, 16, 27, 20, PAL["hull_light"])
    px(d, 15, 18, PAL["glow_amber"]); px(d, 17, 18, PAL["glow_green"])
    done()

    img, d, done = prop("nav_screen", 28, 22)
    screen_panel(d, 1, 1, 26, 18, PAL["glow_blue"])
    d.line((6, 14, 12, 8), fill=PAL["glow_amber"]); d.line((12, 8, 20, 6), fill=PAL["glow_amber"])
    rect(d, 10, 19, 17, 21, PAL["hull_dark"])
    done()

    img, d, done = prop("weapons_console", 30, 24)
    orect(d, 2, 8, 27, 21, PAL["hull_dark"])
    screen_panel(d, 4, 2, 25, 11, PAL["glow_red"])
    d.ellipse((12, 4, 17, 9), outline=PAL["glow_red"])
    px(d, 14, 6, PAL["glow_red"]); px(d, 15, 6, PAL["glow_red"])
    rect(d, 6, 14, 10, 18, PAL["hull_light"]); rect(d, 18, 14, 23, 18, PAL["hull_light"])
    done()

    img, d, done = prop("scanner_console", 28, 24)
    orect(d, 2, 10, 25, 21, PAL["hull_dark"])
    d.ellipse((5, 1, 22, 16), fill=PAL["screen"], outline=PAL["outline"])
    d.ellipse((9, 5, 18, 12), outline=shade(PAL["glow_green"], 0.7))
    d.line((13, 8, 20, 3), fill=PAL["glow_green"])
    px(d, 10, 7, PAL["glow_green"]); px(d, 16, 11, PAL["glow_amber"])
    done()

    img, d, done = prop("engineering_panel", 32, 28)
    orect(d, 1, 2, 30, 25, PAL["hull_dark"])
    for i, x in enumerate(range(5, 27, 5)):
        h = (3, 8, 5, 10, 6)[i % 5]
        rect(d, x, 20 - h, x + 2, 20, (PAL["glow_amber"] if i % 2 else PAL["glow_green"]))
    rect(d, 4, 22, 27, 23, shade(PAL["hull_dark"], 1.3))
    done()

    img, d, done = prop("drive_core", 36, 40)
    orect(d, 6, 2, 29, 37, PAL["hull_dark"])
    for y in (6, 12, 18, 24, 30):
        rect(d, 8, y, 27, y + 2, shade(PAL["hull_dark"], 1.25))
    rect(d, 14, 4, 21, 35, shade(PAL["glow_amber"], 0.55))
    rect(d, 16, 4, 19, 35, PAL["glow_amber"])
    done()

    img, d, done = prop("power_grid", 26, 20)
    screen_panel(d, 0, 0, 25, 19, PAL["glow_amber"], on=False)
    for i, x in enumerate((4, 12, 20)):
        c = (PAL["glow_red"], PAL["glow_amber"], PAL["glow_cyan"])[i]
        rect(d, x, 4, x + 2, 15, shade(c, 0.4)); rect(d, x, 9 - i, x + 2, 15, c)
    done()

    # -- cryo & med
    img, d, done = prop("cryo_pod", 22, 42)
    orect(d, 2, 2, 19, 39, PAL["hull_light"])
    orect(d, 4, 5, 17, 26, shade(PAL["glow_cyan"], 0.45))
    rect(d, 6, 8, 15, 22, shade(PAL["glow_cyan"], 0.65))
    px(d, 8, 32, PAL["glow_green"]); px(d, 12, 32, PAL["glow_amber"])
    rect(d, 5, 35, 16, 36, PAL["hull_dark"])
    done()

    img, d, done = prop("med_shelf", 30, 24)
    orect(d, 1, 1, 28, 22, PAL["pale_dark"])
    for y in (6, 13, 20):
        d.line((2, y, 27, y), fill=shade(PAL["pale_dark"], 0.7))
    for x, c in ((4, PAL["glow_red"]), (9, PAL["pale"]), (14, PAL["glow_cyan"]),
                 (19, PAL["pale"]), (24, PAL["glow_green"])):
        rect(d, x, 3, x + 2, 5, c); rect(d, x, 9, x + 2, 12, shade(c, 0.85))
    done()

    img, d, done = prop("med_monitor", 20, 18)
    screen_panel(d, 0, 0, 19, 14, PAL["glow_green"], on=False)
    d.line((2, 8, 6, 8), fill=PAL["glow_green"]); d.line((6, 8, 8, 3), fill=PAL["glow_green"])
    d.line((8, 3, 10, 12), fill=PAL["glow_green"]); d.line((10, 12, 17, 8), fill=PAL["glow_green"])
    rect(d, 7, 15, 12, 17, PAL["hull_dark"])
    done()

    # -- galley & quarters
    img, d, done = prop("galley_table", 40, 28)
    orect(d, 2, 6, 37, 21, PAL["wood"])
    rect(d, 3, 7, 36, 12, shade(PAL["wood"], 1.2))
    rect(d, 4, 22, 7, 26, PAL["wood_dark"]); rect(d, 32, 22, 35, 26, PAL["wood_dark"])
    rect(d, 10, 9, 16, 14, PAL["pale"])  # charts on the table
    rect(d, 20, 10, 27, 15, shade(PAL["pale"], 0.9))
    done()

    img, d, done = prop("chart_table", 34, 24)
    orect(d, 2, 4, 31, 19, PAL["hull_dark"])
    screen_panel(d, 4, 6, 29, 17, PAL["glow_blue"])
    rect(d, 4, 20, 7, 23, PAL["hull_dark"]); rect(d, 26, 20, 29, 23, PAL["hull_dark"])
    done()

    img, d, done = prop("food_dispenser", 20, 30)
    orect(d, 1, 1, 18, 28, PAL["hull_light"])
    rect(d, 4, 5, 15, 10, PAL["screen"]); px(d, 6, 7, PAL["glow_green"])
    orect(d, 5, 15, 14, 22, PAL["hull_dark"])
    rect(d, 7, 24, 12, 26, PAL["warm"])
    done()

    img, d, done = prop("porthole", 18, 18)
    d.ellipse((0, 0, 17, 17), fill=PAL["hull_light"], outline=PAL["outline"])
    d.ellipse((3, 3, 14, 14), fill=(12, 14, 26, 255))
    px(d, 6, 7, (240, 244, 255, 255)); px(d, 11, 5, (200, 210, 240, 255))
    px(d, 9, 11, (180, 190, 220, 255))
    done()

    img, d, done = prop("bunk", 38, 22)
    orect(d, 1, 4, 36, 19, PAL["hull_dark"])
    rect(d, 3, 6, 34, 12, (86, 96, 130, 255))
    rect(d, 3, 6, 12, 12, PAL["pale"])  # pillow
    rect(d, 3, 13, 34, 17, shade((86, 96, 130, 255), 0.8))
    done()

    img, d, done = prop("locker", 18, 30)
    orect(d, 1, 1, 16, 28, PAL["hull"])
    d.line((8, 2, 8, 27), fill=shade(PAL["hull"], 0.72))
    px(d, 6, 14, PAL["outline"]); px(d, 11, 14, PAL["outline"])
    rect(d, 2, 3, 15, 5, shade(PAL["hull"], 1.2))
    done()

    # -- cargo & mining
    img, d, done = prop("crate", 22, 20)
    orect(d, 1, 1, 20, 18, PAL["warm_dark"])
    rect(d, 2, 2, 19, 8, shade(PAL["warm_dark"], 1.2))
    d.line((1, 9, 20, 9), fill=PAL["outline"])
    rect(d, 8, 4, 13, 7, shade(PAL["warm"], 1.1))
    done()

    img, d, done = prop("crate_stack", 34, 34)
    for (x, y) in ((2, 16), (16, 16), (9, 2)):
        orect(d, x, y, x + 15, y + 15, PAL["warm_dark"])
        rect(d, x + 1, y + 1, x + 14, y + 6, shade(PAL["warm_dark"], 1.2))
    done()

    img, d, done = prop("mining_rig", 36, 32)
    orect(d, 4, 12, 31, 29, PAL["hull_dark"])
    rect(d, 8, 4, 12, 12, PAL["hull_light"])   # arm
    rect(d, 6, 2, 14, 6, PAL["warm"])          # drill head
    px(d, 24, 16, PAL["glow_amber"]); px(d, 27, 16, PAL["glow_red"])
    rect(d, 8, 20, 27, 24, shade(PAL["hull_dark"], 1.25))
    done()

    img, d, done = prop("airlock_door", 30, 36)
    orect(d, 1, 1, 28, 34, PAL["hull_light"])
    orect(d, 5, 4, 24, 31, PAL["hull_dark"])
    d.line((14, 5, 14, 30), fill=shade(PAL["hull_dark"], 1.3))
    for y in (8, 27):
        rect(d, 7, y, 22, y + 1, PAL["warm"])
    px(d, 26, 17, PAL["glow_green"])
    done()

    img, d, done = prop("cargo_pallet", 30, 16)
    rect(d, 1, 10, 28, 14, PAL["wood_dark"])
    orect(d, 4, 2, 14, 10, PAL["warm_dark"]); orect(d, 16, 4, 25, 10, (92, 102, 88, 255))
    done()

    # -- the Interval bar
    img, d, done = prop("bar_counter", 56, 26)
    orect(d, 1, 8, 54, 23, PAL["wood"])
    rect(d, 2, 9, 53, 13, shade(PAL["wood"], 1.25))
    rect(d, 2, 18, 53, 22, PAL["wood_dark"])
    for x in (10, 24, 40):  # glasses on the counter
        rect(d, x, 5, x + 2, 8, PAL["pale"])
    done()

    img, d, done = prop("bottle_shelf", 40, 26)
    orect(d, 1, 1, 38, 24, PAL["wood_dark"])
    for y in (8, 16, 23):
        d.line((2, y, 37, y), fill=shade(PAL["wood_dark"], 0.7))
    rng = random.Random(11)
    for row_y in (3, 11, 18):
        for x in range(4, 35, 4):
            c = rng.choice((PAL["glow_amber"], (150, 60, 48, 255), PAL["glow_green"],
                            (110, 130, 190, 255), PAL["pale"]))
            rect(d, x, row_y, x + 1, row_y + 4, c)
    done()

    img, d, done = prop("bar_stool", 12, 14)
    d.ellipse((1, 1, 10, 6), fill=PAL["warm_dark"], outline=PAL["outline"])
    rect(d, 5, 7, 6, 12, PAL["hull_dark"])
    done()

    img, d, done = prop("stool_fallen", 16, 10)
    d.ellipse((0, 3, 8, 9), fill=PAL["warm_dark"], outline=PAL["outline"])
    rect(d, 9, 5, 14, 6, PAL["hull_dark"])
    done()

    img, d, done = prop("table_round", 26, 20)
    d.ellipse((1, 1, 24, 14), fill=PAL["wood"], outline=PAL["outline"])
    d.ellipse((3, 3, 22, 9), fill=shade(PAL["wood"], 1.2))
    rect(d, 11, 14, 14, 18, PAL["wood_dark"])
    done()

    img, d, done = prop("table_broken", 28, 16)
    d.polygon(((1, 8), (14, 2), (14, 12), (2, 14)), fill=PAL["wood"], outline=PAL["outline"])
    d.polygon(((16, 4), (26, 8), (24, 14), (15, 13)), fill=shade(PAL["wood"], 0.85),
              outline=PAL["outline"])
    rect(d, 6, 13, 8, 15, PAL["wood_dark"])
    done()

    img, d, done = prop("terminal", 22, 28)
    orect(d, 2, 2, 19, 25, PAL["hull"])
    screen_panel(d, 4, 4, 17, 14, PAL["glow_green"])
    rect(d, 5, 17, 16, 22, PAL["hull_dark"])
    px(d, 7, 19, PAL["glow_amber"]); px(d, 10, 19, PAL["glow_green"])
    done()

    img, d, done = prop("terminal_wrecked", 24, 28)
    orect(d, 2, 4, 19, 27, PAL["hull"])
    orect(d, 4, 6, 17, 16, (16, 16, 20, 255))
    d.line((6, 8, 15, 14), fill=PAL["outline"])  # cracked dark screen
    d.line((8, 14, 14, 8), fill=PAL["outline"])
    px(d, 20, 6, PAL["glow_amber"]); px(d, 22, 3, PAL["glow_amber"])
    px(d, 21, 9, PAL["glow_red"])
    rect(d, 3, 20, 18, 26, shade(PAL["hull"], 0.8))
    done()

    img, d, done = prop("extinguisher_residue", 40, 18)
    rng = random.Random(23)
    for _ in range(90):
        x, y = rng.randint(0, 39), rng.randint(0, 17)
        if ((x - 20) / 20.0) ** 2 + ((y - 9) / 9.0) ** 2 < 1.0:
            px(d, x, y, (222, 226, 232, rng.randint(60, 140)))
    done()

    # -- offices, shops, memorials
    img, d, done = prop("desk", 36, 24)
    orect(d, 2, 6, 33, 19, PAL["wood_dark"])
    rect(d, 3, 7, 32, 11, shade(PAL["wood_dark"], 1.25))
    rect(d, 5, 8, 12, 10, PAL["pale"])  # ledgers
    rect(d, 24, 7, 30, 12, PAL["screen"]); px(d, 26, 9, PAL["glow_amber"])
    rect(d, 4, 20, 7, 23, PAL["outline"]); rect(d, 28, 20, 31, 23, PAL["outline"])
    done()

    img, d, done = prop("shop_counter", 48, 24)
    orect(d, 1, 8, 46, 21, PAL["hull_dark"])
    rect(d, 2, 9, 45, 13, shade(PAL["hull_dark"], 1.3))
    for x, c in ((6, PAL["glow_amber"]), (16, PAL["glow_cyan"]), (26, PAL["glow_red"]),
                 (36, PAL["glow_green"])):
        orect(d, x, 2, x + 6, 8, PAL["hull"])
        px(d, x + 3, 5, c)
    done()

    img, d, done = prop("memorial_wall", 44, 30)
    orect(d, 1, 1, 42, 28, PAL["hull_dark"])
    rng = random.Random(847)
    for y in range(5, 26, 3):
        for x in range(5, 39, 6):
            if rng.random() < 0.9:
                d.line((x, y, x + rng.randint(2, 4), y), fill=PAL["pale_dark"])
    rect(d, 17, 2, 26, 3, PAL["warm"])
    done()

    img, d, done = prop("viewport_wide", 60, 22)
    orect(d, 0, 0, 59, 21, PAL["hull_light"])
    rect(d, 2, 2, 57, 19, (12, 14, 26, 255))
    rng = random.Random(5)
    for _ in range(26):
        x, y = rng.randint(3, 56), rng.randint(3, 18)
        px(d, x, y, (rng.randint(170, 250),) * 3 + (255,))
    done()

    img, d, done = prop("plant", 14, 18)
    rect(d, 4, 12, 9, 16, PAL["warm_dark"])
    for (x, y) in ((3, 6), (6, 3), (9, 6), (6, 8), (2, 9), (10, 9)):
        rect(d, x, y, x + 2, y + 3, (74, 120, 66, 255))
    done()

    # -- Sprint 3: two decks, the landing bay, ore processing, damage
    img, d, done = prop("ladder", 24, 44)
    orect(d, 4, 0, 7, 43, PAL["hull_light"])
    orect(d, 16, 0, 19, 43, PAL["hull_light"])
    for y in range(3, 42, 6):
        rect(d, 7, y, 16, y + 1, shade(PAL["hull_light"], 0.85))
    rect(d, 0, 0, 23, 2, PAL["warm"])  # hazard lip at the hatch
    px(d, 2, 1, PAL["outline"]); px(d, 21, 1, PAL["outline"])
    done()

    img, d, done = prop("shuttle", 96, 56)
    # The grafted-on shuttle: stubby lifting body, big enough to read as a craft
    d.polygon(((6, 28), (20, 12), (72, 10), (90, 24), (90, 36), (72, 46), (20, 44)),
              fill=PAL["hull"], outline=PAL["outline"])
    d.polygon(((20, 14), (70, 12), (84, 24), (20, 26)), fill=shade(PAL["hull"], 1.18))
    orect(d, 66, 16, 82, 26, PAL["screen"])  # canopy
    px(d, 70, 19, PAL["glow_cyan"]); px(d, 76, 21, PAL["glow_cyan"])
    rect(d, 8, 30, 18, 34, PAL["warm"])      # engine cowls
    rect(d, 8, 38, 18, 42, PAL["warm_dark"])
    for x in (30, 44, 58):
        d.line((x, 14, x, 44), fill=shade(PAL["hull"], 0.8))
    rect(d, 36, 30, 54, 42, PAL["hull_dark"])  # hatch
    px(d, 52, 36, PAL["glow_green"])
    done()

    img, d, done = prop("landing_clamp", 40, 14)
    rect(d, 2, 8, 37, 12, PAL["hull_dark"])
    for x in (4, 18, 32):
        orect(d, x, 2, x + 4, 9, PAL["hull_light"])
    px(d, 20, 10, PAL["glow_amber"])
    done()

    img, d, done = prop("ore_processor", 52, 48)
    orect(d, 4, 8, 47, 45, PAL["hull_dark"])
    orect(d, 10, 0, 30, 12, PAL["hull"])         # intake hopper
    d.polygon(((12, 12), (28, 12), (24, 20), (16, 20)), fill=shade(PAL["hull"], 0.8))
    for y in (24, 32, 40):
        rect(d, 8, y, 43, y + 2, shade(PAL["hull_dark"], 1.25))
    rect(d, 14, 26, 37, 30, shade(PAL["glow_amber"], 0.5))  # crusher glow
    rect(d, 18, 26, 33, 30, PAL["glow_amber"])
    orect(d, 38, 2, 46, 10, PAL["screen"]); px(d, 41, 5, PAL["glow_green"])
    done()

    img, d, done = prop("ore_hopper", 30, 24)
    d.polygon(((2, 2), (27, 2), (22, 16), (7, 16)), fill=PAL["hull"], outline=PAL["outline"])
    rect(d, 10, 16, 19, 22, PAL["hull_dark"])
    rng = random.Random(31)
    for _ in range(14):  # raw ore chunks in the mouth
        x, y = rng.randint(5, 24), rng.randint(3, 8)
        px(d, x, y, rng.choice((PAL["warm_dark"], (140, 120, 90, 255), PAL["grate"])))
    done()

    img, d, done = prop("handrail", 40, 10)
    rect(d, 0, 2, 39, 4, PAL["warm"])
    for x in (2, 19, 36):
        rect(d, x, 4, x + 1, 9, PAL["hull_light"])
    done()

    # Damage decals: two frames each; the engine flickers between them.
    for frame, seed in (("a", 41), ("b", 87)):
        img, d, done = prop(f"damage_fire_{frame}", 26, 30)
        rng = random.Random(seed)
        flames = ((13, 4), (6, 12), (20, 10), (10, 8), (16, 6))
        for i, (fx, fy) in enumerate(flames):
            h = rng.randint(10, 18)
            d.polygon(((fx - 3, 28), (fx, fy + (3 if frame == "b" else 0)), (fx + 3, 28)),
                      fill=(240, 120 + rng.randint(0, 60), 40, 255))
            d.polygon(((fx - 1, 28), (fx, fy + 6), (fx + 1, 28)),
                      fill=(255, 220, 120, 255))
        rect(d, 2, 27, 23, 29, (30, 24, 22, 255))  # scorched base
        done()

        img, d, done = prop(f"damage_sparks_{frame}", 24, 24)
        rng = random.Random(seed + 3)
        orect(d, 6, 8, 17, 20, PAL["hull_dark"])  # torn panel
        d.line((8, 10, 15, 17), fill=PAL["outline"])
        for _ in range(8):
            x, y = rng.randint(2, 21), rng.randint(1, 14)
            px(d, x, y, rng.choice((PAL["glow_amber"], (255, 240, 180, 255), PAL["glow_red"])))
            if frame == "b":
                px(d, x + 1, y + 1, (255, 240, 180, 160))
        done()

    img, d, done = prop("damage_breach", 28, 24)
    d.ellipse((4, 4, 23, 19), fill=(10, 10, 14, 255), outline=PAL["outline"])
    d.ellipse((8, 7, 19, 16), fill=(4, 4, 8, 255))
    for (x, y) in ((3, 10), (24, 8), (14, 2), (12, 21)):  # peeled hull petals
        rect(d, x, y, x + 2, y + 2, PAL["hull_light"])
    px(d, 25, 18, PAL["glow_red"])
    done()

    img, d, done = prop("scorch_mark", 34, 16)
    rng = random.Random(53)
    for _ in range(70):
        x, y = rng.randint(0, 33), rng.randint(0, 15)
        if ((x - 17) / 17.0) ** 2 + ((y - 8) / 8.0) ** 2 < 1.0:
            px(d, x, y, (26, 22, 24, rng.randint(70, 160)))
    done()

    img, d, done = prop("repair_locker", 20, 28)
    orect(d, 1, 1, 18, 26, (120, 60, 44, 255))
    rect(d, 2, 2, 17, 5, shade((120, 60, 44, 255), 1.2))
    d.line((10, 2, 10, 25), fill=shade((120, 60, 44, 255), 0.7))
    # white cross: the damage-control locker
    rect(d, 4, 10, 8, 12, PAL["pale"]); rect(d, 5, 8, 7, 14, PAL["pale"])
    px(d, 14, 13, PAL["glow_amber"])
    done()


if __name__ == "__main__":
    gen_characters()
    gen_tiles()
    gen_props()
    print("pixel art pass complete")
