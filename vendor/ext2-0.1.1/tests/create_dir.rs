mod tests_common;

use std::fs::File;
use std::io::ErrorKind;

use nix::dir::Entry;
use nix::dir::Type::*;
use tests_common::*;

#[test]
fn create_dir() {
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

    // common errors
    match ext2.create_dir("/").map_err(|e| e.kind()) {
        Err(ErrorKind::AlreadyExists) => {}
        _ => panic!("Must be AlreadyExists"),
    };
    match ext2.create_dir("/bananes/../poires").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2.create_dir("/bananes/./poires").map_err(|e| e.kind()) {
        Err(ErrorKind::AlreadyExists) => {}
        _ => panic!("Must be AlreadyExists"),
    };
    match ext2
        .create_dir("/bananesddddd/poires")
        .map_err(|e| e.kind())
    {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    match ext2.create_dir("/bananes/poires").map_err(|e| e.kind()) {
        Err(ErrorKind::AlreadyExists) => {}
        _ => panic!("Must be AlreadyExists"),
    };
    mount_disk();
    umount_disk();
}
