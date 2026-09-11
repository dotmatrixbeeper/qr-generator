use std::fmt;

#[derive(Debug)]
pub enum QrError {
    InvalidVersion(usize),
    InputTooLong
}

impl fmt::Display for QrError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            QrError::InvalidVersion(v) => write!(f, "version {v} is not range 1 to 40"),
            QrError::InputTooLong => write!(f, "input is too long to be encoded in QR code")
        }
    }
}

impl std::error::Error for QrError {}