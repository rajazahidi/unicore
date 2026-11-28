#!/usr/bin/env python3
"""Generate UniCore OS v1.3.0 10-Theme Desktop Environment assets.

Pure Python 3 standard library only (runs cleanly inside Alpine 3.22 build containers).
Generates into <rootfs>:
  - 7 crisp 1280x800 HD PNG wallpapers (paired with existing midnight/daylight/aurora PNGs = 10 total)
  - 10 Openbox / labwc window decoration themes (/usr/share/themes/UniCore-<Name>/openbox-3/themerc)
  - 10 Waybar CSS stylesheets (/usr/share/unicore/themes/waybar-<id>.css)
  - 10 Fuzzel launcher configs (/usr/share/unicore/themes/fuzzel-<id>.ini)
  - 10 Foot terminal color schemes (/usr/share/unicore/themes/foot-<id>.ini)
  - 10 Mako notification themes (/usr/share/unicore/themes/mako-<id>.conf)
"""

import math
import os
import struct
import sys
import zlib

W, H = 1280, 800

THEMES = [
    {
        "id": "midnight",
        "ob_name": "UniCore-Midnight",
        "mode": "dark",
        "radius": 14,
        "btn_radius": 8,
        "win_radius": 10,
        "bar_bg": "rgba(15, 23, 42, 0.94)",
        "bar_border": "rgba(56, 189, 248, 0.35)",
        "fg": "#f8fafc",
        "fg_muted": "#cbd5e1",
        "surface": "#1e293b",
        "surface_hover": "#334155",
        "border": "#334155",
        "accent": "#0284c7",
        "accent_dark": "#0369a1",
        "accent_bright": "#38bdf8",
        "active_bg": "#1e3a5f",
        "ai_bg": "#4f46e5",
        "ai_hover": "#4338ca",
        "power_bg": "#991b1b",
        "power_hover": "#b91c1c",
        "title_active": "#0f172a",
        "title_inactive": "#1e293b",
        "close_fg": "#f87171",
        "term_bg": "0f172a",
        "term_fg": "f8fafc",
        "wallpaper": None,  # uses existing /usr/share/unicore/wallpaper.png
    },
    {
        "id": "daylight",
        "ob_name": "UniCore-Daylight",
        "mode": "light",
        "radius": 14,
        "btn_radius": 8,
        "win_radius": 10,
        "bar_bg": "#f1f5f9",
        "bar_border": "rgba(148, 163, 184, 0.55)",
        "fg": "#0f172a",
        "fg_muted": "#334155",
        "surface": "#ffffff",
        "surface_hover": "#e2e8f0",
        "border": "#cbd5e1",
        "accent": "#0284c7",
        "accent_dark": "#0369a1",
        "accent_bright": "#0284c7",
        "active_bg": "#dbeafe",
        "ai_bg": "#4f46e5",
        "ai_hover": "#4338ca",
        "power_bg": "#dc2626",
        "power_hover": "#b91c1c",
        "title_active": "#f8fafc",
        "title_inactive": "#e2e8f0",
        "close_fg": "#dc2626",
        "term_bg": "f8fafc",
        "term_fg": "0f172a",
        "wallpaper": None,  # uses existing /usr/share/unicore/wallpaper-light.png
    },
    {
        "id": "aurora",
        "ob_name": "UniCore-Aurora",
        "mode": "dark",
        "radius": 14,
        "btn_radius": 8,
        "win_radius": 10,
        "bar_bg": "rgba(19, 17, 38, 0.94)",
        "bar_border": "rgba(165, 180, 252, 0.40)",
        "fg": "#f5f3ff",
        "fg_muted": "#c4b5fd",
        "surface": "#231f42",
        "surface_hover": "#312c5c",
        "border": "#3b3570",
        "accent": "#6366f1",
        "accent_dark": "#4f46e5",
        "accent_bright": "#a5b4fc",
        "active_bg": "#312e81",
        "ai_bg": "#7c3aed",
        "ai_hover": "#6d28d9",
        "power_bg": "#9f1239",
        "power_hover": "#be123c",
        "title_active": "#131126",
        "title_inactive": "#231f42",
        "close_fg": "#fb7185",
        "term_bg": "131126",
        "term_fg": "f5f3ff",
        "wallpaper": None,  # uses existing /usr/share/unicore/wallpaper-aurora.png
    },
    {
        "id": "nordic",
        "ob_name": "UniCore-Nordic",
        "mode": "dark",
        "radius": 14,
        "btn_radius": 9,
        "win_radius": 12,
        "bar_bg": "rgba(46, 52, 64, 0.94)",
        "bar_border": "rgba(136, 192, 208, 0.45)",
        "fg": "#eceff4",
        "fg_muted": "#d8dee9",
        "surface": "#3b4252",
        "surface_hover": "#434c5e",
        "border": "#4c566a",
        "accent": "#5e81ac",
        "accent_dark": "#4c6a8f",
        "accent_bright": "#88c0d0",
        "active_bg": "#434c5e",
        "ai_bg": "#5e81ac",
        "ai_hover": "#81a1c1",
        "power_bg": "#bf616a",
        "power_hover": "#a54e56",
        "title_active": "#2e3440",
        "title_inactive": "#3b4252",
        "close_fg": "#bf616a",
        "term_bg": "2e3440",
        "term_fg": "eceff4",
        "wallpaper": "wallpaper-nordic.png",
    },
    {
        "id": "dracula",
        "ob_name": "UniCore-Dracula",
        "mode": "dark",
        "radius": 14,
        "btn_radius": 8,
        "win_radius": 10,
        "bar_bg": "rgba(40, 42, 54, 0.95)",
        "bar_border": "rgba(189, 147, 249, 0.45)",
        "fg": "#f8f8f2",
        "fg_muted": "#bd93f9",
        "surface": "#343746",
        "surface_hover": "#44475a",
        "border": "#44475a",
        "accent": "#9580ff",
        "accent_dark": "#7c65e0",
        "accent_bright": "#ff79c6",
        "active_bg": "#44475a",
        "ai_bg": "#bd93f9",
        "ai_hover": "#a87ef0",
        "power_bg": "#ff5555",
        "power_hover": "#e04444",
        "title_active": "#282a36",
        "title_inactive": "#343746",
        "close_fg": "#ff5555",
        "term_bg": "282a36",
        "term_fg": "f8f8f2",
        "wallpaper": "wallpaper-dracula.png",
    },
    {
        "id": "emerald",
        "ob_name": "UniCore-Emerald",
        "mode": "dark",
        "radius": 14,
        "btn_radius": 8,
        "win_radius": 10,
        "bar_bg": "rgba(11, 31, 26, 0.94)",
        "bar_border": "rgba(52, 211, 153, 0.42)",
        "fg": "#ecfdf5",
        "fg_muted": "#a7f3d0",
        "surface": "#132e26",
        "surface_hover": "#1b4338",
        "border": "#1f4f42",
        "accent": "#059669",
        "accent_dark": "#047857",
        "accent_bright": "#34d399",
        "active_bg": "#064e3b",
        "ai_bg": "#0d9488",
        "ai_hover": "#0f766e",
        "power_bg": "#991b1b",
        "power_hover": "#b91c1c",
        "title_active": "#0b1f1a",
        "title_inactive": "#132e26",
        "close_fg": "#f87171",
        "term_bg": "0b1f1a",
        "term_fg": "ecfdf5",
        "wallpaper": "wallpaper-emerald.png",
    },
    {
        "id": "sunset",
        "ob_name": "UniCore-Sunset",
        "mode": "dark",
        "radius": 10,
        "btn_radius": 7,
        "win_radius": 8,
        "bar_bg": "rgba(28, 20, 16, 0.95)",
        "bar_border": "rgba(251, 191, 36, 0.42)",
        "fg": "#fffbeb",
        "fg_muted": "#fde68a",
        "surface": "#2c1f18",
        "surface_hover": "#3f2c22",
        "border": "#523a2d",
        "accent": "#d97706",
        "accent_dark": "#b45309",
        "accent_bright": "#fbbf24",
        "active_bg": "#78350f",
        "ai_bg": "#ea580c",
        "ai_hover": "#c2410c",
        "power_bg": "#991b1b",
        "power_hover": "#b91c1c",
        "title_active": "#1c1410",
        "title_inactive": "#2c1f18",
        "close_fg": "#fb923c",
        "term_bg": "1c1410",
        "term_fg": "fffbeb",
        "wallpaper": "wallpaper-sunset.png",
    },
    {
        "id": "sakura",
        "ob_name": "UniCore-Sakura",
        "mode": "light",
        "radius": 16,
        "btn_radius": 10,
        "win_radius": 12,
        "bar_bg": "rgba(255, 245, 247, 0.96)",
        "bar_border": "rgba(244, 63, 94, 0.35)",
        "fg": "#3f1722",
        "fg_muted": "#881337",
        "surface": "#ffffff",
        "surface_hover": "#ffe4e6",
        "border": "#fecdd3",
        "accent": "#e11d48",
        "accent_dark": "#be123c",
        "accent_bright": "#f43f5e",
        "active_bg": "#ffe4e6",
        "ai_bg": "#db2777",
        "ai_hover": "#be185d",
        "power_bg": "#be123c",
        "power_hover": "#9f1239",
        "title_active": "#fff5f7",
        "title_inactive": "#ffe4e6",
        "close_fg": "#e11d48",
        "term_bg": "fff5f7",
        "term_fg": "3f1722",
        "wallpaper": "wallpaper-sakura.png",
    },
    {
        "id": "cyberpunk",
        "ob_name": "UniCore-Cyberpunk",
        "mode": "dark",
        "radius": 8,
        "btn_radius": 5,
        "win_radius": 6,
        "bar_bg": "rgba(9, 8, 20, 0.96)",
        "bar_border": "rgba(236, 72, 153, 0.65)",
        "fg": "#f0fdfa",
        "fg_muted": "#67e8f9",
        "surface": "#16122b",
        "surface_hover": "#251e47",
        "border": "#3b2c6b",
        "accent": "#ec4899",
        "accent_dark": "#db2777",
        "accent_bright": "#22d3ee",
        "active_bg": "#3b1348",
        "ai_bg": "#06b6d4",
        "ai_hover": "#0891b2",
        "power_bg": "#be123c",
        "power_hover": "#e11d48",
        "title_active": "#090814",
        "title_inactive": "#16122b",
        "close_fg": "#f43f5e",
        "term_bg": "090814",
        "term_fg": "f0fdfa",
        "wallpaper": "wallpaper-cyberpunk.png",
    },
    {
        "id": "classic98",
        "ob_name": "UniCore-Classic98",
        "mode": "light",
        "radius": 0,
        "btn_radius": 2,
        "win_radius": 0,
        "bar_bg": "#c0c0c0",
        "bar_border": "#808080",
        "fg": "#000000",
        "fg_muted": "#202020",
        "surface": "#dfdfdf",
        "surface_hover": "#d4d0c8",
        "border": "#808080",
        "accent": "#000080",
        "accent_dark": "#00005c",
        "accent_bright": "#1084d0",
        "active_bg": "#ffffff",
        "ai_bg": "#008080",
        "ai_hover": "#006666",
        "power_bg": "#800000",
        "power_hover": "#a00000",
        "title_active": "#000080",
        "title_inactive": "#808080",
        "close_fg": "#ffffff",
        "term_bg": "000000",
        "term_fg": "c0c0c0",
        "wallpaper": "wallpaper-classic98.png",
    },
]


