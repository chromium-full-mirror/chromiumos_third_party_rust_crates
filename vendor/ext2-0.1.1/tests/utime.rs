mod tests_common;
use std::{fs::File, io::ErrorKind};

use nix::sys::stat::stat;
use tests_common::*;

#[test]
fn utime() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    ext2.utime(
        "/bananes/toto.txt",
        Some(&libc::utimbuf {
            actime: 42,
            modtime: 42,
        }),
    )
    .unwrap();

    mount_disk();
    let t1 = stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str()).unwrap();
    umount_disk();

    test(t1);
    fn test(t: libc::stat) {
        assert_eq!(t.st_atime, 42);
        assert_eq!(t.st_mtime, 42);
    }

    // common errors
    match ext2.utime("/unknown_dir", None).map_err(|e| e.kind()) {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    match ext2.utime("/bananes/.././test", None).map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
}
