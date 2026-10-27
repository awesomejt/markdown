//! Header drift checks and the current-host C package smoke.
//!
//! `header_constants_match_rust` parses `core/include/markdown_core.h`.
//! `c_smoke` builds the `markdown_ffi` shared library for this test's profile
//! and features, compiles `tests/c/smoke.c` with the host C compiler (`$CC`,
//! else `cc`), links a private copy of the library and runs it: once to
//! compare every struct size/field offset with Rust, once to exercise the ABI
//! (asserting the model mask the linked library reports).
//! Without a compiler the smoke is skipped unless `MD_REQUIRE_C_SMOKE=1`
//! (set by `tools/check.py` on macOS/Linux), which turns a skip into a failure.
//! Windows always skips the C smoke (MSVC/import-library wiring is WIN-001B);
//! the drift check of constants still runs there.
use markdown_ffi::abi::*;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}
fn header() -> PathBuf {
    root().join("core/include/markdown_core.h")
}

#[test]
fn header_constants_match_rust() {
    let text = std::fs::read_to_string(header()).unwrap();
    // Drop /* ... */ comments, then read `#define MD_X N` and `MD_X = N` items.
    let mut code = String::new();
    let mut rest = text.as_str();
    while let Some(start) = rest.find("/*") {
        code.push_str(&rest[..start]);
        rest = &rest[start + rest[start..].find("*/").unwrap() + 2..];
    }
    code.push_str(rest);
    let parse = |value: &str| match value.trim().trim_end_matches('u').strip_prefix("0x") {
        Some(hex) => u64::from_str_radix(hex, 16).unwrap(),
        None => value.trim().trim_end_matches('u').parse::<u64>().unwrap(),
    };
    let mut header = BTreeMap::new();
    let mut add = |name: &str, value: u64| {
        assert!(
            header.insert(name.to_owned(), value).is_none(),
            "duplicate {name}"
        );
    };
    for line in code.lines() {
        if let Some(define) = line.trim().strip_prefix("#define ") {
            if let Some((name, value)) = define.split_once(' ')
                && name.starts_with("MD_")
            {
                add(name, parse(value));
            }
            continue;
        }
        for item in line.split([',', '{', '}', ';']) {
            if let Some((name, value)) = item.split_once('=')
                && name.trim().starts_with("MD_")
            {
                add(name.trim(), parse(value));
            }
        }
    }
    let rust: BTreeMap<String, u64> = CONSTANTS
        .iter()
        .map(|(name, value)| ((*name).to_owned(), *value))
        .collect();
    assert_eq!(header, rust);
    assert!(header.len() > 100);
}

macro_rules! layout {
    ($($ty:ident { $($field:ident),+ $(,)? })+) => {{
        let mut map = BTreeMap::new();
        $(
            map.insert(format!("size {}", stringify!($ty)), size_of::<$ty>());
            $(map.insert(
                format!("offset {}.{}", stringify!($ty), stringify!($field)),
                std::mem::offset_of!($ty, $field),
            );)+
        )+
        map
    }};
}

fn rust_layout() -> BTreeMap<String, usize> {
    layout! {
        md_str { offset, length }
        md_abi_info {
            struct_size, abi_version, model_versions, anchor_version, pointer_size,
            max_anchor_bytes, session_options_size, session_status_size, job_result_size,
            snapshot_view_size, node_size, intent_size, outline_entry_size, diagnostic_size,
            property_size, anchor_resolution_size,
        }
        md_session_options {
            struct_size, model_version, max_source_bytes, max_nodes, max_depth, max_events,
            max_text_bytes,
        }
        md_session_status_info {
            struct_size, flags, latest, confirmed, pending, published, failure_generation,
            availability, unreadable_reason, failure, reserved,
        }
        md_job_result { struct_size, outcome, generation, latest, snapshot, failure, reserved }
        md_node {
            kind, parent, first_child, child_count, span_start, span_end, flags, value, number,
            id, text, extra, aux, aux_count,
        }
        md_intent {
            id, destination, target, fragment, span_start, span_end, definition_start,
            definition_end, source_kind, target_kind, denial, flags,
        }
        md_outline_entry { node, level, title, anchor }
        md_diagnostic { code, message, severity, flags, span_start, span_end, node, reserved }
        md_property { key, kind, boolean, integer, string, first_string, string_count }
        md_snapshot_view {
            struct_size, model_version, generation, source_length, strings_length, source,
            strings, nodes, roots, children, alignments, links, resources, outline, diagnostics,
            properties, string_list, node_count, root_count, child_count, alignment_count,
            link_count, resource_count, outline_count, diagnostic_count, property_count,
            string_list_count, capabilities, flags, front_matter_start, front_matter_end,
            document_id, revision_id, source_sha256, title,
        }
        md_anchor_resolution { struct_size, quality, node, reserved, offset, source_offset }
    }
}

const LIBRARY: &str = if cfg!(target_os = "macos") {
    "libmarkdown_ffi.dylib"
} else {
    "libmarkdown_ffi.so"
};

