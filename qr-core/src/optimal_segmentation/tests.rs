//! Unit tests for the optimal segmentation DP.
//!
//! These print a running commentary of what they check. To see it:
//!
//! ```text
//! cargo test -p qr-core -- --nocapture --test-threads=1
//! ```
//!
//! (`--test-threads=1` keeps the output of separate tests from interleaving.)

use super::*;

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

fn describe(seg: &OptimalSegment, input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let body = seg
        .mode_hint
        .iter()
        .map(|s| {
            let text: String = chars[s.start..=s.end].iter().collect();
            format!("{:?}[{}..={}]={}", s.mode, s.start, s.end, preview(&text))
        })
        .collect::<Vec<_>>()
        .join("  ");
    if body.is_empty() { "<no segments>".to_string() } else { body }
}

fn report(input: &str, seg: &OptimalSegment) {
    note!("input    {}", preview(input));
    note!("picked   version {} at ECC {:?}", seg.version, seg.ecc_level);
    if seg.mode_hint.is_empty() {
        note!("segments <none>");
    } else {
        let chars: Vec<char> = input.chars().collect();
        note!("segments {}", seg.mode_hint.len());
        for s in &seg.mode_hint {
            let text: String = chars[s.start..=s.end].iter().collect();
            note!(
                "   [{:>4} ..={:>4}]  {:<13} {}",
                s.start,
                s.end,
                format!("{:?}", s.mode),
                preview(&text)
            );
        }
    }
}

// ---------------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------------

fn segment(input: &str, ecc: ECCLevel) -> OptimalSegment {
    OptimalSegment::create_segmentation(input, ecc)
        .unwrap_or_else(|e| panic!("expected {input:?} to be encodable, got {e:?}"))
}

/// Every character of `input` must belong to exactly one segment, the segments
/// must be in reading order, and each character must be representable in the
/// mode its segment claims.
fn assert_partitions(input: &str, seg: &OptimalSegment) {
    let chars: Vec<char> = input.chars().collect();
    let mut cursor = 0;

    for s in &seg.mode_hint {
        assert!(s.end >= s.start, "empty or inverted segment {s:?} in {input:?}");
        assert_eq!(s.start, cursor, "segment {s:?} does not start where the previous one ended in {input:?}");
        for &c in &chars[s.start..=s.end] {
            assert!(
                char_cost(c, s.mode).is_some(),
                "{c:?} is not representable in {:?} but was placed in {s:?}",
                s.mode
            );
        }
        cursor = s.end + 1;
    }

    assert_eq!(cursor, chars.len(), "segments do not cover all of {input:?}");
    ok!(
        "{} segment(s) tile all {} char(s), no gaps or overlaps, every char legal in its mode",
        seg.mode_hint.len(),
        chars.len()
    );
}

fn modes(seg: &OptimalSegment) -> Vec<Mode> {
    seg.mode_hint.iter().map(|s| s.mode).collect()
}

fn bounds(seg: &OptimalSegment) -> Vec<(usize, usize)> {
    seg.mode_hint.iter().map(|s| (s.start, s.end)).collect()
}

/// A kanji-mode character (Shift-JIS double byte), 3 bytes in UTF-8.
const KANJI_CHAR: char = 'こ';

// ---------------------------------------------------------------------------
// char_cost
// ---------------------------------------------------------------------------

#[test]
fn numeric_accepts_digits_only() {
    checking!("char_cost", "numeric mode takes digits at 20 sixths (10/3 bits) and nothing else");

    for c in '0'..='9' {
        assert_eq!(char_cost(c, Mode::Numeric), Some(20), "digit {c:?}");
    }
    ok!("'0'..'9' all cost 20 sixths");

    let rejected = ['A', 'a', ' ', '$', KANJI_CHAR, '\u{0}', '\u{7f}'];
    for c in rejected {
        assert_eq!(char_cost(c, Mode::Numeric), None, "non-digit {c:?}");
    }
    ok!("{} non-digits rejected: {:?}", rejected.len(), rejected);

    done!("10 accepted, {} rejected", rejected.len());
}

