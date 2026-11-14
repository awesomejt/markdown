//! Two-phase relaunch restoration verification (LNX-001H).
//!
//! Launched twice by `tools/check.py`:
//! `markdown-linux --relaunch-check prepare <file>`
//! `markdown-linux --relaunch-check verify <file>`

use std::path::{Path, PathBuf};
use std::time::Duration;

use gtk::prelude::*;
use gtk::{gio, glib};

use markdown_linux::render::DocumentPosition;
use markdown_linux::restoration::{self, WindowRestorationState};

pub fn run(args: &[String]) -> Result<(), String> {
    if args.len() < 2 {
        return Err("Usage: markdown-linux --relaunch-check <prepare|verify> <file>".into());
    }
    let phase = &args[0];
    let file_path = PathBuf::from(&args[1]);

    if !file_path.exists() {
        return Err(format!("file does not exist: {}", file_path.display()));
    }

    let phase = phase.clone();
    let app = gtk::Application::builder()
        .application_id(crate::APP_ID)
        .flags(gio::ApplicationFlags::NON_UNIQUE | gio::ApplicationFlags::HANDLES_OPEN)
        .build();

    app.connect_startup(crate::install_commands);

    app.connect_activate(move |app| {
        let window = crate::new_window(app);
        let path = file_path.clone();
        let phase = phase.clone();
        let app_handle = app.clone();

        glib::spawn_future_local(async move {
            let res = match phase.as_str() {
                "prepare" => run_prepare(&app_handle, &window, &path).await,
                "verify" => run_verify(&app_handle, &window, &path).await,
                other => Err(format!("unknown relaunch phase: {other}")),
            };
            if let Err(e) = res {
                eprintln!("relaunch-check: FAIL: {e}");
                std::process::exit(1);
            }
        });
    });

    let exit_code = app.run_with_args(&["markdown-linux"]);
    if exit_code == glib::ExitCode::SUCCESS {
        Ok(())
    } else {
        Err(format!("application exited with {exit_code:?}"))
    }
}

async fn wait_for<F: Fn() -> bool>(desc: &str, condition: F) -> Result<(), String> {
    let start = std::time::Instant::now();
    while !condition() {
        if start.elapsed() > Duration::from_secs(10) {
            return Err(format!("timed out waiting for {desc}"));
        }
        glib::timeout_future(Duration::from_millis(20)).await;
    }
    Ok(())
}

async fn run_prepare(
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    file_path: &Path,
) -> Result<(), String> {
    wait_for("window is mapped in prepare", || window.is_mapped()).await?;
    crate::files::open_for_smoke(window, &gio::File::for_path(file_path));

    wait_for("document loaded in prepare", || {
        crate::files::reader(window).is_some_and(|r| r.document().is_some())
    })
    .await?;

    let reader = crate::files::reader(window).ok_or("reader missing")?;
    let doc = reader.document().ok_or("document missing")?;
    let plan = reader.plan().ok_or("plan missing")?;

    let heading_index = doc
        .outline
        .iter()
        .position(|entry| doc.text(entry.title).as_deref() == Ok("Part 80"))
        .ok_or("heading Part 80 not found in outline")?;

    let outline_frag = plan
        .outline
        .get(heading_index)
        .map(|e| e.fragment as usize)
        .unwrap_or(0);

    reader.activate_outline(heading_index as u32);
    wait_for("target fragment is bound in prepare", || {
        reader
            .observed()
            .iter()
            .any(|(idx, _)| (*idx as isize - outline_frag as isize).abs() <= 1)
    })
    .await?;

    let mut found_frag = None;
    let mut word_start = 0;

    for f_idx in outline_frag..plan.fragments.len().min(outline_frag + 15) {
        let text = plan.fragments[f_idx].text();
        if let Some(pos) = text.find("marked") {
            found_frag = Some(f_idx);
            word_start = text[..pos].chars().count();
            break;
        }
    }

    let found_frag = found_frag.ok_or("word 'marked' not found near Part 80")?;
    let word_len = "marked".chars().count();
    reader.set_selection(Some(markdown_linux::render::DocumentSelection::new(
        DocumentPosition::new(found_frag, word_start),
        DocumentPosition::new(found_frag, word_start + word_len),
    )));

    let saved_pos = reader.save_position(&doc).ok_or("save_position failed")?;
    let state = WindowRestorationState::from_saved_position(
        file_path.to_string_lossy().to_string(),
        Some(&saved_pos),
        reader.text_scale(),
        reader.is_outline_visible(),
    );

    restoration::save(&[state]);
    println!("relaunch-check: prepared");
    app.quit();
    Ok(())
}

async fn run_verify(
    app: &gtk::Application,
    window: &gtk::ApplicationWindow,
    file_path: &Path,
) -> Result<(), String> {
    wait_for("window is mapped in verify", || window.is_mapped()).await?;
    let states = restoration::load();
    if states.is_empty() {
        return Err("expected at least 1 saved restoration state, got 0".into());
    }
    let state = &states[0];
    let saved_path = PathBuf::from(&state.file_path);
    if saved_path.canonicalize().unwrap_or(saved_path.clone())
        != file_path.canonicalize().unwrap_or(file_path.to_path_buf())
    {
        return Err(format!(
            "saved path mismatch: {} vs {}",
            state.file_path,
            file_path.display()
        ));
    }

    crate::files::open_restored_state(window, state);

    wait_for("document loaded in verify", || {
        crate::files::reader(window).is_some_and(|r| r.document().is_some())
    })
    .await?;

    let reader = crate::files::reader(window).ok_or("reader missing")?;
    let doc = reader.document().ok_or("document missing")?;
    let plan = reader.plan().ok_or("plan missing")?;

    let heading_index = doc
        .outline
        .iter()
        .position(|entry| doc.text(entry.title).as_deref() == Ok("Part 80"))
        .ok_or("heading Part 80 not found in restored outline")?;

    let target_frag = plan.outline[heading_index].fragment;

    wait_for("target fragment is bound in verify", || {
        crate::files::reader(window).is_some_and(|r| {
            r.observed()
                .iter()
                .any(|(idx, _)| (*idx as isize - target_frag as isize).abs() <= 1)
        })
    })
    .await?;

    let first_visible = reader
        .first_visible_fragment()
        .ok_or("no visible fragment")?;
    if (first_visible as isize - target_frag as isize).abs() > 2 {
        return Err(format!(
            "top fragment mismatch: first_visible={first_visible}, target_frag={target_frag}"
        ));
    }

    let sel_text = reader.selected_text();
    if sel_text.as_deref() != Some("marked") {
        return Err(format!("restored selection mismatch: {sel_text:?}"));
    }

    if reader.is_outline_visible() != state.outline_shown {
        return Err("restored outline visibility mismatch".into());
    }

    if (reader.text_scale() - state.text_scale).abs() > 0.001 {
        return Err("restored text scale mismatch".into());
    }

    restoration::clear();
    println!("relaunch-check: pass (restored window, document, top line and selection)");
    app.quit();
    Ok(())
}
