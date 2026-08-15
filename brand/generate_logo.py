import math
from PIL import Image, ImageDraw

SS, S = 4, 1024
N = SS * S
INDIGO, VIOLET = (91, 91, 214), (139, 92, 246)

def lerp(a, b, t): return tuple(round(x + (y - x) * t) for x, y in zip(a, b))

def gradient(size, a, b):
    img = Image.new("RGB", (size, size))
    d = ImageDraw.Draw(img)
    for i in range(size * 2):
        d.line([(i, 0), (0, i)], fill=lerp(a, b, i / (size * 2 - 1)))
    return img

def rounded_mask(size, radius):
    m = Image.new("L", (size, size), 0)
    ImageDraw.Draw(m).rounded_rectangle([0, 0, size - 1, size - 1], radius, fill=255)
    return m

def pt(c, r, deg):
    a = math.radians(deg)
    return (c + r * math.cos(a), c + r * math.sin(a))

def build(outer=0.335, hole=0.190, seam=0.044, rot=-90):
    """White iris ring with a hexagonal opening and tangential blade seams."""
    base = gradient(N, INDIGO, VIOLET)
    base.putalpha(rounded_mask(N, int(N * 0.225)))

    c = N / 2
    R, r = N * outer, N * hole

    # 1. white disc
    ring = Image.new("L", (N, N), 0)
    d = ImageDraw.Draw(ring)
    d.ellipse([c - R, c - R, c + R, c + R], fill=255)

    hexagon = [pt(c, r, rot + i * 60) for i in range(6)]

    # 2. blade seams, drawn before the opening so their ends cannot notch it
    for i in range(6):
        v = hexagon[i]
        prev = hexagon[(i - 1) % 6]
        dx, dy = v[0] - prev[0], v[1] - prev[1]
        length = math.hypot(dx, dy)
        dx, dy = dx / length, dy / length
        d.line([v, (v[0] + dx * R * 1.6, v[1] + dy * R * 1.6)],
               fill=0, width=int(N * seam))

    # 3. punch the opening last, so it stays a crisp hexagon
    d.polygon(hexagon, fill=0)

    white = Image.new("RGBA", (N, N), (255, 255, 255, 255))
    base.paste(white, (0, 0), ring)
    return base.resize((S, S), Image.LANCZOS)

if __name__ == "__main__":
    build().save("logo_v4.png")
    # contact sheet at real icon sizes, on a neutral strip
    sheet = Image.new("RGBA", (560, 120), (235, 237, 241, 255))
    x = 20
    for size in (16, 24, 32, 48, 64, 96):
        ic = build().resize((size, size), Image.LANCZOS)
        sheet.alpha_composite(ic, (x, 60 - size // 2))
        x += size + 24
    sheet.resize((1120, 240), Image.NEAREST).save("logo_v4_sizes.png")
    print("saved")
