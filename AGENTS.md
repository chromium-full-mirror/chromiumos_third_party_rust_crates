# rust-crates

## Patching a crate

If you need to patch an existing crate, it's notable that `vendor.py` wipes out
the entire `vendor/` and `vendor_artifacts/` subdirectories on each run (so
patching sources in `vendor/` is ephemeral).

Instead, the recommended flow is:

1. Make changes to `vendor/${CRATE}/...`, knowing they will be wiped out.
2. Run `git diff --relative=vendor/${CRATE}`.
3. Write that as a patch to `patches/`.

NOTE: If a diff is large and mechanical, you can also use a shell script where
`cwd="vendor/${CRATE}"`. See `vendor.py` for details on how to write/use these
scripts.

NOTE: The above is sufficient to upload a CL to change the crate, but if
you need to build/test with the modified crate locally, you need to
follow extra steps in your chroot. See README.md's "How do I make my changes go
live in dev-rust/third-party-crates-src?" section for specific instructions.

# README.md

@README.md
