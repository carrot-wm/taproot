use libc::{c_int, gid_t};

#[no_mangle]
unsafe extern "C" fn getgid() -> gid_t {
    libc!(libc::getgid());
    rustix::process::getgid().as_raw()
}

#[no_mangle]
unsafe extern "C" fn setgid(_gid: gid_t) -> c_int {
    libc!(libc::setgid(_gid));

    // rustix has a `set_thread_gid` function, but it just wraps the Linux
    // syscall which sets a per-thread GID rather than the whole process GID.
    // Linux expects libc's to have logic to set the GID for all the threads.
    errno::set_errno(errno::Errno(libc::ENOSYS));
    -1
}

#[no_mangle]
unsafe extern "C" fn getresgid(rgid: *mut gid_t, egid: *mut gid_t, sgid: *mut gid_t) -> c_int {
    libc!(libc::getresgid(rgid, egid, sgid));

    let (r, e, s) = match super::uid::proc_status_ids(b"Gid:") {
        Some([r, e, s]) => (r, e, s),
        None => {
            let e = rustix::process::getegid().as_raw();
            (rustix::process::getgid().as_raw(), e, e)
        }
    };
    if !rgid.is_null() {
        *rgid = r;
    }
    if !egid.is_null() {
        *egid = e;
    }
    if !sgid.is_null() {
        *sgid = s;
    }
    0
}
