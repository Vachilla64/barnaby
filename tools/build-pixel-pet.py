"""Builds Barnaby's pixel-art pet.

Writes src/pet-art.ts (SVG markup made of <rect> pixels) and, with --preview PATH, a PNG contact
sheet so the art can be checked without running the app.

Each layer is a list of (left_offset, row_text). '.' is transparent. The grid is 32 wide.
"""
import sys

W, H = 32, 44

PALETTE = {
    "K": "#16161d", "k": "#2e2e3b", "g": "#474759",          # hat and suit blacks, hat band
    "S": "#f0c8a0", "s": "#d9a47c", "R": "#e0877a",          # skin, skin shade, nose
    "W": "#dcdce4", "w": "#a4a4b2",                          # hair
    "M": "#c4c4d0", "m": "#8c8c9c",                          # moustache
    "B": "#e4e4ec", "b": "#aaaab8",                          # beard
    "F": "#f7f7fa", "T": "#0b0b10", "V": "#22222c", "p": "#50506a",  # shirt, bow tie, vest, stripe
    "C": "#37e6f5", "c": "#12a5b5", "l": "#a8fbff",          # monocle ring, dark, lens
    "E": "#14141a", "Y": "#ffffff", "X": "#d6453a", "Z": "#6e1c1c",  # eye, shine, tongue, mouth
}

# --- static body: hat, head, beard, suit, legs -------------------------------------------------
BODY = [
    (10, "KKKKKKKKKKKK"),
    (9, "KkkKKKKKKKKKKK"),
    (9, "KkkKKKKKKKKKKK"),
    (9, "KkkKKKKKKKKKKK"),
    (9, "KkkKKKKKKKKKKK"),
    (9, "KkkKKKKKKKKKKK"),
    (9, "KkkKKKKKKKKKKK"),
    (9, "KKKKKKKKKKKKKK"),
    (9, "KKKKKKKKKKKKKK"),
    (9, "gggggggggggggg"),
    (9, "KKKKKKKKKKKKKK"),
    (4, "KKKKKKKKKKKKKKKKKKKKKKKK"),
    (3, "KkKKKKKKKKKKKKKKKKKKKKKKKK"),
    (6, "KKKKKKKKKKKKKKKKKKKK"),
    (6, "wWSSSSSSSSSSSSSSSSWw"),
    (6, "WWSSSSSSSSSSSSSSSSWW"),
    (6, "WSSSSSSSSSSSSSSSSSSW"),
    (6, "WSSSSSSSSSSSSSSSSSSW"),
    (6, "WSSSSSSSSSSSSSSSSSSW"),
    (6, "WSSSSSSSSSRRSSSSSSSW"),
    (6, "WSSSSSSSSRRRRSSSSSSW"),
    (6, "WSmMMMMMMMMMMMMMMmSW"),
    (6, "WSSmMMMMMmmMMMMMmSSW"),
    (6, "WWBBBBBBBBBBBBBBBBWW"),
    (6, "wWBBBBBBBBBBBBBBBBWw"),
    (7, "bBBBBBBBBBBBBBBBBb"),
    (8, "bBBBBBBBBBBBBBBb"),
    (9, "bBBBBBBBBBBBBb"),
    (11, "bBBBBBBBBb"),
    (9, "KKFTTTTTTTTFKK"),
    (8, "KKKFFTTTTTTFFKKK"),
    (8, "KKKFFFFFFFFFFKKK"),
    (7, "KKKKVFFFFFFFFVKKKK"),
    (7, "KKKKVVFFFFFFVVKKKK"),
    (7, "KKKKVVpVFFVpVVKKKK"),
    (7, "KKKKVpVVppVVpVKKKK"),
    (7, "KKKKVpVVppVVpVKKKK"),
    (7, "KKKKVpVVppVVpVKKKK"),
    (7, "KKKKKVVVVVVVVKKKKK"),
    (8, "KKKKkKKKKKKkKKKK"),
    (10, "KKKK....KKKK"),
    (10, "KKKK....KKKK"),
    (8, "kKKKKKK..KKKKKKk"),
    (8, "KKKKKKK..KKKKKKK"),
]
# The block above starts at row 1; row 0 is the hat's top edge row.
BODY_START = 0

# The list above has 43 rows? Verified by the assertion below.

POCKET_SQUARE = [(9, 34, "F"), (10, 34, "F"), (10, 33, "F")]  # (x, y, colour) over the jacket

BROWS = [(9, 15, "M"), (10, 15, "M"), (11, 15, "M"), (12, 15, "M"),
         (18, 14, "M"), (19, 14, "M"), (20, 14, "M"), (21, 14, "M"), (22, 14, "M")]

# Eyes. Left eye is plain; the right eye sits behind the monocle.
LEFT_EYE_OPEN = [(10, 16, "Y"), (11, 16, "E"), (10, 17, "E"), (11, 17, "E"), (10, 18, "E"), (11, 18, "E")]
LEFT_EYE_CLOSED = [(9, 17, "E"), (10, 18, "E"), (11, 18, "E"), (12, 17, "E")]
RIGHT_EYE_OPEN = [(20, 16, "Y"), (21, 16, "E"), (20, 17, "E"), (21, 17, "E"), (20, 18, "E"), (21, 18, "E")]
RIGHT_EYE_CLOSED = [(19, 17, "E"), (20, 18, "E"), (21, 18, "E"), (22, 17, "E")]

