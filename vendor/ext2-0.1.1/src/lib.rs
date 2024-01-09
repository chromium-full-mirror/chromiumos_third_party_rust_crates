//! This crate was created with the purpose of being able to read and write on ext2 partitions, whether they are in block device or in the form of a simple disk image file.
//!
//! It does not require any Unix kernel module to function. Originally, it was designed to open ext2 images from within a Docker container without the need for privileged mode.
//!
//! This crate covers basic system calls:
//!
//! - **open :** Open a file.
//! - **read_dir :** Returns a Vector over the entries within a directory.
//! - **create_dir :** Creates a new, empty directory at the provided path.
//! - **remove_dir :** Removes an empty directory.
//! - **chmod :** Change the file permission bits of the specified file.
//! - **chown :** Change the ownership of the file at path to be owned by the specified owner (user) and group.
//! - **stat :** This function returns information about a file.
//! - **remove_file :** Removes a file from the filesystem.
//! - **utime :** Change the access and modification times of a file.
//! - **rename :** Rename a file or directory to a new name, it cannot replace the original file if to already exists.
//! - **link :** Make a new name for a file. It is also called “hard-link”.
//! - **symlink :** Make a new name for a file. It is symbolic links.
//!
//! Additionally, the crate also has its own implementation of OpenOptions.
//!
//! *You have full permissions on the files, and all specified paths must be absolute. Currently, this crate only works on Unix-like operating systems.*
//!
//! **Disclaimer :** This crate is still in its early stages and should be used with caution on existing system partitions.
//!
//! this module contains a ext2 driver
//! see [osdev](https://wiki.osdev.org/Ext2)
//!
//! **FUTURE ROAD MAP**
//! - Fix some incoherencies
//! - Use std::io::Error instead of IOError
//! - Use ErrorKind instead of errno
//! - Made compilation on others platforms than UNIX
//! - no-std
//! - Cache of directory entries
//! - Change current directory
//! - Set Permissions
#![deny(missing_docs)]
#![cfg_attr(feature = "unstable", feature(maybe_uninit_uninit_array))]
#![cfg_attr(feature = "unstable", feature(maybe_uninit_array_assume_init))]
#![cfg_attr(feature = "unstable", feature(io_error_more))]

mod inner;

use inner::{Ext2Filesystem, Inode};
type IoResult<T> = std::result::Result<T, Errno>;

use libc::{blksize_t, c_void, ino_t, off_t, time_t};
use nix::dir::Entry;
use nix::errno::Errno;
use nix::sys::stat::{Mode, SFlag};
use nix::unistd::{Gid, Uid};

use std::io::{Error, ErrorKind, Read, Result, Seek, SeekFrom, Write};
use std::mem::MaybeUninit;
use std::mem::{size_of, transmute};
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// This structure represents an entire ext2 filesystem.
#[derive(Debug)]
pub struct Ext2<T: Read + Seek + Write>(Arc<Mutex<Ext2Filesystem<T>>>);

impl<T> Clone for Ext2<T>
where
    T: Read + Seek + Write,
{
    fn clone(&self) -> Self {
        Ext2(self.0.clone())
    }
}

/// Invocation of a new FileSystem instance: Take anything that implements Read + Seek + Write.
/// ```rust,ignore
/// let f = std::fs::OpenOptions::new()
///     .read(true)
///     .write(true)
///     .open(MY_DISK_OBJECT)
///     .expect("open filesystem failed");
/// let ext2 = open_ext2_drive(f).unwrap();
/// ```
pub fn open_ext2_drive<T>(disk: T) -> Result<Ext2<T>>
where
    T: Read + Seek + Write,
{
    Ext2::new(disk)
}

