#!/usr/bin/env python3
"""Generate lightweight branded UniCore OS framebuffer boot splash (P6 PPM + fbsplash.ini).

Uses Python standard library only so it runs inside Alpine 3.20 containers with zero extra deps.
"""

import math
import os
import sys

W, H = 640, 480

# Minimal 5x7 uppercase bitmap font for "UNICORE OS"
FONT = {
    "U": ["10001", "10001", "10001", "10001", "10001", "10001", "01110"],
    "N": ["10001", "11001", "10101", "10011", "10001", "10001", "10001"],
    "I": ["01110", "00100", "00100", "00100", "00100", "00100", "01110"],
    "C": ["01110", "10001", "10000", "10000", "10000", "10001", "01110"],
    "O": ["01110", "10001", "10001", "10001", "10001", "10001", "01110"],
    "R": ["11110", "10001", "10001", "11110", "10100", "10010", "10001"],
    "E": ["11111", "10000", "10000", "11110", "10000", "10000", "11111"],
    "S": ["01111", "10000", "10000", "01110", "00001", "00001", "11110"],
    " ": ["00000", "00000", "00000", "00000", "00000", "00000", "00000"],
}


def draw_text(buf, text, cx, y0, scale, rgb):
    glyph_w = 5 * scale
    gap = 2 * scale
    total_w = len(text) * glyph_w + max(0, len(text) - 1) * gap
    x0 = cx - total_w // 2
    r, g, b = rgb
    for idx, ch in enumerate(text):
        rows = FONT.get(ch, FONT[" "])
        gx = x0 + idx * (glyph_w + gap)
        for ry, row in enumerate(rows):
            for rx, bit in enumerate(row):
                if bit != "1":
                    continue
                for dy in range(scale):
                    py = y0 + ry * scale + dy
                    if not (0 <= py < H):
                        continue
                    base = py * W * 3
                    for dx in range(scale):
                        px = gx + rx * scale + dx
                        if 0 <= px < W:
                            off = base + px * 3
                            buf[off] = r
                            buf[off + 1] = g
                            buf[off + 2] = b


def generate(out_ppm, out_ini):
    buf = bytearray(W * H * 3)
    cx, cy = W // 2, 190
    for y in range(H):
        ty = y / float(H - 1)
        row_off = y * W * 3
        for x in range(W):
            dx = x - cx
            dy = y - cy
            dist = math.hypot(dx, dy)
            # Midnight slate radial/vertical backdrop
            r = int(10 + 8 * (1.0 - ty))
            g = int(18 + 16 * (1.0 - ty))
            b = int(34 + 30 * (1.0 - ty))
            # Soft ambient cyan glow behind emblem
            if dist < 130.0:
                glow = (1.0 - dist / 130.0) ** 2
                r = min(255, int(r + 18 * glow))
                g = min(255, int(g + 78 * glow))
                b = min(255, int(b + 125 * glow))
            # Crisp UniCore double-core emblem ring
            if 44.0 <= dist <= 56.0:
                r, g, b = 56, 189, 248
            elif 22.0 <= dist <= 31.0:
                r, g, b = 129, 140, 248
            elif dist <= 12.0:
                r, g, b = 224, 242, 254
            off = row_off + x * 3
            buf[off] = r
            buf[off + 1] = g
            buf[off + 2] = b

    draw_text(buf, "UNICORE OS", W // 2, 278, 4, (241, 245, 249))

    # Subtle progress bar frame (matches splash.ini coordinates below)
    bx0, by0, bw, bh = 180, 356, 280, 12
    for y in range(by0 - 2, by0 + bh + 2):
        for x in range(bx0 - 2, bx0 + bw + 2):
            off = (y * W + x) * 3
            if y < by0 or y >= by0 + bh or x < bx0 or x >= bx0 + bw:
                buf[off : off + 3] = bytes((51, 65, 85))
            else:
                buf[off : off + 3] = bytes((15, 23, 42))

    os.makedirs(os.path.dirname(os.path.abspath(out_ppm)), exist_ok=True)
    with open(out_ppm, "wb") as f:
        f.write(f"P6\n{W} {H}\n255\n".encode("ascii"))
        f.write(buf)

    with open(out_ini, "w", encoding="utf-8", newline="\n") as f:
        f.write(
            f"BAR_WIDTH={bw}\n"
            f"BAR_HEIGHT={bh}\n"
            f"BAR_LEFT={bx0}\n"
            f"BAR_TOP={by0}\n"
            "BAR_R=56\n"
            "BAR_G=189\n"
            "BAR_B=248\n"
        )


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.stderr.write("Usage: generate-splash.py <out.ppm> <out.ini>\n")
        sys.exit(2)
    generate(sys.argv[1], sys.argv[2])
