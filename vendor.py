#!/usr/bin/env python3
# -*- coding: utf-8 -*-
# Copyright 2021 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.
""" This script cleans up the vendor directory.
"""
import argparse
import collections
import functools
import hashlib
import json
import os
import pathlib
import re
import shutil
import subprocess
import sys
import textwrap
import toml

# We only care about crates we're actually going to use and that's usually
# limited to ones with cfg(linux). For running `cargo metadata`, limit results
# to only these platforms.
ALL_SUPPORTED_PLATFORMS = (
    # Main targets.
    "x86_64-cros-linux-gnu",
    "armv7a-cros-linux-gnueabihf",
    "aarch64-cros-linux-gnu",
    # As far as we care, this is the same as x86_64-cros-linux-gnu.
    # "x86_64-pc-linux-gnu",
    # Baremetal targets.
    "thumbv6m-none-eabi",
    "thumbv7m-none-eabi",
    "thumbv7em-none-eabihf",
    "i686-unknown-uefi",
    "x86_64-unknown-uefi",
)

# A series of crates which are to be made empty by having no (non-comment)
# contents in their `lib.rs`, rather than by inserting a compilation error.
NOP_EMPTY_CRATES = frozenset({"windows"})

EMPTY_CRATE_BODY = """\
compile_error!("This crate cannot be built for this configuration.");
"""
NOP_EMPTY_CRATE_BODY = "// " + EMPTY_CRATE_BODY


def _rerun_checksums(package_path):
    """Re-run checksums for given package.

    Writes resulting checksums to $package_path/.cargo-checksum.json.
    """
    hashes = dict()
    checksum_path = os.path.join(package_path, ".cargo-checksum.json")
    if not pathlib.Path(checksum_path).is_file():
        return False

    with open(checksum_path, "r") as fread:
        contents = json.load(fread)

    for root, _, files in os.walk(package_path, topdown=True):
        for f in files:
            # Don't checksum an existing checksum file
            if f == ".cargo-checksum.json":
                continue

            file_path = os.path.join(root, f)
            with open(file_path, "rb") as frb:
                m = hashlib.sha256()
                m.update(frb.read())
                d = m.hexdigest()

                # Key is relative to the package path so strip from beginning
                key = os.path.relpath(file_path, package_path)
                hashes[key] = d

    if hashes:
        print(
            "{} regenerated {} hashes".format(package_path, len(hashes.keys()))
        )
        contents["files"] = hashes
        with open(checksum_path, "w") as fwrite:
            json.dump(contents, fwrite, sort_keys=True)

    return True


def _remove_OWNERS_checksum(root):
    """Delete all OWNERS files from the checksum file.

    Args:
        root: Root directory for the vendored crate.

    Returns:
        True if OWNERS was found and cleaned up. Otherwise False.
    """
    checksum_path = os.path.join(root, ".cargo-checksum.json")
    if not pathlib.Path(checksum_path).is_file():
        return False

    with open(checksum_path, "r") as fread:
        contents = json.load(fread)

    del_keys = []
    for cfile in contents["files"]:
        if "OWNERS" in cfile:
            del_keys.append(cfile)

    for key in del_keys:
        del contents["files"][key]

    if del_keys:
        print("{} deleted: {}".format(root, del_keys))
        with open(checksum_path, "w") as fwrite:
            json.dump(contents, fwrite, sort_keys=True)

    return bool(del_keys)


def cleanup_owners(vendor_path):
    """Remove owners checksums from the vendor directory.

    We currently do not check in the OWNERS files from vendored crates because
    they interfere with the find-owners functionality in gerrit. This cleanup
    simply finds all instances of "OWNERS" in the checksum files within and
    removes them.

    Args:
        vendor_path: Absolute path to vendor directory.
    """
    deps_cleaned = []
    for root, dirs, _ in os.walk(vendor_path):
        for d in dirs:
            removed = _remove_OWNERS_checksum(os.path.join(root, d))
            if removed:
                deps_cleaned.append(d)

    if deps_cleaned:
        print("Cleanup owners:\n {}".format("\n".join(deps_cleaned)))


