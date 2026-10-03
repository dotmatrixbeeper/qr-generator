//! Unit tests for the data encoder.
//!
//! Every input is piped through `OptimalSegmentHint::create_segmentation` first,
//! exactly as `text_qr` does, so these exercise the encoder on the segment
//! hints it will really see. Expected codewords come from three places:
//!
//! - worked examples from ISO/IEC 18004 (Annex I, 8.4.5) and thonky.com,
//!   hard-coded byte for byte;
//! - hand-built bit strings for one rule at a time (headers, grouping, padding);
//! - `reference_codewords`, an independent bit-by-bit implementation of
//!   ISO/IEC 18004 §7.4, used to sweep many inputs across every mode, every
//!   count-indicator block and every capacity edge.
//!
//! To see the running commentary:
//!
//! ```text
//! cargo test -p qr-core encoding -- --nocapture --test-threads=1
//! ```

use super::*;
use crate::lookups::{MODE_CCI_LEN, VERSION_BLOCKS};
use crate::qr_code::ECCLevel;

// ---------------------------------------------------------------------------
// output formatting
// ---------------------------------------------------------------------------

/// Opens a test's output block: which area is under test and what is expected.
macro_rules! checking {
    ($group:expr, $($what:tt)*) => {
        println!("\n╭─ [{}]", $group);
        println!("│  {}", format!($($what)*));
    };
}

/// A detail line inside the block.
macro_rules! note {
    ($($a:tt)*) => { println!("│    {}", format!($($a)*)) };
}

/// A passing observation inside the block.
macro_rules! ok {
    ($($a:tt)*) => { println!("│  ✓ {}", format!($($a)*)) };
}

/// Closes the block.
macro_rules! done {
    ($($a:tt)*) => { println!("╰─ ✓ {}", format!($($a)*)) };
}

/// Shortens long inputs so a 7089-digit string does not flood the terminal.
fn preview(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return "\"\" (empty)".to_string();
    }
    if chars.len() <= 30 {
        return format!("{:?}", s);
    }
    let head: String = chars[..14].iter().collect();
    let tail: String = chars[chars.len() - 6..].iter().collect();
    format!("\"{head}…{tail}\" ({} chars)", chars.len())
}

/// Codewords as hex, truncated for long symbols.
fn hex(bytes: &[u8]) -> String {
    let shown: Vec<String> = bytes.iter().take(16).map(|b| format!("{b:02X}")).collect();
    if bytes.len() > 16 {
        format!("{} … ({} codewords)", shown.join(" "), bytes.len())
    } else {
        shown.join(" ")
    }
}

/// Codewords as a bit string, for diffing the exact bit where two encodings part ways.
fn bit_string(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:08b}")).collect::<Vec<_>>().join(" ")
}

