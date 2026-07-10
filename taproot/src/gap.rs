//! The libc surface Mesa's C++ closure (libstdc++) reaches that c-gull doesn't
//! implement: the POSIX/"C"-locale, wide-char, and i18n functions libstdc++'s
//! static initializers call while building `std::locale::classic()`. c-gull is a
//! minimal libc; these are thin, C-locale-correct fills. Every symbol here is a
//! JUMP_SLOT import (the loader skips undefined ones - they only matter once
//! actually called), so this covers exactly the init/render call path, not all
//! of glibc. Upstream candidates for c-ward.

use core::ffi::{c_char, c_double, c_float, c_int, c_long, c_ulong, c_void};

#[allow(non_camel_case_types)]
type wchar_t = i32;
#[allow(non_camel_case_types)]
type wint_t = u32;
#[allow(non_camel_case_types)]
type locale_t = *mut c_void;
#[allow(non_camel_case_types)]
type wctype_t = c_ulong;
#[allow(non_camel_case_types)]
type nl_item = c_int;

const WEOF: wint_t = 0xffff_ffff;
const EOF: c_int = -1;

// base fns c-gull already provides, forwarded to for C-locale behavior.
unsafe extern "C" {
    fn strtod(s: *const c_char, end: *mut *mut c_char) -> c_double;
    fn strtof(s: *const c_char, end: *mut *mut c_char) -> c_float;
    fn strtoul(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_ulong;
    fn strxfrm(dst: *mut c_char, src: *const c_char, n: usize) -> usize;
    fn strcmp(a: *const c_char, b: *const c_char) -> c_int;
    fn getrandom(buf: *mut c_void, len: usize, flags: u32) -> isize;
}

// -- locale objects ------------------------------------------------------------
// One immutable sentinel stands in for the only locale we support ("C"). All the
// *_l functions ignore the handle and act C-locale, so any stable non-null
// pointer works; freelocale is a no-op, uselocale just echoes the sentinel.
static C_LOCALE: u8 = 0;
#[inline]
fn c_locale() -> locale_t {
    &C_LOCALE as *const u8 as locale_t
}

#[unsafe(no_mangle)]
extern "C" fn newlocale(_mask: c_int, _name: *const c_char, _base: locale_t) -> locale_t {
    c_locale()
}
#[unsafe(no_mangle)]
extern "C" fn __newlocale(_mask: c_int, _name: *const c_char, _base: locale_t) -> locale_t {
    c_locale()
}
#[unsafe(no_mangle)]
extern "C" fn freelocale(_loc: locale_t) {}
#[unsafe(no_mangle)]
extern "C" fn __freelocale(_loc: locale_t) {}
#[unsafe(no_mangle)]
extern "C" fn uselocale(_loc: locale_t) -> locale_t {
    c_locale()
}
#[unsafe(no_mangle)]
extern "C" fn __uselocale(_loc: locale_t) -> locale_t {
    c_locale()
}
#[unsafe(no_mangle)]
extern "C" fn __duplocale(_loc: locale_t) -> locale_t {
    c_locale()
}

// -- langinfo ------------------------------------------------------------------
// C locale: CODESET is ASCII; everything else empty. libstdc++ reads CODESET to
// pick the wchar converter; "ANSI_X3.4-1968" tells it plain ASCII.
const CODESET: nl_item = 14;
static CODESET_C: &[u8] = b"ANSI_X3.4-1968\0";
static EMPTY: &[u8] = b"\0";
#[inline]
fn langinfo(item: nl_item) -> *mut c_char {
    let s = if item == CODESET { CODESET_C } else { EMPTY };
    s.as_ptr() as *mut c_char
}
#[unsafe(no_mangle)]
extern "C" fn nl_langinfo(item: nl_item) -> *mut c_char {
    langinfo(item)
}
#[unsafe(no_mangle)]
extern "C" fn nl_langinfo_l(item: nl_item, _loc: locale_t) -> *mut c_char {
    langinfo(item)
}
#[unsafe(no_mangle)]
extern "C" fn __nl_langinfo_l(item: nl_item, _loc: locale_t) -> *mut c_char {
    langinfo(item)
}

// -- ctype (C locale, ASCII) ---------------------------------------------------
#[unsafe(no_mangle)]
extern "C" fn __towlower_l(wc: wint_t, _loc: locale_t) -> wint_t {
    if (b'A' as wint_t..=b'Z' as wint_t).contains(&wc) { wc + 32 } else { wc }
}
#[unsafe(no_mangle)]
extern "C" fn __towupper_l(wc: wint_t, _loc: locale_t) -> wint_t {
    if (b'a' as wint_t..=b'z' as wint_t).contains(&wc) { wc - 32 } else { wc }
}

// wctype names -> a small class id; iswctype decodes it. The exact ids are
// private to this pair, so any stable mapping works.
const CLASSES: &[(&[u8], wctype_t)] = &[
    (b"alnum\0", 1), (b"alpha\0", 2), (b"cntrl\0", 3), (b"digit\0", 4),
    (b"graph\0", 5), (b"lower\0", 6), (b"print\0", 7), (b"punct\0", 8),
    (b"space\0", 9), (b"upper\0", 10), (b"xdigit\0", 11), (b"blank\0", 12),
];
#[unsafe(no_mangle)]
unsafe extern "C" fn __wctype_l(name: *const c_char, _loc: locale_t) -> wctype_t {
    if name.is_null() {
        return 0;
    }
    for (n, id) in CLASSES {
        if unsafe { strcmp(name, n.as_ptr() as *const c_char) } == 0 {
            return *id;
        }
    }
    0
}
#[unsafe(no_mangle)]
extern "C" fn __iswctype_l(wc: wint_t, desc: wctype_t, _loc: locale_t) -> c_int {
    if wc > 0x7f {
        return 0; // C locale: non-ASCII is unclassified
    }
    let b = wc as u8;
    let r = match desc {
        1 => b.is_ascii_alphanumeric(),
        2 => b.is_ascii_alphabetic(),
        3 => b.is_ascii_control(),
        4 => b.is_ascii_digit(),
        5 => b.is_ascii_graphic(),
        6 => b.is_ascii_lowercase(),
        7 => b.is_ascii_graphic() || b == b' ',
        8 => b.is_ascii_punctuation(),
        9 => b.is_ascii_whitespace() || b == 0x0b || b == 0x0c,
        10 => b.is_ascii_uppercase(),
        11 => b.is_ascii_hexdigit(),
        12 => b == b' ' || b == b'\t',
        _ => false,
    };
    r as c_int
}

// -- collation / transform (C locale = byte order, identity transform) ---------
#[unsafe(no_mangle)]
unsafe extern "C" fn __strcoll_l(a: *const c_char, b: *const c_char, _loc: locale_t) -> c_int {
    unsafe { strcmp(a, b) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __strxfrm_l(dst: *mut c_char, src: *const c_char, n: usize, _loc: locale_t) -> usize {
    unsafe { strxfrm(dst, src, n) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __wcscoll_l(a: *const wchar_t, b: *const wchar_t, _loc: locale_t) -> c_int {
    unsafe { wcscmp(a, b) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __wcsxfrm_l(dst: *mut wchar_t, src: *const wchar_t, n: usize, _loc: locale_t) -> usize {
    let len = unsafe { wcslen(src) };
    if n != 0 && !dst.is_null() {
        let copy = core::cmp::min(len, n - 1);
        unsafe {
            core::ptr::copy_nonoverlapping(src, dst, copy);
            *dst.add(copy) = 0;
        }
    }
    len
}

// -- number parsing (C locale) -------------------------------------------------
#[unsafe(no_mangle)]
unsafe extern "C" fn __strtod_l(s: *const c_char, end: *mut *mut c_char, _loc: locale_t) -> c_double {
    unsafe { strtod(s, end) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __strtof_l(s: *const c_char, end: *mut *mut c_char, _loc: locale_t) -> c_float {
    unsafe { strtof(s, end) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc23_strtoul(s: *const c_char, end: *mut *mut c_char, base: c_int) -> c_ulong {
    unsafe { strtoul(s, end, base) }
}

// -- wide char string/mem (C locale, single-byte) ------------------------------
#[unsafe(no_mangle)]
unsafe extern "C" fn wcslen(s: *const wchar_t) -> usize {
    let mut n = 0;
    while unsafe { *s.add(n) } != 0 {
        n += 1;
    }
    n
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wcscmp(a: *const wchar_t, b: *const wchar_t) -> c_int {
    let mut i = 0;
    loop {
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x != y {
            return if x < y { -1 } else { 1 };
        }
        if x == 0 {
            return 0;
        }
        i += 1;
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wmemchr(s: *const wchar_t, c: wchar_t, n: usize) -> *mut wchar_t {
    for i in 0..n {
        if unsafe { *s.add(i) } == c {
            return unsafe { s.add(i) } as *mut wchar_t;
        }
    }
    core::ptr::null_mut()
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wmemcmp(a: *const wchar_t, b: *const wchar_t, n: usize) -> c_int {
    for i in 0..n {
        let (x, y) = unsafe { (*a.add(i), *b.add(i)) };
        if x != y {
            return if x < y { -1 } else { 1 };
        }
    }
    0
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wmemcpy(d: *mut wchar_t, s: *const wchar_t, n: usize) -> *mut wchar_t {
    unsafe { core::ptr::copy_nonoverlapping(s, d, n) };
    d
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wmemmove(d: *mut wchar_t, s: *const wchar_t, n: usize) -> *mut wchar_t {
    unsafe { core::ptr::copy(s, d, n) };
    d
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wmemset(d: *mut wchar_t, c: wchar_t, n: usize) -> *mut wchar_t {
    for i in 0..n {
        unsafe { *d.add(i) = c };
    }
    d
}

// -- multibyte <-> wide (C locale: one byte == one wchar, values 0..256) -------
#[unsafe(no_mangle)]
extern "C" fn btowc(c: c_int) -> wint_t {
    if c == EOF { WEOF } else { (c as u8) as wint_t }
}
#[unsafe(no_mangle)]
extern "C" fn wctob(wc: wint_t) -> c_int {
    if wc < 0x100 { wc as c_int } else { EOF }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mbrtowc(pwc: *mut wchar_t, s: *const c_char, n: usize, _ps: *mut c_void) -> usize {
    if s.is_null() {
        return 0; // query: C locale is stateless
    }
    if n == 0 {
        return usize::MAX - 1; // (size_t)-2: incomplete
    }
    let b = unsafe { *s } as u8;
    if !pwc.is_null() {
        unsafe { *pwc = b as wchar_t };
    }
    if b == 0 { 0 } else { 1 }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wcrtomb(s: *mut c_char, wc: wchar_t, _ps: *mut c_void) -> usize {
    if s.is_null() {
        return 1;
    }
    unsafe { *s = wc as c_char };
    1
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mbsrtowcs(dst: *mut wchar_t, src: *mut *const c_char, len: usize, _ps: *mut c_void) -> usize {
    let mut p = unsafe { *src };
    let mut i = 0;
    loop {
        let b = unsafe { *p } as u8;
        if !dst.is_null() {
            if i >= len {
                unsafe { *src = p };
                return i;
            }
            unsafe { *dst.add(i) = b as wchar_t };
        }
        if b == 0 {
            if !dst.is_null() {
                unsafe { *src = core::ptr::null() };
            }
            return i;
        }
        p = unsafe { p.add(1) };
        i += 1;
    }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn mbsnrtowcs(dst: *mut wchar_t, src: *mut *const c_char, nmc: usize, len: usize, _ps: *mut c_void) -> usize {
    let mut p = unsafe { *src };
    let mut i = 0;
    let mut left = nmc;
    while left > 0 {
        let b = unsafe { *p } as u8;
        if !dst.is_null() {
            if i >= len {
                unsafe { *src = p };
                return i;
            }
            unsafe { *dst.add(i) = b as wchar_t };
        }
        if b == 0 {
            if !dst.is_null() {
                unsafe { *src = core::ptr::null() };
            }
            return i;
        }
        p = unsafe { p.add(1) };
        i += 1;
        left -= 1;
    }
    if !dst.is_null() {
        unsafe { *src = p };
    }
    i
}
#[unsafe(no_mangle)]
unsafe extern "C" fn wcsnrtombs(dst: *mut c_char, src: *mut *const wchar_t, nwc: usize, len: usize, _ps: *mut c_void) -> usize {
    let mut p = unsafe { *src };
    let mut i = 0;
    let mut left = nwc;
    while left > 0 {
        let wc = unsafe { *p };
        if !dst.is_null() {
            if i >= len {
                unsafe { *src = p };
                return i;
            }
            unsafe { *dst.add(i) = wc as c_char };
        }
        if wc == 0 {
            if !dst.is_null() {
                unsafe { *src = core::ptr::null() };
            }
            return i;
        }
        p = unsafe { p.add(1) };
        i += 1;
        left -= 1;
    }
    if !dst.is_null() {
        unsafe { *src = p };
    }
    i
}

// -- i18n (no catalogs: identity) ----------------------------------------------
#[unsafe(no_mangle)]
extern "C" fn gettext(msgid: *const c_char) -> *mut c_char {
    msgid as *mut c_char
}
#[unsafe(no_mangle)]
extern "C" fn dgettext(_domain: *const c_char, msgid: *const c_char) -> *mut c_char {
    msgid as *mut c_char
}
#[unsafe(no_mangle)]
extern "C" fn bindtextdomain(_domain: *const c_char, _dir: *const c_char) -> *mut c_char {
    core::ptr::null_mut()
}
#[unsafe(no_mangle)]
extern "C" fn bind_textdomain_codeset(_domain: *const c_char, _codeset: *const c_char) -> *mut c_char {
    core::ptr::null_mut()
}

// -- misc ----------------------------------------------------------------------
#[unsafe(no_mangle)]
extern "C" fn arc4random() -> u32 {
    let mut v: u32 = 0;
    let p = &mut v as *mut u32 as *mut c_void;
    // blocking read is fine here; 4 bytes never short-reads.
    unsafe { getrandom(p, 4, 0) };
    v
}
#[unsafe(no_mangle)]
unsafe extern "C" fn arc4random_buf(buf: *mut c_void, n: usize) {
    let mut done = 0usize;
    while done < n {
        let r = unsafe { getrandom(buf.byte_add(done), n - done, 0) };
        if r <= 0 {
            break;
        }
        done += r as usize;
    }
}
#[unsafe(no_mangle)]
extern "C" fn fegetround() -> c_int {
    0 // FE_TONEAREST
}
#[unsafe(no_mangle)]
extern "C" fn fesetround(_mode: c_int) -> c_int {
    0
}
// get_phys_pages / get_avphys_pages / getpagesize / pthread_condattr_setclock /
// pthread_setname_np are fixed in the vendored c-scape fork now, not overridden
// here. What remains below is purely additive - functions c-scape lacks.

// Thread scheduling c-gull lacks. ANV sets a worker thread's priority; it's an
// optimization, so succeed without changing anything (default priority).
#[unsafe(no_mangle)]
extern "C" fn pthread_setschedparam(_t: usize, _policy: c_int, _param: *const c_void) -> c_int {
    0
}
#[unsafe(no_mangle)]
extern "C" fn pthread_getschedparam(_t: usize, policy: *mut c_int, _param: *mut c_void) -> c_int {
    if !policy.is_null() {
        unsafe { *policy = 0 }; // SCHED_OTHER
    }
    0
}
#[unsafe(no_mangle)]
extern "C" fn pthread_setschedprio(_t: usize, _prio: c_int) -> c_int {
    0
}
// ANV reads CPU affinity to size its worker pool - this must be real, or the CPU
// count is wrong. Back it with sched_getaffinity(2) on the calling thread (0).
#[unsafe(no_mangle)]
unsafe extern "C" fn pthread_getaffinity_np(_t: usize, cpusetsize: usize, cpuset: *mut c_void) -> c_int {
    let ret = unsafe { syscall3(204, 0, cpusetsize as isize, cpuset as isize) }; // SYS_sched_getaffinity
    if ret < 0 { (-ret) as c_int } else { 0 }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn pthread_setaffinity_np(_t: usize, cpusetsize: usize, cpuset: *const c_void) -> c_int {
    let ret = unsafe { syscall3(203, 0, cpusetsize as isize, cpuset as isize) }; // SYS_sched_setaffinity
    if ret < 0 { (-ret) as c_int } else { 0 }
}

// dladdr (Mesa uses it to find its own install dir) lives in `dl.rs` - a real
// impl backed by /proc/self/maps, since we load via dlopen-rs not glibc.

// scandir comparator the Intel ICD takes the address of (GLOB_DAT, so it must
// resolve at load). C locale: order by d_name (offset 19 in struct dirent64:
// u64 d_ino, u64 d_off, u16 d_reclen, u8 d_type, then char d_name[]).
#[unsafe(no_mangle)]
unsafe extern "C" fn alphasort64(a: *const *const c_void, b: *const *const c_void) -> c_int {
    let na = unsafe { (*a as *const c_char).add(19) };
    let nb = unsafe { (*b as *const c_char).add(19) };
    unsafe { strcmp(na, nb) }
}

// Mesa enumerates DRM nodes with scandir64: read the directory, apply the
// caller's filter, dup each surviving dirent64 (malloc), sort with the caller's
// comparator, hand back the array. c-gull has no directory-stream layer, so this
// goes straight to getdents64. dirent64 d_reclen is at offset 16 (u16).
type Filter = Option<unsafe extern "C" fn(*const c_void) -> c_int>;
type Compar = Option<unsafe extern "C" fn(*const *const c_void, *const *const c_void) -> c_int>;

unsafe extern "C" {
    fn malloc(n: usize) -> *mut c_void;
    fn realloc(p: *mut c_void, n: usize) -> *mut c_void;
    fn free(p: *mut c_void);
    fn getc(stream: *mut c_void) -> c_int;
    fn strtoll(s: *const c_char, end: *mut *mut c_char, base: c_int) -> i64;
    fn strtoull(s: *const c_char, end: *mut *mut c_char, base: c_int) -> u64;
}

// C23 renamed the *scanf/strto* integer parsers to __isoc23_*; forward the
// wide ones to c-gull's base (strtol/strtoul are handled elsewhere).
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc23_strtoll(s: *const c_char, end: *mut *mut c_char, base: c_int) -> i64 {
    unsafe { strtoll(s, end, base) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc23_strtoull(s: *const c_char, end: *mut *mut c_char, base: c_int) -> u64 {
    unsafe { strtoull(s, end, base) }
}

// getline/getdelim: c-scape has none, and libdrm reads the PCI uevent line with
// getline() (via __getdelim). Read char-by-char with c-gull's getc, growing the
// caller's buffer. Returns bytes read (incl. delim, excl. NUL), or -1 at EOF.
#[unsafe(no_mangle)]
unsafe extern "C" fn __getdelim(
    lineptr: *mut *mut c_char,
    n: *mut usize,
    delim: c_int,
    stream: *mut c_void,
) -> isize {
    if lineptr.is_null() || n.is_null() || stream.is_null() {
        return -1;
    }
    let mut buf = unsafe { *lineptr };
    let mut cap = unsafe { *n };
    if buf.is_null() || cap == 0 {
        cap = 128;
        buf = unsafe { malloc(cap) } as *mut c_char;
        if buf.is_null() {
            return -1;
        }
    }
    let mut len = 0usize;
    loop {
        let c = unsafe { getc(stream) };
        if c < 0 {
            break; // EOF or error
        }
        if len + 2 > cap {
            cap *= 2;
            let nb = unsafe { realloc(buf as *mut c_void, cap) } as *mut c_char;
            if nb.is_null() {
                return -1;
            }
            buf = nb;
        }
        unsafe { *buf.add(len) = c as c_char };
        len += 1;
        if c == delim {
            break;
        }
    }
    unsafe {
        *lineptr = buf;
        *n = cap;
    }
    if len == 0 {
        return -1; // nothing read before EOF
    }
    unsafe { *buf.add(len) = 0 };
    len as isize
}
#[unsafe(no_mangle)]
unsafe extern "C" fn getdelim(
    lineptr: *mut *mut c_char,
    n: *mut usize,
    delim: c_int,
    stream: *mut c_void,
) -> isize {
    unsafe { __getdelim(lineptr, n, delim, stream) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn getline(lineptr: *mut *mut c_char, n: *mut usize, stream: *mut c_void) -> isize {
    unsafe { __getdelim(lineptr, n, b'\n' as c_int, stream) }
}

#[inline]
unsafe fn syscall3(n: isize, a: isize, b: isize, c: isize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!("syscall",
            inlateout("rax") n => ret, in("rdi") a, in("rsi") b, in("rdx") c,
            out("rcx") _, out("r11") _, options(nostack, preserves_flags));
    }
    ret
}

#[unsafe(no_mangle)]
unsafe extern "C" fn scandir64(
    dirp: *const c_char,
    namelist: *mut *mut *mut c_void,
    filter: Filter,
    compar: Compar,
) -> c_int {
    const SYS_OPENAT: isize = 257;
    const SYS_GETDENTS64: isize = 217;
    const SYS_CLOSE: isize = 3;
    const AT_FDCWD: isize = -100;
    const FLAGS: isize = 0x1_0000 | 0x8_0000; // O_DIRECTORY | O_CLOEXEC

    let fd = unsafe { syscall3(SYS_OPENAT, AT_FDCWD, dirp as isize, FLAGS) };
    if fd < 0 {
        return -1;
    }
    let mut list: *mut *mut c_void = core::ptr::null_mut();
    let mut count: usize = 0;
    let mut cap: usize = 0;
    let mut buf = [0u8; 32768];
    let ok = loop {
        let n = unsafe {
            syscall3(SYS_GETDENTS64, fd, buf.as_mut_ptr() as isize, buf.len() as isize)
        };
        if n < 0 {
            break false;
        }
        if n == 0 {
            break true;
        }
        let mut off = 0usize;
        while off < n as usize {
            let ent = unsafe { buf.as_ptr().add(off) };
            let reclen = unsafe { *(ent.add(16) as *const u16) } as usize;
            let keep = match filter {
                Some(f) => unsafe { f(ent as *const c_void) != 0 },
                None => true,
            };
            if keep {
                let copy = unsafe { malloc(reclen) } as *mut u8;
                if copy.is_null() {
                    break;
                }
                unsafe { core::ptr::copy_nonoverlapping(ent, copy, reclen) };
                if count == cap {
                    cap = if cap == 0 { 16 } else { cap * 2 };
                    let grown = unsafe {
                        realloc(list as *mut c_void, cap * core::mem::size_of::<*mut c_void>())
                    } as *mut *mut c_void;
                    if grown.is_null() {
                        unsafe { free(copy as *mut c_void) };
                        break;
                    }
                    list = grown;
                }
                unsafe { *list.add(count) = copy as *mut c_void };
                count += 1;
            }
            off += reclen;
        }
    };
    unsafe { syscall3(SYS_CLOSE, fd, 0, 0) };
    if !ok {
        for i in 0..count {
            unsafe { free(*list.add(i)) };
        }
        unsafe { free(list as *mut c_void) };
        return -1;
    }
    // insertion sort the array with the caller's comparator (compar gets &elem)
    if let Some(cmp) = compar {
        for i in 1..count {
            let mut j = i;
            while j > 0 {
                let a = unsafe { list.add(j - 1) };
                let b = unsafe { list.add(j) };
                if unsafe { cmp(a as *const *const c_void, b as *const *const c_void) <= 0 } {
                    break;
                }
                unsafe { core::ptr::swap(a, b) };
                j -= 1;
            }
        }
    }
    unsafe { *namelist = list };
    count as c_int
}

#[unsafe(no_mangle)]
unsafe extern "C" fn truncate(path: *const c_char, length: c_long) -> c_int {
    let ret: isize;
    unsafe {
        core::arch::asm!(
            "syscall",
            inlateout("rax") 76isize => ret,   // SYS_truncate = 76 (x86_64)
            in("rdi") path,
            in("rsi") length,
            out("rcx") _, out("r11") _,
            options(nostack, preserves_flags),
        );
    }
    ret as c_int
}
