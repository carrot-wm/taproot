//! A minimal `sscanf`/`fscanf` (and their C23 `__isoc23_*` names).
//!
//! c-gull has no scanf family at all, and libdrm/Mesa parse the PCI
//! vendor/device IDs out of sysfs with `fscanf(fp,"%x",..)` / `sscanf`. Without
//! it every GPU is rejected. This covers the conversions they use: `d u i o x X
//! s c n %`, a `*` suppressor, field widths, and `h`/`hh`/`l`/`ll` length
//! modifiers, matching literals and skipping whitespace like C scanf.

use core::ffi::{c_char, c_int, c_void, VaList};

unsafe extern "C" {
    fn fread(ptr: *mut c_void, size: usize, nmemb: usize, stream: *mut c_void) -> usize;
}

#[inline]
unsafe fn at(p: *const c_char, i: usize) -> u8 {
    unsafe { *p.add(i) as u8 }
}
fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}
fn digit_val(b: u8, base: i32) -> Option<i32> {
    let v = match b {
        b'0'..=b'9' => (b - b'0') as i32,
        b'a'..=b'f' => (b - b'a') as i32 + 10,
        b'A'..=b'F' => (b - b'A') as i32 + 10,
        _ => return None,
    };
    if v < base { Some(v) } else { None }
}

// core parser over a NUL-terminated input string
unsafe fn vsscanf(input: *const c_char, fmt: *const c_char, mut ap: VaList) -> c_int {
    let mut ii = 0usize; // input index
    let mut fi = 0usize; // fmt index
    let mut matched: c_int = 0;
    let mut any_input = false;

    loop {
        let f = unsafe { at(fmt, fi) };
        if f == 0 {
            break;
        }
        if is_space(f) {
            while is_space(unsafe { at(fmt, fi) }) {
                fi += 1;
            }
            while unsafe { at(input, ii) } != 0 && is_space(unsafe { at(input, ii) }) {
                ii += 1;
            }
            continue;
        }
        if f != b'%' {
            // literal must match
            let c = unsafe { at(input, ii) };
            if c == 0 || c != f {
                return if matched == 0 && c == 0 && !any_input { -1 } else { matched };
            }
            ii += 1;
            fi += 1;
            any_input = true;
            continue;
        }

        // '%' directive
        fi += 1;
        let mut suppress = false;
        if unsafe { at(fmt, fi) } == b'*' {
            suppress = true;
            fi += 1;
        }
        // width
        let mut width: usize = 0;
        while let Some(d) = digit_val(unsafe { at(fmt, fi) }, 10) {
            width = width * 10 + d as usize;
            fi += 1;
        }
        // length modifier
        let mut len = 0i32; // 0=int, -1=short, -2=char, 1=long, 2=long long
        loop {
            match unsafe { at(fmt, fi) } {
                b'h' => {
                    len -= 1;
                    fi += 1;
                }
                b'l' | b'L' | b'q' | b'j' | b'z' | b't' => {
                    len += 1;
                    fi += 1;
                }
                _ => break,
            }
        }
        let conv = unsafe { at(fmt, fi) };
        fi += 1;
        let wcap = if width == 0 { usize::MAX } else { width };

        match conv {
            b'%' => {
                while unsafe { at(input, ii) } != 0 && is_space(unsafe { at(input, ii) }) {
                    ii += 1;
                }
                if unsafe { at(input, ii) } != b'%' {
                    return matched;
                }
                ii += 1;
            }
            b'n' => {
                if !suppress {
                    let p = unsafe { ap.next_arg::<*mut c_int>() };
                    unsafe { *p = ii as c_int };
                }
            }
            b'd' | b'u' | b'i' | b'o' | b'x' | b'X' | b'p' => {
                while unsafe { at(input, ii) } != 0 && is_space(unsafe { at(input, ii) }) {
                    ii += 1;
                }
                let mut base = match conv {
                    b'd' | b'u' => 10,
                    b'o' => 8,
                    b'x' | b'X' | b'p' => 16,
                    _ => 0, // 'i': auto
                };
                let mut consumed = 0usize;
                let mut neg = false;
                let start = ii;
                let c = unsafe { at(input, ii) };
                if (c == b'+' || c == b'-') && consumed < wcap {
                    neg = c == b'-';
                    ii += 1;
                    consumed += 1;
                }
                // optional 0x / base detection
                if (base == 16 || base == 0)
                    && unsafe { at(input, ii) } == b'0'
                    && matches!(unsafe { at(input, ii + 1) }, b'x' | b'X')
                {
                    ii += 2;
                    consumed += 2;
                    base = 16;
                } else if base == 0 {
                    base = if unsafe { at(input, ii) } == b'0' { 8 } else { 10 };
                }
                let mut val: u64 = 0;
                let mut got = false;
                while consumed < wcap {
                    match digit_val(unsafe { at(input, ii) }, base) {
                        Some(d) => {
                            val = val.wrapping_mul(base as u64).wrapping_add(d as u64);
                            ii += 1;
                            consumed += 1;
                            got = true;
                        }
                        None => break,
                    }
                }
                if !got {
                    ii = start;
                    return if matched == 0 { -1 } else { matched };
                }
                let signed = if neg { (val as i64).wrapping_neg() } else { val as i64 };
                if !suppress {
                    store_int(&mut ap, signed as u64, len);
                    matched += 1;
                }
                any_input = true;
            }
            b's' => {
                while unsafe { at(input, ii) } != 0 && is_space(unsafe { at(input, ii) }) {
                    ii += 1;
                }
                if unsafe { at(input, ii) } == 0 {
                    return if matched == 0 { -1 } else { matched };
                }
                let dst = if suppress {
                    core::ptr::null_mut()
                } else {
                    unsafe { ap.next_arg::<*mut c_char>() }
                };
                let mut n = 0usize;
                while n < wcap {
                    let c = unsafe { at(input, ii) };
                    if c == 0 || is_space(c) {
                        break;
                    }
                    if !dst.is_null() {
                        unsafe { *dst.add(n) = c as c_char };
                    }
                    ii += 1;
                    n += 1;
                }
                if !dst.is_null() {
                    unsafe { *dst.add(n) = 0 };
                    matched += 1;
                }
                any_input = true;
            }
            b'c' => {
                let count = if width == 0 { 1 } else { width };
                let dst = if suppress {
                    core::ptr::null_mut()
                } else {
                    unsafe { ap.next_arg::<*mut c_char>() }
                };
                let mut n = 0usize;
                while n < count {
                    let c = unsafe { at(input, ii) };
                    if c == 0 {
                        break;
                    }
                    if !dst.is_null() {
                        unsafe { *dst.add(n) = c as c_char };
                    }
                    ii += 1;
                    n += 1;
                }
                if n < count {
                    return if matched == 0 { -1 } else { matched };
                }
                if !dst.is_null() {
                    matched += 1;
                }
                any_input = true;
            }
            _ => return matched, // unsupported conversion: stop
        }
    }
    matched
}

