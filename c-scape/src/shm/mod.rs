use crate::convert_res;
use core::ffi::CStr;
use errno::{set_errno, Errno};
use libc::{c_char, c_int, c_void, mode_t};
use rustix::fd::IntoRawFd;
use rustix::fs::Mode;
use rustix::shm;

#[no_mangle]
unsafe extern "C" fn shm_open(name: *const c_char, oflags: c_int, mode: mode_t) -> c_int {
    libc!(libc::shm_open(name, oflags, mode));

    let name = CStr::from_ptr(name);
    let mode = Mode::from_bits((mode & !libc::S_IFMT) as _).unwrap();
    let oflags = shm::OFlags::from_bits(oflags as _).unwrap();
    match convert_res(shm::open(name, oflags, mode)) {
        Some(fd) => fd.into_raw_fd(),
        None => -1,
    }
}

#[no_mangle]
unsafe extern "C" fn shm_unlink(name: *const c_char) -> c_int {
    libc!(libc::shm_unlink(name));

    let name = CStr::from_ptr(name);
    match convert_res(shm::unlink(name)) {
        Some(()) => 0,
        None => -1,
    }
}

// taproot: System V shared memory is not supported; answer the contract
// errors instead of aborting. Callers (X11 MIT-SHM, tooling) all carry
// fallbacks for kernels/containers where sysvipc is unavailable.

#[no_mangle]
unsafe extern "C" fn shmget(_key: libc::key_t, _size: libc::size_t, _flag: c_int) -> c_int {
    set_errno(Errno(libc::ENOSYS));
    -1
}

#[no_mangle]
unsafe extern "C" fn shmat(_id: c_int, _addr: *const c_void, _flag: c_int) -> *mut c_void {
    set_errno(Errno(libc::ENOSYS));
    usize::MAX as *mut c_void
}

#[no_mangle]
unsafe extern "C" fn shmdt(_addr: *const c_void) -> c_int {
    set_errno(Errno(libc::EINVAL));
    -1
}

#[no_mangle]
unsafe extern "C" fn shmctl(_id: c_int, _cmd: c_int, _buf: *mut c_void) -> c_int {
    set_errno(Errno(libc::ENOSYS));
    -1
}
