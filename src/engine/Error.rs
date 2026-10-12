use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum HnswError {
    InvalidConfig(&'static str),
    EmptyVector,
    NonFinite,
    InvalidNorm,
    DimensionMismatch { expected: usize, got: usize },
    NotFound(usize),
}

impl fmt::Display for HnswError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(msg) => write!(f, "invalid config: {msg}"),
            Self::EmptyVector => write!(f, "vector is empty"),
            Self::NonFinite => write!(f, "vector contains NaN or infinity"),
            Self::InvalidNorm => write!(f, "vector has zero or overflowing norm"),
            Self::DimensionMismatch { expected, got } => {
                write!(f, "dimension mismatch: expected {expected}, got {got}")
            }
            Self::NotFound(id) => write!(f, "vector {id} not found"),
        }
    }
}

impl std::error::Error for HnswError {}

pub type Result<T> = std::result::Result<T, HnswError>;