def apply_single_patch(patch, workdir):
    """Apply a single patch and return whether it was successful.

    Returns:
        True if successful. False otherwise.
    """
    proc = subprocess.run(
        [
            "patch",
            "-p1",
            "--no-backup-if-mismatch",
            "-i",
            patch,
        ],
        cwd=workdir,
    )
    return proc.returncode == 0


def apply_patch_script(script, workdir):
    """Run the given patch script, returning whether it exited cleanly.

    Returns:
        True if successful. False otherwise.
    """
    return subprocess.run([script], cwd=workdir).returncode == 0


def determine_vendor_crates(vendor_path):
    """Returns a map of {crate_name: [directory]} at the given vendor_path."""
    result = collections.defaultdict(list)
    crate_version_re = re.compile(r"-\d+\.\d+\.\d+(:?[+-]|$)")
    for crate_name_plus_ver in os.listdir(vendor_path):
        version = crate_version_re.search(crate_name_plus_ver)
        assert version, crate_name_plus_ver
        name = crate_name_plus_ver[: version.start()]
        result[name].append(crate_name_plus_ver)

    for crate_list in result.values():
        crate_list.sort()
    return result


def apply_patches(patches_path, vendor_path):
    """Finds patches and applies them to sub-folders in the vendored crates.

    Args:
        patches_path: Path to folder with patches. Expect all patches to be one
                    level down (matching the crate name).
        vendor_path: Root path to vendored crates directory.
    """
    checksums_for = {}

    # Don't bother running if patches directory is empty
    if not pathlib.Path(patches_path).is_dir():
        return

    patches_failed = False
    vendor_crate_map = determine_vendor_crates(vendor_path)
    # Look for all patches and apply them
    for d in os.listdir(patches_path):
        dir_path = os.path.join(patches_path, d)

        # We don't process patches in root dir
        if not os.path.isdir(dir_path):
            continue

        # We accept one of two forms here:
        # - direct targets (these name # `${crate_name}-${version}`)
        # - simply the crate name (which applies to all versions of the
        #   crate)
        direct_target = os.path.join(vendor_path, d)
        if os.path.isdir(direct_target):
            patch_targets = [d]
        elif d in vendor_crate_map:
            patch_targets = vendor_crate_map[d]
        else:
            raise RuntimeError(f"Unknown crate in {vendor_path}: {d}")

        for patch in os.listdir(dir_path):
            file_path = os.path.join(dir_path, patch)

            # Skip if not a patch file
            if not os.path.isfile(file_path):
                continue

            if patch.endswith(".patch"):
                apply = apply_single_patch
            elif os.access(file_path, os.X_OK):
                apply = apply_patch_script
            else:
                # Unrecognized. Skip it.
                continue

            for target_name in patch_targets:
                checksums_for[target_name] = True
                target = os.path.join(vendor_path, target_name)
                print(f"-- Applying {file_path} to {target}")
                if not apply(file_path, target):
                    print(f"Failed to apply {file_path} to {target}")
                    patches_failed = True

    # Do this late, so we can report all of the failing patches in one
    # invocation.
    if patches_failed:
        raise ValueError("Patches failed; please see above logs")

    # Re-run checksums for all modified packages since we applied patches.
    for key in checksums_for.keys():
        _rerun_checksums(os.path.join(vendor_path, key))


def get_workspace_cargo_toml(working_dir):
    """Returns all Cargo.toml files under working_dir."""
    return [working_dir / "projects" / "Cargo.toml"]


