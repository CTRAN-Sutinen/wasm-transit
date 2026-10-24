//! Errors for GTFS parsing. Kept in one place so callers can match on a
//! single `GtfsError` for anything that goes wrong while loading a feed.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum GtfsError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("zip error: {0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("CSV error in {file}: {source}")]
    Csv {
        file: &'static str,
        #[source]
        source: csv::Error,
    },

    #[error("missing required file: {0}")]
    MissingFile(&'static str),

    #[error("invalid data in {file}: {message}")]
    Invalid { file: &'static str, message: String },
}

pub type Result<T> = std::result::Result<T, GtfsError>;
