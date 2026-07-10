//! `dladdr` + `dl_iterate_phdr` for the dlopen-rs world.
//!
//! Mesa needs both to locate its own `.so` and read its `.note.gnu.build-id`
//! (for the pipeline-cache UUID). glibc answers from the dynamic linker's link
//! map; we load through dlopen-rs, so neither glibc function is present and
//! c-gull's (if any) only knows the main program, not the dlopen'd ICD.
//! Reconstruct both from `/proc/self/maps`: each maps entry with a file path is
//! a loaded object; its lowest mapping is the load base (and the ELF header +
//! program headers live there).

use core::ffi::{c_char, c_int, c_void};

#[repr(C)]
struct DlInfo {
    dli_fname: *const c_char,
    dli_fbase: *mut c_void,
    dli_sname: *const c_char,
    dli_saddr: *mut c_void,
}

unsafe extern "C" {
    fn malloc(n: usize) -> *mut c_void;
    fn free(p: *mut c_void);
}

#[inline]
unsafe fn sc3(n: isize, a: isize, b: isize, c: isize) -> isize {
    let ret: isize;
    unsafe {
        core::arch::asm!("syscall",
            inlateout("rax") n => ret, in("rdi") a, in("rsi") b, in("rdx") c,
            out("rcx") _, out("r11") _, options(nostack, preserves_flags));
    }
    ret
}

fn hexval(b: u8) -> Option<usize> {
    match b {
        b'0'..=b'9' => Some((b - b'0') as usize),
        b'a'..=b'f' => Some((b - b'a') as usize + 10),
        b'A'..=b'F' => Some((b - b'A') as usize + 10),
        _ => None,
    }
}

// one /proc/self/maps line: start, end, and byte range of the path field (if any)
struct Line {
    start: usize,
    end: usize,
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
        // resync to next line
        while *i < d.len() && d[*i] != b'\n' {
            *i += 1;
        }
        if *i < d.len() {
            *i += 1;
        }
        return Some(Line { start: 0, end: 0, path: None });
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
    // find newline; path is the first '/'.. before it
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
    Some(Line { start, end, path })
}

#[unsafe(no_mangle)]
unsafe extern "C" fn dladdr(addr: *const c_void, info: *mut DlInfo) -> c_int {
    if info.is_null() {
        return 0;
    }
    unsafe {
        (*info).dli_fname = core::ptr::null();
        (*info).dli_fbase = core::ptr::null_mut();
        (*info).dli_sname = core::ptr::null();
        (*info).dli_saddr = core::ptr::null_mut();
    }
    let target = addr as usize;

    // read /proc/self/maps into a heap buffer
    let path = c"/proc/self/maps".as_ptr() as isize;
    let fd = unsafe { sc3(257, -100, path, 0) }; // openat(AT_FDCWD, .., O_RDONLY)
    if fd < 0 {
        return 0;
    }
    let cap = 1usize << 20;
    let buf = unsafe { malloc(cap) } as *mut u8;
    if buf.is_null() {
        unsafe { sc3(3, fd, 0, 0) };
        return 0;
    }
    let mut filled = 0usize;
    loop {
        let n = unsafe { sc3(0, fd, buf.add(filled) as isize, (cap - 1 - filled) as isize) };
        if n <= 0 {
            break;
        }
        filled += n as usize;
        if filled >= cap - 1 {
            break;
        }
    }
    unsafe { sc3(3, fd, 0, 0) };
    let d = unsafe { core::slice::from_raw_parts(buf, filled) };

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
        None => {
            unsafe { free(buf as *mut c_void) };
            return 0;
        }
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

    // strdup the path so it outlives this call (glibc's dli_fname is stable)
    let name = unsafe { malloc(pl + 1) } as *mut u8;
    if name.is_null() {
        unsafe { free(buf as *mut c_void) };
        return 0;
    }
    unsafe {
        core::ptr::copy_nonoverlapping(buf.add(po), name, pl);
        *name.add(pl) = 0;
        (*info).dli_fname = name as *const c_char;
        (*info).dli_fbase = base as *mut c_void;
        free(buf as *mut c_void);
    }
    1
}

