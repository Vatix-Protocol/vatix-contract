//! Placeholder for code shared across the Vatix contract workspace.
//!
//! This crate currently ships no functionality — it is not a workspace
//! member (see the root `Cargo.toml`) and nothing in `contracts/*` depends
//! on it. It previously contained a `_todo()` function whose body was just
//! `todo!()`; that stub had no callers anywhere in the workspace, so calling
//! it would have been a guaranteed panic if it were ever wired up by
//! accident. Per audit hygiene (no `panic!`/`todo!()` on shipped library
//! paths), the stub has been removed rather than implemented, since there is
//! no actual behavior for it to implement.