impl<T> Ext2<T>
where
    T: Read + Seek + Write,
{
    /// Invocation of a new FileSystem instance: Take anything that implements Read + Seek + Write.
    /// ```rust,ignore
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = open_ext2_drive(f).unwrap();
    /// ```
    pub fn new(disk: T) -> Result<Self> {
        Ok(Self(Arc::new(Mutex::new(
            Ext2Filesystem::new(disk).map_err(|e| Error::from_raw_os_error(e as i32))?,
        ))))
    }

    /// Opens a file in write-only mode.
    ///
    /// This function will create a file if it does not exist,
    /// and will truncate it if it does.
    ///
    /// Depending on the platform, this function may fail if the
    /// full directory path does not exist.
    /// See the [`OpenOptions::open`] function for more details.
    pub fn create<P: AsRef<Path>>(&mut self, path: P) -> Result<File<T>> {
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path.as_ref(), self.clone())
    }

    /// Attempts to open a file in read-only mode.
    ///
    /// See the [`OpenOptions::open`] method for more details.
    ///
    /// # Errors
    ///
    /// This function will return an error if `path` does not already exist.
    /// Other errors may also be returned according to [`OpenOptions::open`].
    pub fn open<P: AsRef<Path>>(&mut self, path: P) -> Result<File<T>> {
        OpenOptions::new()
            .read(true)
            .open(path.as_ref(), self.clone())
    }

    /// Returns a Vector over the entries within a directory.
    ///
    /// The collection will yield instances of <code>[std::io::Result]<[Entry]></code>.
    /// New errors may be encountered after an iterator is initially constructed.
    /// ```rust,ignore
    /// let v = ext2.read_dir("/").unwrap();
    /// for entry in v {
    ///     dbg!(entry);
    /// }
    /// ```
    pub fn read_dir<P: AsRef<Path>>(&self, path: P) -> Result<Vec<Entry>> {
        let path = get_path(&path)?;
        let ext2 = self.0.lock().unwrap();
        let iter = _lookup_directory(&ext2, path)?;

        let type_field = ext2.get_superblock().directory_entry_contain_type_field();
        use inner::DirectoryEntryType::*;
        Ok(iter
            .enumerate()
            .map(move |(i, entry)| {
                let dirent = libc::dirent {
                    d_ino: entry.directory.header.inode as u64,
                    d_off: i as i64,
                    d_reclen: size_of::<libc::dirent>() as u16,
                    d_type: match type_field {
                        true => match entry.directory.header.type_indicator {
                            RegularFile => libc::DT_REG,
                            Directory => libc::DT_DIR,
                            CharacterDevice => libc::DT_CHR,
                            BlockDevice => libc::DT_BLK,
                            Fifo => libc::DT_FIFO,
                            Socket => libc::DT_SOCK,
                            SymbolicLink => libc::DT_LNK,
                        },
                        false => libc::DT_UNKNOWN,
                    },
                    d_name: entry.directory.filename.0,
                };
                unsafe { transmute::<_, Entry>(dirent) } // Inner field of type dirent is not public.
            })
            .collect())
    }

    /// Creates a new, empty directory at the provided path.
    /// ```rust,ignore
    /// ext2.create_dir("/bananes").unwrap();
    /// ```
    pub fn create_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = get_path(&path)?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let parent = path.parent().ok_or_else(|| ErrorKind::AlreadyExists)?;
        let filename: &str = path.file_name().unwrap().to_str().unwrap();
        let mut ext2 = self.0.lock().unwrap();
        let iter = _lookup_directory(&ext2, &parent)?;
        let parent = iter.fold(Ok(None), |res, entry| {
            if entry.directory.filename == filename.try_into().unwrap() {
                return Err(ErrorKind::AlreadyExists);
            }
            res.map(|opt| {
                opt.or({
                    if unsafe { entry.directory.get_filename() == "." } {
                        Some(entry)
                    } else {
                        None
                    }
                })
            })
        })?;
        let parent_inode_nbr = parent.unwrap().directory.header.inode;
        ext2.create_dir(
            parent_inode_nbr,
            filename,
            timestamp as u32,
            def_mode() | Mode::S_IXUSR | Mode::S_IXOTH | Mode::S_IXGRP,
            (
                nix::unistd::geteuid().as_raw(),
                nix::unistd::getegid().as_raw(),
            ),
        )?;
        Ok(())
    }

    /// Removes an empty directory.
    /// ```rust,ignore
    /// ext2.remove_dir("/bananes").unwrap();
    /// ```
    pub fn remove_dir<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = get_path(&path)?;
        let mut ext2 = self.0.lock().unwrap();
        let iter = _lookup_directory(&ext2, path)?;
        let parent = iter.enumerate().fold(Ok(None), |res, (idx, entry)| {
            if idx > 1 {
                #[cfg(unstable)]
                return Err(ErrorKind::DirectoryNotEmpty);
                #[cfg(not(unstable))]
                return Err(ErrorKind::PermissionDenied);
            }
            res.map(|opt| {
                opt.or({
                    if unsafe { entry.directory.get_filename() == ".." } {
                        Some(entry)
                    } else {
                        None
                    }
                })
            })
        })?;
        match path.file_name() {
            Some(filename) => Ok(ext2.rmdir(
                parent.unwrap().directory.get_inode(),
                filename.to_str().unwrap(),
            )?),
            None => Err(ErrorKind::AlreadyExists.into()),
        }
    }

    /// Change the file permission bits of the specified file.
    /// ```rust,ignore
    /// let mode = Mode::S_IRWXU | Mode::S_IRWXG | Mode::S_IRWXO;
    /// ext2.chmod("/bananes/toto.txt", mode).unwrap();
    /// ```
    pub fn chmod<P: AsRef<Path>>(&mut self, path: P, mode: Mode) -> Result<()> {
        let path = get_path(&path)?;
        let mut ext2 = self.0.lock().unwrap();

        match _find_entry(&ext2, path)? {
            Some(entry) => Ok(ext2.chmod(entry.directory.get_inode(), mode)?),
            None => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Change the ownership of the file at `path` to be owned by the specified
    /// `owner` (user) and `group` (see
    /// [chown(2)](https://pubs.opengroup.org/onlinepubs/9699919799/functions/chown.html)).
    /// ```rust,ignore
    /// ext2.chown("/bananes/toto.txt", Uid::from_raw(0), Gid::from_raw(0)).unwrap();
    /// ```
    pub fn chown<P: AsRef<Path>>(&mut self, path: P, owner: Uid, group: Gid) -> Result<()> {
        let path = get_path(&path)?;
        let mut ext2 = self.0.lock().unwrap();

        match _find_entry(&ext2, path)? {
            Some(entry) => {
                Ok(ext2.chown(entry.directory.get_inode(), owner.into(), group.into())?)
            }
            None => Err(ErrorKind::NotFound.into()),
        }
    }

    /// This function returns information about a file,
    /// ```rust,ignore
    /// let s1 = ext2.stat("/bananes/toto.txt").unwrap();
    /// ```
    pub fn stat<P: AsRef<Path>>(&self, path: P) -> Result<libc::stat> {
        let path = get_path(&path)?;
        let ext2 = self.0.lock().unwrap();

        match _find_entry(&ext2, path)? {
            Some(entry) => Ok(_stat(&ext2, entry.directory.get_inode(), entry.inode)?),
            None => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Removes a file from the filesystem.
    ///
    /// # Platform-specific behavior
    ///
    /// This function currently corresponds to the `unlink` function on Unix
    /// and the `DeleteFile` function on Windows.
    /// Note that, this may change in the future.
    /// ```rust,ignore
    /// ext2.remove_file("/bananes/toto.txt").unwrap();
    /// ```
    pub fn remove_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = get_path(&path)?;
        #[cfg(unstable)]
        path.parent().ok_or_else(|| ErrorKind::IsADirectory)?;
        #[cfg(not(unstable))]
        path.parent().ok_or_else(|| ErrorKind::PermissionDenied)?;
        let mut ext2 = self.0.lock().unwrap();

        let parent = _find_entry(&ext2, path.parent().unwrap())?;
        let parent_inode_nbr = parent.unwrap().directory.header.inode;
        Ok(ext2.unlink(
            parent_inode_nbr,
            path.file_name().unwrap().to_str().unwrap(),
            true,
        )?)
    }

    /// Change the access and modification times of a file.
    /// ```rust,ignore
    /// ext2.utime("/bananes/toto.txt", Some(&libc::utimbuf {
    ///     actime: 42,
    ///     modtime: 42,
    /// })).unwrap();
    /// ```
    pub fn utime<P: AsRef<Path>>(&mut self, path: P, time: Option<&libc::utimbuf>) -> Result<()> {
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let path = get_path(&path)?;
        let mut ext2 = self.0.lock().unwrap();
        match _find_entry(&ext2, path)? {
            Some(entry) => Ok(ext2.utime(entry.directory.get_inode(), time, timestamp as u32)?),
            None => Err(ErrorKind::NotFound.into()),
        }
    }

    /// Rename a file or directory to a new name, it cannot replace the original file if
    /// `to` already exists.
    /// ```rust,ignore
    /// ext2.rename("/bananes/toto.txt", "/tata.txt").unwrap();
    /// ```
    pub fn rename<P: AsRef<Path>>(&mut self, path: P, new_path: P) -> Result<()> {
        let path = get_path(&path)?;
        let new_path = get_path(&new_path)?;
        match (path.parent(), new_path.parent()) {
            (Some(parent), Some(new_parent)) => {
                let mut ext2 = self.0.lock().unwrap();
                if let Ok(Some(_)) = _find_entry(&ext2, new_path) {
                    return Err(ErrorKind::AlreadyExists.into());
                }
                let child = _find_entry(&ext2, parent)?;
                match child {
                    Some(child) => {
                        let new_parent = _find_entry(&ext2, new_parent)?;
                        Ok(ext2.rename(
                            child.directory.get_inode(),
                            path.file_name().unwrap().to_str().unwrap(),
                            new_parent.unwrap().directory.get_inode(),
                            new_path.file_name().unwrap().to_str().unwrap(),
                        )?)
                    }
                    None => Err(ErrorKind::NotFound.into()),
                }
            }
            _ => Err(ErrorKind::Unsupported.into()),
        }
    }

    /// Make a new name for a file. It is also called "hard-link".
    /// ```rust,ignore
    /// ext2.link("/bananes/toto.txt", "/tata.txt").unwrap();
    /// ```
    pub fn link<P: AsRef<Path>>(&mut self, target_path: P, link_path: P) -> Result<()> {
        let target_path = get_path(&target_path)?;
        let link_path = get_path(&link_path)?;
        match link_path.parent() {
            Some(link_parent) => {
                let mut ext2 = self.0.lock().unwrap();
                if let Ok(Some(_)) = _find_entry(&ext2, link_path) {
                    return Err(ErrorKind::AlreadyExists.into());
                }
                let target_entry = _find_entry(&ext2, target_path)?;
                match target_entry {
                    Some(target_entry) => {
                        let parent_link = _find_entry(&ext2, link_parent)?;
                        ext2.link(
                            parent_link.unwrap().directory.get_inode(),
                            target_entry.directory.get_inode(),
                            link_path.file_name().unwrap().to_str().unwrap(),
                        )?;
                        Ok(())
                    }
                    None => Err(ErrorKind::NotFound.into()),
                }
            }
            _ => Err(ErrorKind::Unsupported.into()),
        }
    }

    /// Make a new name for a file. It is symbolic links.
    /// ```rust,ignore
    /// ext2.symlink("/bananes/toto.txt", "/tata.txt").unwrap();
    /// ```
    pub fn symlink<P: AsRef<Path>>(&mut self, target_path: P, link_path: P) -> Result<()> {
        let link_path = get_path(&link_path)?;
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        match link_path.parent() {
            Some(link_parent) => {
                let mut ext2 = self.0.lock().unwrap();
                if let Ok(Some(_)) = _find_entry(&ext2, link_path) {
                    return Err(ErrorKind::AlreadyExists.into());
                }
                let parent_link_entry = _find_entry(&ext2, link_parent)?;
                ext2.symlink(
                    parent_link_entry.unwrap().directory.get_inode(),
                    target_path.as_ref().to_str().unwrap(),
                    link_path.file_name().unwrap().to_str().unwrap(),
                    timestamp as u32,
                )?;
                Ok(())
            }
            _ => Err(ErrorKind::Unsupported.into()),
        }
    }
}

fn def_mode() -> Mode {
    Mode::S_IRUSR | Mode::S_IWUSR | Mode::S_IRGRP | Mode::S_IROTH
}

fn get_path<'a, P: AsRef<Path>>(path: &'a P) -> Result<&'a Path> {
    let path = path.as_ref();
    for component in path.components() {
        if component == Component::ParentDir {
            return Err(ErrorKind::Unsupported.into());
        }
    }
    match path.is_absolute() {
        true => Ok(path),
        false => Err(ErrorKind::Unsupported.into()),
    }
}

