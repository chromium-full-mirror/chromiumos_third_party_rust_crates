#![allow(dead_code)]
//! to run tests, to prevent multithreading bugs, you have to run:
//! sudo RUST_TEST_TASKS=1 RUST_TEST_THREADS=1 cargo  test

use ext2::Ext2;
// use ext2::Ext2Filesystem;
// use std::fs::{File, OpenOptions};
use nix::sys::stat::Mode;
use nix::unistd::mkdir;
use nix::Result as NixResult;
use std::{
    fs::{File, OpenOptions},
    io::Read,
    io::Result,
    io::Write,
    process::Command,
};

pub const DD_PATHNAME: &str = "/usr/bin/dd";
pub const MKFS_EXT2_PATHNAME: &str = "/usr/sbin/mkfs.ext2";
pub const FUSE2FS_PATHNAME: &str = "/usr/sbin/fuse2fs"; // set user_allow_other into /etc/fuse.conf
pub const FUSERMOUNT_PATHNAME: &str = "/usr/bin/fusermount";

pub const DISK_NAME: &str = "disk";
pub const DISK_MOUNTED_NAME: &str = "disk_mounted/";

pub const DIRECT_MAX_SIZE: usize = 12 * 1024;
pub const SINGLY_MAX_SIZE: usize = DIRECT_MAX_SIZE + (1024 / 4) * 1024;
pub const DOUBLY_MAX_SIZE: usize = SINGLY_MAX_SIZE + (1024 / 4) * (1024 / 4) * 1024;

pub fn exec_shell(cmd: &str, args: &[&str]) -> bool {
    let exit_code = Command::new(cmd).args(args).status().unwrap();
    if !exit_code.success() {
        eprintln!("command failed: {}", cmd);
    }
    exit_code.success()
}

pub fn create_disk(size: usize) {
    exec_shell("sync", &[]);
    exec_shell("mkdir", &["-p", DISK_MOUNTED_NAME]);
    exec_shell(FUSERMOUNT_PATHNAME, &["-u", DISK_MOUNTED_NAME]);
    exec_shell(
        DD_PATHNAME,
        &[
            "if=/dev/zero",
            &format!("of={}", DISK_NAME),
            "bs=1024",
            &format!("count={}", size / 1024),
        ],
    );
    exec_shell(
        MKFS_EXT2_PATHNAME,
        &[
            "-E",
            &format!(
                "root_owner={}:{}",
                nix::unistd::geteuid(),
                nix::unistd::getegid()
            ),
            DISK_NAME,
        ],
    );
    exec_shell("sync", &[]);
}

pub fn mount_disk() {
    exec_shell("sync", &[]);
    exec_shell(FUSE2FS_PATHNAME, &[DISK_NAME, DISK_MOUNTED_NAME]);
    exec_shell("sync", &[]);
}

pub fn umount_disk() {
    exec_shell("sync", &[]);
    while !exec_shell(FUSERMOUNT_PATHNAME, &["-u", DISK_MOUNTED_NAME]) {
        eprintln!("Retry...");
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    exec_shell("sync", &[]);
}

pub fn create_dir(filename: &str) -> NixResult<()> {
    mkdir(
        format!("{}{}", DISK_MOUNTED_NAME, filename).as_str(),
        Mode::S_IRWXU,
    )
}

pub fn create_file(filename: &str, content: &[u8]) -> Result<usize> {
    let mut f = OpenOptions::new()
        .write(true)
        .create(true)
        .open(format!("{}{}", DISK_MOUNTED_NAME, filename))
        .expect("open filesystem failed");
    f.write(content)
}

pub fn read_file(filename: &str, content: &mut Vec<u8>) -> Result<usize> {
    let mut f = OpenOptions::new()
        .read(true)
        .open(format!("{}{}", DISK_MOUNTED_NAME, filename))
        .expect("open filesystem failed");
    f.read_to_end(content)
}

pub fn new_ext2_instance<T>() -> Ext2<File> {
    let f = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(DISK_NAME)
        .expect("open filesystem failed");
    Ext2::new(f).unwrap()
}
