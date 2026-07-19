// `__*_chk` functions that have to live in c-gull because they depend on
// C functions not in the libc crate, due to `VaList` being unstable.
//
// On x86_64 the variadic entries are `vararg_entry!` shims (the `va`
// walker's naked-entry protocol) and the `__v*_chk` functions take `*mut
// VaListTag` (C's `va_list` is a pointer to the tag at the ABI level, so
// the exported signatures are unchanged). Where a shim's implementation
// signature matches its `__v*_chk` function, the entry targets it
// directly. The walker is x86_64-only, so other architectures keep the
// nightly variadic definitions below.

#[cfg(target_arch = "x86_64")]
use crate::va::{vararg_entry, VaListTag};
#[cfg(not(target_arch = "x86_64"))]
use core::ffi::VaList;
use libc::{c_char, c_int, size_t};

extern "C" {
    #[cold]
    fn __chk_fail() -> !;
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---snprintf-chk-1.html>
#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn __snprintf_chk(
        ptr: *mut c_char,
        len: size_t,
        flag: c_int,
        slen: size_t,
        ...
    ) -> c_int => __snprintf_chk_impl
}

#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn __snprintf_chk_impl(
    ptr: *mut c_char,
    len: size_t,
    flag: c_int,
    slen: size_t,
    tag: *mut VaListTag,
) -> c_int {
    // C's fifth named argument `fmt` is fetched as the first walked
    // argument rather than declared named (the entry macro carries four;
    // the walk starts at r8's spill slot either way), as exec.rs does
    // for execl's first `arg`.
    let fmt = (*tag).arg::<*const c_char>();
    __vsnprintf_chk(ptr, len, flag, slen, fmt, tag)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---vsnprintf-chk-1.html>
#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vsnprintf_chk(
    ptr: *mut c_char,
    len: size_t,
    flag: c_int,
    slen: size_t,
    fmt: *const c_char,
    va_list: *mut VaListTag,
) -> c_int {
    if slen < len {
        __chk_fail();
    }

    let _ = flag; // taproot: ignore fortify level, forward to the plain formatter

    super::vsnprintf(ptr, len, fmt, va_list)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---sprintf-chk-1.html>
#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn __sprintf_chk(
        ptr: *mut c_char,
        flag: c_int,
        strlen: size_t,
        format: *const c_char,
        ...
    ) -> c_int => __vsprintf_chk
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vsprintf_chk(
    ptr: *mut c_char,
    flag: c_int,
    strlen: size_t,
    fmt: *const c_char,
    va_list: *mut VaListTag,
) -> c_int {
    let _ = flag; // taproot: same

    if strlen == 0 {
        __chk_fail();
    }

    // We can't check `vsprintf` up front, so do a `vsnprintf` and check the
    // results.
    let n = super::vsnprintf(ptr, strlen, fmt, va_list);
    if n >= 0 && n as size_t >= strlen {
        __chk_fail();
    }
    n
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---fprintf-chk-1.html>
#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn __fprintf_chk(
        file: *mut libc::FILE,
        flag: c_int,
        fmt: *const c_char,
        ...
    ) -> c_int => __vfprintf_chk
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---vfprintf-chk-1.html>
#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vfprintf_chk(
    file: *mut libc::FILE,
    flag: c_int,
    fmt: *const c_char,
    va_list: *mut VaListTag,
) -> c_int {
    let _ = flag; // taproot: same

    // Our `printf` uses `printf_compat` which doesn't support `%n`.

    super::vfprintf(file, fmt, va_list)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---printf-chk-1.html>
#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn __printf_chk(flag: c_int, fmt: *const c_char, ...) -> c_int
        => __vprintf_chk
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vprintf_chk(
    flag: c_int,
    fmt: *const c_char,
    va_list: *mut VaListTag,
) -> c_int {
    let _ = flag; // taproot: same

    // Our `printf` uses `printf_compat` which doesn't support `%n`.

    super::vprintf(fmt, va_list)
}

#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn __asprintf_chk(
        strp: *mut *mut c_char,
        flag: c_int,
        fmt: *const c_char,
        ...
    ) -> c_int => __vasprintf_chk
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vasprintf_chk(
    strp: *mut *mut c_char,
    flag: c_int,
    fmt: *const c_char,
    va_list: *mut VaListTag,
) -> c_int {
    let _ = flag; // taproot: same

    super::vasprintf(strp, fmt, va_list)
}