fn _find_entry<T>(ext2: &Ext2Filesystem<T>, path: &Path) -> Result<Option<inner::Entry>>
where
    T: Read + Seek + Write,
{
    Ok(match path.parent() {
        Some(parent) => {
            let mut iter = _lookup_directory(ext2, parent)?;
            iter.find(|entry| unsafe { entry.directory.get_filename() } == path.file_name().unwrap().to_str().unwrap())
        }
        // rootdir
        None => {
            let mut iter = _lookup_directory(ext2, path)?;
            iter.find(|entry| unsafe { entry.directory.get_filename() } == ".")
        }
    })
}

fn _lookup_directory<'a, T>(
    ext2: &'a Ext2Filesystem<T>,
    path: &Path,
) -> Result<impl Iterator<Item = inner::Entry> + 'a>
where
    T: Read + Seek + Write,
{
    debug_assert_eq!(path.is_absolute(), true);
    let mut iter = ext2.lookup_directory(2).expect("Root mut be a directory");
    for directory in path.components() {
        if directory == Component::RootDir {
            continue;
        } else {
            let elem = iter.find(|entry| {
                let filelen = directory.as_os_str().len();
                unsafe {
                    match libc::memcmp(
                        entry.directory.filename.0.as_ptr() as *const c_void,
                        directory.as_os_str().as_bytes().as_ptr() as *const c_void,
                        filelen,
                    ) {
                        0 => true,
                        _ => false,
                    }
                }
            });
            match elem {
                None => return Err(ErrorKind::NotFound.into()),
                Some(entry) => {
                    let inode = entry.directory.get_inode();
                    iter = ext2.lookup_directory(inode)?;
                }
            }
        }
    }
    Ok(iter)
}
fn _stat<T>(ext2: &Ext2Filesystem<T>, inode_nbr: u32, inode: Inode) -> Result<libc::stat>
where
    T: Read + Seek + Write,
{
    let mut stat = MaybeUninit::<libc::stat>::zeroed();
    let ptr = stat.as_mut_ptr();
    unsafe {
        (*ptr).st_dev = 0; // Device ID
        (*ptr).st_ino = inode_nbr as ino_t;
        (*ptr).st_mode = inode.type_and_perm.0 as u32; // Mode of file (see below).
        (*ptr).st_nlink = inode.nbr_hard_links as u64; // Number of hard links to the file.
        (*ptr).st_uid = inode.user_id as u32; // User ID of file.
        (*ptr).st_gid = inode.group_id as u32; // Group ID of file.
        (*ptr).st_rdev = 0; // Device ID (if file is character or block special).
        (*ptr).st_size = inode.low_size as off_t; // For regular files, the file size in bytes.
        (*ptr).st_atime = inode.last_access_time as time_t;
        (*ptr).st_mtime = inode.last_modification_time as time_t;
        (*ptr).st_ctime = inode.creation_time as time_t;
        (*ptr).st_blksize = ext2.get_block_size() as blksize_t;
        (*ptr).st_blocks = inode.nbr_disk_sectors as i64; // Number of blocks allocated for this object.
        (*ptr).st_atime_nsec = 0;
        (*ptr).st_mtime_nsec = 0;
        (*ptr).st_ctime_nsec = 0;
        Ok(stat.assume_init())
    }
}

