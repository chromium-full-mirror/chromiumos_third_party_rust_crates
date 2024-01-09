use crate::IoResult;

use nix::errno::Errno;

use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::{size_of, MaybeUninit};

pub struct Disk<T: Write + Read + Seek>(pub T);

impl<T: Write + Read + Seek> Disk<T> {
    pub fn write_buffer(&mut self, offset: u64, buf: &[u8]) -> IoResult<u64> {
        let _r = self.0.seek(SeekFrom::Start(offset));
        match self.0.write_all(buf) {
            Ok(_) => Ok(buf.len() as u64),
            Err(e) => Err(Errno::from_i32(e.raw_os_error().unwrap())),
        }
    }

    pub fn read_buffer(&mut self, offset: u64, buf: &mut [u8]) -> IoResult<u64> {
        let _r = self.0.seek(SeekFrom::Start(offset));
        match self.0.read_exact(buf) {
            Ok(_) => Ok(buf.len() as u64),
            Err(e) => Err(Errno::from_i32(e.raw_os_error().unwrap())),
        }
    }

    /// Write a particulary struct inside file object
    pub fn write_struct<C: Copy>(&mut self, offset: u64, t: &C) -> IoResult<u64> {
        let s = unsafe { std::slice::from_raw_parts(t as *const _ as *const u8, size_of::<C>()) };
        self.write_buffer(offset, s)
    }

    /// Read a particulary struct in file object
    pub fn read_struct<C: Copy>(&mut self, offset: u64) -> IoResult<C> {
        let t = MaybeUninit::<C>::uninit();
        let count = self.read_buffer(offset, unsafe {
            std::slice::from_raw_parts_mut(t.as_ptr() as *mut u8, size_of::<C>())
        })?;
        let t = unsafe { t.assume_init() };
        if count as usize != size_of::<C>() {
            return Err(Errno::EIO);
        }
        Ok(t)
    }
}
