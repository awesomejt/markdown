#!/usr/bin/env python3
"""Generate the FEAS-008 image and link policy fixture under build/feas008/media.

Images are synthesized here, so the fixture has no third-party content.
"""

import os
import struct
import sys
import zlib
from pathlib import Path


def png(width: int, height: int, *, claim: tuple[int, int] | None = None) -> bytes:
    def chunk(kind: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    rows = b"".join(
        b"\x00" + bytes(v for x in range(width) for v in ((x * 255) // width, (y * 255) // height, 160))
        for y in range(height)
    )
    w, h = claim or (width, height)
    header = struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)
    return b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", header) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")


DOCUMENT = """# Media and link policy

Every image and link below exercises one policy decision. Remote content is
never fetched; local images must stay inside this document's folder.

## Allowed local image

![Gradient test image](img/gradient.png)

## Refused images

![Remote image](https://example.com/remote.png)

![Escaping path](../outside.png)

![Absolute path](/etc/hostname.png)

![Symlink that leaves the folder](img/escape.png)

![Text file renamed as PNG](img/forged.png)

![Header claims 20000 by 20000](img/huge.png)

![Vector image](img/vector.svg)

![Data URI](data:image/png;base64,iVBORw0KGgo=)

![Missing file](img/missing.png)

## Links

- [Fragment to the allowed image](#allowed-local-image)
- [Missing fragment](#no-such-heading)
- [External HTTPS](https://example.com/docs)
- [Mail](mailto:someone@example.com)
- [Local document](notes/other.md)
- [Escaping document](../../secret.md)
- [JavaScript](javascript:alert(1))
- [File URL](file:///etc/passwd)
- [Data URL](data:text/html,<b>x</b>)

## Filler

""" + "\n\n".join(
    f"Filler paragraph {n} keeps the policy rows off-screen at launch so jumps reach them." for n in range(120)
) + "\n\n## End\n\nBack to [the allowed image](#allowed-local-image).\n"


def main() -> int:
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "build/feas008/media")
    img = root / "img"
    img.mkdir(parents=True, exist_ok=True)
    (root / "media-links.md").write_text(DOCUMENT, encoding="utf-8")
    (img / "gradient.png").write_bytes(png(240, 120))
    (img / "huge.png").write_bytes(png(4, 4, claim=(20000, 20000)))
    (img / "forged.png").write_text("not an image\n", encoding="utf-8")
    (img / "vector.svg").write_text('<svg xmlns="http://www.w3.org/2000/svg"/>\n', encoding="utf-8")
    outside = root.parent / "outside.png"
    outside.write_bytes(png(8, 8))
    escape = img / "escape.png"
    if escape.is_symlink() or escape.exists():
        escape.unlink()
    os.symlink(os.path.relpath(outside, img), escape)
    print(root / "media-links.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
