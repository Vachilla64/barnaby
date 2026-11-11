"""Generates icon variations for Barnaby:
1. Pure transparent face (128x128 and 256x256)
2. Transparent face with soft contrast rim for dark themes
3. Barnaby with glowing cyan code brackets < / >
4. Barnaby with glowing monocle code inspection scan
5. Barnaby in front of a sleek mini code editor
"""
import importlib.util
from pathlib import Path
from PIL import Image, ImageDraw, ImageFilter

root = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("pet", root / "tools" / "build-pixel-pet.py")
pet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pet)

SIZE = 128
SCALE = 4

p = pet.parts()
face = (
    [px for px in p["hat"]]
    + [px for px in p["torso_rest"] if px[1] <= 28]
    + pet.BROWS + pet.MONOCLE_LENS + pet.LEFT_EYE_OPEN + pet.RIGHT_EYE_OPEN + pet.MONOCLE_RING + pet.MOUTH_IDLE
)
x0, x1, y0, y1 = 3, 29, 0, 29
w, h = (x1 - x0) * SCALE, (y1 - y0) * SCALE
ox, oy = (SIZE - w) // 2, (SIZE - h) // 2 + 2

def hex_to_rgba(hexv, alpha=255):
    hexv = hexv.lstrip("#")
    return tuple(int(hexv[i:i + 2], 16) for i in (0, 2, 4)) + (alpha,)

def draw_face(img, offset_x=ox, offset_y=oy, scale=SCALE):
    for x, y, ch in face:
        if not (x0 <= x < x1 and y0 <= y < y1):
            continue
        col = hex_to_rgba(pet.PALETTE[ch])
        px_x = offset_x + (x - x0) * scale
        px_y = offset_y + (y - y0) * scale
        for yy in range(scale):
            for xx in range(scale):
                img.putpixel((px_x + xx, px_y + yy), col)

# --- 1. Pure transparent face ---
img1 = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
draw_face(img1)

# --- 2. Transparent with contrast rim (outer edge highlight on hat) ---
img2 = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
# Create mask of the hat to add subtle edge
hat_pixels = set()
for x, y, ch in face:
    if y <= 13: # hat area
        px_x = ox + (x - x0) * SCALE
        px_y = oy + (y - y0) * SCALE
        for yy in range(SCALE):
            for xx in range(SCALE):
                hat_pixels.add((px_x + xx, px_y + yy))

# Rim around hat perimeter
rim_col = (75, 75, 95, 200) # subtle pewter rim
for px, py in hat_pixels:
    for dx, dy in ((-1,0), (1,0), (0,-1), (0,1), (-1,-1), (1,-1), (-1,1), (1,1)):
        nx, ny = px + dx, py + dy
        if (nx, ny) not in hat_pixels and 0 <= nx < SIZE and 0 <= ny < SIZE:
            img2.putpixel((nx, ny), rim_col)
draw_face(img2)

# --- 3. Barnaby with glowing cyan code brackets < / > ---
img3 = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
# Shift Barnaby slightly down/centered to make room
draw_face(img3)
# Draw pixel code brackets '<' on left and '/>' on right
CYAN_GLOW = (55, 230, 245, 255)
CYAN_BRIGHT = (168, 251, 255, 255)
CYAN_DIM = (18, 165, 181, 180)

