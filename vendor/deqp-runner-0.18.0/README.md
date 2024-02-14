## deqp-runner

This is a tool for parallelizing a VK-GL-CTS or dEQP run across the
CPUs in a system, collecting the results, and comparing them to the
baseline for the device.  It is geared toward driver developers and CI
systems looking for fast turnaround of dEQP results (it is not a valid
way to produce a VK/GL conformance result).

### Installing

```
apt-get install cargo
cargo install deqp-runner
```

### Running

Example run:

```
deqp-runner run \
      --deqp ~/src/VK-GL-CTS-build/modules/gles2/deqp-gles2 \
      --caselist ~/src/VK-GL-CTS/external/openglcts/data/mustpass/gles/aosp_mustpass/3.2.6.x/gles2-master.txt \
      --output new-run  \
      --baseline last-run/failures.csv \
      --testlog-to-xml ~/src/VK-GL-CTS-build/executor/testlog-to-xml \
      -- \
      --deqp-surface-width=256 --deqp-surface-height=256 \
      --deqp-surface-type=pbuffer \
      --deqp-gl-config-name=rgba8888d24s8ms0 \
      --deqp-visibility=hidden
```

You will get summary output as tests are run and results collected, with failing
tests logged to stderr with references of where to find the associated caselists
(and the full QPA file next to them).  The failing tests will also be extracted
to testname.xml which can be combined the VK-GL-CTS's testlog-stylesheet and
served through a simple http server (such as cargo install simple-http-server;
simple-http-server outputdir/) for visualizing the failure.

Unexpectedly failing tests will be automatically rerun to detect flaky
tests, which will be treated as success of the pipeline (this is
useful in CI, to keep rare and uncategorized flakes from impacting
other developers).  Additionally, you can pass a --flakes argument to
a list of regexes of known flaky tests, to reduce the chance of
spurious failures in a CI pipeline.

### Deqp Suite support

The VK-GL-CTS comes with multiple dEQP binaries (deqp-gles2, deqp-gles3,
deqp-egl, deqp-vk, glcts), and the basic "run" syntax only supports running a
single binary.  The suite subcommand lets you specify a toml file with most of
the same arguments as the run command as keys, such as this snippet for part of
a setup for the softpipe driver.

```
[[deqp]]
deqp = "~/src/VK-GL-CTS-build/modules/gles2/deqp-gles2"
caselists = ["~/src/VK-GL-CTS-build/external/openglcts/modules/gl_cts/data/mustpass/gles/aosp_mustpass/3.2.6.x/gles2-master.txt"]
baseline = "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-fails.txt"
skips = ["~/src/mesa/.gitlab-ci/deqp-all-skips.txt", "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-skips.txt"]
deqp_args = [
    "--deqp-surface-width=256", "--deqp-surface-height=256",
    "--deqp-surface-type=pbuffer", "--deqp-gl-config-name=rgba8888d24s8ms0", "--deqp-visibility=hidden"
]

[[deqp]]
deqp = "~/src/VK-GL-CTS-build/modules/gles3/deqp-gles3"
caselists = ["~/src/VK-GL-CTS-build/external/openglcts/modules/gl_cts/data/mustpass/gles/aosp_mustpass/3.2.6.x/gles3-master.txt"]
baseline = "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-fails.txt"
skips = ["~/src/mesa/.gitlab-ci/deqp-all-skips.txt", "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-skips.txt"]
deqp_args = [
    "--deqp-surface-width=256", "--deqp-surface-height=256",
    "--deqp-surface-type=pbuffer", "--deqp-gl-config-name=rgba8888d24s8ms0", "--deqp-visibility=hidden"
]

[[deqp]]
deqp = "~/src/VK-GL-CTS-build/modules/gles2/deqp-gles2"
caselists = ["~/src/VK-GL-CTS-build/external/openglcts/modules/gl_cts/data/mustpass/gles/aosp_mustpass/3.2.6.x/gles2-master.txt"]
baseline = "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-fails.txt"
skips = ["~/src/mesa/.gitlab-ci/deqp-all-skips.txt", "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-skips.txt"]
deqp_args = [
    "--deqp-surface-width=256", "--deqp-surface-height=256",
    "--deqp-surface-type=pbuffer", "--deqp-gl-config-name=rgba8888d24s8ms0", "--deqp-visibility=hidden"
]
# You can set environment vars as part of a deqp run.
  [deqp.env]
  NIR_VALIDATE = "1"

# Specifying a prefix lets you include multiple invocations of deqp tests with debug
# flags that might produce different results.  The prefix is used for comparing
# to the baseline and for matching skips and flakes.
prefix = "validation-"
deqp = "~/src/VK-GL-CTS-build/modules/gles2/deqp-gles2"
caselists = ["~/src/VK-GL-CTS-build/external/openglcts/modules/gl_cts/data/mustpass/gles/aosp_mustpass/3.2.6.x/gles2-master.txt"]
baseline = "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-fails.txt"
skips = ["~/src/mesa/.gitlab-ci/deqp-all-skips.txt", "~/src/mesa/src/gallium/drivers/softpipe/ci/deqp-softpipe-skips.txt"]
deqp_args = [
    "--deqp-surface-width=256", "--deqp-surface-height=256",
    "--deqp-surface-type=pbuffer", "--deqp-gl-config-name=rgba8888d24s8ms0", "--deqp-visibility=hidden"
]

# You can run piglit as part of your test suite, too.
[[piglit]]
piglit_folder = "/home/anholt/src/piglit"
profile = "quick"
baseline = "/home/anholt/src/mesa/src/gallium/drivers/softpipe/ci/softpipe-fails.txt"
skips = ["/home/anholt/src/mesa/.gitlab-ci/all-skips.txt", "/home/anholt/src/mesa/src/gallium/drivers/softpipe/ci/softpipe-skips.txt"]
process_isolation = true
  [piglit.env]
  PIGLIT_PLATFORM = "gbm"

```