#[test]
fn alphanumeric_accepts_the_45_char_charset() {
    checking!("char_cost", "alphanumeric mode takes exactly the 45-char QR charset at 33 sixths (11/2 bits)");

    let charset: Vec<char> = "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ $%*+-./:".chars().collect();
    assert_eq!(charset.len(), 45, "the QR alphanumeric charset has 45 entries");
    note!("charset  {}", charset.iter().collect::<String>());

    for &c in &charset {
        assert_eq!(char_cost(c, Mode::Alphanumeric), Some(33), "charset member {c:?}");
    }
    ok!("all 45 charset members cost 33 sixths");

    let mut rejected = 0;
    for c in (0u8..128).map(char::from) {
        if !charset.contains(&c) {
            assert_eq!(char_cost(c, Mode::Alphanumeric), None, "non-member {c:?}");
            rejected += 1;
        }
    }
    assert_eq!(char_cost(KANJI_CHAR, Mode::Alphanumeric), None);
    ok!("the other {rejected} ASCII chars rejected, plus {KANJI_CHAR:?}");

    done!("45 accepted, {} rejected", rejected + 1);
}

#[test]
fn byte_cost_scales_with_utf8_length() {
    checking!("char_cost", "byte mode charges 48 sixths (8 bits) per UTF-8 byte");

    let cases = [('a', 1, 48), ('0', 1, 48), ('é', 2, 96), (KANJI_CHAR, 3, 144), ('🦀', 4, 192)];
    for (c, bytes, cost) in cases {
        assert_eq!(char_cost(c, Mode::Byte), Some(cost), "{c:?}");
        note!("{c:?}  {bytes} UTF-8 byte(s)  →  {cost} sixths ({} bits)", cost / 6);
    }

    done!("cost tracks UTF-8 length across {} cases", cases.len());
}

#[test]
fn kanji_accepts_only_shift_jis_double_bytes() {
    checking!("char_cost", "kanji mode takes Shift-JIS double bytes at 78 sixths (13 bits)");

    for c in [KANJI_CHAR, '世'] {
        assert_eq!(char_cost(c, Mode::Kanji), Some(78), "{c:?}");
        note!("{c:?}  →  78 sixths (13 bits)");
    }

    let rejected = ['a', 'A', '0', ' ', '🦀'];
    for c in rejected {
        assert_eq!(char_cost(c, Mode::Kanji), None, "non-kanji {c:?}");
    }
    ok!("rejected {rejected:?} — note '🦀' is outside the BMP so the table cannot hold it");

    done!("2 accepted, {} rejected", rejected.len());
}

#[test]
fn every_char_is_representable_in_byte_mode() {
    checking!("char_cost", "byte mode must never be a dead end, or a DP column goes unreachable and min_seal overflows on u32::MAX");

    let cases = ['a', '0', 'A', ' ', KANJI_CHAR, '🦀', '\u{0}', '\u{10ffff}'];
    for c in cases {
        assert!(char_cost(c, Mode::Byte).is_some(), "byte mode rejected {c:?}");
    }
    ok!("all {} probes accepted, including U+0000 and U+10FFFF", cases.len());

    done!("byte mode is total, so every column keeps at least one finite cell");
}

// ---------------------------------------------------------------------------
// min_seal / min_mode
// ---------------------------------------------------------------------------

fn column(costs: [u32; 4]) -> [Cell; 4] {
    [
        Cell::new(Mode::Numeric, costs[0], Mode::Numeric),
        Cell::new(Mode::Alphanumeric, costs[1], Mode::Alphanumeric),
        Cell::new(Mode::Byte, costs[2], Mode::Byte),
        Cell::new(Mode::Kanji, costs[3], Mode::Kanji),
    ]
}

