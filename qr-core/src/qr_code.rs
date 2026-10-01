use crate::lookups::alphanumeric_value;
use crate::optimal_segmentation::OptimalSegmentHint;
use crate::{QrError, lookups};
use crate::encoding;

/// A QR symbol version, guaranteed to be in 1..=40.
///
/// The only way to build one is [`Version::new`], so anything holding a
/// `Version` can index the version tables without re-checking the range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version(u8);

impl Version {
    pub const MIN: u8 = 1;
    pub const MAX: u8 = 40;

    pub fn new(version: u8) -> Result<Version, QrError> {
        if (Self::MIN..=Self::MAX).contains(&version) {
            Ok(Version(version))
        } else {
            Err(QrError::InvalidVersion(version))
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// Zero-based position of this version in the per-version tables.
    pub fn index(self) -> usize {
        (self.0 - 1) as usize
    }

    pub fn version_block(&self) -> u8 {
        match self.get() {
            1..=9 => 0,
            10..=26 => 1,
            _ => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Numeric = 0,
    Alphanumeric = 1,
    Byte = 2,
    Kanji = 3
}

impl Mode {
    pub fn numeric_bit_count(digits: u8) -> i8 {
        debug_assert!(digits <= 3, "a maximum of three digits can be grouped in numeric mode");
        match digits {
            3 => 10,
            2 => 7,
            1 => 4,
            _ => panic!("Invalid digit grouping. 1, 2 or 3 digits allowed in a numeric word.")
        }
    }

    pub fn alphanumeric_bit_count(chars: u8) -> i8 {
        debug_assert!(chars <= 2, "a maximum of two chars can be grouped in alphanumeric mode");
        match chars {
            2 => 11,
            1 => 6,
            _ => panic!("Invalid character grouping. 1 or 2 characters allowed in a alphanumeric word.")
        }
    }

    pub fn alphanumeric_value_conversion(chunk: &[char]) -> u16 {
        debug_assert!(chunk.len() <= 2, "a maximum of two chars can be grouped in alphanumeric mode");
        match chunk.len() {
            2 => {
                (alphanumeric_value(chunk[0]).unwrap() as u16 * 45) + alphanumeric_value(chunk[1]).unwrap() as u16
            },
            1 => {
                alphanumeric_value(chunk[1]).unwrap() as u16
            },
            _ => panic!("invalid alphanumeric chunk. maximm of two chars can be grouped in alphanumeric mode.")
        }
    }
}


#[derive(Debug, Clone, Copy)]
#[repr(u8)]
pub enum ECCLevel {
    L = 0,
    M = 1,
    Q = 2,
    H = 3
}

impl ECCLevel {
    pub fn raise(&mut self, offset: u8) {
        if offset > 3 || (*self as u8 + offset > 3) {
            *self = ECCLevel::H;
        } else {
            let mut level = *self;
            for _ in 0..offset {
                level = level.next();
            }
            *self = level;
        }
    }

    fn next(&self) -> ECCLevel {
        match self {
            ECCLevel::L => ECCLevel::M,
            ECCLevel::M => ECCLevel::Q,
            ECCLevel::Q => ECCLevel::H,
            ECCLevel::H => ECCLevel::H
        }
    }
}
pub struct Matrix {
    // single vector with access logics
    data_matrix: Vec<u16>,
    module_len: u16
}

impl Matrix {
    pub fn new(version: Version) -> Self {
        let side = lookups::version_dimension(version);
        let length = side * side;
        Matrix { data_matrix: vec![0; length as usize], module_len: side }
    }

    pub fn coordinates(&self, index: u16) -> (u8, u8) {
        ((index / self.module_len) as u8, (index % self.module_len) as u8)
    }

    pub fn index(&self, x: u8, y: u8) -> u16 {
        (x as u16 * self.module_len) + y as u16 
    }
}

pub fn text_qr(input: &str) -> Result<Matrix, QrError> {
    let mut segments = OptimalSegmentHint::create_segmentation(input)?;
    let mut encode = encoding::encode_data(input, segments);
    return Ok(Matrix::new(Version::new(8).unwrap()));
}

// pub fn binary_qr(input: &[u8]) -> Result<Matrix, QrError> {
    
// }