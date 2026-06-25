# Fixing Rust Advisories

This is the ChromeOS `rust_crates` directory, which contains all third-party
rust crates. Your job is to identify if any have active rustsec advisories,
and fix them.

## Identifying advisories

To identify any active advisories, run `scripts/cargo-audit.py`. Note that this
will log some ignored crates; pay no attention to them. Instead, look for fatal
advisories. For example,

```
** Fatal advisories found:
  - crate "rand" version "0.7.3" is unsound
```

## Fixing advisories

For each advisory, your flow should look as follows. If a step fails, use
`git checkout -- .` to reset to HEAD, and move on to the next advisory.
**Please** be sure to note the failure in your summary.

1. Attempt to update the crate. This should be done via `cargo update`.
   **Always** do targeted updates, like so:

```bash
$ cd projects && cargo update -p rand
```

2. Run `./vendor.py`. Note that this regenerates all crates, and patches may
   fail to apply. The `vendor.py` script has more information on how patches
   are identified and applied.
3. If the above is successful, run `git add .` and create a commit. The message
   of the commit should look like:

"""
cargo-update PACKAGE_NAME_VERSION_WITH_ADVISORY

This updates PACKAGE_NAME_WITH_ADVISORY, which DETAILS_ABOUT_ADVISORY.

BUG=FIXME
TEST=CQ+1
"""

**NOTE**: The `Cargo.toml` files in `projects/` are _pinned_, as they reference
dependencies cloned from other codebases. The dependency constraints listed in
them should be considered immutable; if the issue can't be fixed without
modifying them, that is a failure you should report to the user.

### Commit message examples

"""
cargo-update time-0.3.40

This updates time, which was impacted by
https://rustsec.org/advisories/RUSTSEC-2026-0009.html.

BUG=FIXME
TEST=CQ+1
"""

Another example for an advisory without a RUSTSEC issue (e.g., `unsound`).

"""
cargo-update rand-0.7.3

This updates rand, which was reported by rustsec as unsound.

BUG=FIXME
TEST=CQ+1
"""

## Completion

When you are out of crates with advisories, summarize your work.

Example summary:

```
Updates complete!

- time-0.3.40: I made commit abcdef12345, upgrading to 0.3.44.
- anyhow-1.30.0: I tried updating, but cargo-audit.py still had errors. No
  semver-compatible version seems to have a fix.

With these changes, the RustSec advisories for time-0.3.40 are cleared.

Please remember: as the user, it's **your responsibility** to verify this work.
Generally, RustSec's website shows semver ranges of impacted crates. Any
unsuccessful upgrades also need action from you to resolve.
```
EOF
