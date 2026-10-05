//! Typed compile errors.
//!
//! A content fault names its file and its line, because an author reads these
//! in a terminal or in an import report and has to find the place.

use thiserror::Error;

/// Anything that can go wrong compiling a repository.
#[derive(Debug, Error)]
pub enum BuildError {
    /// The repository does not satisfy CONTENT-MODEL.md, or a body does not
    /// render. Every fault, each as `file:line: message`, in the order
    /// `stemin check` lists them.
    #[error("the content is not valid:\n{}", .0.join("\n"))]
    Content(Vec<String>),

    /// A file could not be read or written. The author's own build only.
    #[error("io error at {path}: {source}")]
    Io {
        /// The path involved.
        path: std::path::PathBuf,
        /// The underlying error.
        source: std::io::Error,
    },

    /// A YAML document failed to parse or to serialise.
    #[error("failed to parse {path}: {source}")]
    Yaml {
        /// The path involved.
        path: std::path::PathBuf,
        /// The underlying parse error.
        source: serde_yaml::Error,
    },
}

/// Convenience result alias.
pub type Result<T> = std::result::Result<T, BuildError>;
