#!/usr/bin/env python3
"""
capture_tui.py — drive a ratatui/crossterm TUI through a PTY and capture
color-accurate screen snapshots as PNGs.

Why a PTY: ratatui apps (crossterm backend) put the terminal in raw mode and
switch to the alternate screen. They only do this when `isatty()` is true, so a
plain pipe won't work. `pexpect.spawn` allocates a real pseudo-terminal
(`os.forkpty`), so the app behaves exactly as on a real terminal.

How colors are recovered: the app emits an ANSI/CSI byte stream. We feed it to
`pyte.Screen`, a terminal emulator that records, for every cell, the resolved
foreground/background. We then rasterize that grid with Pillow — so the CSS
cascade's color changes (`:focus` -> accent blue, `:disabled` -> muted gray)
are literally visible in the PNG, not just implied by text.

Requires: pexpect, pyte, pillow.  (`pip install pexpect pyte pillow`)

Usage:
    scripts/capture_tui.py                       # default: build + run 08_live_demo
    scripts/capture_tui.py --bin target/debug/examples/12_theme_switcher
    scripts/capture_tui.py --cols 100 --rows 30 --out target/tui-captures

Output: one PNG per scripted step plus an `all_frames.png` contact sheet, in --out.
"""
import argparse
import os
import time

import pexpect
import pyte
from PIL import Image, ImageDraw, ImageFont

# Resolve project root from this script's location (scripts/ -> repo root).
ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DEFAULT_BIN = os.path.join(
    ROOT, "target", "debug", "examples", "08_live_demo"
)

# Scripted interaction for examples/08_live_demo.rs.
# Each step: (label, keystrokes). Keystrokes are raw terminal bytes; the app
# redraws within its ~100ms poll loop, and we settle 0.6s before snapshotting.
DEFAULT_STEPS = [
    ("01_initial  (focus: Open, disabled off)", b""),
    ("02_right_x2 (focus -> Export)",           b"\x1b[C\x1b[C"),
    ("03_toggle_d (disabled on)",               b"d"),
    ("04_back_to_focus_Open (disabled on)",     b"\x1b[D\x1b[D"),
]

# --------------------------------------------------------------------------- #
# Color helpers — map pyte's fg/bg values to RGB.                             #
# pyte emits: "default", hex "#rrggbb", basic-16 names, or an int (256-pal).   #
# --------------------------------------------------------------------------- #
NAMED16 = {
    "black": 0, "red": 1, "green": 2, "yellow": 3, "blue": 4,
    "magenta": 5, "cyan": 6, "white": 7, "brightblack": 8, "brightred": 9,
    "brightgreen": 10, "brightyellow": 11, "brightblue": 12, "brightmagenta": 13,
    "brightcyan": 14, "brightwhite": 15,
}


def _build_xterm256():
    pal = [(0, 0, 0), (205, 0, 0), (0, 205, 0), (205, 205, 0), (0, 0, 238),
           (205, 0, 205), (0, 205, 205), (229, 229, 229), (127, 127, 127),
           (255, 0, 0), (0, 255, 0), (255, 255, 0), (92, 92, 255), (255, 0, 255),
           (0, 255, 255), (255, 255, 255)]
    levels = [0, 95, 135, 175, 215, 255]
    for r in levels:
        for g in levels:
            for b in levels:
                pal.append((r, g, b))
    for i in range(24):
        v = 8 + 10 * i
        pal.append((v, v, v))
    return pal


XTERM256 = _build_xterm256()


_HEX = set("0123456789abcdef")


def color_rgb(c):
    """Return an (r, g, b) tuple for a pyte color value, or None for default.

    pyte 0.8.x stores colors as: "default", a bare 6-digit hex string WITHOUT
    '#' (e.g. "313244"), a basic-16 name, or an int (256-palette index).
    """
    if c is None or c == "default":
        return None
    if isinstance(c, int):
        return XTERM256[c] if 0 <= c < len(XTERM256) else None
    if isinstance(c, str):
        s = c.strip().lstrip("#").lower()
        if not s or s == "default":
            return None
        if len(s) == 6 and all(ch in _HEX for ch in s):   # bare hex, no '#'
            return (int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16))
        if s in NAMED16:
            return XTERM256[NAMED16[s]]
        if s.isdigit():                                    # 256-palette index
            idx = int(s)
            return XTERM256[idx] if 0 <= idx < len(XTERM256) else None
        return None
    return None


