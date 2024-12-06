Vendored Rust Libraries
===

This repository contains vendored Rust libraries for ChromeOS userspace
binaries. They are installed by the `dev-rust/third-party-crates-src` ebuild.
Dependencies are fetched using `cargo vendor`.

Please see the individuals listed in the OWNERS file if you'd like to learn more
about this repo.

[TOC]

# Updating packages

In order to update or add any package, follow these steps:

* Find the project in `projects/` corresponding to the first-party package you'd
  like to update. If it does not exist, please see the "Adding a first-party
  package," section.
* Modify its `Cargo.toml` to add, remove or upgrade packages.
* Run `python vendor.py` inside chroot.
    * This runs `cargo vendor` first, which updates `Cargo.lock` and puts
      downloaded crates into the `vendor` directory
    * It applies any patches in the `patches` directory. It also regenerates
      checksums for packages that were modified.
    * It removes OWNER files from packages that have it (interferes with our own
      OWNERS management) and regenerates checksums
    * It checks that all packages have a supported license and lists the set of
      licenses used by the crates.
    * If `--license-map=<filename>` is given, it will dump a json file which is
      a dictionary with the crate names as keys and another dictionary with the
      `license` and `license_file` as keys.
* Verify that no patches need to be updated. Patches in `patches/` can either
  be named after a crate (in which case, they apply to _all_ versions of their
  corresponding crate), or can have a version in their name, in which case
  they're meant to apply specifically to a given version of a crate.
* If any licenses are unsupported, do the following:
    * Check if the package is actually included in the build. `cargo vendor`
      seems to also pick up dependencies for unused configs (i.e. windows). You
      will need to make sure these packages are stripped by `cargo vendor`.
    * Check if the license file exists in the crate's repository. Sometimes the
      crate authors just forget to list it in Cargo.toml. In this case, add
      a new patch to apply the license to the crate and send a patch upstream to
      include the license in the crate as well.
    * Check if the license is permissive and ok with ChromeOS. If you don't know
      how to do this, reach out to the OWNERS of this repo.
      * Yes: Update the `vendor.py` script with the new license and also update
        `dev-rust/third-party-crates-src` with the new license.
      * No: Do not use this package. Contact the OWNERS of this repo for next
        steps.

