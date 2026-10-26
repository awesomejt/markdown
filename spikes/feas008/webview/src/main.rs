//! FEAS-008 WebKitGTK document baseline.
//!
//! The same model and policy render to HTML in a WebKitGTK 6 view. Page
//! scripts are disabled; one injected user script reports when the first
//! frame after load has been produced. Local images load through a custom
//! `mdv-res:` scheme that applies the native host's containment checks,
//! because WebKit would otherwise follow symlinks out of the folder.

use feas008_docmodel::{BlockKind, Document, LinkIntent};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};
use webkit6::prelude::*;

const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const STYLE: &str = "body{font:15px/1.45 sans-serif;max-width:none;margin:12px 18px}\
code,pre{font-family:monospace}pre{background:#8881;padding:6px;white-space:pre-wrap}\
p,li{overflow-wrap:anywhere}img{max-height:360px}blockquote{border-left:3px solid #8886;margin-left:0;padding-left:12px}";
const PRESENTED_JS: &str = "requestAnimationFrame(() => requestAnimationFrame(() => window.webkit.messageHandlers.presented.postMessage(1)));";

#[derive(Clone, Copy)]
struct Presented {
    paint_us: i64,
    presentation_us: i64,
}

type OnPresented = Box<dyn FnOnce(Presented)>;

struct State {
    window: gtk::ApplicationWindow,
    view: webkit6::WebView,
    pending: RefCell<Option<OnPresented>>,
    waiting_frame: Cell<Option<i64>>,
    message_us: Cell<i64>,
}

fn ms(from: i64, to: i64) -> f64 {
    (to - from) as f64 / 1000.0
}

fn event(value: Value) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "EVENT {value}");
    let _ = out.flush();
}

fn hwm_kib(pid: &str) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    status
        .lines()
        .find(|l| l.starts_with("VmHWM:"))?
        .split_whitespace()
        .nth(1)?
        .parse()
        .ok()
}

/// Peak RSS of this process plus its WebKit child processes.
fn peak_rss_tree_kib() -> (u64, u64) {
    let me = std::process::id().to_string();
    let own = hwm_kib(&me).unwrap_or(0);
    let mut children = 0;
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let pid = entry.file_name().to_string_lossy().to_string();
            let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) else {
                continue;
            };
            if status
                .lines()
                .any(|l| l.split_whitespace().collect::<Vec<_>>() == ["PPid:", me.as_str()])
            {
                children += hwm_kib(&pid).unwrap_or(0);
            }
        }
    }
    (own, children)
}

