//! `pthread_barrier_*` - c-gull has no barrier, and ANV rendezvouses its worker
//! threads on one. A real futex-backed barrier (a no-op would let threads race
//! ahead). The barrier's 32-byte `pthread_barrier_t` storage holds: [0] count,
//! [1] arrived (atomic), [2] generation seq (atomic); threads wait on the seq
//! futex until the last arrival bumps it.

use core::ffi::{c_int, c_uint, c_void};
use core::sync::atomic::{AtomicU32, Ordering};

const FUTEX_WAIT_PRIVATE: i32 = 0 | 128;
const FUTEX_WAKE_PRIVATE: i32 = 1 | 128;
const SERIAL: c_int = -1; // PTHREAD_BARRIER_SERIAL_THREAD
const EINVAL: c_int = 22;

#[inline]
unsafe fn futex(uaddr: *mut u32, op: i32, val: u32) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") 202isize => ret, // SYS_futex
            in("rdi") uaddr,
            in("rsi") op,
            in("rdx") val,
            in("r10") 0isize, // timeout = NULL
            out("rcx") _, out("r11") _,
            options(nostack, preserves_flags),
        );
    }
    ret
}

#[inline]
unsafe fn field<'a>(b: *mut u32, i: usize) -> &'a AtomicU32 {
    unsafe { &*(b.add(i) as *const AtomicU32) }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn pthread_barrier_init(b: *mut u32, _attr: *const c_void, count: c_uint) -> c_int {
    if count == 0 {
        return EINVAL;
    }
    unsafe {
        *b.add(0) = count;
        field(b, 1).store(0, Ordering::SeqCst); // arrived
        field(b, 2).store(0, Ordering::SeqCst); // seq
    }
    0
}

#[unsafe(no_mangle)]
unsafe extern "C" fn pthread_barrier_wait(b: *mut u32) -> c_int {
    let count = unsafe { *b.add(0) };
    let arrived = unsafe { field(b, 1) };
    let seq = unsafe { field(b, 2) };
    let gen = seq.load(Ordering::SeqCst);
    let n = arrived.fetch_add(1, Ordering::SeqCst) + 1;
    if n >= count {
        arrived.store(0, Ordering::SeqCst);
        seq.fetch_add(1, Ordering::SeqCst);
        unsafe { futex(b.add(2), FUTEX_WAKE_PRIVATE, u32::MAX) };
        SERIAL
    } else {
        while seq.load(Ordering::SeqCst) == gen {
            unsafe { futex(b.add(2), FUTEX_WAIT_PRIVATE, gen) };
        }
        0
    }
}

#[unsafe(no_mangle)]
extern "C" fn pthread_barrier_destroy(_b: *mut u32) -> c_int {
    0
}

// barrierattr is advisory; no-op success is fine (we only support process-private).
#[unsafe(no_mangle)]
extern "C" fn pthread_barrierattr_init(_a: *mut c_void) -> c_int {
    0
}
#[unsafe(no_mangle)]
extern "C" fn pthread_barrierattr_destroy(_a: *mut c_void) -> c_int {
    0
}
#[unsafe(no_mangle)]
extern "C" fn pthread_barrierattr_setpshared(_a: *mut c_void, _pshared: c_int) -> c_int {
    0
}
