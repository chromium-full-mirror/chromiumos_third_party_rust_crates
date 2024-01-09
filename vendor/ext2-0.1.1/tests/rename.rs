mod tests_common;
use tests_common::*;

use nix::errno::Errno;
use nix::sys::stat::stat;

use std::fs::File;
use std::io::ErrorKind;

#[test]
fn rename() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_dir("fraises").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    ext2.rename("/bananes/toto.txt", "/tata.txt").unwrap();

    mount_disk();
    assert_eq!(
        stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    assert_ne!(
        stat(format!("{}/tata.txt", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    ext2.rename("/bananes/", "/poires/").unwrap();

    mount_disk();
    assert_eq!(
        stat(format!("{}/bananes/", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    assert_ne!(
        stat(format!("{}/poires/", DISK_MOUNTED_NAME).as_str()),
        Err(Errno::ENOENT)
    );
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    assert_eq!(
        ext2.rename("/nodir", "/t1.txt").map_err(|e| e.kind()),
        Err(ErrorKind::NotFound)
    );
    assert_eq!(
        ext2.rename("/", "/tata.txt").map_err(|e| e.kind()),
        Err(ErrorKind::Unsupported)
    );
    assert_eq!(
        ext2.rename("/poires/", "/fraises/").map_err(|e| e.kind()),
        Err(ErrorKind::AlreadyExists)
    );
}
