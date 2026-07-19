fn main() {
    println!("cargo:rustc-check-cfg=cfg(origin_relocation_pic)");
    // pic is the default relocation model for every target origin's
    // relocate path supports; a consumer building -C relocation-model=static
    // opts out here and the relocate code compiles away
    let flags = std::env::var("CARGO_ENCODED_RUSTFLAGS").unwrap_or_default();
    let non_pic = flags
        .split('\u{1f}')
        .collect::<Vec<_>>()
        .windows(2)
        .any(|w| w[0] == "-C" && w[1].starts_with("relocation-model=") && w[1] != "relocation-model=pic")
        || flags.split('\u{1f}').any(|f| {
            f.starts_with("-Crelocation-model=") && f != "-Crelocation-model=pic"
        });
    if !non_pic {
        println!("cargo:rustc-cfg=origin_relocation_pic");
    }
}