/// Options and flags which can be used to configure how a file is opened.
///
/// This builder exposes the ability to configure how a [`File`] is opened and
/// what operations are permitted on the open file. The [`OpenOptions::open`] and
/// [`OpenOptions::create`] methods are aliases for commonly used options using this
/// builder.
///
/// Generally speaking, when using `OpenOptions`, you'll first call
/// [`OpenOptions::new`], then chain calls to methods to set each option, then
/// call [`OpenOptions::open`], passing the path of the file you're trying to
/// open. This will give you a [`std::io::Result`] with a [`File`] inside that you
/// can further operate on.
#[derive(Debug, Copy, Clone)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    create: bool,
    append: bool,
    truncate: bool,
}

impl OpenOptions {
    /// Creates a blank new set of options ready for configuration.
    ///
    /// All options are initially set to `false`.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let mut options = OpenOptions::new();
    /// let file = options.read(true).open("/foo.txt", ext2);
    /// ```
    pub fn new() -> Self {
        OpenOptions {
            read: false,
            write: false,
            create: false,
            append: false,
            truncate: false,
        }
    }

    /// Sets the option for read access.
    ///
    /// This option, when true, will indicate that the file should be
    /// `read`-able if opened.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let file = OpenOptions::new().read(true).open("/foo.txt", ext2);
    /// ```
    pub fn read(&mut self, read: bool) -> &mut Self {
        self.read = read;
        self
    }

