use core::slice;
use libc::c_int;

use crate::convert_res;

#[no_mangle]
unsafe extern "C" fn poll(fds: *mut libc::pollfd, nfds: libc::nfds_t, timeout: c_int) -> c_int {
    libc!(libc::poll(fds, nfds, timeout));

    let pollfds: *mut rustix::event::PollFd<'_> = checked_cast!(fds);

    let fds = slice::from_raw_parts_mut(pollfds, nfds.try_into().unwrap());
    let timeout = if timeout < 0 {
        None
    } else {
        Some(rustix::event::Timespec {
            tv_sec: i64::from(timeout) / 1000,
            tv_nsec: (i64::from(timeout) % 1000) * 1_000_000,
        })
    };
    match convert_res(rustix::event::poll(fds, timeout.as_ref())) {
        Some(num) => num.try_into().unwrap(),
        None => -1,
    }
}

#[no_mangle]
unsafe extern "C" fn ppoll(
    fds: *mut libc::pollfd,
    nfds: libc::nfds_t,
    timeout: *const libc::timespec,
    sigmask: *const libc::sigset_t,
) -> c_int {
    libc!(libc::ppoll(fds, nfds, timeout, sigmask));

    // no atomic mask+wait without the raw syscall; set, poll, restore.
    // the window between them is the same one single-threaded code has
    // between sigprocmask and poll
    let mut old = core::mem::MaybeUninit::<libc::sigset_t>::uninit();
    let swapped = !sigmask.is_null()
        && libc::pthread_sigmask(libc::SIG_SETMASK, sigmask, old.as_mut_ptr()) == 0;

    let pollfds: *mut rustix::event::PollFd<'_> = checked_cast!(fds);
    let fds = slice::from_raw_parts_mut(pollfds, nfds.try_into().unwrap());
    let timeout = if timeout.is_null() {
        None
    } else {
        Some(rustix::event::Timespec {
            tv_sec: (*timeout).tv_sec,
            tv_nsec: (*timeout).tv_nsec,
        })
    };
    let res = rustix::event::poll(fds, timeout.as_ref());

    if swapped {
        libc::pthread_sigmask(libc::SIG_SETMASK, old.as_ptr(), core::ptr::null_mut());
    }
    match convert_res(res) {
        Some(num) => num.try_into().unwrap(),
        None => -1,
    }
}

#[no_mangle]
unsafe extern "C" fn __ppoll_chk(
    fds: *mut libc::pollfd,
    nfds: libc::nfds_t,
    timeout: *const libc::timespec,
    sigmask: *const libc::sigset_t,
    fdslen: libc::size_t,
) -> c_int {
    assert!(fdslen >= nfds as libc::size_t);
    ppoll(fds, nfds, timeout, sigmask)
}
