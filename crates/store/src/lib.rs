//! Persistence layer.
//!
//! All SQL lives here; handlers never build queries. Migrations are run from
//! this crate but the SQL files live in the workspace-level `migrations/`
//! directory (see ADR-012).

#![forbid(unsafe_code)]

mod db;

pub use db::{Db, DbError};
