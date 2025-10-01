#!/bin/bash -eu
# Copyright 2025 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

# The crate includes private key files used for tests. Remove those files to
# avoid triggering warnings about private keys being checked in.
crate_name=$(basename "$(dirname "${BASH_SOURCE[0]}")")
private_keys=("tests/data/"*".pem")

die() {
  local error_msg="$@"
  echo "ERROR: ${error_msg}"
  exit 1
}

echo "Deleting test private keys for ${crate_name}: ${private_keys[@]}"

rm "${private_keys[@]}" || die "Failed to delete files ${private_keys[@]}"

echo "Done deleting test private keys for ${crate_name}"