#[test]
fn min_seal_picks_the_cheapest_mode() {
    checking!("min_seal", "sealing a column follows the cheapest mode in it");

    let costs = [120, 60, 300, 240];
    note!("column   Numeric {} · Alphanumeric {} · Byte {} · Kanji {}", costs[0], costs[1], costs[2], costs[3]);
    let sealed = min_seal(&column(costs));
    assert_eq!(sealed.mode, Mode::Alphanumeric);
    assert_eq!(sealed.cost, 60);

    done!("sealed to {:?} at {} sixths", sealed.mode, sealed.cost);
}

#[test]
fn min_seal_rounds_up_to_a_whole_bit() {
    checking!("min_seal", "a segment boundary pads to a whole bit, so a partial sixth rounds up");

    let sealed = min_seal(&column([61, 200, 200, 200]));
    assert_eq!(sealed.cost, 66, "61 sixths should seal to 11 bits");
    ok!("61 sixths (10 1/6 bits)  →  66 sixths (11 bits), padded");

    let sealed = min_seal(&column([60, 200, 200, 200]));
    assert_eq!(sealed.cost, 60, "10 whole bits should not be padded");
    ok!("60 sixths (10 bits)      →  60 sixths (10 bits), already aligned");

    done!("rounds up only when the cost is off a bit boundary");
}

#[test]
fn min_mode_returns_the_index_of_the_cheapest_cell() {
    checking!("min_mode", "the finishing mode is the cheapest cell of the last column");

    for (costs, expected) in [
        ([10, 20, 30, 40], 0),
        ([40, 30, 20, 10], 3),
        ([u32::MAX, u32::MAX, 5, u32::MAX], 2),
    ] {
        assert_eq!(min_mode(&column(costs)), expected);
        note!("{costs:?}  →  index {expected}");
    }

    done!("cheapest index found, unreachable u32::MAX cells skipped");
}

// ---------------------------------------------------------------------------
// single-mode segmentation
// ---------------------------------------------------------------------------

#[test]
fn pure_numeric_is_one_numeric_segment() {
    checking!("segmentation", "an all-digit string stays in one numeric segment");

    let input = "12345678";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Numeric]);
    assert_eq!(bounds(&seg), vec![(0, 7)]);
    assert_partitions(input, &seg);

    done!("one numeric segment, no spurious mode switches");
}

#[test]
fn pure_alphanumeric_is_one_alphanumeric_segment() {
    checking!("segmentation", "uppercase and space stay in one alphanumeric segment");

    let input = "HELLO WORLD";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric]);
    assert_eq!(bounds(&seg), vec![(0, 10)]);
    assert_partitions(input, &seg);

    done!("one alphanumeric segment covering all 11 chars");
}

#[test]
fn lowercase_falls_back_to_byte() {
    checking!("segmentation", "lowercase is outside the alphanumeric charset, so it must fall back to byte");

    let input = "hello";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Byte]);
    assert_eq!(bounds(&seg), vec![(0, 4)]);

    done!("fell back to byte mode as expected");
}

#[test]
fn pure_kanji_is_one_kanji_segment() {
    checking!("segmentation", "kanji at 13 bits beats byte at 24 bits per char, so it wins outright");

    let input = "こんにちは世界";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Kanji]);
    assert_eq!(bounds(&seg), vec![(0, 6)]);
    assert_partitions(input, &seg);

    done!("kanji mode chosen over byte, 13 bits/char vs 24");
}

#[test]
fn non_bmp_chars_use_byte_mode() {
    checking!("segmentation", "chars outside the BMP have no kanji entry and must land in byte mode");

    let input = "🦀🦀";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Byte]);
    assert_partitions(input, &seg);

    done!("4-byte chars handled without falling out of the DP");
}

#[test]
fn single_character_inputs() {
    checking!("segmentation", "a one-character input must produce one segment spanning [0..=0]");

    for (input, expected) in [
        ("1", Mode::Numeric),
        ("A", Mode::Alphanumeric),
        ("a", Mode::Byte),
        ("🦀", Mode::Byte),
    ] {
        let seg = segment(input, ECCLevel::L);
        assert_eq!(modes(&seg), vec![expected], "input {input:?}");
        assert_eq!(bounds(&seg), vec![(0, 0)], "input {input:?}");
        note!("{:<6}  →  {:?}[0..=0]  version {}", format!("{input:?}"), expected, seg.version);
    }

    done!("4 single-char inputs, each one segment at [0..=0]");
}

