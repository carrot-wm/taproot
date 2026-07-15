//! mem correctness + throughput smoke for the runtime-erms dispatch.
//! exercises memcpy/memmove/memset through the c ABI symbols origin
//! exports, across the qword/movsb threshold, misaligned, and
//! overlapping - then times a dram-sized copy loop.

#![no_std]
#![no_main]

const BIG: usize = 128 * 1024 * 1024;
static mut SRC: [u8; BIG] = [0; BIG];
static mut DST: [u8; BIG] = [0; BIG];

fn put(msg: &[u8]) {
    let mut remaining = msg;
    while !remaining.is_empty() {
        match unsafe {
            libc::write(libc::STDOUT_FILENO, remaining.as_ptr().cast(), remaining.len())
        } {
            -1 => match errno::errno().0 {
                libc::EINTR => continue,
                _ => unsafe { libc::exit(2) },
            },
            n => remaining = &remaining[n as usize..],
        }
    }
}

fn put_num(mut v: u64) {
    let mut buf = [0u8; 20];
    let mut i = buf.len();
    loop {
        i -= 1;
        buf[i] = b'0' + (v % 10) as u8;
        v /= 10;
        if v == 0 {
            break;
        }
    }
    put(&buf[i..]);
}

fn fail(what: &[u8], size: usize, off: usize) -> ! {
    put(b"FAIL ");
    put(what);
    put(b" size=");
    put_num(size as u64);
    put(b" off=");
    put_num(off as u64);
    put(b"\n");
    unsafe { libc::exit(1) }
}

fn now_ns() -> u64 {
    let mut ts = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut ts);
    }
    ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64
}

// sizes straddling the erms threshold, page edges, and both loops' tails
const SIZES: &[usize] = &[
    0, 1, 3, 7, 8, 9, 15, 16, 17, 31, 32, 33, 63, 64, 65, 100, 127, 128, 129, 255, 256, 1000,
    4095, 4096, 4097, 65536, 1 << 20,
];

#[no_mangle]
unsafe extern "C" fn main(_argc: i32, _argv: *const *const u8, _envp: *const *const u8) -> i32 {
    let src = core::ptr::addr_of_mut!(SRC);
    let dst = core::ptr::addr_of_mut!(DST);

    // memcpy: pattern survives every size and src/dst misalignment combo
    for &size in SIZES {
        for &soff in &[0usize, 1, 7, 63] {
            for &doff in &[0usize, 1, 7, 63] {
                let s = (*src).as_mut_ptr().add(soff);
                let d = (*dst).as_mut_ptr().add(doff);
                for i in 0..size {
                    *s.add(i) = (i as u8) ^ (soff as u8) ^ 0x5a;
                    *d.add(i) = 0;
                }
                libc::memcpy(d.cast(), s.cast(), size);
                for i in 0..size {
                    if *d.add(i) != (i as u8) ^ (soff as u8) ^ 0x5a {
                        fail(b"memcpy", size, soff * 100 + doff);
                    }
                }
            }
        }
    }
    put(b"memcpy ok\n");

    // memmove: overlapping in both directions
    for &size in SIZES {
        if size == 0 {
            continue;
        }
        for &shift in &[1usize, 7, 64, 4096] {
            if shift >= size {
                continue;
            }
            // forward overlap: dst > src
            let base = (*dst).as_mut_ptr();
            for i in 0..size + shift {
                *base.add(i) = i as u8 ^ 0xa5;
            }
            libc::memmove(base.add(shift).cast(), base.cast(), size);
            for i in 0..size {
                if *base.add(shift + i) != (i as u8) ^ 0xa5 {
                    fail(b"memmove-fwd", size, shift);
                }
            }
            // backward overlap: dst < src
            for i in 0..size + shift {
                *base.add(i) = i as u8 ^ 0x3c;
            }
            libc::memmove(base.cast(), base.add(shift).cast(), size);
            for i in 0..size {
                if *base.add(i) != ((i + shift) as u8) ^ 0x3c {
                    fail(b"memmove-back", size, shift);
                }
            }
        }
    }
    put(b"memmove ok\n");

    // memset across the threshold and offsets
    for &size in SIZES {
        for &off in &[0usize, 1, 7, 63] {
            let d = (*dst).as_mut_ptr().add(off);
            libc::memset(d.cast(), 0xbe_u8 as i32 as libc::c_int, size);
            for i in 0..size {
                if *d.add(i) != 0xbe {
                    fail(b"memset", size, off);
                }
            }
        }
    }
    put(b"memset ok\n");

    // throughput smoke: dram-sized copies, best-of to dodge page faults
    for i in 0..BIG {
        *(*src).as_mut_ptr().add(i) = i as u8;
        *(*dst).as_mut_ptr().add(i) = 0;
    }
    let mut best = u64::MAX;
    for _ in 0..8 {
        let t0 = now_ns();
        libc::memcpy((*dst).as_mut_ptr().cast(), (*src).as_ptr().cast(), BIG);
        let dt = now_ns() - t0;
        if dt < best {
            best = dt;
        }
    }
    // GB/s * 100 for two decimals without floats
    let gbps_x100 = (BIG as u64 * 100_000) / best.max(1) / 1_000;
    put(b"memcpy 128MiB best: ");
    put_num(gbps_x100 / 100);
    put(b".");
    put_num(gbps_x100 % 100 / 10);
    put_num(gbps_x100 % 10);
    put(b" GB/s\n");

    put(b"PASS\n");
    libc::exit(0)
}
