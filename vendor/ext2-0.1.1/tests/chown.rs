mod tests_common;
use std::fs::File;
use std::io::ErrorKind;

use nix::sys::stat::stat;
use nix::unistd::{Gid, Uid};
use tests_common::*;

#[test]
fn chown() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();

    match ext2.chown("/bananes/toto.txt", Uid::from_raw(0), Gid::from_raw(0)) {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };
    match ext2.chown("/bananes/", Uid::from_raw(0), Gid::from_raw(0)) {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };
    match ext2.chown("/", Uid::from_raw(0), Gid::from_raw(0)) {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };

    // common errors
    match ext2
        .chown("/unknown_dir", Uid::from_raw(0), Gid::from_raw(0))
        .map_err(|e| e.kind())
    {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    match ext2
        .chown("/bananes/.././test", Uid::from_raw(0), Gid::from_raw(0))
        .map_err(|e| e.kind())
    {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2
        .chown("bananes/toto.txt", Uid::from_raw(0), Gid::from_raw(0))
        .map_err(|e| e.kind())
    {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };

    mount_disk();
    assert_eq!(
        stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_uid,
        0
    );
    assert_eq!(
        stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_gid,
        0
    );
    assert_eq!(
        stat(format!("{}/bananes/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_uid,
        0
    );
    assert_eq!(
        stat(format!("{}/bananes/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_gid,
        0
    );
    assert_eq!(
        stat(format!("{}/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_uid,
        0
    );
    assert_eq!(
        stat(format!("{}/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_gid,
        0
    );
    umount_disk();
}
