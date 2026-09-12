use crate::errors::QrError;

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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum Mode {
    Numeric = 0,
    Alphanumeric = 1,
    Byte = 2,
    Kanji = 3
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