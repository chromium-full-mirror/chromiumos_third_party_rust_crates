#![cfg_attr(feature = "unstable", feature(io_error_more))]

mod tests_common;
use std::fs::File;
use std::io::ErrorKind;

use nix::errno::Errno;
use nix::sys::stat::stat;
use tests_common::*;

#[test]
fn remove_file() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    create_file("bananes/tata.txt", "toto21".as_bytes()).unwrap();
    create_file("titi.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();

    match ext2.remove_file("/bananes/toto.txt") {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };
    match ext2.remove_file("/bananes/tata.txt") {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };
    match ext2.remove_file("/titi.txt") {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };

    // common errors
    match ext2.remove_file("/unknown_dir").map_err(|e| e.kind()) {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    match ext2.remove_file("/bananes/.././test").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2.remove_file("bananes/toto.txt").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2.remove_file("/bananes/").map_err(|e| e.kind()) {
        #[cfg(unstable)]
        Err(ErrorKind::IsADirectory) => {}
        #[cfg(not(unstable))]
        Err(ErrorKind::PermissionDenied) => {}
        _ => panic!("Must be IsADirectory"),
    };
    match ext2.remove_file("/").map_err(|e| e.kind()) {
        #[cfg(unstable)]
        Err(ErrorKind::IsADirectory) => {}
        #[cfg(not(unstable))]
        Err(ErrorKind::PermissionDenied) => {}
        _ => panic!("Must be IsADirectory"),
    };

    mount_disk();
    assert_eq!(
        stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    assert_eq!(
        stat(format!("{}/bananes/tata.txt", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    assert_eq!(
        stat(format!("{}/titi.txt", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    umount_disk();
}