def run_cargo_vendor(working_dir):
    """Runs cargo vendor.

    Args:
        working_dir: Directory to run inside. This should be the directory where
                     Cargo.toml is kept.
    """
    # `cargo vendor` may update dependencies (which may update metadata).
    load_all_package_metadata.cache_clear()

    # Cargo will refuse to revendor into versioned directories, which leads to
    # repeated `./vendor.py` invocations trying to apply patches to
    # already-patched sources. Remove the existing vendor directory to avoid
    # this.
    vendor_dir = working_dir / "vendor"
    if vendor_dir.exists():
        shutil.rmtree(vendor_dir)

    cargo_cmdline = [
        "cargo",
        "vendor",
        "--versioned-dirs",
        "-v",
        "--manifest-path=projects/Cargo.toml",
        "--",
        "vendor",
    ]
    subprocess.check_call(cargo_cmdline, cwd=working_dir)


def load_single_metadata(working_dir, filter_platform):
    """Load metadata for all projects under a given directory.

    Args:
        working_dir: Base directory to run from.
        filter_platform: Filter packages to ones configured for this platform.
    """
    metadata_objects = []
    cmd = [
        "cargo",
        "metadata",
        "--format-version=1",
        "--manifest-path=projects/Cargo.toml",
    ]
    # Conditionally add platform filter
    if filter_platform:
        cmd += ("--filter-platform", filter_platform)
    output = subprocess.check_output(cmd, cwd=working_dir)
    return json.loads(output)


# Calls to this are somewhat expensive, and repeated a fair few times
# throughout `./vendor.py`. Measuring locally, having a cache here speeds this
# script up by 1.4x.
@functools.lru_cache()
def load_all_package_metadata(working_dir, platforms=ALL_SUPPORTED_PLATFORMS):
    """Loads and merges metadata for all platforms in `platforms`.

    This drops a lot of data from `cargo metadata`. Some of this metadata is
    hard to merge, other bits of it just aren't worth keeping at the moment.
    """
    assert platforms, f"`platforms` should have things; has {platforms}"

    found_package_ids = set()
    results = []
    for platform in platforms:
        metadata = load_single_metadata(working_dir, platform)["packages"]
        for package in metadata:
            package_id = package["id"]
            if package_id in found_package_ids:
                continue

            found_package_ids.add(package_id)
            results.append(
                {
                    "id": package["id"],
                    "license": package["license"],
                    "license_file": package["license_file"],
                    "name": package["name"],
                    "version": package["version"],
                }
            )

    return results


