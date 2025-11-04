#!/bin/sh -eux
# Copyright 2025 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

# Remove test-only disk images. They're a bit big, and we don't use them.
rm -rf tests/fixtures