unsafe fn store_int(ap: &mut VaList, val: u64, len: i32) {
    unsafe {
        match len {
            -2 => *ap.next_arg::<*mut i8>() = val as i8,
            -1 => *ap.next_arg::<*mut i16>() = val as i16,
            0 => *ap.next_arg::<*mut i32>() = val as i32,
            _ => *ap.next_arg::<*mut i64>() = val as i64,
        }
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn sscanf(s: *const c_char, fmt: *const c_char, args: ...) -> c_int {
    unsafe { vsscanf(s, fmt, args) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc23_sscanf(s: *const c_char, fmt: *const c_char, args: ...) -> c_int {
    unsafe { vsscanf(s, fmt, args) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc99_sscanf(s: *const c_char, fmt: *const c_char, args: ...) -> c_int {
    unsafe { vsscanf(s, fmt, args) }
}

// fscanf: read the stream into a bounded buffer, then scan it. libdrm/Mesa use
// fscanf only for one-shot reads of tiny sysfs files, so reading to the buffer
// cap and scanning is sufficient.
unsafe fn fscanf_buf(stream: *mut c_void, fmt: *const c_char, ap: VaList) -> c_int {
    let mut buf = [0u8; 4096];
    let n = unsafe { fread(buf.as_mut_ptr() as *mut c_void, 1, buf.len() - 1, stream) };
    buf[n.min(buf.len() - 1)] = 0;
    unsafe { vsscanf(buf.as_ptr() as *const c_char, fmt, ap) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn fscanf(stream: *mut c_void, fmt: *const c_char, args: ...) -> c_int {
    unsafe { fscanf_buf(stream, fmt, args) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc23_fscanf(stream: *mut c_void, fmt: *const c_char, args: ...) -> c_int {
    unsafe { fscanf_buf(stream, fmt, args) }
}
#[unsafe(no_mangle)]
unsafe extern "C" fn __isoc99_fscanf(stream: *mut c_void, fmt: *const c_char, args: ...) -> c_int {
    unsafe { fscanf_buf(stream, fmt, args) }
}