def _png_chunk(tag: bytes, data: bytes) -> bytes:
    crc = zlib.crc32(tag + data) & 0xFFFFFFFF
    return struct.pack("!I", len(data)) + tag + data + struct.pack("!I", crc)


def write_png_rgb(path: str, width: int, height: int, rows_iter) -> None:
    raw = bytearray()
    for row in rows_iter:
        raw.append(0)  # filter type 0 (None)
        raw.extend(row)
    ihdr = struct.pack("!IIBBBBB", width, height, 8, 2, 0, 0, 0)
    comp = zlib.compress(bytes(raw), level=6)
    os.makedirs(os.path.dirname(os.path.abspath(path)), exist_ok=True)
    with open(path, "wb") as f:
        f.write(b"\x89PNG\r\n\x1a\n")
        f.write(_png_chunk(b"IHDR", ihdr))
        f.write(_png_chunk(b"IDAT", comp))
        f.write(_png_chunk(b"IEND", b""))


def _lerp(a: float, b: float, t: float) -> int:
    v = int(a + (b - a) * t)
    return 0 if v < 0 else (255 if v > 255 else v)


def _clamp(v: float) -> int:
    iv = int(v)
    return 0 if iv < 0 else (255 if iv > 255 else iv)


def generate_wallpaper(theme_id: str, out_path: str) -> None:
    """Fast procedural 1280x800 wallpapers using row-oriented math (zero Pillow dependency)."""
    # Precompute 1D waves along X for crisp layered mountain ridges / aurora / synthwave grid
    xs = [x / float(W - 1) for x in range(W)]
    sin_x1 = [math.sin(t * math.pi * 2.2 + 0.4) for t in xs]
    sin_x2 = [math.sin(t * math.pi * 4.7 + 1.9) for t in xs]
    sin_x3 = [math.sin(t * math.pi * 7.5 + 0.8) for t in xs]

    def rows():
        for y in range(H):
            ty = y / float(H - 1)
            row = bytearray(W * 3)
            if theme_id == "nordic":
                # Polar night sky with soft frost aurora ribbon & dark fjord silhouettes
                sky_r = _lerp(36, 59, ty)
                sky_g = _lerp(42, 69, ty)
                sky_b = _lerp(56, 86, ty)
                for x in range(W):
                    tx = xs[x]
                    ridge1 = 0.66 + 0.08 * sin_x1[x] + 0.03 * sin_x2[x]
                    ridge2 = 0.78 + 0.05 * sin_x2[x] + 0.02 * sin_x3[x]
                    if ty > ridge2:
                        r, g, b = 36, 42, 54
                    elif ty > ridge1:
                        r, g, b = 46, 52, 64
                    else:
                        ribbon = 0.30 + 0.09 * sin_x1[x] + 0.04 * sin_x2[x]
                        dist = abs(ty - ribbon)
                        glow = max(0.0, 1.0 - dist * 5.2) ** 2
                        r = _clamp(sky_r + 88 * glow)
                        g = _clamp(sky_g + 135 * glow)
                        b = _clamp(sky_b + 145 * glow)
                    off = x * 3
                    row[off], row[off + 1], row[off + 2] = r, g, b

            elif theme_id == "dracula":
                # Twilight violet-indigo backdrop with glowing orchid/cyan geometric arcs
                base_r = _lerp(32, 48, ty)
                base_g = _lerp(34, 42, ty)
                base_b = _lerp(48, 72, ty)
                for x in range(W):
                    tx = xs[x]
                    d1 = math.hypot(tx - 0.25, (ty - 0.30) * 0.625)
                    d2 = math.hypot(tx - 0.78, (ty - 0.68) * 0.625)
                    g1 = max(0.0, 1.0 - d1 * 2.1) ** 2
                    g2 = max(0.0, 1.0 - d2 * 2.2) ** 2
                    r = _clamp(base_r + 145 * g1 + 185 * g2)
                    g = _clamp(base_g + 95 * g1 + 55 * g2)
                    b = _clamp(base_b + 190 * g1 + 130 * g2)
                    off = x * 3
                    row[off], row[off + 1], row[off + 2] = r, g, b

            elif theme_id == "emerald":
                # Misty rainforest canopy layers in deep pine, jade, and mint highlight
                sky_r = _lerp(9, 18, ty)
                sky_g = _lerp(28, 66, ty)
                sky_b = _lerp(24, 54, ty)
                for x in range(W):
                    r1 = 0.56 + 0.07 * sin_x1[x] + 0.03 * sin_x3[x]
                    r2 = 0.70 + 0.06 * sin_x2[x] + 0.02 * sin_x1[x]
                    r3 = 0.84 + 0.04 * sin_x3[x]
                    if ty > r3:
                        r, g, b = 8, 22, 18
                    elif ty > r2:
                        r, g, b = 12, 36, 29
                    elif ty > r1:
                        r, g, b = 16, 54, 43
                    else:
                        mist = max(0.0, 1.0 - abs(ty - 0.38) * 3.4) ** 2
                        r = _clamp(sky_r + 22 * mist)
                        g = _clamp(sky_g + 95 * mist)
                        b = _clamp(sky_b + 65 * mist)
                    off = x * 3
                    row[off], row[off + 1], row[off + 2] = r, g, b

            elif theme_id == "sunset":
                # Warm golden-amber horizon glow over silhouetted canyon hills
                if ty < 0.62:
                    kt = ty / 0.62
                    sky_r = _lerp(42, 234, kt)
                    sky_g = _lerp(22, 118, kt)
                    sky_b = _lerp(36, 48, kt)
                else:
                    sky_r, sky_g, sky_b = 40, 26, 20
                for x in range(W):
                    tx = xs[x]
                    h1 = 0.60 + 0.05 * sin_x1[x] + 0.025 * sin_x2[x]
                    h2 = 0.74 + 0.04 * sin_x2[x] + 0.02 * sin_x3[x]
                    if ty > h2:
                        r, g, b = 26, 17, 13
                    elif ty > h1:
                        r, g, b = 46, 28, 20
                    else:
                        sun_d = math.hypot((tx - 0.50) * 1.6, ty - 0.52)
                        if sun_d < 0.16:
                            r, g, b = 254, 240, 138
                        else:
                            glow = max(0.0, 1.0 - sun_d * 1.8) ** 2
                            r = _clamp(sky_r + 45 * glow)
                            g = _clamp(sky_g + 68 * glow)
                            b = _clamp(sky_b + 25 * glow)
                    off = x * 3
                    row[off], row[off + 1], row[off + 2] = r, g, b

            elif theme_id == "sakura":
                # Soft daylight cherry-blossom cream-rose gradient with gentle pastel clouds
                base_r = _lerp(255, 255, ty)
                base_g = _lerp(236, 248, ty)
                base_b = _lerp(242, 250, ty)
                for x in range(W):
                    tx = xs[x]
                    wave1 = 0.72 + 0.06 * sin_x1[x] + 0.02 * sin_x2[x]
                    wave2 = 0.84 + 0.04 * sin_x2[x]
                    if ty > wave2:
                        r, g, b = 251, 207, 220
                    elif ty > wave1:
                        r, g, b = 253, 224, 234
                    else:
                        d = math.hypot(tx - 0.72, (ty - 0.28) * 0.625)
                        blush = max(0.0, 1.0 - d * 2.0) ** 2
                        r = _clamp(base_r - 2 * blush)
                        g = _clamp(base_g - 32 * blush)
                        b = _clamp(base_b - 18 * blush)
                    off = x * 3
                    row[off], row[off + 1], row[off + 2] = r, g, b

            elif theme_id == "cyberpunk":
                # Retro-futuristic synthwave horizon, neon magenta sun & cyan perspective grid
                horizon = 0.56
                if ty < horizon:
                    kt = ty / horizon
                    sky_r = _lerp(9, 44, kt)
                    sky_g = _lerp(8, 14, kt)
                    sky_b = _lerp(24, 68, kt)
                    for x in range(W):
                        tx = xs[x]
                        sd = math.hypot((tx - 0.50) * 1.6, ty - 0.42)
                        if sd < 0.20 and (ty < 0.44 or (y % 10) < 7):
                            st = max(0.0, min(1.0, (ty - 0.22) / 0.40))
                            r = _lerp(253, 236, st)
                            g = _lerp(224, 72, st)
                            b = _lerp(71, 153, st)
                        else:
                            glow = max(0.0, 1.0 - sd * 2.0) ** 2
                            r = _clamp(sky_r + 125 * glow)
                            g = _clamp(sky_g + 22 * glow)
                            b = _clamp(sky_b + 95 * glow)
                        off = x * 3
                        row[off], row[off + 1], row[off + 2] = r, g, b
                else:
                    # Perspective synthwave grid below horizon
                    depth = max(0.015, (ty - horizon) / (1.0 - horizon))
                    h_phase = (1.0 / (depth + 0.06)) % 1.0
                    is_hline = h_phase < 0.08
                    for x in range(W):
                        tx = xs[x] - 0.5
                        v_coord = (tx / (depth + 0.08)) * 2.2
                        v_phase = abs(v_coord - round(v_coord))
                        is_vline = v_phase < (0.028 / (depth + 0.25))
                        if is_hline or is_vline:
                            r, g, b = (34, 211, 238) if is_vline else (236, 72, 153)
                        else:
                            r = _lerp(22, 10, depth)
                            g = _lerp(12, 8, depth)
                            b = _lerp(42, 22, depth)
                        off = x * 3
                        row[off], row[off + 1], row[off + 2] = r, g, b

            else:
                # classic98: Iconic Windows 98 teal (#008080) with subtle woven retro texture
                for x in range(W):
                    checker = 3 if ((x // 4 + y // 4) & 1) == 0 else -2
                    vignette = int(10 * abs(xs[x] - 0.5) + 10 * abs(ty - 0.5))
                    r = _clamp(0 + checker)
                    g = _clamp(128 + checker - vignette)
                    b = _clamp(128 + checker - vignette)
                    off = x * 3
                    row[off], row[off + 1], row[off + 2] = r, g, b
            yield row

    write_png_rgb(out_path, W, H, rows())


def write_themerc(t: dict, out_path: str) -> None:
    is_light = t["mode"] == "light"
    is_c98 = t["id"] == "classic98"
    active_label_fg = "#ffffff" if is_c98 else t["fg"]
    inactive_label_fg = "#e0e0e0" if is_c98 else t["fg_muted"]
    btn_fg = "#ffffff" if is_c98 else ("#334155" if is_light else t["fg"])
    border_w = 2 if is_c98 else 1
    content = f"""# {t["ob_name"]} - UniCore OS v1.3.0 Openbox/labwc decoration theme

border.width: {border_w}
padding.width: 6
padding.height: 5
window.handle.width: 0
window.client.padding.width: 0
window.client.padding.height: 0

window.active.border.color: {t["accent_bright"]}
window.inactive.border.color: {t["border"]}

window.active.title.bg: flat solid
window.active.title.bg.color: {t["title_active"]}
window.inactive.title.bg: flat solid
window.inactive.title.bg.color: {t["title_inactive"]}

window.active.label.bg: parentrelative
window.active.label.text.color: {active_label_fg}
window.inactive.label.bg: parentrelative
window.inactive.label.text.color: {inactive_label_fg}
window.label.text.justify: left

window.active.button.unpressed.bg: parentrelative
window.active.button.unpressed.image.color: {btn_fg}
window.active.button.hover.bg: flat solid
window.active.button.hover.bg.color: {t["surface_hover"]}
window.active.button.pressed.bg: flat solid
window.active.button.pressed.bg.color: {t["border"]}

window.active.button.close.unpressed.bg: parentrelative
window.active.button.close.unpressed.image.color: {t["close_fg"]}
window.active.button.close.hover.bg: flat solid
window.active.button.close.hover.bg.color: {t["power_bg"]}
window.active.button.close.hover.image.color: #ffffff
window.active.button.close.pressed.bg: flat solid
window.active.button.close.pressed.bg.color: {t["power_hover"]}

window.inactive.button.unpressed.bg: parentrelative
window.inactive.button.unpressed.image.color: {inactive_label_fg}
window.inactive.button.hover.bg: flat solid
window.inactive.button.hover.bg.color: {t["surface_hover"]}

osd.border.width: 1
osd.border.color: {t["accent_bright"]}
osd.bg: flat solid
osd.bg.color: {t["title_active"]}
osd.label.text.color: {active_label_fg}
osd.hilight.bg.color: {t["accent"]}
osd.unhilight.bg.color: {t["border"]}

menu.border.width: 1
menu.border.color: {t["border"]}
menu.items.bg: flat solid
menu.items.bg.color: {t["surface"]}
menu.items.text.color: {t["fg"]}
menu.items.active.bg: flat solid
menu.items.active.bg.color: {t["accent"]}
menu.items.active.text.color: #ffffff
menu.separator.color: {t["border"]}
"""
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)


def write_waybar_css(t: dict, out_path: str) -> None:
    active_fg = "#000000" if t["mode"] == "light" else "#ffffff"
    css = f"""* {{
    font-family: "DejaVu Sans", "Font Awesome 6 Free", "Font Awesome 6 Free Solid", "Noto Color Emoji", sans-serif;
    font-size: 11.5px;
    border: none;
    border-radius: 0;
    min-height: 0;
}}

window#waybar {{
    background-color: {t["bar_bg"]};
    color: {t["fg"]};
    border: 1px solid {t["bar_border"]};
    border-radius: {t["radius"]}px;
}}

#custom-start {{
    background-color: {t["accent"]};
    color: #ffffff;
    font-size: 14px;
    font-weight: bold;
    padding: 3px 12px;
    margin: 5px 2px 5px 6px;
    border-radius: {t["btn_radius"]}px;
}}

#custom-start:hover {{
    background-color: {t["accent_dark"]};
}}

#custom-files,
#custom-browser,
#custom-center,
#custom-taskmgr {{
    background-color: {t["surface"]};
    color: {t["fg"]};
    font-size: 13px;
    padding: 3px 10px;
    margin: 5px 1px;
    border-radius: {t["btn_radius"]}px;
    border: 1px solid {t["border"]};
}}

#custom-files:hover,
#custom-browser:hover,
#custom-center:hover,
#custom-taskmgr:hover {{
    background-color: {t["surface_hover"]};
}}

#taskbar button {{
    padding: 3px 8px;
    margin: 5px 1px;
    border-radius: {t["btn_radius"]}px;
    background-color: {t["surface"]};
    color: {t["fg_muted"]};
    border-bottom: 2px solid transparent;
}}

#taskbar button.active {{
    background-color: {t["active_bg"]};
    color: {active_fg};
    border-bottom: 2px solid {t["accent_bright"]};
}}

#custom-rec.recording {{
    background-color: #dc2626;
    color: #ffffff;
    font-weight: bold;
    padding: 3px 8px;
    margin: 5px 2px;
    border-radius: {t["btn_radius"]}px;
}}

#custom-rec.idle,
#custom-update.up-to-date {{
    padding: 0;
    margin: 0;
    min-width: 0;
}}

#custom-update.update-available {{
    background-color: {t["accent"]};
    color: #ffffff;
    font-weight: bold;
    padding: 3px 8px;
    margin: 5px 2px;
    border-radius: {t["btn_radius"]}px;
}}

#custom-update.update-available:hover {{
    background-color: {t["accent_dark"]};
}}

#tray {{
    padding: 3px 7px;
    margin: 5px 1px;
    background-color: {t["surface"]};
    border: 1px solid {t["border"]};
    border-radius: {t["btn_radius"]}px;
}}

#custom-ai {{
    background-color: {t["ai_bg"]};
    color: #ffffff;
    font-weight: bold;
    padding: 3px 9px;
    margin: 5px 1px;
    border-radius: {t["btn_radius"]}px;
}}

#custom-ai:hover {{
    background-color: {t["ai_hover"]};
}}

#network,
#pulseaudio,
#battery,
#custom-quick {{
    padding: 3px 8px;
    margin: 5px 1px;
    background-color: {t["surface"]};
    border: 1px solid {t["border"]};
    border-radius: {t["btn_radius"]}px;
    color: {t["fg"]};
}}

#custom-quick {{
    font-size: 13px;
}}

#network:hover,
#pulseaudio:hover,
#battery:hover,
#custom-quick:hover {{
    background-color: {t["surface_hover"]};
}}

#clock {{
    padding: 2px 9px;
    margin: 4px 2px;
    background-color: {t["surface"]};
    border: 1px solid {t["border"]};
    border-radius: {t["btn_radius"]}px;
    color: {t["fg"]};
    font-size: 10.5px;
    font-weight: bold;
}}

#custom-power {{
    background-color: {t["power_bg"]};
    color: #ffffff;
    font-size: 13px;
    font-weight: bold;
    padding: 3px 9px;
    margin: 5px 2px;
    border-radius: {t["btn_radius"]}px;
}}

#custom-power:hover {{
    background-color: {t["power_hover"]};
}}

#custom-desktop {{
    min-width: 8px;
    padding: 0 3px;
    margin: 5px 4px 5px 2px;
    border-left: 1px solid {t["border"]};
    background-color: transparent;
}}

#custom-desktop:hover {{
    background-color: {t["accent_bright"]};
    border-radius: 4px;
}}
"""
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(css)


def _strip_hex(h: str) -> str:
    return h.lstrip("#").lower()


def write_fuzzel_ini(t: dict, out_path: str) -> None:
    bg_hex = _strip_hex(t["surface"] if t["mode"] == "light" else t["title_active"])
    fg_hex = _strip_hex(t["fg"])
    match_hex = _strip_hex(t["accent_bright"])
    sel_hex = _strip_hex(t["accent"])
    border_hex = _strip_hex(t["accent_bright"])
    content = f"""[main]
font=DejaVu Sans:size=11
icon-theme=Adwaita
prompt="Search apps... "
anchor=bottom-left
x-margin=8
y-margin=8
width=42
lines=12
horizontal-pad=16
vertical-pad=12
inner-pad=6
terminal=foot -e

[colors]
background={bg_hex}ff
text={fg_hex}ff
match={match_hex}ff
selection={sel_hex}ff
selection-text=ffffffff
border={border_hex}ff

[border]
width=2
radius={t["btn_radius"]}
"""
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)


