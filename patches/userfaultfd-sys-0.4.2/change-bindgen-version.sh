#!/bin/bash -eux
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.
#
# Upgrade bindgen version to 0.64.0.
#
# TODO(b/278951478): Remove this script once we upstream this version upgrade.
#
# bindgen fails to parse <linux/userfaultfd.h> due to LLVM upgrade.
# https://github.com/rust-lang/rust-bindgen/pull/2319
#
# The fix for it is available from bindgen 0.62 while userfaultfd-sys uses
# 0.60.1. Apply patch to use the latest bindgen which is already installed
# in chromeos rust_crates.
sed -i s/'0.60.1'/'0.64.0'/ Cargo.toml
