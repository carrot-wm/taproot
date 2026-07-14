use libc::{c_int, uid_t};

// real/effective/saved from /proc/self/status - the saved id has no
// syscall of its own and rustix exposes nothing for it
pub(crate) fn proc_status_ids(tag: &[u8]) -> Option<[u32; 3]> {
    use core::ffi::CStr;
    use rustix::fs::{Mode, OFlags};
    let fd = rustix::fs::open(
        unsafe { CStr::from_bytes_with_nul_unchecked(b"/proc/self/status\0") },
        OFlags::RDONLY | OFlags::CLOEXEC,
        Mode::empty(),
    )
    .ok()?;
    let mut buf = [0u8; 2048];
    let mut filled = 0;
    while filled < buf.len() {
        match rustix::io::read(&fd, &mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(rustix::io::Errno::INTR) => continue,
            Err(_) => return None,
        }
    }
    let mut ids = [0u32; 3];
    for line in buf[..filled].split(|&b| b == b'\n') {
        let Some(rest) = line.strip_prefix(tag) else {
            continue;
        };
        let mut field = 0;
        let mut cur: Option<u32> = None;
        for &b in rest {
            match b {
                b'0'..=b'9' => cur = Some(cur.unwrap_or(0).wrapping_mul(10) + (b - b'0') as u32),
                _ => {
                    if let Some(v) = cur.take() {
                        if field < 3 {
                            ids[field] = v;
                        }
                        field += 1;
                    }
                }
            }
        }
        if let Some(v) = cur {
            if field < 3 {
                ids[field] = v;
            }
            field += 1;
        }
        if field >= 3 {
            return Some(ids);
        }
        return None;
    }
    None
}

#[no_mangle]
unsafe extern "C" fn getuid() -> uid_t {
    libc!(libc::getuid());
    rustix::process::getuid().as_raw()
}

#[no_mangle]
unsafe extern "C" fn setuid(uid: uid_t) -> c_int {
    libc!(libc::setuid(uid));

    // rustix has a `set_thread_uid` function, but it just wraps the Linux
    // syscall which sets a per-thread UID rather than the whole process UID.
    // Linux expects libc's to have logic to set the UID for all the threads.
    errno::set_errno(errno::Errno(libc::ENOSYS));
    -1
}

#[no_mangle]
unsafe extern "C" fn getresuid(ruid: *mut uid_t, euid: *mut uid_t, suid: *mut uid_t) -> c_int {
    libc!(libc::getresuid(ruid, euid, suid));

    let (r, e, s) = match proc_status_ids(b"Uid:") {
        Some([r, e, s]) => (r, e, s),
        // no /proc: real and effective still have syscalls; assume the
        // saved id tracks the effective one, as it does un-setuid
        None => {
            let e = rustix::process::geteuid().as_raw();
            (rustix::process::getuid().as_raw(), e, e)
        }
    };
    if !ruid.is_null() {
        *ruid = r;
    }
    if !euid.is_null() {
        *euid = e;
    }
    if !suid.is_null() {
        *suid = s;
    }
    0
}