def write_foot_ini(t: dict, out_path: str) -> None:
    content = f"""# UniCore OS {t["ob_name"]} foot terminal palette
[main]
font=DejaVu Sans Mono:size=10.5
pad=8x6

[colors]
alpha=0.96
background={t["term_bg"]}
foreground={t["term_fg"]}
selection-background={_strip_hex(t["accent"])}
selection-foreground=ffffff
"""
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)


def write_mako_conf(t: dict, out_path: str) -> None:
    bg_hex = _strip_hex(t["surface"] if t["mode"] == "light" else t["title_active"])
    content = f"""# UniCore OS {t["ob_name"]} mako notification theme
font=DejaVu Sans 11
background-color=#{bg_hex}ee
text-color={t["fg"]}
border-color={t["accent_bright"]}
border-size=2
border-radius={t["btn_radius"]}
padding=12
margin=12
default-timeout=5000
max-visible=4
anchor=top-right

[urgency=high]
border-color=#ef4444
default-timeout=8000

[mode=do-not-disturb]
invisible=1
"""
    os.makedirs(os.path.dirname(os.path.abspath(out_path)), exist_ok=True)
    with open(out_path, "w", encoding="utf-8", newline="\n") as f:
        f.write(content)


def generate_all(rootfs: str) -> None:
    share_dir = os.path.join(rootfs, "usr", "share", "unicore")
    themes_dir = os.path.join(share_dir, "themes")
    ob_root = os.path.join(rootfs, "usr", "share", "themes")
    os.makedirs(themes_dir, exist_ok=True)
    os.makedirs(ob_root, exist_ok=True)

    for t in THEMES:
        tid = t["id"]
        if t["wallpaper"]:
            wp_path = os.path.join(share_dir, t["wallpaper"])
            generate_wallpaper(tid, wp_path)
        write_themerc(t, os.path.join(ob_root, t["ob_name"], "openbox-3", "themerc"))
        write_waybar_css(t, os.path.join(themes_dir, f"waybar-{tid}.css"))
        write_fuzzel_ini(t, os.path.join(themes_dir, f"fuzzel-{tid}.ini"))
        write_foot_ini(t, os.path.join(themes_dir, f"foot-{tid}.ini"))
        write_mako_conf(t, os.path.join(themes_dir, f"mako-{tid}.conf"))

    # Backward-compatible symlinks replacing legacy static dark/light duplicate files
    compat_links = [
        (os.path.join(ob_root, "UniCore-Dark"), "UniCore-Midnight"),
        (os.path.join(ob_root, "UniCore-Light"), "UniCore-Daylight"),
        (os.path.join(themes_dir, "waybar-dark.css"), "waybar-midnight.css"),
        (os.path.join(themes_dir, "waybar-light.css"), "waybar-daylight.css"),
        (os.path.join(themes_dir, "fuzzel-dark.ini"), "fuzzel-midnight.ini"),
        (os.path.join(themes_dir, "fuzzel-light.ini"), "fuzzel-daylight.ini"),
    ]
    for link_path, target in compat_links:
        if os.path.lexists(link_path):
            if os.path.isdir(link_path) and not os.path.islink(link_path):
                import shutil
                shutil.rmtree(link_path)
            else:
                os.unlink(link_path)
        os.symlink(target, link_path)

    print(f"Generated 10 UniCore OS Desktop Theme Environments into {rootfs}")


if __name__ == "__main__":
    if len(sys.argv) != 2:
        sys.stderr.write("Usage: generate-theme-assets.py <rootfs_dir>\n")
        sys.exit(2)
    generate_all(sys.argv[1])
