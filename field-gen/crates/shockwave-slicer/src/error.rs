use std::fmt;

pub type SliceResult<T> = Result<T, SliceError>;

#[derive(Debug)]
pub enum SliceError {
    Cancelled,
    Io(std::io::Error),
    Message(String),
}

impl fmt::Display for SliceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => write!(formatter, "slicing was cancelled"),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Message(error) => write!(formatter, "{error}"),
        }
    }
}

impl std::error::Error for SliceError {}

impl From<String> for SliceError {
    fn from(error: String) -> Self {
        Self::Message(error)
    }
}

impl From<std::io::Error> for SliceError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}
