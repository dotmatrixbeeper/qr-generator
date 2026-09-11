#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Numeric,
    Alphanumeric,
    Byte,
    Kanji
}

impl Mode {
    pub fn mode_code(&self) -> u8 {
        match self {
            Mode::Numeric => 0,
            Mode::Alphanumeric => 1,
            Mode::Byte => 2,
            Mode::Kanji => 3
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ECCLevel {
    L,
    M,
    Q,
    H
}

impl ECCLevel {
    pub fn error_index(&self) -> u8 {
        match self {
            ECCLevel::L => 0,
            ECCLevel::M => 1,
            ECCLevel::Q => 2,
            ECCLevel::H => 3
        }
    }
}