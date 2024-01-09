#![cfg_attr(feature = "unstable", feature(io_error_more))]

mod tests_common;

use tests_common::*;

use ext2::OpenOptions;

use std::fs::File;
use std::io::{ErrorKind, Read, Seek, SeekFrom};
use std::str;

#[test]
fn read() {
    create_disk(1024 * 1024);
    mount_disk();
    create_dir("bananes").unwrap();
    create_file("bananes/toto.txt", "toto21toto42".as_bytes()).unwrap();
    umount_disk();

    let ext2 = new_ext2_instance::<File>();

    let mut f = OpenOptions::new()
        .read(true)
        .open("/bananes/toto.txt", ext2.clone())
        .unwrap();

    let mut buf = [0; 6];
    let size = f.read(&mut buf).unwrap();
    assert_eq!("toto21", str::from_utf8(&buf[..size]).unwrap());

    let mut buf = [0; 6];
    let size = f.read(&mut buf).unwrap();
    assert_eq!("toto42", str::from_utf8(&buf[..size]).unwrap());

    let mut buf = [0; 6];
    let size = f.read(&mut buf).unwrap();
    assert_eq!("", str::from_utf8(&buf[..size]).unwrap());

    match OpenOptions::new()
        .read(true)
        .open("/bananes/", ext2.clone())
        .map_err(|e| e.kind())
    {
        #[cfg(unstable)]
        Err(ErrorKind::IsADirectory) => {}
        #[cfg(not(unstable))]
        Err(ErrorKind::PermissionDenied) => {}
        _ => panic!("Must be Unsupported"),
    };

    let mut f = OpenOptions::new()
        .read(true)
        .open("/bananes/toto.txt", ext2.clone())
        .unwrap();
    f.seek(SeekFrom::End(3)).unwrap();
    let mut buf = [0; 6];
    let size = f.read(&mut buf).unwrap();
    assert_eq!("o42", str::from_utf8(&buf[..size]).unwrap());

    f.seek(SeekFrom::Start(3)).unwrap();
    let mut buf = [0; 6];
    let size = f.read(&mut buf).unwrap();
    assert_eq!("o21tot", str::from_utf8(&buf[..size]).unwrap());

    f.seek(SeekFrom::Current(-3)).unwrap();
    let mut buf = [0; 6];
    let size = f.read(&mut buf).unwrap();
    assert_eq!("toto42", str::from_utf8(&buf[..size]).unwrap());
}
