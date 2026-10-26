//! Disposable FEAS-008 C ABI over the spike block model, so the Windows C#
//! and C++/WinRT hosts present the same values the GTK host links directly.
//! See `feas008.h` for the contract. FEAS-006 already tested a hardened
//! boundary; this one only carries the model and is not validated further.

use feas008_docmodel::{BlockKind, Document, ImageRequest, LinkIntent, Pos, classify_link};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

pub const F8_OK: u32 = 0;
pub const F8_BAD_ARGUMENT: u32 = 1;
pub const F8_INVALID_UTF8: u32 = 2;
pub const F8_PANIC: u32 = 3;
pub const F8_NONE: u32 = u32::MAX;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Str {
    pub ptr: *const u8,
    pub len: u64,
}

impl Str {
    const EMPTY: Str = Str {
        ptr: std::ptr::null(),
        len: 0,
    };

    fn of(text: &str) -> Str {
        Str {
            ptr: text.as_ptr(),
            len: text.len() as u64,
        }
    }

    fn owned(text: String) -> Str {
        let boxed = text.into_bytes().into_boxed_slice();
        let len = boxed.len() as u64;
        Str {
            ptr: Box::into_raw(boxed) as *const u8,
            len,
        }
    }
}

#[repr(C)]
pub struct FfiRun {
    pub start: u32,
    pub end: u32,
    pub link: u32,
    pub style: u32,
}

#[repr(C)]
pub struct FfiBlock {
    pub kind: u32,
    pub level: u32,
    pub indent: u32,
    pub quote: u32,
    pub text: Str,
    pub sep_before: Str,
    pub slug: Str,
    pub image: Str,
    pub image_kind: u32,
    pub run_count: u32,
    pub run_offset: u64,
    pub source_start: u64,
    pub source_end: u64,
}

#[repr(C)]
pub struct FfiOutline {
    pub block: u32,
    pub level: u32,
    pub title: Str,
}

#[repr(C)]
pub struct FfiLink {
    pub intent: u32,
    pub target_block: u32,
    pub dest: Str,
    pub detail: Str,
}

#[repr(C)]
pub struct FfiItem {
    pub block: u32,
    pub start: u32,
    pub end: u32,
}

#[repr(C)]
pub struct FfiMatch {
    pub block: u32,
    pub start: u32,
    pub end: u32,
}

#[repr(C)]
pub struct FfiDoc {
    pub version: u32,
    pub struct_bytes: u32,
    pub handle: u64,
    pub blocks: *const FfiBlock,
    pub block_count: u64,
    pub runs: *const FfiRun,
    pub run_count: u64,
    pub items: *const FfiItem,
    pub item_count: u64,
    pub first_item: *const u32,
    pub outline: *const FfiOutline,
    pub outline_count: u64,
    pub links: *const FfiLink,
    pub link_count: u64,
    pub source: Str,
    pub parse_ns: u64,
}

/// Owner of the model and of the flat views handed to the host. The views
/// borrow strings from `doc`, which is never mutated after construction.
struct Owner {
    doc: Document,
    blocks: Vec<FfiBlock>,
    runs: Vec<FfiRun>,
    items: Vec<FfiItem>,
    outline: Vec<FfiOutline>,
    links: Vec<FfiLink>,
    /// Owned strings referenced by `links` (slugs and paths).
    details: Vec<String>,
}

fn kind_code(kind: BlockKind) -> (u32, u32) {
    match kind {
        BlockKind::Heading(level) => (0, level as u32),
        BlockKind::Paragraph => (1, 0),
        BlockKind::ListItem => (2, 0),
        BlockKind::Code => (3, 0),
        BlockKind::TableRow => (4, 0),
        BlockKind::Rule => (5, 0),
        BlockKind::Html => (6, 0),
        BlockKind::Math => (7, 0),
        BlockKind::Image => (8, 0),
    }
}

