//! futex-backed POSIX semaphores. the count lives in the first word of
//! `sem_t`; process-shared init is refused rather than half-supported.

use core::sync::atomic::{AtomicU32, Ordering::SeqCst};
use errno::{set_errno, Errno};
use libc::{c_int, c_uint};
use rustix::thread::futex;

// linux: INT_MAX; the libc crate does not carry the constant
const SEM_VALUE_MAX: u32 = 0x7fff_ffff;

fn count(sem: *mut libc::sem_t) -> &'static AtomicU32 {
    unsafe { &*sem.cast::<AtomicU32>() }
}

#[no_mangle]
unsafe extern "C" fn sem_init(sem: *mut libc::sem_t, pshared: c_int, value: c_uint) -> c_int {
    libc!(libc::sem_init(sem, pshared, value));

    if pshared != 0 {
        set_errno(Errno(libc::ENOSYS));
        return -1;
    }
    if value > SEM_VALUE_MAX {
        set_errno(Errno(libc::EINVAL));
        return -1;
    }
    count(sem).store(value, SeqCst);
    0
}

#[no_mangle]
unsafe extern "C" fn sem_destroy(sem: *mut libc::sem_t) -> c_int {
    libc!(libc::sem_destroy(sem));

    count(sem).store(0, SeqCst);
    0
}

#[no_mangle]
unsafe extern "C" fn sem_getvalue(sem: *mut libc::sem_t, sval: *mut c_int) -> c_int {
    libc!(libc::sem_getvalue(sem, sval));

    *sval = count(sem).load(SeqCst) as c_int;
    0
}

#[no_mangle]
unsafe extern "C" fn sem_post(sem: *mut libc::sem_t) -> c_int {
    libc!(libc::sem_post(sem));

    let c = count(sem);
    if c.fetch_add(1, SeqCst) == SEM_VALUE_MAX {
        c.fetch_sub(1, SeqCst);
        set_errno(Errno(libc::EOVERFLOW));
        return -1;
    }
    let _ = futex::wake(c, futex::Flags::PRIVATE, 1);
    0
}

fn try_take(c: &AtomicU32) -> bool {
    let mut cur = c.load(SeqCst);
    while cur > 0 {
        match c.compare_exchange_weak(cur, cur - 1, SeqCst, SeqCst) {
            Ok(_) => return true,
            Err(now) => cur = now,
        }
    }
    false
}

#[no_mangle]
unsafe extern "C" fn sem_trywait(sem: *mut libc::sem_t) -> c_int {
    libc!(libc::sem_trywait(sem));

    if try_take(count(sem)) {
        0
    } else {
        set_errno(Errno(libc::EAGAIN));
        -1
    }
}

#[no_mangle]
unsafe extern "C" fn sem_wait(sem: *mut libc::sem_t) -> c_int {
    libc!(libc::sem_wait(sem));

    let c = count(sem);
    loop {
        if try_take(c) {
            return 0;
        }
        match futex::wait(c, futex::Flags::PRIVATE, 0, None) {
            Ok(()) | Err(rustix::io::Errno::AGAIN) => {}
            Err(rustix::io::Errno::INTR) => {
                set_errno(Errno(libc::EINTR));
                return -1;
            }
            Err(err) => {
                set_errno(Errno(err.raw_os_error()));
                return -1;
            }
        }
    }
}

#[no_mangle]
unsafe extern "C" fn sem_timedwait(
    sem: *mut libc::sem_t,
    abstime: *const libc::timespec,
) -> c_int {
    libc!(libc::sem_timedwait(sem, abstime));

    let c = count(sem);
    loop {
        if try_take(c) {
            return 0;
        }
        // the deadline is CLOCK_REALTIME absolute; the futex wait is relative
        let now = rustix::time::clock_gettime(rustix::time::ClockId::Realtime);
        let deadline = abstime.read();
        let mut sec = deadline.tv_sec as i64 - now.tv_sec;
        let mut nsec = deadline.tv_nsec as i64 - now.tv_nsec;
        if nsec < 0 {
            sec -= 1;
            nsec += 1_000_000_000;
        }
        if sec < 0 {
            set_errno(Errno(libc::ETIMEDOUT));
            return -1;
        }
        let rel = rustix::time::Timespec {
            tv_sec: sec,
            tv_nsec: nsec,
        };
        match futex::wait(c, futex::Flags::PRIVATE, 0, Some(&rel)) {
            Ok(()) | Err(rustix::io::Errno::AGAIN) => {}
            Err(rustix::io::Errno::TIMEDOUT) => {
                set_errno(Errno(libc::ETIMEDOUT));
                return -1;
            }
            Err(rustix::io::Errno::INTR) => {
                set_errno(Errno(libc::EINTR));
                return -1;
            }
            Err(err) => {
                set_errno(Errno(err.raw_os_error()));
                return -1;
            }
        }
    }
}
