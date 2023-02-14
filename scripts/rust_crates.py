#!/usr/bin/env python3
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""General utilities for `rust_crates`."""

from pathlib import Path
import shutil
import subprocess
import sys


def die_if_not_running_in_chroot():
    """Exit with an error if this script is not being run within the chroot."""
    if not Path("/etc/cros_chroot_version").exists():
        sys.exit("This script can only be run within the chroot.")


def emerge_toml_if_unavailable():
    """`emerge`s the toml module if it is not available."""
    try:
        import toml

        return
    except ImportError:
        pass

    print("dev-python/toml isn't available; autoinstalling...")
    subprocess.run(
        [
            "sudo",
            "emerge",
            "-g",
            "dev-python/toml",
        ],
        check=True,
    )
