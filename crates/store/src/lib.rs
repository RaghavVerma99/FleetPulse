//! Persistence layer.
//!
//! All SQL lives here; handlers never build queries. Migrations are run from
//! this crate but the SQL files live in the workspace-level `migrations/`
//! directory (see ADR-012).

#![forbid(unsafe_code)]

mod auth;
mod db;
mod telemetry;

pub use auth::device_id_for_api_key;
pub use db::{Db, DbError};
pub use telemetry::{ingest_batch, IngestOutcome, NewReading};
