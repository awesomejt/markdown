use markdown_core::{
    CancellationToken, Document, DocumentId, ModelLimits, ParseLimits, RevisionId, parse_markdown,
    parse_markdown_cancellable, parse_markdown_v2, parse_markdown_v2_cancellable, v2,
};
use markdown_ffi::abi::*;
use markdown_ffi::{
    md_abi_negotiate, md_job_release, md_job_run, md_live_handles, md_session_create,
    md_session_current, md_session_release, md_session_submit, md_snapshot_anchor_at,
    md_snapshot_anchor_for_node, md_snapshot_get_view, md_snapshot_release,
    md_snapshot_resolve_anchor,
};
use std::mem::size_of;
use std::ptr::null;
use std::slice;

unsafe extern "C" {
    fn md_snapshot_find(
        snapshot: md_snapshot,
        query: *const u8,
        query_length: usize,
        options: *const md_find_options,
        matches: *mut md_find_match,
        match_capacity: usize,
        segments: *mut md_match_segment,
        segment_capacity: usize,
        out_result: *mut md_find_result,
    ) -> md_status;

    fn md_snapshot_read_stats(
        snapshot: md_snapshot,
        options: *const md_read_options,
        sections: *mut md_read_section,
        section_capacity: usize,
        out_stats: *mut md_read_stats,
    ) -> md_status;

    fn md_workspace_classify_path(
        path: *const u8,
        path_length: usize,
        out_info: *mut md_workspace_path_info,
    ) -> md_status;

    fn md_workspace_find_names(
        paths: *const u8,
        paths_length: usize,
        entries: *const md_workspace_entry,
        entry_count: usize,
        query: *const u8,
        query_length: usize,
        options: *const md_name_options,
        out_hits: *mut md_name_hit,
        hit_capacity: usize,
        out_result: *mut md_name_result,
    ) -> md_status;

    fn md_workspace_content_admit(
        query: *const u8,
        query_length: usize,
        options: *const md_content_options,
        path: *const u8,
        path_length: usize,
        byte_length: u64,
        progress: *mut md_content_progress,
        out_outcome: *mut u32,
        out_reason: *mut u32,
    ) -> md_status;

    fn md_workspace_content_search(
        query: *const u8,
        query_length: usize,
        options: *const md_content_options,
        path: *const u8,
        path_length: usize,
        content: *const u8,
        content_length: usize,
        progress: *mut md_content_progress,
        hits: *mut md_content_hit,
        hit_capacity: usize,
        text: *mut u8,
        text_capacity: usize,
        anchors: *mut u8,
        anchor_capacity: usize,
        out_result: *mut md_content_result,
    ) -> md_status;
}

