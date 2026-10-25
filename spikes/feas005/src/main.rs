use std::cell::Cell;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Instant;

use gtk::glib::ControlFlow;
use gtk::pango::{Style, Underline};
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, ScrolledWindow, TextBuffer, TextTag, TextView};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

const APPLICATION_ID: &str = "dev.markdown.NativeRenderingProbe";

#[derive(Clone)]
struct Tags {
    h1: TextTag,
    h2: TextTag,
    h3: TextTag,
    h4: TextTag,
    h5: TextTag,
    h6: TextTag,
    emphasis: TextTag,
    strong: TextTag,
    strike: TextTag,
    code: TextTag,
    quote: TextTag,
    link: TextTag,
    metadata: TextTag,
}

fn add_tag(buffer: &TextBuffer, tag: &TextTag) {
    assert!(buffer.tag_table().add(tag), "duplicate text tag");
}

fn make_tags(buffer: &TextBuffer) -> Tags {
    let tags = Tags {
        h1: TextTag::builder()
            .name("h1")
            .scale(2.0)
            .weight(700)
            .pixels_above_lines(16)
            .pixels_below_lines(8)
            .build(),
        h2: TextTag::builder()
            .name("h2")
            .scale(1.6)
            .weight(700)
            .pixels_above_lines(14)
            .pixels_below_lines(6)
            .build(),
        h3: TextTag::builder()
            .name("h3")
            .scale(1.35)
            .weight(700)
            .pixels_above_lines(12)
            .pixels_below_lines(4)
            .build(),
        h4: TextTag::builder()
            .name("h4")
            .scale(1.2)
            .weight(700)
            .pixels_above_lines(10)
            .build(),
        h5: TextTag::builder()
            .name("h5")
            .weight(700)
            .pixels_above_lines(8)
            .build(),
        h6: TextTag::builder()
            .name("h6")
            .weight(700)
            .style(Style::Italic)
            .pixels_above_lines(8)
            .build(),
        emphasis: TextTag::builder()
            .name("emphasis")
            .style(Style::Italic)
            .build(),
        strong: TextTag::builder().name("strong").weight(700).build(),
        strike: TextTag::builder()
            .name("strike")
            .strikethrough(true)
            .build(),
        code: TextTag::builder()
            .name("code")
            .family("monospace")
            .background("#eeeeee")
            .build(),
        quote: TextTag::builder()
            .name("quote")
            .style(Style::Italic)
            .left_margin(24)
            .build(),
        link: TextTag::builder()
            .name("link")
            .foreground("#1a5fb4")
            .underline(Underline::Single)
            .build(),
        metadata: TextTag::builder()
            .name("metadata")
            .foreground("#77767b")
            .family("monospace")
            .build(),
    };
    for tag in [
        &tags.h1,
        &tags.h2,
        &tags.h3,
        &tags.h4,
        &tags.h5,
        &tags.h6,
        &tags.emphasis,
        &tags.strong,
        &tags.strike,
        &tags.code,
        &tags.quote,
        &tags.link,
        &tags.metadata,
    ] {
        add_tag(buffer, tag);
    }
    tags
}

fn parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
}

fn heading_tag(tags: &Tags, level: HeadingLevel) -> TextTag {
    match level {
        HeadingLevel::H1 => tags.h1.clone(),
        HeadingLevel::H2 => tags.h2.clone(),
        HeadingLevel::H3 => tags.h3.clone(),
        HeadingLevel::H4 => tags.h4.clone(),
        HeadingLevel::H5 => tags.h5.clone(),
        HeadingLevel::H6 => tags.h6.clone(),
    }
}

fn insert(buffer: &TextBuffer, text: &str, active: &[TextTag]) {
    let mut end = buffer.end_iter();
    let refs: Vec<&TextTag> = active.iter().collect();
    buffer.insert_with_tags(&mut end, text, &refs);
}

fn render_markdown(buffer: &TextBuffer, source: &str) -> usize {
    let tags = make_tags(buffer);
    let mut active = Vec::<TextTag>::new();
    let mut events = 0;
    buffer.begin_user_action();
    for event in Parser::new_ext(source, parser_options()) {
        events += 1;
        match event {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => active.push(heading_tag(&tags, level)),
                Tag::Emphasis => active.push(tags.emphasis.clone()),
                Tag::Strong => active.push(tags.strong.clone()),
                Tag::Strikethrough => active.push(tags.strike.clone()),
                Tag::CodeBlock(_) => active.push(tags.code.clone()),
                Tag::BlockQuote(_) => active.push(tags.quote.clone()),
                Tag::Link { .. } => active.push(tags.link.clone()),
                Tag::MetadataBlock(_) => active.push(tags.metadata.clone()),
                Tag::Item => insert(buffer, "• ", &active),
                Tag::FootnoteDefinition(label) => {
                    insert(buffer, &format!("[{label}] "), &active);
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_)
                | TagEnd::Paragraph
                | TagEnd::CodeBlock
                | TagEnd::BlockQuote(_)
                | TagEnd::Item
                | TagEnd::Table
                | TagEnd::TableHead
                | TagEnd::TableRow
                | TagEnd::FootnoteDefinition
                | TagEnd::MetadataBlock(_) => {
                    if matches!(
                        tag,
                        TagEnd::Heading(_)
                            | TagEnd::CodeBlock
                            | TagEnd::BlockQuote(_)
                            | TagEnd::MetadataBlock(_)
                    ) {
                        active.pop();
                    }
                    insert(buffer, "\n", &active);
                }
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                    active.pop();
                }
                TagEnd::TableCell => insert(buffer, "\t", &active),
                _ => {}
            },
            Event::Text(text) => insert(buffer, &text, &active),
            Event::Code(text) => insert(buffer, &text, std::slice::from_ref(&tags.code)),
            Event::InlineMath(text) => insert(buffer, &format!("${text}$"), &active),
            Event::DisplayMath(text) => insert(buffer, &format!("\n{text}\n"), &active),
            Event::Html(text) | Event::InlineHtml(text) => insert(buffer, &text, &active),
            Event::FootnoteReference(label) => {
                insert(buffer, &format!("[{label}]"), &active);
            }
            Event::SoftBreak | Event::HardBreak => insert(buffer, "\n", &active),
            Event::Rule => insert(buffer, "────────────────────────\n", &active),
            Event::TaskListMarker(checked) => {
                insert(buffer, if checked { "☑ " } else { "☐ " }, &active);
            }
        }
    }
    buffer.end_user_action();
    events
}

