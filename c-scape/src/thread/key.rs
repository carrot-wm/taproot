use alloc::boxed::Box;
use core::alloc::Layout;
use core::cell::Cell;
use core::ptr::{null, null_mut};
use core::sync::atomic::{AtomicU32, Ordering};
use libc::{c_int, c_void};

use rustix_futex_sync::RwLock;

#[cfg(target_env = "gnu")]
const PTHREAD_KEYS_MAX: u32 = 1024;
#[cfg(target_env = "musl")]
const PTHREAD_KEYS_MAX: u32 = 128;
const PTHREAD_DESTRUCTOR_ITERATIONS: u8 = 4;

#[derive(Clone, Copy)]
struct KeyData {
    next_key: libc::pthread_key_t,
    destructors: [Option<unsafe extern "C" fn(_: *mut c_void)>; PTHREAD_KEYS_MAX as usize],
}

#[derive(Clone, Copy)]
struct ValueWithEpoch {
    epoch: u32,
    data: *mut c_void,
}

static KEY_DATA: RwLock<KeyData> = RwLock::new(KeyData {
    next_key: 0,
    destructors: [None; PTHREAD_KEYS_MAX as usize],
});

static EPOCHS: [AtomicU32; PTHREAD_KEYS_MAX as usize] =
    [const { AtomicU32::new(0) }; PTHREAD_KEYS_MAX as usize];

/// Per-thread pthread-key storage.
///
/// This hangs off origin's per-thread specifics slot (a pointer next to
/// errno, reached only via `origin::thread::specifics_location()`) instead
/// of nightly-only thread-local statics, so c-scape builds on stable. The
/// block is allocated zeroed on a thread's first `pthread_setspecific` and
/// freed by the thread-exit cleanup after every key destructor has
/// settled; threads that never touch pthread specifics never allocate it.
struct Specifics {
    // This uses an epoch-based system for differentiating between
    // reused keys corresponding to the same slot.
    values: [Cell<ValueWithEpoch>; PTHREAD_KEYS_MAX as usize],
    has_registered_cleanup: Cell<bool>,
}

/// Return the current thread's `Specifics` block if one exists.
///
/// All-zero bytes are a valid `Specifics` (every value has epoch 0 and a
/// null pointer, the flag is `false`), so an absent block and a freshly
/// zeroed one read identically: every key is null.
#[inline]
fn specifics_opt() -> Option<&'static Specifics> {
    // SAFETY: The slot is null or holds a live block that `specifics`
    // allocated for this thread; nothing else ever stores to it.
    unsafe {
        (*origin::thread::specifics_location())
            .cast::<Specifics>()
            .as_ref()
    }
}

/// Return the current thread's `Specifics` block, allocating it zeroed on
/// first touch.
///
/// Returns `None` if the allocator fails; the caller reports `ENOMEM` in
/// the usual pthread way (returning the error code). Panicking is not an
/// option here, as c-scape may be hosting a panic=abort process.
fn specifics() -> Option<&'static Specifics> {
    // SAFETY: The slot is only ever written here and in the thread-exit
    // cleanup, always with null or a block from the global allocator with
    // this same layout.
    unsafe {
        let slot = origin::thread::specifics_location();
        let mut ptr = (*slot).cast::<Specifics>();
        if ptr.is_null() {
            ptr = alloc::alloc::alloc_zeroed(Layout::new::<Specifics>()).cast();
            if ptr.is_null() {
                return None;
            }
            *slot = ptr.cast();
        }
        Some(&*ptr)
    }
}

#[no_mangle]
unsafe extern "C" fn pthread_getspecific(key: libc::pthread_key_t) -> *mut c_void {
    libc!(libc::pthread_getspecific(key));

    let latest_epoch = match EPOCHS.get(key as usize) {
        Some(epoch) => epoch,
        None => return null_mut(),
    };
    let latest_epoch = latest_epoch.load(Ordering::SeqCst);

    // No block means this thread never stored any value: every key reads
    // as null, exactly as the zeroed storage would.
    let specifics = match specifics_opt() {
        Some(specifics) => specifics,
        None => return null_mut(),
    };
    let ValueWithEpoch { epoch, data } = specifics.values[key as usize].get();

    // If the latest epoch is newer, then that means this slot got reallocated.
    // Either we are reusing a deleted key, which is UB, or we are reusing
    // the new key, which must return null initially.
    if epoch < latest_epoch {
        null_mut()
    } else {
        data
    }
}

