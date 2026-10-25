//! Experimental FEAS-006 bulk C ABI; not the production semantic contract.
//! Core receives UTF-8 capabilities only and never accesses paths or UI types.

use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::BTreeMap;
use std::mem::size_of;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub const ABI: u32 = 1;
const MAX_INPUT: u64 = 16 * 1024 * 1024;
const MAX_OUTPUT: u64 = 64 * 1024 * 1024;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok = 0,
    BadArgument = 1,
    VersionMismatch = 2,
    InvalidUtf8 = 3,
    InputLimit = 4,
    Cancelled = 5,
    Panic = 6,
    InvalidHandle = 7,
    OutputLimit = 8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct AbiInfo {
    pub version: u32,
    pub pointer_bytes: u32,
    pub options_bytes: u32,
    pub block_bytes: u32,
    pub snapshot_bytes: u32,
    pub probe_hooks: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ParseOptions {
    pub version: u32,
    pub struct_bytes: u32,
    pub max_input_bytes: u64,
    pub max_output_bytes: u64,
    pub max_blocks: u64,
    pub cancel_handle: u64,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            version: ABI,
            struct_bytes: size_of::<Self>() as u32,
            max_input_bytes: MAX_INPUT,
            max_output_bytes: MAX_OUTPUT,
            max_blocks: 1_000_000,
            cancel_handle: 0,
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default, Debug)]
pub struct Block {
    pub id: u64,
    pub kind: u32, // 0 paragraph/rule, 1 heading, 2 code, 3 inert HTML, 4 table cell, 5 metadata
    pub heading_level: u32,
    pub source_start: u64,
    pub source_end: u64,
    pub text_offset: u64,
    pub text_len: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub version: u32,
    pub struct_bytes: u32,
    pub handle: u64,
    pub text: *const u8,
    pub text_len: u64,
    pub blocks: *const Block,
    pub block_count: u64,
    pub core_ns: u64,
}

impl Default for Snapshot {
    fn default() -> Self {
        Self {
            version: ABI,
            struct_bytes: size_of::<Self>() as u32,
            handle: 0,
            text: std::ptr::null(),
            text_len: 0,
            blocks: std::ptr::null(),
            block_count: 0,
            core_ns: 0,
        }
    }
}

struct OwnedSnapshot {
    text: Box<[u8]>,
    blocks: Box<[Block]>,
}
#[derive(Default)]
struct Registry {
    snapshots: BTreeMap<u64, OwnedSnapshot>,
    cancellations: BTreeMap<u64, Arc<AtomicBool>>,
}
static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static ACTIVE_PARSES: AtomicU64 = AtomicU64::new(0);
struct ActiveParse;
impl ActiveParse {
    fn new() -> Self {
        ACTIVE_PARSES.fetch_add(1, Ordering::AcqRel);
        Self
    }
}
impl Drop for ActiveParse {
    fn drop(&mut self) {
        ACTIVE_PARSES.fetch_sub(1, Ordering::AcqRel);
    }
}
fn registry() -> &'static Mutex<Registry> {
    REGISTRY.get_or_init(|| Mutex::new(Registry::default()))
}
fn new_id() -> Result<u64, Status> {
    loop {
        let current = NEXT_ID.load(Ordering::Relaxed);
        let next = current.checked_add(1).ok_or(Status::OutputLimit)?;
        if NEXT_ID
            .compare_exchange_weak(current, next, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            return Ok(current);
        }
    }
}
fn guard(action: impl FnOnce() -> Result<(), Status>) -> Status {
    match catch_unwind(AssertUnwindSafe(action)) {
        Ok(Ok(())) => Status::Ok,
        Ok(Err(status)) => status,
        Err(_) => Status::Panic,
    }
}
fn cancelled(token: &Option<Arc<AtomicBool>>) -> bool {
    token.as_ref().is_some_and(|t| t.load(Ordering::Acquire))
}

