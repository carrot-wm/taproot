use core::ffi::CStr;
use errno::{set_errno, Errno};
use rustix::fd::{BorrowedFd, IntoRawFd};
use rustix::fs::{Mode, OFlags, CWD};

use libc::{c_char, c_int, mode_t};

use crate::convert_res;

// Shared body for the `open` variants, which all morally delegate to
// openat64; a macro (rather than a helper fn) lets each entry keep its own
// `libc!` signature check.
//
// taproot: fixed arity. A variadic caller and a fixed-arity callee agree on
// where the leading INTEGER-class arguments live (SysV x86_64), so `mode`
// reads whatever sits in the third argument slot; when the caller didn't
// pass one that's garbage, which is fine because it is only inspected under
// O_CREAT/O_TMPFILE, exactly when C requires callers to pass it.
macro_rules! openat_impl {
    ($fd:expr, $pathname:ident, $flags:ident, $mode:ident) => {{
        let flags = OFlags::from_bits($flags as _).unwrap();
        let mode = if flags.contains(OFlags::CREATE) || flags.contains(OFlags::TMPFILE) {
            Mode::from_bits(($mode & !libc::S_IFMT) as _).unwrap()
        } else {
            Mode::empty()
        };
        match convert_res(rustix::fs::openat(
            &$fd,
            CStr::from_ptr($pathname.cast()),
            flags,
            mode,
        )) {
            Some(fd) => fd.into_raw_fd(),
            None => -1,
        }
    }};
}

// we open all files with O_LARGEFILE as that is what Rustix does
// hopefully this doesn't break any C programs
#[no_mangle]
unsafe extern "C" fn open(pathname: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    libc!(libc::open(pathname, flags, mode));

    openat_impl!(CWD, pathname, flags, mode)
}

#[no_mangle]
unsafe extern "C" fn open64(pathname: *const c_char, flags: c_int, mode: mode_t) -> c_int {
    libc!(libc::open64(pathname, flags, mode));

    openat_impl!(CWD, pathname, flags, mode)
}

// same behavior with `O_LARGEFILE` as open/open64
#[no_mangle]
unsafe extern "C" fn openat(
    fd: c_int,
    pathname: *const c_char,
    flags: c_int,
    mode: mode_t,
) -> c_int {
    libc!(libc::openat(fd, pathname, flags, mode));

    openat_impl!(BorrowedFd::borrow_raw(fd), pathname, flags, mode)
}

#[no_mangle]
unsafe extern "C" fn openat64(
    fd: c_int,
    pathname: *const c_char,
    flags: c_int,
    mode: mode_t,
) -> c_int {
    libc!(libc::openat64(fd, pathname, flags, mode));

    openat_impl!(BorrowedFd::borrow_raw(fd), pathname, flags, mode)
}

#[no_mangle]
unsafe extern "C" fn creat(name: *const c_char, mode: mode_t) -> c_int {
    libc!(libc::creat(name, mode));

    creat64(name, mode)
}

#[no_mangle]
unsafe extern "C" fn creat64(name: *const c_char, mode: mode_t) -> c_int {
    libc!(libc::creat64(name, mode));

    open(name, libc::O_CREAT | libc::O_WRONLY | libc::O_TRUNC, mode)
}

#[no_mangle]
unsafe extern "C" fn close(fd: c_int) -> c_int {
    libc!(libc::close(fd));

    if fd == -1 {
        set_errno(Errno(libc::EBADF));
        return -1;
    }

    rustix::io::close(fd);
    0
}
