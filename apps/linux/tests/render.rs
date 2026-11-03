//! Real packaged C-ABI model-v2 values, never a second Markdown parser.
use markdown_linux::document::{
    DocumentResources, ImageBudget, ImageFormatInspector, ResourceError,
};
use markdown_linux::render::{RenderError, RenderPlan, Target};
use markdown_linux_core::{CoreDocument, CoreSession};
use std::{cell::Cell, collections::BTreeMap, path::Path};
static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn document(source: &str) -> CoreDocument {
    let session = CoreSession::new("renderer-test").unwrap();
    let _guard = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    let result = session
        .parse_blocking(source.as_bytes().to_vec(), None, true)
        .unwrap();
    session.close().unwrap();
    result.document.unwrap()
}
fn plan(source: &str) -> RenderPlan {
    RenderPlan::build(&document(source), || false).unwrap()
}
#[test]
fn inline_lists_code_html_and_links() {
    let p = plan(
        "# Héllo\n\n[go](#héllo) **bold *both*** ~~gone~~ `a<&`  \nnext\n\n3. first\n   - nested\n\n   second paragraph\n4. last\n\n```rust\n<&>\n```\n\n<script>alert(1)</script>\n\n![image alt](image.png)\n\n[external](https://example.com/a) [mail](mailto:a@example.com) [relative](other.md) [unsafe](javascript:alert) [missing](#absent)\n",
    );
    let text = p
        .fragments
        .iter()
        .map(|f| f.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("3. first"), "{text}");
    assert!(text.contains("4. last"));
    assert!(text.contains("• nested"));
    assert!(text.contains("second paragraph"));
    assert!(text.contains("<script>alert(1)</script>"));
    assert!(text.contains("image alt"));
    assert!(p.fragments.iter().any(|f| f.code && f.text() == "<&>\n"));
    let markup = p
        .fragments
        .iter()
        .enumerate()
        .map(|(i, f)| f.markup(17, i as u32))
        .collect::<String>();
    assert!(markup.contains("<b><i>both</i></b>"));
    assert!(markup.contains("<s>gone</s>"));
    assert!(markup.contains("<tt>a&lt;&amp;</tt>"));
    assert!(!markup.contains("<script>"));
    assert_eq!(p.targets[0], Target::Fragment(0));
    assert!(
        p.targets
            .iter()
            .any(|t| matches!(t,Target::External(s) if s.starts_with("https:")))
    );
    assert!(
        p.targets
            .iter()
            .any(|t| matches!(t,Target::External(s) if s.starts_with("mailto:")))
    );
    assert_eq!(p.targets.iter().filter(|t| **t == Target::Inert).count(), 3);
    let (index, fragment) = p
        .fragments
        .iter()
        .enumerate()
        .find(|(_, f)| f.has_link(0))
        .unwrap();
    assert!(fragment.markup(17, index as u32).contains("md:17:"));
    assert_eq!(
        p.token_target(&format!("md:17:{index}:0"), 17, index as u32),
        Some(&Target::Fragment(0))
    );
    for token in [
        format!("md:16:{index}:0"),
        format!("md:17:{index}:1"),
        format!("md:17:{index}:0:extra"),
        "https://example.com".into(),
    ] {
        assert!(p.token_target(&token, 17, index as u32).is_none());
    }
}
#[test]
fn unicode_fragments_styles_budget_and_cancellation() {
    for text in [
        "文🦀".repeat(2000),
        format!("{}end", "word 文 🦀 ".repeat(2000)),
    ] {
        let p = plan(&format!("[**{text}**](https://example.com)"));
        assert_eq!(
            p.fragments.iter().map(|f| f.text()).collect::<String>(),
            text
        );
        for (index, f) in p.fragments.iter().enumerate() {
            assert!(!f.text().is_empty());
            assert!(f.text().len() <= 512);
            assert!(f.runs.iter().all(|r| r.style & 1 != 0 && r.link == Some(0)));
            assert!(
                p.token_target(&format!("md:2:{index}:0"), 2, index as u32)
                    .is_some()
            );
        }
    }
    let doc = document(&"word ".repeat(2000));
    assert_eq!(
        RenderPlan::with_budget(&doc, || false, 128).unwrap_err(),
        RenderError::Budget
    );
    let calls = Cell::new(0);
    assert_eq!(
        RenderPlan::build(&doc, || {
            calls.set(calls.get() + 1);
            calls.get() > 30
        })
        .unwrap_err(),
        RenderError::Cancelled
    );
    let mut broken = document("[link](https://example.com)");
    let node = broken
        .nodes
        .iter_mut()
        .find(|n| n.kind == markdown_linux_core::ffi::MD_NODE_LINK)
        .unwrap();
    node.aux = u32::MAX;
    assert_eq!(
        RenderPlan::build(&broken, || false).unwrap_err(),
        RenderError::InvalidModel
    );
}
#[test]
fn corpus_outlines_match_published_core_and_expected_contracts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures");
    for (dir, name) in [
        ("foundation", "gfm-tour"),
        ("foundation", "code-fences"),
        ("foundation", "unsafe-html"),
        ("advanced", "inline-diagrams"),
        ("advanced", "voice-parity"),
        ("advanced", "math-alerts"),
        ("paths", "unicode-filename"),
        ("paths", "nfd-filename"),
    ] {
        let source =
            std::fs::read_to_string(root.join(format!("markdown/{dir}/{name}.md"))).unwrap();
        let d = document(&source);
        let p = RenderPlan::build(&d, || false).unwrap();
        let expected = std::fs::read_to_string(root.join(format!("expected/{name}.yaml"))).unwrap();
        assert_eq!(p.outline.len(), d.outline.len(), "{name}");
        for (entry, core) in p.outline.iter().zip(&d.outline) {
            assert_eq!(entry.title, d.text(core.title).unwrap());
            assert_eq!(entry.anchor, d.text(core.anchor).unwrap());
            assert_eq!(entry.level, core.level);
            assert!(expected.contains(&entry.anchor), "{name}: {}", entry.anchor);
            assert_eq!(p.fragments[entry.fragment as usize].heading, entry.level);
        }
        assert!(p.fragments.iter().all(|f| f.text().len() <= 512));
    }
}