    /// Sets the option for write access.
    ///
    /// This option, when true, will indicate that the file should be
    /// `write`-able if opened.
    ///
    /// If the file already exists, any write calls on it will overwrite its
    /// contents, without truncating it.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let file = OpenOptions::new().write(true).open("/foo.txt", ext2);
    /// ```
    pub fn write(&mut self, write: bool) -> &mut Self {
        self.write = write;
        self
    }

    /// Sets the option to create a new file, or open it if it already exists.
    ///
    /// In order for the file to be created, [`OpenOptions::write`] or
    /// [`OpenOptions::append`] access must be used.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let file = OpenOptions::new().write(true).create(true).open("/foo.txt", ext2);
    /// ```
    pub fn create(&mut self, create: bool) -> &mut Self {
        self.create = create;
        self
    }

    /// Sets the option for the append mode.
    ///
    /// This option, when true, means that writes will append to a file instead
    /// of overwriting previous contents.
    /// Note that setting `.write(true).append(true)` has the same effect as
    /// setting only `.append(true)`.
    ///
    /// For most filesystems, the operating system guarantees that all writes are
    /// atomic: no writes get mangled because another process writes at the same
    /// time.
    ///
    /// One maybe obvious note when using append-mode: make sure that all data
    /// that belongs together is written to the file in one operation. This
    /// can be done by concatenating strings before passing them to [`write()`],
    /// or using a buffered writer (with a buffer of adequate size),
    /// and calling [`flush()`] when the message is complete.
    ///
    /// If a file is opened with both read and append access, beware that after
    /// opening, and after every write, the position for reading may be set at the
    /// end of the file. So, before writing, save the current position (using
    /// <code>[seek]\([SeekFrom]::[Current]\(0))</code>), and restore it before the next read.
    ///
    /// ## Note
    ///
    /// This function doesn't create the file if it doesn't exist. Use the
    /// [`OpenOptions::create`] method to do so.
    ///
    /// [`write()`]: Write::write "io::Write::write"
    /// [`flush()`]: Write::flush "io::Write::flush"
    /// [seek]: Seek::seek "io::Seek::seek"
    /// [Current]: SeekFrom::Current "io::SeekFrom::Current"
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let file = OpenOptions::new().append(true).open("/foo.txt", ext2);
    /// ```
    pub fn append(&mut self, append: bool) -> &mut Self {
        self.append = append;
        self
    }

