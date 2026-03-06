use std::{env, net::SocketAddr};

use anyhow::{anyhow, Context, Result};
use url::Url;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub app_name: String,
    pub api_bind_addr: SocketAddr,
    pub base_url: Url,
    pub cors_allowed_origins: Vec<String>,
    pub database_url: String,
    pub redis_url: String,
    pub s3_endpoint: String,
    pub s3_region: String,
    pub s3_access_key: String,
    pub s3_secret_key: String,
    pub raw_bucket: String,
    pub hls_bucket: String,
    pub transcode_stream: String,
    pub transcode_consumer_group: String,
    pub max_upload_bytes: u64,
    pub job_visibility_timeout_secs: u64,
    pub run_migrations: bool,
    pub worker_poll_interval_ms: u64,
    pub db_max_connections: u32,
}

impl AppConfig {
    pub fn from_env(app_name: impl Into<String>) -> Result<Self> {
        let api_bind_addr = read_env("API_BIND_ADDR")?
            .parse()
            .context("API_BIND_ADDR must be a valid socket address")?;
        let base_url =
            Url::parse(&read_env("BASE_URL")?).context("BASE_URL must be a valid URL")?;

        Ok(Self {
            app_name: app_name.into(),
            api_bind_addr,
            base_url,
            cors_allowed_origins: split_csv_env(
                "CORS_ALLOWED_ORIGINS",
                "http://localhost:5173,http://127.0.0.1:5173",
            ),
            database_url: read_env("DATABASE_URL")?,
            redis_url: read_env("REDIS_URL")?,
            s3_endpoint: read_env("MINIO_ENDPOINT")?,
            s3_region: read_env_with_default("AWS_REGION", "us-east-1"),
            s3_access_key: read_env("MINIO_ACCESS_KEY")?,
            s3_secret_key: read_env("MINIO_SECRET_KEY")?,
            raw_bucket: read_env_with_default("MINIO_RAW_BUCKET", "videos-raw"),
            hls_bucket: read_env_with_default("MINIO_HLS_BUCKET", "videos-hls"),
            transcode_stream: read_env_with_default("TRANSCODE_STREAM", "transcode_jobs"),
            transcode_consumer_group: read_env_with_default("TRANSCODE_CONSUMER_GROUP", "workers"),
            max_upload_bytes: read_parsed_env("MAX_UPLOAD_BYTES", 1_073_741_824)?,
            job_visibility_timeout_secs: read_parsed_env("JOB_VISIBILITY_TIMEOUT_SECS", 300)?,
            run_migrations: read_bool_env("RUN_MIGRATIONS", true)?,
            worker_poll_interval_ms: read_parsed_env("WORKER_POLL_INTERVAL_MS", 5_000)?,
            db_max_connections: read_parsed_env("DB_MAX_CONNECTIONS", 5)?,
        })
    }
}

fn read_env(key: &str) -> Result<String> {
    env::var(key).map_err(|_| anyhow!("missing required environment variable `{key}`"))
}

fn read_env_with_default(key: &str, default: &str) -> String {
    env::var(key).unwrap_or_else(|_| default.to_owned())
}

