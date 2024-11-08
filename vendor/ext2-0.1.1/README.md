Ext2.rs
==============

This crate was created with the purpose of being able to read and write on ext2 partitions, whether they are in block device or in the form of a simple disk image file.

It does not require any Unix kernel module to function. Originally, it was designed to open ext2 images from within a Docker container without the need for privileged mode.

This crate covers basic system calls:

- **open :** Open a file.
- **read_dir :** Returns a Vector over the entries within a directory.
- **create_dir :** Creates a new, empty directory at the provided path.
- **remove_dir :** Removes an empty directory.
- **chmod :** Change the file permission bits of the specified file.
- **chown :** Change the ownership of the file at path to be owned by the specified owner (user) and group.
- **stat :** This function returns information about a file.
- **remove_file :** Removes a file from the filesystem.
- **utime :** Change the access and modification times of a file.
- **rename :** Rename a file or directory to a new name, it cannot replace the original file if to already exists.
- **link :** Make a new name for a file. It is also called “hard-link”.
- **symlink :** Make a new name for a file. It is symbolic links.

Additionally, the crate also has its own implementation of OpenOptions.

*You have full permissions on the files, and all specified paths must be absolute. Currently, this crate only works on Unix-like operating systems.*

**Disclaimer :** This crate is still in its early stages and should be used with caution on existing system partitions.

this module contains a ext2 driver
see [osdev](https://wiki.osdev.org/Ext2)

**FUTURE ROAD MAP**
- Fix some incoherencies
- Use std::io::Error instead of IOError
- Use ErrorKind instead of errno
- Made compilation on others platforms than UNIX
- no-std
- Cache of directory entries
- Change current directory
- Set Permissions

# Getting Started

Add the following dependency to your Cargo manifest...

```toml
[dependencies]
ext2 = "0.1"

# or
ext2 = { version = "0.1" }
```

# Example

First, create a new ext2 disk image named 'disk.img':

```console
dd if=/dev/zero of=disk.img bs=1024 count=1024
mkfs.ext2 disk.img
```

Open, the disk image, and write to the '/foo.raw' file :

```rust
use ext2;

let f = std::fs::OpenOptions::new() // OpenOptions from std
    .read(true)
    .write(true)
    .open("disk.img")
    .expect("open filesystem failed");
let ext2 = ext2::Ext2::new(f).unwrap();

let mut f = ext2::OpenOptions::new() // OpenOptions from ext2
    .write(true)
    .create(true)
    .open("/foo.raw", ext2)
    .unwrap();
let b: Box<[u8; 1024 * 128]> = Box::new([42; 1024 * 128]);
let size = f.write(&*b).unwrap(); // write the box content to file
```

## License

Licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any
additional terms or conditions.
