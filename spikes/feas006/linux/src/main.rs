mod core;

use core::{CoreDocument, CoreError, CoreLibrary, ParseOptions, Status};
use gtk::glib::ControlFlow;
use gtk::prelude::*;
use gtk::{Application, ApplicationWindow, Label, ScrolledWindow, TextBuffer, TextTag, TextView};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

const APPLICATION_ID: &str = "dev.markdown.Feas006LinuxProbe";
#[derive(Debug)]
enum WorkerOutcome {
    Document(CoreDocument),
    Cancelled { observed_entry: bool },
}
type WorkerResult = Result<WorkerOutcome, String>;
type WorkerReceiver = Rc<RefCell<Option<mpsc::Receiver<WorkerResult>>>>;

#[derive(Debug)]
enum Command {
    Abi,
    SelfTest(PathBuf),
    NormalCheck,
    Contract(PathBuf),
    DebugParse(PathBuf),
    Console(PathBuf, usize),
    Gtk {
        input: PathBuf,
        checkpoint: Option<PathBuf>,
        cancel_before: bool,
        cancel_on_entry: bool,
    },
}

fn usage() -> &'static str {
    "usage: feas006-linux-host [--core PATH] --abi\n\
     feas006-linux-host [--core PATH] --self-test LARGE_INPUT\n\
     feas006-linux-host [--core PATH] --normal-check\n\
     feas006-linux-host [--core PATH] --contract INPUT\n\
     feas006-linux-host [--core PATH] --debug-parse INPUT\n\
     feas006-linux-host [--core PATH] INPUT [ITERATIONS]\n\
     feas006-linux-host [--core PATH] --gtk INPUT [--automated CHECKPOINT] \
       [--cancel-before-parse|--cancel-on-entry]"
}

fn parse_args() -> Result<(Option<PathBuf>, Command), String> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let core = if args.first().is_some_and(|arg| arg == "--core") {
        if args.len() < 2 {
            return Err("--core requires a path".into());
        }
        let value = PathBuf::from(args.remove(1));
        args.remove(0);
        Some(value)
    } else {
        None
    };
    if args.is_empty() {
        return Err("missing command".into());
    }
    let command = match args[0].as_str() {
        "--abi" if args.len() == 1 => Command::Abi,
        "--self-test" if args.len() == 2 => Command::SelfTest(PathBuf::from(&args[1])),
        "--normal-check" if args.len() == 1 => Command::NormalCheck,
        "--contract" if args.len() == 2 => Command::Contract(PathBuf::from(&args[1])),
        "--debug-parse" if args.len() == 2 => Command::DebugParse(PathBuf::from(&args[1])),
        "--gtk" => {
            if args.len() < 2 {
                return Err("--gtk requires an input".into());
            }
            let input = PathBuf::from(&args[1]);
            let mut checkpoint = None;
            let mut cancel_before = false;
            let mut cancel_on_entry = false;
            let mut index = 2;
            while index < args.len() {
                match args[index].as_str() {
                    "--automated" if checkpoint.is_none() && index + 1 < args.len() => {
                        checkpoint = Some(PathBuf::from(&args[index + 1]));
                        index += 2;
                    }
                    "--cancel-before-parse" if !cancel_before && !cancel_on_entry => {
                        cancel_before = true;
                        index += 1;
                    }
                    "--cancel-on-entry" if !cancel_before && !cancel_on_entry => {
                        cancel_on_entry = true;
                        index += 1;
                    }
                    unknown => return Err(format!("unknown or conflicting GTK option: {unknown}")),
                }
            }
            Command::Gtk {
                input,
                checkpoint,
                cancel_before,
                cancel_on_entry,
            }
        }
        flag if flag.starts_with('-') => return Err(format!("unknown command: {flag}")),
        _ if args.len() <= 2 => {
            let iterations = args
                .get(1)
                .map(|value| value.parse::<usize>())
                .transpose()
                .map_err(|_| "iterations must be an integer".to_string())?
                .unwrap_or(5);
            if iterations == 0 {
                return Err("iterations must be positive".into());
            }
            Command::Console(PathBuf::from(&args[0]), iterations)
        }
        _ => return Err("too many arguments".into()),
    };
    Ok((core, command))
}

