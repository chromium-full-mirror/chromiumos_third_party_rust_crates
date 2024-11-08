extern crate libslirp_sys as ffi;

use std::ffi::CStr;
use std::str;

#[test]
fn version() {
    let version =
        str::from_utf8(unsafe { CStr::from_ptr(::ffi::slirp_version_string()) }.to_bytes())
            .unwrap_or("");
    println!("version {}", version);
}
