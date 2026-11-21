use fleetpulse_store::Db;

/// Shared state handed to every handler.
#[derive(Debug, Clone)]
pub struct AppState {
    pub db: Db,
}
