# taproot patches on the c-ward base

taproot is a maintained fork of **c-ward** (github.com/sunfishcode/c-ward). Every
edit below is marked `// taproot:` in-source and sits on top of the fork's base
commit; `git log` and a diff against that parent show them exactly. Most edits
are in **c-scape** plus the new **`taproot/`** cdylib crate (a workspace member
that builds `libc.so.6`); the errno sweep below also touches c-gull's resolver
and nss. Point edits carry `// taproot:` markers; the sweep sites are listed by
their commit instead.

| File | Change | Why |
|------|--------|-----|
| `c-scape/src/lib.rs` | add `#![no_builtins]` | LLVM loop-idiom recognition compiles `strcpy`/`strcat` down to `mov %rdi,%rax; ret` (no-op) in a cdylib/LTO build; `no_builtins` forbids it. |
| `c-scape/src/io/mod.rs` | `ioctl` forwards unrecognized requests to the kernel (rustix generic `Ioctl`, via `next_arg`) with errno, instead of `panic!` | GPU drivers issue almost exclusively custom ioctls. |
| `c-scape/src/stdio/chk.rs` | `__*printf_chk` ignore the fortify flag instead of `unimplemented!()` | All of NixOS compiles `_FORTIFY_SOURCE=2`. |
| `c-scape/src/process_.rs` | `page_size()` falls back to 4 KiB when `rustix::param::page_size()` is 0; removed the main-exe-only `dl_iterate_phdr` | A dlopened libc never runs origin's `_start`, so the auxv page-size cache is empty (div-by-zero in `get_phys_pages`). |
| `c-scape/src/dl.rs` (new) | `dladdr` + `dl_iterate_phdr` reconstructed from `/proc/self/maps`, reporting **all** loaded objects | there is no dynamic linker to ask: the main program may be a static-PIE and other objects arrive via an external loader. Mesa walks `dl_iterate_phdr` for the driver build-id; `std`'s backtrace and the `unwinding` crate reference it from any eyra binary (a binary linking c-scape without it doesn't link). Was in the `taproot/` cdylib; moved into c-scape so binaries get it too. |
| `c-scape/src/process_.rs` | full `getauxval` + `__getauxval` read from `/proc/self/auxv`; unknown/absent types return 0 with `ENOENT` | origin's shim recognizes only HWCAP/HWCAP2/MINSIGSTKSZ and `todo!()`-panics on the rest - `getauxval(AT_SECURE)` (secure_getenv semantics, e.g. the `secure-execution` crate) aborts any eyra binary. `/proc/self/auxv` also works in a dlopened `libc.so.6`, where origin's captured auxv never exists. NOTE: origin still defines `getauxval` (same CGU as its entry code), so a binary linking both needs `-Wl,--allow-multiple-definition` (rustc orders c-scape first); the upstreamable fix is origin returning 0 for unrecognized types. |
| `c-scape/src/process_.rs` | `dlsym(RTLD_DEFAULT, ..)` answers unknown probe symbols with null instead of `unimplemented!()` | that is the dlsym contract, and probing callers have fallbacks by construction - a mesa update probing `__epoll_pwait2_time64` aborted the whole compositor. |
| `c-scape/src/malloc/mod.rs` | `valloc`/`pvalloc` use the same page-size fallback | same auxv issue |
| `c-scape/src/thread/mutex.rs` | `pthread_condattr_setclock` no longer prints an "unimplemented" warning | it already returned success; just noise |
| `c-scape/src/jmp.rs` | `_setjmp`/`_longjmp`/`__sigsetjmp` become real `#[no_mangle]` naked trampolines, plus a new `__longjmp_chk` (frame check skipped, like the `__*printf_chk` family) | the upstream `.set` assembler aliases never reach a cdylib's `.dynsym` - rustc's version script exports only the `#[no_mangle]` items it knows. glibc headers make every caller import the alias names (`setjmp` is a macro for `_setjmp`, fortified `longjmp` becomes `__longjmp_chk`), so mesa's spirv-to-nir error handling hit a silently-unresolved PLT slot (elf_loader skips unresolvable JUMP_SLOTs without erroring, even under RTLD_NOW) and the first real shader compile jumped to an unmapped link-time address. |
| ICD gap fill (one commit): `c-scape` mkostemp64/mkstemps64, rewinddir ungated from `todo`, syslog family no-ops, pthread_cancel (ENOSYS) + pthread_setcanceltype (no-op success), SysV shm contract stubs; `taproot` `__progname`/`__progname_full` | every remaining strong UND symbol of `libvulkan_intel.so` resolves - elf_loader leaves unresolvable JUMP_SLOTs at their link-time value *silently* even under RTLD_NOW, so any missing symbol is a jump-to-unmapped-memory crash deferred until first call. Mesa's disk cache writes entries via `mkostemp64`/`mkstemps64` (a cold cache - i.e. any mesa rebuild - hits this right after the first shader compile) and evicts via `rewinddir`. |
| errno sweep (one commit): `c-scape` time, fcntl, net sockopts, pthread mutex kinds, sysconf/pathconf/prctl, dlsym-with-handle, setuid/setgid/setgroups, posix_spawn stubs; `c-gull` resolve + nss | every C-ABI dispatch fallthrough answers its contract error (EINVAL, ENOPROTOOPT, ENOSYS, EAI_*, null, or the site's own failure arm) instead of `unimplemented!()`/`todo!()`/`panic!()` | a libc that aborts the process on an unknown input is a compositor-killer: three separate mesa/kbvm probes have taken the session down this way (`__printf_chk`, `getauxval(AT_SECURE)`, `dlsym(__epoll_pwait2_time64)`). Kept as real panics: hex-float `strtod`, `longjmp`, `___tls_get_addr`, internal invariants - places where a silently wrong answer beats nothing. |
| `.cargo/config.toml` + `tools/link-shim.sh` (new) | links run through a shim that renames the WEAK HIDDEN math/int builtin aliases (`ceil`, `sqrt`, `__clzdi2`, ...) inside a copy of the toolchain's prebuilt compiler-builtins | ELF linking merges the most constraining visibility per symbol name, so on stable rustc those dead weak copies demote c-scape's own default-visibility definitions to hidden and the cdylib silently stops exporting the C math surface; mesa binds `ceil`/`sqrt` eagerly (`-fno-plt`), so every lost export is a deferred jump-to-unmapped crash. No-op on toolchains whose compiler-builtins lacks the aliases (the pinned nightly). |
| root `Cargo.toml` | `[profile.release.package.*] codegen-units = 1` for taproot-origin, taproot-c-scape and taproot-c-gull | rustc links dependency rlibs without `--whole-archive`, so a `#[no_mangle]` entry whose codegen unit nothing referenced is never extracted from the archive and silently vanishes from the `.so`; one unit per libc crate makes extraction all-or-nothing. |
| `origin/src/naked.rs` | `naked_fn!` always emits a real `#[unsafe(naked)]` function (stable since Rust 1.88); the `global_asm!` fallback is gone | rustc's cdylib version script exports only symbols it knows, so the asm-defined `_start`/`return_from_signal_handler` fell out of `.dynsym` once the origin "nightly" feature was dropped. |

## Toolchain
The fork builds on stable Rust (1.96+) for x86_64; the pinned
**`nightly-2026-06-11`** (`rust-toolchain.toml`, or the `flake.nix` fenix pin)
remains the devshell toolchain and the only path for other architectures,
whose variadic definitions still need `feature(c_variadic)` (on that nightly
`VaList::arg` is named `next_arg`, which the non-x86_64 arms use). On x86_64
every variadic entry point is fixed arity or a `c-scape/src/va.rs` walker
shim, and the printf family formats through the vendored
`c-scape/src/printf_impl` port of printf-compat 0.4.0.

## Build
From the **`taproot/` member directory** (cargo reads `.cargo/config.toml` from the
cwd, so building with `-p taproot` from the workspace root silently drops the
soname/`--export-dynamic`/`-nodefaultlibs` flags and emits a broken `.so` under
`target/release/deps/`):

`cd taproot && nix develop .. -c cargo build --release` -> `../target/x86_64-unknown-linux-gnu/release/libtaproot.so` (soname `libc.so.6`, no `NEEDED`, no `PT_INTERP`). Copy to `libc.so.6`/`libm.so.6` at point of use (dlopen matches by filename).
| publish renames (one commit): crates become `taproot-c-scape`/`taproot-c-gull`, upstream `eyra` forked in-tree as `taproot-eyra` (deps rewired by `package =` rename, so no source changes), the `taproot` cdylib routes through the fork by path, workspace `[patch.crates-io]` retired | crates.io publishing - upstream owns the `c-scape`/`c-gull`/`eyra` names, registries reject git/path deps, and `[patch]` never propagates to registry consumers; a rename chain is the only shape that resolves end to end from crates.io |
