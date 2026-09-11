use std::collections::VecDeque;
use std::u32;

use crate::errors::QrError;
use crate::encoding::{ECCLevel, Mode};
use crate::lookups;

static MODE_CCI: [[u32; 4]; 3] = [ 
                                    [10,  9,  8,  8], 
                                    [12, 11, 16, 10],
                                    [14, 13, 16, 12]
                                ];

static VERSION_BLOCKS: [[u8; 2]; 3] = [[1, 9], [10, 26], [27, 40]];

static MODE_INDICATOR: u8 = 4;

#[derive(Debug, Clone, Copy)]
struct Cell {
    mode: Mode,
    cost: u32,
    switched_from: Mode
}

impl Cell {
    pub fn new(mode: Mode, cost: u32, switched_from: Mode) -> Cell {
        Cell { mode, cost, switched_from }
    }
}

#[derive(Debug, Clone, Copy)]
struct Segment {
    mode: Mode,
    start: usize,
    end: usize
}

#[derive(Debug)]
struct OptimalSegment {
    mode_hint: Vec<Segment>,
    version: u8,
    ecc_level: ECCLevel
}

impl OptimalSegment {
    fn new(version: u8, ecc_level: ECCLevel) -> Self {
        OptimalSegment { mode_hint: Vec::new(), version, ecc_level }
    }

    fn create_segmentation(input: &str, ecc_level: ECCLevel) -> Result<OptimalSegment, QrError> {
        let optimal_segment = execute_dp(input, 0, ecc_level)
            .or_else(|| execute_dp(input, 1, ecc_level))
            .or_else(|| execute_dp(input, 2, ecc_level));

        match optimal_segment {
            Some(ots) => Ok(ots),
            None => Err(QrError::InputTooLong)
        }
    }
}

fn first_column() -> [Cell; 4] {
    return [
        Cell {
            mode: Mode::Numeric,
            cost: u32::MAX,
            switched_from: Mode::Numeric
        },
        Cell {
            mode: Mode::Alphanumeric,
            cost: u32::MAX,
            switched_from: Mode::Alphanumeric
        },
        Cell {
            mode: Mode::Byte,
            cost: u32::MAX,
            switched_from: Mode::Byte
        },
        Cell {
            mode: Mode::Kanji,
            cost: u32::MAX,
            switched_from: Mode::Kanji
        }];
}

fn execute_dp(input: &str, version_block: usize, ecc_level: ECCLevel) -> Option<OptimalSegment> {
    let version = first_version(version_block);
    let dp_column = first_column();
    let input_size = input.chars().count();
    let chars = input.chars().collect::<Vec<char>>();

    let mut dp_table = vec![dp_column; input_size + 1];
    
    dp_table[0][0].cost = (MODE_INDICATOR as u32 + MODE_CCI[version_block][0]) * 6;
    dp_table[0][1].cost = (MODE_INDICATOR as u32 + MODE_CCI[version_block][1]) * 6;
    dp_table[0][2].cost = (MODE_INDICATOR as u32 + MODE_CCI[version_block][2]) * 6;
    dp_table[0][3].cost = (MODE_INDICATOR as u32 + MODE_CCI[version_block][3]) * 6;

    // iterate over the cells i
    for i in 1..=input_size {
        // winnin strategy compuation for each cell in this column
        let min_seal = min_seal(&dp_table[i - 1]);
        
        for j in 0..4 {
            let char_cost = char_cost(chars[i - 1], dp_table[i][j].mode);
            if char_cost.is_none() {
                continue;
            }

            let seal_cost = (min_seal.cost) + ((MODE_INDICATOR as u32 + MODE_CCI[version_block][j]) * 6);
            let extend_cost = dp_table[i - 1][j].cost;

            if seal_cost < extend_cost {
                // gotta switch
                dp_table[i][j].cost = seal_cost + char_cost.unwrap();
                dp_table[i][j].switched_from = min_seal.mode;
            } else {
                // continue
                dp_table[i][j].cost = extend_cost + char_cost.unwrap();
                dp_table[i][j].switched_from = dp_table[i - 1][j].mode;
            }
        }
    }

    let min_index = min_mode(&dp_table[input_size]);
    let min_cost = dp_table[input_size][min_index].cost.div_ceil(6);
    
    // TODO: implement automatic ecc level upgrade if ecc level is not specified 
    let mut optimal_segmentation = OptimalSegment::new(version, ecc_level);
    
    // check if version can be fit here.
    let min_ver = VERSION_BLOCKS[version_block][0];
    let max_ver = VERSION_BLOCKS[version_block][1];

    let lowest_ver = (min_ver..=max_ver)
        .find(|version| min_cost <= lookups::version_data_bits(*version as usize, optimal_segmentation.ecc_level).unwrap() as u32);
    
    if lowest_ver.is_none() {
        return None;
    }
    optimal_segmentation.version = lowest_ver.unwrap();
    optimal_segmentation.mode_hint = construct_optimal_segment(&dp_table, input_size, min_index);

    return Some(optimal_segmentation);
}

fn construct_optimal_segment(dp_table: &Vec<[Cell; 4]>, input_size: usize, min_index: usize) -> Vec<Segment> {
    // nothing to trace back through, and the final push below would underflow
    if input_size == 0 {
        return Vec::new();
    }

    let mut opt_seg = VecDeque::with_capacity(input_size);
    let mut current_index = min_index;
    let mut start_cursor = input_size;
    // `end_cursor` is one past the last character of the segment being traced.
    let mut end_cursor = input_size;
    while start_cursor > 0 {
        let cell = dp_table[start_cursor][current_index];
        // a cell that came from a different mode is where this segment begins
        if cell.switched_from != cell.mode {
            opt_seg.push_front(Segment {
                mode: cell.mode,
                start: start_cursor - 1,
                end: end_cursor - 1
            });

            // the previous segment ends on the character before this one
            end_cursor = start_cursor - 1;
            current_index = cell.switched_from.mode_code() as usize;
        }
        start_cursor -= 1;
    }
    opt_seg.push_front(Segment {
        mode: dp_table[0][current_index].mode,
        start: 0,
        end: end_cursor - 1
    });

    // backtracking collects segments last to first
    opt_seg.iter().cloned().collect::<Vec<Segment>>()
}

fn min_mode(column: &[Cell; 4]) -> usize {
    return column.iter()
        .enumerate().map(|(i, cell)| {
            return (i, cell);
        })
        .min_by_key(|(_, cell)| cell.cost)
        .map(|(i, _)| i)
        .unwrap();
}

/// Cost of encoding `c` in `mode`, in sixths of a bit (the unit the whole DP works in).
/// Returns `None` when the character is not representable in that mode.
fn char_cost(c: char, mode: Mode) -> Option<u32> {
    match mode {
        Mode::Numeric => c.is_ascii_digit().then_some(20),
        Mode::Alphanumeric => lookups::alphanumeric_value(c).map(|_| 33),
        Mode::Byte => Some(c.len_utf8() as u32 * 48),
        Mode::Kanji => lookups::kanji_value(c).map(|_| 78),
    }
}

fn min_seal(column: &[Cell; 4]) -> Cell {
    let min_cell = column.iter().min_by_key(| cell | cell.cost.div_ceil(6)).unwrap();
    Cell { mode: min_cell.mode, cost: min_cell.cost.div_ceil(6) * 6, switched_from: min_cell.switched_from }
}

fn first_version(version_block: usize) -> u8 {
    VERSION_BLOCKS[version_block][0]
}

#[cfg(test)]
mod tests;
