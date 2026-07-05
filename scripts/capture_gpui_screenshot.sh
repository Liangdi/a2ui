#!/usr/bin/env bash
# Capture a screenshot of the GPUI sci-fi HUD example into screenshot/.
#
# Why this exists: the other backends' sci-fi-hud screenshots were grabbed
# with desktop screenshot tools or an in-app self-screenshot mode, but on a
# locked-down GNOME Wayland session neither works for GPUI — `grim` reports
# "compositor doesn't support the screen capture protocol" and `gnome-screenshot`
# falls back to X11 and captures nothing. GPUI also has no egui/Bevy-style
# built-in screenshot API to fall back on.
#
# So instead we run the example under `xvfb-run` (an X11 virtual framebuffer),
# which forces GPUI onto its X11 backend (where it renders correctly), warm it
# up a few seconds, then grab the root window with `import` and crop to the
# 1000x680 content. Requires `xvfb-run` (xorg-x11-server-Xvfb) and `import`
# (ImageMagick).
#
# Usage:
#   scripts/capture_gpui_screenshot.sh                  # -> screenshot/sci-fi-hud-gpui.png
#   scripts/capture_gpui_screenshot.sh path/to/out.png
set -euo pipefail

# Run from the repo root regardless of where the script is invoked from.
cd "$(git rev-parse --show-toplevel)"

OUT="${1:-screenshot/sci-fi-hud-gpui.png}"
mkdir -p "$(dirname "$OUT")"

# The example's window is 1000x680; size the virtual screen to match so the
# window fills it (no centering offset to crop around).
WIN_W=1000
WIN_H=680

BIN="target/debug/examples/17_scifi_hud"

# Build the example (separate workspace — reach it via --manifest-path).
cargo build --manifest-path crates/gpui/Cargo.toml --features backend --example 17_scifi_hud

# Render under Xvfb and grab the root window. `import` needs the DISPLAY that
# xvfb-run sets up, so the capture runs inside the same `xvfb-run` invocation.
xvfb-run -a -s "-screen 0 ${WIN_W}x${WIN_H}x24" bash -c "
  unset WAYLAND_DISPLAY
  cd '$(pwd)'
  ./$BIN >/dev/null 2>&1 & APP=\$!
  # Warm up: let GPUI open the window + render a few HUD ticks.
  sleep 14
  import -window root /tmp/gpui-hud-raw.png
  kill \$APP 2>/dev/null
  wait \$APP 2>/dev/null || true
"

# Crop to the content bounding box (trim any black border from the framebuffer).
python3 - "$OUT" <<'PY'
import sys
from PIL import Image, ImageChops
out = sys.argv[1]
im = Image.open('/tmp/gpui-hud-raw.png').convert('RGB')
bbox = ImageChops.difference(im, Image.new('RGB', im.size, (0, 0, 0))).getbbox()
if bbox:
    im = im.crop(bbox)
im.save(out)
print(f"Saved {out} ({im.size[0]}x{im.size[1]})")
PY