fn read(path: &Path) -> Result<Vec<u8>, CoreError> {
    fs::read(path).map_err(|error| CoreError::Io(error.to_string()))
}

fn abi_json(core: &CoreLibrary) -> Value {
    let abi = core.abi();
    json!({
        "version": abi.version,
        "pointer_bytes": abi.pointer_bytes,
        "options_bytes": abi.options_bytes,
        "block_bytes": abi.block_bytes,
        "snapshot_bytes": abi.snapshot_bytes,
        "probe_hooks": abi.probe_hooks,
        "core_path": core.path(),
    })
}

fn document_json(document: &CoreDocument) -> Value {
    json!({
        "core_parse_model_ms": document.core_ms,
        "native_call_ms": document.native_call_ms,
        "copy_decode_ms": document.copy_decode_ms,
        "blocks": document.blocks.len(),
        "text_bytes": document.text_bytes,
        "table_bytes": document.table_bytes,
        "resident_bytes": resident_bytes(),
    })
}

fn resident_bytes() -> u64 {
    fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| {
            status.lines().find_map(|line| {
                line.strip_prefix("VmRSS:")
                    .and_then(|value| value.split_whitespace().next())
                    .and_then(|value| value.parse::<u64>().ok())
            })
        })
        .unwrap_or(0)
        * 1024
}

fn require(condition: bool, message: &'static str) -> Result<(), CoreError> {
    condition
        .then_some(())
        .ok_or(CoreError::InvalidSnapshot(message))
}

fn require_status(
    actual: Result<Status, CoreError>,
    expected: Status,
    message: &'static str,
) -> Result<(), CoreError> {
    require(actual? == expected, message)
}

fn require_zero(core: &CoreLibrary) -> Result<(), CoreError> {
    require(core.probe_live()? == (0, 0), "probe owners not zero")
}