/// Asserts two codeword streams are equal, pointing at the first differing bit on failure.
fn assert_codewords(label: &str, actual: &[u8], expected: &[u8]) {
    if actual == expected {
        return;
    }
    let a: String = actual.iter().map(|b| format!("{b:08b}")).collect();
    let e: String = expected.iter().map(|b| format!("{b:08b}")).collect();
    let first_diff = a.chars().zip(e.chars()).position(|(x, y)| x != y).unwrap_or(a.len().min(e.len()));
    panic!(
        "{label}: codewords differ (first differing bit {first_diff}, byte {})\n  actual   ({:>4} bytes) {}\n  expected ({:>4} bytes) {}",
        first_diff / 8,
        actual.len(),
        hex(actual),
        expected.len(),
        hex(expected),
    );
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn segment(input: &str) -> OptimalSegmentHint {
    OptimalSegmentHint::create_segmentation(input)
        .unwrap_or_else(|e| panic!("expected {input:?} to be encodable, got {e:?}"))
}

/// Segments `input` and runs the encoder on the result, as `text_qr` does.
fn encode_input(input: &str) -> Vec<u8> {
    encode_data(input, segment(input))
}

fn block_of(version: Version) -> usize {
    VERSION_BLOCKS.iter().position(|[lo, hi]| (*lo..=*hi).contains(&version.get())).unwrap()
}

fn capacity_bits(seg: &OptimalSegmentHint) -> usize {
    lookups::version_data_bits(*seg.version(), *seg.ecc_level()) as usize
}

fn modes(seg: &OptimalSegmentHint) -> Vec<Mode> {
    seg.mode_hint().iter().map(|s| s.mode()).collect()
}

/// Reads `n` bits (MSB first) starting at bit `offset` of the codeword stream.
fn read_bits(bytes: &[u8], offset: usize, n: usize) -> u32 {
    (offset..offset + n).fold(0, |acc, i| (acc << 1) | ((bytes[i / 8] >> (7 - i % 8)) & 1) as u32)
}

fn version(v: u8) -> Version {
    Version::new(v).unwrap()
}

/// A kanji-mode character (Shift-JIS double byte), 3 bytes in UTF-8.
const KANJI_CHAR: char = 'こ';

/// Shift-JIS codes for the kanji the tests use, so the reference encoder does
/// not lean on `lookups::kanji_value` (the thing under test).
const SHIFT_JIS: &[(char, u16)] = &[
    ('点', 0x935F),
    ('茗', 0xE4AA),
    ('こ', 0x82B1),
    ('ん', 0x82F1),
    ('に', 0x82C9),
    ('ち', 0x82BF),
    ('は', 0x82CD),
    ('世', 0x90A2),
    ('界', 0x8A45),
];

/// ISO/IEC 18004 §7.4.6: subtract 0x8140 (or 0xC140 above 0x9FFC), then
/// high byte × 0xC0 + low byte, giving a 13-bit value.
fn kanji_compact(c: char) -> u16 {
    let sjis = SHIFT_JIS
        .iter()
        .find(|(k, _)| *k == c)
        .unwrap_or_else(|| panic!("{c:?} is not in the test Shift-JIS table; add it to SHIFT_JIS"))
        .1;
    let offset = if sjis <= 0x9FFC { sjis - 0x8140 } else { sjis - 0xC140 };
    (offset >> 8) * 0xC0 + (offset & 0xFF)
}

/// MSB-first bit accumulator, one `bool` per bit. Deliberately naive so it
/// shares no logic with the encoder's in-place byte packing.
#[derive(Default)]
struct Bits(Vec<bool>);

impl Bits {
    fn push(&mut self, value: u32, n: usize) {
        assert!(n == 32 || value < (1 << n), "{value} does not fit in {n} bits");
        for i in (0..n).rev() {
            self.0.push((value >> i) & 1 == 1);
        }
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn to_bytes(&self) -> Vec<u8> {
        assert_eq!(self.0.len() % 8, 0, "not byte aligned");
        self.0.chunks(8).map(|byte| byte.iter().fold(0u8, |acc, &b| (acc << 1) | b as u8)).collect()
    }
}

/// Mode indicator, count indicator and data bits for every segment, in order.
fn reference_payload(input: &str, seg: &OptimalSegmentHint) -> Bits {
    let chars: Vec<char> = input.chars().collect();
    let block = block_of(*seg.version());
    let mut bits = Bits::default();

    for s in seg.mode_hint() {
        let text = &chars[s.start()..=s.end()];
        let cci_len = MODE_CCI_LEN[block][s.mode() as usize] as usize;
        match s.mode() {
            Mode::Numeric => {
                bits.push(0b0001, 4);
                bits.push(text.len() as u32, cci_len);
                for group in text.chunks(3) {
                    let value: u32 = group.iter().collect::<String>().parse().unwrap();
                    bits.push(value, [0, 4, 7, 10][group.len()]);
                }
            }
            Mode::Alphanumeric => {
                bits.push(0b0010, 4);
                bits.push(text.len() as u32, cci_len);
                for pair in text.chunks(2) {
                    let v: Vec<u32> = pair.iter().map(|&c| lookups::alphanumeric_value(c).unwrap() as u32).collect();
                    match v[..] {
                        [a, b] => bits.push(a * 45 + b, 11),
                        [a] => bits.push(a, 6),
                        _ => unreachable!(),
                    }
                }
            }
            Mode::Byte => {
                let bytes = text.iter().collect::<String>().into_bytes();
                bits.push(0b0100, 4);
                // ISO/IEC 18004 §7.4.5: byte mode counts bytes, not characters
                bits.push(bytes.len() as u32, cci_len);
                for b in bytes {
                    bits.push(b as u32, 8);
                }
            }
            Mode::Kanji => {
                bits.push(0b1000, 4);
                bits.push(text.len() as u32, cci_len);
                for &c in text {
                    bits.push(kanji_compact(c) as u32, 13);
                }
            }
        }
    }
    bits
}

/// The full data codeword sequence ISO/IEC 18004 §7.4.9–7.4.10 prescribes:
/// payload, up to four terminator zeros, zero bits to the next byte boundary,
/// then 0xEC / 0x11 alternating until the symbol's data capacity is filled.
fn reference_codewords(input: &str, seg: &OptimalSegmentHint) -> Vec<u8> {
    let capacity = capacity_bits(seg);
    let mut bits = reference_payload(input, seg);
    assert!(bits.len() <= capacity, "reference payload overflows {}-{:?}", seg.version().get(), seg.ecc_level());

    let terminator = (capacity - bits.len()).min(4);
    bits.push(0, terminator);
    let to_boundary = (8 - bits.len() % 8) % 8;
    bits.push(0, to_boundary);

    let mut bytes = bits.to_bytes();
    for pad in [0xEC, 0x11].into_iter().cycle() {
        if bytes.len() * 8 >= capacity {
            break;
        }
        bytes.push(pad);
    }
    bytes
}

/// Encodes `input` and checks it against the reference, returning the codewords.
fn assert_matches_reference(input: &str) -> Vec<u8> {
    let seg = segment(input);
    let expected = reference_codewords(input, &seg);
    let label = format!("{} at {}-{:?}", preview(input), seg.version().get(), seg.ecc_level());
    let actual = encode_data(input, seg);
    assert_codewords(&label, &actual, &expected);
    actual
}

// ---------------------------------------------------------------------------
// reference self-check
// ---------------------------------------------------------------------------

#[test]
fn reference_kanji_compaction_matches_iso_example() {
    checking!("reference", "the test-side kanji compaction must reproduce ISO/IEC 18004 §7.4.6 before it judges the encoder");

    assert_eq!(kanji_compact('点'), 0x0D9F, "0x935F is in the 0x8140..=0x9FFC range");
    ok!("'点' 0x935F  →  0x0D9F  (lower Shift-JIS range)");
    assert_eq!(kanji_compact('茗'), 0x1AAA, "0xE4AA is in the 0xE040..=0xEBBF range");
    ok!("'茗' 0xE4AA  →  0x1AAA  (upper Shift-JIS range)");

    done!("reference compaction agrees with the standard on both ranges");
}

// ---------------------------------------------------------------------------
// the bit writer: encode()
// ---------------------------------------------------------------------------

/// Runs a sequence of `(value, width)` writes through `encode` from an empty buffer.
fn write_all(writes: &[(u16, i8)]) -> (Vec<u8>, u8) {
    let mut offset = 0;
    let mut out = Vec::new();
    for &(value, width) in writes {
        encode(&mut offset, &mut out, value, width);
    }
    (out, offset)
}

#[test]
fn encode_writes_msb_first_into_a_fresh_byte() {
    checking!("encode", "a write into an empty buffer starts a byte and fills it from the top bit down");

    let (out, offset) = write_all(&[(0b1011, 4)]);
    note!("wrote 1011 (4 bits)  →  {}  offset {offset}", bit_string(&out));
    assert_eq!(out, vec![0b1011_0000]);
    assert_eq!(offset, 4, "four bits of the byte should still be free");

    done!("value lands in the high nibble, offset tracks the 4 free bits");
}

#[test]
fn encode_fills_a_partial_byte_before_starting_a_new_one() {
    checking!("encode", "consecutive writes share a byte until it is full");

    let (out, offset) = write_all(&[(0b0001, 4), (0b0010, 4)]);
    note!("0001 + 0010  →  {}", bit_string(&out));
    assert_eq!(out, vec![0b0001_0010], "two nibbles should pack into one byte");
    assert_eq!(offset, 0, "the byte is exactly full");
    ok!("two 4-bit writes pack into a single byte");

    let (out, _) = write_all(&[(1, 1); 8]);
    assert_eq!(out, vec![0xFF], "eight single-bit writes should make one byte");
    ok!("eight 1-bit writes  →  FF");

    let (out, _) = write_all(&[(1, 1); 9]);
    assert_eq!(out, vec![0xFF, 0b1000_0000], "a ninth bit should open a second byte");
    ok!("nine 1-bit writes   →  FF 80");

    done!("bytes are filled completely before a new one is pushed");
}

#[test]
fn encode_straddles_byte_boundaries() {
    checking!("encode", "a value wider than the free space splits across bytes without losing bits");

    // 0001 | 0000001000 — the numeric header for 8 digits at version 1
    let (out, offset) = write_all(&[(0b0001, 4), (8, 10)]);
    note!("0001 + 0000001000  →  {}", bit_string(&out));
    assert_eq!(out, vec![0b0001_0000, 0b0010_0000]);
    assert_eq!(offset, 2);
    ok!("10-bit value split 4 + 6 across two bytes");

    // 3 bits then 13 bits fills exactly two bytes
    let (out, offset) = write_all(&[(0b101, 3), (0b1_1111_0000_1111, 13)]);
    note!("101 + 1111100001111  →  {}", bit_string(&out));
    assert_eq!(out, vec![0b1011_1111, 0b0000_1111]);
    assert_eq!(offset, 0);
    ok!("13-bit kanji-width value split 5 + 8");

    // 7 bits then 11 bits spans three bytes
    let (out, offset) = write_all(&[(0b1010101, 7), (0b111_0000_1111, 11)]);
    note!("1010101 + 11100001111  →  {}", bit_string(&out));
    assert_eq!(out, vec![0b1010_1011, 0b1100_0011, 0b1100_0000]);
    assert_eq!(offset, 6);
    ok!("11-bit value split 1 + 8 + 2 across three bytes");

    done!("straddling writes keep every bit in order");
}

#[test]
fn encode_handles_the_full_16_bit_width() {
    checking!("encode", "byte-mode count indicators are 16 bits in versions 10-40, the widest write the encoder makes");

    let (out, offset) = write_all(&[(0xBEEF, 16)]);
    assert_eq!(out, vec![0xBE, 0xEF]);
    assert_eq!(offset, 0);
    ok!("aligned  BEEF  →  {}", hex(&out));

    let (out, offset) = write_all(&[(0b0100, 4), (0xBEEF, 16)]);
    assert_eq!(out, vec![0x4B, 0xEE, 0xF0]);
    assert_eq!(offset, 4);
    ok!("after a 4-bit mode indicator  →  {}", hex(&out));

    done!("no bits lost to u16 shifts at full width");
}

#[test]
fn encode_masks_nothing_it_was_not_asked_to() {
    checking!("encode", "leading zeros of a value are written, not skipped");

    let (out, _) = write_all(&[(0, 10), (1, 6)]);
    assert_eq!(out, vec![0x00, 0b0000_0001]);
    ok!("0 in 10 bits then 1 in 6 bits  →  {}", bit_string(&out));

    done!("zero-valued and small values occupy their full width");
}

#[test]
fn encode_of_zero_width_is_a_no_op() {
    checking!("encode", "the terminator can be 0 bits wide when the payload exactly fills the symbol");

    let (out, offset) = write_all(&[(0b1010_1010, 8), (0, 0)]);
    assert_eq!(out, vec![0b1010_1010]);
    assert_eq!(offset, 0);

    done!("zero-width write left buffer and offset untouched");
}

// ---------------------------------------------------------------------------
// pad_bytes
// ---------------------------------------------------------------------------

#[test]
fn pad_bytes_alternates_ec_11_up_to_capacity() {
    checking!("pad_bytes", "pad codewords alternate 11101100 / 00010001 starting with 0xEC");

    let mut out = vec![0x10, 0x20];
    pad_bytes(&mut out, 72);
    note!("2 data bytes, 72-bit symbol  →  {}", hex(&out));
    assert_eq!(out, vec![0x10, 0x20, 0xEC, 0x11, 0xEC, 0x11, 0xEC, 0x11, 0xEC]);

    done!("filled to exactly 9 codewords, never past capacity");
}

#[test]
fn pad_bytes_is_a_no_op_on_a_full_symbol() {
    checking!("pad_bytes", "a symbol already at capacity gets no pad codewords");

    let mut out = vec![0xAA; 19];
    pad_bytes(&mut out, 152);
    assert_eq!(out.len(), 19, "1-L holds 19 data codewords; got {}", out.len());

    done!("length stayed at 19");
}

#[test]
fn pad_bytes_fills_an_empty_buffer_for_every_version_and_level() {
    checking!("pad_bytes", "starting from nothing, the result is exactly capacity / 8 codewords for all 160 symbols");

    for v in Version::MIN..=Version::MAX {
        for ecc in [ECCLevel::L, ECCLevel::M, ECCLevel::Q, ECCLevel::H] {
            let bits = lookups::version_data_bits(version(v), ecc);
            let mut out = Vec::new();
            pad_bytes(&mut out, bits);
            assert_eq!(out.len(), bits as usize / 8, "{v}-{ecc:?}");
            assert!(out.iter().step_by(2).all(|&b| b == 0xEC), "{v}-{ecc:?} even positions must be 0xEC");
            assert!(out.iter().skip(1).step_by(2).all(|&b| b == 0x11), "{v}-{ecc:?} odd positions must be 0x11");
        }
    }

    done!("all 160 version/level capacities padded exactly");
}

// ---------------------------------------------------------------------------
// known vectors
// ---------------------------------------------------------------------------

#[test]
fn iso_annex_numeric_example() {
    checking!("known vector", "ISO/IEC 18004 Annex I: \"01234567\" packs to 10 20 0C 56 61 80 then pad");

    let input = "01234567";
    let seg = segment(input);
    note!("segmented to {:?} at {}-{:?}", modes(&seg), seg.version().get(), seg.ecc_level());
    assert_eq!(modes(&seg), vec![Mode::Numeric]);
    assert_eq!(seg.version().get(), 1);
    assert_eq!(*seg.ecc_level() as u8, ECCLevel::H as u8, "41 bits leave room for 1-H (72 bits)");

    let out = encode_data(input, seg);
    note!("got      {}", hex(&out));
    // 0001 0000001000 0000001100 0101011001 1000011 0000 000 | EC 11 EC
    assert_codewords(input, &out, &[0x10, 0x20, 0x0C, 0x56, 0x61, 0x80, 0xEC, 0x11, 0xEC]);

    done!("matches the standard's worked example, padded to the 9 codewords of 1-H");
}

#[test]
fn thonky_hello_world_example() {
    checking!("known vector", "\"HELLO WORLD\" at 1-Q is the classic alphanumeric walkthrough");

    let input = "HELLO WORLD";
    let seg = segment(input);
    note!("segmented to {:?} at {}-{:?}", modes(&seg), seg.version().get(), seg.ecc_level());
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric]);
    assert_eq!(*seg.ecc_level() as u8, ECCLevel::Q as u8, "74 bits fit 1-Q (104) but not 1-H (72)");

    let out = encode_data(input, seg);
    note!("got      {}", hex(&out));
    assert_codewords(
        input,
        &out,
        &[0x20, 0x5B, 0x0B, 0x78, 0xD1, 0x72, 0xDC, 0x4D, 0x43, 0x40, 0xEC, 0x11, 0xEC],
    );

    done!("13 codewords for 1-Q, identical to the published walkthrough");
}

