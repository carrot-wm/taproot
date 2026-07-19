#!/usr/bin/env bash
# taproot link shim (see PATCHES.md).
#
# Stable rustc's prebuilt compiler-builtins defines the C math and int
# builtins (ceil, sqrt, __clzdi2, ...) as WEAK HIDDEN aliases of its
# internal implementations. ELF linking merges the most constraining
# visibility per symbol name, so those dead weak copies demote the
# libc's own default-visibility definitions to hidden, and the cdylib
# stops exporting them (the strong c-scape definition still wins the
# resolution; only the visibility is poisoned). Rename the shadow
# copies in a patched copy of the rlib so c-scape's definitions own
# the names. A no-op on toolchains whose compiler-builtins doesn't
# carry the aliases (the pinned nightly), and for members that don't
# define them: --redefine-sym ignores absent names.
set -euo pipefail

renames=(
    __clzdi2 __ctzdi2
    cbrt cbrtf ceil ceilf copysign copysignf fabs fabsf fdim fdimf
    floor floorf fma fmaf fmax fmaxf fmin fminf fmod fmodf
    rint rintf round roundf sqrt sqrtf trunc truncf
)

tmp=""
trap '[ -n "$tmp" ] && rm -rf "$tmp"' EXIT

args=("$@")
for i in "${!args[@]}"; do
    case "${args[$i]}" in
    */libcompiler_builtins-*.rlib)
        objcopy=$(cc -print-prog-name=objcopy)
        flags=()
        for s in "${renames[@]}"; do
            flags+=("--redefine-sym" "$s=__taproot_shadowed_$s")
        done
        tmp=$(mktemp -d)
        patched="$tmp/$(basename "${args[$i]}")"
        "$objcopy" "${flags[@]}" "${args[$i]}" "$patched"
        args[$i]="$patched"
        ;;
    esac
done

cc "${args[@]}"
