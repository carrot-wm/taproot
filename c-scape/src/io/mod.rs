#[cfg(not(target_os = "wasi"))]
mod dup;
#[cfg(any(target_os = "android", target_os = "linux"))]
mod epoll;
mod isatty;
mod pipe;
mod poll;
mod read;
mod select;
mod splice;
#[cfg(any(target_os = "android", target_os = "linux"))]
mod timerfd;
mod write;

use rustix::event::EventfdFlags;
use rustix::fd::{BorrowedFd, IntoRawFd};

use libc::{c_int, c_long, c_uint, c_void};

use crate::convert_res;

// taproot fork: forward any ioctl request c-scape doesn't special-case straight
// to the kernel via rustix's generic Ioctl, instead of panicking. GPU drivers
// issue custom ioctls almost exclusively, so this is load-bearing. `convert_res`
// sets errno on failure, matching the C ioctl contract.
struct GenericIoctl {
    opcode: rustix::ioctl::Opcode,
    arg: *mut c_void,
}
unsafe impl rustix::ioctl::Ioctl for GenericIoctl {
    type Output = rustix::ioctl::IoctlOutput;
    const IS_MUTATING: bool = true;
    fn opcode(&self) -> rustix::ioctl::Opcode {
        self.opcode
    }
    fn as_ptr(&mut self) -> *mut c_void {
        self.arg
    }
    unsafe fn output_from_ptr(
        out: rustix::ioctl::IoctlOutput,
        _extract: *mut c_void,
    ) -> rustix::io::Result<rustix::ioctl::IoctlOutput> {
        Ok(out)
    }
}

// taproot: fixed arity. One `c_long` third argument covers every request's
// int and pointer forms (a single INTEGER-class slot either way, loaded
// identically by variadic and fixed callers); each arm casts it to what its
// request defines, and requests without an argument never look at it.
#[cfg(not(target_os = "wasi"))]
#[no_mangle]
unsafe extern "C" fn ioctl(fd: c_int, request: c_long, arg: c_long) -> c_int {
    const TCGETS: c_long = libc::TCGETS as c_long;
    const FIONBIO: c_long = libc::FIONBIO as c_long;
    const TIOCINQ: c_long = libc::TIOCINQ as c_long;
    const TIOCGWINSZ: c_long = libc::TIOCGWINSZ as c_long;
    const FICLONE: c_long = libc::FICLONE as c_long;
    match request {
        TCGETS => {
            libc!(libc::ioctl(fd, libc::TCGETS));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::termios::tcgetattr(fd)) {
                Some(x) => {
                    (arg as *mut rustix::termios::Termios).write(x);
                    0
                }
                None => -1,
            }
        }
        FIONBIO | TIOCINQ => {
            let ptr = arg as *mut c_int;
            let value = *ptr != 0;
            libc!(libc::ioctl(fd, libc::FIONBIO, value as c_int));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::io::ioctl_fionbio(fd, value)) {
                Some(()) => 0,
                None => -1,
            }
        }
        TIOCGWINSZ => {
            libc!(libc::ioctl(fd, libc::TIOCGWINSZ));
            let fd = BorrowedFd::borrow_raw(fd);
            match convert_res(rustix::termios::tcgetwinsize(fd)) {
                Some(size) => {
                    let size = libc::winsize {
                        ws_row: size.ws_row,
                        ws_col: size.ws_col,
                        ws_xpixel: size.ws_xpixel,
                        ws_ypixel: size.ws_ypixel,
                    };
                    (arg as *mut libc::winsize).write(size);
                    0
                }
                None => -1,
            }
        }
        FICLONE => {
            let src_fd = arg as c_int;
            libc!(libc::ioctl(fd, libc::FICLONE as _, src_fd));
            let fd = BorrowedFd::borrow_raw(fd);
            let src_fd = BorrowedFd::borrow_raw(src_fd);
            match convert_res(rustix::fs::ioctl_ficlone(fd, src_fd)) {
                Some(()) => 0,
                None => -1,
            }
        }
        _ => {
            let arg = arg as *mut c_void;
            let fd = BorrowedFd::borrow_raw(fd);
            let generic = GenericIoctl {
                opcode: request as rustix::ioctl::Opcode,
                arg,
            };
            match convert_res(rustix::ioctl::ioctl(fd, generic)) {
                Some(out) => out,
                None => -1,
            }
        }
    }
}

#[cfg(any(target_os = "android", target_os = "linux"))]
#[no_mangle]
unsafe extern "C" fn eventfd(initval: c_uint, flags: c_int) -> c_int {
    libc!(libc::eventfd(initval, flags));
    let flags = EventfdFlags::from_bits(flags.try_into().unwrap()).unwrap();
    match convert_res(rustix::event::eventfd(initval, flags)) {
        Some(fd) => fd.into_raw_fd(),
        None => -1,
    }
}
