mod tests_common;
use std::fs::File;
use std::io::ErrorKind;

use nix::sys::stat::stat;
use nix::sys::stat::Mode;
use tests_common::*;

#[test]
fn chmod() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    let mode = Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO;

    match ext2.chmod("/bananes/toto.txt", mode) {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };
    match ext2.chmod("/bananes/", mode) {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };
    match ext2.chmod("/", mode) {
        Ok(()) => {}
        _ => panic!("Must be done"),
    };

    // common errors
    match ext2.chmod("/unknown_dir", mode).map_err(|e| e.kind()) {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    match ext2.chmod("/bananes/.././test", mode).map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2.chmod("bananes/toto.txt", mode).map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };

    let mask = Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO;
    mount_disk();
    assert_eq!(
        stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_mode
            & mask.bits(),
        mode.bits()
    );
    assert_eq!(
        stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_mode
            & mask.bits(),
        mode.bits()
    );
    assert_eq!(
        stat(format!("{}/bananes/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_mode
            & mask.bits(),
        mode.bits()
    );
    assert_eq!(
        stat(format!("{}/bananes/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_mode
            & mask.bits(),
        mode.bits()
    );
    assert_eq!(
        stat(format!("{}/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_mode
            & mask.bits(),
        mode.bits()
    );
    assert_eq!(
        stat(format!("{}/", DISK_MOUNTED_NAME).as_str())
            .unwrap()
            .st_mode
            & mask.bits(),
        mode.bits()
    );
    umount_disk();
}
