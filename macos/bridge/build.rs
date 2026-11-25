//! Generate the native macOS adapter's typed C++ interface.
fn main() {
    cxx_build::bridge("src/lib.rs")
        .std("c++17")
        .compile("drink-bridge");
    println!("cargo:rerun-if-changed=src/lib.rs");
}
