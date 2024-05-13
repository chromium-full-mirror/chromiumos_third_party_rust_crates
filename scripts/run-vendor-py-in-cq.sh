#!/bin/bash -eu
# Copyright 2024 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

# SWARMING_TASK_ID is always set to a nonempty value on builders. It's mildly
# hacky to use it to detect "are we on a builder?" in presubmit checks, but
# apparently isn't unheard of.
if [[ -z "${SWARMING_TASK_ID:-}" ]]; then
  echo "Skipping vendor.py check - this isn't in the CQ."
  exit 0
fi

me="$(readlink -m "$0")"
my_dir="$(dirname "${me}")"

cd "${my_dir}"
if [[ ! -e "/etc/cros_chroot_version" ]]; then
  echo "Re-exec'ing in the chroot..."
  exec cros_sdk \
    --working-dir=. \
    "SWARMING_TASK_ID=${SWARMING_TASK_ID}" \
    -- \
    "./$(basename "${me}")" \
    "$@"
fi

# A worktree is necessary, since fullcheckout-presubmit runs all checks in
# parallel, and `vendor.py` treats the place it's run in as fully mutable.
tmpdir="$(mktemp --tmpdir -d rust-crates-cq-tmpdir-XXXXXX)"
clean_up_tmpdir() {
  echo "Cleaning up tempdir at ${tmpdir}"
  rm -rf "${tmpdir}" || :
  # Call `git worktree prune`, since the workdir has been removed.
  (cd "${my_dir}" && git worktree prune) || :
}
trap clean_up_tmpdir EXIT

echo "Establishing worktree in ${tmpdir}..."
git worktree add --detach --force "${tmpdir}/rust_crates"
cd "${tmpdir}/rust_crates"

echo "Running vendor.py..."
# Note that metallurgy bits are skipped, since those symlink into other repos.
# This check only cares about diffs in the current repo, since no diffs here
# imply no changes elsewhere.
#
# Also skip the version check, since we know that HEAD is the right thing to
# check.
if ! ./vendor.py --skip-version-check --skip-metallurgy; then
  echo >&2 "vendor.py failed; can't check for a diff"
  exit 1
fi
echo "vendor.py completed successfully."

# The vendor-in-progress stamp will remain, due to `--skip-metallurgy`. Since
# there's no intent to commit this (just checking diffs), forcibly removing is
# fine.
rm -f vendor_artifacts/vendor_script_in_progress

if [[ -z "$(git status --porcelain)" ]]; then
  echo "vendor.py introduced no diff - all seems OK."
  exit 0
fi

echo >&2
echo >&2
echo >&2 "!! vendor.py introduced a diff. 'git status' output:"
git >&2 --no-pager status

echo >&2
echo >&2
echo >&2 "!! 'git diff' output:"
git >&2 --no-pager diff

echo >&2
echo >&2
echo >&2
echo >&2 '!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!'
echo >&2 "Running vendor.py should not lead to a diff in 'rust_crates'."
echo >&2 "Please see the 'git diff' and 'git status' output above"
echo >&2 "to learn more about what diff was observed."
echo >&2 '!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!'
exit 1
