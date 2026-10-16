//! Rolling-window accounting with explicit clocks and fallible persistence.
//! GUI and system time are adapters; the domain never reads a clock itself.
pub mod chart_axes;
pub mod charts;
pub mod cli;
pub mod domain;
pub mod engine;
#[cfg(feature = "gui")]
pub mod gui;
pub mod preferences;
pub mod storage;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{0}")]
    Invalid(&'static str),
    #[error("The clock moved backwards; tracking will resume when it catches up")]
    ClockBackwards,
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Configuration: {0}")]
    Configuration(String),
    #[error("Legacy data: {0}")]
    Legacy(String),
    #[error("Another Rolling Timer is using this data directory")]
    AlreadyRunning,
    #[error("Desktop error: {0}")]
    Desktop(String),
}
pub type Result<T> = std::result::Result<T, Error>;

impl From<std::fs::TryLockError> for Error {
    fn from(error: std::fs::TryLockError) -> Self {
        match error {
            std::fs::TryLockError::WouldBlock => Self::AlreadyRunning,
            std::fs::TryLockError::Error(error) => Self::Io(error),
        }
    }
}

#[cfg(feature = "gui")]
impl From<fltk::prelude::FltkError> for Error {
    fn from(error: fltk::prelude::FltkError) -> Self {
        Self::Desktop(error.to_string())
    }
}
