//! A dlopen'd libc.so runs its init_array, not origin's `_start`, so this
//! copy of origin/rustix never sees the kernel's auxv: `page_size()` reads 0
//! and `pthread_create` builds threads with no TLS image and no guard page.
//! Feed them the real auxv from /proc here, shaped as the envp-then-auxv
//! block rustix expects. The ctor's own argv/envp are loader-shaped and not
//! trustworthy under every host, so they are ignored.

use core::ffi::{c_char, c_int};
use rustix::fs::{Mode, OFlags};

unsafe extern "C" fn dso_runtime_init(
    _argc: c_int,
    _argv: *mut *mut c_char,
    _envp: *mut *mut c_char,
) {
    // buf[0] is the empty environment's terminator; the auxv copy starts at
    // buf[1] and holds at most 39 pairs, so the zeroed tail always leaves an
    // AT_NULL stop even on a truncated read
    let mut buf = [0usize; 82];
    let Ok(fd) = rustix::fs::open(
        c"/proc/self/auxv",
        OFlags::RDONLY | OFlags::CLOEXEC,
        Mode::empty(),
    ) else {
        return;
    };
    let dst = unsafe {
        core::slice::from_raw_parts_mut(
            buf[1..].as_mut_ptr().cast::<u8>(),
            78 * core::mem::size_of::<usize>(),
        )
    };
    let mut filled = 0;
    while filled < dst.len() {
        match rustix::io::read(&fd, &mut dst[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return,
        }
    }
    unsafe { origin::program::init_from_dso(buf.as_mut_ptr().cast()) };
}

#[used]
#[unsafe(link_section = ".init_array")]
static DSO_RUNTIME_INIT: unsafe extern "C" fn(c_int, *mut *mut c_char, *mut *mut c_char) =
    dso_runtime_init;