# Monocle: ring, lens tint, chain to the ear.
MONOCLE_RING = (
    [(x, 15, "C") for x in range(19, 23)]
    + [(18, y, "C") for y in range(16, 20)] + [(23, y, "C") for y in range(16, 20)]
    + [(x, 20, "C") for x in range(19, 23)]
    + [(23, 20, "c"), (24, 21, "c"), (24, 22, "c"), (25, 23, "c"), (25, 24, "c"), (25, 25, "c")]
)
MONOCLE_LENS = [(x, y, "l") for x in range(19, 23) for y in range(16, 20)]

# Mouth sits in the beard below the moustache (rows 23-24).
MOUTH_IDLE = [(13, 23, "E"), (14, 24, "E"), (15, 24, "E"), (16, 24, "E"), (17, 24, "E"), (18, 23, "E")]
MOUTH_OPEN = ([(x, 23, "Z") for x in range(14, 18)] + [(x, 24, "Z") for x in range(14, 18)]
              + [(15, 24, "X"), (16, 24, "X"), (13, 23, "E"), (18, 23, "E"), (14, 25, "E"), (15, 25, "E"),
                 (16, 25, "E"), (17, 25, "E")])


def pixels(layer_rows, start_row=0):
    out = []
    for dy, (off, text) in enumerate(layer_rows):
        for dx, ch in enumerate(text):
            if ch != ".":
                out.append((off + dx, start_row + dy, ch))
    return out


def rects(px):
    """Merge horizontal runs of one colour into single <rect> elements."""
    by_row = {}
    for x, y, ch in px:
        by_row.setdefault(y, {})[x] = ch
    parts = []
    for y in sorted(by_row):
        xs = sorted(by_row[y])
        i = 0
        while i < len(xs):
            j = i
            while j + 1 < len(xs) and xs[j + 1] == xs[j] + 1 and by_row[y][xs[j + 1]] == by_row[y][xs[i]]:
                j += 1
            parts.append(f'<rect x="{xs[i]}" y="{y}" width="{j - i + 1}" height="1" fill="{PALETTE[by_row[y][xs[i]]]}"/>')
            i = j + 1
    return "".join(parts)


def build_svg():
    body = pixels(BODY, BODY_START)
    body_set = {(x, y) for x, y, _ in body}
    hat_px = [p for p in body if p[1] <= 13]
    rest_px = [p for p in body if p[1] > 13]
    assert all(0 <= x < W and 0 <= y < H for x, y, _ in body), "pixel outside the grid"
    assert len(BODY) == H, f"body has {len(BODY)} rows, expected {H}"
    rest_px = [p for p in rest_px if (p[0], p[1]) not in {(x, y) for x, y, _ in POCKET_SQUARE}] + POCKET_SQUARE
    svg = (
        f'<svg viewBox="0 0 {W} {H}" width="{W * 3}" height="{H * 3}" class="pet-svg" '
        'xmlns="http://www.w3.org/2000/svg" shape-rendering="crispEdges">'
        f'<g class="pet-body"><g class="pet-hat">{rects(hat_px)}</g>{rects(rest_px)}</g>'
        f'<g class="pet-brows">{rects(pixels([], 0) + BROWS)}</g>'
        f'<g class="pet-eye pet-eye-left"><g class="eye-open"><g class="eye-pupil">{rects(LEFT_EYE_OPEN)}</g></g>'
        f'<g class="eye-closed">{rects(LEFT_EYE_CLOSED)}</g></g>'
        f'<g class="monocle"><g class="monocle-lens">{rects(MONOCLE_LENS)}</g>'
        f'<g class="pet-eye pet-eye-right"><g class="eye-open"><g class="eye-pupil">{rects(RIGHT_EYE_OPEN)}</g></g>'
        f'<g class="eye-closed">{rects(RIGHT_EYE_CLOSED)}</g></g>'
        f'<g class="monocle-ring">{rects(MONOCLE_RING)}</g></g>'
        f'<g class="pet-mouth-group"><g class="mouth-idle">{rects(MOUTH_IDLE)}</g>'
        f'<g class="mouth-open">{rects(MOUTH_OPEN)}</g></g>'
        "</svg>"
    )
    return svg


def preview(path):
    from PIL import Image

    scale = 12
    sheet = Image.new("RGBA", (W * scale * 3 + 40, H * scale + 20), (222, 222, 230, 255))

    def draw(dx, layers):
        for x, y, ch in layers:
            hexv = PALETTE[ch].lstrip("#")
            col = tuple(int(hexv[i:i + 2], 16) for i in (0, 2, 4)) + (255,)
            for yy in range(scale):
                for xx in range(scale):
                    sheet.putpixel((dx + x * scale + xx, 10 + y * scale + yy), col)

    base = pixels(BODY) + POCKET_SQUARE
    face = BROWS + MONOCLE_LENS + MONOCLE_RING
    draw(10, base + face + LEFT_EYE_OPEN + RIGHT_EYE_OPEN + MOUTH_IDLE)
    draw(W * scale + 20, base + face + LEFT_EYE_OPEN + RIGHT_EYE_OPEN + MOUTH_OPEN)
    draw(W * scale * 2 + 30, base + face + LEFT_EYE_CLOSED + RIGHT_EYE_CLOSED + MOUTH_IDLE)
    sheet.save(path)


if __name__ == "__main__":
    svg = build_svg()
    with open("src/pet-art.ts", "w", encoding="utf-8", newline="\n") as f:
        f.write("// Generated by tools/build-pixel-pet.py. Do not edit by hand.\n")
        f.write(f"export const PET_ART = `{svg}`;\n")
    print(f"wrote src/pet-art.ts ({len(svg)} bytes)")
    if "--preview" in sys.argv:
        preview(sys.argv[sys.argv.index("--preview") + 1])
        print("wrote preview")
