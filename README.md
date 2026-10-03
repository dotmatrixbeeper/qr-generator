# qr-generator
A QR Code generator in Rust, written from scratch, ISO/IEC 18004-compliant. Includes a core encoder, CLI, and HTTP Service.

## Current Progress
- [x] Optimal Text Segmentation
- [ ] Data Encoding
- [ ] Error Correction Coding
- [ ] Structure Final Message
- [ ] Module Placement in Matrix
- [ ] Data Masking
- [ ] Format and Version Information
- [ ] QR Code CLI implementation
- [ ] QR Code SVG renderer
- [ ] QR Code PNG renderer
- [ ] QR Code API server

## Kanji Lookup Table
Kanji mode packs each character into 13 bits instead of the 16 bits of its Shift-JIS code. [`kanji_table.bin`](./qr-core/build-data/kanji_table.bin) holds those 13-bit values precomputed, and [`lookups.rs`](./qr-core/src/lookups.rs) embeds it at compile time with `include_bytes!`.

### Layout
- 65,536 little-endian `u16` entries, one per Basic Multilingual Plane code point (128 KiB).
- Entry `n` is the kanji-mode value of `U+n`, or `0xFFFF` if that character cannot be encoded in kanji mode.
- `kanji_value(c)` is then a single array index: `None` for `0xFFFF` and for anything outside the BMP, which no kanji maps to.

A direct-indexed table trades 128 KiB of binary size for an O(1) lookup with no hashing or searching. The DP probes every character in every mode, so this lookup sits on the hot path.

### Source
The mapping comes from Unicode's [JIS0208.TXT](https://www.unicode.org/Public/MAPPINGS/OBSOLETE/EASTASIA/JIS/JIS0208.TXT). Its columns are the Shift-JIS code, the JIS X 0208 code and the Unicode code point. Only the first and third are used: QR kanji mode is defined on Shift-JIS, not on the raw JIS X 0208 code.

### Value computation (ISO/IEC 18004 §7.4.6)
For a Shift-JIS code:
1. Subtract `0x8140` if the code is in `0x8140..=0x9FFC`, or `0xC140` if it is in `0xE040..=0xEBBF`.
2. Multiply the high byte of the result by `0xC0` and add the low byte.

Example: `点` is Shift-JIS `0x935F` → `0x935F − 0x8140 = 0x121F` → `0x12 × 0xC0 + 0x1F = 0x0D9F`.

Every value fits in 13 bits; the largest is `0x1F24`.

### Exception: backslash
JIS0208.TXT maps Shift-JIS `0x815F` to the ASCII backslash `U+005C`. Scanners decode `0x815F` as a fullwidth backslash or a yen sign, so an ASCII `\` sent through kanji mode would not read back as `\`. The generator maps `0x815F` to the fullwidth backslash `U+FF3C` instead, which keeps the ASCII `\` in byte mode.

### Regenerating
[`gen_kanji_table.py`](./qr-core/build-data/gen_kanji_table.py) rebuilds the table. From the repository root:

```sh
python3 qr-core/build-data/gen_kanji_table.py                # downloads JIS0208.TXT from unicode.org
python3 qr-core/build-data/gen_kanji_table.py JIS0208.TXT    # or uses a local copy
```

The script refuses to write a value wider than 13 bits or a code point mapped twice. `cargo test -p qr-core` checks the result against the standard's worked example (`点茗` → `0x0D9F`, `0x1AAA`).

## References
I use the following references to research, implement and take inspiration from, with all due credit.
1. [Denso wave's QR Code product page](https://www.qrcode.com/en/)
2. [Thonky.com's QR Code tutorial](https://www.thonky.com/qr-code-tutorial/)
3. [Project Nayuki's Optimal text segmentation for QR codes](https://www.nayuki.io/page/optimal-text-segmentation-for-qr-codes)

## AI Use Disclosure
I have used the following "AI" tools in the specified capacity.
1. **Anthropic's Claude**:
    - Claude Code was used in [`optimal_segmentation.rs`](./qr-core/src/optimal_segmentation.rs) to correct the method `probe_ecc_level_raise`.
    - Claude Code was used in [`encoding.rs`](./qr-core/src/encoding.rs) to introduce the `Version` struct and remove redundant error handlings.
    - Claude Code was used to generate the constant lookup tables in [`lookups.rs](./qr-core/src/lookups.rs).
    - Claude chat was used to understand the Optimal Text Segmentation and develop the algorithm to implement.
    - Claude Code was used in [tests/optimal_segmentation.rs](./qr-core/src/test/optimal_segmentation.rs) to write comprehensive test cases for all the text segmenation scenarios.
    - Claude Code was used in [tests/encoding.rs](./qr-core/src/tests/encoding.rs) to write all test cases for encoding module.
    - Claude Code was used to diagnose the kanji lookup table and write [`gen_kanji_table.py`](./qr-core/build-data/gen_kanji_table.py) to regenerate it.
    - Claude chat was used for Rust syntax lookups.
2. **Google's Gemini**:
    - Google's Gemini answers questions that you google about, so some sideline and Rust syntax research were done on Geimini in the way to normal googling.