class LicenseManager:
    """Manage consolidating licenses for all packages."""

    # These are all the licenses we support. Keys are what is seen in metadata
    # and values are what is expected by ebuilds.
    SUPPORTED_LICENSES = {
        "0BSD": "0BSD",
        "Apache-2.0": "Apache-2.0",
        "BSD-2-Clause": "BSD-2",
        "BSD-3-Clause": "BSD-3",
        "ISC": "ISC",
        "MIT": "MIT",
        "MPL-2.0": "MPL-2.0",
        "unicode": "unicode",
        "Zlib": "ZLIB",
    }

    # Prefer to take attribution licenses in this order. All these require that
    # we actually use the license file found in the package so they MUST have
    # a license file set.
    PREFERRED_ATTRIB_LICENSE_ORDER = ["MIT", "BSD-3", "ISC"]

    # If Apache license is found, always prefer it (simplifies attribution)
    APACHE_LICENSE = "Apache-2.0"

    # Regex for license files found in the vendored directories. Search for
    # these files with re.IGNORECASE.
    #
    # These will be searched in order with the earlier entries being preferred.
    LICENSE_NAMES_REGEX = [
        r"^license-mit$",
        r"^copyright$",
        r"^licen[cs]e.*$",
    ]

    # Some crates have their license file in other crates. This usually occurs
    # because multiple crates are published from the same git repository and the
    # license isn't updated in each sub-crate. In these cases, we can just
    # ignore these packages.
    MAP_LICENSE_TO_OTHER = {
        "failure_derive": "failure",
        "grpcio-compiler": "grpcio",
        "grpcio-sys": "grpcio",
        "protobuf-codegen": "protobuf",
        "protobuf-parse": "protobuf",
        "protobuf-support": "protobuf",
        "rustyline-derive": "rustyline",
    }

    # Map a package to a specific license and license file. Only use this if
    # a package doesn't have an easily discoverable license or exports its
    # license in a weird way. Prefer to patch the project with a license and
    # upstream the patch instead.
    STATIC_LICENSE_MAP = {
        # "package name": ("license name", "license file relative location")
        # Patch for adding these are upstream, but the patch application
        # doesn't apply to `cargo metadata`. This is presumably because it
        # can't detect our vendor directory.
        # https://gitlab.freedesktop.org/slirp/libslirp-sys/-/merge_requests/6
        "libslirp-sys": ("MIT", "LICENSE"),
        # https://gitlab.freedesktop.org/anholt/deqp-runner/-/merge_requests/48
        "deqp-runner": ("MIT", "LICENSE"),
        # Upstream prefers to embed license text inside README.md:
        "riscv": ("ISC", "README.md"),
        "riscv-rt": ("ISC", "README.md"),
        "zerocopy": ("BSD-2", "LICENSE"),
        "zerocopy-derive": ("BSD-2", "LICENSE"),
    }

    def __init__(self, working_dir, vendor_dir):
        self.working_dir = working_dir
        self.vendor_dir = vendor_dir

    def _find_license_in_dir(self, search_dir):
        for p in os.listdir(search_dir):
            # Ignore anything that's not a file
            if not os.path.isfile(os.path.join(search_dir, p)):
                continue

            # Now check if the name matches any of the regexes
            # We'll return the first matching file.
            for regex in self.LICENSE_NAMES_REGEX:
                if re.search(regex, p, re.IGNORECASE):
                    yield os.path.join(search_dir, p)
                    break

    def _guess_license_type(self, license_file):
        if "-MIT" in license_file:
            return "MIT"
        elif "-APACHE" in license_file:
            return "APACHE"
        elif "-BSD" in license_file:
            return "BSD-3"

        with open(license_file, "r") as f:
            lines = f.read()
            if "MIT" in lines:
                return "MIT"
            elif "Apache" in lines:
                return "APACHE"
            elif "BSD 3-Clause" in lines:
                return "BSD-3"

        return ""

    def generate_license(
        self, skip_license_check, print_map_to_file, license_shorthand_file
    ):
        """Generate single massive license file from metadata."""
        metadata = load_all_package_metadata(self.working_dir)

        special_unicode_license = "(MIT OR Apache-2.0) AND Unicode-DFS-2016"
        bad_licenses = {}

        # Keep license map ordered so it generates a consistent license map
        license_map = {}

        skip_license_check = skip_license_check or []
        has_unicode_license = False

        for package in metadata:
            # Skip the synthesized Cargo.toml packages that exist solely to
            # list dependencies.
            if "path+file:///" in package["id"]:
                continue

            pkg_name = package["name"]
            if pkg_name in skip_license_check:
                print(
                    "Skipped license check on {}. Reason: Skipped from command line".format(
                        pkg_name
                    )
                )
                continue

            if pkg_name in self.MAP_LICENSE_TO_OTHER:
                print(
                    "Skipped license check on {}. Reason: License already in {}".format(
                        pkg_name, self.MAP_LICENSE_TO_OTHER[pkg_name]
                    )
                )
                continue

            # Check if we have a static license map for this package. Use the
            # static values if we have it already set.
            if pkg_name in self.STATIC_LICENSE_MAP:
                license, license_file = self.STATIC_LICENSE_MAP[pkg_name]
                license_map[pkg_name] = {
                    "license": license,
                    "license_file": license_file,
                }
                continue

            license_files = []
            # use `or ''` instead of get's default, since `package` may have a
            # None value for 'license'.
            license = package.get("license") or ""

            # We ignore the metadata for license file because most crates don't
            # have it set. Just scan the source for licenses.
            pkg_version = package["version"]
            license_files = list(
                self._find_license_in_dir(
                    os.path.join(self.vendor_dir, f"{pkg_name}-{pkg_version}")
                )
            )

            # FIXME(b/240953811): The code later in this loop is only
            # structured to handle ORs, not ANDs. Fortunately, this license in
            # particular is `AND`ed between a super common license (Apache) and
            # a more obscure one (unicode). This hack is specifically intended
            # for the `unicode-ident` crate, though no crate name check is
            # made, since it's OK other crates happen to have this license.
            if license == special_unicode_license:
                has_unicode_license = True
                # We'll check later to be sure MIT or Apache-2.0 is represented
                # properly.
                for x in license_files:
                    if os.path.basename(x) == "LICENSE-UNICODE":
                        license_file = x
                        break
                else:
                    raise ValueError(
                        "No LICENSE-UNICODE found in " f"{license_files}"
                    )
                license_map[pkg_name] = {
                    "license": license,
                    "license_file": license_file,
                }
                continue

            # If there are multiple licenses, they are delimited with "OR" or "/"
            delim = " OR " if " OR " in license else "/"
            found = [x.strip() for x in license.split(delim)]

            # Filter licenses to ones we support
            licenses_or = [
                self.SUPPORTED_LICENSES[f]
                for f in found
                if f in self.SUPPORTED_LICENSES
            ]

            # If apache license is found, always prefer it because it simplifies
            # license attribution (we can use existing Apache notice)
            if self.APACHE_LICENSE in licenses_or:
                license_map[pkg_name] = {"license": self.APACHE_LICENSE}

            # Handle single license that has at least one license file
            # We pick the first license file and the license
            elif len(licenses_or) == 1:
                if license_files:
                    l = licenses_or[0]
                    lf = license_files[0]

                    license_map[pkg_name] = {
                        "license": l,
                        "license_file": os.path.relpath(lf, self.working_dir),
                    }
                else:
                    bad_licenses[pkg_name] = "{} missing license file".format(
                        licenses_or[0]
                    )
            # Handle multiple licenses
            elif len(licenses_or) > 1:
                # Check preferred licenses in order
                license_found = False
                for l in self.PREFERRED_ATTRIB_LICENSE_ORDER:
                    if not l in licenses_or:
                        continue

                    for f in license_files:
                        if self._guess_license_type(f) == l:
                            license_found = True
                            license_map[pkg_name] = {
                                "license": l,
                                "license_file": os.path.relpath(
                                    f, self.working_dir
                                ),
                            }
                            break

                    # Break out of loop if license is found
                    if license_found:
                        break
            else:
                bad_licenses[pkg_name] = license

        # If we had any bad licenses, we need to abort
        if bad_licenses:
            for k in bad_licenses.keys():
                print(
                    "{} had no acceptable licenses: {}".format(
                        k, bad_licenses[k]
                    )
                )
            raise Exception("Bad licenses in vendored packages.")

        # Write license map to file
        if print_map_to_file:
            with open(
                os.path.join(self.working_dir, print_map_to_file), "w"
            ) as lfile:
                json.dump(license_map, lfile, sort_keys=True)

        # Raise missing licenses unless we have a valid reason to ignore them
        raise_missing_license = False
        for name, v in license_map.items():
            if (
                "license_file" not in v
                and v.get("license", "") != self.APACHE_LICENSE
            ):
                raise_missing_license = True
                print(
                    "  {}: Missing license file. Fix or add to ignorelist.".format(
                        name
                    )
                )

        if raise_missing_license:
            raise Exception(
                "Unhandled missing license file. "
                "Make sure all are accounted for before continuing."
            )

        has_license_types = {x["license"] for x in license_map.values()}
        if has_unicode_license:
            # Replace this license with the actual SPDX license we plan to use.
            has_license_types.remove(special_unicode_license)
            has_license_types.add("unicode")
            if self.APACHE_LICENSE not in has_license_types:
                raise ValueError(
                    "Need the apache license; currently have: "
                    f"{sorted(has_license_types)}"
                )

        sorted_licenses = sorted(has_license_types)
        print("The following licenses are in use:", sorted_licenses)
        header = textwrap.dedent(
            """\
            # File to describe the licenses used by this registry.
            # Used so it's easy to automatically verify ebuilds are updated.
            # Each line is a license. Lines starting with # are comments.
            """
        )
        with open(license_shorthand_file, "w", encoding="utf-8") as f:
            f.write(header)
            f.write("\n".join(sorted_licenses))


