#!/usr/bin/env python3
"""Builds kanji_table.bin, the Unicode -> QR kanji-mode lookup used by lookups::kanji_value.

Source: Unicode's JIS X 0208 mapping table
    https://www.unicode.org/Public/MAPPINGS/OBSOLETE/EASTASIA/JIS/JIS0208.TXT
whose columns are: Shift-JIS code, JIS X 0208 code, Unicode code point.

Output: 65536 little-endian u16 values, one per BMP code point. Each holds the
13-bit kanji-mode value from ISO/IEC 18004 §7.4.6, or 0xFFFF when the code
point has no kanji-mode encoding.

Usage (from the repository root):
    python3 qr-core/build-data/gen_kanji_table.py [path/to/JIS0208.TXT]
Without a path, the mapping file is downloaded from unicode.org.
"""

import struct
import sys
import urllib.request
from pathlib import Path

SOURCE_URL = "https://www.unicode.org/Public/MAPPINGS/OBSOLETE/EASTASIA/JIS/JIS0208.TXT"
OUTPUT = Path(__file__).with_name("kanji_table.bin")
NO_VALUE = 0xFFFF

# JIS0208.TXT maps Shift-JIS 0x815F to U+005C (ASCII backslash). Scanners decode
# 0x815F as a fullwidth backslash or a yen sign, so an ASCII '\' encoded in kanji
# mode would not round-trip. Map 0x815F to U+FF3C instead, leaving '\' to byte mode.
UNICODE_OVERRIDES = {0x815F: 0xFF3C}


def compact(sjis: int) -> int:
    """ISO/IEC 18004 §7.4.6: Shift-JIS double byte -> 13-bit kanji-mode value."""
    if 0x8140 <= sjis <= 0x9FFC:
        offset = sjis - 0x8140
    elif 0xE040 <= sjis <= 0xEBBF:
        offset = sjis - 0xC140
    else:
        raise ValueError(f"{sjis:#06x} is outside the QR kanji-mode ranges")
    return (offset >> 8) * 0xC0 + (offset & 0xFF)


def read_source(argv: list[str]) -> str:
    if len(argv) > 1:
        return Path(argv[1]).read_text(encoding="utf-8")
    with urllib.request.urlopen(SOURCE_URL) as response:
        return response.read().decode("utf-8")


def main() -> None:
    table = [NO_VALUE] * 65536
    for line in read_source(sys.argv).splitlines():
        if not line or line.startswith("#"):
            continue
        sjis, _jis, unicode = (int(field, 16) for field in line.split("\t")[:3])
        unicode = UNICODE_OVERRIDES.get(sjis, unicode)
        value = compact(sjis)
        assert value < 1 << 13, f"U+{unicode:04X}: {value:#x} does not fit 13 bits"
        assert table[unicode] == NO_VALUE, f"U+{unicode:04X} is mapped twice"
        table[unicode] = value

    OUTPUT.write_bytes(struct.pack("<65536H", *table))
    mapped = sum(v != NO_VALUE for v in table)
    print(f"wrote {OUTPUT} ({mapped} code points mapped)")


if __name__ == "__main__":
    main()
