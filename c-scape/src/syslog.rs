//! Syslog.
//!
//! taproot: mesa and libudev import these. A dlopened libc.so.6 serving a
//! compositor has no syslog connection worth opening, so entries are
//! accepted and dropped; only the mask round-trips.

#[cfg(target_arch = "x86_64")]
use crate::va::VaListTag;
#[cfg(not(target_arch = "x86_64"))]
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

// The bodies drop every entry, so the variadic arguments were never read:
// `syslog` and `__syslog_chk` can be fixed arity (variadic and fixed callees
// receive the named INTEGER-class arguments identically on SysV x86_64), and,
// on x86_64, the `va_list` parameters become `*mut VaListTag` (C's `va_list`
// is a pointer to the tag at the ABI level, so the exported signature is
// unchanged). The `va` walker is x86_64-only, so other architectures keep
// `vsyslog`/`__vsyslog_chk` on the nightly `core::ffi::VaList` parameter
// that predates the walker.

#[no_mangle]
unsafe extern "C" fn syslog(_priority: c_int, _format: *const c_char) {}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn vsyslog(_priority: c_int, _format: *const c_char, _va_list: *mut VaListTag) {}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn vsyslog(_priority: c_int, _format: *const c_char, _va_list: VaList<'_>) {}

#[no_mangle]
unsafe extern "C" fn __syslog_chk(_priority: c_int, _flag: c_int, _format: *const c_char) {}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vsyslog_chk(
    _priority: c_int,
    _flag: c_int,
    _format: *const c_char,
    _va_list: *mut VaListTag,
) {
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vsyslog_chk(
    _priority: c_int,
    _flag: c_int,
    _format: *const c_char,
    _va_list: VaList<'_>,
) {
}