#[test]
fn iso_kanji_example() {
    checking!("known vector", "ISO/IEC 18004 §7.4.6: \"点茗\" compacts to 0x0D9F and 0x1AAA");

    let input = "点茗";
    let seg = segment(input);
    note!("segmented to {:?} at {}-{:?}", modes(&seg), seg.version().get(), seg.ecc_level());
    assert_eq!(modes(&seg), vec![Mode::Kanji]);

    let out = encode_data(input, seg);
    note!("got      {}", hex(&out));
    // 1000 00000010 0110110011111 1101010101010 0000 000000 | EC 11 EC
    assert_codewords(input, &out, &[0x80, 0x26, 0xCF, 0xEA, 0xA8, 0x00, 0xEC, 0x11, 0xEC]);

    done!("both Shift-JIS ranges compacted to the standard's 13-bit values");
}

#[test]
fn byte_mode_hello_example() {
    checking!("known vector", "\"hello\" is five 8-bit bytes behind a byte-mode header");

    let input = "hello";
    let out = encode_input(input);
    note!("got      {}", hex(&out));
    // 0100 00000101 01101000 01100101 01101100 01101100 01101111 0000 | EC 11
    assert_codewords(input, &out, &[0x40, 0x56, 0x86, 0x56, 0xC6, 0xC6, 0xF0, 0xEC, 0x11]);

    done!("ASCII bytes copied verbatim");
}

