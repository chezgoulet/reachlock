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
import zlib

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
# The Terraria-school pass: 32x48 frames, big readable head (~1/3 of height),
# expressive hair/headgear as the identity silhouette, layered body, 3-step
# shading ramps (base, top-left light, bottom-right dark), 1px outline, walk
# cycles with visible leg/arm swing and hair bounce.
#
# Wardrobe comes from npc data (npc schema `wardrobe`): no aliens have been
# discovered — humans dressed by their biome and their work, droids as
# chassis with humanizing touches. Facing rows: 0 down, 1 up, 2 left,
# 3 right. Frames: stand, step-A, stand, step-B.

import json

NPCS_DIR = os.path.join(REPO, "godot", "mods", "reachlock", "npcs")

FW, FH = 32, 48  # frame size — CharacterSprite.FRAME mirrors this

# The player: a neutral Reach spacer. Engine-side art, so the wardrobe
# lives here rather than in a soul file.
PLAYER_WARDROBE = dict(
    culture="reach", work="spacer",
    skin=[202, 158, 122], hair=dict(style="short", color=[62, 48, 42]),
    palette=dict(top=[74, 112, 142], pants=[62, 66, 80], accent=[110, 190, 255]),
    gear=["jacket"],
)

CULTURE_DEFAULTS = {
    # culture -> (top, pants, accent) when a wardrobe omits its palette
    "reach": ([140, 130, 100], [88, 78, 60], [220, 180, 90]),
    "station": ([110, 118, 148], [66, 70, 88], [110, 190, 255]),
    "earth_remnant": ([110, 92, 64], [70, 66, 58], [190, 160, 110]),
    "corp_charter": ([84, 96, 140], [52, 54, 66], [228, 176, 96]),
    "droid": ([120, 130, 142], [84, 92, 102], [96, 226, 219]),
}


def load_wardrobes() -> dict:
    """Every npc soul file that ships a wardrobe block gets a sheet."""
    specs = {}
    for name in sorted(os.listdir(NPCS_DIR)):
        if not name.endswith(".json"):
            continue
        with open(os.path.join(NPCS_DIR, name), encoding="utf-8") as fh:
            data = json.load(fh)
        if "wardrobe" in data:
            specs[data["id"]] = data["wardrobe"]
    return specs


def _rgb(value, fallback):
    if isinstance(value, (list, tuple)) and len(value) >= 3:
        return (int(value[0]), int(value[1]), int(value[2]), 255)
    return tuple(fallback) + (255,)


def ramp(c):
    """The 3-step ramp: (light, base, dark) — light from top-left."""
    return shade(c, 1.22), c, shade(c, 0.72)