fn resource(base: &Path, rel: &str) -> Result<(Vec<u8>, &'static str), String> {
    let base = base.canonicalize().map_err(|e| e.to_string())?;
    let path = base
        .join(rel.trim_start_matches('/'))
        .canonicalize()
        .map_err(|e| format!("not found: {e}"))?;
    if !path.starts_with(&base) {
        return Err("resolves outside the document folder".into());
    }
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if !meta.is_file() || meta.len() > MAX_IMAGE_BYTES {
        return Err("not a regular file within the size limit".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let mime = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "image/png"
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        "image/jpeg"
    } else if bytes.starts_with(b"GIF8") {
        "image/gif"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "image/webp"
    } else {
        return Err("content is not an allowed raster format".into());
    };
    Ok((bytes, mime))
}

impl State {
    /// Resolve on the first toplevel frame painted after the page's
    /// double-rAF message, using compositor presentation time when known.
    fn presented(&self, done: impl FnOnce(Presented) + 'static) {
        *self.pending.borrow_mut() = Some(Box::new(done));
    }

    fn on_message(&self) {
        self.message_us.set(glib::monotonic_time());
        self.waiting_frame.set(Some(-1));
        self.window.queue_draw();
    }

    fn after_paint(&self, clock: &gdk::FrameClock) {
        let counter = clock.frame_counter();
        match self.waiting_frame.get() {
            Some(-1) => {
                self.waiting_frame.set(Some(counter));
                self.message_us.set(glib::monotonic_time());
                self.window.queue_draw();
            }
            Some(frame) => {
                let timings = clock.timings(frame);
                if timings.as_ref().is_some_and(|t| t.is_complete()) || counter - frame > 30 {
                    self.waiting_frame.set(None);
                    let presented = Presented {
                        paint_us: self.message_us.get(),
                        presentation_us: timings.map_or(0, |t| t.presentation_time()),
                    };
                    if let Some(done) = self.pending.borrow_mut().take() {
                        done(presented);
                    }
                } else {
                    self.window.queue_draw();
                }
            }
            None => {}
        }
    }
}

async fn wait_presented(state: &Rc<State>) -> Presented {
    let slot: Rc<RefCell<Option<Presented>>> = Rc::new(RefCell::new(None));
    let s = slot.clone();
    state.presented(move |p| *s.borrow_mut() = Some(p));
    loop {
        if let Some(p) = slot.borrow_mut().take() {
            return p;
        }
        glib::timeout_future(Duration::from_millis(2)).await;
    }
}

fn best(p: Presented) -> i64 {
    if p.presentation_us > 0 {
        p.presentation_us
    } else {
        p.paint_us
    }
}

fn main() -> glib::ExitCode {
    let main_us = glib::monotonic_time();
    let launch_us = std::env::var("FEAS008_LAUNCH_NS")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
        .map(|ns| ns / 1000);
    let args = std::env::args().skip(1);
    let mut path = PathBuf::new();
    let (mut automated, mut control) = (false, false);
    for arg in args {
        match arg.as_str() {
            "--automated" => automated = true,
            "--control" => control = true,
            other => path = PathBuf::from(other),
        }
    }
    let started = Instant::now();
    let source = std::fs::read_to_string(&path).expect("read document");
    let read_ms = started.elapsed().as_secs_f64() * 1000.0;
    let rendered = Instant::now();
    let body = feas008_docmodel::html::render(&source);
    let html = format!(
        "<!doctype html><html><head><meta charset=utf-8>\
         <meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src mdv-res:; style-src 'unsafe-inline'\">\
         <style>{STYLE}</style></head><body>{body}</body></html>"
    );
    let render_ms = rendered.elapsed().as_secs_f64() * 1000.0;
    let base_dir = path
        .canonicalize()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."));
    let html = Rc::new(html);
    let source = Rc::new(source);

    let application = gtk::Application::builder()
        .application_id("dev.markdown.Feas008WebView")
        .flags(gio::ApplicationFlags::NON_UNIQUE)
        .build();
    application.connect_activate(move |application| {
        let context = webkit6::WebContext::new();
        let base = base_dir.clone();
        context.register_uri_scheme("mdv-res", move |request| {
            let rel = request.path().map(|p| p.to_string()).unwrap_or_default();
            match resource(&base, &rel) {
                Ok((bytes, mime)) => {
                    let len = bytes.len() as i64;
                    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_owned(bytes));
                    request.finish(&stream, len, Some(mime));
                }
                Err(reason) => {
                    eprintln!("image refused: {rel}: {reason}");
                    let mut error = glib::Error::new(gio::IOErrorEnum::PermissionDenied, &reason);
                    request.finish_error(&mut error);
                }
            }
        });
        let manager = webkit6::UserContentManager::new();
        manager.register_script_message_handler("presented", None);
        manager.add_script(&webkit6::UserScript::new(
            PRESENTED_JS,
            webkit6::UserContentInjectedFrames::TopFrame,
            webkit6::UserScriptInjectionTime::End,
            &[],
            &[],
        ));
        let view = webkit6::WebView::builder().web_context(&context).user_content_manager(&manager).build();
        let settings = webkit6::prelude::WebViewExt::settings(&view).unwrap();
        settings.set_enable_javascript_markup(false);
        settings.set_allow_file_access_from_file_urls(false);
        settings.set_enable_page_cache(false);
        settings.set_enable_html5_database(false);
        view.connect_decide_policy(|view, decision, kind| {
            if kind != webkit6::PolicyDecisionType::NavigationAction {
                return false;
            }
            let Some(nav) = decision.downcast_ref::<webkit6::NavigationPolicyDecision>() else { return false };
            let uri = nav.navigation_action().and_then(|a| a.request()).and_then(|r| r.uri()).map(|u| u.to_string()).unwrap_or_default();
            if uri.starts_with("mdv-res://doc/") && view.uri().is_none_or(|u| u.split('#').next() == uri.split('#').next()) {
                return false;
            }
            let intent = feas008_docmodel::classify_link(&uri, &Default::default());
            eprintln!("navigation {uri}: {intent:?}");
            if let LinkIntent::External(url) = intent {
                gtk::UriLauncher::new(&url).launch(None::<&gtk::Window>, gio::Cancellable::NONE, |_| {});
            }
            decision.ignore();
            true
        });

        let title = path.file_name().map_or("document".into(), |n| n.to_string_lossy().to_string());
        let window = gtk::ApplicationWindow::builder()
            .application(application)
            .title(format!("FEAS-008 WebView — {title}"))
            .default_width(1200)
            .default_height(900)
            .child(&view)
            .build();
        let state = Rc::new(State {
            window: window.clone(),
            view: view.clone(),
            pending: RefCell::new(None),
            waiting_frame: Cell::new(None),
            message_us: Cell::new(0),
        });
        let s = state.clone();
        manager.connect_script_message_received(Some("presented"), move |_, _| s.on_message());
        let s = state.clone();
        window.connect_realize(move |window| {
            let clock = window.frame_clock().expect("frame clock");
            let s = s.clone();
            clock.connect_after_paint(move |clock| s.after_paint(clock));
        });
        window.present();
        view.load_html(&html, Some("mdv-res://doc/"));

        let source = source.clone();
        let s = state.clone();
        glib::spawn_future_local(async move {
            let presented = wait_presented(&s).await;
            let origin = launch_us.unwrap_or(main_us);
            let mut report = json!({
                "engine": "webkitgtk",
                "webkit": format!("{}.{}.{}", webkit6::functions::major_version(), webkit6::functions::minor_version(), webkit6::functions::micro_version()),
                "document": source.len(),
                "launch_to_main_ms": ms(origin, main_us),
                "read_ms": read_ms,
                "parse_ms": render_ms,
                "launch_to_paint_ms": ms(origin, presented.paint_us),
                "launch_to_presented_ms": ms(origin, best(presented)),
                "presentation_time_available": presented.presentation_us > 0,
            });
            event(json!({"event": "presented", "launch_to_presented_ms": report["launch_to_presented_ms"]}));
            if automated {
                automate(&s, &source, &mut report).await;
            }
            if control {
                let (tx, rx) = std::sync::mpsc::channel::<String>();
                std::thread::spawn(move || {
                    for line in std::io::stdin().lock().lines().map_while(Result::ok) {
                        if tx.send(line).is_err() {
                            break;
                        }
                    }
                });
                event(json!({"event": "ready"}));
                loop {
                    match rx.try_recv() {
                        Ok(line) if line.trim() == "quit" => break,
                        Ok(_) => {}
                        Err(std::sync::mpsc::TryRecvError::Empty) => glib::timeout_future(Duration::from_millis(20)).await,
                        Err(_) => break,
                    }
                }
            }
            let (own, children) = peak_rss_tree_kib();
            report["peak_rss_kib"] = json!(own + children);
            report["peak_rss_ui_kib"] = json!(own);
            report["peak_rss_children_kib"] = json!(children);
            println!("REPORT {report}");
            if automated || control {
                s.window.application().unwrap().quit();
            }
        });
    });
    application.run_with_args::<&str>(&[])
}

