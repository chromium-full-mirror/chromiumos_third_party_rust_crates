mod tests_common;

use tests_common::*;

use ext2::OpenOptions;

use std::fs::File;
use std::io::Write;
use std::str;

#[test]
fn write_simple() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    umount_disk();

    let ext2 = new_ext2_instance::<File>();

    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .open("/bananes/toto.txt", ext2)
        .unwrap();
    let s: &str = "toto21toto42";
    let size = f.write(s.as_bytes()).unwrap();
    assert_eq!(size, s.as_bytes().len());

    mount_disk();
    let mut v = Vec::new();
    let size = read_file("bananes/toto.txt", &mut v).unwrap();
    umount_disk();

    assert_eq!(size, v.len());
    assert_eq!(&v, s.as_bytes());
}