def draw_character_frame(wardrobe: dict, facing: str, frame: int) -> Image.Image:
    img = Image.new("RGBA", (FW, FH), (0, 0, 0, 0))
    d = ImageDraw.Draw(img)
    culture = wardrobe.get("culture", "station")
    gear = wardrobe.get("gear", [])
    droid = culture == "droid"
    heavy = "heavy" in gear
    defaults = CULTURE_DEFAULTS.get(culture, CULTURE_DEFAULTS["station"])
    palette = wardrobe.get("palette", {})
    skin = _rgb(wardrobe.get("skin"), [200, 156, 120])
    top = _rgb(palette.get("top"), defaults[0])
    pants = _rgb(palette.get("pants"), defaults[1])
    accent = _rgb(palette.get("accent"), defaults[2])
    hair_spec = wardrobe.get("hair", {}) or {}
    hair = _rgb(hair_spec.get("color"), [50, 42, 38])
    style = hair_spec.get("style", "dome" if droid else "short")
    top_l, _, top_d = ramp(top)
    pants_l, _, pants_d = ramp(pants)
    skin_l, _, skin_d = ramp(skin)

    cx = FW // 2
    side = facing in ("left", "right")
    mirror = facing == "right"

    # Walk cycle: stride alternates; body lifts 1px on step frames; hair
    # bounce reads as the hair lagging that lift by one pixel.
    stride = {0: 0, 1: -3, 2: 0, 3: 3}[frame]
    bob = -1 if frame in (1, 3) else 0
    hair_bob = bob + (1 if frame in (1, 3) else 0)  # the bounce

    # Side views are genuinely narrower — a person in profile, not a front
    # view with one eye.
    half = (8 if heavy else 6) if not side else (6 if heavy else 4)
    hw = (8 if heavy else 7) if not side else (6 if heavy else 5)
    coat = ("coat" in gear) or ("medcoat" in gear) or culture == "earth_remnant"

    # ---- legs & boots (y 34..46)
    ly0, ly1 = 34 + bob, 42 + bob
    boot = shade(pants, 0.55)
    boot_h = 4 if culture == "reach" else 3   # sealed boots run taller
    if side:
        # Profile stride: the legs scissor around cx, the whole ±3px of
        # it. Far leg first, in shadow; near leg over it, lit; the near
        # boot grows a toe pointing the way we walk.
        far = -stride
        near = stride
        orect(d, cx - 2 + far, ly0 + 1, cx + 2 + far, ly1, pants_d)
        rect(d, cx - 2 + far, ly1 - boot_h + 1, cx + 2 + far, ly1 + 3, shade(boot, 0.75))
        orect(d, cx - 2 + near, ly0, cx + 2 + near, ly1, pants)
        rect(d, cx - 1 + near, ly0 + 1, cx + near, ly1 - boot_h, pants_l)
        rect(d, cx - 2 + near, ly1 - boot_h + 1, cx + 2 + near, ly1 + 3, boot)
        toe_dx = 3 if mirror else -3
        rect(d, cx + near + (1 if mirror else -2) + toe_dx, ly1 + 1,
             cx + near + (2 if mirror else -1) + toe_dx, ly1 + 3, boot)
    else:
        lw = 5
        left_x0 = cx - 1 - lw
        right_x0 = cx + 1
        l_dy = 2 if stride > 0 else 0   # the stepping leg lifts, visibly
        r_dy = 2 if stride < 0 else 0
        orect(d, left_x0, ly0 + l_dy, left_x0 + lw - 1, ly1, pants)
        orect(d, right_x0, ly0 + r_dy, right_x0 + lw - 1, ly1, pants)
        rect(d, left_x0 + 1, ly0 + l_dy + 1, left_x0 + 2, ly1 - boot_h, pants_l)
        rect(d, right_x0 + lw - 2, ly0 + r_dy + 1, right_x0 + lw - 1, ly1 - boot_h, pants_d)
        rect(d, left_x0, ly1 - boot_h + 1 + l_dy, left_x0 + lw - 1, ly1 + 3, boot)
        rect(d, right_x0, ly1 - boot_h + 1 + r_dy, right_x0 + lw - 1, ly1 + 3, boot)
    if culture == "reach" and not droid and not side:
        # sealed-boot clasp catches the light
        px(d, cx - 3, ly1 + 1, shade(accent, 0.9))
        px(d, cx + 2, ly1 + 1, shade(accent, 0.9))

    # ---- torso (y 18..34); coats run longer and a pixel wider at the hip
    ty0, ty1 = 18 + bob, 34 + bob
    tx0, tx1 = cx - half, cx + half - 1
    if coat:
        ty1 = 38 + bob
        orect(d, tx0 - 1, ty0 + 8, tx1 + 1, ty1, shade(top, 0.9))  # skirt of the coat
    orect(d, tx0, ty0, tx1, min(ty1, 34 + bob) if coat else ty1, top)
    rect(d, tx0 + 1, ty0 + 1, cx - 1, (ty1 if not coat else 33 + bob) - 1, top_l)
    rect(d, tx1 - 1, ty0 + 2, tx1 - 1, (ty1 if not coat else 33 + bob) - 1, top_d)

    # culture layering on the torso; front-of-garment details (zips,
    # lapels, crosses) only exist where a front is visible
    front = facing == "down"
    if culture == "station" and not droid:
        # layered synthetics: a vest panel over an under-layer
        rect(d, tx0 + 1, ty0 + 7, tx1 - 1, ty0 + 8, shade(top, 0.62))
        rect(d, tx0 + 2, ty0 + 9, tx1 - 2, ty1 - 2, shade(top, 0.86))
        if front:
            d.line((cx, ty0 + 1, cx, ty0 + 6), fill=shade(accent, 0.85))  # collar zip
    if culture == "corp_charter":
        # crisp: shoulder bars and a belt line, nothing out of place
        if not side:
            rect(d, tx0 + 1, ty0, tx0 + 3, ty0 + 1, accent)
            rect(d, tx1 - 3, ty0, tx1 - 1, ty0 + 1, accent)
        rect(d, tx0 + 1, ty1 - 3, tx1 - 1, ty1 - 3, shade(top, 0.5))
    if culture == "earth_remnant":
        # old-world fabric: a visible hem and a patch pocket
        d.line((tx0, ty0 + 12, tx1, ty0 + 12), fill=shade(top, 0.7))
        if front:
            rect(d, tx0 + 2, ty0 + 9, tx0 + 4, ty0 + 11, shade(top, 0.8))
    if "jacket" in gear and front:
        d.line((cx, ty0 + 1, cx, ty1 - 1), fill=shade(top, 0.6))       # open front
        rect(d, cx - 2, ty0, cx + 1, ty0 + 1, top_d)                    # collar
    if "medcoat" in gear:
        rect(d, tx0, ty0, tx0 + 1, ty1, (232, 236, 240, 255))
        rect(d, tx1 - 1, ty0, tx1, ty1, (232, 236, 240, 255))
        if front:
            rect(d, cx + 2, ty0 + 4, cx + 4, ty0 + 6, (196, 60, 56, 255))  # the cross
    if "toolbelt" in gear:
        rect(d, tx0, ty1 - 2, tx1, ty1 - 1, shade(accent, 0.8))
        rect(d, cx + 2, ty1 - 3, cx + 4, ty1, shade(accent, 0.55))      # pouch
    if "scarf" in gear:
        rect(d, tx0 + 1, ty0 - 1, tx1 - 1, ty0 + 1, accent)
        if front:
            rect(d, cx - 1, ty0 + 1, cx, ty0 + 5, shade(accent, 0.85))  # tail of it
    if droid:
        # chassis seams and the humanizing touch of a heartbeat lamp
        d.line((tx0 + 2, ty0 + 5, tx1 - 2, ty0 + 5), fill=shade(top, 0.6))
        if front:
            d.line((cx, ty0 + 6, cx, ty1 - 2), fill=shade(top, 0.6))
            if "chest_lamp" in gear:
                rect(d, cx - 1, ty0 + 2, cx, ty0 + 3, accent)

    # ---- arms: 3px columns, swinging opposite the legs
    swing = -stride if side else (1 if frame == 1 else -1 if frame == 3 else 0)
    ay0 = ty0 + 1
    arm_len = 11
    sleeve = top if not droid else skin
    sleeve_d = shade(sleeve, 0.72)
    if side:
        # One visible arm sweeps the full stride in front of the torso;
        # no outline (it would read as a slab), just a darker sleeve with
        # a lit edge and a hand.
        ax = cx + swing
        arm_sleeve = shade(top, 0.8) if not droid else shade(skin, 0.84)
        rect(d, ax - 1, ay0 + 1, ax + 1, ay0 + arm_len, arm_sleeve)
        rect(d, ax - 1, ay0 + 2, ax - 1, ay0 + arm_len - 3, shade(top, 0.95) if not droid else skin)
        rect(d, ax - 1, ay0 + arm_len - 2, ax + 1, ay0 + arm_len,
             skin if not droid else skin_d)
    else:
        orect(d, tx0 - 3, ay0 + swing, tx0 - 1, ay0 + arm_len + swing, sleeve)
        orect(d, tx1 + 1, ay0 - swing, tx1 + 3, ay0 + arm_len - swing, sleeve)
        rect(d, tx0 - 3, ay0 + arm_len + swing - 1, tx0 - 1, ay0 + arm_len + swing, skin if not droid else skin_d)
        rect(d, tx1 + 1, ay0 + arm_len - swing - 1, tx1 + 3, ay0 + arm_len - swing, skin if not droid else skin_d)
        rect(d, tx0 - 2, ay0 + swing + 1, tx0 - 2, ay0 + 4 + swing, shade(sleeve, 1.18))
        rect(d, tx1 + 2, ay0 - swing + 1, tx1 + 2, ay0 + 4 - swing, sleeve_d)
    if "datapad" in gear and facing != "up":
        hx = (cx + 1) if side else tx1 + 1
        rect(d, hx, ay0 + arm_len - 4, hx + 3, ay0 + arm_len, PAL["screen"])
        px(d, hx + 1, ay0 + arm_len - 3, accent)

    # ---- head (y 2..17): the big readable third
    hy0, hy1 = 2 + bob, 17 + bob
    orect(d, cx - hw, hy0 + 2, cx + hw - 1, hy1, skin)
    rect(d, cx - hw + 1, hy0 + 3, cx - 1, hy1 - 2, skin_l)
    rect(d, cx + hw - 2, hy0 + 4, cx + hw - 2, hy1 - 2, skin_d)

    if droid:
        # dome crown, optic band, jaw seam — a face, machined
        rect(d, cx - hw + 1, hy0, cx + hw - 2, hy0 + 3, shade(skin, 0.82))
        d.line((cx - hw + 1, hy0 + 3, cx + hw - 2, hy0 + 3), fill=shade(skin, 0.6))
        if facing != "up":
            vx0 = cx - hw + 2 if facing != "right" else cx - 1
            vx1 = cx + hw - 3 if facing != "left" else cx
            rect(d, vx0, hy0 + 7, vx1, hy0 + 9, accent)
            px(d, (vx0 + vx1) // 2, hy0 + 8, shade(accent, 1.3))
        d.line((cx - 3, hy1 - 2, cx + 2, hy1 - 2), fill=shade(skin, 0.6))
    else:
        hb = hair_bob
        if style == "wrap":
            # headscarf: covers the crown, knotted at the side
            rect(d, cx - hw, hy0 + hb, cx + hw - 1, hy0 + 7 + hb, accent)
            rect(d, cx - hw + 1, hy0 + 1 + hb, cx - 1, hy0 + 5 + hb, shade(accent, 1.18))
            rect(d, cx + hw - 2, hy0 + 6 + hb, cx + hw, hy0 + 9 + hb, shade(accent, 0.8))
        elif style == "buzz":
            rect(d, cx - hw, hy0 + 1 + hb, cx + hw - 1, hy0 + 4 + hb, shade(hair, 0.9))
        elif style == "crop":
            rect(d, cx - hw, hy0 + hb, cx + hw - 1, hy0 + 5 + hb, hair)
            rect(d, cx - hw + 1, hy0 + 1 + hb, cx - 1, hy0 + 3 + hb, shade(hair, 1.25))
        else:
            # short / tail / bun share the cap-and-fringe crown
            rect(d, cx - hw, hy0 + hb, cx + hw - 1, hy0 + 6 + hb, hair)
            rect(d, cx - hw + 1, hy0 + 1 + hb, cx - 1, hy0 + 4 + hb, shade(hair, 1.25))
            for fx in range(cx - hw + 1, cx + hw - 1, 3):  # fringe teeth
                px(d, fx, hy0 + 7 + hb, hair)
        if facing == "up" and style != "wrap":
            rect(d, cx - hw, hy0 + hb, cx + hw - 1, hy0 + 11 + hb, hair)
            rect(d, cx - hw + 1, hy0 + 1 + hb, cx - 1, hy0 + 8 + hb, shade(hair, 1.25))
        if style == "tail":
            # The ponytail hangs off the BACK of the head (walking left,
            # the back is +x) and bounces a pixel behind the step.
            if facing == "left":
                tx = cx + hw - 1
            elif facing == "right":
                tx = cx - hw - 2
            else:
                tx = cx + hw - 1  # down/up: it peeks past one shoulder
            rect(d, tx, hy0 + 6 + hb, tx + 2, hy0 + 16 + hb, hair)
            rect(d, tx, hy0 + 15 + hb, tx + 2, hy0 + 16 + hb, shade(hair, 0.75))
        if style == "bun":
            rect(d, cx - 2, hy0 - 2 + hb, cx + 2, hy0 + 1 + hb, hair)
            px(d, cx - 1, hy0 - 1 + hb, shade(hair, 1.25))
        # eyes: 2px, whites make the face read at distance
        ey = hy0 + 9
        if facing == "down":
            for ex in (cx - 4, cx + 2):
                rect(d, ex, ey, ex + 1, ey + 1, (244, 244, 246, 255))
                px(d, ex + 1, ey, PAL["outline"])
            px(d, cx - 1, ey + 3, skin_d)  # nose
            d.line((cx - 2, ey + 5, cx + 1, ey + 5), fill=shade(skin, 0.6))  # mouth
        elif facing == "left":
            rect(d, cx - 5, ey, cx - 4, ey + 1, (244, 244, 246, 255))
            px(d, cx - 5, ey, PAL["outline"])
        elif facing == "right":
            rect(d, cx + 3, ey, cx + 4, ey + 1, (244, 244, 246, 255))
            px(d, cx + 4, ey, PAL["outline"])

    return img


def gen_characters():
    rows = ["down", "up", "left", "right"]
    wardrobes = load_wardrobes()
    wardrobes["player"] = PLAYER_WARDROBE
    for cid, wardrobe in sorted(wardrobes.items()):
        sheet = Image.new("RGBA", (FW * 4, FH * 4), (0, 0, 0, 0))
        for r, facing in enumerate(rows):
            for f in range(4):
                frame = draw_character_frame(wardrobe, facing, f)
                sheet.paste(frame, (f * FW, r * FH))
        single = draw_character_frame(wardrobe, "down", 0)
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
    # zlib.crc32, not hash(): Python salts str hashes per process, which made
    # every regeneration churn the tile noise. Stable input, stable pixels.
    rng = random.Random(seed + zlib.crc32(name.encode()) % 1000)
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

    # The flight suit on its rack: mag-soled EVA gear, unmistakably wearable.
    img, d, done = prop("flight_suit", 24, 34)
    rect(d, 2, 0, 21, 1, PAL["hull_light"])            # rack bar
    rect(d, 11, 1, 12, 4, PAL["hull_dark"])            # hanger hook
    orect(d, 7, 4, 16, 11, (222, 140, 60, 255))        # helmet (high-vis orange)
    rect(d, 9, 6, 14, 9, PAL["screen"])                # visor
    px(d, 10, 7, PAL["glow_cyan"])
    orect(d, 5, 11, 18, 24, (206, 122, 48, 255))       # torso
    rect(d, 7, 13, 16, 15, shade((206, 122, 48, 255), 1.2))
    rect(d, 10, 16, 13, 22, PAL["hull_dark"])          # front seam
    orect(d, 2, 12, 5, 21, (206, 122, 48, 255))        # arms
    orect(d, 18, 12, 21, 21, (206, 122, 48, 255))
    orect(d, 6, 24, 10, 30, (188, 110, 44, 255))       # legs
    orect(d, 13, 24, 17, 30, (188, 110, 44, 255))
    rect(d, 5, 30, 11, 33, PAL["hull_dark"])           # the mag boots
    rect(d, 12, 30, 18, 33, PAL["hull_dark"])
    px(d, 8, 31, PAL["glow_amber"]); px(d, 15, 31, PAL["glow_amber"])
    done()


if __name__ == "__main__":
    gen_characters()
    gen_tiles()
    gen_props()
    print("pixel art pass complete")
