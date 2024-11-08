extern crate pkg_config;

fn main() {
    #[cfg(target_os = "windows")]
    {
        println!("cargo:rustc-link-lib=libslirp");
    }
    #[cfg(not(target_os = "windows"))]
    {
        pkg_config::find_library("slirp >= 4.2.0").unwrap();
    }
}
