#!/bin/bash -eu
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.
#
# Checks for the vendor_script_in_progress stamp and errors if it exists. For
# more info on that, see the `InProgressStamp` class in vendor.py.

my_dir="$(dirname "$(readlink -m "$0")")"
cd "${my_dir}/.."
stamp="vendor_artifacts/vendor_script_in_progress"

if [[ ! -e "${stamp}" ]]; then
  exit
fi

echo "ERROR: vendor.py stamp at ${PWD}/${stamp} detected." >&2
echo "This means the most recent run of vendor.py did not" >&2
echo "terminate successfully. Please run vendor.py again and" >&2
echo "make sure it's not erroring out before uploading your" >&2
echo "change. See the top-level README if you need help." >&2
exit 1
