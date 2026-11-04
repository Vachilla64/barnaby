"""Draws the VS Code extension icon: Barnaby's face on a dark tile. Writes vscode-extension/icon.png.

Reuses the art grids from build-pixel-pet.py, so the icon always matches the pet.
Run from the repo root: python tools/build-extension-icon.py
"""
import importlib.util
from pathlib import Path

from PIL import Image

root = Path(__file__).resolve().parent.parent
spec = importlib.util.spec_from_file_location("pet", root / "tools" / "build-pixel-pet.py")
pet = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pet)

SIZE = 128
SCALE = 4
BG = (24, 29, 42, 255)
BRASS = (201, 162, 75, 255)

p = pet.parts()
# Head and hat only: rows 0-28, x 3-28 (the brim is the widest part).
face = (
    [px for px in p["hat"]]
    + [px for px in p["torso_rest"] if px[1] <= 28]
    + pet.BROWS + pet.MONOCLE_LENS + pet.LEFT_EYE_OPEN + pet.RIGHT_EYE_OPEN + pet.MONOCLE_RING + pet.MOUTH_IDLE
)
x0, x1, y0, y1 = 3, 29, 0, 29
w, h = (x1 - x0) * SCALE, (y1 - y0) * SCALE
ox, oy = (SIZE - w) // 2, (SIZE - h) // 2 + 2

img = Image.new("RGBA", (SIZE, SIZE), BG)
# Brass frame, two pixels thick.
for i in range(SIZE):
    for t in range(3):
        for a, b in ((i, t), (i, SIZE - 1 - t), (t, i), (SIZE - 1 - t, i)):
            img.putpixel((a, b), BRASS)

for x, y, ch in face:
    if not (x0 <= x < x1 and y0 <= y < y1):
        continue
    hexv = pet.PALETTE[ch].lstrip("#")
    col = tuple(int(hexv[i:i + 2], 16) for i in (0, 2, 4)) + (255,)
    for yy in range(SCALE):
        for xx in range(SCALE):
            img.putpixel((ox + (x - x0) * SCALE + xx, oy + (y - y0) * SCALE + yy), col)

out = root / "vscode-extension" / "icon.png"
img.save(out)
print("wrote", out)
