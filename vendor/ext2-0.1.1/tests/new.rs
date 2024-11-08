mod tests_common;
use std::fs::File;

use tests_common::*;

#[test]
fn open() {
    create_disk(1024 * 1024);
    let ext2 = new_ext2_instance::<File>();
    dbg!(ext2);
}
