fn main() {
    // Expose the full target triple (e.g. "x86_64-unknown-linux-gnu") at compile time
    // via env!("TARGET_TRIPLE"). Cargo always sets TARGET in build scripts.
    println!(
        "cargo:rustc-env=TARGET_TRIPLE={}",
        std::env::var("TARGET").unwrap()
    );
}