// -- dl_iterate_phdr ----------------------------------------------------------
// glibc layout; Mesa's build_id walker reads dlpi_addr/name/phdr/phnum.
#[repr(C)]
struct DlPhdrInfo {
    dlpi_addr: usize,
    dlpi_name: *const c_char,
    dlpi_phdr: *const c_void, // Elf64_Phdr *
    dlpi_phnum: u16,
    _pad: [u8; 6],
    dlpi_adds: u64,
    dlpi_subs: u64,
    dlpi_tls_modid: usize,
    dlpi_tls_data: *mut c_void,
}

type PhdrCb = unsafe extern "C" fn(*mut DlPhdrInfo, usize, *mut c_void) -> c_int;

#[unsafe(no_mangle)]
unsafe extern "C" fn dl_iterate_phdr(callback: Option<PhdrCb>, data: *mut c_void) -> c_int {
    let cb = match callback {
        Some(f) => f,
        None => return 0,
    };
    // read /proc/self/maps
    let path = c"/proc/self/maps".as_ptr() as isize;
    let fd = unsafe { sc3(257, -100, path, 0) };
    if fd < 0 {
        return 0;
    }
    let cap = 1usize << 20;
    let buf = unsafe { malloc(cap) } as *mut u8;
    if buf.is_null() {
        unsafe { sc3(3, fd, 0, 0) };
        return 0;
    }
    let mut filled = 0usize;
    loop {
        let n = unsafe { sc3(0, fd, buf.add(filled) as isize, (cap - 1 - filled) as isize) };
        if n <= 0 {
            break;
        }
        filled += n as usize;
        if filled >= cap - 1 {
            break;
        }
    }
    unsafe { sc3(3, fd, 0, 0) };
    let d = unsafe { core::slice::from_raw_parts(buf, filled) };

    // report each distinct file-backed object once, at its lowest (first) mapping
    let mut seen: [(usize, usize); 512] = [(0, 0); 512];
    let mut nseen = 0usize;
    let mut ret = 0;
    let mut i = 0usize;
    while let Some(l) = parse_line(d, &mut i) {
        let (po, pl) = match l.path {
            Some(p) => p,
            None => continue,
        };
        // dedup: already reported this path?
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
        // first occurrence = load base; the ELF header sits there
        let base = l.start as *const u8;
        // validate \x7fELF
        let magic = unsafe { core::slice::from_raw_parts(base, 4) };
        if magic != [0x7f, b'E', b'L', b'F'] {
            continue;
        }
        // Elf64_Ehdr: e_phoff @ 0x20 (u64), e_phnum @ 0x38 (u16)
        let e_phoff = unsafe { core::ptr::read_unaligned(base.add(0x20) as *const u64) };
        let e_phnum = unsafe { core::ptr::read_unaligned(base.add(0x38) as *const u16) };
        // NUL-terminate the path in-place (byte after path is '\n')
        unsafe { *buf.add(po + pl) = 0 };

        let mut info = DlPhdrInfo {
            dlpi_addr: l.start,
            dlpi_name: unsafe { buf.add(po) } as *const c_char,
            dlpi_phdr: unsafe { base.add(e_phoff as usize) } as *const c_void,
            dlpi_phnum: e_phnum,
            _pad: [0; 6],
            dlpi_adds: 0,
            dlpi_subs: 0,
            dlpi_tls_modid: 0,
            dlpi_tls_data: core::ptr::null_mut(),
        };
        ret = unsafe { cb(&mut info, core::mem::size_of::<DlPhdrInfo>(), data) };
        if ret != 0 {
            break;
        }
    }
    unsafe { free(buf as *mut c_void) };
    ret
}
