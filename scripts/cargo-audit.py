#!/usr/bin/env python3
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""Runs `cargo-audit` for rust_crates, and outputs results.

Exits unsuccessfully if a problem happens, or if advisories are identified.

This is automatically run on a regular basis by the ChromeOS toolchain team.
Please contact chromeos-toolchain@google.com with any questions.

This differs from a simple invocation of `cargo-audit` in that:
    - it filters complaints about packages which ChromeOS doesn't care about,
    - it serves as an accessible source of truth about any advisories we're
      ignoring (since a failure of this script turns into a bug for
      chromeos-toolchain@), and
    - it ignores advisories for crates which we've explicitly emptied.
"""

import argparse
import dataclasses
import enum
import hashlib
import json
import logging
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
from typing import Any, Iterable, List, Set

import cargo


# The CPU arches that we care about.
SUPPORTED_ARCHES = (
    "aarch64",
    "arm",
    "x86",
    "x86_64",
)

# A list of advisory IDs that we ignore. If one is added, please add a comment
# explaining why.
IGNORED_ADVISORIES = ()

EMPTY_CRATE_CONTENTS = (
    b'compile_error!("This crate cannot be built for this configuration.");'
)


@dataclasses.dataclass(frozen=True, eq=True, order=True)
class Crate:
    """Uniquely identifies a crate."""

    name: str
    version: str


class AdvisoryType(enum.IntEnum):
    """The kinds of advisories/warnings an Advisory can have."""

    ADVISORY = 1  # An advisory with a RUSTSEC advisory ID.
    UNSOUND = 2  # A warning noting that the given crate is unsound.
    UNMAINTAINED = 3  # A warning noting that the given crate is unmaintained.
    YANKED = 4  # A warning noting that the given crate has been yanked.


@dataclasses.dataclass(frozen=True, eq=True, order=True)
class Advisory:
    """Defines advisories and warnings for a given crate.

    A union of problems that should be surfaced to the user. These must be
    insertable into a set.
    """

    crate: Crate
    advisory_type: AdvisoryType
    id: str = ""  # Only set for DepAdvisory


def parse_cargo_audit_json(output: str) -> List[Advisory]:
    """Parses the JSON output of `cargo audit` into advisories."""

    def parse_crate(package_object: Any) -> Crate:
        return Crate(
            name=package_object["name"],
            version=package_object["version"],
        )

    audit_results = json.loads(output)

    advisories = []
    # Even if the output is empty, this path should exist in audit_results.
    for vuln in audit_results["vulnerabilities"]["list"]:
        advisories.append(
            Advisory(
                id=vuln["advisory"]["id"],
                crate=parse_crate(vuln["package"]),
                advisory_type=AdvisoryType.ADVISORY,
            )
        )

    # "warnings" exists even if empty.
    warnings = audit_results["warnings"]
    for unmaintained in warnings.pop("unmaintained", ()):
        advisories.append(
            Advisory(
                crate=parse_crate(unmaintained["package"]),
                advisory_type=AdvisoryType.UNMAINTAINED,
            )
        )

    for yanked in warnings.pop("yanked", ()):
        advisories.append(
            Advisory(
                crate=parse_crate(yanked["package"]),
                advisory_type=AdvisoryType.YANKED,
            )
        )

    for unsound in warnings.pop("unsound", ()):
        advisories.append(
            Advisory(
                crate=parse_crate(unsound["package"]),
                advisory_type=AdvisoryType.UNSOUND,
            )
        )

    if warnings:
        raise ValueError(
            f"Unexpected warnings key(s): {sorted(warnings.keys())}"
        )

    return advisories


def run_cargo_audit(
    rust_crates: Path, arches: Iterable[str], ignored_advisories: Iterable[str]
) -> Set[Advisory]:
    """Runs cargo-audit on the given arch list."""
    projects_dir = rust_crates / "projects"
    advisories = set()
    base_cmd = ["cargo", "audit", "--json", "--target-os=linux"]
    base_cmd.extend(f"--ignore={x}" for x in ignored_advisories)
    for i, arch in enumerate(SUPPORTED_ARCHES):
        cmd = base_cmd.copy()
        cmd.append(f"--target-arch={arch}")
        # Only fetch on the first iteration; fetching afterward may lead to
        # inconsistent results, and has no realistic value.
        if i:
            cmd.append("--no-fetch")

        logging.debug("Running `cargo audit` for arch %s", arch)
        result = subprocess.run(
            cmd,
            check=False,
            cwd=projects_dir,
            stdout=subprocess.PIPE,
            encoding="utf-8",
        )
        # So cargo-audit's returncode isn't super useful. A returncode of 1
        # means either an error happened, or there were vulns found. Scan
        # stdout to differentiate.
        stdout = result.stdout.strip()
        if not stdout.endswith("}"):
            result.check_returncode()
        arch_advisories = parse_cargo_audit_json(stdout)
        logging.info(
            "%d advisories found for arch %s", len(arch_advisories), arch
        )
        advisories.update(arch_advisories)

    return advisories


def determine_empty_crates(rust_crates: Path) -> Set[Crate]:
    """Returns a list of crates that `vendor.py` emptied out."""
    empty = set()
    # Empty crates have a lib.rs containing a single line with a
    # `compile_error!` in it. This `compile_error!` may or may not be
    # `// commented out`, depending on `vendor.py`'s configuration. Match that
    # here.
    for crate in (rust_crates / "vendor").iterdir():
        try:
            with (crate / "src" / "lib.rs").open("rb") as f:
                first_line = f.readline()
                if EMPTY_CRATE_CONTENTS not in first_line:
                    continue
                if f.readline():
                    continue
        except OSError:
            # Any reasonable OSError here is enough of a signal that this isn't
            # an empty crate.
            continue

        # Crate directories are formatted as f"{crate_name}-{version}".
        # crate_name may have instances of '-' in it, but '.' isn't allowed.
        # `version` matches the regex /^\d+\./, so find the sep by looking
        # before the first '.' in the directory name.
        crate_name = crate.name
        first_dot = crate_name.index(".")
        dash_before_dot = crate_name.rindex("-", 0, first_dot)
        empty.add(
            Crate(
                name=crate_name[:dash_before_dot],
                version=crate_name[dash_before_dot + 1 :],
            )
        )
    return empty


# Instructions on how to generate a `cargo audit` tarball:
#   1. `git clone` the rustsec repo here:
#       https://github.com/rustsec/rustsec
#   2. `checkout` the tag you're interested in, e.g.,
#      `git checkout cargo-audit/v0.17.4`
#   3. `rm -rf .git` in the repo.
#   4. tweak the version number in rustsec/cargo-audit/Cargo.toml to
#      include `+cros`, so we always autosync to the hermetic ChromeOS
#      version.
#   5. `cargo vendor` in rustsec/cargo-audit, and follow the instructions
#      that it prints out RE "To use vendored sources, ...".
#   6. `cargo build --offline --locked && rm -rf ../target` in
#      rustsec/cargo-audit, to ensure it builds.
#   7. `tar cf rustsec-${version}.tar.bz2 rustsec \
#           --use-compress-program="bzip2 -9"`
#      in the parent of your `rustsec` directory.
#   8. Upload to gs://; don't forget the `-a public-read`.
def ensure_cargo_audit_is_installed():
    """Ensures that `cargo-audit` is installed."""
    want_version = "0.22.2+cros"
    cargo.ensure_cargo_utility_is_installed(
        utility_name="cargo-audit",
        want_version=want_version,
        gs_path=f"gs://chromeos-localmirror/distfiles/rustsec-{want_version}.tar.bz2",
        sha256="63fea57e2c7f9480b658377ebb6f468891b6112705c00c1e44abeb66ea6526b1",
        build_subdir=Path("rustsec") / "cargo-audit",
    )


def main(argv: List[str]):
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--debug",
        action="store_true",
        help="Enable debug logging.",
    )
    parser.add_argument(
        "--rust-crates",
        type=Path,
        help="Path to rust_crates.",
        default=Path(__file__).resolve().parent.parent,
    )
    parser.add_argument(
        "--skip-install",
        help="Do not version-check or try to install rust-crates.",
        action="store_true",
    )
    opts = parser.parse_args(argv)

    logging.basicConfig(
        format=">> %(asctime)s: %(levelname)s: %(filename)s:%(lineno)d: "
        "%(message)s",
        level=logging.DEBUG if opts.debug else logging.INFO,
    )

    cargo.ensure_cargo_bin_is_in_path()
    if not opts.skip_install:
        ensure_cargo_audit_is_installed()

    rust_crates = opts.rust_crates
    advisories = run_cargo_audit(
        rust_crates,
        SUPPORTED_ARCHES,
        IGNORED_ADVISORIES,
    )
    empty_crates = determine_empty_crates(rust_crates)
    logging.info("Discovered %d empty crates", len(empty_crates))
    complaint_lines = []
    # Sort by prioritizing the crate name+version, but sort on `x` itself if we
    # have multiple issues.
    for advisory in sorted(advisories):
        crate = advisory.crate
        if crate in empty_crates:
            logging.info(
                "Ignoring advisory for empty crate %s: %s", crate, advisory
            )
            continue

        if advisory.advisory_type == AdvisoryType.ADVISORY:
            if (
                advisory.id == "RUSTSEC-2024-0437"
                and advisory.crate.name == "protobuf"
                and advisory.crate.version.startswith("2.")
            ):
                logging.info(
                    "Ignoring advisory %r for %r version %r: "
                    "b/401976739#comment4",
                    advisory.id,
                    advisory.crate.name,
                    advisory.crate.version,
                )
                continue
            complaint_lines.append(
                f"crate {crate.name!r} version {crate.version!r} has advisory "
                f"https://rustsec.org/advisories/{advisory.id}.html"
            )
        elif advisory.advisory_type == AdvisoryType.YANKED:
            complaint_lines.append(
                f"crate {crate.name!r} version {crate.version!r} "
                "has been yanked"
            )
        elif advisory.advisory_type == AdvisoryType.UNSOUND:
            if (
                advisory.crate.name == "inventory"
                and advisory.crate.version.startswith("0.1")
            ):
                logging.info(
                    "Ignoring unsoundness advisory for %r version %r: "
                    "b/318697301",
                    advisory.crate.name,
                    advisory.crate.version,
                )
                continue
            if advisory.crate.name == "rand" and (
                advisory.crate.version.startswith("0.7.")
                or advisory.crate.version.startswith("0.8.")
            ):
                logging.info(
                    "Ignoring unsoundness advisory for %r version %r; "
                    "we have a local patch: b/502125873",
                    advisory.crate.name,
                    advisory.crate.version,
                )
                continue

            if (
                advisory.crate.name == "memmap2"
                and advisory.crate.version == "0.8.0"
            ):
                logging.info(
                    "Ignoring unsoundness advisory for %r version %r; "
                    "we have a local patch: RUSTSEC-2026-0186",
                    advisory.crate.name,
                    advisory.crate.version,
                )
                continue

            complaint_lines.append(
                f"crate {crate.name!r} version {crate.version!r} is unsound"
            )
        elif advisory.advisory_type == AdvisoryType.UNMAINTAINED:
            logging.info(
                "Ignoring unmaintained advisory for %s", advisory.crate
            )
        else:
            raise ValueError(
                f"Unexpected advisory type: {advisory.advisory_type}"
            )

    if not complaint_lines:
        logging.info("No fatal advisories found. Exiting cleanly.")
        return

    # Add two leading newlines to visually separate this from log statements.
    print("\n\n** Fatal advisories found:")
    for complaint in complaint_lines:
        print(f"  - {complaint}")

    sys.exit("one or more fatal advisories detected")


if __name__ == "__main__":
    main(sys.argv[1:])