# --------------------------------------------------------------------------- #
# Font + rasterization.                                                        #
# --------------------------------------------------------------------------- #
FONT_CANDIDATES = [
    "/usr/share/fonts/google-noto-vf/NotoSansMono[wght].ttf",
    "/usr/share/fonts/urw-base35/NimbusMonoPS-Regular.otf",
    "/usr/share/fonts/google-noto-sans-mono-cjk-vf-fonts/NotoSansMonoCJK-VF.ttc",
]


def load_font(size):
    for path in FONT_CANDIDATES:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                continue
    return ImageFont.load_default()


def render_screen(screen, font, default_bg=(0, 0, 0), default_fg=(205, 205, 205)):
    cell_w = max(int(round(font.getlength("M"))), 6)
    cell_h = int(round(font.size * 1.25))
    img = Image.new("RGB", (cell_w * screen.columns, cell_h * screen.lines), default_bg)
    draw = ImageDraw.Draw(img)
    for y in range(screen.lines):
        row = screen.buffer[y]
        for x in range(screen.columns):
            ch = row[x]
            bg = color_rgb(ch.bg) or default_bg
            fg = color_rgb(ch.fg) or default_fg
            px, py = x * cell_w, y * cell_h
            draw.rectangle([px, py, px + cell_w - 1, py + cell_h - 1], fill=bg)
            data = ch.data
            if data and data != " ":
                draw.text((px + 1, py), data, font=font, fill=fg)
    return img, cell_w, cell_h


# --------------------------------------------------------------------------- #
# Capture loop.                                                                #
# --------------------------------------------------------------------------- #
def capture(bin_path, cols, rows, steps, out_dir, quit_key=b"q", settle=0.6):
    os.makedirs(out_dir, exist_ok=True)
    child = pexpect.spawn(bin_path, dimensions=(rows, cols), encoding=None, timeout=10)
    buf = bytearray()
    font = load_font(18)
    frames = []

    def snapshot(label):
        child.expect(pexpect.TIMEOUT, timeout=settle)
        if child.before:
            buf.extend(child.before)
        screen = pyte.Screen(cols, rows)
        pyte.Stream(screen).feed(buf.decode("utf-8", errors="replace"))
        img, _, _ = render_screen(screen, font)
        frames.append((label, img))

    for label, keys in steps:
        if keys:
            child.send(keys)
        snapshot(label)

    child.send(quit_key)
    child.expect(pexpect.EOF, timeout=5)
    child.close()

    _save_frames(frames, out_dir, font)
    return frames, child.exitstatus


def _save_frames(frames, out_dir, font):
    slug = lambda s: "".join(c if c.isalnum() else "_" for c in s)[:40]
    for i, (label, img) in enumerate(frames):
        img.save(os.path.join(out_dir, f"{i:02d}_{slug(label)}.png"))

    # Contact sheet: label bar + frame, stacked vertically.
    label_h = 28
    gap = 8
    total_h = sum(label_h + gap + f.height for _, f in frames) + gap
    max_w = max(f.width for _, f in frames)
    sheet = Image.new("RGB", (max_w, total_h), (24, 24, 24))
    draw = ImageDraw.Draw(sheet)
    y = gap
    for label, img in frames:
        draw.text((6, y + 4), label, font=font, fill=(180, 200, 255))
        y += label_h
        sheet.paste(img, (0, y))
        y += img.height + gap
    sheet.save(os.path.join(out_dir, "all_frames.png"))


def main():
    p = argparse.ArgumentParser(description=__doc__,
                                formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--bin", default=DEFAULT_BIN, help="path to the example binary")
    p.add_argument("--cols", type=int, default=80)
    p.add_argument("--rows", type=int, default=24)
    p.add_argument("--out", default=os.path.join(ROOT, "target", "tui-captures"))
    p.add_argument("--quit-key", default="q")
    args = p.parse_args()

    if not os.path.exists(args.bin):
        raise SystemExit(
            f"binary not found: {args.bin}\nbuild it first, e.g. "
            f"`cargo build --example 08_live_demo`"
        )

    print(f"driving {args.bin} ({args.cols}x{args.rows}) via PTY ...")
    t0 = time.time()
    frames, status = capture(
        args.bin, args.cols, args.rows, DEFAULT_STEPS, args.out,
        quit_key=args.quit_key.encode(),
    )
    print(f"captured {len(frames)} frames -> {args.out}  ({time.time()-t0:.1f}s)")
    for label, img in frames:
        print(f"  {img.width}x{img.height}  {label}")
    print(f"app exit status: {status}  ({'clean' if status == 0 else 'WARN'})")
    print(f"contact sheet:  {os.path.join(args.out, 'all_frames.png')}")


if __name__ == "__main__":
    main()