// ---------------------------------------------------------------------------
// segment headers
// ---------------------------------------------------------------------------

#[test]
fn mode_indicator_leads_each_mode() {
    checking!("header", "the first four bits name the mode: 0001 N, 0010 A, 0100 B, 1000 K");

    for (input, mode, indicator) in [
        ("123", Mode::Numeric, 0b0001),
        ("ABC", Mode::Alphanumeric, 0b0010),
        ("abc", Mode::Byte, 0b0100),
        ("こんにちは", Mode::Kanji, 0b1000),
    ] {
        let seg = segment(input);
        assert_eq!(modes(&seg), vec![mode], "{input:?} should be a single {mode:?} segment");
        let out = encode_data(input, seg);
        let got = read_bits(&out, 0, 4);
        assert_eq!(got, indicator, "{input:?}: mode indicator {got:04b}, expected {indicator:04b}");
        note!("{:<14} {:<13} →  {got:04b}", format!("{input:?}"), format!("{mode:?}"));
    }

    done!("all four mode indicators correct");
}

#[test]
fn character_count_indicator_holds_the_segment_length() {
    checking!("header", "the count indicator follows the mode and holds the number of characters in the segment");

    for (input, width) in [
        ("1", 10),
        ("12345678", 10),
        ("A", 9),
        ("HELLO WORLD", 9),
        ("a", 8),
        ("hello", 8),
        ("こ", 8),
        ("こんにちは世界", 8),
    ] {
        let out = encode_input(input);
        let count = read_bits(&out, 4, width);
        let expected = input.chars().count() as u32;
        assert_eq!(count, expected, "{input:?}: count indicator {count}, expected {expected}");
        note!("{:<18} {width:>2}-bit count  →  {count}", format!("{input:?}"));
    }

    done!("counts read back as the segment length, including length-1 segments");
}

