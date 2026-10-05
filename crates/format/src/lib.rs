//! The Stemin content format: parse it, walk it, and check it.
//!
//! This crate is the **contract** a content repository is written against, so it
//! is deliberately small and carries **no renderer**. It builds for
//! `wasm32-unknown-unknown`, because the app parses the same format in the
//! browser that `stemin check` validates on the author's machine, and the two
//! must never disagree.
//!
//! `core` renders on top of this, and holds `stemin check`: the structural
//! rules in [`check`], then the compile the browser runs. The app reads this
//! crate directly.
//! CONTENT-MODEL.md is the specification.

pub mod block;
pub mod check;
pub mod error;
pub mod front;
pub mod id;
pub mod plot;
pub mod tree;

pub use block::{Block, BlockKind, Document};
pub use error::{At, Fault, FaultKind};