fn build(doc: Document) -> Box<Owner> {
    let mut owner = Box::new(Owner {
        doc,
        blocks: Vec::new(),
        runs: Vec::new(),
        items: Vec::new(),
        outline: Vec::new(),
        links: Vec::new(),
        details: Vec::new(),
    });
    let slugs = owner.doc.slugs();
    let mut details = Vec::new();
    let intents: Vec<(u32, u32, usize)> = owner
        .doc
        .links
        .iter()
        .map(|dest| {
            let (intent, target, detail) = match classify_link(dest, &slugs) {
                LinkIntent::External(_) => (0, F8_NONE, String::new()),
                LinkIntent::Fragment(slug) => (1, slugs[&slug], slug),
                LinkIntent::MissingFragment(slug) => (2, F8_NONE, slug),
                LinkIntent::LocalDocument(path) => (3, F8_NONE, path),
                LinkIntent::Blocked(reason) => (4, F8_NONE, reason.to_string()),
            };
            details.push(detail);
            (intent, target, details.len() - 1)
        })
        .collect();
    owner.details = details;
    let o = &mut *owner;
    for (dest, (intent, target, detail)) in o.doc.links.iter().zip(intents) {
        o.links.push(FfiLink {
            intent,
            target_block: target,
            dest: Str::of(dest),
            detail: Str::of(&o.details[detail]),
        });
    }
    for block in &o.doc.blocks {
        let (kind, level) = kind_code(block.kind);
        let (image_kind, image) = match &block.image {
            None => (0, Str::EMPTY),
            Some(ImageRequest::Local(path)) => (1, Str::of(path)),
            Some(ImageRequest::Blocked(reason)) => (2, Str::of(reason)),
        };
        o.blocks.push(FfiBlock {
            kind,
            level,
            indent: block.indent as u32,
            quote: block.quote as u32,
            text: Str::of(&block.text),
            sep_before: Str::of(block.sep_before),
            slug: block.slug.as_deref().map_or(Str::EMPTY, Str::of),
            image,
            image_kind,
            run_count: block.runs.len() as u32,
            run_offset: o.runs.len() as u64,
            source_start: block.source.start as u64,
            source_end: block.source.end as u64,
        });
        for run in &block.runs {
            o.runs.push(FfiRun {
                start: run.range.start,
                end: run.range.end,
                link: run.link.unwrap_or(F8_NONE),
                style: run.style as u32,
            });
        }
    }
    o.items = o
        .doc
        .items
        .iter()
        .map(|i| FfiItem {
            block: i.block,
            start: i.start,
            end: i.end,
        })
        .collect();
    for entry in &o.doc.outline {
        o.outline.push(FfiOutline {
            block: entry.block,
            level: entry.level as u32,
            title: Str::of(&entry.title),
        });
    }
    owner
}

/// # Safety
/// `handle` must come from `f8_open_v1` and not yet be freed.
unsafe fn owner<'a>(handle: u64) -> Option<&'a Owner> {
    unsafe { (handle as *const Owner).as_ref() }
}

fn guarded(f: impl FnOnce() -> u32) -> u32 {
    catch_unwind(AssertUnwindSafe(f)).unwrap_or(F8_PANIC)
}

/// Clamp a host position to a valid character boundary.
fn pos(doc: &Document, block: u32, offset: u32) -> Option<Pos> {
    let text = &doc.blocks.get(block as usize)?.text;
    let mut offset = (offset as usize).min(text.len());
    while !text.is_char_boundary(offset) {
        offset -= 1;
    }
    Some(Pos {
        block,
        offset: offset as u32,
    })
}

/// # Safety
/// `utf8` must be readable for `len` bytes and `out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_open_v1(
    utf8: *const u8,
    len: u64,
    fragment_bytes: u64,
    out: *mut FfiDoc,
) -> u32 {
    if utf8.is_null() || out.is_null() {
        return F8_BAD_ARGUMENT;
    }
    guarded(|| {
        let bytes = unsafe { std::slice::from_raw_parts(utf8, len as usize) };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return F8_INVALID_UTF8;
        };
        let started = Instant::now();
        let fragment = match fragment_bytes {
            0 => feas008_docmodel::FRAGMENT_BYTES,
            n => n as usize,
        };
        let owner = build(Document::parse_with(text.to_string(), fragment));
        let parse_ns = started.elapsed().as_nanos() as u64;
        let doc = &owner.doc;
        let value = FfiDoc {
            version: 1,
            struct_bytes: size_of::<FfiDoc>() as u32,
            handle: 0,
            blocks: owner.blocks.as_ptr(),
            block_count: owner.blocks.len() as u64,
            runs: owner.runs.as_ptr(),
            run_count: owner.runs.len() as u64,
            items: owner.items.as_ptr(),
            item_count: owner.items.len() as u64,
            first_item: doc.first_item.as_ptr(),
            outline: owner.outline.as_ptr(),
            outline_count: owner.outline.len() as u64,
            links: owner.links.as_ptr(),
            link_count: owner.links.len() as u64,
            source: Str::of(&doc.source),
            parse_ns,
        };
        let handle = Box::into_raw(owner) as u64;
        unsafe { out.write(FfiDoc { handle, ..value }) };
        F8_OK
    })
}