def clean_source_related_lines_in_place(cargo_toml):
    """Removes all [[bin]] (and similar) sections in `cargo_toml`."""
    cargo_toml.pop("bench", None)
    cargo_toml.pop("bin", None)
    cargo_toml.pop("examples", None)
    cargo_toml.pop("test", None)

    lib = cargo_toml.get("lib")
    if lib:
        lib.pop("path", None)

    package = cargo_toml.get("package")
    if package:
        package.pop("build", None)
        package.pop("default-run", None)
        package.pop("include", None)


def clean_features_in_place(cargo_toml):
    """Removes all side-effects of features in `cargo_toml`."""
    features = cargo_toml.get("features")
    if not features:
        return

    for name in features:
        features[name] = []


def remove_all_dependencies_in_place(cargo_toml):
    """Removes all `target.*.dependencies` from `cargo_toml`."""
    cargo_toml.pop("build-dependencies", None)
    cargo_toml.pop("dependencies", None)
    cargo_toml.pop("dev-dependencies", None)

    target = cargo_toml.get("target")
    if not target:
        return

    empty_keys = []
    for key, values in target.items():
        values.pop("build-dependencies", None)
        values.pop("dependencies", None)
        values.pop("dev-dependencies", None)
        if not values:
            empty_keys.append(key)

    if len(empty_keys) == len(target):
        del cargo_toml["target"]
    else:
        for key in empty_keys:
            del target[key]


