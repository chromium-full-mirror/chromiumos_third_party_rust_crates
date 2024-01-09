mod tests_common;

use tests_common::*;

use ext2::OpenOptions;

use std::fs::File;
use std::io::Write;

#[test]
fn write_long() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    umount_disk();

    let ext2 = new_ext2_instance::<File>();

    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .open("/bananes/vector.img", ext2)
        .unwrap();
    let b: Box<[u8; 1024 * 128]> = Box::new([42; 1024 * 128]);
    let size = f.write(&*b).unwrap();
    assert_eq!(size, (&*b).len());

    mount_disk();
    let mut v = Vec::new();
    let size = read_file("/bananes/vector.img", &mut v).unwrap();
    umount_disk();

    assert_eq!(size, v.len());
    assert_eq!(&v, &*b);
}
