//! Pure, GTK-free hierarchical projection of the picker's flat file list.
//!
//! The projection (and its unit tests) now lives in `wolf-core`'s
//! [`wolf_core::tree`] module — a single, FFI-expressible source of truth shared
//! by every front-end. This module re-exports it so the GTK glue in
//! `tree_view.rs` (and `lib.rs`'s re-exports) keeps compiling against the
//! `crate::tree::{TreeNode, children_of, ancestors_to_reveal, node_rel_path,
//! rel_path_for}` paths unchanged.

pub use wolf_core::tree::*;