unsafe fn slice_from_raw<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if ptr.is_null() || len == 0 {
        &[]
    } else {
        // SAFETY: libFuzzer passes a valid `ptr` of `len` bytes for the duration of the call.
        unsafe { slice::from_raw_parts(ptr, len) }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn md_fuzz_parser(ptr: *const u8, len: usize) {
    // SAFETY: Caller (libFuzzer) supplies a valid byte slice for the call.
    let data = unsafe { slice_from_raw(ptr, len) };
    let Ok(source) = std::str::from_utf8(data) else {
        return;
    };

    let doc_id = DocumentId("fuzz".into());
    let rev = RevisionId("rev-1".into());

    if let Ok(doc) = parse_markdown(source, doc_id.clone(), rev.clone(), ParseLimits::default()) {
        if let Ok(json) = doc.to_json() {
            let decoded = Document::from_json(&json, source, ModelLimits::default())
                .expect("parsed v1 document must round-trip through JSON");
            assert_eq!(doc.input().nodes.len(), decoded.input().nodes.len());
        }
        let token = CancellationToken::new();
        let _ = parse_markdown_cancellable(source, doc_id.clone(), rev.clone(), ParseLimits::default(), &token);
    }

    if let Ok(doc_v2) = parse_markdown_v2(source, doc_id.clone(), rev.clone(), ParseLimits::default()) {
        if let Ok(json) = doc_v2.to_json() {
            let decoded = v2::Document::from_json(&json, source, ModelLimits::default())
                .expect("parsed v2 document must round-trip through JSON");
            assert_eq!(doc_v2.input().nodes.len(), decoded.input().nodes.len());
        }
        let token = CancellationToken::new();
        let _ = parse_markdown_v2_cancellable(source, doc_id, rev, ParseLimits::default(), &token);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn md_fuzz_model_decoder(ptr: *const u8, len: usize) {
    // SAFETY: Caller (libFuzzer) supplies a valid byte slice for the call.
    let data = unsafe { slice_from_raw(ptr, len) };

    // Split into (source, json_bytes) using a two-byte length prefix when available.
    let (source_bytes, json_bytes) = if data.len() >= 2 {
        let split = (u16::from_le_bytes([data[0], data[1]]) as usize) % (data.len() - 1);
        (&data[2..2 + split], &data[2 + split..])
    } else {
        (&[][..], data)
    };
    let source = String::from_utf8_lossy(source_bytes);

    let _ = Document::from_json(json_bytes, &source, ModelLimits::default());
    let _ = v2::Document::from_json(json_bytes, &source, ModelLimits::default());
    // Also exercise raw bytes directly as JSON against an empty source.
    let _ = Document::from_json(data, "", ModelLimits::default());
    let _ = v2::Document::from_json(data, "", ModelLimits::default());
}

#[unsafe(no_mangle)]
pub extern "C" fn md_fuzz_ffi_session_find_anchor(ptr: *const u8, len: usize) {
    // SAFETY: Caller (libFuzzer) supplies a valid byte slice for the call.
    let data = unsafe { slice_from_raw(ptr, len) };

    let mut abi_info = md_abi_info {
        struct_size: size_of::<md_abi_info>() as u32,
        ..Default::default()
    };
    // SAFETY: `abi_info` is a valid mutable stack struct.
    assert_eq!(
        unsafe { md_abi_negotiate(MD_ABI_VERSION, &mut abi_info) },
        MD_OK
    );

    // Partition input: [doc_bytes | query_bytes | anchor_bytes]
    let (doc_bytes, rest) = if data.len() >= 4 {
        let doc_len = (u16::from_le_bytes([data[0], data[1]]) as usize) % (data.len() - 3);
        (&data[4..4 + doc_len], &data[4 + doc_len..])
    } else {
        (data, &[][..])
    };
    let (query_bytes, anchor_bytes) = if rest.len() >= 2 {
        let q_len = (rest[0] as usize) % rest.len();
        (&rest[1..1 + q_len], &rest[1 + q_len..])
    } else {
        (rest, data)
    };

    let doc_id = b"fuzz-doc";
    let mut session: md_session = 0;
    let opts = md_session_options {
        struct_size: size_of::<md_session_options>() as u32,
        model_version: 2,
        ..Default::default()
    };
    // SAFETY: All pointers and lengths reference valid stack/slice buffers.
    let status = unsafe {
        md_session_create(
            doc_id.as_ptr(),
            doc_id.len(),
            &opts,
            &mut session,
        )
    };
    if status != MD_OK {
        return;
    }

    let token = b"rev-1";
    let mut job: md_job = 0;
    let mut generation: u64 = 0;
    // SAFETY: `doc_bytes` and `token` are valid for their lengths.
    let submit_status = unsafe {
        md_session_submit(
            session,
            if doc_bytes.is_empty() { null() } else { doc_bytes.as_ptr() },
            doc_bytes.len(),
            token.as_ptr(),
            token.len(),
            &mut job,
            &mut generation,
        )
    };
    if submit_status == MD_OK && job != 0 {
        let mut job_res = md_job_result {
            struct_size: size_of::<md_job_result>() as u32,
            ..Default::default()
        };
        // SAFETY: `job_res` is valid on the stack.
        let _ = unsafe { md_job_run(job, &mut job_res) };
        if job_res.snapshot != 0 {
            let _ = md_snapshot_release(job_res.snapshot);
        }
        let _ = md_job_release(job);
    }

    let mut snapshot: md_snapshot = 0;
    // SAFETY: `snapshot` is a valid out-pointer.
    if unsafe { md_session_current(session, &mut snapshot) } == MD_OK && snapshot != 0 {
        let mut view = md_snapshot_view {
            struct_size: size_of::<md_snapshot_view>() as u32,
            ..Default::default()
        };
        // SAFETY: `view` is valid on the stack.
        let _ = unsafe { md_snapshot_get_view(snapshot, &mut view) };

        // Fuzz find with valid caller-owned buffers.
        let find_opts = md_find_options {
            struct_size: size_of::<md_find_options>() as u32,
            flags: 0,
            max_matches: 64,
            match_size: size_of::<md_find_match>() as u32,
            segment_size: size_of::<md_match_segment>() as u32,
            reserved: 0,
        };
        let mut matches = vec![md_find_match::default(); 64];
        let mut segments = vec![md_match_segment::default(); 128];
        let mut find_res = md_find_result {
            struct_size: size_of::<md_find_result>() as u32,
            ..Default::default()
        };
        // SAFETY: All query and output buffers are valid with matching capacities.
        let _ = unsafe {
            md_snapshot_find(
                snapshot,
                if query_bytes.is_empty() { null() } else { query_bytes.as_ptr() },
                query_bytes.len(),
                &find_opts,
                matches.as_mut_ptr(),
                matches.len(),
                segments.as_mut_ptr(),
                segments.len(),
                &mut find_res,
            )
        };

        // Fuzz read_stats with valid caller-owned section buffer.
        let read_opts = md_read_options {
            struct_size: size_of::<md_read_options>() as u32,
            flags: 0,
            words_per_minute: 200,
            section_size: size_of::<md_read_section>() as u32,
        };
        let mut stats = md_read_stats {
            struct_size: size_of::<md_read_stats>() as u32,
            ..Default::default()
        };
        let mut sections = vec![md_read_section::default(); 32];
        // SAFETY: Output buffers are valid.
        let _ = unsafe {
            md_snapshot_read_stats(
                snapshot,
                &read_opts,
                sections.as_mut_ptr(),
                sections.len(),
                &mut stats,
            )
        };

        // Fuzz anchor creation and resolution with fuzz-derived offsets and anchor bytes.
        let mut anchor_buf = [0u8; 512];
        let mut anchor_len: usize = 0;
        let offset = data.first().copied().unwrap_or(0) as u64;
        // SAFETY: `anchor_buf` is valid for 512 bytes.
        let _ = unsafe {
            md_snapshot_anchor_at(
                snapshot,
                offset,
                anchor_buf.as_mut_ptr(),
                anchor_buf.len(),
                &mut anchor_len,
            )
        };
        let node = data.get(1).copied().unwrap_or(0) as u32;
        // SAFETY: `anchor_buf` is valid for 512 bytes.
        let _ = unsafe {
            md_snapshot_anchor_for_node(
                snapshot,
                node,
                offset,
                anchor_buf.as_mut_ptr(),
                anchor_buf.len(),
                &mut anchor_len,
            )
        };

        let mut res = md_anchor_resolution {
            struct_size: size_of::<md_anchor_resolution>() as u32,
            ..Default::default()
        };
        // SAFETY: `anchor_bytes` is valid for `anchor_bytes.len()` bytes.
        let _ = unsafe {
            md_snapshot_resolve_anchor(
                snapshot,
                if anchor_bytes.is_empty() { null() } else { anchor_bytes.as_ptr() },
                anchor_bytes.len(),
                &mut res,
            )
        };
        if anchor_len > 0 && anchor_len <= anchor_buf.len() {
            // SAFETY: `anchor_buf[..anchor_len]` is valid.
            let _ = unsafe {
                md_snapshot_resolve_anchor(
                    snapshot,
                    anchor_buf.as_ptr(),
                    anchor_len,
                    &mut res,
                )
            };
        }

        let _ = md_snapshot_release(snapshot);
    }

    let _ = md_session_release(session);

    let (mut s, mut j, mut snap) = (0u64, 0u64, 0u64);
    // SAFETY: Output pointers are valid stack variables.
    assert_eq!(unsafe { md_live_handles(&mut s, &mut j, &mut snap) }, MD_OK);
    assert_eq!((s, j, snap), (0, 0, 0), "leaked FFI handles");
}

#[unsafe(no_mangle)]
pub extern "C" fn md_fuzz_ffi_workspace(ptr: *const u8, len: usize) {
    // SAFETY: Caller (libFuzzer) supplies a valid byte slice for the call.
    let data = unsafe { slice_from_raw(ptr, len) };

    // 1. Classify path
    let mut path_info = md_workspace_path_info {
        struct_size: size_of::<md_workspace_path_info>() as u32,
        ..Default::default()
    };
    // SAFETY: `data` and `path_info` are valid.
    let _ = unsafe {
        md_workspace_classify_path(
            if data.is_empty() { null() } else { data.as_ptr() },
            data.len(),
            &mut path_info,
        )
    };

    // Split into query and entries buffer.
    let (query, entries_buf) = if data.len() >= 2 {
        let q_len = (data[0] as usize) % data.len();
        (&data[1..1 + q_len], &data[1 + q_len..])
    } else {
        (data, &[][..])
    };

    // 2. Find names over fuzz-constructed entries backed by `entries_buf`.
    let mut entries = Vec::new();
    let mut offset = 0usize;
    let mut id = 1u64;
    for chunk in entries_buf.split(|&b| b == 0).take(16) {
        entries.push(md_workspace_entry {
            id,
            path_offset: offset as u64,
            path_length: chunk.len() as u32,
            reserved: 0,
        });
        offset += chunk.len() + 1;
        id += 1;
    }

    let name_opts = md_name_options {
        struct_size: size_of::<md_name_options>() as u32,
        flags: 0,
        max_results: 32,
        max_entries: 32,
        entry_size: size_of::<md_workspace_entry>() as u32,
        hit_size: size_of::<md_name_hit>() as u32,
    };
    let mut name_hits = vec![md_name_hit::default(); 32];
    let mut name_res = md_name_result {
        struct_size: size_of::<md_name_result>() as u32,
        ..Default::default()
    };
    // SAFETY: All backing buffers (`entries_buf`, `entries`, `query`, `name_hits`, `name_res`) are valid.
    let _ = unsafe {
        md_workspace_find_names(
            if entries_buf.is_empty() { null() } else { entries_buf.as_ptr() },
            entries_buf.len(),
            if entries.is_empty() { null() } else { entries.as_ptr() },
            entries.len(),
            if query.is_empty() { null() } else { query.as_ptr() },
            query.len(),
            &name_opts,
            name_hits.as_mut_ptr(),
            name_hits.len(),
            &mut name_res,
        )
    };

    // 3. Content admit and content search with valid backing buffers.
    let content_opts = md_content_options {
        struct_size: size_of::<md_content_options>() as u32,
        flags: 0,
        max_files: 8,
        max_total_bytes: 64 * 1024,
        max_file_bytes: 16 * 1024,
        max_matches_per_file: 16,
        max_total_matches: 32,
        snippet_context: 24,
        hit_size: size_of::<md_content_hit>() as u32,
        reserved: 0,
    };
    let mut progress = md_content_progress {
        struct_size: size_of::<md_content_progress>() as u32,
        ..Default::default()
    };
    let path = b"notes/fuzz.md";
    let mut admit_outcome: u32 = 0;
    let mut admit_reason: u32 = 0;
    // SAFETY: `query`, `content_opts`, `path`, `progress`, and out pointers are valid.
    let _ = unsafe {
        md_workspace_content_admit(
            if query.is_empty() { null() } else { query.as_ptr() },
            query.len(),
            &content_opts,
            path.as_ptr(),
            path.len(),
            entries_buf.len() as u64,
            &mut progress,
            &mut admit_outcome,
            &mut admit_reason,
        )
    };

    let mut content_hits = vec![md_content_hit::default(); 32];
    let mut text_buf = vec![0u8; 4096];
    let mut anchor_buf = vec![0u8; 1024];
    let mut content_res = md_content_result {
        struct_size: size_of::<md_content_result>() as u32,
        ..Default::default()
    };
    // SAFETY: All backing buffers are valid with exact capacities.
    let _ = unsafe {
        md_workspace_content_search(
            if query.is_empty() { null() } else { query.as_ptr() },
            query.len(),
            &content_opts,
            path.as_ptr(),
            path.len(),
            if entries_buf.is_empty() { null() } else { entries_buf.as_ptr() },
            entries_buf.len(),
            &mut progress,
            content_hits.as_mut_ptr(),
            content_hits.len(),
            text_buf.as_mut_ptr(),
            text_buf.len(),
            anchor_buf.as_mut_ptr(),
            anchor_buf.len(),
            &mut content_res,
        )
    };
}
