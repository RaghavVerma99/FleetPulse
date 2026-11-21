//! Persistence layer.
//!
//! All SQL lives here; handlers never build queries. Migrations are run from
//! this crate but the SQL files live in the workspace-level `migrations/`
//! directory (see ADR-012).

#![forbid(unsafe_code)]

mod auth;
mod db;
mod devices;
mod telemetry;

pub use auth::{device_id_for_api_key, generate_device_key, hash_device_key};
pub use db::{Db, DbError};
pub use devices::{
    get_device, latest_readings, list_devices, list_readings, DeviceRow, LatestReadingRow,
    ReadingRow, ReadingsQuery,
};
pub use telemetry::{ingest_batch, IngestOutcome, NewReading};
