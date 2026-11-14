//! Development performance measurements (D-012/D-016) launched by measure.py or directly via --measure.
use gtk::{gdk, glib, prelude::*};
use markdown_linux::render::{DocumentPosition, RenderPlan};
use markdown_linux_core::{CoreDocument, CoreSession};
use std::{path::Path, time::Instant};

const D016_TARGET_BYTES: usize = 1024 * 1024;

unsafe extern "C" {
    fn malloc_trim(pad: usize) -> i32;
}

fn escape_json(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

fn get_rss_kib() -> (u64, u64) {
    let mut vmrss = 0;
    let mut vmhwm = 0;
    if let Ok(content) = std::fs::read_to_string("/proc/self/status") {
        for line in content.lines() {
            if let Some(val) = line.strip_prefix("VmRSS:") {
                vmrss = val
                    .trim()
                    .trim_end_matches(" kB")
                    .trim()
                    .parse()
                    .unwrap_or(0);
            } else if let Some(val) = line.strip_prefix("VmHWM:") {
                vmhwm = val
                    .trim()
                    .trim_end_matches(" kB")
                    .trim()
                    .parse()
                    .unwrap_or(0);
            }
        }
    }
    (vmrss, vmhwm)
}

fn parse_doc(bytes: Vec<u8>) -> Result<CoreDocument, String> {
    let session = CoreSession::new("measure").map_err(|e| format!("session new: {e}"))?;
    let res = session
        .parse_blocking(bytes, None, true)
        .map_err(|e| format!("parse: {e}"))?;
    session.close().map_err(|e| format!("session close: {e}"))?;
    res.document
        .ok_or_else(|| "no document published".to_string())
}

pub fn run(args: &[String]) -> Result<(), String> {
    if args.is_empty() {
        return Err(
            "Usage: markdown-linux --measure <launch|interaction|repeated=N|refresh=N> [document-path]"
                .to_string(),
        );
    }

    let mode = &args[0];
    let doc_path = if args.len() > 1 {
        Some(Path::new(&args[1]))
    } else {
        None
    };

    if mode == "launch" {
        run_launch(doc_path)
    } else if let Some(count_str) = mode.strip_prefix("repeated=") {
        let count: usize = count_str.parse().map_err(|_| "invalid repeated count")?;
        run_repeated(doc_path, count)
    } else if let Some(count_str) = mode.strip_prefix("refresh=") {
        let count: usize = count_str.parse().map_err(|_| "invalid refresh count")?;
        run_refresh(doc_path, count)
    } else if mode == "interaction" {
        run_interaction(doc_path)
    } else {
        Err(format!("unknown measure mode: {mode}"))
    }
}

fn run_launch(doc_path: Option<&Path>) -> Result<(), String> {
    let doc_path = doc_path.ok_or("document path required for launch")?;
    let start_total = Instant::now();

    let bytes = std::fs::read(doc_path).map_err(|e| format!("read file: {e}"))?;
    let read_ms = start_total.elapsed().as_secs_f64() * 1000.0;

    let parse_start = Instant::now();
    let doc = parse_doc(bytes.clone())?;
    let parse_ms = parse_start.elapsed().as_secs_f64() * 1000.0;

    let plan_start = Instant::now();
    let plan = RenderPlan::build(&doc, || false).map_err(|e| format!("build plan: {e:?}"))?;
    let plan_ms = plan_start.elapsed().as_secs_f64() * 1000.0;

    let pipeline_ms = start_total.elapsed().as_secs_f64() * 1000.0;
    let (working_set_kib, peak_rss_kib) = get_rss_kib();

    let file_name = doc_path
        .file_name()
        .map(|s| escape_json(&s.to_string_lossy()))
        .unwrap_or_default();

    let meets_warm = if bytes.len() <= D016_TARGET_BYTES {
        Some(pipeline_ms <= 400.0)
    } else {
        None
    };
    let meets_cold = if bytes.len() <= D016_TARGET_BYTES {
        Some(pipeline_ms <= 700.0)
    } else {
        None
    };

    println!(
        "{{\n  \"mode\": \"launch\",\n  \"document\": \"{file_name}\",\n  \"source_bytes\": {},\n  \"nodes\": {},\n  \"fragments\": {},\n  \"read_ms\": {:.2},\n  \"parse_ms\": {:.2},\n  \"plan_ms\": {:.2},\n  \"pipeline_ms\": {:.2},\n  \"peak_rss_kib\": {peak_rss_kib},\n  \"working_set_kib\": {working_set_kib},\n  \"meets_warm_budget_400ms\": {},\n  \"meets_cold_budget_700ms\": {}\n}}",
        bytes.len(),
        doc.nodes.len(),
        plan.fragments.len(),
        read_ms,
        parse_ms,
        plan_ms,
        pipeline_ms,
        meets_warm.map_or("null".to_string(), |b| b.to_string()),
        meets_cold.map_or("null".to_string(), |b| b.to_string()),
    );

    Ok(())
}

fn run_repeated(doc_path: Option<&Path>, count: usize) -> Result<(), String> {
    let doc_path = doc_path.ok_or("document path required for repeated")?;
    let bytes = std::fs::read(doc_path).map_err(|e| format!("read file: {e}"))?;

    let file_name = doc_path
        .file_name()
        .map(|s| escape_json(&s.to_string_lossy()))
        .unwrap_or_default();

    let (baseline_rss, _) = get_rss_kib();
    let baseline_rss_mib = (baseline_rss as f64) / 1024.0;

    let mut loads_json = Vec::new();
    for i in 1..=count {
        let start = Instant::now();
        let doc = parse_doc(bytes.clone())?;
        let _plan = RenderPlan::build(&doc, || false).map_err(|e| format!("build plan: {e:?}"))?;
        let elapsed_ms = start.elapsed().as_secs_f64() * 1000.0;

        let (before_trim, _) = get_rss_kib();
        unsafe {
            malloc_trim(0);
        }
        let (after_trim, _) = get_rss_kib();

        let retained_rss_mib = (before_trim as f64) / 1024.0;
        let trimmed_rss_mib = (after_trim as f64) / 1024.0;

        loads_json.push(format!(
            "{{\"load\": {i}, \"elapsed_ms\": {:.2}, \"retained_rss_mib\": {:.2}, \"trimmed_rss_mib\": {:.2}}}",
            elapsed_ms, retained_rss_mib, trimmed_rss_mib
        ));
    }

    let (final_rss, _) = get_rss_kib();
    let final_rss_mib = (final_rss as f64) / 1024.0;

    println!(
        "{{\n  \"mode\": \"repeated\",\n  \"document\": \"{file_name}\",\n  \"loads\": {count},\n  \"baseline_rss_mib\": {:.2},\n  \"final_rss_mib\": {:.2},\n  \"loads_data\": [\n    {}\n  ]\n}}",
        baseline_rss_mib,
        final_rss_mib,
        loads_json.join(",\n    ")
    );

    Ok(())
}

fn run_interaction(doc_path: Option<&Path>) -> Result<(), String> {
    let doc_path = doc_path.ok_or("document path required for interaction")?;
    let bytes = std::fs::read(doc_path).map_err(|e| format!("read file: {e}"))?;

    let doc = parse_doc(bytes.clone())?;
    let plan = RenderPlan::build(&doc, || false).map_err(|e| format!("build plan: {e:?}"))?;

    let file_name = doc_path
        .file_name()
        .map(|s| escape_json(&s.to_string_lossy()))
        .unwrap_or_default();

    // 1. Off-screen jump timing (outline resolution to middle, last, first)
    let mut jump_times = Vec::new();
    if !plan.outline.is_empty() {
        let targets = [plan.outline.len() / 2, plan.outline.len() - 1, 0];
        for t in targets {
            let target_frag = plan.outline[t].fragment as usize;
            let sw = Instant::now();
            let frag = &plan.fragments[target_frag];
            let _ = frag.text();
            jump_times.push(sw.elapsed().as_secs_f64() * 1000.0);
        }
    }

    // 2. Select All and copy slicing
    let last_frag = plan.fragments.len().saturating_sub(1);
    let last_len = plan
        .fragments
        .get(last_frag)
        .map_or(0, |f| f.text().chars().count());

    let copy_start = Instant::now();
    let start_pos = DocumentPosition::new(0, 0);
    let end_pos = DocumentPosition::new(last_frag, last_len);
    let text = plan.text_between(start_pos, end_pos);
    let source_opt = plan.source_between(start_pos, end_pos, &doc);
    let source_bytes = source_opt.unwrap_or(text.as_bytes());
    let copy_slice_ms = copy_start.elapsed().as_secs_f64() * 1000.0;

    // 3. Clipboard setting (if GTK display is available)
    let clip_start = Instant::now();
    let mut clipboard_skipped = false;
    let clip_ok = if gtk::init().is_ok()
        && let Some(display) = gdk::Display::default()
    {
        let provider = gdk::ContentProvider::new_union(&[
            gdk::ContentProvider::for_value(&text.to_value()),
            gdk::ContentProvider::for_bytes("text/markdown", &glib::Bytes::from(source_bytes)),
        ]);
        display.clipboard().set_content(Some(&provider)).is_ok()
    } else {
        clipboard_skipped = true;
        false
    };
    let clip_write_ms = clip_start.elapsed().as_secs_f64() * 1000.0;

    // Verify copy accuracy
    let source_matches = source_bytes == bytes.as_slice();
    let text_not_empty = !text.is_empty();
    let copy_verified = source_matches && text_not_empty && (clip_ok || clipboard_skipped);

    let max_jump = jump_times.iter().copied().fold(0.0f64, f64::max);

    let jump_times_str = jump_times
        .iter()
        .map(|t| format!("{t:.2}"))
        .collect::<Vec<_>>()
        .join(", ");

    println!(
        "{{\n  \"mode\": \"interaction\",\n  \"document\": \"{file_name}\",\n  \"jump_targets\": {},\n  \"outline_resolve_times_ms\": [{jump_times_str}],\n  \"outline_resolve_max_ms\": {:.2},\n  \"copy_slice_ms\": {:.2},\n  \"clip_write_ms\": {:.2},\n  \"copy_source_bytes\": {},\n  \"copy_text_chars\": {},\n  \"clip_ok\": {clip_ok},\n  \"clipboard_skipped\": {clipboard_skipped},\n  \"copy_verified\": {copy_verified}\n}}",
        jump_times.len(),
        max_jump,
        copy_slice_ms,
        clip_write_ms,
        source_bytes.len(),
        text.chars().count(),
    );

    Ok(())
}

fn run_refresh(doc_path: Option<&Path>, count: usize) -> Result<(), String> {
    let doc_path = doc_path.ok_or("document path required for refresh")?;
    let original = std::fs::read(doc_path).map_err(|e| format!("read file: {e}"))?;

    let file_name = doc_path
        .file_name()
        .map(|s| escape_json(&s.to_string_lossy()))
        .unwrap_or_default();

    let max_len = original.len().saturating_sub(64);
    let mut cut = original.len();
    for i in (0..=max_len).rev() {
        if original.get(i) == Some(&b'\n') {
            cut = i;
            break;
        }
    }

    let prefix = b"Refreshed from outside.\n\n";
    let mut edited = Vec::with_capacity(prefix.len() + cut);
    edited.extend_from_slice(prefix);
    edited.extend_from_slice(&original[..cut]);

    struct RestoreGuard<'a> {
        path: &'a Path,
        original: &'a [u8],
    }
    impl<'a> Drop for RestoreGuard<'a> {
        fn drop(&mut self) {
            let _ = std::fs::write(self.path, self.original);
        }
    }
    let _guard = RestoreGuard {
        path: doc_path,
        original: &original,
    };

    let session = CoreSession::new("measure-refresh").map_err(|e| format!("session new: {e}"))?;
    let initial_res = session
        .parse_blocking(original.clone(), None, true)
        .map_err(|e| format!("initial parse: {e}"))?;
    let initial_doc = initial_res
        .document
        .ok_or_else(|| "initial parse returned no document".to_string())?;
    let initial_plan = RenderPlan::build(&initial_doc, || false)
        .map_err(|e| format!("initial build plan: {e:?}"))?;

    if initial_plan.outline.is_empty() {
        return Err("document has no headings".to_string());
    }

    let middle_heading = initial_plan.outline.len() / 2;
    let expected_title = initial_plan.outline[middle_heading].title.clone();
    let middle_frag = initial_plan.outline[middle_heading].fragment as usize;
    let middle_offset =
        initial_plan.source_offset_at(DocumentPosition::new(middle_frag, 0), &initial_doc);
    let initial_anchor = initial_doc
        .anchor_at(middle_offset)
        .map_err(|e| format!("failed to anchor middle heading: {e:?}"))?;

    let (baseline_rss, _) = get_rss_kib();
    let baseline_rss_mib = (baseline_rss as f64) / 1024.0;

    let mut results = Vec::new();
    for iteration in 1..=count {
        let bytes_to_write = if iteration % 2 == 1 {
            &edited
        } else {
            &original
        };
        let sw = Instant::now();
        std::fs::write(doc_path, bytes_to_write).map_err(|e| format!("write file: {e}"))?;

        let read_bytes = std::fs::read(doc_path).map_err(|e| format!("read file: {e}"))?;
        let parse_res = session
            .parse_blocking(read_bytes, None, false)
            .map_err(|e| format!("refresh parse: {e}"))?;
        let new_doc = parse_res
            .document
            .ok_or_else(|| "refresh parse returned no document".to_string())?;
        let new_plan = RenderPlan::build(&new_doc, || false)
            .map_err(|e| format!("refresh build plan: {e:?}"))?;
        let refresh_ms = sw.elapsed().as_secs_f64() * 1000.0;

        let top_res = new_doc
            .resolve_anchor(&initial_anchor)
            .map_err(|e| format!("resolve anchor: {e:?}"))?;
        let restored_pos =
            top_res.map(|r| new_plan.position_at_source_offset(r.source_offset, &new_doc));
        let kept_place = if let Some(pos) = restored_pos {
            pos.fragment_index < new_plan.fragments.len()
                && (new_plan.fragments[pos.fragment_index]
                    .text()
                    .contains(&expected_title)
                    || new_plan.outline.iter().any(|o| {
                        o.title == expected_title
                            && (o.fragment as isize - pos.fragment_index as isize).abs() <= 1
                    }))
        } else {
            false
        };

        unsafe {
            malloc_trim(0);
        }
        let (current_rss, _) = get_rss_kib();
        let current_rss_mib = (current_rss as f64) / 1024.0;

        results.push(format!(
            "{{\"iteration\": {iteration}, \"refresh_ms\": {:.2}, \"kept_place\": {kept_place}, \"baseline_footprint_mib\": {:.2}, \"footprint_mib\": {:.2}}}",
            refresh_ms, baseline_rss_mib, current_rss_mib
        ));
    }

    session.close().map_err(|e| format!("session close: {e}"))?;

    println!(
        "{{\n  \"mode\": \"refresh\",\n  \"document\": \"{file_name}\",\n  \"refreshes\": {count},\n  \"baseline_footprint_mib\": {:.2},\n  \"refreshes_data\": [\n    {}\n  ]\n}}",
        baseline_rss_mib,
        results.join(",\n    ")
    );

    Ok(())
}
