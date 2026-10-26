#!/usr/bin/env python3
"""Compare the Windows clipboard, read by this separate process, with a
document after an owner's Ctrl+A, Ctrl+C in a FEAS-008 Windows host.

text/markdown must equal the source bytes; CF_UNICODETEXT must equal the
model's rendered text, computed here through the same feas008_ffi.dll.
"""
from __future__ import annotations

import ctypes
import ctypes.wintypes as wt
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
user32.GetClipboardData.restype = wt.HANDLE
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalLock.argtypes = [wt.HGLOBAL]
kernel32.GlobalSize.restype = ctypes.c_size_t
kernel32.GlobalSize.argtypes = [wt.HGLOBAL]
kernel32.GlobalUnlock.argtypes = [wt.HGLOBAL]


class Str(ctypes.Structure):
    _fields_ = [("ptr", ctypes.c_void_p), ("len", ctypes.c_uint64)]


class Block(ctypes.Structure):
    _fields_ = [("kind", ctypes.c_uint32), ("level", ctypes.c_uint32), ("indent", ctypes.c_uint32), ("quote", ctypes.c_uint32),
                ("text", Str), ("sep_before", Str), ("slug", Str), ("image", Str),
                ("image_kind", ctypes.c_uint32), ("run_count", ctypes.c_uint32),
                ("run_offset", ctypes.c_uint64), ("source_start", ctypes.c_uint64), ("source_end", ctypes.c_uint64)]


class Doc(ctypes.Structure):
    _fields_ = [("version", ctypes.c_uint32), ("struct_bytes", ctypes.c_uint32), ("handle", ctypes.c_uint64),
                ("blocks", ctypes.POINTER(Block)), ("block_count", ctypes.c_uint64),
                ("runs", ctypes.c_void_p), ("run_count", ctypes.c_uint64),
                ("items", ctypes.c_void_p), ("item_count", ctypes.c_uint64),
                ("first_item", ctypes.c_void_p),
                ("outline", ctypes.c_void_p), ("outline_count", ctypes.c_uint64),
                ("links", ctypes.c_void_p), ("link_count", ctypes.c_uint64),
                ("source", Str), ("parse_ns", ctypes.c_uint64)]


def model_text(source: bytes) -> bytes:
    ffi = ctypes.CDLL(str(ROOT / "spikes/feas008/target/release/feas008_ffi.dll"))
    doc = Doc()
    if ffi.f8_open_v1(source, ctypes.c_uint64(len(source)), ctypes.c_uint64(0), ctypes.byref(doc)) != 0:
        raise SystemExit("f8_open_v1 failed")
    last = doc.block_count - 1
    out = Str()
    ffi.f8_text_between_v1(ctypes.c_uint64(doc.handle), 0, 0, ctypes.c_uint32(last),
                           ctypes.c_uint32(doc.blocks[last].text.len), ctypes.byref(out))
    text = ctypes.string_at(out.ptr, out.len)
    ffi.f8_str_free_v1(out)
    ffi.f8_free_v1(ctypes.c_uint64(doc.handle))
    return text


def read(format_id: int) -> bytes | None:
    for _ in range(50):
        if user32.OpenClipboard(None):
            break
        kernel32.Sleep(20)
    else:
        raise SystemExit("clipboard unavailable")
    try:
        handle = user32.GetClipboardData(format_id)
        if not handle:
            return None
        size = kernel32.GlobalSize(handle)
        pointer = kernel32.GlobalLock(handle)
        try:
            return ctypes.string_at(pointer, size)
        finally:
            kernel32.GlobalUnlock(handle)
    finally:
        user32.CloseClipboard()


def main() -> int:
    document = Path(sys.argv[1] if len(sys.argv) > 1 else ROOT / "build/fixtures/performance/prose-5mib.md")
    source = document.read_bytes()
    markdown = read(user32.RegisterClipboardFormatW("text/markdown"))
    unicode = read(13)
    plain = unicode.decode("utf-16-le").split("\0", 1)[0].encode("utf-8") if unicode else None
    expected = model_text(source)
    print(json.dumps({
        "document": document.name,
        "source_bytes": len(source),
        "markdown_bytes": len(markdown) if markdown is not None else None,
        "markdown_equals_source": markdown == source,
        "plain_bytes": len(plain) if plain is not None else None,
        "model_plain_bytes": len(expected),
        "plain_equals_model": plain == expected,
    }, indent=2))
    return 0 if markdown == source and plain == expected else 1


if __name__ == "__main__":
    raise SystemExit(main())
