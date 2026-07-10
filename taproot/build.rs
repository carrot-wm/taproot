fn main() {
    // Eyra provides its own startup; disable host startfiles (harmless in a .so).
    println!("cargo:rustc-link-arg=-nostartfiles");
}
