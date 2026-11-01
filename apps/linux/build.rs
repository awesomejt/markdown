//! The core adapter's link arguments do not reach this binary, so repeat its RUNPATH.

fn main() {
    let runpath =
        std::env::var("DEP_MARKDOWN_FFI_RUNPATH").expect("markdown-linux-core build script");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{runpath}");
}