#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn __dprintf_chk(
        fd: c_int,
        flag: c_int,
        fmt: *const c_char,
        ...
    ) -> c_int => __vdprintf_chk
}

#[cfg(target_arch = "x86_64")]
#[no_mangle]
unsafe extern "C" fn __vdprintf_chk(
    fd: c_int,
    flag: c_int,
    fmt: *const c_char,
    va_list: *mut VaListTag,
) -> c_int {
    let _ = flag; // taproot: same

    super::vdprintf(fd, fmt, va_list)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---snprintf-chk-1.html>
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __snprintf_chk(
    ptr: *mut c_char,
    len: size_t,
    flag: c_int,
    slen: size_t,
    fmt: *const c_char,
    args: ...
) -> c_int {
    __vsnprintf_chk(ptr, len, flag, slen, fmt, args)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---vsnprintf-chk-1.html>
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vsnprintf_chk(
    ptr: *mut c_char,
    len: size_t,
    flag: c_int,
    slen: size_t,
    fmt: *const c_char,
    va_list: VaList<'_>,
) -> c_int {
    if slen < len {
        __chk_fail();
    }

    let _ = flag; // taproot: ignore fortify level, forward to the plain formatter

    super::vsnprintf(ptr, len, fmt, va_list)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---sprintf-chk-1.html>
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __sprintf_chk(
    ptr: *mut c_char,
    flag: c_int,
    strlen: size_t,
    format: *const c_char,
    args: ...
) -> c_int {
    __vsprintf_chk(ptr, flag, strlen, format, args)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vsprintf_chk(
    ptr: *mut c_char,
    flag: c_int,
    strlen: size_t,
    fmt: *const c_char,
    va_list: VaList<'_>,
) -> c_int {
    let _ = flag; // taproot: same

    if strlen == 0 {
        __chk_fail();
    }

    // We can't check `vsprintf` up front, so do a `vsnprintf` and check the
    // results.
    let n = super::vsnprintf(ptr, strlen, fmt, va_list);
    if n >= 0 && n as size_t >= strlen {
        __chk_fail();
    }
    n
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---fprintf-chk-1.html>
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __fprintf_chk(
    file: *mut libc::FILE,
    flag: c_int,
    fmt: *const c_char,
    args: ...
) -> c_int {
    __vfprintf_chk(file, flag, fmt, args)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---vfprintf-chk-1.html>
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vfprintf_chk(
    file: *mut libc::FILE,
    flag: c_int,
    fmt: *const c_char,
    va_list: VaList<'_>,
) -> c_int {
    let _ = flag; // taproot: same

    // Our `printf` uses `printf_compat` which doesn't support `%n`.

    super::vfprintf(file, fmt, va_list)
}

// <https://refspecs.linuxbase.org/LSB_5.0.0/LSB-Core-generic/LSB-Core-generic/baselib---printf-chk-1.html>
#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __printf_chk(flag: c_int, fmt: *const c_char, args: ...) -> c_int {
    __vprintf_chk(flag, fmt, args)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vprintf_chk(flag: c_int, fmt: *const c_char, va_list: VaList<'_>) -> c_int {
    let _ = flag; // taproot: same

    // Our `printf` uses `printf_compat` which doesn't support `%n`.

    super::vprintf(fmt, va_list)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __asprintf_chk(
    strp: *mut *mut c_char,
    flag: c_int,
    fmt: *const c_char,
    args: ...
) -> c_int {
    __vasprintf_chk(strp, flag, fmt, args)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vasprintf_chk(
    strp: *mut *mut c_char,
    flag: c_int,
    fmt: *const c_char,
    va_list: VaList<'_>,
) -> c_int {
    let _ = flag; // taproot: same

    super::vasprintf(strp, fmt, va_list)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __dprintf_chk(fd: c_int, flag: c_int, fmt: *const c_char, args: ...) -> c_int {
    __vdprintf_chk(fd, flag, fmt, args)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn __vdprintf_chk(
    fd: c_int,
    flag: c_int,
    fmt: *const c_char,
    va_list: VaList<'_>,
) -> c_int {
    let _ = flag; // taproot: same

    super::vdprintf(fd, fmt, va_list)
}
