//! Real packaged C-ABI model-v2 values, never a second Markdown parser.
use markdown_linux::render::{RenderError, RenderPlan, Target};
use markdown_linux_core::{CoreDocument, CoreSession};
use std::{cell::Cell, path::Path};
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
