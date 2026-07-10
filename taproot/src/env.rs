//! Initialize `environ` for the dlopened libc.so.6.
//!
//! Our libc is loaded as a shared object, so c-gull's origin `_start` - which
//! normally captures argv/envp off the initial stack and sets `environ` - never
//! runs (a dlopened .so runs its `.init_array`, not `_start`). c-gull's global
//! `environ` is therefore null, and the first `getenv()` a driver or libstdc++
//! makes dereferences it and segfaults. This constructor runs when the .so is
//! dlopened and points `environ` at the process's real environment, read from
//! `/proc/self/environ`. Static buffers only: no malloc, no std runtime.

use core::ffi::{c_char, c_int};

unsafe extern "C" {
    // c-gull's global `char **environ`; we set it, not redefine it.
    static mut environ: *mut *mut c_char;
}

const SYS_OPENAT: isize = 257;
const SYS_READ: isize = 0;
const SYS_CLOSE: isize = 3;
const AT_FDCWD: isize = -100;
const O_RDONLY: isize = 0;

#[inline]
unsafe fn sys3(n: isize, a: isize, b: isize, c: isize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") n => ret,
            in("rdi") a, in("rsi") b, in("rdx") c,
            out("rcx") _, out("r11") _,
            options(nostack, preserves_flags),
        );
    }
    ret
}

// The environment block (KEY=VAL\0KEY=VAL\0...) and the argv-style pointer array
// that `environ` will point at. Sized generously for a desktop session.
static mut ENV_BLOCK: [u8; 128 * 1024] = [0; 128 * 1024];
static mut ENV_PTRS: [*mut c_char; 4096] = [core::ptr::null_mut(); 4096];

// Registered in .init_array so the loader runs it when libc.so.6 is dlopened.
#[used]
#[unsafe(link_section = ".init_array")]
static INIT_ENVIRON: extern "C" fn() = init_environ;

extern "C" fn init_environ() {
    unsafe {
        let block = core::ptr::addr_of_mut!(ENV_BLOCK) as *mut u8;
        let ptrs = core::ptr::addr_of_mut!(ENV_PTRS) as *mut *mut c_char;

        // read /proc/self/environ fully into ENV_BLOCK
        let path = c"/proc/self/environ".as_ptr() as isize;
        let fd = sys3(SYS_OPENAT, AT_FDCWD, path, O_RDONLY) as c_int;
        let mut filled: usize = 0;
        if fd >= 0 {
            let cap = ENV_BLOCK.len() - 1;
            loop {
                let n = sys3(
                    SYS_READ,
                    fd as isize,
                    block.add(filled) as isize,
                    (cap - filled) as isize,
                );
                if n <= 0 {
                    break;
                }
                filled += n as usize;
                if filled >= cap {
                    break;
                }
            }
            sys3(SYS_CLOSE, fd as isize, 0, 0);
        }

        // split on NUL into the pointer array, null-terminated
        let mut count = 0usize;
        let mut start = 0usize;
        let max = ENV_PTRS.len() - 1;
        let mut i = 0usize;
        while i < filled && count < max {
            if *block.add(i) == 0 {
                if i > start {
                    *ptrs.add(count) = block.add(start) as *mut c_char;
                    count += 1;
                }
                start = i + 1;
            }
            i += 1;
        }
        *ptrs.add(count) = core::ptr::null_mut();

        environ = ptrs;
    }
}
