use std::env;
use std::process::Command;
use std::str;

fn main() {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let out_dir = env::var_os("OUT_DIR").expect("OUT_DIR not set");

    // Test if VaList has 2 lifetimes.
    let test_code = r#"
        #![no_std]
        #![feature(c_variadic)]
        use core::ffi::VaList;
        fn test<'a, 'b>(args: VaList<'a, 'b>) {}
        fn main() {}
    "#;

    let mut cmd = Command::new(rustc);
    cmd.arg("--crate-type=lib");
    cmd.arg(format!("--out-dir={}", out_dir.to_str().unwrap()));
    cmd.arg("-");

    let mut child = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to execute rustc");

    {
        use std::io::Write;
        let mut stdin = child.stdin.take().expect("failed to open stdin");
        stdin.write_all(test_code.as_bytes()).expect("failed to write to stdin");
    }

    let output = child.wait_with_output().expect("failed to wait for rustc");

    if output.status.success() {
        println!("cargo:rustc-cfg=cros_va_list_2_lifetimes");
    }
}
