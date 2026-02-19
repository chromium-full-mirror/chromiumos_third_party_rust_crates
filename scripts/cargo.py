#!/usr/bin/env python3
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""Utilities to help interact with cargo."""

import argparse
import hashlib
import logging
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

import rust_crates


def ensure_cargo_bin_is_in_path():
    """Ensures that .cargo/bin is in $PATH for this process."""
    cargo_bin = str(Path.home() / ".cargo" / "bin")
    path = os.getenv("PATH", "")
    path_has_cargo_bin = path.endswith(cargo_bin) or cargo_bin + ":" in path
    if not path_has_cargo_bin:
        os.environ["PATH"] = cargo_bin + ":" + path


def ensure_cargo_utility_is_installed(
    utility_name: str,
    want_version: str,
    gs_path: str,
    sha256: str,
    build_subdir: Path,
):
    """Ensures that the given cargo utility is installed.

    Args:
        utility_name: the name of the given utility, e.g., cargo-audit.
        want_version: the version string that should be installed.
        gs_path: the gs:// path to download vendored sources for this from.
        sha256: the SHA to expect for the gs_path tarball.
        build_subdir: the subdirectory from which we should run `cargo
            install`. This is relative to the directory from which the source
            tarball is unpacked.
    """
    # Unfortunately, `cargo ${tool} --version` does not always print the
    # version for the values of ${tool} we care about. Query `cargo` instead.
    version = subprocess.run(
        ["cargo", "install", "--list"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        encoding="utf-8",
    )
    # Since we do local installations, cargo-install will list this as e.g.,
    # `{utility_name} v{want_version} ({tempdir_it_was_installed_in}):`.
    want_version_string = f"{utility_name} v{want_version} "
    has_version = any(
        x.startswith(want_version_string) for x in version.stdout.splitlines()
    )
    if has_version:
        return

    logging.info("Auto-installing %s version %s", utility_name, want_version)

    tempdir = Path(tempfile.mkdtemp(prefix="cargo-util-install"))
    logging.info(
        "Using %s as a tempdir. This will not be cleaned up on failures.",
        tempdir,
    )
    tbz2_name = "cargo-utility.tar.bz2"
    rust_crates.download_gs_file_to(
        target_path=tempdir / tbz2_name,
        gs_path=gs_path,
        sha256=sha256,
    )

    logging.info("Unpacking...")
    subprocess.run(
        ["tar", "xaf", tbz2_name],
        check=True,
        cwd=tempdir,
    )
    logging.info(
        "Building and installing %s (this may take a bit)...", utility_name
    )
    env = dict(os.environ)
    # cargo-audit fails to build due to a dependency complaining about old GCCs:
    # b/485820420. Prefer Clang since it's not impacted by the bug, and Clang
    # is better-supported in ChromeOS anyway.
    if shutil.which("clang"):
        env["CC"] = "clang"
        env["CXX"] = "clang++"
    else:
        logging.warning(
            "No Clang detected on $PATH; leaving default CC/CXX. "
            "This may lead to build failures due to "
            "https://gcc.gnu.org/bugzilla/show_bug.cgi?id=95189. Rerun in the "
            "chroot if so."
        )
    subprocess.run(
        ["cargo", "install", "--locked", "--offline", "--path=.", "--quiet"],
        check=True,
        cwd=tempdir / build_subdir,
        env=env,
    )
    logging.info("`%s` installed successfully.", utility_name)

    # Only clean this up on successful installs. It's useful for debugging, and
    # lands in /tmp anyway.
    shutil.rmtree(tempdir)
