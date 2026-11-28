use std::time::Duration;

use sqlx::postgres::{PgPool, PgPoolOptions};

const ACQUIRE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("database connection failed: {0}")]
    Connect(#[source] sqlx::Error),
    #[error("database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("database query failed: {0}")]
    Query(#[source] sqlx::Error),
    #[error("conflicting write")]
    Conflict,
    #[error("invalid data returned from database: {0}")]
    InvalidData(String),
    #[error("key hash failed: {0}")]
    Hash(String),
}

/// Handle to the Postgres connection pool.
#[derive(Debug, Clone)]
pub struct Db {
    pool: PgPool,
}

impl Db {
    /// Connect eagerly. Used by the binary so startup fails fast if Postgres is
    /// misconfigured.
    pub async fn connect(database_url: &str, max_connections: u32) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect(database_url)
            .await
            .map_err(DbError::Connect)?;
        Ok(Self { pool })
    }

    /// Build a pool without opening a connection. Used by tests that exercise
    /// routing without a live database.
    pub fn connect_lazy(database_url: &str, max_connections: u32) -> Result<Self, DbError> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .acquire_timeout(ACQUIRE_TIMEOUT)
            .connect_lazy(database_url)
            .map_err(DbError::Connect)?;
        Ok(Self { pool })
    }

    /// Apply every migration under `/migrations` in order.
    pub async fn migrate(&self) -> Result<(), DbError> {
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }

    /// Cheapest possible round trip, used by the readiness probe.
    pub async fn ping(&self) -> Result<(), DbError> {
        sqlx::query("select 1")
            .execute(&self.pool)
            .await
            .map_err(DbError::Query)?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