fn build(
    input: &str,
    options: ParseOptions,
    token: Option<Arc<AtomicBool>>,
) -> Result<OwnedSnapshot, Status> {
    let flags = Options::ENABLE_TABLES
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_GFM
        | Options::ENABLE_MATH
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
    let mut text = Vec::new();
    let mut blocks = Vec::new();
    let mut current: Option<Block> = None;
    for (event, range) in Parser::new_ext(input, flags).into_offset_iter() {
        if cancelled(&token) {
            return Err(Status::Cancelled);
        }
        if matches!(event, Event::Start(Tag::Item)) {
            finish(&mut current, &mut blocks, options)?;
            current = Some(Block {
                id: blocks.len() as u64 + 1,
                source_start: range.start as u64,
                source_end: range.start as u64,
                text_offset: text.len() as u64,
                ..Block::default()
            });
        }
        let start = match &event {
            Event::Start(Tag::Paragraph) => Some((0, 0)),
            Event::Start(Tag::Heading { level, .. }) => Some((1, *level as u32)),
            Event::Start(Tag::CodeBlock(_)) => Some((2, 0)),
            Event::Start(Tag::HtmlBlock) => Some((3, 0)),
            Event::Start(Tag::TableCell) => Some((4, 0)),
            Event::Start(Tag::MetadataBlock(_)) => Some((5, 0)),
            _ => None,
        };
        if let Some((kind, heading_level)) = start {
            if current.as_ref().is_some_and(|b| b.text_len != 0) {
                finish(&mut current, &mut blocks, options)?;
            }
            current = Some(Block {
                id: blocks.len() as u64 + 1,
                kind,
                heading_level,
                source_start: range.start as u64,
                source_end: range.end as u64,
                text_offset: text.len() as u64,
                text_len: 0,
            });
        }
        let chunk: Option<&str> = match &event {
            Event::Text(s) | Event::Code(s) | Event::Html(s) | Event::InlineHtml(s) => Some(s),
            Event::InlineMath(_) | Event::DisplayMath(_) => Some(&input[range.clone()]),
            Event::SoftBreak | Event::HardBreak => Some("\n"),
            Event::TaskListMarker(true) => Some("[x] "),
            Event::TaskListMarker(false) => Some("[ ] "),
            Event::Rule => Some("---"),
            _ => None,
        };
        if let Some(chunk) = chunk {
            // Tight list items and standalone math/rules may have no Paragraph tag.
            let standalone = current.is_none();
            if standalone {
                current = Some(Block {
                    id: blocks.len() as u64 + 1,
                    source_start: range.start as u64,
                    source_end: range.end as u64,
                    text_offset: text.len() as u64,
                    ..Block::default()
                });
            }
            let projected = text.len() as u64
                + chunk.len() as u64
                + (blocks.len() as u64 + 1) * size_of::<Block>() as u64;
            if projected > options.max_output_bytes {
                return Err(Status::OutputLimit);
            }
            text.extend_from_slice(chunk.as_bytes());
            if let Some(block) = current.as_mut() {
                block.text_len += chunk.len() as u64;
                block.source_end = block.source_end.max(range.end as u64);
            }
            if standalone {
                finish(&mut current, &mut blocks, options)?;
            }
        }
        if matches!(
            event,
            Event::End(
                TagEnd::Paragraph
                    | TagEnd::Heading(_)
                    | TagEnd::CodeBlock
                    | TagEnd::HtmlBlock
                    | TagEnd::TableCell
                    | TagEnd::MetadataBlock(_)
            )
        ) {
            if let Some(block) = current.as_mut() {
                block.source_end = range.end as u64;
            }
            finish(&mut current, &mut blocks, options)?;
        }
        if matches!(event, Event::End(TagEnd::Item)) {
            finish(&mut current, &mut blocks, options)?;
        }
    }
    if cancelled(&token) {
        return Err(Status::Cancelled);
    }
    finish(&mut current, &mut blocks, options)?;
    Ok(OwnedSnapshot {
        text: text.into_boxed_slice(),
        blocks: blocks.into_boxed_slice(),
    })
}

fn finish(
    current: &mut Option<Block>,
    blocks: &mut Vec<Block>,
    options: ParseOptions,
) -> Result<(), Status> {
    if let Some(block) = current.take() {
        if blocks.len() as u64 >= options.max_blocks
            || (blocks.len() as u64 + 1) * size_of::<Block>() as u64
                + block.text_offset
                + block.text_len
                > options.max_output_bytes
        {
            return Err(Status::OutputLimit);
        }
        blocks.push(block);
    }
    Ok(())
}

/// # Safety
/// `output` must point to a writable, aligned AbiInfo for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn md_get_abi_v1(version: u32, output: *mut AbiInfo) -> Status {
    guard(|| {
        if output.is_null() {
            return Err(Status::BadArgument);
        }
        if version != ABI {
            return Err(Status::VersionMismatch);
        }
        unsafe {
            output.write(AbiInfo {
                version: ABI,
                pointer_bytes: size_of::<usize>() as u32,
                options_bytes: size_of::<ParseOptions>() as u32,
                block_bytes: size_of::<Block>() as u32,
                snapshot_bytes: size_of::<Snapshot>() as u32,
                probe_hooks: u32::from(cfg!(feature = "probe-hooks")),
            });
        }
        Ok(())
    })
}

