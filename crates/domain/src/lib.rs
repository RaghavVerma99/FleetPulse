//! Core domain types for FleetPulse.
//!
//! This crate is dependency-light on purpose: it defines the vocabulary
//! (devices, readings, batches, API keys) that every other crate speaks, and
//! stays free of HTTP and database concerns.

#![forbid(unsafe_code)]

mod api_key;
mod batch;
mod device;
mod reading;

pub use api_key::ApiKey;
pub use batch::TelemetryBatch;
pub use device::{Device, DeviceStatus, UnknownDeviceStatus};
pub use reading::Reading;
