# taproot patches on the c-ward base

taproot is a maintained fork of **c-ward** (github.com/sunfishcode/c-ward). Every
edit below is marked `// taproot:` in-source and sits on top of the fork's base
commit; `git log` and a diff against that parent show them exactly. c-gull is
unmodified - all changes are in **c-scape**, plus the new **`taproot/`** cdylib
crate (a workspace member that builds `libc.so.6`).

| File | Change | Why |
|------|--------|-----|
| `c-scape/src/lib.rs` | add `#![no_builtins]` | LLVM loop-idiom recognition compiles `strcpy`/`strcat` down to `mov %rdi,%rax; ret` (no-op) in a cdylib/LTO build; `no_builtins` forbids it. |
| `c-scape/src/io/mod.rs` | `ioctl` forwards unrecognized requests to the kernel (rustix generic `Ioctl`, via `next_arg`) with errno, instead of `panic!` | GPU drivers issue almost exclusively custom ioctls. |
| `c-scape/src/stdio/chk.rs` | `__*printf_chk` ignore the fortify flag instead of `unimplemented!()` | All of NixOS compiles `_FORTIFY_SOURCE=2`. |
| `c-scape/src/process_.rs` | `page_size()` falls back to 4 KiB when `rustix::param::page_size()` is 0; removed the main-exe-only `dl_iterate_phdr` | A dlopened libc never runs origin's `_start`, so the auxv page-size cache is empty (div-by-zero in `get_phys_pages`). The `taproot/` cdylib provides a `/proc/self/maps` `dl_iterate_phdr` reporting **all** loaded objects (Mesa needs the driver build-id). |
| `c-scape/src/malloc/mod.rs` | `valloc`/`pvalloc` use the same page-size fallback | same auxv issue |
| `c-scape/src/thread/mutex.rs` | `pthread_condattr_setclock` no longer prints an "unimplemented" warning | it already returned success; just noise |

## Toolchain
Built on the fork's pinned **`nightly-2026-06-11`** (`rust-toolchain.toml`, or the
`flake.nix` fenix pin). On this nightly `VaList::arg` was renamed to `next_arg`,
so c-scape's `ioctl` and the cdylib's `scanf.rs` use `next_arg`.

## Build
`cd taproot && nix develop -c cargo build --release` -> `target/x86_64-unknown-linux-gnu/release/libtaproot.so` (soname `libc.so.6`, no `NEEDED`, no `PT_INTERP`). Copy to `libc.so.6`/`libm.so.6` at point of use (dlopen matches by filename).