# '<' bracket on left (approx x: 2..8, y: 55..75)
bracket_left = [
    (7, 54), (6, 55), (5, 56), (4, 57), (3, 58), (2, 59), (2, 60),
    (3, 61), (4, 62), (5, 63), (6, 64), (7, 65)
]
for bx, by in bracket_left:
    for dx in range(2):
        for dy in range(2):
            img3.putpixel((bx*SCALE//3 + dx, by + dy), CYAN_GLOW)
            # soft glow neighbor
            if 0 <= bx*SCALE//3 + dx - 1 < SIZE:
                c = img3.getpixel((bx*SCALE//3 + dx - 1, by + dy))
                if c[3] == 0:
                    img3.putpixel((bx*SCALE//3 + dx - 1, by + dy), CYAN_DIM)

# '/>' bracket on right (approx x: 110..122, y: 54..66)
bracket_right = [
    (115, 65), (116, 64), (117, 63), (118, 62), (119, 61), (120, 60), (120, 59),
    (119, 58), (118, 57), (117, 56), (116, 55), (115, 54)
]
slash = [
    (110, 65), (111, 63), (112, 61), (113, 59), (114, 57), (115, 55)
]
for bx, by in bracket_right:
    for dx in range(2):
        for dy in range(2):
            if 0 <= bx + dx < SIZE and 0 <= by + dy < SIZE:
                img3.putpixel((bx + dx, by + dy), CYAN_GLOW)
for sx, sy in slash:
    for dx in range(2):
        for dy in range(2):
            if 0 <= sx + dx < SIZE and 0 <= sy + dy < SIZE:
                img3.putpixel((sx + dx, sy + dy), CYAN_BRIGHT)

# --- 4. Monocle Code Inspection Ray ---
img4 = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
# Barnaby's monocle is at approximately x: 19..23, y: 17..21 in pet grid
# In icon coordinates:
# mx0 = ox + (19 - x0) * SCALE
# my0 = oy + (17 - y0) * SCALE
mx = ox + (21 - x0) * SCALE
my = oy + (19 - y0) * SCALE

# Draw inspection scan beam from monocle down-right
scan_layer = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
d = ImageDraw.Draw(scan_layer)
# Soft cone beam
d.polygon([(mx, my), (SIZE - 4, my + 10), (SIZE - 4, SIZE - 8), (mx + 8, SIZE - 8)], fill=(18, 165, 181, 45))
d.polygon([(mx, my), (SIZE - 8, my + 18), (SIZE - 8, SIZE - 12), (mx + 12, SIZE - 12)], fill=(55, 230, 245, 60))

# Draw code glyphs inside the beam (pixel code lines)
# fn() tag
for y_line, color in [(my + 22, (255, 123, 114, 230)), (my + 28, (121, 192, 255, 240)), (my + 34, (126, 231, 135, 230))]:
    for x_p in range(mx + 14, SIZE - 12, 3):
        scan_layer.putpixel((x_p, y_line), color)
        scan_layer.putpixel((x_p + 1, y_line), color)

# Composite beam behind Barnaby's face
img4 = Image.alpha_composite(img4, scan_layer)
draw_face(img4)
# Monocle extra glow dot
for dy in range(-2, 3):
    for dx in range(-2, 3):
        if dx*dx + dy*dy <= 4:
            img4.putpixel((mx + dx, my + dy), (168, 251, 255, 255))

# --- 5. Code Editor Backdrop with Sir Barnaby ---
img5 = Image.new("RGBA", (SIZE, SIZE), (0, 0, 0, 0))
# Mini dark editor card behind him with rounded corners
d5 = ImageDraw.Draw(img5)
d5.rounded_rectangle([6, 6, SIZE - 6, SIZE - 6], radius=12, fill=(24, 29, 42, 250), outline=(55, 230, 245, 140), width=1)
# 3 editor title bar dots (mac/linux style: red, yellow, green)
d5.ellipse([14, 12, 18, 16], fill=(239, 68, 68, 220))
d5.ellipse([22, 12, 26, 16], fill=(234, 179, 8, 220))
d5.ellipse([30, 12, 34, 16], fill=(34, 197, 94, 220))

# Syntax code lines in background
code_lines = [
    (24, [(14, 26, (255, 123, 114, 160)), (30, 52, (121, 192, 255, 160)), (56, 70, (228, 228, 236, 140))]),
    (32, [(20, 38, (126, 231, 135, 160)), (42, 64, (255, 166, 87, 160))]),
    (40, [(20, 48, (121, 192, 255, 160)), (52, 78, (126, 231, 135, 160))]),
    (48, [(14, 30, (255, 123, 114, 160))]),
]
for y_pos, segments in code_lines:
    for x_s, x_e, col in segments:
        d5.line([x_s, y_pos, x_e, y_pos], fill=col, width=2)

draw_face(img5)

# Save all variations to brain directory and to vscode-extension
brain_var = Path(r"C:\Users\Vachi\.gemini\antigravity\brain\731efb20-db6d-4b7c-b41f-614255b2d06c\variations")
img1.save(brain_var / "var1_transparent_pure.png")
img2.save(brain_var / "var2_transparent_rim.png")
img3.save(brain_var / "var3_code_brackets.png")
img4.save(brain_var / "var4_monocle_code_scan.png")
img5.save(brain_var / "var5_code_editor_card.png")

# Also save 2x enlarged versions for easy visual inspection
for idx, im in enumerate([img1, img2, img3, img4, img5], 1):
    im_2x = im.resize((256, 256), Image.Resampling.NEAREST)
    im_2x.save(brain_var / f"preview_var{idx}.png")

# Overwrite vscode-extension/icon.png with Variant 1 (pure transparent face)
out_ext = root / "vscode-extension" / "icon.png"
img1.save(out_ext)
print("Saved all variations successfully!")
