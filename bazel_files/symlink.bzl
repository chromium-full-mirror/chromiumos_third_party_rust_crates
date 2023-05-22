# Copyright 2023 The ChromiumOS Authors
# Use of this source code is governed by a BSD-style license that can be
# found in the LICENSE file.

def _symlink_impl(ctx):
    out = ctx.actions.declare_file(ctx.label.name)
    ctx.actions.symlink(output = out, target_file = ctx.file.actual)

    return [DefaultInfo(files = depset([out]))]

symlink = rule(
    implementation = _symlink_impl,
    attrs = dict(actual = attr.label(allow_single_file = True, mandatory = True)),
)
