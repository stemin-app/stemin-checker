//! The Stemin content model, and the compiler that turns a course repository
//! into per-domain bundles.
//!
//! **The compiler runs in the browser.** A learner adds a repository by its
//! URL, the Stemin app fetches its files, and [`compile`] turns them into the
//! bundles the app reads, in a Web Worker. The `stemin check` command runs the
//! same code over a directory on disk, so a repository cannot mean one thing
//! to its author and another to a learner.
//!
//! There is **no document engine**. A live figure is a ```` ```plot ```` block
//! the app draws; a still one is an author's SVG, re-emitted through the
//! allowlist in [`svg`], never filtered. The renderer's MathML passes through
//! the allowlist in [`mathml`] the same way. See `CONTENT-MODEL.md`.

pub mod b64;
pub mod model;

#[cfg(feature = "compile")]
pub mod asset;
#[cfg(feature = "compile")]
pub mod compile;
#[cfg(feature = "compile")]
pub mod error;
#[cfg(feature = "compile")]
pub mod mathml;
#[cfg(feature = "compile")]
pub mod render;
#[cfg(feature = "compile")]
pub mod svg;

#[cfg(feature = "compile")]
pub use compile::Source;
#[cfg(feature = "compile")]
pub use error::{BuildError, Result};
