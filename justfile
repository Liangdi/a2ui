
# Default stack + every opt-in GUI backend, each as its own `cargo` invocation.
# GUI backends are opt-in, AND bevy+iced can't be resolved together in one
# `--workspace` build (upstream codespan-reporting 0.12 / naga `termcolor`
# conflict — see the KNOWN LIMITATION note in Cargo.toml), so lint/test/check
# must iterate them per-backend rather than `--workspace`.
default-stack := "a2ui-base a2ui-image a2ui-tui a2ui-gallery a2ui"
gui-backends := "slint egui bevy iced dioxus"
# The GPUI backend lives in its OWN workspace (gpui's `cocoa =0.26.0` pin
# conflicts with dioxus's `cocoa ^0.26.1` in a shared Cargo.lock — see the root
# Cargo.toml `exclude` note). It can't be reached via `-p a2ui-gpui`, so every
# recipe below uses `--manifest-path` against these two workspace roots.
gpui-lib := "crates/gpui/Cargo.toml"
gpui-gallery := "crates/gpui-gallery/Cargo.toml"

# Lint everything: default stack (`--all-targets`) + each GUI backend + GPUI.
clippy:
    cargo clippy -p {{default-stack}} --all-targets
    @for b in {{gui-backends}}; do \
        echo "==> clippy a2ui-$$b"; \
        cargo clippy -p a2ui-$$b --features backend --all-targets || exit 1; \
    done
    @echo "==> clippy a2ui-gpui (separate workspace)"
    cargo clippy --manifest-path {{gpui-lib}} --features backend --all-targets
    cargo clippy --manifest-path {{gpui-gallery}} --all-targets

# Test everything: default stack + each GUI backend + GPUI.
test:
    cargo test -p {{default-stack}}
    @for b in {{gui-backends}}; do \
        echo "==> test a2ui-$$b"; \
        cargo test -p a2ui-$$b --features backend || exit 1; \
    done
    @echo "==> test a2ui-gpui (separate workspace)"
    cargo test --manifest-path {{gpui-lib}} --features backend

# Type-check everything (faster than clippy/test): default stack + each backend + GPUI.
check:
    cargo check -p {{default-stack}} --all-targets
    @for b in {{gui-backends}}; do \
        echo "==> check a2ui-$$b"; \
        cargo check -p a2ui-$$b --features backend --all-targets || exit 1; \
    done
    @echo "==> check a2ui-gpui (separate workspace)"
    cargo check --manifest-path {{gpui-lib}} --features backend --all-targets
    cargo check --manifest-path {{gpui-gallery}} --all-targets

release-patch:
    cargo release patch --no-publish --execute
    just release-gpui patch

release-minor:
    cargo release minor --no-publish --execute
    just release-gpui minor

release-major:
    cargo release major --no-publish --execute
    just release-gpui major

# Bump the GPUI crates (separate workspace, hardcoded versions — not inherited
# from the root workspace) in lockstep with a `release-*` run. `--no-publish`
# mirrors the root recipes; cargo-release runs once per GPUI workspace root.
# Usage: `just release-gpui patch` (also called by release-patch above).
release-gpui level:
    cargo release {{level}} --no-publish --execute --manifest-path {{gpui-lib}}
    cargo release {{level}} --no-publish --execute --manifest-path {{gpui-gallery}}

upgrade:
    cargo +nightly update --breaking -Z unstable-options

# Publish all workspace crates to crates.io in dependency order
# (core -> image -> tui -> slint -> egui -> bevy -> iced -> dioxus -> gpui -> umbrella -> gallery -> *-gallery).
# - `a2ui-image` precedes the backends (tui + every GUI backend consume it for
#   image source resolution / decode).
# - `a2ui-slint`, `a2ui-egui`, `a2ui-bevy`, `a2ui-iced`, `a2ui-dioxus`, `a2ui-gpui`
#   precede the umbrella / their galleries because each is an (optional) published
#   dependency of a later crate, and crates.io requires every (even optional)
#   published dependency to already exist in the registry.
# - The gallery binaries are last: each depends on `a2ui-gallery` + its backend.
# - The GPUI crates are in a separate workspace, so they publish via
#   `--manifest-path` (can't use `-p a2ui-gpui`). `a2ui-gpui` only needs
#   `a2ui-base` + `a2ui-image`; `a2ui-gpui-gallery` needs those + `a2ui-tui` +
#   `a2ui-gallery` + `a2ui-gpui`, hence its position near the end.
# If a later crate fails with "failed to find dependency", the registry index
# hasn't propagated yet — wait ~1 min and re-run from that crate.
publish:
    cargo publish -p a2ui-base --registry crates-io
    cargo publish -p a2ui-image --registry crates-io
    cargo publish -p a2ui-tui --registry crates-io
    cargo publish -p a2ui-slint --registry crates-io
    cargo publish -p a2ui-egui --registry crates-io
    cargo publish -p a2ui-bevy --registry crates-io
    cargo publish -p a2ui-iced --registry crates-io
    cargo publish -p a2ui-dioxus --registry crates-io
    cargo publish --manifest-path {{gpui-lib}} --registry crates-io
    cargo publish -p a2ui --registry crates-io
    cargo publish -p a2ui-gallery --registry crates-io
    cargo publish -p a2ui-slint-gallery --registry crates-io
    cargo publish -p a2ui-egui-gallery --registry crates-io
    cargo publish -p a2ui-bevy-gallery --registry crates-io
    cargo publish -p a2ui-iced-gallery --registry crates-io
    cargo publish -p a2ui-dioxus-gallery --registry crates-io
    cargo publish --manifest-path {{gpui-gallery}} --registry crates-io

# Publish only the GPUI crates (handy on their own — they live in a separate
# workspace, so this can be re-run independently of the main `publish`).
publish-gpui:
    cargo publish --manifest-path {{gpui-lib}} --registry crates-io
    cargo publish --manifest-path {{gpui-gallery}} --registry crates-io