// ---------------------------------------------------------------------------
// mixed-mode segmentation
// ---------------------------------------------------------------------------

#[test]
fn mixed_input_splits_on_exact_boundaries() {
    checking!("segmentation", "a three-mode string splits on the exact character where the mode changes");

    let input = "AAAAAAAAAAAAAA12345678aaaaaaaaaaaa";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    note!("expected [0..=13] [14..=21] [22..=33] — an off-by-one here means segments overlap");
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric, Mode::Numeric, Mode::Byte]);
    assert_eq!(bounds(&seg), vec![(0, 13), (14, 21), (22, 33)]);
    assert_partitions(input, &seg);

    done!("boundaries land exactly on the mode changes");
}

#[test]
fn segments_are_returned_in_reading_order() {
    checking!("segmentation", "backtracking collects segments last-to-first, so the result must be reversed");

    let input = "AAAAAAAAAAAAAA12345678aaaaaaaaaaaa";
    let seg = segment(input, ECCLevel::L);
    let starts: Vec<usize> = seg.mode_hint.iter().map(|s| s.start).collect();
    note!("starts   {starts:?}");
    let mut sorted = starts.clone();
    sorted.sort();
    assert_eq!(starts, sorted, "segments must not come back reversed");
    assert_eq!(seg.mode_hint[0].start, 0, "first segment must start at char 0");

    done!("starts ascend from 0 — encoder can consume them directly");
}

#[test]
fn four_way_mixed_input_partitions_cleanly() {
    checking!("segmentation", "all four modes in one string still tile the input exactly");

    let input = "abc123こんにちは456ABCDEF";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_partitions(input, &seg);
    assert!(seg.mode_hint.len() > 1, "expected several segments, got {:?}", modes(&seg));

    done!("{} segments across {} modes", seg.mode_hint.len(), {
        let mut m = modes(&seg);
        m.dedup();
        m.len()
    });
}

#[test]
fn short_digit_run_stays_inside_an_alphanumeric_segment() {
    checking!("segmentation", "switching costs a mode + count indicator, more than a 2-digit run can save");

    let input = "ABCDEFGHIJ12ABCDEFGHIJ";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    note!("a 2-digit run saves ~3 bits but a switch costs 13+ — staying put is cheaper");
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric], "a 2-digit run should not be split out");
    assert_partitions(input, &seg);

    done!("run absorbed into the surrounding alphanumeric segment");
}

#[test]
fn long_digit_run_is_split_out_of_an_alphanumeric_segment() {
    checking!("segmentation", "a long enough digit run does pay for its own segment");

    let input = "ABCDEFGHIJ1234567890123456789012345678901234567890ABCDEFGHIJ";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    note!("a 40-digit run saves ~60 bits, comfortably more than the switch costs");
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric, Mode::Numeric, Mode::Alphanumeric]);
    assert_eq!(bounds(&seg), vec![(0, 9), (10, 49), (50, 59)]);
    assert_partitions(input, &seg);

    done!("run split out into its own numeric segment");
}

#[test]
fn many_alternating_runs_still_partition() {
    checking!("segmentation", "repeated mode changes must not drift the cursors out of alignment");

    let input = "1234567890abcdefghij1234567890ABCDEFGHIJ1234567890こんにちは世界1234567890";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_partitions(input, &seg);

    done!("{} alternating runs traced back without drift", seg.mode_hint.len());
}

#[test]
fn partitions_hold_across_every_ecc_level() {
    checking!("segmentation", "the same string must partition cleanly at all four ECC levels");

    let input = "abc123こんにちは456ABCDEF hello WORLD 9876543210";
    note!("input    {}", preview(input));
    for ecc in [ECCLevel::L, ECCLevel::M, ECCLevel::Q, ECCLevel::H] {
        let seg = segment(input, ecc);
        note!("{:?}  →  version {:<2}  {}", ecc, seg.version, describe(&seg, input));
        assert_partitions(input, &seg);
    }

    done!("L, M, Q and H all partition the input correctly");
}

