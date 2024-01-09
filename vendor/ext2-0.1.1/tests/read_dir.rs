#![cfg_attr(feature = "unstable", feature(io_error_more))]

mod tests_common;
use std::{fs::File, io::ErrorKind};

use nix::dir::Entry;
use nix::dir::Type::*;
use tests_common::*;

#[test]
fn read_dir() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let ext2 = new_ext2_instance::<File>();

    let check = |entry: &Entry, name: &str, r#type| -> bool {
        entry.file_name().to_str().unwrap() == name && entry.file_type() == Some(r#type)
    };

    // test root content
    let v = ext2.read_dir("/").unwrap();
    assert_ne!(v.iter().find(|p| check(p, "..", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, ".", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "lost+found", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "bananes", Directory)), None);

    assert_eq!(v.iter().find(|p| check(p, "xxx", Directory)), None);

    // test subdir content
    let v = ext2.read_dir("/bananes").unwrap();
    assert_ne!(v.iter().find(|p| check(p, "..", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, ".", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "toto.txt", File)), None);

    assert_eq!(v.iter().find(|p| check(p, "lost+found", Directory)), None);
    assert_eq!(v.iter().find(|p| check(p, "bananes", Directory)), None);

    // common errors
    match ext2.read_dir("/./bananes").map_err(|e| e.kind()) {
        Ok(_) => {}
        _ => panic!("Must be Ok"),
    };

    match &ext2.read_dir("/bananes/../bananes").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _out => panic!("Must be Unsupported"),
    };

    match &ext2.read_dir("/toto.xtxt").map_err(|e| e.kind()) {
        Err(ErrorKind::NotFound) => {}
        _out => panic!("Must be NotFound"),
    };

    match ext2.read_dir("/bananes/toto.txt").map_err(|e| e.kind()) {
        #[cfg(unstable)]
        Err(ErrorKind::NotADirectory) => {}
        #[cfg(not(unstable))]
        Err(ErrorKind::PermissionDenied) => {}
        _out => panic!("Must be Not a directory"),
    };
}
