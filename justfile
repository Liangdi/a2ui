
# Default stack + every opt-in GUI backend, each as its own `cargo` invocation.
# GUI backends are opt-in, AND bevy+iced can't be resolved together in one
# `--workspace` build (upstream codespan-reporting 0.12 / naga `termcolor`
# conflict — see the KNOWN LIMITATION note in Cargo.toml), so lint/test/check
# must iterate them per-backend rather than `--workspace`.
default-stack := "a2ui-base a2ui-image a2ui-tui a2ui-gallery a2ui"
gui-backends := "slint egui bevy iced dioxus"

# Lint everything: default stack (`--all-targets`) + each GUI backend.
clippy:
    cargo clippy -p {{default-stack}} --all-targets
    @for b in {{gui-backends}}; do \
        echo "==> clippy a2ui-$$b"; \
        cargo clippy -p a2ui-$$b --features backend --all-targets || exit 1; \
    done

# Test everything: default stack + each GUI backend.
test:
    cargo test -p {{default-stack}}
    @for b in {{gui-backends}}; do \
        echo "==> test a2ui-$$b"; \
        cargo test -p a2ui-$$b --features backend || exit 1; \
    done

# Type-check everything (faster than clippy/test): default stack + each backend.
check:
    cargo check -p {{default-stack}} --all-targets
    @for b in {{gui-backends}}; do \
        echo "==> check a2ui-$$b"; \
        cargo check -p a2ui-$$b --features backend --all-targets || exit 1; \
    done

release-patch:
    cargo release patch --no-publish --execute

release-minor:
    cargo release minor --no-publish --execute

release-major:
    cargo release major --no-publish --execute

upgrade:
    cargo +nightly update --breaking -Z unstable-options

# Publish all workspace crates to crates.io in dependency order
# (core -> image -> tui -> slint -> egui -> bevy -> iced -> dioxus -> umbrella -> gallery -> *-gallery).
# - `a2ui-image` precedes the backends (tui + every GUI backend consume it for
#   image source resolution / decode).
# - `a2ui-slint`, `a2ui-egui`, `a2ui-bevy`, `a2ui-iced`, `a2ui-dioxus` precede
#   the umbrella because `a2ui` has all five as optional dependencies, and
#   crates.io requires every (even optional) published dependency to already
#   exist in the registry.
# - The gallery binaries are last: each depends on `a2ui-gallery` + its backend.
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
    cargo publish -p a2ui --registry crates-io
    cargo publish -p a2ui-gallery --registry crates-io
    cargo publish -p a2ui-slint-gallery --registry crates-io
    cargo publish -p a2ui-egui-gallery --registry crates-io
    cargo publish -p a2ui-bevy-gallery --registry crates-io
    cargo publish -p a2ui-iced-gallery --registry crates-io
    cargo publish -p a2ui-dioxus-gallery --registry crates-io