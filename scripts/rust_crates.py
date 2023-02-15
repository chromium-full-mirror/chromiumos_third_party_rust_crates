#!/usr/bin/env python3
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

"""General utilities for `rust_crates`."""

import dataclasses
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