#[test]
fn byte_mode_counts_utf8_bytes_not_chars() {
    checking!("header", "byte mode's count is the number of bytes (§7.4.5); multi-byte chars must not undercount");

    for input in ["é", "éé", "🦀", "🦀🦀", "naïve café"] {
        let seg = segment(input);
        assert!(modes(&seg).iter().all(|m| *m == Mode::Byte), "{input:?} should be byte only, got {:?}", modes(&seg));
        let out = encode_data(input, seg);
        let count = read_bits(&out, 4, 8);
        assert_eq!(count, input.len() as u32, "{input:?}: {} chars, {} bytes", input.chars().count(), input.len());
        note!("{:<14} {} char(s)  →  count {count}", format!("{input:?}"), input.chars().count());
    }

    done!("count tracks UTF-8 length, not char count");
}

#[test]
fn count_indicator_widens_with_the_version_block() {
    checking!("header", "count indicators widen at v10 and v27 (ISO/IEC 18004 table 3)");
    note!("            v1-9  v10-26  v27-40");
    note!("numeric       10      12      14");
    note!("alphanumeric   9      11      13");
    note!("byte           8      16      16");
    note!("kanji          8      10      12");

    let kanji = |n: usize| std::iter::repeat_n(KANJI_CHAR, n).collect::<String>();
    let cases: Vec<(&str, String, Mode)> = vec![
        ("numeric", "1".repeat(10), Mode::Numeric),
        ("numeric", "1".repeat(600), Mode::Numeric),
        ("numeric", "1".repeat(3400), Mode::Numeric),
        ("alphanumeric", "A".repeat(10), Mode::Alphanumeric),
        ("alphanumeric", "A".repeat(400), Mode::Alphanumeric),
        ("alphanumeric", "A".repeat(3000), Mode::Alphanumeric),
        ("byte", "a".repeat(10), Mode::Byte),
        ("byte", "a".repeat(400), Mode::Byte),
        ("byte", "a".repeat(2000), Mode::Byte),
        ("kanji", kanji(10), Mode::Kanji),
        ("kanji", kanji(200), Mode::Kanji),
        ("kanji", kanji(1000), Mode::Kanji),
    ];

    let mut blocks_seen = [false; 3];
    for (label, input, mode) in &cases {
        let seg = segment(input);
        assert_eq!(modes(&seg), vec![*mode], "{label} input should be one {mode:?} segment");
        let block = block_of(*seg.version());
        blocks_seen[block] = true;
        let width = MODE_CCI_LEN[block][*mode as usize] as usize;
        let v = seg.version().get();

        let out = encode_data(input, seg);
        let count = read_bits(&out, 4, width);
        assert_eq!(count, input.chars().count() as u32, "{label} at v{v}: {width}-bit count read back {count}");
        note!("{label:<13} {:>5} chars  →  v{v:<2}  {width:>2}-bit count  {count}", input.chars().count());
    }
    assert!(blocks_seen.iter().all(|b| *b), "every version block should be exercised");

    done!("count width correct in all three blocks for all four modes");
}

// ---------------------------------------------------------------------------
// numeric data
// ---------------------------------------------------------------------------

#[test]
fn numeric_groups_digits_in_threes() {
    checking!("numeric", "digits pack in groups of three (10 bits), with a 2-digit tail in 7 bits and a 1-digit tail in 4");

    // (input, groups the data must be written as)
    let cases: [(&str, &[(u32, usize)]); 7] = [
        ("1", &[(1, 4)]),
        ("12", &[(12, 7)]),
        ("123", &[(123, 10)]),
        ("1234", &[(123, 10), (4, 4)]),
        ("12345", &[(123, 10), (45, 7)]),
        ("123456", &[(123, 10), (456, 10)]),
        ("9999999", &[(999, 10), (999, 10), (9, 4)]),
    ];

    for (input, groups) in cases {
        let out = encode_input(input);
        let mut offset = 4 + 10;
        for &(value, width) in groups {
            let got = read_bits(&out, offset, width);
            assert_eq!(got, value, "{input:?}: group at bit {offset} read {got}, expected {value} in {width} bits");
            offset += width;
        }
        let summary = groups.iter().map(|(v, w)| format!("{v}/{w}b")).collect::<Vec<_>>().join(" ");
        note!("{:<10} →  {summary}", format!("{input:?}"));
    }

    done!("every tail length (0, 1, 2) packed at the right width");
}

#[test]
fn numeric_keeps_leading_zeros() {
    checking!("numeric", "a group like \"007\" is the value 7 in 10 bits; its zeros are part of the count, not dropped");

    for input in ["0", "00", "000", "007", "0000", "00000000"] {
        let out = assert_matches_reference(input);
        let count = read_bits(&out, 4, 10);
        assert_eq!(count, input.len() as u32, "{input:?}: leading zeros must still be counted");
        note!("{:<12} →  count {count}  {}", format!("{input:?}"), hex(&out));
    }

    done!("all-zero and zero-led groups encode at full width");
}