fn run_self_test(core: Arc<CoreLibrary>, large: Vec<u8>) -> Result<Value, CoreError> {
    require(
        core.abi().probe_hooks == 1,
        "self-test requires probe hooks",
    )?;
    require_zero(&core)?;
    let small = b"# Package caf\xc3\xa9\n\nbody\n".to_vec();
    require_status(
        core.null_control_status(),
        Status::BadArgument,
        "null controls",
    )?;
    require_status(
        core.raw_status(&[0xff], ParseOptions::default()),
        Status::InvalidUtf8,
        "UTF-8 status",
    )?;
    let mut options = ParseOptions {
        version: 2,
        ..ParseOptions::default()
    };
    require_status(
        core.raw_status(&small, options),
        Status::VersionMismatch,
        "version status",
    )?;
    options = ParseOptions {
        max_input_bytes: 1,
        ..ParseOptions::default()
    };
    require_status(
        core.raw_status(&small, options),
        Status::InputLimit,
        "input limit",
    )?;
    options = ParseOptions {
        max_output_bytes: 1,
        ..ParseOptions::default()
    };
    require_status(
        core.raw_status(&small, options),
        Status::OutputLimit,
        "output limit",
    )?;
    require_status(
        core.test_cancel_request(u64::MAX),
        Status::InvalidHandle,
        "invalid handle",
    )?;
    let wrong_class = core.cancellation()?;
    require_status(
        core.test_snapshot_free(wrong_class.id_for_test()?),
        Status::InvalidHandle,
        "wrong-class handle",
    )?;
    wrong_class.close()?;
    require(
        core.test_double_snapshot_free(&small)? == (Status::Ok, Status::InvalidHandle),
        "double snapshot release",
    )?;
    let empty = core.parse_and_copy(&[], None, false)?;
    require(empty.blocks.is_empty(), "empty document")?;
    require_zero(&core)?;

    let retained = core.parse_and_copy(&small, None, false)?;
    require(
        retained
            .blocks
            .first()
            .is_some_and(|block| block.text == "Package café"),
        "copied value",
    )?;
    match core.parse_and_copy(&small, None, true) {
        Err(CoreError::HostConversionFailure) => {}
        _ => return Err(CoreError::InvalidSnapshot("host failure injection")),
    }
    require_zero(&core)?;

    let mut rss = Vec::new();
    for index in 0..1000 {
        let input = if index < 10 { &large } else { &small };
        let document = core.parse_and_copy(input, None, false)?;
        require(!document.blocks.is_empty(), "sequential parse blocks")?;
        if index % 100 == 99 {
            require_zero(&core)?;
            rss.push(resident_bytes());
        }
    }
    require_zero(&core)?;

    let small = Arc::new(small);
    let mut workers = Vec::new();
    for _ in 0..8 {
        let core = core.clone();
        let small = small.clone();
        workers.push(thread::spawn(move || -> Result<(), String> {
            for _ in 0..50 {
                let document = core
                    .parse_and_copy(&small, None, false)
                    .map_err(|e| e.to_string())?;
                if document.blocks.is_empty() {
                    return Err("empty concurrent parse".into());
                }
            }
            Ok(())
        }));
    }
    for worker in workers {
        worker
            .join()
            .map_err(|_| CoreError::InvalidSnapshot("concurrent worker panic"))?
            .map_err(CoreError::Io)?;
    }
    require_zero(&core)?;

    let large = Arc::new(large);
    let mut cancellation_ms = Vec::new();
    for _ in 0..10 {
        require_zero(&core)?;
        let mut observed_result = None;
        for _attempt in 0..5 {
            let cancellation = Arc::new(core.cancellation()?);
            let parser_core = core.clone();
            let parser_input = large.clone();
            let parser_cancel = cancellation.clone();
            let parser = thread::spawn(move || {
                parser_core.parse_and_copy(&parser_input, Some(&parser_cancel), false)
            });
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut observed = false;
            while Instant::now() < deadline && !parser.is_finished() {
                if core.probe_active()? > 0 {
                    observed = true;
                    break;
                }
                thread::yield_now();
            }
            if observed {
                let started = Instant::now();
                require(
                    cancellation.request()? == Status::Ok,
                    "cancellation request",
                )?;
                let result = parser
                    .join()
                    .map_err(|_| CoreError::InvalidSnapshot("cancel worker panic"))?;
                cancellation.close()?;
                match result {
                    Err(CoreError::Status(Status::Cancelled)) => {
                        observed_result = Some(started.elapsed().as_secs_f64() * 1000.0);
                        break;
                    }
                    _ => return Err(CoreError::InvalidSnapshot("observed cancellation result")),
                }
            } else {
                let result = parser
                    .join()
                    .map_err(|_| CoreError::InvalidSnapshot("cancel worker panic"))?;
                cancellation.close()?;
                result?;
            }
        }
        cancellation_ms
            .push(observed_result.ok_or(CoreError::InvalidSnapshot("native entry not observed"))?);
        require_zero(&core)?;
    }

    require(core.probe_panic()? == Status::Panic, "panic containment")?;
    let recovery = core.parse_and_copy(&small, None, false)?;
    require(!recovery.blocks.is_empty(), "post-panic recovery")?;
    require_zero(&core)?;
    Ok(json!({
        "passed": true,
        "sequential_loads": 1000,
        "large_loads": 10,
        "concurrent_loads": 400,
        "observed_cancellations": 10,
        "cancellation_after_entry_ms": cancellation_ms,
        "resident_bytes_after_batches": rss,
        "live_snapshots": 0,
        "live_cancellations": 0,
        "typed_errors": true,
        "host_exception_cleanup": true,
        "copied_values_survive_free": true,
        "panic_contained_and_next_call_passed": true,
    }))
}

fn run_normal_check(core: Arc<CoreLibrary>) -> Result<Value, CoreError> {
    require(
        core.abi().probe_hooks == 0,
        "normal library exposes probe hooks",
    )?;
    let small = b"# Package caf\xc3\xa9\n\nbody\n";
    for _ in 0..100 {
        let document = core.parse_and_copy(small, None, false)?;
        require(
            document
                .blocks
                .first()
                .is_some_and(|block| block.text == "Package café"),
            "normal copied value",
        )?;
    }
    require_status(
        core.raw_status(&[0xff], ParseOptions::default()),
        Status::InvalidUtf8,
        "normal UTF-8",
    )?;
    let cancellation = core.cancellation()?;
    require(cancellation.request()? == Status::Ok, "pre-cancel request")?;
    let result = core.parse_and_copy(small, Some(&cancellation), false);
    require(
        matches!(result, Err(CoreError::Status(Status::Cancelled))),
        "pre-cancel result",
    )?;
    cancellation.close()?;
    require(
        matches!(
            cancellation.request(),
            Err(CoreError::Status(Status::InvalidHandle))
        ),
        "closed cancel owner",
    )?;
    match core.parse_and_copy(small, None, true) {
        Err(CoreError::HostConversionFailure) => {}
        _ => return Err(CoreError::InvalidSnapshot("normal host error cleanup")),
    }
    let recovery = core.parse_and_copy(small, None, false)?;
    require(!recovery.blocks.is_empty(), "normal recovery")?;
    Ok(json!({"passed": true, "loads": 100, "typed_errors": true,
        "pre_cancelled": true, "host_exception_recovery": true, "probe_hooks": 0}))
}

