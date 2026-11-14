//! The core adapter's link arguments do not reach this binary, so repeat its RUNPATH.

fn main() {
    println!("cargo:rerun-if-env-changed=MARKDOWN_PACKAGED_RUNPATH");
    let runpath = match std::env::var("MARKDOWN_PACKAGED_RUNPATH") {
        Ok(value) if !value.is_empty() => value,
        _ => std::env::var("DEP_MARKDOWN_FFI_RUNPATH").expect("markdown-linux-core build script"),
    };
    println!("cargo:rustc-link-arg=-Wl,-rpath,{runpath}");
}
