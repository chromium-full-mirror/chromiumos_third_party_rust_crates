#!/usr/bin/env python3
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""Ensures `cargo vet` is installed, and runs it.

Moreover, this ensures that any `cargo vet` commands ignore our "destroyed"
crates, and is run within the proper directory. If you'd like to take power
into your own hands, passing `--debug` to this script will print the full
invocation of `cargo-vet` that this script issued, so you may customize it to
your liking.

If you'd like to see cargo-vet's top-level help, try `./run_cargo_vet.py --
--help`.

Please note that after this script is run once (which ensures you're using the
expected version of cargo vet), running `cargo vet` from ../projects should
_generally_ work for interactive use. As referenced above, this script adds
custom exemptions for destroyed crates, so tread lightly if you're using e.g.,
`cargo vet suggest` instead of ./cargo-vet.py suggest`.
"""

import argparse
import logging
import os
from pathlib import Path
import shlex
import subprocess
import sys
from typing import List, Tuple

import cargo


def fetch_destroyed_crates(rust_crates: Path) -> List[Tuple[str, str]]:
    """Reads the list of destroyed crates from vendor.py."""
    destroyed_crates = rust_crates / "vendor_artifacts" / "destroyed_crates.txt"
    with destroyed_crates.open(encoding="utf-8") as f:
        results = []
        for line in f:
            no_comment = line.split("#")[0].strip()
            if not no_comment:
                continue
            pieces = no_comment.split(" ", 1)
            if len(pieces) != 2:
                raise ValueError(
                    "Broken destroyed_crates file; no version delim "
                    f"in {line}"
                )
            name, version = pieces
            results.append((name, version))
    return results


def build_cargo_vet_exclude_expr(
    destroyed_crates: List[Tuple[str, str]],
) -> str:
    """Builds the --filter-graph expression for `destroyed_crates`."""
    package_clauses = (
        f"all(name({x}),version({y}))" for x, y in destroyed_crates
    )
    joined_package_clauses = ",".join(package_clauses)
    return f"exclude(any({joined_package_clauses}))"


def run_cargo_vet(
    rust_crates: Path, destroyed_crates: List[Tuple[str, str]], args: List[str]
) -> int:
    """Runs `cargo vet` with the given args."""
    projects_dir = rust_crates / "projects"

    vet_cmd = ["cargo", "vet"]
    vet_cmd += args
    vet_cmd.append(
        f"--filter-graph={build_cargo_vet_exclude_expr(destroyed_crates)}"
    )
    logging.debug(
        "In %s, running `%s`...",
        projects_dir,
        " ".join(shlex.quote(x) for x in vet_cmd),
    )
    try:
        return subprocess.call(vet_cmd, cwd=projects_dir)
    except KeyboardInterrupt:
        # 130 is the exit status of a program terminated by SIGINT. Don't print
        # a stack trace, since the user probably doesn't care.
        sys.exit(130)


# Instructions on how to generate a `cargo-vet` tarball:
#   1. `git clone` the cargo-vet repo here:
#       https://github.com/mozilla/cargo-vet
#   2. `checkout` the tag you're interested in, e.g.,
#      `git checkout cargo-audit/v0.17.4`
#   3. `rm -rf .git` in the repo.
#   4. tweak the version number in Cargo.toml to
#      include `+cros`, so we always autosync to the hermetic ChromeOS
#      version.
#   5. `cargo vendor`, and follow the instructions that it prints out RE "To
#      use vendored sources, ...".
#   6. `cargo build --offline --locked && rm -rf target`, to ensure it builds.
#   7. `tar cf cargo-vet-${version}.tar.bz2 cargo-vet \
#           --use-compress-program="bzip2 -9"`
#      in the parent of your `cargo-vet` directory.
#   8. Upload to gs://; don't forget the `-a public-read`.
def ensure_cargo_vet_is_installed():
    """Ensures that `cargo-vet` is installed."""
    want_version = "v0.10.2+cros"
    cargo.ensure_cargo_utility_is_installed(
        utility_name="cargo-vet",
        # b/372723132: cargo-vet's tags contain a leading "v", but the version
        # reported by the CLI doesn't.
        want_version=want_version.lstrip("v"),
        gs_path=f"gs://chromeos-localmirror/distfiles/cargo-vet-{want_version}.tar.bz2",
        sha256="3a92893854536dcf2cfbe6fede0e324125b90926a598e66f8fd51a323df8e2bc",
        build_subdir=Path("cargo-vet"),
    )


def main(argv: List[str]) -> int:
    rust_crates = Path(__file__).resolve().parent.parent

    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--debug", action="store_true", help="Enable debug logging."
    )
    parser.add_argument("cargo_vet_args", nargs=argparse.REMAINDER)
    opts = parser.parse_args(argv)

    logging.basicConfig(
        format=">> %(asctime)s: %(levelname)s: %(filename)s:%(lineno)d: "
        "%(message)s",
        level=logging.DEBUG if opts.debug else logging.INFO,
    )

    cargo.ensure_cargo_bin_is_in_path()
    ensure_cargo_vet_is_installed()
    destroyed_crates = fetch_destroyed_crates(rust_crates)
    cargo_vet_args = opts.cargo_vet_args
    if cargo_vet_args == ["help"] or cargo_vet_args == ["--", "--help"]:
        cargo_vet_args = ["--help"]
    return run_cargo_vet(rust_crates, destroyed_crates, cargo_vet_args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