fn headings(document: &CoreDocument) -> Value {
    Value::Array(
        document
            .blocks
            .iter()
            .filter(|block| block.kind == 1)
            .map(|block| json!({"level": block.heading_level, "text": block.text}))
            .collect(),
    )
}

fn checkpoint(path: &Path, value: &Value) -> Result<(), CoreError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| CoreError::Io(e.to_string()))?;
    }
    let temporary = path.with_extension(format!(
        "{}.tmp",
        path.extension().and_then(|v| v.to_str()).unwrap_or("json")
    ));
    fs::write(
        &temporary,
        serde_json::to_vec_pretty(value).map_err(|e| CoreError::Io(e.to_string()))?,
    )
    .map_err(|e| CoreError::Io(e.to_string()))?;
    fs::rename(temporary, path).map_err(|e| CoreError::Io(e.to_string()))
}

fn remember_failure(failure: &Rc<RefCell<Option<String>>>, message: impl Into<String>) {
    let mut slot = failure.borrow_mut();
    if slot.is_none() {
        *slot = Some(message.into());
    }
}

fn render_document(buffer: &TextBuffer, document: &CoreDocument) -> usize {
    let heading_tags: Vec<TextTag> = (1..=6)
        .map(|level| {
            TextTag::builder()
                .name(format!("h{level}"))
                .scale(match level {
                    1 => 2.0,
                    2 => 1.6,
                    3 => 1.35,
                    4 => 1.2,
                    _ => 1.0,
                })
                .weight(700)
                .pixels_above_lines(8)
                .pixels_below_lines(4)
                .build()
        })
        .collect();
    let code = TextTag::builder()
        .name("code")
        .family("monospace")
        .background("#eeeeee")
        .build();
    let metadata = TextTag::builder()
        .name("metadata")
        .family("monospace")
        .foreground("#77767b")
        .build();
    for tag in heading_tags.iter().chain([&code, &metadata]) {
        let _ = buffer.tag_table().add(tag);
    }
    buffer.begin_user_action();
    for block in &document.blocks {
        debug_assert!(block.id > 0 && block.source_start <= block.source_end);
        let mut end = buffer.end_iter();
        let tag = if block.kind == 1 && (1..=6).contains(&block.heading_level) {
            Some(&heading_tags[block.heading_level as usize - 1])
        } else if block.kind == 2 || block.kind == 3 {
            Some(&code)
        } else if block.kind == 5 {
            Some(&metadata)
        } else {
            None
        };
        if let Some(tag) = tag {
            buffer.insert_with_tags(&mut end, &block.text, &[tag]);
        } else {
            buffer.insert(&mut end, &block.text);
        }
        buffer.insert(&mut end, "\n");
    }
    buffer.end_user_action();
    buffer.char_count() as usize
}

