use errno::{set_errno, Errno};
use rustix::fd::{BorrowedFd, IntoRawFd};
use rustix::fs::{FlockOperation, OFlags};
use rustix::io::FdFlags;

use libc::{c_int, c_long};

use crate::convert_res;

// taproot: fixed arity. One `c_long` third argument covers both the int and
// the pointer forms (both are a single INTEGER-class slot, and a variadic
// caller loads it into the same register); it is only interpreted, per cmd,
// in the arms whose commands define a third argument.
#[no_mangle]
unsafe extern "C" fn fcntl(fd: c_int, cmd: c_int, arg: c_long) -> c_int {
    _fcntl::<libc::flock>(fd, cmd, arg)
}

#[no_mangle]
unsafe extern "C" fn fcntl64(fd: c_int, cmd: c_int, arg: c_long) -> c_int {
    _fcntl::<libc::flock64>(fd, cmd, arg)
}

unsafe fn _fcntl<FlockTy: Flock>(fd: c_int, cmd: c_int, arg: c_long) -> c_int {
    match cmd {
        libc::F_GETFL => {
            libc!(libc::fcntl(fd, libc::F_GETFL));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::fs::fcntl_getfl(fd)) {
                Some(flags) => flags.bits() as _,
                None => -1,
            }
        }
        libc::F_SETFL => {
            let flags = arg as c_int;
            libc!(libc::fcntl(fd, libc::F_SETFL, flags));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::fs::fcntl_setfl(
                fd,
                OFlags::from_bits(flags as _).unwrap(),
            )) {
                Some(()) => 0,
                None => -1,
            }
        }
        libc::F_GETFD => {
            libc!(libc::fcntl(fd, libc::F_GETFD));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::io::fcntl_getfd(fd)) {
                Some(flags) => flags.bits() as _,
                None => -1,
            }
        }
        libc::F_SETFD => {
            let flags = arg as c_int;
            libc!(libc::fcntl(fd, libc::F_SETFD, flags));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::io::fcntl_setfd(
                fd,
                FdFlags::from_bits(flags as _).unwrap(),
            )) {
                Some(()) => 0,
                None => -1,
            }
        }
        libc::F_SETLK | libc::F_SETLKW => {
            let ptr = arg as *mut FlockTy;
            libc!(libc::fcntl(fd, cmd, ptr));
            let fd = BorrowedFd::borrow_raw(fd);
            let is_blocking = cmd == libc::F_SETLKW;
            let flock = &mut *ptr;
            let op = match (flock.l_type() as _, is_blocking) {
                (libc::F_RDLCK, true) => FlockOperation::LockShared,
                (libc::F_WRLCK, true) => FlockOperation::LockExclusive,
                (libc::F_UNLCK, true) => FlockOperation::Unlock,
                (libc::F_RDLCK, false) => FlockOperation::NonBlockingLockShared,
                (libc::F_WRLCK, false) => FlockOperation::NonBlockingLockExclusive,
                (libc::F_UNLCK, false) => FlockOperation::NonBlockingUnlock,
                _ => {
                    set_errno(Errno(libc::EINVAL));
                    return -1;
                }
            };
            // We currently only support whole-file locks.
            assert_eq!(
                flock.l_whence(),
                libc::SEEK_SET as _,
                "partial-file locks not yet implemented"
            );
            assert_eq!(flock.l_start(), 0, "partial-file locks not yet implemented");
            assert_eq!(flock.l_len(), 0, "partial-file locks not yet implemented");
            match convert_res(rustix::fs::fcntl_lock(fd, op)) {
                Some(()) => {
                    flock.l_pid(-1);
                    0
                }
                None => -1,
            }
        }
        #[cfg(not(target_os = "wasi"))]
        libc::F_DUPFD_CLOEXEC => {
            let arg = arg as c_int;
            libc!(libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, arg));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::io::fcntl_dupfd_cloexec(fd, arg)) {
                Some(fd) => fd.into_raw_fd(),
                None => -1,
            }
        }
        _ => {
            errno::set_errno(errno::Errno(libc::EINVAL));
            -1
        }
    }
}

trait Flock {
    fn l_type(&self) -> i16;
    fn l_whence(&self) -> i16;
    fn l_start(&self) -> libc::off64_t;
    fn l_len(&self) -> libc::off64_t;
    fn l_pid(&mut self, pid: libc::pid_t);
}

impl Flock for libc::flock {
    fn l_type(&self) -> i16 {
        self.l_type
    }

    fn l_whence(&self) -> i16 {
        self.l_whence
    }

    fn l_start(&self) -> libc::off64_t {
        self.l_start.into()
    }

    fn l_len(&self) -> libc::off64_t {
        self.l_len.into()
    }

    fn l_pid(&mut self, pid: libc::pid_t) {
        self.l_pid = pid;
    }
}

impl Flock for libc::flock64 {
    fn l_type(&self) -> i16 {
        self.l_type
    }

    fn l_whence(&self) -> i16 {
        self.l_whence
    }

    fn l_start(&self) -> libc::off64_t {
        self.l_start
    }

    fn l_len(&self) -> libc::off64_t {
        self.l_len
    }

    fn l_pid(&mut self, pid: libc::pid_t) {
        self.l_pid = pid;
    }
}

// whole-file only, like the F_SETLK arm above; a section lock covering
// [offset, offset+len) locks the whole file instead, which is stronger
// but never weaker
#[no_mangle]
unsafe extern "C" fn lockf(fd: c_int, cmd: c_int, _len: libc::off_t) -> c_int {
    libc!(libc::lockf(fd, cmd, _len));

    let fd = BorrowedFd::borrow_raw(fd);
    let op = match cmd {
        libc::F_ULOCK => FlockOperation::Unlock,
        libc::F_LOCK => FlockOperation::LockExclusive,
        libc::F_TLOCK | libc::F_TEST => FlockOperation::NonBlockingLockExclusive,
        _ => {
            set_errno(Errno(libc::EINVAL));
            return -1;
        }
    };
    match convert_res(rustix::fs::fcntl_lock(fd, op)) {
        Some(()) => {
            if cmd == libc::F_TEST {
                // only probing: put it back
                let _ = rustix::fs::fcntl_lock(fd, FlockOperation::Unlock);
            }
            0
        }
        None => -1,
    }
}

#[no_mangle]
unsafe extern "C" fn lockf64(fd: c_int, cmd: c_int, len: libc::off64_t) -> c_int {
    lockf(fd, cmd, len)
}
