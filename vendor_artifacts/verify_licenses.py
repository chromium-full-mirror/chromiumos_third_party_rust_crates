#!/usr/bin/env python3
# Copyright 2022 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""Verifies license correctness.

If the contents of `--license-file` do not match with `--expected-licenses`,
this prints an informative error. This is used by the third-party-crates-src
ebuild.
"""

import argparse
from pathlib import Path
import sys


def get_parser():
    """Creates a parser for cmdline arguments."""
    parser = argparse.ArgumentParser(
        description=__doc__,
        formatter_class=argparse.RawDescriptionHelpFormatter,
    )
    parser.add_argument(
        "--license-file",
        type=Path,
        help="Path to the file containing actual licenses. Comments start "
        " with #. One line per license name. Blank lines are ignored.",
    )
    parser.add_argument(
        "--expected-licenses",
        help="String containing expected space-separated license names.",
    )
    return parser


def main(argv):
    """Main function."""
    opts = get_parser().parse_args(argv)

    license_translations = {"BSD-3": "BSD"}

    license_file_contents = opts.license_file.read_text(encoding="utf-8")
    license_file_lines = (
        x.split("#")[0].strip() for x in license_file_contents.splitlines()
    )
    required_licenses = {
        license_translations.get(x, x) for x in license_file_lines if x
    }

    listed_licenses = set(opts.expected_licenses.split())
    if required_licenses == listed_licenses:
        return

    required_but_unlisted = required_licenses - listed_licenses
    if required_but_unlisted:
        print(
            "The following licenses are not mentioned in "
            f"EXPECTED_LICENSES: {sorted(required_but_unlisted)}"
        )

    listed_but_not_required = listed_licenses - required_licenses
    if listed_but_not_required:
        print(
            "The following licenses are mentioned in EXPECTED_LICENSES, "
            f"but are not required: {sorted(listed_but_not_required)}"
        )

    sys.exit("Listed licenses differ from required licenses.")


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
