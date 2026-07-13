fn main() {
    // these ride the build script (not .cargo/config) so registry consumers
    // link the same way source builds do
    // Eyra provides its own startup; disable host startfiles (harmless in a .so).
    println!("cargo:rustc-link-arg=-nostartfiles");
    // no glibc under our libc; export the c-gull symbols; name it libc.so.6
    println!("cargo:rustc-link-arg=-nodefaultlibs");
    println!("cargo:rustc-link-arg=-Wl,--export-dynamic");
    println!("cargo:rustc-link-arg=-Wl,-soname,libc.so.6");
    // first definition wins (rustc orders dependents first): c-scape's full
    // getauxval shadows origin's few-types shim, which shares a CGU with
    // origin's always-linked entry code. see PATCHES.md.
    println!("cargo:rustc-link-arg=-Wl,--allow-multiple-definition");
}