// ---------------------------------------------------------------------------
// alphanumeric data
// ---------------------------------------------------------------------------

#[test]
fn alphanumeric_packs_pairs_as_45a_plus_b() {
    checking!("alphanumeric", "pairs pack as 45 × first + second in 11 bits; an odd last char takes 6 bits");

    let cases: [(&str, &[(u32, usize)]); 5] = [
        ("A", &[(10, 6)]),
        ("AC", &[(10 * 45 + 12, 11)]),
        ("HE", &[(17 * 45 + 14, 11)]),
        ("HEL", &[(779, 11), (21, 6)]),
        ("::", &[(44 * 45 + 44, 11)]),
    ];

    for (input, groups) in cases {
        let seg = segment(input);
        assert_eq!(modes(&seg), vec![Mode::Alphanumeric], "{input:?}");
        let out = encode_data(input, seg);
        let mut offset = 4 + 9;
        for &(value, width) in groups {
            let got = read_bits(&out, offset, width);
            assert_eq!(got, value, "{input:?}: group at bit {offset} read {got}, expected {value} in {width} bits");
            offset += width;
        }
        let summary = groups.iter().map(|(v, w)| format!("{v}/{w}b")).collect::<Vec<_>>().join(" ");
        note!("{:<8} →  {summary}", format!("{input:?}"));
    }

    done!("pair arithmetic and odd-length tail correct, including the largest pair (2024)");
}

#[test]
fn alphanumeric_covers_the_whole_charset() {
    checking!("alphanumeric", "every one of the 45 charset members encodes to its table value");

    let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
    let out = assert_matches_reference(input);
    note!("{} chars  →  {}", input.len(), hex(&out));
    ok!("letters, space and the eight symbols all match the reference");

    let input = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:";
    let seg = segment(input);
    note!("full charset segmented as {:?}", modes(&seg));
    assert_matches_reference(input);
    ok!("with digits too, whatever segmentation it picks");

    done!("charset values agree with ISO/IEC 18004 table 5");
}

// ---------------------------------------------------------------------------
// byte and kanji data
// ---------------------------------------------------------------------------

#[test]
fn byte_mode_writes_utf8_bytes() {
    checking!("byte", "multi-byte characters are written as their full UTF-8 sequence");

    for input in ["é", "🦀", "naïve café", "hello, world!", "a\u{0}b", "\u{10FFFF}"] {
        let out = assert_matches_reference(input);
        note!("{:<18} →  {}", format!("{input:?}"), hex(&out));
    }

    let out = encode_input("🦀");
    let bytes: Vec<u32> = (0..4).map(|i| read_bits(&out, 12 + i * 8, 8)).collect();
    assert_eq!(bytes, vec![0xF0, 0x9F, 0xA6, 0x80]);
    ok!("'🦀' emitted as F0 9F A6 80");

    done!("UTF-8 sequences copied byte for byte, NUL and U+10FFFF included");
}

#[test]
fn kanji_values_are_13_bit_compacted_shift_jis() {
    checking!("kanji", "each kanji is its compacted Shift-JIS value in 13 bits");

    for c in SHIFT_JIS.iter().map(|(c, _)| *c) {
        let input = c.to_string();
        let seg = segment(&input);
        assert_eq!(modes(&seg), vec![Mode::Kanji], "{c:?}");
        let out = encode_data(&input, seg);
        let got = read_bits(&out, 12, 13);
        let expected = kanji_compact(c) as u32;
        assert_eq!(got, expected, "{c:?}: read {got:#06x}, expected {expected:#06x}");
        note!("{c:?}  →  {got:#06x}");
    }

    done!("{} kanji compacted correctly", SHIFT_JIS.len());
}

#[test]
fn kanji_runs_match_reference() {
    checking!("kanji", "multi-character kanji segments match the reference");

    for input in ["こんにちは", "こんにちは世界", "点茗点茗点茗"] {
        let out = assert_matches_reference(input);
        note!("{:<20} →  {}", format!("{input:?}"), hex(&out));
    }

    done!("kanji runs encoded end to end");
}

// ---------------------------------------------------------------------------
// mixed segments
// ---------------------------------------------------------------------------

#[test]
fn segments_are_encoded_back_to_back() {
    checking!("mixed", "each segment gets its own header, directly after the previous segment's last data bit");

    let input = "AAAAAAAAAAAAAA12345678aaaaaaaaaaaa";
    let seg = segment(input);
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric, Mode::Numeric, Mode::Byte]);
    let out = encode_data(input, seg);

    // alphanumeric: 4 + 9 + 7 × 11 = 90 bits, numeric: 4 + 10 + 2 × 10 + 7 = 41 bits
    let mut at = 0;
    assert_eq!(read_bits(&out, at, 4), 0b0010, "segment 1 mode");
    assert_eq!(read_bits(&out, at + 4, 9), 14, "segment 1 count");
    note!("bit {at:>3}  Alphanumeric × 14");
    at += 90;
    assert_eq!(read_bits(&out, at, 4), 0b0001, "segment 2 mode");
    assert_eq!(read_bits(&out, at + 4, 10), 8, "segment 2 count");
    note!("bit {at:>3}  Numeric × 8");
    at += 41;
    assert_eq!(read_bits(&out, at, 4), 0b0100, "segment 3 mode");
    assert_eq!(read_bits(&out, at + 4, 8), 12, "segment 3 count");
    note!("bit {at:>3}  Byte × 12");

    assert_matches_reference(input);
    done!("headers land at bits 0, 90 and 131 with no gap or overlap");
}