fn main() -> ExitCode {
    let mut args = env::args_os().skip(1);
    let Some(path) = args.next().map(PathBuf::from) else {
        eprintln!("usage: linux-native-rendering-probe <markdown-file> [--automated]");
        return ExitCode::from(2);
    };
    let automated = args.any(|arg| arg == "--automated");
    let source = match fs::read_to_string(&path) {
        Ok(source) => Arc::new(source),
        Err(error) => {
            eprintln!("failed to read {}: {error}", path.display());
            return ExitCode::FAILURE;
        }
    };
    let process_start = Instant::now();
    let application = Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    let application_for_activate = application.clone();
    let path_for_activate = Arc::new(path);

    application.connect_activate(move |app| {
        let buffer = TextBuffer::new(None);
        let render_start = Instant::now();
        let event_count = render_markdown(&buffer, &source);
        let render_ns = render_start.elapsed().as_nanos();

        let text_view = TextView::builder()
            .buffer(&buffer)
            .editable(false)
            .cursor_visible(true)
            .wrap_mode(gtk::WrapMode::WordChar)
            .left_margin(32)
            .right_margin(32)
            .top_margin(24)
            .bottom_margin(24)
            .build();
        let scroller = ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .child(&text_view)
            .build();
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Markdown native rendering probe")
            .default_width(900)
            .default_height(700)
            .child(&scroller)
            .build();
        window.present();

        if automated {
            let ticks = Rc::new(Cell::new(0_u8));
            let first_frame_ns = Rc::new(Cell::new(0_u128));
            let scroll_request_ns = Rc::new(Cell::new(0_u128));
            let scroll_request_accepted = Rc::new(Cell::new(false));
            let app = application_for_activate.clone();
            let ticks_in_callback = ticks.clone();
            let first_frame_in_callback = first_frame_ns.clone();
            let scroll_request_in_callback = scroll_request_ns.clone();
            let scroll_accepted_in_callback = scroll_request_accepted.clone();
            let source_bytes = source.len();
            let path = path_for_activate.clone();
            window.add_tick_callback(move |window, _| {
                let tick = ticks_in_callback.get() + 1;
                ticks_in_callback.set(tick);
                if tick == 1 {
                    first_frame_in_callback.set(process_start.elapsed().as_nanos());
                    return ControlFlow::Continue;
                }
                if tick == 2 {
                    let scroll_start = Instant::now();
                    let mut end = buffer.end_iter();
                    scroll_accepted_in_callback
                        .set(text_view.scroll_to_iter(&mut end, 0.0, true, 0.0, 1.0));
                    scroll_request_in_callback.set(scroll_start.elapsed().as_nanos());
                    return ControlFlow::Continue;
                }
                let adjustment = scroller.vadjustment();
                let remaining_scroll =
                    (adjustment.upper() - adjustment.page_size() - adjustment.value()).max(0.0);
                if remaining_scroll > 32.0 && tick < 120 {
                    return ControlFlow::Continue;
                }

                let scroll_upper = adjustment.upper();
                let scroll_page_size = adjustment.page_size();
                let scroll_value = adjustment.value();
                text_view.grab_focus();
                let end_offset = buffer.char_count().min(32);
                buffer.select_range(
                    &buffer.iter_at_offset(end_offset),
                    &buffer.iter_at_offset(0),
                );
                let selection_chars = buffer
                    .selection_bounds()
                    .map(|(start, end)| buffer.text(&start, &end, true).chars().count())
                    .unwrap_or_default();
                println!("schema_version: 1");
                println!("file: {}", path.display());
                println!("source_bytes: {source_bytes}");
                println!("events: {event_count}");
                println!("buffer_characters: {}", buffer.char_count());
                println!("buffer_lines: {}", buffer.line_count());
                println!("parse_and_buffer_ns: {render_ns}");
                println!("first_frame_ns: {}", first_frame_in_callback.get());
                println!("scroll_settled_ns: {}", process_start.elapsed().as_nanos());
                println!("frame_ticks: {tick}");
                println!("scroll_request_ns: {}", scroll_request_in_callback.get());
                println!(
                    "scroll_request_accepted: {}",
                    scroll_accepted_in_callback.get()
                );
                println!("selection_characters: {selection_chars}");
                println!("scroll_upper: {scroll_upper:.3}");
                println!("scroll_page_size: {scroll_page_size:.3}");
                println!("scroll_value: {scroll_value:.3}");
                println!("scroll_remaining: {remaining_scroll:.3}");
                println!("scale_factor: {}", window.scale_factor());
                println!("text_accessible_role: {:?}", text_view.accessible_role());
                println!(
                    "backend: {}",
                    env::var("GDK_BACKEND").unwrap_or_else(|_| "auto".into())
                );
                app.quit();
                ControlFlow::Break
            });
        }
    });

    application.run_with_args::<&str>(&[]);
    ExitCode::SUCCESS
}