class CrateDestroyer:
    def __init__(self, working_dir, vendor_dir):
        self.working_dir = working_dir
        self.vendor_dir = vendor_dir

    def _modify_cargo_toml(self, pkg_path):
        with open(os.path.join(pkg_path, "Cargo.toml"), "r") as cargo:
            contents = toml.load(cargo)

        package = contents["package"]

        # Change description, license and delete license key
        package["description"] = "Empty crate that should not build."
        package["license"] = "Apache-2.0"

        package.pop("license_file", None)
        # If there's no build.rs but we specify `links = "foo"`, Cargo gets
        # upset.
        package.pop("links", None)

        # Some packages have cfg-specific dependencies. Remove them here; we
        # don't care about the dependencies of an empty package.
        #
        # This is a load-bearing optimization: `dev-python/toml` doesn't
        # always round-trip dumps(loads(x)) correctly when `x` has keys with
        # strings (b/242589711#comment3). The place this has bitten us so far
        # is target dependencies, which can be harmlessly removed for now.
        #
        # Cleaning features in-place is also necessary, since we're removing
        # dependencies, and a feature can enable features in dependencies.
        # Cargo errors out on `[features] foo = "bar/baz"` if `bar` isn't a
        # dependency.
        clean_features_in_place(contents)
        remove_all_dependencies_in_place(contents)

        # Since we're removing all source files, also be sure to remove
        # source-related keys.
        clean_source_related_lines_in_place(contents)

        with open(os.path.join(pkg_path, "Cargo.toml"), "w") as cargo:
            toml.dump(contents, cargo)

    def _replace_source_contents(self, package_path, compile_error):
        # First load the checksum file before starting
        checksum_file = os.path.join(package_path, ".cargo-checksum.json")
        with open(checksum_file, "r") as csum:
            checksum_contents = json.load(csum)

        # Also load the cargo.toml file which we need to write back
        cargo_file = os.path.join(package_path, "Cargo.toml")
        with open(cargo_file, "rb") as cfile:
            cargo_contents = cfile.read()

        shutil.rmtree(package_path)

        # Make package and src dirs and replace lib.rs
        os.makedirs(os.path.join(package_path, "src"), exist_ok=True)
        with open(os.path.join(package_path, "src", "lib.rs"), "w") as librs:
            librs.write(
                EMPTY_CRATE_BODY if compile_error else NOP_EMPTY_CRATE_BODY
            )

        # Restore cargo.toml
        with open(cargo_file, "wb") as cfile:
            cfile.write(cargo_contents)

        # Restore checksum
        with open(checksum_file, "w") as csum:
            json.dump(checksum_contents, csum)

    def destroy_unused_crates(self, destroyed_crates_file: pathlib.Path):
        metadata = [
            (x["name"], x["version"])
            for x in load_single_metadata(
                self.working_dir, filter_platform=None
            )["packages"]
        ]
        used_packages = {
            (x["name"], x["version"])
            for x in load_all_package_metadata(self.working_dir)
        }

        cleaned_packages = []
        # Since we're asking for _all_ metadata packages, we may see
        # duplication.
        for package_desc in metadata:
            # Skip used packages
            if package_desc in used_packages:
                continue

            package_name, package_version = package_desc
            # Detect the correct package path to destroy
            pkg_path = os.path.join(
                self.vendor_dir,
                "{}-{}".format(package_name, package_version),
            )
            if not os.path.isdir(pkg_path):
                print(f"Crate {package_name} not found at {pkg_path}")
                continue

            self._replace_source_contents(
                pkg_path, compile_error=package_name not in NOP_EMPTY_CRATES
            )
            self._modify_cargo_toml(pkg_path)
            _rerun_checksums(pkg_path)
            cleaned_packages.append((package_name, package_version))

        for pkg, ver in cleaned_packages:
            print(f"Removed unused crate {pkg}@{ver}")

        # Write a list of crates that've been destroyed. This is used by
        # `scripts/cargo-vet.py`.
        file_header = "# List of destroyed crates autogenerated by vendor.py."
        file_lines = [f"{pkg} {ver}" for pkg, ver in cleaned_packages]
        destroyed_crates_file.write_text(
            "\n".join([file_header] + file_lines), encoding="utf-8"
        )