NOTE: If your patch changes over 30 files, please see the [Splitting
Patches](#splitting-patches) section below.

## Adding patches

When it is necessary to patch a package due to incompatibility, you can create
a patch targeting the specific package and store it in
`patches/`. For any given `${crate}` at `${version}`, if
`patches/${crate}-${version}` exists, the patches from that directory are
applied to the crate. If such a directory does not exist, `patches/${crate}` is
checked. Similarly, if this exists, patches are applied; otherwise, the crate
is left unpatched.

After putting the patch into the appropriate directory, run the script `./vendor.py`.
The script will re-download crates, apply all relevant patches and regenerate
`.cargo-checksum.json` files and files under `vendor_artifacts/`. You will need
to commit these changes when submitting a patch, since patches will not be
applied later during the package build.

If `./vendor.py` complains about a specific directory in `patches/` not having
a corresponding `vendor/` directory, the most likely fixes are:

* the crate is no longer required, and the patches should be deleted.
* the crate has been upgraded, and the patches that were previously applied
  should be evaluated for whether they are still relevant.

Patches can come in two forms. Files with names ending in `.patch` are always applied with
`patch -p1` to the vendor directory. Executable files that do not have names
ending in `patch` will be executed in the vendor directory which they should
apply to. All other files are ignored.

## Testing updates

Updates to this repo will be captured by the CQ. To directly test changes,
either build the `net-wireless/floss` package, or run
`cros_workon --board=${BOARD} start dev-rust/third-party-crates-src` and build
packages + run tests for your board of choice.

## Adding a first-party package

The `projects/` subdirectory contains a set of `Cargo.toml` files that roughly
correspond to what exists in the ChromeOS tree. These exist only to provide
dependency information with which we can create our `vendor/` directory, so
everything is removed except for author information, dependencies, and (if
necessary) a minimal set of features/annotations. Dependency sections outside of
what would build for ChromeOS, like `[target.'cfg(windows)'.dependencies]`, are
also removed.

Create a new directory mirroring where your Rust crate lives in the ChromeOS
tree. Add the minimal `Cargo.toml` file to the folder you just created.
Once your `Cargo.toml` seems correct and you're ready to test, run
`projects/populate-workspace.py` to add it to the workspace.
No need to create any other files, the script will do this for you.

After adding the crate to the workspace, continue with the [Updating
packages](#updating-packages) section.

Admittedly, it's sort of awkward to have two `Cargo.toml`s for each first-party
project. It may be worth trying to consolidate this in the future, though our
future bazel migration potentially influences what the 'ideal' setup here is.

NOTE: If your patch changes over 30 files, please see the [Splitting
Patches](#splitting-patches) section below.

### What is "a minimal set of features/annotations"?

`cargo vendor` will trim the vendor directory based on the _features you
require_ of your dependencies, but not their optionality. For instance, given:

```
[dependencies]
tokio = { version = "1", optional = true }
```

`cargo vendor` will simply vendor `tokio` as though the `optional = true` isn't
present. That said, it's important to note that `cargo vendor` _will_ try to
trim optional dependencies of crates which `Cargo.toml` files depend upon. For
instance, given:

```
[dependencies]
tokio = { version = "1", features = [] }
```

`cargo vendor` selects a much smaller set of crates to vendor than it would if
`features = []` were replaced with `features = ["full"]`. Hence, `Cargo.toml`s
should be sure to either specify all of the features they require explicitly in
a dependency's `features = []` block, or keep a crate-level `[features]` block
which makes the set of _potential_ `features` of dependencies we use visible to
`cargo vendor`. For example, _both_ of the following two `Cargo.toml` bodies are
OK:

```
[dependencies]
tokio = { version = "1", features = ["full"] }
```

&nbsp;

```
[dependencies]
tokio = { version = "1", features = [] }

[features]
tokio-full = [ "tokio/full" ]
```

*tl;dr*: if your crate has `optional` dependencies, feel free to drop the
`optional = ` annotation. If your crate depends on features of other crates,
please do not remove those.

### My crate's name conflicts with another first party's

The crate names in these `Cargo.toml`s aren't really relevant or used; please
add some context that seems meaningful to your package's name, or do that to the
conflicting package.

# Splitting Patches

Due to the nature of `rust_crates`, patches can get massive (tens-hundreds of
thousands of lines of diff over thousands of files). For ease of review, it's
requested that large changes are split into a few logically independent pieces:

1. All changes outside of `vendor/`.
2. Changes to `vendor/` as a result of running `./vendor.py`.

Note that `Cargo.lock` changes may land in either the first or second CLs. Use
your judgment to determine whether `Cargo.lock` changes are noise (e.g., "I
added a new package, and `Cargo.lock` now reflects that.") or signal (e.g., "I
ran `cargo update` on a few packages, and `Cargo.lock` is the _only_ thing
outside of `vendor/` that can reflect that.").

These patches should all be *landed* at the same time. The split is meant to
help increase reviewability. If you're unfamiliar with Gerrit's `Relation chain`
feature, you can make a patch stack like the above by making multiple commits in
git, and running `repo upload` as usual.

If you have one large patch that you'd like to split out into three, `git
checkout` may be helpful to you:

```sh
## Assuming you currently have the full change you want to commit
## checked out, and `git status` says your repo is clean:
$ all_changes=$(git rev-parse HEAD)
$ git checkout HEAD~ -- vendor/ projects/Cargo.lock
$ git commit --amend --no-edit
$ git checkout "${all_changes}" -- vendor projects/Cargo.lock
$ git commit -a # This commits vendor changes.

## NOTE: if any part of this fails, you can always get back to your original
## state by running `git reset --hard ${all_changes}`.
```

To ensure your splitting did the right thing, you can run
`git diff ${all_changes}`. If no changes are printed and `git status` is clean,
you successfully split your CL.

In order to make changes to old CLs, `git rebase -i` and/or `git commit --fixup`
combined with `git rebase -i --autosquash` might be helpful to you. Please see
`git`'s manual for more info.

# FAQ

## Why is `vendor.py` complaining about `cargo-vet` audits?

Please see [the cargo-vet readme](cargo-vet/README.md).

## How do I make my changes go live in `dev-rust/third-party-crates-src`?

`dev-rust/third-party-crates-src` is a `cros_workon` package. If you'd like to
hack on it for a board, you can use
`cros-workon-${BOARD} start dev-rust/third-party-crates-src`. If you'd like
changes to apply to the host, use
`cros-workon --host start dev-rust/third-party-crates-src` instead. After
running one of these, any `emerge`s of `dev-rust/third-party-crates-src` should
output that the `9999` version of this package is being emerged, and your local
changes should apply.

As an example, for local development on dependencies for the host, the "add a
crate and make the change live" cycle looks like:

```
$ cros-workon --host start dev-rust/third-party-crates-src
$ while ! rust_crates_does_what_i_want; do
   $EDITOR projects/foo/Cargo.toml && \
   ./vendor.py && \
   sudo emerge dev-rust/third-party-crates-src
 done
```

## How do I depend on two versions of the same crate?

Use the `package` field:

```
# Depend on both wasi 0.10.0 and 0.9.0.
[dependencies]
wasi-0-10 = { package = "wasi", version = "0.10.0" }
wasi-0-9 = { package = "wasi", version = "0.9.0" }
```

## How do I depend on packages that I won't actually use?

`vendor.py` will automatically clean up packages that aren't used for ChromeOS
triples, so if you just need to placate Cargo by having e.g., a Windows-only
package available, you can put your dependencies in a `cfg(windows)` dependency
block:

```
# By putting this in a cfg(windows) block, a package for wasi-0.10.* will be
# installed in the registry, but it won't actually contain any code from the
# package.
#
# As a side-effect from the above, this package will be exempt from audit
# requirements.
[target.'cfg(windows)'.dependencies]
wasi = "0.10"
```