/// Build the shared library for exactly this test binary's profile and
/// feature set. The cdylib cargo leaves in `target/<profile>/deps` is not
/// feature-hashed and is not rewritten when a previously built feature set is
/// fresh, so it can belong to a different configuration. A nested build into
/// a target directory keyed by profile and model mask is always the matching
/// one (and incremental after the first run); `--offline --locked` forbids
/// network access or lock changes.
fn built_library() -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let target = root().join(format!(
        "target/markdown-ffi-smoke/{profile}-models{}",
        expected_models()
    ));
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut command = Command::new(cargo);
    command
        .args([
            "build",
            "--package",
            "markdown-ffi",
            "--lib",
            "--locked",
            "--offline",
        ])
        .arg("--target-dir")
        .arg(&target)
        .current_dir(root());
    if !cfg!(debug_assertions) {
        command.arg("--release");
    }
    if cfg!(feature = "model-v2") {
        command.args(["--features", "model-v2"]);
    }
    let status = command.status().expect("run cargo build");
    assert!(status.success(), "building the library failed: {command:?}");
    let library = target.join(profile).join(LIBRARY);
    assert!(library.is_file(), "missing {}", library.display());
    library
}

/// `model_versions` this test binary's feature set implies.
fn expected_models() -> u32 {
    let mut mask = 1 << MD_MODEL_V1;
    if cfg!(feature = "model-v2") {
        mask |= 1 << MD_MODEL_V2;
    }
    mask
}

/// Copy the library into a fresh per-run directory, so concurrent or later
/// builds cannot swap it underneath the smoke. On macOS also give the copy an
/// `@rpath` install name: a cargo-built dylib records its absolute build
/// path, which would make the smoke load that file instead of the copy.
fn stage_library(dir: &Path) {
    std::fs::copy(built_library(), dir.join(LIBRARY)).unwrap();
    if cfg!(target_os = "macos") {
        let status = Command::new("install_name_tool")
            .arg("-id")
            .arg(format!("@rpath/{LIBRARY}"))
            .arg(dir.join(LIBRARY))
            .status()
            .expect("run install_name_tool");
        assert!(status.success(), "install_name_tool failed");
    }
}

fn compiler() -> Option<String> {
    if cfg!(windows) {
        // Not wired up: MSVC needs a developer environment and links the
        // `markdown_ffi.dll.lib` import library. WIN-001B owns that consumer.
        return None;
    }
    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".into());
    Command::new(&cc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|_| cc)
}

fn build_smoke(cc: &str, out: &Path) -> PathBuf {
    stage_library(out);
    let lib = out;
    let exe = out.join("smoke");
    let mut command = Command::new(cc);
    command
        .args([
            "-std=c11",
            "-Wall",
            "-Wextra",
            "-Werror",
            "-pedantic",
            "-O1",
        ])
        .arg("-I")
        .arg(root().join("core/include"))
        .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/c/smoke.c"))
        .arg("-o")
        .arg(&exe)
        .arg(format!("-L{}", lib.display()))
        .arg("-lmarkdown_ffi")
        .arg(format!("-Wl,-rpath,{}", lib.display()));
    if cfg!(target_os = "linux") {
        // DT_RPATH (not RUNPATH) is searched before LD_LIBRARY_PATH; dladdr
        // lives in libdl on glibc before 2.34.
        command.args(["-Wl,--disable-new-dtags", "-ldl"]);
    }
    let status = command.status().expect("run C compiler");
    assert!(
        status.success(),
        "C smoke failed to compile/link: {command:?}"
    );
    exe
}

/// Run the smoke without the loader search paths cargo test sets (it puts
/// `target/<profile>` and `deps` on them), so only the staged copy can load.
fn smoke(exe: &Path) -> Command {
    let mut command = Command::new(exe);
    for variable in [
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "DYLD_FALLBACK_LIBRARY_PATH",
    ] {
        command.env_remove(variable);
    }
    command
}

#[test]
fn c_smoke() {
    let required = std::env::var("MD_REQUIRE_C_SMOKE").is_ok_and(|v| v == "1");
    let Some(cc) = compiler() else {
        assert!(
            !required,
            "MD_REQUIRE_C_SMOKE=1 but no C compiler was found"
        );
        eprintln!("SKIPPED: no C compiler found; C package smoke not run on this host");
        return;
    };
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!(
        "markdown-ffi-c-smoke-{profile}-models{}-{}",
        expected_models(),
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&out);
    std::fs::create_dir_all(&out).unwrap();
    let exe = build_smoke(&cc, &out);

    let layout = smoke(&exe).arg("layout").output().unwrap();
    assert!(layout.status.success());
    let c: BTreeMap<String, usize> = String::from_utf8(layout.stdout)
        .unwrap()
        .lines()
        .map(|line| {
            let (key, value) = line.rsplit_once(' ').unwrap();
            (key.to_owned(), value.parse().unwrap())
        })
        .collect();
    assert_eq!(
        c,
        rust_layout(),
        "C header layout differs from Rust repr(C)"
    );

    let run = smoke(&exe)
        .arg("run")
        .arg(expected_models().to_string())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&run.stdout);
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(run.status.success(), "C smoke failed:\n{stdout}\n{stderr}");
    assert!(stdout.contains("c smoke ok"), "{stdout}");
    let loaded = stdout
        .lines()
        .find_map(|line| line.strip_prefix("library "))
        .expect("smoke reports the loaded library");
    assert_eq!(
        std::fs::canonicalize(loaded).unwrap(),
        std::fs::canonicalize(out.join(LIBRARY)).unwrap(),
        "the smoke loaded a library other than the staged copy"
    );
    eprintln!("{}", stdout.trim());
    let _ = std::fs::remove_dir_all(&out);
}
