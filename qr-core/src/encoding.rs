use crate::lookups;
use crate::optimal_segmentation::{OptimalSegmentHint, Segment}; 
use crate::qr_code::{Mode, Version};

const PAD_BYTES: [u8; 2] = [0b1110_1100, 0b0001_0001];

pub fn encode_data(input: &str, segment_hint: OptimalSegmentHint) -> Vec<u8> {
    let chars = input.chars().collect::<Vec<char>>();
    let version_block = segment_hint.version().version_block();
    let mut lb_offset = 0;                                 // Last byte offset
    let mut encoded = vec![];
    
    for segment in segment_hint.mode_hint() {
        add_header(&chars, &segment_hint.version(), &segment, &mut lb_offset, &mut encoded);
        add_bytes(&chars[segment.start()..=segment.end()], &segment.mode(), &mut lb_offset, &mut encoded);
    }
    termination_zeros(&mut lb_offset, &mut encoded, &segment_hint);
    pad_bytes(&mut encoded, lookups::version_data_bits(*segment_hint.version(), *segment_hint.ecc_level()));
    return encoded;
}

fn add_header(chars: &Vec<char>, version: &Version, segment: &Segment, lb_offset: &mut u8, encoded: &mut Vec<u8>) {
    let mode_byte = lookups::MODE_INDICATOR[segment.mode() as usize];
    encode(lb_offset, encoded, mode_byte as u16, 4);                            // mode encode
    let cci_count = lookups::MODE_CCI_LEN[version.version_block() as usize][segment.mode() as usize];
    let cc = segment_cc(chars, segment);
    encode(lb_offset, encoded, cc as u16, cci_count as i8);                     // char count encode
}

fn segment_cc(chars: &Vec<char>, segment: &Segment) -> u16 {
    match segment.mode() {
        Mode::Byte => {
            let mut byte_count = 0;
            for c in &chars[segment.start()..=segment.end()]  {
                byte_count += c.len_utf8();
            }
            return byte_count as u16;
        }, 
        _ => (segment.end() - segment.start()) as u16 + 1
    }
}

fn add_bytes(chars: &[char], mode: &Mode, lb_offset: &mut u8, encoded: &mut Vec<u8>) {
    match mode {
        Mode::Numeric => {
            chars.chunks(3)
                .for_each(|chunk| {
                    let chunk_val = chunk.iter().collect::<String>().parse::<u16>().unwrap();
                    encode(lb_offset, encoded, chunk_val, Mode::numeric_bit_count(chunk.len() as u8));
                });
        },
        Mode::Alphanumeric => {
            chars.chunks(2)
                .for_each(|chunk| {
                    let chunk_val = Mode::alphanumeric_value_conversion(chunk);
                    encode(lb_offset, encoded, chunk_val, Mode::alphanumeric_bit_count(chunk.len() as u8));
                });
        },
        Mode::Byte => {
            chars.iter()
                .collect::<String>()
                .as_bytes()
                .iter()
                .for_each(|b| encode(lb_offset, encoded, *b as u16, 8));
        },
        Mode::Kanji => {
            chars.iter()
                .for_each(|c| {
                    encode(lb_offset, encoded, lookups::kanji_value(*c).unwrap(), 13);
                });
        },
    }
}

fn encode(offset: &mut u8, encoded: &mut Vec<u8>, mut value: u16, mut bit_count: i8) {
    debug_assert!(*offset < 8, "offset should be in [0, 7]");
    while bit_count > 0 {
        let last_byte = if *offset == 0 {
            let val = encoded.push_mut(0b0000_0000);
            *offset = 8;
            val
        } else {
            encoded.last_mut().unwrap()
        };

        if bit_count.cast_unsigned() <= *offset {
            let shift = *offset - bit_count.cast_unsigned();
            *last_byte |= (value << shift) as u8;
            *offset = shift;
            bit_count = 0;
        } else {
            let shift = bit_count.cast_unsigned() - *offset;
            *last_byte |= (value >> shift) as u8;
            bit_count -= *offset as i8;
            *offset = 0;
        }
    }
}


fn termination_zeros(lb_offset: &mut u8, encoded: &mut Vec<u8>, segment_hint: &OptimalSegmentHint) {
    let mut zero_count = 0;
    let encoded_bit_len = ((encoded.len() * 8) as u16) - (*lb_offset as u16);
    let version_bits = lookups::version_data_bits(*segment_hint.version(), *segment_hint.ecc_level());
    debug_assert!(encoded_bit_len <= version_bits, "payload of {encoded_bit_len} bits overflows {version_bits}-bit symbol");
    if encoded_bit_len < version_bits {
    let overshoot = version_bits - encoded_bit_len;
        zero_count = if overshoot > 4 { 4 } else { overshoot };
    }

    encode(lb_offset, encoded, 0b000_0000, zero_count as i8);
    *lb_offset = 0;
}

fn pad_bytes(encoded: &mut Vec<u8>, version_bits: u16) {
    let version_bytes = (version_bits / 8) as usize;
    let mut pad_byte_index: u8 = 0;
    while encoded.len() < version_bytes {
        encoded.push(PAD_BYTES[pad_byte_index as usize]);
        pad_byte_index = (pad_byte_index + 1) % 2;
    }
}

#[cfg(test)]
#[path ="./tests/encoding.rs"]
mod tests;