#[test]
fn list_only_and_empty_items_keep_markers() {
    let p = plan("1. - nested\n2.\n");
    let text = p
        .fragments
        .iter()
        .map(|f| f.text())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(text.contains("1. "));
    assert!(text.contains("2. "));
    assert!(text.contains("• nested"));
}

#[test]
fn nul_and_xml_controls_become_visible_replacement_in_native_text() {
    let p = plan("```\ncode \0<&>\u{1}\n```\n\n<div>raw \0\u{1}</div>\n");
    let text = p.fragments.iter().map(|f| f.text()).collect::<String>();
    assert!(!text.contains('\0'));
    assert!(!text.contains('\u{1}'));
    assert!(text.contains('\u{fffd}'));
    for (i, f) in p.fragments.iter().enumerate() {
        assert!(!f.markup(1, i as u32).contains('\0'));
    }
}

#[test]
fn soft_breaks_flow_as_spaces_and_hard_breaks_preserve_newlines() {
    let p = plan("ordinary\nwrapped\n\nexplicit  \nbreak\n\nbackslash\\\nbreak\n");
    let text = p.fragments.iter().map(|f| f.text()).collect::<Vec<_>>();
    assert_eq!(
        text,
        ["ordinary wrapped", "explicit\nbreak", "backslash\nbreak"]
    );
}

#[test]
fn long_heading_keeps_one_semantic_start_and_bounded_styled_continuations() {
    let title = format!("{}end", "word 文 ".repeat(220));
    let p = plan(&format!("## {title}\n"));
    assert!(p.fragments.len() > 2);
    assert_eq!(
        p.fragments.iter().map(|f| f.text()).collect::<String>(),
        title
    );
    assert!(
        p.fragments
            .iter()
            .all(|f| f.heading == 2 && f.text().len() <= 512)
    );
    assert_eq!(p.fragments.iter().filter(|f| f.heading_start).count(), 1);
    assert!(p.fragments[0].heading_start);
    assert_eq!(p.outline.len(), 1);
    assert_eq!(p.outline[0].fragment, 0);
}

#[test]
fn mixed_case_source_schemes_use_the_core_canonical_external_target() {
    let doc = document(
        "[secure](HTTPS://example.com/a) [web](HtTp://example.com/b) [mail](MAILTO:Person@example.com) [mixed](MailTo:Other@example.com)",
    );
    let p = RenderPlan::build(&doc, || false).unwrap();
    assert_eq!(p.targets.len(), 4);
    for (target, intent) in p.targets.iter().zip(&doc.links) {
        let canonical = doc.text(intent.target).unwrap();
        assert_eq!(target, &Target::External(canonical.to_owned()));
        assert!(
            ["https:", "http:", "mailto:"]
                .iter()
                .any(|scheme| canonical.starts_with(scheme)),
            "core scheme must be canonical: {canonical}"
        );
    }
}