// ---------------------------------------------------------------------------
// version selection
// ---------------------------------------------------------------------------

#[test]
fn smallest_fitting_version_is_chosen() {
    checking!("version", "the smallest symbol that fits must win, never a larger one");

    for input in ["1", "HELLO WORLD"] {
        let seg = segment(input, ECCLevel::L);
        assert_eq!(seg.version, 1, "input {input:?}");
        note!("{:<14}  →  version {}", format!("{input:?}"), seg.version);
    }

    done!("both short payloads fit version 1");
}

#[test]
fn a_higher_ecc_level_needs_a_higher_version() {
    checking!("version", "ECC steals data capacity, so the same payload needs a bigger symbol at H");

    let low = segment(&"1".repeat(500), ECCLevel::L).version;
    let high = segment(&"1".repeat(500), ECCLevel::H).version;
    note!("500 digits  →  L: version {low}   H: version {high}");
    assert!(high > low, "H ({high}) should need a larger symbol than L ({low})");

    done!("H needs {} more version(s) than L", high - low);
}

#[test]
fn version_grows_monotonically_with_input_length() {
    checking!("version", "a longer payload must never select a smaller symbol");

    let mut previous = 0;
    for len in [1, 50, 200, 500, 1000, 2000, 4000, 7089] {
        let version = segment(&"1".repeat(len), ECCLevel::L).version;
        assert!(version >= previous, "{len} digits picked v{version} after v{previous}");
        assert!((1..=40).contains(&version), "v{version} is out of range");
        note!("{len:>5} digits  →  version {version}");
        previous = version;
    }

    done!("versions ascend across 8 lengths, all within 1..=40");
}

#[test]
fn versions_are_found_in_every_version_block() {
    checking!("version", "the DP re-runs per block because the count indicator widens at v10 and v27");

    let small = segment(&"1".repeat(10), ECCLevel::L).version;
    let medium = segment(&"1".repeat(600), ECCLevel::L).version;
    let large = segment(&"1".repeat(3400), ECCLevel::L).version;

    note!("   10 digits  →  version {small:<2}  (block 1, versions 1-9)");
    note!("  600 digits  →  version {medium:<2}  (block 2, versions 10-26)");
    note!(" 3400 digits  →  version {large:<2}  (block 3, versions 27-40)");

    assert!((1..=9).contains(&small), "expected block 1, got v{small}");
    assert!((10..=26).contains(&medium), "expected block 2, got v{medium}");
    assert!((27..=40).contains(&large), "expected block 3, got v{large}");

    done!("all three count-indicator blocks reachable");
}

// ---------------------------------------------------------------------------
// capacity limits (ISO/IEC 18004 table 7, 40-L)
// ---------------------------------------------------------------------------

/// Asserts the documented 40-L character capacity for a mode: `n` fits, `n + 1` does not.
fn assert_capacity_boundary(label: &str, max: usize, build: impl Fn(usize) -> String) {
    let fits = build(max);
    let seg = segment(&fits, ECCLevel::L);
    assert_eq!(seg.version, 40, "{label}: {max} chars should fill version 40");
    note!("{label:<13} {max:>5} chars  →  version {}  (fits)", seg.version);

    let over = build(max + 1);
    let result = OptimalSegment::create_segmentation(&over, ECCLevel::L);
    assert!(result.is_err(), "{label}: {} chars should not fit", max + 1);
    note!("{label:<13} {:>5} chars  →  InputTooLong  (rejected)", max + 1);
}

#[test]
fn numeric_capacity_boundary() {
    checking!("capacity", "40-L holds exactly 7089 digits per ISO/IEC 18004 table 7");
    assert_capacity_boundary("numeric", 7089, |n| "1".repeat(n));
    done!("boundary is exact at 7089/7090");
}

