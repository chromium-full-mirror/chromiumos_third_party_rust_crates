mod tests_common;

use std::fs::File;

use tests_common::*;

use ext2::OpenOptions;

#[test]
fn open() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21".as_bytes()).unwrap();
    umount_disk();

    let ext2 = new_ext2_instance::<File>();

    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .open("/bananes/toto.txt", ext2);
    let _r = dbg!(f);
}
