use std::fmt;
use std::process::ExitStatus;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum Error {
    // A child tool failed; keep its exit code so the caller reports the real
    // failure instead of a generic one.
    Status(ExitStatus),
    Message(String),
}

impl Error {
    pub fn message(text: impl Into<String>) -> Self {
        Self::Message(text.into())
    }

    pub fn exit_code(&self) -> i32 {
        match self {
            Self::Status(status) => status.code().unwrap_or(1),
            Self::Message(_) => 1,
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Status(status) => write!(formatter, "{status}"),
            Self::Message(message) => formatter.write_str(message),
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Self::Message(error.to_string())
    }
}

impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Message(error.to_string())
    }
}