The testlog-to-xml is specified on the command line, and fractions can be
specified on both the command line (for sharding between runners) and on the
deqp definition (for only running some fraction of the test suite).

### igt-gpu-tools support

deqp-runner now contains experimental support for running IGT tests.  An
example run might look like:

```
igt-runner run \
      --output new-run \
      --igt-folder ~/src/igt/build/tests \
      --caselist caselist.txt \
      --baseline last-run/failures.csv \
      --skips igt-skips.txt
```

### skqp support

deqp-runner now contains experimental support for running skqp tests.  An
example run might look like:

```
skqp-runner run \
      --output new-run \
      --skqp ~/src/skia/out/amd64-Debug/skqp \
      --skqp-assets ~/src/skia/platform_tools/android/apps/skqp/src/main/assets/ \
      --skqp-backends gl,gles \
      --baseline last-run/failures.csv \
      --skips skips.txt
```

Similar to the other test suites, you will get summary output as tests are run
and results collected, with failing tests logged to stderr with references of
where to find the associated caselists (and the output folder generated by skqp
with html results next to them).

### Piglit support

deqp-runner now contains experimental support for running piglit tests.  An
example run might look like:

```
piglit-runner run \
      --output new-run \
      --piglit-folder ~/src/piglit \
      --profile quick \
      --baseline last-run/failures.csv \
      --skips piglit-soft-skips.txt \
      --process-isolation
```

It has some known issues:

- No piglit subtest result tracking (includes shader_tests in from no_isolation)
- No fast skipping based on extensions enabled

### gtest support

deqp-runner now contains experimental support for running gtest tests.  An
example run might look like:

```
gtest-runner run \
      --output new-run \
      --bin test-binary \
      --baseline last-run/failures.csv
```

It has some known issues:

- gtest is slow at handling the long gtest_filter arguments we create.
  (https://github.com/google/googletest/issues/3614)

### Cross building for your embedded device

Add the following to ~/.cargo/config:

```
[target.armv7-unknown-linux-gnueabihf]
linker = "arm-linux-gnueabihf-gcc"

[target.aarch64-unknown-linux-gnu]
linker = "aarch64-linux-gnu-gcc"
```

And set up the new toolchain and build:

```
rustup target add aarch64-unknown-linux-gnu
cargo build --release --target aarch64-unknown-linux-gnu deqp-runner
scp target/aarch64-unknown-linux-gnu/release/deqp-runner device:bin/
```

### License

Licensed under the MIT license
   ([LICENSE](LICENSE) or http://opensource.org/licenses/MIT)
