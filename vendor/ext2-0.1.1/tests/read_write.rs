mod tests_common;

use rand::random;
use tests_common::*;

use ext2::OpenOptions;

use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
const NB_TESTS: usize = 2;

#[test]
fn read_write() {
    fn read_write_of_size(size: usize) {
        //create a disk of size of the file + a little space for metadata
        let filename = format!("/simple_file-{}", size);

        let mut ext2 = new_ext2_instance::<File>();
        let mut f = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&filename, ext2.clone())
            .unwrap();

        // CREATE with the std
        // mount_disk();
        // {
        //     let filename_mounted = DISK_MOUNTED_NAME.to_owned() + "/" + &filename;
        //     File::create(&filename_mounted).expect(&format!(
        //         "open on mouted filesystem failed {}",
        //         &filename_mounted
        //     ));
        // }
        // umount_disk();

        /* WRITE with the ext2 */
        let v: Vec<u8> = (0..(size)).map(|_| random::<u8>()).collect();
        let count = f.write(&v).unwrap();
        assert_eq!(count, size);

        /* READ with the ext2 */
        let mut buf = vec![42; size];
        f.seek(SeekFrom::Start(0)).unwrap();
        let count = f.read(&mut buf).unwrap();
        assert_eq!(count, size);

        assert_eq!(buf[..], v[..]);

        ext2.remove_file(filename).unwrap();
    }

    let sizes: Vec<usize> = (0..NB_TESTS)
        .map(|_| random::<usize>() % DOUBLY_MAX_SIZE)
        .collect();
    let sum: usize = sizes.iter().sum();
    create_disk(sum + 1024 * 1024 * 10);
    for size in sizes {
        read_write_of_size(size);
    }
}