fn worker_parse(
    core: Arc<CoreLibrary>,
    input: Arc<Vec<u8>>,
    cancel_before: bool,
    cancel_on_entry: bool,
) -> Result<WorkerOutcome, CoreError> {
    if !cancel_before && !cancel_on_entry {
        return core
            .parse_and_copy(&input, None, false)
            .map(WorkerOutcome::Document);
    }
    let cancellation = Arc::new(core.cancellation()?);
    if cancel_before {
        require(
            cancellation.request()? == Status::Ok,
            "GTK pre-cancel request",
        )?;
        let result = core.parse_and_copy(&input, Some(&cancellation), false);
        cancellation.close()?;
        return match result {
            Err(CoreError::Status(Status::Cancelled)) => Ok(WorkerOutcome::Cancelled {
                observed_entry: false,
            }),
            Ok(_) => Err(CoreError::InvalidSnapshot(
                "pre-cancellation did not return Cancelled",
            )),
            Err(error) => Err(error),
        };
    }
    let parser_core = core.clone();
    let parser_input = input.clone();
    let parser_cancel = cancellation.clone();
    let parser = thread::spawn(move || {
        parser_core.parse_and_copy(&parser_input, Some(&parser_cancel), false)
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut observed = false;
    while Instant::now() < deadline && !parser.is_finished() {
        if core.probe_active()? > 0 {
            observed = true;
            require(
                cancellation.request()? == Status::Ok,
                "GTK in-flight cancel request",
            )?;
            break;
        }
        thread::yield_now();
    }
    let result = parser
        .join()
        .map_err(|_| CoreError::InvalidSnapshot("GTK parse worker panic"))?;
    cancellation.close()?;
    classify_inflight_cancellation(observed, result)
}

fn classify_inflight_cancellation(
    observed: bool,
    result: Result<CoreDocument, CoreError>,
) -> Result<WorkerOutcome, CoreError> {
    if !observed {
        return Err(CoreError::InvalidSnapshot(
            "in-flight cancellation did not observe native entry",
        ));
    }
    match result {
        Err(CoreError::Status(Status::Cancelled)) => Ok(WorkerOutcome::Cancelled {
            observed_entry: true,
        }),
        Ok(_) => Err(CoreError::InvalidSnapshot(
            "in-flight cancellation did not return Cancelled",
        )),
        Err(error) => Err(error),
    }
}

fn run_gtk(
    core: Arc<CoreLibrary>,
    input_path: PathBuf,
    checkpoint_path: Option<PathBuf>,
    cancel_before: bool,
    cancel_on_entry: bool,
) -> Result<(), CoreError> {
    let input = Arc::new(read(&input_path)?);
    let process_start = Instant::now();
    let application = Application::builder()
        .application_id(APPLICATION_ID)
        .build();
    let failure = Rc::new(RefCell::new(None::<String>));
    let failure_activate = failure.clone();
    application.connect_activate(move |app| {
        let loading = Label::new(Some("Loading Markdown…"));
        let window = ApplicationWindow::builder()
            .application(app)
            .title("Markdown FEAS-006 Linux probe")
            .default_width(900)
            .default_height(700)
            .child(&loading)
            .build();
        window.present();
        let started = Rc::new(Cell::new(false));
        let receiver: WorkerReceiver = Rc::new(RefCell::new(None));
        let core_for_tick = core.clone();
        let input_for_tick = input.clone();
        let started_for_tick = started.clone();
        let receiver_for_tick = receiver.clone();
        window.add_tick_callback(move |_, _| {
            if !started_for_tick.replace(true) {
                let (sender, incoming) = mpsc::channel();
                *receiver_for_tick.borrow_mut() = Some(incoming);
                let core = core_for_tick.clone();
                let input = input_for_tick.clone();
                thread::spawn(move || {
                    let result = worker_parse(core, input, cancel_before, cancel_on_entry)
                        .map_err(|error| error.to_string());
                    let _ = sender.send(result);
                });
            }
            ControlFlow::Break
        });
        let app = app.clone();
        let window = window.clone();
        let checkpoint_path = checkpoint_path.clone();
        let input_path = input_path.clone();
        let core = core.clone();
        let failure = failure_activate.clone();
        gtk::glib::timeout_add_local(Duration::from_millis(1), move || {
            let result = match receiver.borrow().as_ref() {
                None => return ControlFlow::Continue,
                Some(incoming) => match incoming.try_recv() {
                    Ok(result) => result,
                    Err(mpsc::TryRecvError::Empty) => return ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        let value = json!({"schema_version": 1, "phase": "worker-disconnected",
                            "core_status": "HostError: worker channel disconnected",
                            "entry_to_worker_complete_ms": process_start.elapsed().as_secs_f64() * 1000.0});
                        if let Some(path) = &checkpoint_path
                            && let Err(error) = checkpoint(path, &value)
                        {
                            remember_failure(
                                &failure,
                                format!("worker disconnected; checkpoint failed: {error}"),
                            );
                        } else {
                            remember_failure(&failure, "worker channel disconnected");
                        }
                        app.quit();
                        return ControlFlow::Break;
                    }
                },
            };
            match result {
                Err(error) => {
                    let value = json!({"schema_version": 1, "phase": "worker-complete", "core_status": error,
                        "native_owners_zero_before_gtk": core.abi().probe_hooks == 0 || core.probe_live().ok() == Some((0, 0)),
                        "entry_to_worker_complete_ms": process_start.elapsed().as_secs_f64() * 1000.0});
                    if let Some(path) = &checkpoint_path
                        && let Err(checkpoint_error) = checkpoint(path, &value)
                    {
                        remember_failure(
                            &failure,
                            format!("worker failed: {error}; checkpoint failed: {checkpoint_error}"),
                        );
                    } else {
                        remember_failure(&failure, format!("worker failed: {error}"));
                    }
                    app.quit();
                }
                Ok(WorkerOutcome::Cancelled { observed_entry }) => {
                    let owners_zero = core.abi().probe_hooks == 0
                        || core.probe_live().ok() == Some((0, 0));
                    let value = json!({"schema_version": 1, "phase": "worker-complete",
                        "core_status": "Cancelled", "observed_cancel_entry": observed_entry,
                        "native_owners_zero_before_gtk": owners_zero,
                        "entry_to_worker_complete_ms": process_start.elapsed().as_secs_f64() * 1000.0});
                    if !owners_zero {
                        remember_failure(&failure, "native owners remained after cancellation");
                    }
                    if let Some(path) = &checkpoint_path
                        && let Err(error) = checkpoint(path, &value)
                    {
                        remember_failure(
                            &failure,
                            format!("cancellation checkpoint failed: {error}"),
                        );
                    }
                    app.quit();
                }
                Ok(WorkerOutcome::Document(document)) => {
                    let owners_zero = core.abi().probe_hooks == 0
                        || core.probe_live().ok() == Some((0, 0));
                    let mut value = json!({"schema_version": 1, "phase": "host-copy-released",
                        "core_status": "Ok", "native_owners_zero_before_gtk": owners_zero,
                        "entry_to_worker_complete_ms": process_start.elapsed().as_secs_f64() * 1000.0,
                        "core_parse_model_ms": document.core_ms, "native_call_ms": document.native_call_ms,
                        "copy_decode_ms": document.copy_decode_ms, "blocks": document.blocks.len(),
                        "text_bytes": document.text_bytes, "table_bytes": document.table_bytes,
                        "observed_cancel_entry": false});
                    if !owners_zero {
                        remember_failure(&failure, "native owners remained before GTK");
                        app.quit();
                        return ControlFlow::Break;
                    }
                    if let Some(path) = &checkpoint_path
                        && let Err(error) = checkpoint(path, &value)
                    {
                        remember_failure(
                            &failure,
                            format!("host-copy checkpoint failed: {error}"),
                        );
                        app.quit();
                        return ControlFlow::Break;
                    }
                    let buffer = TextBuffer::new(None);
                    let adapter_start = Instant::now();
                    let characters = render_document(&buffer, &document);
                    let adapter_ms = adapter_start.elapsed().as_secs_f64() * 1000.0;
                    let text_view = TextView::builder().buffer(&buffer).editable(false).cursor_visible(true)
                        .wrap_mode(gtk::WrapMode::WordChar).left_margin(32).right_margin(32)
                        .top_margin(24).bottom_margin(24).build();
                    let scroller = ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never)
                        .vscrollbar_policy(gtk::PolicyType::Automatic).child(&text_view).build();
                    window.set_child(Some(&scroller));
                    value["phase"] = json!("gtk-buffer-built");
                    value["gtk_adapter_ms"] = json!(adapter_ms);
                    value["buffer_characters"] = json!(characters);
                    if let Some(path) = &checkpoint_path
                        && let Err(error) = checkpoint(path, &value)
                    {
                        remember_failure(
                            &failure,
                            format!("GTK-buffer checkpoint failed: {error}"),
                        );
                        app.quit();
                        return ControlFlow::Break;
                    }
                    let value = Rc::new(RefCell::new(value));
                    let ticks = Rc::new(Cell::new(0_u8));
                    let path = checkpoint_path.clone();
                    let app = app.clone();
                    let failure = failure.clone();
                    let input_name = input_path.display().to_string();
                    window.add_tick_callback(move |window, _| {
                        let tick = ticks.get() + 1;
                        ticks.set(tick);
                        if tick < 2 { return ControlFlow::Continue; }
                        let mut value = value.borrow_mut();
                        value["phase"] = json!("first-useful-tick");
                        value["entry_to_first_useful_tick_ms"] = json!(process_start.elapsed().as_secs_f64() * 1000.0);
                        value["scale_factor"] = json!(window.scale_factor());
                        value["text_accessible_role"] = json!(format!("{:?}", text_view.accessible_role()));
                        value["backend"] = json!(env::var("GDK_BACKEND").unwrap_or_else(|_| "auto".into()));
                        value["input"] = json!(input_name);
                        if let Some(path) = &path {
                            if let Err(error) = checkpoint(path, &value) {
                                remember_failure(
                                    &failure,
                                    format!("final checkpoint failed: {error}"),
                                );
                            }
                            app.quit();
                        }
                        ControlFlow::Break
                    });
                }
            }
            ControlFlow::Break
        });
    });
    application.run_with_args::<&str>(&[]);
    let result = failure.borrow().clone();
    result.map_or(Ok(()), |message| Err(CoreError::Io(message)))
}

