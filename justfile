
release-patch:
    cargo release patch --no-publish --execute

release-minor:
    cargo release minor --no-publish --execute

release-major:
    cargo release major --no-publish --execute

upgrade:
    cargo +nightly update --breaking -Z unstable-options

# Publish all workspace crates to crates.io in dependency order
# (core -> tui -> slint -> egui -> bevy -> umbrella -> gallery -> *-gallery).
# - `a2ui-slint`, `a2ui-egui`, `a2ui-bevy` precede the umbrella because `a2ui`
#   has all three as optional dependencies, and crates.io requires every (even
#   optional) published dependency to already exist in the registry.
# - The gallery binaries are last: each depends on `a2ui-gallery` + its backend.
# If a later crate fails with "failed to find dependency", the registry index
# hasn't propagated yet — wait ~1 min and re-run from that crate.
publish:
    cargo publish -p a2ui-base --registry crates-io
    cargo publish -p a2ui-tui --registry crates-io
    cargo publish -p a2ui-slint --registry crates-io
    cargo publish -p a2ui-egui --registry crates-io
    cargo publish -p a2ui-bevy --registry crates-io
    cargo publish -p a2ui --registry crates-io
    cargo publish -p a2ui-gallery --registry crates-io
    cargo publish -p a2ui-slint-gallery --registry crates-io
    cargo publish -p a2ui-egui-gallery --registry crates-io
    cargo publish -p a2ui-bevy-gallery --registry crates-io