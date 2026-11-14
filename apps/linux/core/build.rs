//! Link the staged model-v2 core library whose configuration matches this profile.
//! `apps/linux/tools/package_core.py --configuration <debug|release>` stages it.
//!
//! Development binaries find the library through a RUNPATH relative to their own
//! directory (`$ORIGIN`), so the build tree can move; nothing absolute is embedded
//! when the package and target directory share a root. Packaging (LNX-001I)
//! installs the library and sets the installed binary's own RUNPATH.

use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-env-changed=MARKDOWN_CORE_PACKAGE");
    // Cargo's PROFILE is "debug" or "release" for every built-in profile (test, bench too).
    let profile = std::env::var("PROFILE").expect("Cargo sets PROFILE");
    let package = match std::env::var_os("MARKDOWN_CORE_PACKAGE") {
        Some(path) => PathBuf::from(path),
        None => PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../../build/linux/core-package")
            .join(&profile),
    };
    let identity_path = package.join("identity");
    let library = package.join("lib/libmarkdown_ffi.so");
    println!("cargo:rerun-if-changed={}", identity_path.display());
    println!("cargo:rerun-if-changed={}", library.display());
    let identity = std::fs::read_to_string(&identity_path).unwrap_or_else(|_| {
        panic!(
            "no core package at {}: run `python3 apps/linux/tools/package_core.py \
             --configuration {profile}` from the repository root",
            package.display()
        )
    });
    let field = |name: &str| -> &str {
        identity
            .lines()
            .find_map(|line| line.strip_prefix(name)?.strip_prefix('='))
            .unwrap_or_else(|| panic!("{} lacks {name}", identity_path.display()))
    };
    let configuration = field("configuration");
    if configuration != profile {
        panic!(
            "core package {} is {configuration}, but this build is {profile}",
            package.display()
        );
    }
    if !library.is_file() {
        panic!("{} is missing", library.display());
    }
    println!("cargo:rustc-env=MD_PACKAGE_CONFIGURATION={configuration}");
    println!("cargo:rustc-env=MD_PACKAGE_FINGERPRINT={}", field("sha256"));
    println!(
        "cargo:rustc-link-search=native={}",
        package.join("lib").display()
    );
    println!("cargo:rustc-link-lib=dylib=markdown_ffi");
    // Dependents (the GTK app) read DEP_MARKDOWN_FFI_RUNPATH; link args do not propagate.
    let runpath = runpath(&package.join("lib"));
    println!("cargo:runpath={runpath}");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{runpath}");
}

/// RUNPATH entries for binaries in the profile directory and in its `deps/`
/// (tests), relative to `$ORIGIN` where possible.
fn runpath(library_dir: &Path) -> String {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("Cargo sets OUT_DIR"));
    // OUT_DIR is <target>/<profile>/build/<package>-<hash>/out.
    let profile_dir = out_dir.ancestors().nth(3).expect("OUT_DIR layout");
    let (Ok(profile_dir), Ok(library_dir)) =
        (profile_dir.canonicalize(), library_dir.canonicalize())
    else {
        panic!("cannot resolve {}", library_dir.display());
    };
    let common = profile_dir
        .ancestors()
        .find(|ancestor| library_dir.starts_with(ancestor))
        .expect("absolute paths share the root");
    if common == Path::new("/") {
        return library_dir.display().to_string();
    }
    let up = profile_dir
        .strip_prefix(common)
        .unwrap()
        .components()
        .count();
    let down = library_dir
        .strip_prefix(common)
        .unwrap()
        .display()
        .to_string();
    let relative = |extra: usize| format!("$ORIGIN/{}{down}", "../".repeat(up + extra));
    format!("{}:{}", relative(0), relative(1))
}