/// # Safety
/// Options/output must be valid aligned structs. Nonempty input must name `length`
/// readable bytes, immutable until return. Pointers returned in output remain valid
/// until exactly one successful md_snapshot_free_v1(handle), never across that free.
#[no_mangle]
pub unsafe extern "C" fn md_parse_v1(
    input: *const u8,
    length: u64,
    options: *const ParseOptions,
    output: *mut Snapshot,
) -> Status {
    guard(|| {
        if options.is_null() || output.is_null() {
            return Err(Status::BadArgument);
        }
        unsafe {
            output.write(Snapshot::default());
        }
        let options = unsafe { options.read() };
        if options.version != ABI || options.struct_bytes != size_of::<ParseOptions>() as u32 {
            return Err(Status::VersionMismatch);
        }
        if options.max_input_bytes == 0
            || options.max_input_bytes > MAX_INPUT
            || options.max_output_bytes == 0
            || options.max_output_bytes > MAX_OUTPUT
            || options.max_blocks == 0
        {
            return Err(Status::BadArgument);
        }
        if length > options.max_input_bytes {
            return Err(Status::InputLimit);
        }
        if input.is_null() && length != 0 {
            return Err(Status::BadArgument);
        }
        let token = if options.cancel_handle == 0 {
            None
        } else {
            Some(
                registry()
                    .lock()
                    .map_err(|_| Status::Panic)?
                    .cancellations
                    .get(&options.cancel_handle)
                    .ok_or(Status::InvalidHandle)?
                    .clone(),
            )
        };
        if cancelled(&token) {
            return Err(Status::Cancelled);
        }
        let bytes = if length == 0 {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(input, length as usize) }
        };
        let input = std::str::from_utf8(bytes).map_err(|_| Status::InvalidUtf8)?;
        let _active = ActiveParse::new();
        let started = Instant::now();
        let owned = build(input, options, token)?;
        let core_ns = started.elapsed().as_nanos() as u64;
        let handle = new_id()?;
        let result = Snapshot {
            handle,
            text: owned.text.as_ptr(),
            text_len: owned.text.len() as u64,
            blocks: owned.blocks.as_ptr(),
            block_count: owned.blocks.len() as u64,
            core_ns,
            ..Snapshot::default()
        };
        registry()
            .lock()
            .map_err(|_| Status::Panic)?
            .snapshots
            .insert(handle, owned);
        unsafe {
            output.write(result);
        }
        Ok(())
    })
}

#[no_mangle]
pub extern "C" fn md_snapshot_free_v1(handle: u64) -> Status {
    guard(|| {
        registry()
            .lock()
            .map_err(|_| Status::Panic)?
            .snapshots
            .remove(&handle)
            .map(|_| ())
            .ok_or(Status::InvalidHandle)
    })
}

/// # Safety
/// `output` must point to a writable, aligned u64.
#[no_mangle]
pub unsafe extern "C" fn md_cancel_create_v1(output: *mut u64) -> Status {
    guard(|| {
        if output.is_null() {
            return Err(Status::BadArgument);
        }
        unsafe {
            output.write(0);
        }
        let id = new_id()?;
        registry()
            .lock()
            .map_err(|_| Status::Panic)?
            .cancellations
            .insert(id, Arc::new(AtomicBool::new(false)));
        unsafe {
            output.write(id);
        }
        Ok(())
    })
}
#[no_mangle]
pub extern "C" fn md_cancel_request_v1(handle: u64) -> Status {
    guard(|| {
        registry()
            .lock()
            .map_err(|_| Status::Panic)?
            .cancellations
            .get(&handle)
            .ok_or(Status::InvalidHandle)?
            .store(true, Ordering::Release);
        Ok(())
    })
}
#[no_mangle]
pub extern "C" fn md_cancel_free_v1(handle: u64) -> Status {
    guard(|| {
        registry()
            .lock()
            .map_err(|_| Status::Panic)?
            .cancellations
            .remove(&handle)
            .map(|_| ())
            .ok_or(Status::InvalidHandle)
    })
}

/// # Safety
/// Both outputs must be valid writable aligned u64 values. Probe-only diagnostics.
#[cfg(feature = "probe-hooks")]
#[no_mangle]
pub unsafe extern "C" fn md_probe_live_v1(snapshots: *mut u64, cancellations: *mut u64) -> Status {
    guard(|| {
        if snapshots.is_null() || cancellations.is_null() {
            return Err(Status::BadArgument);
        }
        let r = registry().lock().map_err(|_| Status::Panic)?;
        unsafe {
            snapshots.write(r.snapshots.len() as u64);
            cancellations.write(r.cancellations.len() as u64);
        }
        Ok(())
    })
}
#[cfg(feature = "probe-hooks")]
#[no_mangle]
pub extern "C" fn md_probe_panic_v1() -> Status {
    guard(|| panic!("FEAS-006 fixed-message panic containment probe"))
}

