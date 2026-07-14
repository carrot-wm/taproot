//! Loose glibc surface that heavy prebuilt closures (gpu drivers and
//! their dependency chains) import: double-underscore variable aliases
//! and small honest functions.

use libc::{c_char, c_int, size_t};

// glibc's double-underscore variable names. in a cdylib whose origin
// startup never runs, the un-prefixed originals are inert storage too,
// so plain same-shaped statics are equally honest here.
#[no_mangle]
static mut __environ: *mut *mut c_char = core::ptr::null_mut();
#[no_mangle]
static mut __daylight: c_int = 0;
#[no_mangle]
static mut __timezone: libc::c_long = 0;
#[no_mangle]
static mut __tzname: [*mut c_char; 2] = [core::ptr::null_mut(); 2];


#[no_mangle]
unsafe extern "C" fn gettid() -> c_int {
    rustix::thread::gettid().as_raw_nonzero().get()
}

/// glibc's sigset_t is 1024 bits; empty means every word is zero
#[no_mangle]
unsafe extern "C" fn sigisemptyset(set: *const libc::sigset_t) -> c_int {
    let words = set.cast::<u64>();
    for i in 0..(core::mem::size_of::<libc::sigset_t>() / 8) {
        if *words.add(i) != 0 {
            return 0;
        }
    }
    1
}

/// "nothing was released" is always a truthful answer
#[no_mangle]
unsafe extern "C" fn malloc_trim(_pad: size_t) -> c_int {
    0
}

/// null is the documented "unknown errno" answer
#[no_mangle]
unsafe extern "C" fn strerrorname_np(_errnum: c_int) -> *const c_char {
    core::ptr::null()
}

/// single-byte answers: the C locale is the only locale we speak
#[no_mangle]
unsafe extern "C" fn mblen(s: *const c_char, n: size_t) -> c_int {
    if s.is_null() {
        return 0;
    }
    if n == 0 {
        return -1;
    }
    if *s == 0 {
        0
    } else {
        1
    }
}

/// gettext without catalogs: the message id is the translation
#[no_mangle]
unsafe extern "C" fn dcgettext(
    _domain: *const c_char,
    msgid: *const c_char,
    _category: c_int,
) -> *const c_char {
    msgid
}

/// stdio locking is always internal here; report exactly that
/// (FSETLOCKING_INTERNAL, which the libc crate does not name)
#[no_mangle]
unsafe extern "C" fn __fsetlocking(_file: *mut libc::FILE, _kind: c_int) -> c_int {
    1
}

// fortify probes: glibc's _2 variants only add an O_CREAT argument
// check before forwarding
#[no_mangle]
unsafe extern "C" fn __open64_2(path: *const c_char, flags: c_int) -> c_int {
    libc::open(path, flags)
}

#[no_mangle]
unsafe extern "C" fn __openat64_2(fd: c_int, path: *const c_char, flags: c_int) -> c_int {
    libc::openat(fd, path, flags)
}