    /// Sets the option for truncating a previous file.
    ///
    /// If a file is successfully opened with this option set it will truncate
    /// the file to 0 length if it already exists.
    ///
    /// The file must be opened with write access for truncate to work.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let file = OpenOptions::new().write(true).truncate(true).open("/foo.txt", ext2);
    /// ```
    pub fn truncate(&mut self, truncate: bool) -> &mut Self {
        self.truncate = truncate;
        self
    }

    /// Opens a file at `path` with the options specified by `self`.
    ///
    /// # Errors
    ///
    /// This function will return an error under a number of different
    /// circumstances. Some of these error conditions are listed here, together
    /// with their [`std::io::ErrorKind`]. The mapping to [`std::io::ErrorKind`]s is not
    /// part of the compatibility contract of the function.
    ///
    /// * [`NotFound`]: The specified file does not exist and neither `create`
    ///   or `create_new` is set.
    /// * [`NotFound`]: One of the directory components of the file path does
    ///   not exist.
    /// * [`InvalidInput`]: Invalid combinations of open options (truncate
    ///   without write access, no access mode set, etc.).
    ///
    /// The following errors don't match any existing [`std::io::ErrorKind`] at the moment:
    /// * One of the directory components of the specified file path
    ///   was not, in fact, a directory.
    /// * Filesystem-level errors: full disk, write permission
    ///   requested on a read-only file system, exceeded disk quota, too many
    ///   open files, too long filename, too many symbolic links in the
    ///   specified path (Unix-like systems only), etc.
    ///
    /// # Examples
    ///
    /// ```rust,ignore
    /// use ext2::OpenOptions;
    ///
    /// let f = std::fs::OpenOptions::new()
    ///     .read(true)
    ///     .write(true)
    ///     .open(MY_DISK_OBJECT)
    ///     .expect("open filesystem failed");
    /// let ext2 = ext2::open_ext2_drive(f).unwrap();
    ///
    /// let file = OpenOptions::new().read(true).open("/foo.txt", ext2);
    /// ```
    ///
    /// [`InvalidInput`]: std::io::ErrorKind::InvalidInput
    /// [`NotFound`]: std::io::ErrorKind::NotFound
    pub fn open<T, P: AsRef<Path>>(&mut self, path: P, ext2_clone: Ext2<T>) -> Result<File<T>>
    where
        T: Read + Seek + Write,
    {
        let path = get_path(&path)?;
        #[cfg(unstable)]
        path.parent().ok_or_else(|| ErrorKind::IsADirectory)?;
        #[cfg(not(unstable))]
        path.parent().ok_or_else(|| ErrorKind::PermissionDenied)?;
        let mut ext2 = ext2_clone.0.lock().unwrap();

        let file = _find_entry(&ext2, path)?;
        match file {
            Some(file) => {
                if file.inode.is_a_directory() {
                    // TODO Must be a regular file
                    #[cfg(unstable)]
                    return Err(ErrorKind::IsADirectory.into());
                    #[cfg(not(unstable))]
                    Err(ErrorKind::PermissionDenied.into())
                } else {
                    if self.truncate && self.write {
                        ext2.truncate(file.directory.get_inode(), 0)?;
                    }
                    let curr_offset = if self.append && self.write {
                        ext2.read_inode(file.directory.get_inode())?.get_size() as i64
                    } else {
                        0
                    };
                    drop(ext2);
                    Ok(File {
                        inode: file.directory.get_inode(),
                        curr_offset: curr_offset as u64,
                        ext2: ext2_clone,
                        options: *self,
                    })
                }
            }
            None => {
                if self.create && self.write {
                    let timestamp = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap()
                        .as_secs();
                    let parent = _find_entry(&ext2, path.parent().unwrap())?;
                    let entry = ext2.create(
                        path.file_name().unwrap().to_str().unwrap(),
                        parent.unwrap().directory.get_inode(),
                        timestamp as u32,
                        (def_mode().bits(), SFlag::S_IFREG).try_into().unwrap(),
                        (
                            nix::unistd::geteuid().as_raw(),
                            nix::unistd::getegid().as_raw(),
                        ),
                    )?;
                    drop(ext2);
                    Ok(File {
                        inode: entry.directory.get_inode(),
                        curr_offset: 0,
                        ext2: ext2_clone,
                        options: *self,
                    })
                } else {
                    Err(ErrorKind::NotFound.into())
                }
            }
        }
    }
}