def main(args):
    current_path = pathlib.Path(__file__).parent.absolute()
    patches = os.path.join(current_path, "patches")
    vendor = os.path.join(current_path, "vendor")
    crab_dir = os.path.join(current_path, "crab", "crates")
    vendor_artifacts = current_path / "vendor_artifacts"
    scripts_dir = current_path / "scripts"
    license_shorthand_file = os.path.join(vendor_artifacts, "licenses_used.txt")
    destroyed_crates_file = vendor_artifacts / "destroyed_crates.txt"

    # First, actually run cargo vendor
    run_cargo_vendor(current_path)

    # Order matters here:
    # - Apply patches (also re-calculates checksums)
    # - Cleanup any owners files (otherwise, git check-in or checksums are
    #   unhappy)
    # - Destroy unused crates
    apply_patches(patches, vendor)
    cleanup_owners(vendor)
    destroyer = CrateDestroyer(current_path, vendor)
    destroyer.destroy_unused_crates(destroyed_crates_file)

    # Combine license file and check for any bad licenses
    lm = LicenseManager(current_path, vendor)
    lm.generate_license(
        args.skip_license_check, args.license_map, license_shorthand_file
    )

    # audit all packages
    rc = subprocess.run([scripts_dir / "cargo-vet.py"]).returncode
    if not rc:
        return

    sys.exit(
        "cargo-vet audit failed. Please audit new packages. See "
        "cargo-vet/README.md if you need help."
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description="Vendor packages properly")
    parser.add_argument(
        "--skip-license-check",
        "-s",
        help="Skip the license check on a specific package",
        action="append",
    )
    parser.add_argument("--license-map", help="Write license map to this file")
    args = parser.parse_args()

    main(args)
