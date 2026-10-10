//! Unit tests for the stage module, one file per layer.
//!
//! `support` holds byte builders that re-implement the DAT/WRLD layouts
//! independently of the readers, so the tests exercise the readers against the
//! documented format rather than against themselves.

mod support;

mod catalog;
mod directory;
mod header;
mod loader;
mod localization;
mod payload;
mod world;
