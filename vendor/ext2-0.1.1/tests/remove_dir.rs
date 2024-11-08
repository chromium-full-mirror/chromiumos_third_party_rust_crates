#![cfg_attr(feature = "unstable", feature(io_error_more))]

mod tests_common;
use std::{fs::File, io::ErrorKind};

use nix::dir::Entry;
use nix::dir::Type::*;
use tests_common::*;

#[test]
fn remove_dir() {
    create_disk(1024 * 1024);
    let mut ext2 = new_ext2_instance::<File>();

    ext2.create_dir("/bananes").unwrap();

    let check = |entry: &Entry, name: &str, r#type| -> bool {
        entry.file_name().to_str().unwrap() == name && entry.file_type() == Some(r#type)
    };
    let v = ext2.read_dir("/").unwrap();
    assert_ne!(v.iter().find(|p| check(p, "..", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, ".", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "lost+found", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "bananes", Directory)), None);

    ext2.create_dir("/bananes/poires").unwrap();

    let v = ext2.read_dir("/bananes/").unwrap();
    assert_ne!(v.iter().find(|p| check(p, "..", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, ".", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "poires", Directory)), None);

    match ext2.remove_dir("/bananes").map_err(|e| e.kind()) {
        #[cfg(unstable)]
        Err(ErrorKind::DirectoryNotEmpty) => {}
        #[cfg(not(unstable))]
        Err(ErrorKind::PermissionDenied) => {}
        _ => panic!("Must be DirectoryNotEmpty"),
    };

    ext2.remove_dir("/bananes/poires").unwrap();

    let v = ext2.read_dir("/bananes/").unwrap();
    assert_ne!(v.iter().find(|p| check(p, "..", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, ".", Directory)), None);

    assert_eq!(v.iter().find(|p| check(p, "poires", Directory)), None);

    ext2.remove_dir("/bananes").unwrap();

    let v = ext2.read_dir("/").unwrap();
    assert_ne!(v.iter().find(|p| check(p, "..", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, ".", Directory)), None);
    assert_ne!(v.iter().find(|p| check(p, "lost+found", Directory)), None);

    assert_eq!(v.iter().find(|p| check(p, "bananes", Directory)), None);

    ext2.create_dir("/bananes").unwrap();
    ext2.create_dir("/bananes/poires").unwrap();

    // common errors
    match ext2.remove_dir("/").map_err(|e| e.kind()) {
        #[cfg(unstable)]
        Err(ErrorKind::DirectoryNotEmpty) => {}
        #[cfg(not(unstable))]
        Err(ErrorKind::PermissionDenied) => {}
        _ => panic!("Must be DirectoryNotEmpty"),
    };
    match ext2.remove_dir("/bananes/../poires").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2.remove_dir("/bananes/./poires").map_err(|e| e.kind()) {
        Ok(()) => {}
        _ => panic!("Must be Ok"),
    };
    match ext2
        .remove_dir("/bananesddddd/poires")
        .map_err(|e| e.kind())
    {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    mount_disk();
    umount_disk();
}
