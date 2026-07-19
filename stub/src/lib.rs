//! Nothing here on purpose. Since glibc 2.34 libpthread/libdl/librt are
//! empty shells whose symbols all live in libc.so.6; this crate is the
//! same idea for taproot. Staged copies of this file own those filenames
//! (plus the ld-linux one that libgcc_s names for _dl_find_object), so a
//! NEEDED entry resolves to an already-loaded empty library and symbol
//! lookup falls through to the preloaded taproot libc.so.6.

#![no_std]

// abort panics (workspace release profile) want no personality item;
// nothing in an empty library panics anyway
#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}
