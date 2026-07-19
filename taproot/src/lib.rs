//! A standalone pure-Rust `libc.so.6`: link in Eyra (origin + c-gull) so a
//! dlopened C library (Mesa / libdrm) binds its libc against Rust, not glibc.
// On x86_64 the scanf entries are c-scape va-walker shims and need no
// nightly gate; other architectures keep the nightly variadic definitions.
#![cfg_attr(not(target_arch = "x86_64"), feature(c_variadic))]

extern crate eyra;

mod barrier;
mod dso_init;
mod env;
mod gap;
mod scanf;

use core::ffi::{c_char, c_int, c_long, c_void};

// eyra's origin (take-charge/call-main) references the program's `main`; a libc
// .so has none, so satisfy the relocation with a stub. It is never invoked - a
// dlopen'd .so runs its init_array, not origin's _start.
#[unsafe(no_mangle)]
extern "C" fn main(_argc: i32, _argv: *const *const u8, _envp: *const *const u8) -> i32 {
    0
}

// ioctl pass-through (unrecognized requests -> kernel, with errno) now lives in
// the vendored c-scape fork (src/io/mod.rs), not as an override here.

// Small c-gull gaps that libdrm imports. Arch builds with -fno-plt, so EVERY
// import is an eager GLOB_DAT that must resolve at load - even ones this path
// never calls. Forward where c-gull has the base; stub the rest (these stubs
// are the real c-ward TODOs: getdelim, the scanf family, open_memstream).
unsafe extern "C" {
    fn strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long;
    fn realpath(path: *const c_char, resolved: *mut c_char) -> *mut c_char;
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc23_strtol(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_long {
    unsafe { strtol(s, end, base) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __realpath_chk(path: *const c_char, resolved: *mut c_char, _n: usize) -> *mut c_char {
    unsafe { realpath(path, resolved) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn open_memstream(_p: *mut *mut c_char, _n: *mut usize) -> *mut c_void {
    core::ptr::null_mut()
}
// the scanf family (incl. __isoc23_*) lives in `scanf.rs` - a real minimal impl;
// getline/getdelim/__getdelim live in `gap.rs` - a real fgetc-based impl

// -- glibc data globals the driver closure binds eagerly (R_X86_64_GLOB_DAT) --
// libudev and libc error paths read the program-name pair (in both its BSD and
// GNU spellings) for diagnostics; __libc_single_threaded gates fast-path lock
// elision - pin it to 0 (multi-threaded) so nothing skips locking under Mesa's
// worker threads. Unresolved PLT *functions* are NOT harmless in the eager
// path: elf_loader skips them silently and the first call jumps to an
// unmapped link-time address (that was the _setjmp crash) - the function-side
// gaps live in c-scape, not here.
#[unsafe(no_mangle)]
static mut program_invocation_name: *const c_char = c"carrot".as_ptr();
#[unsafe(no_mangle)]
static mut program_invocation_short_name: *const c_char = c"carrot".as_ptr();
#[unsafe(no_mangle)]
static mut __progname: *const c_char = c"carrot".as_ptr();
#[unsafe(no_mangle)]
static mut __progname_full: *const c_char = c"carrot".as_ptr();
#[unsafe(no_mangle)]
static mut __libc_single_threaded: c_char = 0;
