//! Syslog.
//!
//! taproot: mesa and libudev import these. A dlopened libc.so.6 serving a
//! compositor has no syslog connection worth opening, so entries are
//! accepted and dropped; only the mask round-trips.

use core::ffi::VaList;
use core::sync::atomic::{AtomicI32, Ordering};
use libc::{c_char, c_int};

#[no_mangle]
unsafe extern "C" fn openlog(_ident: *const c_char, _option: c_int, _facility: c_int) {}

#[no_mangle]
unsafe extern "C" fn closelog() {}

#[no_mangle]
unsafe extern "C" fn setlogmask(mask: c_int) -> c_int {
    static MASK: AtomicI32 = AtomicI32::new(0xff);
    if mask == 0 {
        MASK.load(Ordering::Relaxed)
    } else {
        MASK.swap(mask, Ordering::Relaxed)
    }
}

#[no_mangle]
unsafe extern "C" fn syslog(_priority: c_int, _format: *const c_char, _args: ...) {}

#[no_mangle]
unsafe extern "C" fn vsyslog(_priority: c_int, _format: *const c_char, _va_list: VaList<'_>) {}

#[no_mangle]
unsafe extern "C" fn __syslog_chk(
    _priority: c_int,
    _flag: c_int,
    _format: *const c_char,
    _args: ...
) {
}

#[no_mangle]
unsafe extern "C" fn __vsyslog_chk(
    _priority: c_int,
    _flag: c_int,
    _format: *const c_char,
    _va_list: VaList<'_>,
) {
}