#[test]
fn mixed_inputs_match_reference() {
    checking!("mixed", "multi-mode inputs encode exactly as the reference does for the same segmentation");

    for input in [
        "abc123こんにちは456ABCDEF",
        "ABCDEFGHIJ1234567890123456789012345678901234567890ABCDEFGHIJ",
        "1234567890abcdefghij1234567890ABCDEFGHIJ1234567890こんにちは世界1234567890",
        "abc123こんにちは456ABCDEF hello WORLD 9876543210",
        "HTTPS://EXAMPLE.COM/PATH?query=1&x=こんにちは",
        "1a",
        "a1",
        "1こ",
        "こ1",
    ] {
        let seg = segment(input);
        let shape = modes(&seg);
        assert_matches_reference(input);
        note!("{:<30} {:?}", preview(input), shape);
    }

    done!("all mixed inputs agree with the reference");
}

// ---------------------------------------------------------------------------
// terminator and bit padding
// ---------------------------------------------------------------------------

#[test]
fn terminator_and_bit_padding_are_zero() {
    checking!("terminator", "after the payload come four 0 bits, then 0 bits up to the next byte boundary");

    // 1 digit: 4 + 10 + 4 = 18 payload bits, + 4 terminator = 22, + 2 pad = 24
    let out = encode_input("1");
    note!("\"1\"  →  {}", bit_string(&out));
    assert_eq!(read_bits(&out, 0, 18), 0b0001_0000000001_0001, "payload");
    assert_eq!(read_bits(&out, 18, 6), 0, "terminator and bit padding must be zero");
    assert_eq!(out[3], 0xEC, "pad codewords start on the next byte");

    done!("bits 18..24 are zero, pad codewords begin at byte 3");
}

#[test]
fn terminator_is_truncated_when_the_symbol_is_nearly_full() {
    checking!("terminator", "with fewer than 4 bits left the terminator shrinks to fit, and no pad codewords follow");

    // 41 digits at 1-L: 4 + 10 + 13 × 10 + 7 = 151 bits of 152
    let input = "1".repeat(41);
    let seg = segment(&input);
    assert_eq!((seg.version().get(), *seg.ecc_level() as u8), (1, ECCLevel::L as u8));
    let out = encode_data(&input, seg);
    note!("41 digits at 1-L  →  {}", hex(&out));
    assert_eq!(out.len(), 19, "1-L holds 19 codewords");
    assert_eq!(out[18] & 1, 0, "the single remaining bit is the terminator");
    assert_matches_reference(&input);
    ok!("151 payload bits + 1 terminator bit, no 0xEC");

    done!("truncated terminator fills the last bit exactly");
}

#[test]
fn exactly_full_symbols_get_no_terminator_or_padding() {
    checking!("terminator", "a payload that fills the symbol to the last bit gets nothing appended");

    let kanji: String = std::iter::repeat_n(KANJI_CHAR, 1817).collect();
    for (label, input) in [
        ("7089 digits", "1".repeat(7089)),
        ("4296 alnum", "A".repeat(4296)),
        ("2953 bytes", "a".repeat(2953)),
        ("1817 kanji", kanji),
    ] {
        let seg = segment(&input);
        let capacity = capacity_bits(&seg);
        let payload = reference_payload(&input, &seg).len();
        let out = encode_data(&input, seg);
        assert_eq!(out.len() * 8, capacity, "{label}: {} codewords for a {capacity}-bit symbol", out.len());
        assert_ne!(*out.last().unwrap(), 0xEC, "{label}: no room for pad codewords");
        note!("{label:<12} {payload:>5} / {capacity} bits  →  {} codewords, terminator {} bits", out.len(), capacity - payload);
    }

    done!("40-L capacities for all four modes produce exactly 2956 codewords");
}

// ---------------------------------------------------------------------------
// output length
// ---------------------------------------------------------------------------

#[test]
fn output_length_equals_data_capacity() {
    checking!("length", "the encoder must emit exactly version_data_bits / 8 codewords, never one more or less");

    let mut lengths = vec![0, 1, 2, 3, 10, 17, 27, 34, 41, 42, 100, 255, 256, 500, 1000, 2000, 4000, 7089];
    lengths.dedup();
    for len in lengths {
        let input = "1".repeat(len);
        let seg = segment(&input);
        let expected = capacity_bits(&seg) / 8;
        let label = format!("v{}-{:?}", seg.version().get(), seg.ecc_level());
        let out = encode_data(&input, seg);
        assert_eq!(out.len(), expected, "{len} digits at {label}");
        note!("{len:>5} digits  →  {label:<7} {expected:>5} codewords");
    }

    done!("codeword count matches capacity across versions 1 to 40");
}

#[test]
fn pad_codewords_alternate_after_the_payload() {
    checking!("padding", "after the byte-aligned payload, codewords alternate 0xEC, 0x11 to the end");

    for input in ["1", "HELLO WORLD", "hello", "こ", "1".repeat(100).as_str()] {
        let seg = segment(input);
        let payload = reference_payload(input, &seg).len();
        let capacity = capacity_bits(&seg);
        let data_bytes = (payload + (capacity - payload).min(4)).div_ceil(8);
        let out = encode_data(input, seg);

        for (i, &b) in out.iter().enumerate().skip(data_bytes) {
            let expected = if (i - data_bytes) % 2 == 0 { 0xEC } else { 0x11 };
            assert_eq!(b, expected, "{}: codeword {i} is {b:#04x}, expected {expected:#04x}", preview(input));
        }
        note!("{:<26} {} data codeword(s), {} pad", preview(input), data_bytes, out.len() - data_bytes);
    }

    done!("pad sequence always starts with 0xEC right after the data");
}