#[test]
fn alphanumeric_capacity_boundary() {
    checking!("capacity", "40-L holds exactly 4296 alphanumeric chars per ISO/IEC 18004 table 7");
    assert_capacity_boundary("alphanumeric", 4296, |n| "A".repeat(n));
    done!("boundary is exact at 4296/4297");
}

#[test]
fn byte_capacity_boundary() {
    checking!("capacity", "40-L holds exactly 2953 bytes per ISO/IEC 18004 table 7");
    assert_capacity_boundary("byte", 2953, |n| "a".repeat(n));
    done!("boundary is exact at 2953/2954");
}

#[test]
fn kanji_capacity_boundary() {
    checking!("capacity", "40-L holds exactly 1817 kanji per ISO/IEC 18004 table 7");
    assert_capacity_boundary("kanji", 1817, |n| std::iter::repeat_n(KANJI_CHAR, n).collect());
    done!("boundary is exact at 1817/1818");
}

#[test]
fn over_capacity_reports_input_too_long() {
    checking!("capacity", "a payload past every version reports InputTooLong rather than panicking");

    let err = OptimalSegment::create_segmentation(&"1".repeat(20000), ECCLevel::L).unwrap_err();
    note!("20000 digits  →  {err:?} (\"{err}\")");
    assert!(matches!(err, QrError::InputTooLong), "got {err:?}");

    done!("all three version blocks exhausted, error returned cleanly");
}

#[test]
fn high_ecc_capacity_is_lower_than_low_ecc() {
    checking!("capacity", "a payload that exactly fills 40-L must overflow 40-H");

    let result = OptimalSegment::create_segmentation(&"1".repeat(7089), ECCLevel::H);
    note!("7089 digits  →  L: version 40   H: {}", if result.is_err() { "InputTooLong" } else { "fits (wrong)" });
    assert!(result.is_err());

    done!("ECC level correctly reduces usable capacity");
}

// ---------------------------------------------------------------------------
// edge cases
// ---------------------------------------------------------------------------

#[test]
fn empty_input_does_not_panic() {
    checking!("edge case", "an empty string skips the backtracking loop entirely, where the final push used to underflow");

    let seg = segment("", ECCLevel::L);
    report("", &seg);
    assert!(seg.mode_hint.is_empty(), "empty input should produce no segments, got {:?}", modes(&seg));

    done!("returned no segments instead of panicking on 0 - 1");
}

#[test]
fn backtracking_never_underflows_on_short_inputs() {
    checking!("edge case", "the final push subtracts from both cursors, so short inputs expose off-by-ones first");

    for len in 1..8 {
        for (label, input) in [("digits", "1".repeat(len)), ("bytes", "a".repeat(len))] {
            let seg = segment(&input, ECCLevel::L);
            note!("{len} {label:<7}  →  {}", describe(&seg, &input));
            assert_partitions(&input, &seg);
        }
    }

    done!("lengths 1 through 7 traced back cleanly in two modes");
}

#[test]
fn mode_switch_on_the_second_character_is_handled() {
    checking!("edge case", "a switch recorded at column 1 would drive end_cursor to 0 in the final push");

    for input in ["1a", "a1", "A1", "1A", "aA", "1こ", "こ1"] {
        let seg = segment(input, ECCLevel::L);
        note!("{:<8}  →  {}", format!("{input:?}"), describe(&seg, input));
        assert_partitions(input, &seg);
    }

    done!("7 two-char inputs handled without underflow");
}

#[test]
fn whitespace_and_punctuation_are_placed_correctly() {
    checking!("segmentation", "punctuation splits along the alphanumeric charset boundary");

    let input = "HELLO $%*+-./: WORLD";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Alphanumeric], "all of these are in the alphanumeric charset");
    ok!("$ % * + - . / : and space are all charset members");

    let input = "hello, world!";
    let seg = segment(input, ECCLevel::L);
    report(input, &seg);
    assert_eq!(modes(&seg), vec![Mode::Byte], "comma and bang are byte-only");
    ok!("',' and '!' are not, forcing byte mode");

    done!("charset membership drives the split correctly");
}