fn create_png(width: u32, height: u32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
    data.extend_from_slice(&13u32.to_be_bytes());
    data.extend_from_slice(b"IHDR");
    data.extend_from_slice(&width.to_be_bytes());
    data.extend_from_slice(&height.to_be_bytes());
    data.extend_from_slice(&[8, 2, 0, 0, 0]);
    data.extend_from_slice(&[0, 0, 0, 0]);
    data
}

fn create_bmp(width: i32, height: i32) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&[0x42, 0x4D]);
    data.extend_from_slice(&[0; 16]);
    data.extend_from_slice(&width.to_le_bytes());
    data.extend_from_slice(&height.to_le_bytes());
    data.extend_from_slice(&[1, 0, 24, 0]);
    data
}

#[test]
fn tables_align_header_and_tsv_copy() {
    let source = "| Name   | Type    | Score |\n\
                  | :----- | :-----: | ----: |\n\
                  | Alpha  | string  |    10 |\n\
                  | Beta   | number  |   200 |\n\
                  | Gamma  | boolean |     3 |\n";

    let p = plan(source);
    let table_fragments: Vec<_> = p.fragments.iter().filter(|f| f.is_table_row).collect();
    assert_eq!(table_fragments.len(), 4, "expected 4 table row fragments");

    assert!(table_fragments[0].is_table_header, "row 0 must be header");
    assert!(
        !table_fragments[1].is_table_header,
        "row 1 must not be header"
    );
    assert!(
        !table_fragments[2].is_table_header,
        "row 2 must not be header"
    );
    assert!(
        !table_fragments[3].is_table_header,
        "row 3 must not be header"
    );

    assert!(
        table_fragments[0]
            .runs
            .iter()
            .any(|r| (r.style & 1) != 0 && r.text.contains("Score")),
        "header row cells must have Strong style"
    );
    assert!(
        table_fragments[1].runs.iter().all(|r| (r.style & 1) == 0),
        "body row cells must not have Strong style"
    );

    let expected_rows = [
        "Name\tType\tScore",
        "Alpha\tstring\t10",
        "Beta\tnumber\t200",
        "Gamma\tboolean\t3",
    ];
    for (i, expected) in expected_rows.iter().enumerate() {
        assert_eq!(
            table_fragments[i].text(),
            *expected,
            "row {i} mismatch: expected '{expected}', got '{}'",
            table_fragments[i].text()
        );
    }

    let short_table = plan("| # | Item |\n| -: | --- |\n| 1 | one |\n| 22 |\n");
    let short_rows: Vec<_> = short_table
        .fragments
        .iter()
        .filter(|f| f.is_table_row)
        .collect();
    assert_eq!(short_rows.len(), 3, "expected 3 rows for short table");
    assert_eq!(
        short_rows[2].text(),
        "22\t",
        "expected padded short row '22\\t'"
    );

    let quoted = plan("> | q |\n> | - |\n> | x |\n");
    let quoted_rows: Vec<_> = quoted.fragments.iter().filter(|f| f.is_table_row).collect();
    assert_eq!(quoted_rows.len(), 2, "expected 2 rows in quoted table");
}

#[test]
fn task_markers_and_accessible_text() {
    let p = plan("- [x] done\n- [ ] open\n- plain\n");
    assert_eq!(p.fragments.len(), 3, "expected 3 list fragments");

    assert_eq!(p.fragments[0].text(), "☑ done");
    assert_eq!(p.fragments[1].text(), "☐ open");
    assert_eq!(p.fragments[2].text(), "• plain");

    let f0_marker = &p.fragments[0].runs[0];
    assert!(
        (f0_marker.style & 32) != 0,
        "checked item must have TaskChecked style"
    );

    let f1_marker = &p.fragments[1].runs[0];
    assert!(
        (f1_marker.style & 64) != 0,
        "unchecked item must have TaskUnchecked style"
    );

    let f2_marker = &p.fragments[2].runs[0];
    assert!(
        (f2_marker.style & (32 | 64)) == 0,
        "plain item must not have task style"
    );
}

