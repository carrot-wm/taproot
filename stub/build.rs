fn main() {
    // no startfiles, no host libs: the artifact must not name a real
    // glibc anywhere - owning a legacy soname is its entire job
    println!("cargo:rustc-link-arg=-nostartfiles");
    println!("cargo:rustc-link-arg=-nodefaultlibs");
}