fn split_csv_env(key: &str, default: &str) -> Vec<String> {
    read_env_with_default(key, default)
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn read_parsed_env<T>(key: &str, default: T) -> Result<T>
where
    T: std::str::FromStr + ToString,
    T::Err: std::fmt::Display,
{
    let raw = env::var(key).unwrap_or_else(|_| default.to_string());
    raw.parse()
        .map_err(|err| anyhow!("invalid value for `{key}`: {err}"))
}

fn read_bool_env(key: &str, default: bool) -> Result<bool> {
    let raw = env::var(key).unwrap_or_else(|_| default.to_string());
    match raw.as_str() {
        "1" | "true" | "TRUE" | "yes" | "YES" => Ok(true),
        "0" | "false" | "FALSE" | "no" | "NO" => Ok(false),
        _ => Err(anyhow!("invalid value for `{key}`: expected boolean")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::BTreeMap,
        sync::{Mutex, OnceLock},
    };

    static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    const TEST_KEYS: [&str; 15] = [
        "API_BIND_ADDR",
        "BASE_URL",
        "CORS_ALLOWED_ORIGINS",
        "DATABASE_URL",
        "REDIS_URL",
        "MINIO_ENDPOINT",
        "MINIO_ACCESS_KEY",
        "MINIO_SECRET_KEY",
        "MINIO_RAW_BUCKET",
        "MINIO_HLS_BUCKET",
        "AWS_REGION",
        "MAX_UPLOAD_BYTES",
        "TRANSCODE_STREAM",
        "TRANSCODE_CONSUMER_GROUP",
        "JOB_VISIBILITY_TIMEOUT_SECS",
    ];
    const EXTRA_TEST_KEYS: [&str; 3] = [
        "RUN_MIGRATIONS",
        "WORKER_POLL_INTERVAL_MS",
        "DB_MAX_CONNECTIONS",
    ];

    #[test]
    fn parses_defaults_for_optional_settings() {
        let _guard = EnvGuard::new(&required_env());

        let config = AppConfig::from_env("api").expect("config should parse");

        assert_eq!(config.app_name, "api");
        assert_eq!(config.raw_bucket, "videos-raw");
        assert_eq!(config.hls_bucket, "videos-hls");
        assert_eq!(config.transcode_stream, "transcode_jobs");
        assert_eq!(config.transcode_consumer_group, "workers");
        assert_eq!(config.max_upload_bytes, 1_073_741_824);
        assert_eq!(config.job_visibility_timeout_secs, 300);
        assert!(config.run_migrations);
        assert_eq!(config.worker_poll_interval_ms, 5_000);
        assert_eq!(config.db_max_connections, 5);
        assert_eq!(
            config.cors_allowed_origins,
            vec![
                "http://localhost:5173".to_owned(),
                "http://127.0.0.1:5173".to_owned()
            ]
        );
    }

    #[test]
    fn parses_overrides_for_optional_settings() {
        let mut vars = required_env();
        vars.extend([
            (
                "CORS_ALLOWED_ORIGINS",
                "https://a.example,https://b.example",
            ),
            ("MINIO_RAW_BUCKET", "raw-test"),
            ("MINIO_HLS_BUCKET", "hls-test"),
            ("AWS_REGION", "ap-south-1"),
            ("MAX_UPLOAD_BYTES", "42"),
            ("TRANSCODE_STREAM", "jobs"),
            ("TRANSCODE_CONSUMER_GROUP", "group"),
            ("JOB_VISIBILITY_TIMEOUT_SECS", "90"),
            ("RUN_MIGRATIONS", "false"),
            ("WORKER_POLL_INTERVAL_MS", "250"),
            ("DB_MAX_CONNECTIONS", "11"),
        ]);
        let _guard = EnvGuard::new(&vars);

        let config = AppConfig::from_env("worker").expect("config should parse");

        assert_eq!(config.app_name, "worker");
        assert_eq!(
            config.cors_allowed_origins,
            vec!["https://a.example", "https://b.example"]
        );
        assert_eq!(config.raw_bucket, "raw-test");
        assert_eq!(config.hls_bucket, "hls-test");
        assert_eq!(config.s3_region, "ap-south-1");
        assert_eq!(config.max_upload_bytes, 42);
        assert_eq!(config.transcode_stream, "jobs");
        assert_eq!(config.transcode_consumer_group, "group");
        assert_eq!(config.job_visibility_timeout_secs, 90);
        assert!(!config.run_migrations);
        assert_eq!(config.worker_poll_interval_ms, 250);
        assert_eq!(config.db_max_connections, 11);
    }

    #[test]
    fn rejects_invalid_boolean_values() {
        let mut vars = required_env();
        vars.push(("RUN_MIGRATIONS", "sometimes"));
        let _guard = EnvGuard::new(&vars);

        let error = AppConfig::from_env("api").expect_err("invalid bool should fail");
        assert!(error.to_string().contains("RUN_MIGRATIONS"));
    }

    fn required_env() -> Vec<(&'static str, &'static str)> {
        vec![
            ("API_BIND_ADDR", "127.0.0.1:8080"),
            ("BASE_URL", "http://localhost:8080"),
            (
                "DATABASE_URL",
                "postgres://postgres:postgres@localhost/hermes",
            ),
            ("REDIS_URL", "redis://127.0.0.1:6379"),
            ("MINIO_ENDPOINT", "http://127.0.0.1:9000"),
            ("MINIO_ACCESS_KEY", "minioadmin"),
            ("MINIO_SECRET_KEY", "minioadmin"),
        ]
    }

    struct EnvGuard {
        saved: BTreeMap<String, Option<String>>,
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl EnvGuard {
        fn new(vars: &[(&str, &str)]) -> Self {
            let lock = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("env lock poisoned");

            let saved = TEST_KEYS
                .into_iter()
                .chain(EXTRA_TEST_KEYS)
                .map(|key| (key.to_owned(), env::var(key).ok()))
                .collect::<BTreeMap<_, _>>();

            for key in TEST_KEYS.into_iter().chain(EXTRA_TEST_KEYS) {
                unsafe { env::remove_var(key) };
            }

            for (key, value) in vars {
                unsafe { env::set_var(key, value) };
            }

            Self { saved, _lock: lock }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                match value {
                    Some(value) => unsafe { env::set_var(key, value) },
                    None => unsafe { env::remove_var(key) },
                }
            }
        }
    }
}
