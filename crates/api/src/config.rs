use std::net::SocketAddr;

use serde::Serialize;

const DEFAULT_BIND_ADDR: &str = "0.0.0.0:8080";
const DEFAULT_DB_MAX_CONNECTIONS: u32 = 10;

/// Runtime configuration, parsed once at startup so the rest of the process
/// never touches the environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Config {
    pub database_url: String,
    pub bind_addr: SocketAddr,
    pub operator_api_key: String,
    pub db_max_connections: u32,
}

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ConfigError {
    #[error("missing required environment variable: {0}")]
    Missing(&'static str),
    #[error("invalid {name}: {reason}")]
    Invalid { name: &'static str, reason: String },
}

impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_lookup(|key| std::env::var(key).ok())
    }

    /// Parse configuration from an arbitrary lookup function. Keeping this
    /// separate from [`Config::from_env`] makes the rules testable without
    /// mutating process-global state.
    pub fn from_lookup<F>(lookup: F) -> Result<Self, ConfigError>
    where
        F: Fn(&str) -> Option<String>,
    {
        let database_url = required(&lookup, "DATABASE_URL")?;
        let operator_api_key = required(&lookup, "OPERATOR_API_KEY")?;

        let bind_addr_raw = lookup("BIND_ADDR").unwrap_or_else(|| DEFAULT_BIND_ADDR.to_owned());
        let bind_addr =
            bind_addr_raw
                .parse::<SocketAddr>()
                .map_err(|error| ConfigError::Invalid {
                    name: "BIND_ADDR",
                    reason: error.to_string(),
                })?;

        let db_max_connections = match lookup("DB_MAX_CONNECTIONS") {
            Some(raw) => raw.parse::<u32>().map_err(|error| ConfigError::Invalid {
                name: "DB_MAX_CONNECTIONS",
                reason: error.to_string(),
            })?,
            None => DEFAULT_DB_MAX_CONNECTIONS,
        };
        if db_max_connections == 0 {
            return Err(ConfigError::Invalid {
                name: "DB_MAX_CONNECTIONS",
                reason: "must be greater than zero".to_owned(),
            });
        }

        Ok(Self {
            database_url,
            bind_addr,
            operator_api_key,
            db_max_connections,
        })
    }
}

fn required<F>(lookup: &F, name: &'static str) -> Result<String, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    lookup(name)
        .filter(|value| !value.trim().is_empty())
        .ok_or(ConfigError::Missing(name))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    fn config_from(pairs: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let values: HashMap<String, String> = pairs
            .iter()
            .map(|(key, value)| ((*key).to_owned(), (*value).to_owned()))
            .collect();
        Config::from_lookup(|key| values.get(key).cloned())
    }

    #[test]
    fn applies_defaults() {
        let config = config_from(&[
            ("DATABASE_URL", "postgres://localhost/fleetpulse"),
            ("OPERATOR_API_KEY", "secret"),
        ])
        .expect("valid config");

        assert_eq!(config.bind_addr.to_string(), "0.0.0.0:8080");
        assert_eq!(config.db_max_connections, 10);
    }

    #[test]
    fn rejects_missing_database_url() {
        let error = config_from(&[("OPERATOR_API_KEY", "secret")]).unwrap_err();
        assert_eq!(error, ConfigError::Missing("DATABASE_URL"));
    }

    #[test]
    fn rejects_blank_database_url() {
        let error =
            config_from(&[("DATABASE_URL", "   "), ("OPERATOR_API_KEY", "secret")]).unwrap_err();
        assert_eq!(error, ConfigError::Missing("DATABASE_URL"));
    }

    #[test]
    fn rejects_invalid_bind_addr() {
        let error = config_from(&[
            ("DATABASE_URL", "postgres://localhost/fleetpulse"),
            ("OPERATOR_API_KEY", "secret"),
            ("BIND_ADDR", "not-an-address"),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigError::Invalid {
                name: "BIND_ADDR",
                ..
            }
        ));
    }

    #[test]
    fn rejects_zero_pool_size() {
        let error = config_from(&[
            ("DATABASE_URL", "postgres://localhost/fleetpulse"),
            ("OPERATOR_API_KEY", "secret"),
            ("DB_MAX_CONNECTIONS", "0"),
        ])
        .unwrap_err();

        assert!(matches!(
            error,
            ConfigError::Invalid {
                name: "DB_MAX_CONNECTIONS",
                ..
            }
        ));
    }
}
