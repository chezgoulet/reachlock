#!/usr/bin/env python3
"""Generate stand-in sprite PNGs for reachlock NPCs — flat 2D SNES-RPG style."""
import hashlib, os, struct
from PIL import Image, ImageDraw

CHARACTERS = {
    "tib":       "#4a7ab5",
    "tove":      "#b5654a",
    "bardo":     "#7a8a5a",
    "doc_keene": "#5a8a7a",
    "prudence":  "#8a5a7a",
    "risc":      "#7a5a4a",
    "boris":     "#5a5a6a",
    "vex":       "#4a4a4a",
}

def draw_character(draw, base_color, seed_str, w=92, h=148):
    rng_data = [hashlib.md5((seed_str + str(i)).encode()).digest() for i in range(10)]
    def rand():
        nonlocal rng_data
        v = struct.unpack('I', rng_data[0][:4])[0] / 2**32
        rng_data = rng_data[1:]
        return v

    color = tuple(int(base_color[i:i+2], 16) for i in (1, 3, 5))
    shade = tuple(int(c * 0.65) for c in color)
    light = tuple(min(255, int(c + (255 - c) * 0.18)) for c in color)
    skin = tuple(min(255, int(s * 0.88 + c * 0.12)) for s, c in zip((219, 184, 153), color))
    outline = tuple(int(c * 0.28) for c in color)

    ox, oy = 0, 0

    # Shadow
    draw.ellipse([ox + w*0.16, oy + h*0.88, ox + w*0.84, oy + h*0.96], fill=(0, 0, 0, 55))

    body_top = int(oy + h * 0.42)
    body_b = int(body_top + h * 0.46)
    mid_x = ox + w // 2

    # Torso left
    draw.rectangle([int(ox + w * 0.20), body_top, mid_x, body_b], fill=color)
    # Torso right (shaded)
    draw.rectangle([mid_x, body_top, int(ox + w * 0.80), body_b], fill=shade)

    build = 0.5 + rand() * 0.5
    shoulder_w = int(w * (0.62 + build * 0.14))
    shoulder_l = ox + (w - shoulder_w) // 2

    draw.rectangle([shoulder_l, int(body_top - h * 0.02), mid_x, int(body_top + h * 0.08)], fill=color)
    draw.rectangle([mid_x, int(body_top - h * 0.02), shoulder_l + shoulder_w, int(body_top + h * 0.08)], fill=shade)
    draw.rectangle([int(ox + w * 0.20), body_top, int(ox + w * 0.80), body_b], outline=outline, width=2)

    head_cx = ox + w // 2
    head_cy = int(oy + h * 0.30)
    head_r = int(w * 0.20)

    draw.ellipse([head_cx - head_r, head_cy - head_r, head_cx + head_r, head_cy + head_r], fill=skin)
    draw.pieslice([head_cx, head_cy - head_r, head_cx + head_r, head_cy + head_r], 270, 90,
                  fill=tuple(int(c * 0.86) for c in skin))
    draw.ellipse([head_cx - head_r, head_cy - head_r, head_cx + head_r, head_cy + head_r],
                 outline=outline, width=2)

    htype = int(rand() * 3)
    if htype == 0:
        draw.rectangle([int(head_cx - head_r * 1.35), int(head_cy - head_r * 0.55),
                        int(head_cx + head_r * 1.35), int(head_cy - head_r * 0.23)], fill=light)
        draw.rectangle([int(head_cx - head_r * 0.8), int(head_cy - head_r * 1.25),
                        int(head_cx + head_r * 1.6), int(head_cy - head_r * 0.55)], fill=color)
    elif htype == 1:
        draw.polygon([int(head_cx - head_r * 1.1), int(head_cy + head_r * 0.9),
                      int(head_cx + head_r * 1.1), int(head_cy + head_r * 0.9),
                      head_cx, int(head_cy - head_r * 0.4)], fill=shade)
        draw.ellipse([head_cx - head_r, head_cy - head_r, head_cx + head_r, head_cy + head_r], fill=skin)
    else:
        draw.arc([int(head_cx - head_r * 1.02), int(head_cy - head_r * 1.02),
                  int(head_cx + head_r * 1.02), int(head_cy + head_r * 1.02)], 180, 360,
                 fill=light, width=int(head_r * 0.5))

def generate_all():
    out_dir = os.path.expanduser("~/repos/reachlock/godot/mods/reachlock/assets/npcs")
    os.makedirs(out_dir, exist_ok=True)
    for name, hex_color in CHARACTERS.items():
        img = Image.new("RGBA", (92, 148), (0, 0, 0, 0))
        draw = ImageDraw.Draw(img)
        draw_character(draw, hex_color, name)
        path = os.path.join(out_dir, f"{name}.png")
        img.save(path)
        print(f"  {name}.png ({hex_color})")
    print(f"Done — {len(CHARACTERS)} sprites in {out_dir}")

generate_all()
