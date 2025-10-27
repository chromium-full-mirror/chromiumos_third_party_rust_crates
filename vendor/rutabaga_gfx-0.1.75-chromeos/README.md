<div align="center">
  <img src="https://github.com/magma-gpu/rutabaga_gfx/raw/main/images/rutabaga_gfx_logo.png" alt="" width=320>
  <p><strong>The Rutabaga Policy on ChromeOS maintainence</strong></p>

[![License](https://img.shields.io/github/license/magma-gpu/rutabaga_gfx)](https://github.com/magma-gpu/rutabaga_gfx/blob/main/LICENSE)
[![Crates.io](https://img.shields.io/crates/d/rutabaga_gfx.svg)](https://crates.io/crates/rutabaga_gfx)
[![rustc 1.81.0](https://img.shields.io/badge/rust-1.81%2B-orange.svg)](https://img.shields.io/badge/rust-1.81%2B-orange.svg)
[![Documentation](https://docs.rs/rutabaga_gfx/badge.svg)](https://docs.rs/rutabaga_gfx)

</div>

Due to recent [strategic changes](https://www.theregister.com/2025/09/25/google_android_chromeos/),
feature work on ChromeOS GPU virtualization work has stopped and interest has gone to enabling
Android Desktop use cases. However, many ChromeOS devices will not migrate to Android Desktop, and
adhere to the [10-year auto-update policy](https://support.google.com/chrome/a/answer/6220366).

The crosvm team has chosen to continue updating crosvm in ChromeOS, and as a consequence, rutabaga
must be updated there too.

This presents a maintainence challenge to Rutabaga, since there exists the risk any random change
can subtlely break ChromeOS. In addition, ChromeOS has it's own way of doing graphics distinct from
standard Linux (minigbm over Mesa GBM, for example), and that impedes use of Rutabaga elsewhere. To
test some of the changes one might want to make, someone would need to procure a Chromebook and make
sure it doesn't break. And nobody -- include the ChromeOS team -- has time for this.

However, thanks to Rutabaga externalization, a solution is readily available. Taking inspiration
from Mesa3D's [Amber branch](https://docs.mesa3d.org/amber.html), we can _essentially_ freeze the
Rutabaga version used by ChromeOS via the **chromeos** branch. The only catch is we need to make
stub API changes to match the **main** branch. The procedure is described as follows:

1. Someone wants to introduce a new API in rutabaga **main**
2. A new API lands
3. A new release is cut (say, **v0.4.2**)
4. A change is landed in rutabaga **chromeos** that stubs out the new API and just returns success,
   or not supported
5. **v0.4.2** and **0.4.2-chromeos** released at the same time on crates.io
6. Upstream crosvm uses **v0.4.2**, ChromeOS crosvm uses **v0.4.2-chromeos**

This keeps ChromeOS stable, but always allows evolution of **main**.
