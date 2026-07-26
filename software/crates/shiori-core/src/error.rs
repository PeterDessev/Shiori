//! Crate-wide error type.

/// Convenience alias used across the workspace.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Top-level error type for Shiori.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}