/// An object providing access to an open file on the EXT2 filesystem.
///
/// An instance of a `File` can be read and/or written depending on what options
/// it was opened with. Files also implement [`Seek`] to alter the logical cursor
/// that the file contains internally.
///
/// Files are automatically closed when they go out of scope.  Errors detected
/// on closing are ignored by the implementation of `Drop`.
#[derive(Debug)]
pub struct File<T>
where
    T: Read + Seek + Write,
{
    inode: u32,
    curr_offset: u64,
    ext2: Ext2<T>,
    options: OpenOptions,
}

impl<T> File<T>
where
    T: Read + Seek + Write,
{
    /// **currently unimplemented!()** : Metadata information about a file.
    ///
    /// This structure is returned from the [`File::metadata`] function or method and represents known
    /// metadata about a file such as its permissions, size, modification
    /// times, etc.
    ///
    pub fn metadata() {
        unimplemented!();
    }
}

impl<T> Seek for File<T>
where
    T: Read + Seek + Write,
{
    // Required method
    fn seek(&mut self, pos: SeekFrom) -> Result<u64> {
        let ext2 = self.ext2.0.lock().unwrap();
        let file_len = ext2.read_inode(self.inode)?.get_size() as i64;
        match pos {
            SeekFrom::Start(u) => {
                if u > file_len as u64 {
                    return Err(ErrorKind::UnexpectedEof.into());
                }
                self.curr_offset = u;
            }
            SeekFrom::End(i) => {
                if i < 0 || i > file_len {
                    return Err(ErrorKind::UnexpectedEof.into());
                }
                self.curr_offset = (file_len - i) as u64;
            }
            SeekFrom::Current(i) => {
                let new_curr_offset = self.curr_offset as i64 + i;
                if new_curr_offset < 0 || new_curr_offset > file_len {
                    return Err(ErrorKind::UnexpectedEof.into());
                }
                self.curr_offset = new_curr_offset as u64;
            }
        }
        Ok(self.curr_offset)
    }
}

impl<T> Write for File<T>
where
    T: Read + Seek + Write,
{
    fn write(&mut self, buf: &[u8]) -> Result<usize> {
        if !self.options.write {
            return Err(ErrorKind::PermissionDenied.into());
        }
        let mut ext2 = self.ext2.0.lock().unwrap();
        Ok(ext2
            .write(self.inode, &mut self.curr_offset, buf)
            .map(|s| s.0 as usize)?)
    }

    fn flush(&mut self) -> Result<()> {
        Ok(())
    }
}

impl<T> Read for File<T>
where
    T: Read + Seek + Write,
{
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if !self.options.read {
            return Err(ErrorKind::PermissionDenied.into());
        }
        let mut ext2 = self.ext2.0.lock().unwrap();
        Ok(ext2
            .read(self.inode, &mut self.curr_offset, buf)
            .map(|s| s as usize)?)
    }
}