/// # Safety
/// `handle` must come from `f8_open_v1` and is invalid afterwards.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_free_v1(handle: u64) -> u32 {
    if handle == 0 {
        return F8_BAD_ARGUMENT;
    }
    guarded(|| {
        drop(unsafe { Box::from_raw(handle as *mut Owner) });
        F8_OK
    })
}

/// # Safety
/// `handle` must be live and `out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_text_between_v1(
    handle: u64,
    a_block: u32,
    a_offset: u32,
    b_block: u32,
    b_offset: u32,
    out: *mut Str,
) -> u32 {
    let Some(owner) = (unsafe { owner(handle) }) else {
        return F8_BAD_ARGUMENT;
    };
    guarded(|| {
        let doc = &owner.doc;
        let (Some(a), Some(b)) = (pos(doc, a_block, a_offset), pos(doc, b_block, b_offset)) else {
            return F8_BAD_ARGUMENT;
        };
        unsafe { out.write(Str::owned(doc.text_between(a, b))) };
        F8_OK
    })
}

/// # Safety
/// `handle` must be live and `out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_source_between_v1(
    handle: u64,
    a_block: u32,
    a_offset: u32,
    b_block: u32,
    b_offset: u32,
    out: *mut Str,
) -> u32 {
    let Some(owner) = (unsafe { owner(handle) }) else {
        return F8_BAD_ARGUMENT;
    };
    guarded(|| {
        let doc = &owner.doc;
        let (Some(a), Some(b)) = (pos(doc, a_block, a_offset), pos(doc, b_block, b_offset)) else {
            return F8_BAD_ARGUMENT;
        };
        unsafe { out.write(Str::of(doc.source_between(a, b))) };
        F8_OK
    })
}

/// # Safety
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_plain_offset_v1(handle: u64, block: u32, offset: u32) -> u64 {
    let Some(owner) = (unsafe { owner(handle) }) else {
        return u64::MAX;
    };
    pos(&owner.doc, block, offset).map_or(u64::MAX, |p| owner.doc.plain_offset(p) as u64)
}

/// # Safety
/// `handle` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_item_for_v1(handle: u64, block: u32, offset: u32) -> u32 {
    let Some(owner) = (unsafe { owner(handle) }) else {
        return F8_NONE;
    };
    pos(&owner.doc, block, offset).map_or(F8_NONE, |p| owner.doc.item_for(p))
}

/// # Safety
/// `handle` must be live, `query` readable for `len` bytes, outputs writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_find_v1(
    handle: u64,
    query: *const u8,
    len: u64,
    out: *mut *const FfiMatch,
    count: *mut u64,
) -> u32 {
    let Some(owner) = (unsafe { owner(handle) }) else {
        return F8_BAD_ARGUMENT;
    };
    if (query.is_null() && len > 0) || out.is_null() || count.is_null() {
        return F8_BAD_ARGUMENT;
    }
    guarded(|| {
        let bytes = if len == 0 {
            &[][..]
        } else {
            unsafe { std::slice::from_raw_parts(query, len as usize) }
        };
        let Ok(query) = std::str::from_utf8(bytes) else {
            return F8_INVALID_UTF8;
        };
        let matches: Box<[FfiMatch]> = owner
            .doc
            .find(query)
            .into_iter()
            .map(|m| FfiMatch {
                block: m.block,
                start: m.start,
                end: m.end,
            })
            .collect();
        unsafe {
            count.write(matches.len() as u64);
            out.write(Box::into_raw(matches) as *const FfiMatch);
        }
        F8_OK
    })
}

/// # Safety
/// `utf8` must be readable for `len` bytes and `out` writable.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_html_v1(utf8: *const u8, len: u64, out: *mut Str) -> u32 {
    if utf8.is_null() || out.is_null() {
        return F8_BAD_ARGUMENT;
    }
    guarded(|| {
        let bytes = unsafe { std::slice::from_raw_parts(utf8, len as usize) };
        let Ok(text) = std::str::from_utf8(bytes) else {
            return F8_INVALID_UTF8;
        };
        unsafe { out.write(Str::owned(feas008_docmodel::html::render(text))) };
        F8_OK
    })
}

