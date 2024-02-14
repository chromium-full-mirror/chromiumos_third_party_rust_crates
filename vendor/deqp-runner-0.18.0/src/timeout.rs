use mio::event::Source;
use mio::{Events, Interest, Poll, Token};
use std::cell::RefCell;
use std::io::{Error, ErrorKind, Read, Result};
use std::ops::{Deref, DerefMut};
use std::process::Child;
use std::rc::Rc;
use std::time::{Duration, Instant};

#[derive(Clone)]
pub struct Timer {
    start: Rc<RefCell<Instant>>,
    timeout: Duration,
}

impl Timer {
    pub fn new(timeout: Duration) -> Timer {
        Timer {
            start: Rc::new(RefCell::new(Instant::now())),
            timeout,
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.start.borrow().elapsed()
    }

    pub fn remaining(&self) -> Duration {
        self.timeout - self.elapsed()
    }

    pub fn expired(&self) -> bool {
        self.elapsed() > self.timeout
    }

    pub fn reset(&self) {
        self.start.replace(Instant::now());
    }
}

pub struct TimeoutChild {
    inner: Child,
    poll: Poll,
    stdout: Option<BufStdio>,
    stderr: Option<BufStdio>,
    timer: Timer,
}

impl TimeoutChild {
    const TOKEN_STDOUT: usize = 0;
    const TOKEN_STDERR: usize = 1;

    pub fn stdout(&mut self) -> Option<TimeoutChildStdout> {
        if self.stdout.is_some() {
            Some(TimeoutChildStdout { child: self })
        } else {
            None
        }
    }

    pub fn stderr(&mut self) -> Option<TimeoutChildStderr> {
        if self.stderr.is_some() {
            Some(TimeoutChildStderr { child: self })
        } else {
            None
        }
    }

    fn close(&mut self) {
        if let Some(stdout) = &mut self.stdout {
            stdout.close();
        }
        if let Some(stderr) = &mut self.stderr {
            stderr.close();
        }
    }

    fn poll(&mut self) -> Result<()> {
        if self.timer.expired() {
            self.close();
            return Err(Error::from(ErrorKind::TimedOut));
        }

        let mut events = Events::with_capacity(8);
        self.poll.poll(&mut events, Some(self.timer.remaining()))?;
        if events.is_empty() {
            self.close();
            return Err(Error::from(ErrorKind::TimedOut));
        }

        for event in &events {
            match event.token().0 {
                Self::TOKEN_STDOUT => self.stdout.as_mut().unwrap().fill_buf(),
                Self::TOKEN_STDERR => self.stderr.as_mut().unwrap().fill_buf(),
                _ => panic!("Unexpected event token"),
            };
        }

        Ok(())
    }
}

pub trait TimeoutChildExt {
    fn with_timeout(self, timer: Timer) -> TimeoutChild;
}

impl TimeoutChildExt for Child {
    fn with_timeout(mut self, timer: Timer) -> TimeoutChild {
        let mut stdout = self.stdout.take().map(BufStdio::new);
        let mut stderr = self.stderr.take().map(BufStdio::new);
        let poll = Poll::new().unwrap();

        if let Some(ref mut stdout) = stdout {
            poll.registry()
                .register(
                    stdout,
                    Token(TimeoutChild::TOKEN_STDOUT),
                    Interest::READABLE,
                )
                .unwrap();
        }
        if let Some(ref mut stderr) = stderr {
            poll.registry()
                .register(
                    stderr,
                    Token(TimeoutChild::TOKEN_STDERR),
                    Interest::READABLE,
                )
                .unwrap();
        }

        TimeoutChild {
            inner: self,
            poll,
            stdout,
            stderr,
            timer,
        }
    }
}

impl Deref for TimeoutChild {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for TimeoutChild {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

pub struct TimeoutChildStdout<'a> {
    child: &'a mut TimeoutChild,
}

impl<'a> Read for TimeoutChildStdout<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            match self.child.stdout.as_mut().unwrap().read(buf) {
                Ok(r) => return Ok(r),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => self.child.poll()?,
                Err(e) => return Err(e),
            }
        }
    }
}

pub struct TimeoutChildStderr<'a> {
    child: &'a mut TimeoutChild,
}

impl<'a> Read for TimeoutChildStderr<'a> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        loop {
            match self.child.stderr.as_mut().unwrap().read(buf) {
                Ok(r) => return Ok(r),
                Err(ref err) if err.kind() == ErrorKind::WouldBlock => self.child.poll()?,
                Err(e) => return Err(e),
            }
        }
    }
}

struct BufStdio {
    buf: Vec<u8>,
    inner: imp::Stdio,
    pos: u64,
    eof: bool,
    error: Option<Error>,
}

impl BufStdio {
    fn new(stdio: impl imp::IntoStdio) -> BufStdio {
        BufStdio {
            buf: Vec::new(),
            inner: imp::new(stdio),
            pos: 0,
            eof: false,
            error: None,
        }
    }

    fn close(&mut self) {
        self.eof = true;
    }

    fn fill_buf(&mut self) {
        // copy() can still have copied data even if an error was returned
        match std::io::copy(&mut self.inner, &mut self.buf) {
            Ok(0) => self.eof = true,
            Ok(_) => {}
            Err(ref err) if err.kind() == ErrorKind::WouldBlock => {}
            Err(e) => self.error = Some(e),
        }
    }

    fn remaining_slice(&self) -> &[u8] {
        let len = self.pos.min(self.buf.len() as u64);
        &self.buf[(len as usize)..]
    }
}

impl Read for BufStdio {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        if self.pos >= self.buf.len() as u64 {
            self.fill_buf();
        }

        if self.pos >= self.buf.len() as u64 {
            if self.eof {
                return Ok(0);
            } else if let Some(err) = self.error.take() {
                self.close();
                return Err(err);
            } else {
                return Err(Error::from(ErrorKind::WouldBlock));
            }
        }
        let n = Read::read(&mut self.remaining_slice(), buf)?;
        self.pos += n as u64;
        Ok(n)
    }
}

impl Source for BufStdio {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: Token,
        interests: Interest,
    ) -> std::io::Result<()> {
        self.inner.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &mio::Registry,
        token: Token,
        interests: Interest,
    ) -> std::io::Result<()> {
        self.inner.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &mio::Registry) -> std::io::Result<()> {
        self.inner.deregister(registry)
    }
}

#[cfg(not(windows))]
mod imp {
    use mio::unix::pipe::Receiver;

    pub(super) type Stdio = Receiver;
    pub(super) trait IntoStdio: Into<Receiver> {}
    impl<T> IntoStdio for T where T: Into<Receiver> {}

    pub(super) fn new(inner: impl IntoStdio) -> Stdio {
        let receiver = inner.into();
        receiver.set_nonblocking(true).unwrap();
        receiver
    }
}

#[cfg(windows)]
mod imp {
    use mio::windows::NamedPipe;
    use std::os::windows::io::{FromRawHandle, IntoRawHandle};

    pub(super) type Stdio = NamedPipe;
    pub(super) trait IntoStdio: IntoRawHandle {}
    impl<T> IntoStdio for T where T: IntoRawHandle {}

    pub(super) fn new(inner: impl IntoStdio) -> Stdio {
        unsafe { NamedPipe::from_raw_handle(inner.into_raw_handle()) }
    }
}
