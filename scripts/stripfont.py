#!/usr/bin/env python3
"""Drop the sfnt tables an icon font doesn't need to render.

guise addresses Lucide glyphs by private-use codepoint (`GLYPHS[i]` in
`icon/lucide.rs`), which resolves through `cmap` alone. That makes two of the
tables upstream ships pure weight in every binary that links guise:

  GSUB  53 KB  ligature substitution, for callers who type icon *names* and
                let the shaper fold them into a glyph. guise never does.
  post  25 KB  version 2.0, which stores a name for all 1782 glyphs. Only
                tools that print glyph names read it; rasterizers don't.

`post` is replaced with a valid version 3.0 header rather than removed, since
it is a required table and some rasterizers check for it. Everything else is
copied through byte for byte, so the outlines are untouched.

Usage: python3 scripts/stripfont.py crates/guise/assets/lucide/lucide.ttf
"""

import struct
import sys

# Tables to remove outright.
DROP = {b"GSUB", b"GPOS", b"GDEF", b"DSIG", b"LTSH", b"hdmx", b"VDMX", b"gasp"}
# Replaced with a 32-byte version 3.0 stub (no glyph names).
STUB_POST = True


def checksum(data: bytes) -> int:
    """The sfnt table checksum: the sum of its 32-bit words, zero-padded."""
    padded = data + b"\0" * (-len(data) % 4)
    total = 0
    for i in range(0, len(padded), 4):
        total += struct.unpack(">I", padded[i : i + 4])[0]
    return total & 0xFFFFFFFF


def strip(raw: bytes) -> bytes:
    sfnt, count = struct.unpack(">IH", raw[:6])
    tables = {}
    order = []
    for i in range(count):
        at = 12 + i * 16
        tag = raw[at : at + 4]
        _, offset, length = struct.unpack(">III", raw[at + 4 : at + 16])
        if tag in DROP:
            continue
        body = raw[offset : offset + length]
        if tag == b"post" and STUB_POST:
            # Keep the metrics from the original header, drop the name index:
            # italicAngle, underlinePosition, underlineThickness, isFixedPitch
            # and the four memory hints are all in the first 32 bytes.
            body = b"\x00\x03\x00\x00" + body[4:32]
        tables[tag] = body
        order.append(tag)

    # The directory must be sorted by tag; the table bodies can follow in any
    # order, so they are written in the same order for a stable diff.
    order.sort()
    n = len(order)
    # Binary-search hints, per the spec.
    entry_selector = max(n.bit_length() - 1, 0)
    search_range = (1 << entry_selector) * 16
    range_shift = n * 16 - search_range

    directory = struct.pack(">IHHHH", sfnt, n, search_range, entry_selector, range_shift)
    body = b""
    offset = 12 + n * 16
    records = b""
    for tag in order:
        data = tables[tag]
        records += struct.pack(">4sIII", tag, checksum(data), offset, len(data))
        padded = data + b"\0" * (-len(data) % 4)
        body += padded
        offset += len(padded)

    out = bytearray(directory + records + body)

    # head.checkSumAdjustment covers the whole file and so has to be zeroed,
    # recomputed, and written back.
    head_at = None
    for i, tag in enumerate(order):
        if tag == b"head":
            head_at = struct.unpack(">I", out[12 + i * 16 + 8 : 12 + i * 16 + 12])[0]
            break
    if head_at is not None:
        out[head_at + 8 : head_at + 12] = b"\0\0\0\0"
        adjustment = (0xB1B0AFBA - checksum(bytes(out))) & 0xFFFFFFFF
        out[head_at + 8 : head_at + 12] = struct.pack(">I", adjustment)
    return bytes(out)


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__)
        return 2
    path = sys.argv[1]
    raw = open(path, "rb").read()
    out = strip(raw)
    open(path, "wb").write(out)
    saved = len(raw) - len(out)
    print(f"{path}: {len(raw)} -> {len(out)} bytes ({saved} saved)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