// ---------------------------------------------------------------------------
// sweeps
// ---------------------------------------------------------------------------

#[test]
fn single_mode_lengths_match_reference() {
    checking!("sweep", "lengths 1..=60 in every mode, covering every grouping tail and pad parity");

    let kanji = |n: usize| std::iter::repeat_n(KANJI_CHAR, n).collect::<String>();
    let builders: [(&str, Box<dyn Fn(usize) -> String>); 4] = [
        ("numeric", Box::new(|n| "1234567890".chars().cycle().take(n).collect())),
        ("alphanumeric", Box::new(|n| "HELLO WORLD $%*+-./:".chars().cycle().take(n).collect())),
        ("byte", Box::new(|n| "hello, world!".chars().cycle().take(n).collect())),
        ("kanji", Box::new(kanji)),
    ];

    for (label, build) in &builders {
        let mut versions = std::collections::BTreeSet::new();
        for len in 1..=60 {
            let input = build(len);
            versions.insert(segment(&input).version().get());
            assert_matches_reference(&input);
        }
        note!("{label:<13} 60 lengths, versions {versions:?}");
    }

    done!("240 single-mode inputs agree with the reference");
}

#[test]
fn every_version_block_matches_reference() {
    checking!("sweep", "long inputs in every mode and block, including the 16-bit byte count");

    let kanji = |n: usize| std::iter::repeat_n(KANJI_CHAR, n).collect::<String>();
    let mixed = |n: usize| "abc123こんにちは456ABCDEF hello WORLD 9876543210".chars().cycle().take(n).collect::<String>();
    let cases: Vec<(&str, String)> = vec![
        ("numeric", "1".repeat(600)),
        ("numeric", "1".repeat(3400)),
        ("alphanumeric", "A".repeat(400)),
        ("alphanumeric", "A".repeat(3000)),
        ("byte", "a".repeat(400)),
        ("byte", "a".repeat(2000)),
        ("kanji", kanji(200)),
        ("kanji", kanji(1000)),
        ("mixed", mixed(300)),
        ("mixed", mixed(1500)),
    ];

    for (label, input) in &cases {
        let seg = segment(input);
        let v = seg.version().get();
        let ecc = *seg.ecc_level();
        let count = seg.mode_hint().len();
        assert_matches_reference(input);
        note!("{label:<13} {:>5} chars  →  v{v:<2} {ecc:?}  {count} segment(s)", input.chars().count());
    }

    done!("blocks 2 and 3 agree with the reference");
}

#[test]
fn segment_hint_bit_length_matches_the_encoded_payload() {
    checking!("contract", "the terminator is sized from data_bit_len, so the segmenter's figure must equal the real payload");

    let mixed = "abc123こんにちは456ABCDEF hello WORLD 9876543210";
    for input in ["1", "12", "HELLO WORLD", "hello", "こんにちは", "é🦀", mixed, &"1".repeat(41), &"a".repeat(2953)] {
        let seg = segment(input);
        let payload = reference_payload(input, &seg).len();
        assert_eq!(
            seg.data_bit_len() as usize,
            payload,
            "{}: segmenter says {} bits, payload is {payload}",
            preview(input),
            seg.data_bit_len()
        );
        note!("{:<28} {payload:>5} bits", preview(input));
    }

    done!("data_bit_len agrees with the bits actually written");
}

// ---------------------------------------------------------------------------
// edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_input_is_terminator_and_padding_only() {
    checking!("edge case", "no segments means no headers: a zero terminator byte, then pad codewords");

    let seg = segment("");
    note!("segments {:?} at {}-{:?}", modes(&seg), seg.version().get(), seg.ecc_level());
    let capacity = capacity_bits(&seg);
    let out = encode_data("", seg);
    note!("got      {}", hex(&out));

    assert_eq!(out.len(), capacity / 8);
    assert_eq!(out[0], 0x00, "first codeword is the terminator plus bit padding");
    assert!(out[1..].iter().step_by(2).all(|&b| b == 0xEC));
    assert!(out[2..].iter().step_by(2).all(|&b| b == 0x11));

    done!("empty input yields 00 EC 11 … to capacity");
}

#[test]
fn single_character_in_each_mode() {
    checking!("edge case", "a one-character segment exercises every short tail: 4-bit digit, 6-bit alnum, 1 byte, 1 kanji");

    for (input, expected) in [
        ("7", &[0x10, 0x05, 0xC0][..]),          // 0001 0000000001 0111 0000 00
        ("Z", &[0x20, 0x0C, 0x60][..]),          // 0010 000000001 100011 0000 0
        ("z", &[0x40, 0x17, 0xA0][..]),          // 0100 00000001 01111010 0000
        ("点", &[0x80, 0x16, 0xCF, 0x80][..]),    // 1000 00000001 0110110011111 0000 000
    ] {
        let out = encode_input(input);
        assert_codewords(input, &out[..expected.len()], expected);
        note!("{:<5} →  {}", format!("{input:?}"), hex(&out));
    }

    done!("all four single-char segments packed bit-exact");
}

#[test]
fn encoding_is_deterministic() {
    checking!("edge case", "the encoder keeps no state between calls");

    let input = "abc123こんにちは456ABCDEF";
    let first = encode_input(input);
    for _ in 0..5 {
        assert_eq!(encode_input(input), first);
    }

    done!("six runs, identical output");
}