#[no_mangle]
unsafe extern "C" fn pthread_setspecific(key: libc::pthread_key_t, value: *const c_void) -> c_int {
    libc!(libc::pthread_setspecific(key, value));

    // The block must exist before we can store into it. A null return from
    // the allocator becomes ENOMEM, which POSIX documents for
    // `pthread_setspecific`.
    let specifics = match specifics() {
        Some(specifics) => specifics,
        None => return libc::ENOMEM,
    };

    // If this is the first-time we have gotten here,
    // we need to actually register the dtors for cleanup.
    if !specifics.has_registered_cleanup.get() {
        origin::thread::at_exit(Box::new(move || {
            for _ in 0..PTHREAD_DESTRUCTOR_ITERATIONS {
                let mut ran_dtor = false;

                for i in 0..PTHREAD_KEYS_MAX {
                    let data = pthread_getspecific(i as libc::pthread_key_t);

                    if data.is_null() {
                        continue;
                    }

                    ran_dtor = true;

                    let dtor = {
                        // POSIX says that `pthread_key_delete` can
                        // be called within a destructor function...
                        // We have to take each dtor one at a time just
                        // in case someone did delete it;
                        let key_data = KEY_DATA.read();
                        key_data.destructors[i as usize]
                    };

                    if let Some(dtor) = dtor {
                        // Null out the data as required
                        // by POSIX semantics. This may bump
                        // the local epoch, but that doesn't matter.
                        pthread_setspecific(i as libc::pthread_key_t, null());

                        // Call the destructor with the old
                        // data, before we set it to null.
                        dtor(data);
                    }
                }

                if !ran_dtor {
                    break;
                }
            }

            // Every destructor above ran with the block still installed,
            // so re-entrant `pthread_getspecific`/`pthread_setspecific`
            // calls saw it; only now that the loop has settled does the
            // block go away. Null the slot before freeing so a straggling
            // get reads null rather than freed memory, and a straggling
            // set starts a fresh block (origin runs destructors registered
            // during exit, so the fresh block's cleanup still runs).
            //
            // SAFETY: The slot holds null or the block this thread
            // allocated in `specifics`, with the same layout.
            let slot = origin::thread::specifics_location();
            let block = (*slot).cast::<Specifics>();
            *slot = null_mut();
            if !block.is_null() {
                alloc::alloc::dealloc(block.cast(), Layout::new::<Specifics>());
            }
        }));

        specifics.has_registered_cleanup.set(true);
    }

    let latest_epoch = match EPOCHS.get(key as usize) {
        Some(epoch) => epoch,
        None => return libc::EINVAL,
    };
    let latest_epoch = latest_epoch.load(Ordering::SeqCst);
    specifics.values[key as usize].set(ValueWithEpoch {
        epoch: latest_epoch,
        data: value.cast_mut(),
    });
    0
}

#[no_mangle]
unsafe extern "C" fn pthread_key_create(
    key: *mut libc::pthread_key_t,
    dtor: Option<unsafe extern "C" fn(_: *mut c_void)>,
) -> c_int {
    libc!(libc::pthread_key_create(key, dtor));

    extern "C" fn empty_dtor(_: *mut c_void) {}

    let mut key_data = KEY_DATA.write();

    let mut next_key = key_data.next_key;
    if next_key < PTHREAD_KEYS_MAX {
        // Fast-path, less than `PTHREAD_KEYS_MAX` slots
        // have been allocated, we just use this as a bump
        // allocator basically.
        key_data.next_key = next_key + 1;
    } else {
        // Slow-path, linearly scan through the table to try
        // and find an empty slot.
        for (index, dtor) in key_data.destructors.iter().enumerate() {
            if dtor.is_none() {
                // We have to bump the epoch now that we are
                // reusing slots.
                if EPOCHS[index].fetch_add(1, Ordering::SeqCst) == 0 {
                    panic!("detected epoch counter overflow");
                }
                next_key = index as libc::pthread_key_t;
                break;
            }
        }

        // If the loop still did not find a valid key
        if next_key >= PTHREAD_KEYS_MAX {
            // pthread functions return the error code, not -1/errno
            let msg = b"taproot: pthread_key_create: all keys in use\n";
            let _ = rustix::io::write(unsafe { rustix::fd::BorrowedFd::borrow_raw(2) }, msg);
            return libc::EAGAIN;
        }
    }

    // We have to `unwrap_or` the dtor because `None` is reserved for signifying
    // that the key is not allocated.
    *key = next_key;
    key_data.destructors[next_key as usize] = Some(dtor.unwrap_or(empty_dtor));

    0
}

// gcc's gthr-posix.h tests this glibc-compat name to decide whether the
// process is threaded; unresolved, every __gthread_* call answers -1 and
// libstdc++'s call_once throws system_error(-1)
#[no_mangle]
unsafe extern "C" fn __pthread_key_create(
    key: *mut libc::pthread_key_t,
    dtor: Option<unsafe extern "C" fn(_: *mut c_void)>,
) -> c_int {
    pthread_key_create(key, dtor)
}

#[no_mangle]
unsafe extern "C" fn pthread_key_delete(key: libc::pthread_key_t) -> c_int {
    libc!(libc::pthread_key_delete(key));

    let mut key_data = KEY_DATA.write();
    key_data.destructors[key as usize] = None;

    0
}
