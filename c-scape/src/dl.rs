//! taproot: `dladdr` and `dl_iterate_phdr` from `/proc/self/maps`. With no
//! dynamic linker to ask (static-PIE main, everything else arrives via an
//! external loader), the address space itself is the link map: a file-backed
//! mapping is a loaded object and its lowest mapping is the load base. Mesa's
//! build-id walk, std's backtrace and `unwinding`'s FDE search resolve here.

use alloc::vec;
use alloc::vec::Vec;
use core::ffi::CStr;
use core::ptr::null_mut;
use libc::{c_char, c_int, c_void, size_t};
use rustix::fs::{Mode, OFlags};

fn read_self_maps() -> Option<(Vec<u8>, usize)> {
    let fd = rustix::fs::open(
        unsafe { CStr::from_bytes_with_nul_unchecked(b"/proc/self/maps\0") },
        OFlags::RDONLY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    // fixed cap, one spare byte so the path NUL-patch below stays in bounds
    let mut buf = vec![0u8; 1 << 20];
    let mut filled = 0usize;
    while filled < buf.len() - 1 {
        match rustix::io::read(&fd, &mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => break,
        }
    }
    Some((buf, filled))
}

fn hexval(b: u8) -> Option<usize> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as usize),
        b'a'..=b'f' => Some((b - b'a') as usize + 10),
        b'A'..=b'F' => Some((b - b'A') as usize + 10),
        _ => None,
    }
}

// one `/proc/self/maps` line: range, readability, and the path field (if any)
struct Line {
    start: usize,
    end: usize,
    readable: bool,
    path: Option<(usize, usize)>, // (offset, len) into the buffer
}

