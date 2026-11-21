use anyhow::Context;
use fleetpulse_api::app;
use fleetpulse_api::config::Config;
use fleetpulse_api::state::AppState;
use fleetpulse_store::Db;
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env().context("invalid configuration")?;

    let db = Db::connect(&config.database_url, config.db_max_connections)
        .await
        .context("failed to connect to postgres")?;
    db.migrate().await.context("failed to apply migrations")?;

    let listener = TcpListener::bind(config.bind_addr)
        .await
        .with_context(|| format!("failed to bind {}", config.bind_addr))?;

    tracing::info!(addr = %config.bind_addr, "fleetpulse api listening");

    axum::serve(
        listener,
        app(AppState {
            db,
            operator_api_key: config.operator_api_key,
        }),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await
    .context("server failed")?;

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("fleetpulse_api=info,fleetpulse_store=info,tower_http=info")
    });

    tracing_subscriber::fmt()
        .json()
        .with_env_filter(filter)
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received; draining in-flight requests");
}