/// # Safety
/// `value` must be an owned string from this library, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_str_free_v1(value: Str) {
    if !value.ptr.is_null() {
        let slice = std::ptr::slice_from_raw_parts_mut(value.ptr as *mut u8, value.len as usize);
        drop(unsafe { Box::from_raw(slice) });
    }
}

/// # Safety
/// `matches` must come from `f8_find_v1` with its `count`, freed once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn f8_matches_free_v1(matches: *const FfiMatch, count: u64) {
    if !matches.is_null() {
        let slice = std::ptr::slice_from_raw_parts_mut(matches as *mut FfiMatch, count as usize);
        drop(unsafe { Box::from_raw(slice) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(text: &str) -> FfiDoc {
        let mut doc = std::mem::MaybeUninit::<FfiDoc>::uninit();
        let status = unsafe { f8_open_v1(text.as_ptr(), text.len() as u64, 0, doc.as_mut_ptr()) };
        assert_eq!(status, F8_OK);
        unsafe { doc.assume_init() }
    }

    fn bytes<'a>(s: Str) -> &'a [u8] {
        if s.ptr.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts(s.ptr, s.len as usize) }
        }
    }

    #[test]
    fn layout_matches_header() {
        assert_eq!(size_of::<FfiBlock>(), 112);
        assert_eq!(size_of::<FfiDoc>(), 128);
        assert_eq!(size_of::<FfiRun>(), 16);
        assert_eq!(size_of::<FfiItem>(), 12);
    }

    #[test]
    fn values_round_trip() {
        let source = "# Title\n\nSee [here](#title) and [x](javascript:1).\n\n![a](img/a.png)\n";
        let doc = open(source);
        assert_eq!(bytes(doc.source), source.as_bytes());
        assert_eq!(doc.block_count, 3);
        let blocks = unsafe { std::slice::from_raw_parts(doc.blocks, 3) };
        assert_eq!((blocks[0].kind, blocks[0].level), (0, 1));
        assert_eq!(bytes(blocks[0].slug), b"title");
        assert_eq!((blocks[2].kind, blocks[2].image_kind), (8, 1));
        assert_eq!(bytes(blocks[2].image), b"img/a.png");
        let links = unsafe { std::slice::from_raw_parts(doc.links, doc.link_count as usize) };
        assert_eq!((links[0].intent, links[0].target_block), (1, 0));
        assert_eq!(links[1].intent, 4);
        let mut all = Str::EMPTY;
        let last = blocks[2].text.len as u32;
        assert_eq!(
            unsafe { f8_text_between_v1(doc.handle, 0, 0, 2, last, &mut all) },
            F8_OK
        );
        assert_eq!(bytes(all), b"Title\n\nSee here and x.\n\na");
        unsafe { f8_str_free_v1(all) };
        let mut src = Str::EMPTY;
        assert_eq!(
            unsafe { f8_source_between_v1(doc.handle, 0, 0, 2, last, &mut src) },
            F8_OK
        );
        assert_eq!(bytes(src), source.as_bytes());
        let (mut found, mut count) = (std::ptr::null(), 0);
        assert_eq!(
            unsafe { f8_find_v1(doc.handle, b"HERE".as_ptr(), 4, &mut found, &mut count) },
            F8_OK
        );
        assert_eq!(count, 1);
        unsafe { f8_matches_free_v1(found, count) };
        // A mid-character offset clamps rather than panicking.
        assert_eq!(
            unsafe { f8_plain_offset_v1(doc.handle, 1, 9999) },
            5 + 2 + 15
        );
        assert_eq!(unsafe { f8_free_v1(doc.handle) }, F8_OK);
    }

    #[test]
    fn rejects_invalid_utf8() {
        let mut doc = std::mem::MaybeUninit::<FfiDoc>::uninit();
        let bad = [0xffu8, 0xfe];
        assert_eq!(
            unsafe { f8_open_v1(bad.as_ptr(), 2, 0, doc.as_mut_ptr()) },
            F8_INVALID_UTF8
        );
    }
}