#[test]
fn images_follow_resource_policy_and_limits() {
    let small_png = create_png(64, 64);
    let wide_png = create_png(2000, 100);
    let huge_bomb = create_png(20000, 20000);

    let images = std::cell::RefCell::new(BTreeMap::new());
    {
        let mut img = images.borrow_mut();
        img.insert("small.png".to_string(), small_png.clone());
        img.insert("wide.png".to_string(), wide_png);
        img.insert("huge.png".to_string(), huge_bomb);
        img.insert(
            "vector.svg".to_string(),
            b"<svg xmlns='http://www.w3.org/2000/svg'/>".to_vec(),
        );
        img.insert(
            "disguised.png".to_string(),
            b"<?xml version='1.0'?><svg></svg>".to_vec(),
        );
        img.insert("corrupt.png".to_string(), b"not an image at all".to_vec());
    }

    let load_count = Cell::new(0);
    let mock_loader =
        |path: &str, _limit: usize| -> Result<(Vec<u8>, usize, usize), ResourceError> {
            load_count.set(load_count.get() + 1);
            if let Some(data) = images.borrow().get(path) {
                let (w, h) = ImageFormatInspector::inspect(data, 32 * 1024 * 1024, 2048)?;
                return Ok((data.clone(), w, h));
            }
            if path == "../up.png" {
                return Err(ResourceError::OutsideFolder);
            }
            Err(ResourceError::NotFound)
        };

    let md = "![small](small.png)\n\n\
              ![huge](huge.png)\n\n\
              ![svg](vector.svg)\n\n\
              ![disguised](disguised.png)\n\n\
              ![corrupt](corrupt.png)\n\n\
              ![up](../up.png)\n\n\
              ![missing](missing.png)\n\n\
              ![remote](https://example.com/pic.png)\n\n\
              ![data](data:image/png;base64,abc)\n";

    let doc = document(md);
    let plan = RenderPlan::build_with_resources(
        &doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        ImageBudget::default(),
    )
    .unwrap();

    assert_eq!(plan.images.len(), 9, "expected 9 images");
    assert_eq!(plan.images[0].failure, None, "small image should succeed");
    assert_eq!(plan.images[0].width, 64);
    assert_eq!(plan.images[0].height, 64);
    assert!(plan.images[0].data.is_some());

    assert_eq!(
        plan.images[1].failure,
        Some(ResourceError::TooLarge),
        "huge image should fail with TooLarge"
    );
    assert_eq!(
        plan.images[2].failure,
        Some(ResourceError::UnsupportedType),
        "svg should fail with UnsupportedType"
    );
    assert_eq!(
        plan.images[3].failure,
        Some(ResourceError::UnsupportedType),
        "disguised svg should fail with UnsupportedType"
    );
    assert_eq!(
        plan.images[4].failure,
        Some(ResourceError::Undecodable),
        "corrupt should fail with Undecodable"
    );
    assert_eq!(
        plan.images[5].failure,
        Some(ResourceError::OutsideFolder),
        "up should fail with OutsideFolder"
    );
    assert_eq!(
        plan.images[6].failure,
        Some(ResourceError::NotFound),
        "missing should fail with NotFound"
    );
    assert_eq!(
        plan.images[7].failure,
        Some(ResourceError::Remote),
        "https image should fail with Remote"
    );
    assert_eq!(
        plan.images[8].failure,
        Some(ResourceError::Blocked),
        "data: image should fail with Blocked"
    );

    // Budget limit: max 3 images
    let budget = ImageBudget {
        max_images: 3,
        ..Default::default()
    };
    let multi_md = "![1](small.png) ![2](wide.png) ![3](i3.png) ![4](i4.png) ![5](i5.png)\n";
    {
        let mut img = images.borrow_mut();
        img.insert("i3.png".to_string(), small_png.clone());
        img.insert("i4.png".to_string(), small_png.clone());
        img.insert("i5.png".to_string(), small_png.clone());
    }

    let budget_doc = document(multi_md);
    let budget_plan = RenderPlan::build_with_resources(
        &budget_doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        budget,
    )
    .unwrap();
    assert_eq!(budget_plan.images.len(), 5);
    assert_eq!(budget_plan.images[0].failure, None);
    assert_eq!(budget_plan.images[1].failure, None);
    assert_eq!(budget_plan.images[2].failure, None);
    assert_eq!(budget_plan.images[3].failure, Some(ResourceError::Limit));
    assert_eq!(budget_plan.images[4].failure, Some(ResourceError::Limit));

    // Decoded memory budget limit
    let mem_budget = ImageBudget {
        max_decoded_bytes: 64 * 64 * 4 * 2,
        ..Default::default()
    };
    let mem_md = "![w](wide.png) ![s1](s1.png) ![s2](s2.png) ![s3](s3.png)\n";
    {
        let mut img = images.borrow_mut();
        img.insert("s1.png".to_string(), small_png.clone());
        img.insert("s2.png".to_string(), small_png.clone());
        img.insert("s3.png".to_string(), small_png.clone());
    }

    let mem_doc = document(mem_md);
    let mem_plan = RenderPlan::build_with_resources(
        &mem_doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        mem_budget,
    )
    .unwrap();
    assert_eq!(mem_plan.images[0].failure, Some(ResourceError::Limit));
    assert_eq!(mem_plan.images[1].failure, None);
    assert_eq!(mem_plan.images[2].failure, None);
    assert_eq!(mem_plan.images[3].failure, Some(ResourceError::Limit));

    // Deduplication check
    load_count.set(0);
    let dedup_md = "![first](small.png)\n\n![second](small.png)\n";
    let dedup_doc = document(dedup_md);
    let dedup_plan = RenderPlan::build_with_resources(
        &dedup_doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        ImageBudget::default(),
    )
    .unwrap();
    assert_eq!(dedup_plan.images.len(), 2);
    assert_eq!(
        load_count.get(),
        1,
        "expected exactly 1 loader call for duplicate image"
    );
    assert!(dedup_plan.images[0].failure.is_none());
    assert!(dedup_plan.images[1].failure.is_none());

    // Mixed paragraph with text, link, and multiple images
    let mixed_md =
        "See ![first](small.png) for details and [docs](#test) plus ![second](small.png)\n";
    let mixed_doc = document(mixed_md);
    let mixed_plan = RenderPlan::build_with_resources(
        &mixed_doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        ImageBudget::default(),
    )
    .unwrap();
    let mixed_frag = mixed_plan
        .fragments
        .iter()
        .find(|f| f.text().contains("See"))
        .unwrap();
    assert_eq!(mixed_frag.images.len(), 2);
    assert!(
        mixed_frag
            .runs
            .iter()
            .any(|r| r.text == "docs" && r.link.is_some())
    );
    assert!(mixed_frag.text().contains("See ") && mixed_frag.text().contains(" for details and "));

    // Image in table cell must not leak into subsequent paragraph
    let table_with_img_md = "| Header |\n| --- |\n| ![cell](small.png) |\n\nNext paragraph text\n";
    let table_doc = document(table_with_img_md);
    let table_plan = RenderPlan::build_with_resources(
        &table_doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        ImageBudget::default(),
    )
    .unwrap();
    let next_frag = table_plan
        .fragments
        .iter()
        .find(|f| f.text().contains("Next paragraph"))
        .unwrap();
    assert_eq!(
        next_frag.images.len(),
        0,
        "table cell image must not leak into subsequent paragraph"
    );

    // Max pixel side (2048) enforcement: 2049x10 fails with TooLarge
    let long_side_png = create_png(2049, 10);
    images
        .borrow_mut()
        .insert("longside.png".to_string(), long_side_png);
    let side_doc = document("![side](longside.png)\n");
    let side_plan = RenderPlan::build_with_resources(
        &side_doc,
        || false,
        markdown_linux::render::PLAN_BUDGET,
        Some(&mock_loader),
        ImageBudget::default(),
    )
    .unwrap();
    assert_eq!(side_plan.images[0].failure, Some(ResourceError::TooLarge));

    // Crafted BMP with negative/overflow height handled safely
    let min_bmp = create_bmp(100, i32::MIN);
    let bmp_res = ImageFormatInspector::inspect(&min_bmp, 32 * 1024 * 1024, 2048);
    assert_eq!(bmp_res, Err(ResourceError::TooLarge));
}

