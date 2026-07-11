use libc::{c_int, c_void};

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnp() -> c_int {
    //libc!(libc::posix_spawnp());
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnattr_destroy(_ptr: *const c_void) -> c_int {
    //libc!(libc::posix_spawn_spawnattr_destroy(ptr));
    rustix::io::write(
        rustix::stdio::stderr(),
        b"unimplemented: posix_spawn_spawnattr_destroy\n",
    )
    .ok();
    0
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnattr_init(_ptr: *const c_void) -> c_int {
    //libc!(libc::posix_spawnattr_init(ptr));
    rustix::io::write(
        rustix::stdio::stderr(),
        b"unimplemented: posix_spawnattr_init\n",
    )
    .ok();
    0
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnattr_setflags() -> c_int {
    //libc!(libc::posix_spawnattr_setflags());
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnattr_setsigdefault() -> c_int {
    //libc!(libc::posix_spawnattr_setsigdefault());
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnattr_setsigmask() -> c_int {
    //libc!(libc::posix_spawnattr_setsigmask());
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawnattr_setpgroup(_ptr: *mut c_void, _pgroup: c_int) -> c_int {
    //libc!(libc::posix_spawnattr_setpgroup(ptr, pgroup));
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawn_file_actions_adddup2() -> c_int {
    //libc!(libc::posix_spawn_file_actions_adddup2());
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawn_file_actions_addchdir_np() -> c_int {
    //libc!(libc::posix_spawn_file_actions_addchdir_np());
    libc::ENOSYS
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawn_file_actions_destroy(_ptr: *const c_void) -> c_int {
    //libc!(libc::posix_spawn_file_actions_destroy(ptr));
    rustix::io::write(
        rustix::stdio::stderr(),
        b"unimplemented: posix_spawn_file_actions_destroy\n",
    )
    .ok();
    0
}

#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn posix_spawn_file_actions_init(_ptr: *const c_void) -> c_int {
    //libc!(libc::posix_spawn_file_actions_init(ptr));
    rustix::io::write(
        rustix::stdio::stderr(),
        b"unimplemented: posix_spawn_file_actions_init\n",
    )
    .ok();
    0
}
