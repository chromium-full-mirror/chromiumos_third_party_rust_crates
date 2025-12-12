#!/bin/bash -eux
# Copyright 2022 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.
#
# This crate has executable md files. Remove the x bit.
find . -name "*.md" -exec chmod -x {} \;
