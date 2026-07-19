use rustix::fd::BorrowedFd;
use rustix::fs::AtFlags;
use rustix::mm::{mmap_anonymous, MapFlags, ProtFlags};

use alloc::vec::Vec;
use core::cmp::max;
use core::ffi::CStr;
use core::ptr::{copy_nonoverlapping, null_mut};
use core::slice;

use crate::env::set::load_environ;
#[cfg(target_arch = "x86_64")]
use crate::va::{vararg_entry, VaListTag};
use crate::{convert_res, set_errno, Errno};

use libc::{c_char, c_int};

// execl/execle/execlp are true variadics: the argument list has no fixed
// arity, so each is a `vararg_entry!` shim whose implementation walks
// `*const c_char` arguments up to and including the null terminator (and,
// for execle, one more envp pointer after it). C's first `arg` parameter is
// fetched as the first walked argument rather than declared named: the walk
// starts at the register after the path either way, and the loop stays
// uniform.
//
// The walker in `crate::va` is x86_64-only (see its module docs), so other
// architectures keep the nightly `extern "C" fn(..., ...)` variadic
// definitions that predate the walker.

#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn execl(path: *const c_char, ...) -> c_int => execl_impl
}

#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn execl_impl(path: *const c_char, tag: *mut VaListTag) -> c_int {
    let tag = &mut *tag;
    let mut vec = Vec::new();

    loop {
        let ptr = tag.arg::<*const c_char>();
        vec.push(ptr);
        if ptr.is_null() {
            break;
        }
    }

    execv(path, vec.as_ptr())
}

#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn execle(path: *const c_char, ...) -> c_int => execle_impl
}

#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn execle_impl(path: *const c_char, tag: *mut VaListTag) -> c_int {
    let tag = &mut *tag;
    let mut vec = Vec::new();

    let envp = loop {
        let ptr = tag.arg::<*const c_char>();
        vec.push(ptr);
        if ptr.is_null() {
            break tag.arg::<*const *const c_char>();
        }
    };

    execve(path, vec.as_ptr(), envp)
}

#[cfg(target_arch = "x86_64")]
vararg_entry! {
    #[no_mangle]
    unsafe extern "C" fn execlp(file: *const c_char, ...) -> c_int => execlp_impl
}