async fn eval(state: &Rc<State>, script: &str) -> Option<String> {
    let slot: Rc<RefCell<Option<Option<String>>>> = Rc::new(RefCell::new(None));
    let s = slot.clone();
    state
        .view
        .evaluate_javascript(script, None, None, gio::Cancellable::NONE, move |r| {
            *s.borrow_mut() = Some(r.ok().map(|v| v.to_str().to_string()));
        });
    loop {
        if let Some(v) = slot.borrow_mut().take() {
            return v;
        }
        glib::timeout_future(Duration::from_millis(2)).await;
    }
}

async fn jump(state: &Rc<State>, script: &str) -> f64 {
    let requested = glib::monotonic_time();
    eval(state, &format!("{script}; {PRESENTED_JS} 0")).await;
    ms(requested, best(wait_presented(state).await))
}

async fn automate(state: &Rc<State>, source: &str, report: &mut Value) {
    let doc = Document::parse(source.to_string());
    let view = &state.view;

    const MARKER: &str = "feas008-clipboard-marker";
    state.window.clipboard().set_text(MARKER);
    let started = Instant::now();
    view.grab_focus();
    view.execute_editing_command("SelectAll");
    glib::timeout_future(Duration::from_millis(50)).await;
    view.execute_editing_command("Copy");
    let mut copied = String::new();
    for _ in 0..500 {
        if let Ok(Some(text)) = state.window.clipboard().read_text_future().await
            && !text.is_empty()
            && text != MARKER
        {
            copied = text.to_string();
            break;
        }
        glib::timeout_future(Duration::from_millis(10)).await;
    }
    let copy_ms = started.elapsed().as_secs_f64() * 1000.0 - 50.0;
    let normalized: String = copied.split_whitespace().collect::<Vec<_>>().join(" ");
    let probes: Vec<bool> = [0.1, 0.5, 0.9, 1.0]
        .iter()
        .map(|f| {
            let from = ((doc.blocks.len() - 1) as f64 * f) as usize;
            let block = doc.blocks[..=from]
                .iter()
                .rev()
                .find(|b| b.kind == BlockKind::Paragraph)
                .unwrap_or(&doc.blocks[from]);
            let want: String = block.text.split_whitespace().collect::<Vec<_>>().join(" ");
            let want: String = want.chars().take(60).collect();
            normalized.contains(&want)
        })
        .collect();
    report["select_all_copy"] = json!({
        "copy_ms": copy_ms,
        "plain_bytes": copied.len(),
        "model_plain_bytes": doc.text_between(doc.start(), doc.end()).len(),
        "contains_samples": probes.iter().all(|p| *p),
        "markdown_flavor": false,
    });
    eval(state, "window.getSelection().removeAllRanges()").await;

    // Heading jumps to the end, middle and start.
    let slugs: Vec<String> = doc.blocks.iter().filter_map(|b| b.slug.clone()).collect();
    let mut samples = Vec::new();
    for slug in [slugs.last(), slugs.get(slugs.len() / 2), slugs.first()]
        .into_iter()
        .flatten()
    {
        let ms = jump(
            state,
            &format!("document.getElementById({slug:?}).scrollIntoView()"),
        )
        .await;
        let top = eval(
            state,
            &format!(
                "String(Math.round(document.getElementById({slug:?}).getBoundingClientRect().top))"
            ),
        )
        .await;
        samples.push(json!({"slug": slug, "jump_to_presented_ms": ms, "heading_top_px": top, "displayed_equals_model": null}));
    }
    report["samples"] = json!(samples);

    // Native WebKit find for a phrase near the end.
    let target = doc
        .blocks
        .iter()
        .skip(doc.blocks.len() * 92 / 100)
        .find(|b| {
            b.kind == BlockKind::Paragraph
                && b.text.len() > 60
                && b.text.is_char_boundary(20)
                && b.text.is_char_boundary(44)
        });
    if let Some(b) = target {
        let query = b.text[20..44].to_string();
        let find = view.find_controller().unwrap();
        let count = Rc::new(Cell::new(None::<u32>));
        let c = count.clone();
        let found = find.connect_found_text(move |_, n| c.set(Some(n)));
        let c = count.clone();
        let failed = find.connect_failed_to_find_text(move |_| c.set(Some(0)));
        let started = Instant::now();
        find.search(
            &query,
            (webkit6::FindOptions::CASE_INSENSITIVE | webkit6::FindOptions::WRAP_AROUND).bits(),
            u32::MAX,
        );
        for _ in 0..30_000 {
            if count.get().is_some() {
                break;
            }
            glib::timeout_future(Duration::from_millis(1)).await;
        }
        let find_ms = started.elapsed().as_secs_f64() * 1000.0;
        find.disconnect(found);
        find.disconnect(failed);
        report["find"] = json!({"query_bytes": query.len(), "first_result_ms": find_ms, "reported_matches": count.get()});
        find.search_finish();
    }
}
