mod tests_common;
use tests_common::*;

use nix::sys::stat::stat;

use std::fs::File;
use std::io::ErrorKind;

#[test]
fn symlink() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_dir("fraises").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    create_file("oldfile", "toto42".as_bytes()).unwrap();
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    ext2.symlink("bananes/toto.txt", "/tata.txt").unwrap();

    mount_disk();
    if let Err(_) = stat(format!("{}/tata.txt", DISK_MOUNTED_NAME).as_str()) {
        panic!("Must be OK");
    }
    umount_disk();

    let mut ext2 = new_ext2_instance::<File>();
    assert_eq!(
        ext2.symlink("nodir", "/t1.txt").map_err(|e| e.kind()),
        Ok(())
    );
    assert_eq!(
        ext2.symlink("./", "/tata2.txt").map_err(|e| e.kind()),
        Ok(())
    );
    assert_eq!(
        ext2.symlink("poires/", "/fraises/").map_err(|e| e.kind()),
        Err(ErrorKind::AlreadyExists)
    );
    assert_eq!(
        ext2.symlink("fraises/", "/fraises2/").map_err(|e| e.kind()),
        Ok(())
    );
}