#[cfg(feature = "probe-hooks")]
#[no_mangle]
pub extern "C" fn md_probe_active_v1() -> u64 {
    ACTIVE_PARSES.load(Ordering::Acquire)
}

#[cfg(all(feature = "probe-hooks", target_os = "windows", target_arch = "x86_64"))]
#[no_mangle]
pub extern "C" fn md_probe_break_v1() {
    unsafe {
        std::arch::asm!("int3");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn parse(bytes: &[u8], options: ParseOptions) -> (Status, Snapshot) {
        let mut result = Snapshot::default();
        let status =
            unsafe { md_parse_v1(bytes.as_ptr(), bytes.len() as u64, &options, &mut result) };
        (status, result)
    }
    #[test]
    fn snapshot_owns_values_and_ranges() {
        let mut input = "# café\n\n**bold** and `code`\n".as_bytes().to_vec();
        let (status, result) = parse(&input, ParseOptions::default());
        assert_eq!(status, Status::Ok);
        input.fill(b'x');
        let text = unsafe { std::slice::from_raw_parts(result.text, result.text_len as usize) };
        assert!(std::str::from_utf8(text).unwrap().contains("café"));
        let blocks =
            unsafe { std::slice::from_raw_parts(result.blocks, result.block_count as usize) };
        assert_eq!(blocks[0].heading_level, 1);
        assert!(blocks.iter().all(|b| b.source_start <= b.source_end
            && b.source_end <= input.len() as u64
            && b.text_offset + b.text_len <= result.text_len));
        assert_eq!(md_snapshot_free_v1(result.handle), Status::Ok);
        assert_eq!(md_snapshot_free_v1(result.handle), Status::InvalidHandle);
    }
    #[test]
    fn invalid_inputs_and_budgets() {
        assert_eq!(
            parse(&[0xff], ParseOptions::default()).0,
            Status::InvalidUtf8
        );
        let mut o = ParseOptions {
            version: 2,
            ..ParseOptions::default()
        };
        assert_eq!(parse(b"x", o).0, Status::VersionMismatch);
        o = ParseOptions {
            max_input_bytes: 1,
            ..ParseOptions::default()
        };
        assert_eq!(parse(b"xx", o).0, Status::InputLimit);
        o = ParseOptions {
            max_output_bytes: 1,
            ..ParseOptions::default()
        };
        assert_eq!(parse(b"# x", o).0, Status::OutputLimit);
        let mut result = Snapshot::default();
        assert_eq!(
            unsafe { md_parse_v1(std::ptr::null(), 1, &ParseOptions::default(), &mut result) },
            Status::BadArgument
        );
    }
    #[test]
    fn cancelled_and_stale_tokens() {
        let mut token = 0;
        assert_eq!(unsafe { md_cancel_create_v1(&mut token) }, Status::Ok);
        assert_eq!(md_cancel_request_v1(token), Status::Ok);
        assert_eq!(
            parse(
                b"# stopped",
                ParseOptions {
                    cancel_handle: token,
                    ..ParseOptions::default()
                }
            )
            .0,
            Status::Cancelled
        );
        assert_eq!(md_cancel_free_v1(token), Status::Ok);
        assert_eq!(
            parse(
                b"# stale",
                ParseOptions {
                    cancel_handle: token,
                    ..ParseOptions::default()
                }
            )
            .0,
            Status::InvalidHandle
        );
    }
    #[test]
    fn panic_stays_inside_guard() {
        assert_eq!(guard(|| panic!("injected")), Status::Panic);
    }
    #[test]
    fn empty_and_inert_html() {
        for input in [
            "",
            "<script>alert('inert')</script>\n",
            "- tight **list**\n- next\n",
            "|a|b|\n|-|-|\n|x|y|\n",
        ] {
            let (status, result) = parse(input.as_bytes(), ParseOptions::default());
            assert_eq!(status, Status::Ok);
            assert_eq!(md_snapshot_free_v1(result.handle), Status::Ok);
        }
    }
    #[test]
    fn math_source_and_tight_list_inline_grouping() {
        let (status, snapshot) = parse(
            b"# Math $E=mc^2$\n\n- tight **list**\n",
            ParseOptions::default(),
        );
        assert_eq!(status, Status::Ok);
        let text = unsafe { std::slice::from_raw_parts(snapshot.text, snapshot.text_len as usize) };
        assert_eq!(
            std::str::from_utf8(text).unwrap(),
            "Math $E=mc^2$tight list"
        );
        assert_eq!(snapshot.block_count, 2);
        assert_eq!(md_snapshot_free_v1(snapshot.handle), Status::Ok);
    }
}
