#!/bin/bash -eux
# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.
#
# This crate has executable md and ini files. Remove the x bit.
chmod -x *.md tests/test.ini