#[cfg(target_arch = "x86_64")]
unsafe extern "C" fn execlp_impl(file: *const c_char, tag: *mut VaListTag) -> c_int {
    let tag = &mut *tag;
    let mut vec = Vec::new();

    loop {
        let ptr = tag.arg::<*const c_char>();
        vec.push(ptr);
        if ptr.is_null() {
            break;
        }
    }

    execvp(file, vec.as_ptr())
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn execl(path: *const c_char, arg: *const c_char, mut argv: ...) -> c_int {
    let mut vec = Vec::new();
    vec.push(arg);

    loop {
        let ptr = argv.next_arg::<*const c_char>();
        vec.push(ptr);
        if ptr.is_null() {
            break;
        }
    }

    execv(path, vec.as_ptr())
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn execle(path: *const c_char, arg: *const c_char, mut argv: ...) -> c_int {
    let mut vec = Vec::new();
    vec.push(arg);

    let envp = loop {
        let ptr = argv.next_arg::<*const c_char>();
        vec.push(ptr);
        if ptr.is_null() {
            break argv.next_arg::<*const *const c_char>();
        }
    };

    execve(path, vec.as_ptr(), envp)
}

#[cfg(not(target_arch = "x86_64"))]
#[no_mangle]
unsafe extern "C" fn execlp(file: *const c_char, arg: *const c_char, mut argv: ...) -> c_int {
    let mut vec = Vec::new();
    vec.push(arg);

    loop {
        let ptr = argv.next_arg::<*const c_char>();
        vec.push(ptr);
        if ptr.is_null() {
            break;
        }
    }

    execvp(file, vec.as_ptr())
}

#[no_mangle]
unsafe extern "C" fn execv(prog: *const c_char, argv: *const *const c_char) -> c_int {
    libc!(libc::execv(prog, argv));

    let environ = load_environ();

    execve(prog, argv, environ as *const _)
}

#[no_mangle]
unsafe extern "C" fn execve(
    prog: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    libc!(libc::execve(prog, argv, envp));

    let err = rustix::runtime::execve(
        CStr::from_ptr(prog),
        argv as *const *const _,
        envp as *const *const _,
    );

    set_errno(Errno(err.raw_os_error()));
    -1
}

#[no_mangle]
unsafe extern "C" fn execvp(file: *const c_char, argv: *const *const c_char) -> c_int {
    libc!(libc::execvp(file, argv));

    let environ = load_environ();

    execvpe(file, argv, environ as *const _)
}

#[no_mangle]
unsafe extern "C" fn execvpe(
    file: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    libc!(libc::execvpe(file, argv, envp));

    let file = CStr::from_ptr(file);
    let file_bytes = file.to_bytes();
    if file_bytes.contains(&b'/') {
        let err = rustix::runtime::execve(file, argv.cast(), envp.cast());
        set_errno(Errno(err.raw_os_error()));
        return -1;
    }

    let path = crate::env::get::_getenv(b"PATH");
    let path = if path.is_null() {
        c"/bin:/usr/bin"
    } else {
        CStr::from_ptr(path)
    };

    // Compute the length of the longest item in `PATH`.
    let mut longest_length = 0;
    for dir in path.to_bytes().split(|byte| *byte == b':') {
        longest_length = max(longest_length, dir.len());
    }

    // Allocate a buffer for concatenating `PATH` items with the requested
    // file name. Use `mmap` because we might be running in the child of a
    // fork, where `malloc` is not safe to call. POSIX for its part says
    // that `execvp` is not async-signal-safe, but real-world code depends
    // on it being so.
    //
    // A seeming alternative to allocating a buffer would be to open the
    // `PATH` item and then use `execveat` to execute the requested filename
    // under it, however on Linux at least, `execveat` doesn't work if the
    // file is a `#!` and the directory fd has `O_CLOEXEC`, which we'd
    // want to avoid leaking the directory fd on other threads.
    //
    // POSIX doesn't say that `mmap` is async-signal-safe either, but we're
    // not calling `libc` here, we're calling rustix with the linux_raw
    // backend where it just makes a raw syscall.
    let buffer = match convert_res(mmap_anonymous(
        null_mut(),
        longest_length + 1 + file_bytes.len() + 1,
        ProtFlags::READ | ProtFlags::WRITE,
        MapFlags::PRIVATE,
    )) {
        Some(buffer) => buffer.cast::<u8>(),
        None => return -1,
    };

    let mut access_error = false;
    for dir in path.to_bytes().split(|byte| *byte == b':') {
        // Concatenate the `PATH` item, a `/`, the requested filename, and a
        // NUL terminator.
        copy_nonoverlapping(dir.as_ptr(), buffer, dir.len());
        buffer.add(dir.len()).write(b'/');
        copy_nonoverlapping(
            file_bytes.as_ptr(),
            buffer.add(dir.len() + 1),
            file_bytes.len(),
        );
        buffer.add(dir.len() + 1 + file_bytes.len()).write(b'\0');
        let slice = slice::from_raw_parts(buffer, dir.len() + 1 + file_bytes.len() + 1);

        // Run it! If this succeeds, it doesn't return.
        let error = rustix::runtime::execve(
            CStr::from_bytes_with_nul(slice).unwrap(),
            argv.cast(),
            envp.cast(),
        );

        match error {
            rustix::io::Errno::ACCESS => access_error = true,
            rustix::io::Errno::NOENT | rustix::io::Errno::NOTDIR => {}
            _ => {
                set_errno(Errno(error.raw_os_error()));
                return -1;
            }
        }
    }

    set_errno(Errno(if access_error {
        libc::EACCES
    } else {
        libc::ENOENT
    }));
    -1
}

#[no_mangle]
unsafe extern "C" fn fexecve(
    fd: c_int,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> c_int {
    libc!(libc::fexecve(fd, argv, envp));

    let mut error = rustix::runtime::execveat(
        BorrowedFd::borrow_raw(fd),
        c"",
        argv as *const *const _,
        envp as *const *const _,
        AtFlags::EMPTY_PATH,
    );

    // If `execveat` is unsupported, emulate it with `execve`, without
    // allocating. This trusts /proc/self/fd.
    #[cfg(any(target_os = "android", target_os = "linux"))]
    if let rustix::io::Errno::NOSYS = error {
        const PREFIX: &[u8] = b"/proc/self/fd/";
        const PREFIX_LEN: usize = PREFIX.len();
        let mut buf = [0_u8; PREFIX_LEN + 20 + 1];
        buf[..PREFIX_LEN].copy_from_slice(PREFIX);
        let fd_dec = rustix::path::DecInt::from_fd(BorrowedFd::borrow_raw(fd));
        let fd_bytes = fd_dec.as_c_str().to_bytes_with_nul();
        buf[PREFIX_LEN..PREFIX_LEN + fd_bytes.len()].copy_from_slice(fd_bytes);

        error = rustix::runtime::execve(
            CStr::from_bytes_with_nul_unchecked(&buf),
            argv.cast(),
            envp.cast(),
        );
    }

    set_errno(Errno(error.raw_os_error()));
    -1
}
