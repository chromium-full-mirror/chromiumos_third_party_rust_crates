mod tests_common;
use std::{fs::File, io::ErrorKind};

use nix::sys::stat::stat;
use tests_common::*;

#[test]
fn stats() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let ext2 = new_ext2_instance::<File>();
    let s1 = ext2.stat("/bananes/toto.txt").unwrap();
    let s2 = ext2.stat("/bananes/").unwrap();
    let s3 = ext2.stat("/").unwrap();

    mount_disk();
    let t1 = stat(format!("{}/bananes/toto.txt", DISK_MOUNTED_NAME).as_str()).unwrap();
    let t2 = stat(format!("{}/bananes/", DISK_MOUNTED_NAME).as_str()).unwrap();
    let t3 = stat(format!("{}/", DISK_MOUNTED_NAME).as_str()).unwrap();
    umount_disk();

    test(s1, t1);
    test(s2, t2);
    test(s3, t3);

    fn test(s: libc::stat, t: libc::stat) {
        // assert_eq!(s.st_dev, t.st_dev);
        assert_eq!(s.st_ino, t.st_ino);
        assert_eq!(s.st_nlink, t.st_nlink);
        assert_eq!(s.st_mode, t.st_mode);
        assert_eq!(s.st_uid, t.st_uid);
        assert_eq!(s.st_gid, t.st_gid);
        // assert_eq!(s.st_rdev, t.st_rdev);
        assert_eq!(s.st_size, t.st_size);
        assert_eq!(s.st_blksize, t.st_blksize);
        // assert_eq!(s.st_blocks, t.st_blocks); // TODO Fix Difference
        assert_eq!(s.st_atime, t.st_atime);
        assert_eq!(s.st_mtime, t.st_mtime);
        assert_eq!(s.st_ctime, t.st_ctime);
        assert_eq!(s.st_atime_nsec, t.st_atime_nsec);
        assert_eq!(s.st_mtime_nsec, t.st_mtime_nsec);
        assert_eq!(s.st_ctime_nsec, t.st_ctime_nsec);
    }

    // common errors
    match ext2.stat("/unknown_dir").map_err(|e| e.kind()) {
        Err(ErrorKind::NotFound) => {}
        _ => panic!("Must be NotFound"),
    };
    match ext2.stat("/bananes/.././test").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
    match ext2.stat("bananes/toto.txt").map_err(|e| e.kind()) {
        Err(ErrorKind::Unsupported) => {}
        _ => panic!("Must be Unsupported"),
    };
}
