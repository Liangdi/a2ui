
release-patch:
    cargo release patch --no-publish --execute

release-minor:
    cargo release minor --no-publish --execute

release-major:
    cargo release major --no-publish --execute

upgrade:
    cargo +nightly update --breaking -Z unstable-options

# Publish all workspace crates to crates.io in dependency order
# (core -> tui -> umbrella -> gallery). If a later crate fails with
# "failed to find dependency", the registry index hasn't propagated yet —
# wait ~1 min and re-run from that crate.
publish:
    cargo publish -p a2ui-core --registry crates-io
    cargo publish -p a2ui-tui --registry crates-io
    cargo publish -p a2ui --registry crates-io
    cargo publish -p a2ui-gallery --registry crates-io