fn parse_line(d: &[u8], i: &mut usize) -> Option<Line> {
    if *i >= d.len() {
        return None;
    }
    let mut start = 0usize;
    let mut got = false;
    while *i < d.len() {
        if let Some(v) = hexval(d[*i]) {
            start = start * 16 + v;
            *i += 1;
            got = true;
        } else {
            break;
        }
    }
    if !got || *i >= d.len() || d[*i] != b'-' {
        // resync to the next line
        while *i < d.len() && d[*i] != b'\n' {
            *i += 1;
        }
        if *i < d.len() {
            *i += 1;
        }
        return Some(Line {
            start: 0,
            end: 0,
            readable: false,
            path: None,
        });
    }
    *i += 1; // '-'
    let mut end = 0usize;
    while *i < d.len() {
        if let Some(v) = hexval(d[*i]) {
            end = end * 16 + v;
            *i += 1;
        } else {
            break;
        }
    }
    // "start-end rwxp ..." - the perms field follows one space
    let readable = *i + 1 < d.len() && d[*i] == b' ' && d[*i + 1] == b'r';
    // find the newline; the path is the first '/'.. before it
    let mut j = *i;
    while j < d.len() && d[j] != b'\n' {
        j += 1;
    }
    let mut ps = *i;
    while ps < j && d[ps] != b'/' {
        ps += 1;
    }
    let path = if ps < j { Some((ps, j - ps)) } else { None };
    *i = if j < d.len() { j + 1 } else { j };
    Some(Line {
        start,
        end,
        readable,
        path,
    })
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn dladdr(addr: *const c_void, info: *mut libc::Dl_info) -> c_int {
    libc!(libc::dladdr(addr, info));

    if info.is_null() {
        return 0;
    }
    (*info).dli_fname = core::ptr::null();
    (*info).dli_fbase = null_mut();
    (*info).dli_sname = core::ptr::null();
    (*info).dli_saddr = null_mut();
    let target = addr as usize;

    let (buf, filled) = match read_self_maps() {
        Some(x) => x,
        None => return 0,
    };
    let d = &buf[..filled];

    // pass 1: the line containing target -> its path
    let mut target_path: Option<(usize, usize)> = None;
    let mut i = 0usize;
    while let Some(l) = parse_line(d, &mut i) {
        if l.start <= target && target < l.end {
            if let Some(p) = l.path {
                target_path = Some(p);
            }
            break;
        }
    }
    let (po, pl) = match target_path {
        Some(x) => x,
        None => return 0,
    };

    // pass 2: lowest mapping start for that same path -> load base
    let mut base = usize::MAX;
    let mut i = 0usize;
    while let Some(l) = parse_line(d, &mut i) {
        if let Some((qo, ql)) = l.path {
            if ql == pl && d[qo..qo + ql] == d[po..po + pl] && l.start < base {
                base = l.start;
            }
        }
    }
    if base == usize::MAX {
        base = target;
    }

    // copy the path so it outlives this call (glibc's dli_fname is stable and
    // never freed by callers)
    let name = libc::malloc(pl + 1) as *mut u8;
    if name.is_null() {
        return 0;
    }
    core::ptr::copy_nonoverlapping(buf.as_ptr().add(po), name, pl);
    *name.add(pl) = 0;
    (*info).dli_fname = name as *const c_char;
    (*info).dli_fbase = base as *mut c_void;
    1
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn dl_iterate_phdr(
    callback: Option<
        unsafe extern "C" fn(info: *mut libc::dl_phdr_info, size: size_t, data: *mut c_void) -> c_int,
    >,
    data: *mut c_void,
) -> c_int {
    libc!(libc::dl_iterate_phdr(callback, data));

    let cb = match callback {
        Some(f) => f,
        None => return 0,
    };
    let (mut buf, filled) = match read_self_maps() {
        Some(x) => x,
        None => return 0,
    };
    let raw = buf.as_mut_ptr();

    // report each distinct file-backed object once, at its lowest (first) mapping
    let mut seen: [(usize, usize); 512] = [(0, 0); 512];
    let mut nseen = 0usize;
    let mut ret = 0;
    let mut i = 0usize;
    loop {
        let d = core::slice::from_raw_parts(raw, filled);
        let l = match parse_line(d, &mut i) {
            Some(l) => l,
            None => break,
        };
        let (po, pl) = match l.path {
            Some(p) => p,
            None => continue,
        };
        // dedup: already handled this path?
        let mut dup = false;
        for k in 0..nseen {
            let (qo, ql) = seen[k];
            if ql == pl && d[qo..qo + ql] == d[po..po + pl] {
                dup = true;
                break;
            }
        }
        if dup {
            continue;
        }
        if nseen < seen.len() {
            seen[nseen] = (po, pl);
            nseen += 1;
        }
        // the first occurrence is the load base; the ELF header sits there.
        // an unreadable base can't hold usable phdrs - and must not be read.
        if !l.readable {
            continue;
        }
        let base = l.start as *const u8;
        let magic = core::slice::from_raw_parts(base, 4);
        if magic != [0x7f, b'E', b'L', b'F'] {
            continue;
        }
        // Elf64_Ehdr: e_phoff @ 0x20 (u64), e_phnum @ 0x38 (u16)
        let e_phoff = core::ptr::read_unaligned(base.add(0x20) as *const u64);
        let e_phnum = core::ptr::read_unaligned(base.add(0x38) as *const u16);
        // NUL-terminate the path in place (the byte after it is '\n')
        *raw.add(po + pl) = 0;

        let mut info = libc::dl_phdr_info {
            dlpi_addr: l.start as _,
            dlpi_name: raw.add(po) as *const c_char,
            dlpi_phdr: base.add(e_phoff as usize) as *const libc::Elf64_Phdr,
            dlpi_phnum: e_phnum,
            dlpi_adds: 0,
            dlpi_subs: 0,
            dlpi_tls_modid: 0,
            dlpi_tls_data: null_mut(),
        };
        ret = cb(&mut info, core::mem::size_of::<libc::dl_phdr_info>(), data);
        if ret != 0 {
            break;
        }
    }
    ret
}
