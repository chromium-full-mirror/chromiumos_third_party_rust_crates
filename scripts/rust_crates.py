#!/usr/bin/env python3
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""General utilities for `rust_crates`."""

import dataclasses
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile

# This sets up the paths for chromite
import chromite_init

from chromite.lib import commandline
from chromite.lib import constants
from chromite.lib import cros_build_lib


def run_inside_chroot():
    """Restart the command inside the chroot if it isn't already."""
    try:
        commandline.RunInsideChroot()
    except commandline.ChrootRequiredError as e:
        sys.exit(
            cros_build_lib.run(
                e.cmd,
                check=False,
                enter_chroot=True,
                chroot_args=e.chroot_args,
                extra_env=e.extra_env,
                cwd=constants.SOURCE_ROOT
                / "src"
                / "third_party"
                / "rust_crates",
            ).returncode
        )


def die_if_running_as_root():
    """Exit with an error if this script is being run as root."""
    if os.geteuid() == 0:
        sys.exit("Don't run this as root and/or with sudo.")


def install_tomli_and_reexec_if_unavailable():
    """Installs the tomli{,_w} modules & restarts the program if necessary."""
    try:
        import tomli
        import tomli_w

        return
    except ImportError:
        pass

    try:
        import pip

        has_pip = True
    except ImportError:
        has_pip = False

    print("toml module isn't available; autoinstalling...")
    if not has_pip:
        print("ensuring pip is available.")
        # The pip version is bundled with python, so we don't need to worry
        # about version checks there.
        subprocess.run(
            [
                "python",
                "-m",
                "ensurepip",
            ],
            check=True,
        )

    tempdir = Path(tempfile.mkdtemp(prefix="pip-install-tomli"))

    tomli_wheels = (
        (
            "tomli-2.0.1-py3-none-any.whl.bz2",
            "218bc14a227cce9a113d6e67e9713e9ce5272325b37373329e673f9f217832e7",
        ),
        (
            "tomli_w-1.0.0-py3-none-any.whl.bz2",
            "e00d7fc56ce3072cc348e4583642848e41f7c795630f004027c4f778f729576c",
        ),
    )
    for bz2_whl_name, sha in tomli_wheels:
        download_gs_file_to(
            target_path=tempdir / bz2_whl_name,
            gs_path=f"gs://chromeos-localmirror/distfiles/{bz2_whl_name}",
            sha256=sha,
        )
        subprocess.run(["bzip2", "-d", bz2_whl_name], check=True, cwd=tempdir)
        subprocess.run(
            [
                "python",
                "-m",
                "pip",
                "--disable-pip-version-check",
                "install",
                "--user",
                tempdir / Path(bz2_whl_name).stem,
            ],
            check=True,
        )

    # Only clean this up on successful installs. It's useful for debugging, and
    # lands in /tmp anyway.
    shutil.rmtree(tempdir)

    # Now restart from scratch. This is necessary if pip had to be installed,
    # since pip adds superpowers to Python's module loader. It's not super
    # clear how to dynamically add those, and this function is intended to be
    # called _very_ early, so re-exec'ing isn't much of an issue.
    if not has_pip:
        os.execvp(sys.argv[0], sys.argv)


@dataclasses.dataclass(frozen=True)
class GitHeadAncestry:
    """Describes information about theancestry of HEAD."""

    # Our upstream branch. May be inferred.
    upstream_branch: str
    # Whether our upstream branch was inferred.
    is_upstream_assumed: bool
    # True if HEAD is a direct ancestor of `upstream_branch`.
    is_upstream_an_ancestor: bool


def collect_git_head_ancestry(rust_crates: Path) -> GitHeadAncestry:
    """Populates a GitHeadAncestry object."""
    upstream = subprocess.run(
        [
            "git",
            "rev-parse",
            "@{u}",
        ],
        cwd=rust_crates,
        check=False,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        encoding="utf-8",
    )

    if upstream.returncode:
        # Assume we're at detached-head. This is technically wrong for
        # branches; we highlight that later.
        upstream = "cros/main"
        upstream_assumed = True
    else:
        upstream = upstream.stdout.strip()
        upstream_assumed = False
        assert upstream, "`git rev-parse @{u}` unexpectedly gave no output"

    merge_base = subprocess.run(
        [
            "git",
            "merge-base",
            "HEAD",
            upstream,
        ],
        cwd=rust_crates,
        check=True,
        stdout=subprocess.PIPE,
        encoding="utf-8",
    )
    merge_base = merge_base.stdout.strip()

    upstream_sha = subprocess.run(
        [
            "git",
            "rev-parse",
            upstream,
        ],
        cwd=rust_crates,
        check=True,
        stdout=subprocess.PIPE,
        encoding="utf-8",
    )
    upstream_sha = upstream_sha.stdout.strip()

    return GitHeadAncestry(
        upstream_branch=upstream,
        is_upstream_assumed=upstream_assumed,
        is_upstream_an_ancestor=upstream_sha == merge_base,
    )


def exit_if_head_is_not_up_to_date(rust_crates: Path, disable_check_flag: str):
    """Runs `sys.exit` with helpful messages if HEAD isn't up-to-date."""
    ancestry = collect_git_head_ancestry(rust_crates)
    if ancestry.is_upstream_an_ancestor:
        return

    exit_message_lines = [
        f"Error: HEAD is not a child of {ancestry.upstream_branch}.",
        "This may lead to this script giving incorrect results.",
        "Please run `repo sync`, or if you'd like to just update this repo, "
        f"`git rebase {ancestry.upstream_branch}`. Afterward, rerun this.",
    ]
    if ancestry.is_upstream_assumed:
        exit_message_lines.append("")
        exit_message_lines.append(
            "Note: upstream branch assumed to be "
            f"{ancestry.upstream_branch} for lack of a better option."
        )

    exit_message_lines.append("")
    exit_message_lines.append(
        f"Note: pass {disable_check_flag} to disable this check."
    )
    sys.exit("\n".join(exit_message_lines))


def download_gs_file_to(
    target_path: Path,
    gs_path: str,
    sha256: str,
):
    """Downloads a file from gs://, checking its SHA against `sha256`.

    Args:
        target_path: the path to download the file to.
        gs_path: the gs:// path to download from.
        sha256: the expected sha256 of the file, in hex.
    """
    print(f"Downloading {gs_path}...")
    subprocess.run(
        ["gsutil", "-q", "cp", gs_path, target_path],
        check=True,
        stdin=subprocess.DEVNULL,
    )

    print("Verifying SHA...")
    with target_path.open("rb") as f:
        got_sha256 = hashlib.sha256()
        for block in iter(lambda: f.read(32 * 1024), b""):
            got_sha256.update(block)
        got_sha256 = got_sha256.hexdigest()
        if got_sha256 != sha256:
            raise ValueError(
                f"SHA256 mismatch for {gs_path}. Got {got_sha256}, want "
                f"{sha256}"
            )