#[test]
fn document_resources_filesystem_security() {
    let temp_dir = std::env::temp_dir().join(format!("markdown-res-test-{}", std::process::id()));
    std::fs::create_dir_all(&temp_dir).unwrap();
    let sub_dir = temp_dir.join("sub");
    std::fs::create_dir_all(&sub_dir).unwrap();
    let outside_dir =
        std::env::temp_dir().join(format!("markdown-outside-test-{}", std::process::id()));
    std::fs::create_dir_all(&outside_dir).unwrap();

    let valid_png = create_png(32, 32);
    let valid_file = temp_dir.join("valid.png");
    std::fs::write(&valid_file, &valid_png).unwrap();

    let sub_file = sub_dir.join("sub.png");
    std::fs::write(&sub_file, &valid_png).unwrap();

    let outside_file = outside_dir.join("outside.png");
    std::fs::write(&outside_file, &valid_png).unwrap();

    let outside_symlink = temp_dir.join("sym_outside.png");
    let _ = std::os::unix::fs::symlink(&outside_file, &outside_symlink);

    let svg_file = temp_dir.join("image.svg");
    std::fs::write(&svg_file, b"<svg></svg>").unwrap();

    let non_image_file = temp_dir.join("doc.txt");
    std::fs::write(&non_image_file, b"plain text").unwrap();

    let resources = DocumentResources::new(Some(temp_dir.clone()));

    // 1. Direct sibling image
    let read_valid = resources.read_image("valid.png", 1024 * 1024);
    assert!(read_valid.is_ok(), "valid sibling image should succeed");
    let (data, w, h) = read_valid.unwrap();
    assert_eq!(data.len(), valid_png.len());
    assert_eq!(w, 32);
    assert_eq!(h, 32);

    // 2. Subfolder image
    let read_sub = resources.read_image("sub/sub.png", 1024 * 1024);
    assert!(read_sub.is_ok(), "subfolder image should succeed");

    // 3. Escaping with ..
    let read_escape = resources.read_image("../outside.png", 1024 * 1024);
    assert_eq!(read_escape, Err(ResourceError::OutsideFolder));

    // 4. Symlink escaping outside folder
    if outside_symlink.exists() {
        let read_sym = resources.read_image("sym_outside.png", 1024 * 1024);
        assert_eq!(read_sym, Err(ResourceError::OutsideFolder));
    }

    // 5. Absolute path or empty path
    assert_eq!(
        resources.read_image("/etc/passwd", 1024),
        Err(ResourceError::OutsideFolder)
    );
    assert_eq!(
        resources.read_image("", 1024),
        Err(ResourceError::OutsideFolder)
    );
    assert_eq!(
        resources.read_image("valid.png\0extra", 1024),
        Err(ResourceError::OutsideFolder)
    );

    // 6. Missing file
    assert_eq!(
        resources.read_image("nonexistent.png", 1024),
        Err(ResourceError::NotFound)
    );

    // 7. Directory path
    assert_eq!(
        resources.read_image("sub", 1024),
        Err(ResourceError::UnsupportedType)
    );

    // 8. SVG rejected
    assert_eq!(
        resources.read_image("image.svg", 1024),
        Err(ResourceError::UnsupportedType)
    );

    // 9. Non-image rejected
    assert_eq!(
        resources.read_image("doc.txt", 1024),
        Err(ResourceError::UnsupportedType)
    );

    // 10. File exceeding limit
    assert_eq!(
        resources.read_image("valid.png", 10),
        Err(ResourceError::TooLarge)
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    let _ = std::fs::remove_dir_all(&outside_dir);
}

#[test]
fn messages_name_no_path() {
    let all_errors = [
        ResourceError::OutsideFolder,
        ResourceError::NotFound,
        ResourceError::PermissionDenied,
        ResourceError::NotRegularFile,
        ResourceError::UnsupportedType,
        ResourceError::TooLarge,
        ResourceError::Limit,
        ResourceError::Undecodable,
        ResourceError::Remote,
        ResourceError::Blocked,
        ResourceError::NotLoaded,
    ];

    for err in all_errors {
        let msg = err.message();
        assert!(
            !msg.contains('/'),
            "error message must not contain '/': {msg}"
        );
        assert!(
            !msg.contains('\\'),
            "error message must not contain '\\': {msg}"
        );
        assert!(
            !msg.contains(".png"),
            "error message must not contain file extension: {msg}"
        );
        assert!(
            !msg.contains(".md"),
            "error message must not contain file extension: {msg}"
        );
        assert!(!msg.is_empty(), "error message must not be empty");
    }
}