fn run() -> Result<(), CoreError> {
    let (core_override, command) = parse_args().map_err(CoreError::Io)?;
    let core_path = match core_override {
        Some(path) => path,
        None => CoreLibrary::adjacent_path()?,
    };
    let core = CoreLibrary::load(&core_path)?;
    let output = match command {
        Command::Abi => abi_json(&core),
        Command::SelfTest(path) => {
            let mut value = abi_json(&core);
            value["self_test"] = run_self_test(core.clone(), read(&path)?)?;
            value
        }
        Command::NormalCheck => {
            let mut value = abi_json(&core);
            value["normal_check"] = run_normal_check(core.clone())?;
            value
        }
        Command::Contract(path) => {
            let document = core.parse_and_copy(&read(&path)?, None, false)?;
            json!({"abi": abi_json(&core), "headings": headings(&document)})
        }
        Command::DebugParse(path) => {
            let document = core.parse_and_copy(&read(&path)?, None, false)?;
            json!({"abi": abi_json(&core), "debug_parse": document_json(&document)})
        }
        Command::Console(path, iterations) => {
            let input = read(&path)?;
            let mut rows = Vec::new();
            for _ in 0..iterations {
                let started = Instant::now();
                let document = core.parse_and_copy(&input, None, false)?;
                let mut row = document_json(&document);
                row["end_to_end_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
                rows.push(row);
            }
            json!({"abi": abi_json(&core), "rows": rows})
        }
        Command::Gtk {
            input,
            checkpoint,
            cancel_before,
            cancel_on_entry,
        } => {
            run_gtk(core, input, checkpoint, cancel_before, cancel_on_entry)?;
            return Ok(());
        }
    };
    println!(
        "{}",
        serde_json::to_string(&output).map_err(|e| CoreError::Io(e.to_string()))?
    );
    Ok(())
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("FEAS-006 Linux host failed: {error}");
            eprintln!("{}", usage());
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn document() -> CoreDocument {
        CoreDocument {
            blocks: Vec::new(),
            core_ms: 0.0,
            native_call_ms: 0.0,
            copy_decode_ms: 0.0,
            text_bytes: 0,
            table_bytes: 0,
        }
    }

    #[test]
    fn in_flight_cancellation_requires_observed_entry() {
        let error = classify_inflight_cancellation(false, Ok(document())).unwrap_err();
        assert!(error.to_string().contains("did not observe native entry"));
    }

    #[test]
    fn in_flight_cancellation_requires_cancelled_result() {
        let error = classify_inflight_cancellation(true, Ok(document())).unwrap_err();
        assert!(error.to_string().contains("did not return Cancelled"));
    }

    #[test]
    fn observed_cancelled_result_is_accepted() {
        let outcome =
            classify_inflight_cancellation(true, Err(CoreError::Status(Status::Cancelled)))
                .expect("observed cancellation should pass");
        assert!(matches!(
            outcome,
            WorkerOutcome::Cancelled {
                observed_entry: true
            }
        ));
    }
}
