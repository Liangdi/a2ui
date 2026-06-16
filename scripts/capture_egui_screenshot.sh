#!/usr/bin/env bash
# Capture a screenshot of the egui sci-fi HUD example into screenshot/.
#
# Why this exists: the other backends' (tui/iced/dioxus) sci-fi-hud screenshots
# were grabbed with desktop screenshot tools, but on a locked-down GNOME Wayland
# session those are unavailable to automation — `gnome-screenshot` can't see a
# Wayland-native window (its X11 fallback captures nothing), and the
# `org.gnome.Shell.Screenshot` D-Bus API returns AccessDenied. Forcing the app
# onto X11/XWayland doesn't help either (`import -window root` is also denied).
#
# So instead we use egui's own, compositor-independent screenshot path: the
# example has a built-in self-screenshot mode (env-gated, see
# `crates/egui/examples/17_scifi_hud.rs`) that opens the window, warms the HUD up
# for a few frames, then requests a screenshot via egui's `ViewportCommand`. The
# glow backend reads the GPU framebuffer *after* painting (the egui analog of
# Bevy's `Screenshot::primary_window()` + `save_to_disk`), so it works where
# desktop tools are blocked. This script just sets the env var and runs it.
#
# Usage:
#   scripts/capture_egui_screenshot.sh                  # -> screenshot/sci-fi-hud-egui.png
#   scripts/capture_egui_screenshot.sh path/to/out.png
set -euo pipefail

# Run from the repo root regardless of where the script is invoked from.
cd "$(git rev-parse --show-toplevel)"

OUT="${1:-screenshot/sci-fi-hud-egui.png}"
mkdir -p "$(dirname "$OUT")"

# The example writes to this path (absolute, since its cwd may differ).
ABS_OUT="$(pwd)/$OUT"

echo "Capturing egui sci-fi HUD screenshot -> $OUT"
# `--features backend` is required: it links the egui + eframe (glow) runtime.
# `WAYLAND_DISPLAY` is intentionally left set so eframe maps a real surface for
# the framebuffer read; the screenshot itself never touches the compositor.
A2UI_SCREENSHOT_PATH="$ABS_OUT" \
  cargo run -p a2ui-egui --example 17_scifi_hud --features backend

if [[ -f "$OUT" ]]; then
  echo "Saved $OUT"
else
  echo "ERROR: $OUT was not written" >&2
  exit 1
fi
